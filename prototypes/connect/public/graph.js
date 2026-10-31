'use strict';

/*
 * Kleiner 3D-Graph-Renderer ohne Bibliotheken: Kräfte-Layout in 3D, perspektivische Projektion
 * auf ein 2D-Canvas, Drehen/Zoomen per Maus und Touch, Medien-Pakete auf den Kanten,
 * Stufen (Idee → Plattform → Ökosystem) und abspielbare Szenarien.
 */

(() => {
  const DATA = window.CONNECT_DATA;
  const canvas = document.getElementById('scene');
  const ctx = canvas.getContext('2d');
  const $ = (id) => document.getElementById(id);
  const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;

  const RING = 120; // Abstand pro Ring im Layout
  const FOCAL = 700;
  const EMOJI_FONT = '"Apple Color Emoji","Segoe UI Emoji","Noto Color Emoji",sans-serif';

  // ---------- Graph aufbauen ----------

  const nodes = DATA.nodes.map((n) => {
    const group = DATA.groups[n.group];
    return {
      ...n,
      color: group.color,
      ring: group.ring,
      icon: DATA.icons[n.id] || '•',
      level: DATA.levels.findIndex((l) => l.groups.includes(n.group)),
      size: n.group === 'kern' ? 22 : n.group === 'akteur' ? 15 : 10,
      x: 0, y: 0, z: 0, vx: 0, vy: 0, vz: 0,
      appear: 0, // 0 = unsichtbar, 1 = voll da (für das Herauswachsen)
      sx: 0, sy: 0, scale: 0, depth: 0,
    };
  });
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const edges = DATA.edges.map(([a, b, rel]) => ({ a: byId.get(a), b: byId.get(b), rel }));
  for (const n of nodes) n.links = [];
  for (const e of edges) {
    e.a.links.push({ node: e.b, rel: e.rel, edge: e });
    e.b.links.push({ node: e.a, rel: e.rel, edge: e });
  }

  // Startpositionen: gleichmäßig auf Kugelschalen (Fibonacci-Kugel) je Ring
  const byRing = new Map();
  for (const n of nodes) {
    if (!byRing.has(n.ring)) byRing.set(n.ring, []);
    byRing.get(n.ring).push(n);
  }
  for (const [ring, list] of byRing) {
    list.forEach((n, i) => {
      const k = list.length;
      const y = k === 1 ? 0 : 1 - (2 * (i + 0.5)) / k;
      const r = Math.sqrt(1 - y * y);
      const phi = i * Math.PI * (3 - Math.sqrt(5)) + ring;
      n.home = { x: Math.cos(phi) * r, y: y * 0.8, z: Math.sin(phi) * r };
      n.x = n.home.x * ring * RING;
      n.y = n.home.y * ring * RING;
      n.z = n.home.z * ring * RING;
    });
  }

  // ---------- Zustand ----------

  const cam = { yaw: 0.6, pitch: -0.25, dist: 620, tx: 0, ty: 0, tz: 0, goal: { x: 0, y: 0, z: 0 } };
  let distGoal = 620;
  let level = 0;
  let selected = null;
  let hovered = null;
  let scenario = null; // { def, step, timer, virtualEdges }
  let lastInteraction = 0;
  let soundOn = false;
  let audio = null;
  let viewShift = 0; // schiebt den Graphen nach oben, solange der Steckbrief unten offen ist
  const packets = [];
  const stars = Array.from({ length: 420 }, () => {
    const u = Math.random() * 2 - 1;
    const t = Math.random() * Math.PI * 2;
    const r = 1600 + Math.random() * 1400;
    const s = Math.sqrt(1 - u * u);
    return { x: Math.cos(t) * s * r, y: u * r, z: Math.sin(t) * s * r, b: Math.random() };
  });

  const visible = (n) => n.level <= level;

  // ---------- Physik ----------

  function simulate() {
    const vis = nodes.filter((n) => n.appear > 0.01);
    for (let i = 0; i < vis.length; i++) {
      const a = vis[i];
      for (let j = i + 1; j < vis.length; j++) {
        const b = vis[j];
        let dx = a.x - b.x, dy = a.y - b.y, dz = a.z - b.z;
        let d2 = dx * dx + dy * dy + dz * dz;
        if (d2 < 1) { dx = Math.random(); dy = Math.random(); dz = Math.random(); d2 = 1; }
        const f = Math.min(4, 2400 / d2) * a.appear * b.appear;
        const d = Math.sqrt(d2);
        dx /= d; dy /= d; dz /= d;
        a.vx += dx * f; a.vy += dy * f; a.vz += dz * f;
        b.vx -= dx * f; b.vy -= dy * f; b.vz -= dz * f;
      }
    }
    for (const e of edges) {
      if (e.a.appear < 0.01 || e.b.appear < 0.01) continue;
      const dx = e.b.x - e.a.x, dy = e.b.y - e.a.y, dz = e.b.z - e.a.z;
      const d = Math.sqrt(dx * dx + dy * dy + dz * dz) || 1;
      const f = (d - RING) * 0.002;
      e.a.vx += (dx / d) * f; e.a.vy += (dy / d) * f; e.a.vz += (dz / d) * f;
      e.b.vx -= (dx / d) * f; e.b.vy -= (dy / d) * f; e.b.vz -= (dz / d) * f;
    }
    for (const n of nodes) {
      // Ziel-Radius je Ring; der Kern bleibt in der Mitte
      const target = n.ring * RING;
      if (n.ring === 0) {
        n.vx -= n.x * 0.05; n.vy -= n.y * 0.05; n.vz -= n.z * 0.05;
      } else {
        const r = Math.sqrt(n.x * n.x + n.y * n.y + n.z * n.z) || 1;
        const f = (target - r) * 0.02;
        n.vx += (n.x / r) * f; n.vy += (n.y / r) * f; n.vz += (n.z / r) * f;
        // leichter Zug zur Startrichtung hält das Bild ruhig und wiedererkennbar
        n.vx += (n.home.x * target - n.x) * 0.002;
        n.vy += (n.home.y * target - n.y) * 0.002;
        n.vz += (n.home.z * target - n.z) * 0.002;
      }
      n.vx *= 0.82; n.vy *= 0.82; n.vz *= 0.82;
      n.x += n.vx; n.y += n.vy; n.z += n.vz;
      // Herauswachsen und Verschwinden
      const goal = visible(n) ? 1 : 0;
      n.appear += (goal - n.appear) * 0.06;
      // Unsichtbare Knoten warten nahe der Mitte und wachsen beim Einblenden nach außen
      if (!goal && n.appear < 0.05) {
        n.x = n.home.x * RING * 0.4;
        n.y = n.home.y * RING * 0.4;
        n.z = n.home.z * RING * 0.4;
      }
    }
  }

  // ---------- Projektion ----------

  let W = 0, H = 0, DPR = 1;
  function resize() {
    DPR = Math.min(window.devicePixelRatio || 1, 2);
    W = window.innerWidth;
    H = window.innerHeight;
    canvas.width = W * DPR;
    canvas.height = H * DPR;
    ctx.setTransform(DPR, 0, 0, DPR, 0, 0);
  }

  function centerX() {
    // Auf breiten Bildschirmen etwas nach rechts, damit der Einleitungstext frei bleibt
    return W > 820 ? W * 0.53 : W / 2;
  }

  function project(p, out, withTarget = true) {
    const x = p.x - (withTarget ? cam.tx : 0);
    const y = p.y - (withTarget ? cam.ty : 0);
    const z = p.z - (withTarget ? cam.tz : 0);
    const cy = Math.cos(cam.yaw), sy = Math.sin(cam.yaw);
    const cp = Math.cos(cam.pitch), sp = Math.sin(cam.pitch);
    const x1 = x * cy - z * sy;
    const z1 = x * sy + z * cy;
    const y2 = y * cp - z1 * sp;
    const z2 = y * sp + z1 * cp;
    const depth = z2 + cam.dist;
    const scale = depth > 10 ? FOCAL / depth : 0;
    out.sx = centerX() + x1 * scale;
    out.sy = (W > 820 ? H / 2 + 20 : H * 0.6) + viewShift * H + y2 * scale;
    out.scale = scale;
    out.depth = depth;
    return out;
  }

  // ---------- Pakete ----------

  function spawnPacket(edge, forward, opts = {}) {
    packets.push({
      edge,
      forward,
      t: 0,
      speed: opts.speed || 0.006 + Math.random() * 0.006,
      glyph: opts.glyph || DATA.media[Math.floor(Math.random() * DATA.media.length)],
      big: !!opts.big,
      color: opts.color || (forward ? edge.b.color : edge.a.color),
      onArrive: opts.onArrive,
    });
  }

  function ambientPackets() {
    if (packets.length > (W < 820 ? 10 : 22) || Math.random() > 0.12) return;
    const candidates = edges.filter((e) => e.a.appear > 0.9 && e.b.appear > 0.9);
    if (!candidates.length) return;
    let e = candidates[Math.floor(Math.random() * candidates.length)];
    // Bei einer Auswahl fließen die Pakete rund um den ausgewählten Knoten
    if (selected && Math.random() < 0.7) {
      const ls = selected.links.filter((l) => l.node.appear > 0.9);
      if (ls.length) e = ls[Math.floor(Math.random() * ls.length)].edge;
    }
    spawnPacket(e, Math.random() < 0.5);
  }

  // ---------- Zeichnen ----------

  const tmp = { sx: 0, sy: 0, scale: 0, depth: 0 };

  function isHighlighted(n) {
    if (scenario) return scenario.path.slice(0, scenario.step + 1).includes(n);
    if (!selected) return true;
    return n === selected || selected.links.some((l) => l.node === n);
  }

  function edgeHighlighted(e) {
    if (scenario) return false;
    if (!selected) return true;
    return e.a === selected || e.b === selected;
  }

  function hexA(hex, a) {
    const v = parseInt(hex.slice(1), 16);
    return `rgba(${(v >> 16) & 255},${(v >> 8) & 255},${v & 255},${a.toFixed(3)})`;
  }

  function draw(time) {
    ctx.clearRect(0, 0, W, H);

    // Hintergrund: leichter Schein in der Mitte und Sterne (nur Rotation, keine Verschiebung)
    const g = ctx.createRadialGradient(centerX(), H / 2, 0, centerX(), H / 2, Math.max(W, H) * 0.6);
    g.addColorStop(0, 'rgba(40,70,120,0.28)');
    g.addColorStop(1, 'rgba(5,7,13,0)');
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, W, H);
    for (const s of stars) {
      project(s, tmp, false);
      if (tmp.scale <= 0) continue;
      const tw = 0.4 + 0.6 * Math.abs(Math.sin(time * 0.0007 + s.b * 20));
      ctx.fillStyle = `rgba(200,215,255,${(0.15 + s.b * 0.45) * tw})`;
      ctx.fillRect(tmp.sx, tmp.sy, 1.2, 1.2);
    }

    for (const n of nodes) project(n, n);

    // Kanten
    ctx.lineCap = 'round';
    for (const e of edges) {
      const vis = Math.min(e.a.appear, e.b.appear);
      if (vis < 0.02 || e.a.scale <= 0 || e.b.scale <= 0) continue;
      const hi = edgeHighlighted(e);
      const fog = Math.min(1, ((e.a.scale + e.b.scale) / 2) * 0.9);
      const alpha = vis * fog * (hi ? (selected ? 0.75 : 0.28) : 0.05);
      const grad = ctx.createLinearGradient(e.a.sx, e.a.sy, e.b.sx, e.b.sy);
      grad.addColorStop(0, hexA(e.a.color, alpha));
      grad.addColorStop(1, hexA(e.b.color, alpha));
      ctx.strokeStyle = grad;
      ctx.lineWidth = (selected && hi ? 1.8 : 1) * Math.max(0.6, fog);
      ctx.beginPath();
      ctx.moveTo(e.a.sx, e.a.sy);
      ctx.lineTo(e.b.sx, e.b.sy);
      ctx.stroke();
      if (selected && hi && fog > 0.5) {
        ctx.font = `12px ${getComputedStyle(document.body).fontFamily}`;
        ctx.fillStyle = `rgba(200,210,230,${0.7 * vis})`;
        ctx.textAlign = 'center';
        ctx.fillText(e.rel, (e.a.sx + e.b.sx) / 2, (e.a.sy + e.b.sy) / 2 - 4);
      }
    }

    // Szenario-Pfad als leuchtende Bögen (auch wenn es keine direkte Kante gibt)
    if (scenario) {
      for (let i = 1; i <= scenario.step; i++) {
        const a = scenario.path[i - 1], b = scenario.path[i];
        if (a === b) continue;
        const mx = (a.sx + b.sx) / 2, my = (a.sy + b.sy) / 2 - 30;
        ctx.strokeStyle = 'rgba(90,209,255,0.85)';
        ctx.lineWidth = 2.5;
        ctx.shadowColor = '#5ad1ff';
        ctx.shadowBlur = 12;
        ctx.beginPath();
        ctx.moveTo(a.sx, a.sy);
        ctx.quadraticCurveTo(mx, my, b.sx, b.sy);
        ctx.stroke();
        ctx.shadowBlur = 0;
      }
    }

    // Pakete
    for (let i = packets.length - 1; i >= 0; i--) {
      const p = packets[i];
      p.t += p.speed * (reducedMotion ? 0.5 : 1);
      const from = p.forward ? p.edge.a : p.edge.b;
      const to = p.forward ? p.edge.b : p.edge.a;
      if (p.t >= 1) {
        packets.splice(i, 1);
        if (p.onArrive) p.onArrive();
        continue;
      }
      if (from.scale <= 0 || to.scale <= 0) continue;
      const t = p.t;
      const arc = p.edge.virtual ? 30 * Math.sin(Math.PI * t) : 0;
      const x = from.sx + (to.sx - from.sx) * t;
      const y = from.sy + (to.sy - from.sy) * t - arc;
      const sc = (from.scale + (to.scale - from.scale) * t);
      const size = (p.big ? 26 : 12) * Math.max(0.6, sc);
      const fade = Math.sin(Math.PI * t);
      ctx.globalAlpha = (p.big ? 1 : 0.85) * fade * (scenario && !p.big ? 0.25 : 1);
      const glow = ctx.createRadialGradient(x, y, 0, x, y, size);
      glow.addColorStop(0, hexA(p.color, 0.55));
      glow.addColorStop(1, hexA(p.color, 0));
      ctx.fillStyle = glow;
      ctx.beginPath();
      ctx.arc(x, y, size, 0, Math.PI * 2);
      ctx.fill();
      ctx.font = `${Math.round(size * 0.8)}px ${EMOJI_FONT}`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(p.glyph, x, y + 1);
      ctx.globalAlpha = 1;
    }

    // Knoten von hinten nach vorne
    const order = nodes.filter((n) => n.appear > 0.02 && n.scale > 0).sort((a, b) => b.depth - a.depth);
    const font = getComputedStyle(document.body).fontFamily;
    for (const n of order) {
      const hi = isHighlighted(n);
      const focus = n === selected || n === hovered || (scenario && scenario.path[scenario.step] === n);
      const pulse = n.group === 'kern' ? 1 + 0.08 * Math.sin(time * 0.003) : 1;
      const r = n.size * n.scale * n.appear * pulse * (focus ? 1.25 : 1);
      const dim = hi ? 1 : 0.15;
      const fog = Math.min(1, n.scale * 1.1);

      const halo = ctx.createRadialGradient(n.sx, n.sy, 0, n.sx, n.sy, r * 3.2);
      halo.addColorStop(0, hexA(n.color, 0.45 * dim * fog));
      halo.addColorStop(1, hexA(n.color, 0));
      ctx.fillStyle = halo;
      ctx.beginPath();
      ctx.arc(n.sx, n.sy, r * 3.2, 0, Math.PI * 2);
      ctx.fill();

      ctx.fillStyle = hexA('#0a0f1c', 0.9 * dim + 0.1);
      ctx.strokeStyle = hexA(n.color, (focus ? 1 : 0.8) * dim * fog + 0.05);
      ctx.lineWidth = focus ? 2.5 : 1.5;
      ctx.beginPath();
      ctx.arc(n.sx, n.sy, r, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();

      ctx.globalAlpha = dim * n.appear;
      ctx.font = `${Math.round(r * 1.1)}px ${EMOJI_FONT}`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      ctx.fillText(n.icon, n.sx, n.sy + r * 0.06);

      const showLabel = focus || (hi && (n.level === 0 || selected || scenario || n.scale > 1.05));
      if (showLabel) {
        const fs = Math.max(11, Math.min(18, (n.group === 'kern' ? 17 : 13) * n.scale));
        ctx.font = `${focus || n.level === 0 ? 600 : 400} ${fs}px ${font}`;
        ctx.textBaseline = 'top';
        ctx.fillStyle = `rgba(232,236,245,${Math.min(1, fog + 0.2)})`;
        ctx.shadowColor = 'rgba(0,0,0,0.9)';
        ctx.shadowBlur = 6;
        ctx.fillText(n.label, n.sx, n.sy + r + 6);
        ctx.shadowBlur = 0;
      }
      ctx.globalAlpha = 1;
    }
  }

  // ---------- Schleife ----------

  function tick(time) {
    simulate();
    if (!reducedMotion && !dragging && time - lastInteraction > 2500) cam.yaw += scenario ? 0.0008 : 0.0018;
    // Kamera weich zum Ziel führen
    cam.tx += (cam.goal.x - cam.tx) * 0.06;
    cam.ty += (cam.goal.y - cam.ty) * 0.06;
    cam.tz += (cam.goal.z - cam.tz) * 0.06;
    cam.dist += (distGoal - cam.dist) * 0.08;
    viewShift += ((scenario && !$('story-details').hidden ? -0.22 : 0) - viewShift) * 0.06;
    if (selected) cam.goal = { x: selected.x, y: selected.y, z: selected.z };
    if (scenario) {
      const n = scenario.path[scenario.step];
      cam.goal = { x: n.x * 0.5, y: n.y * 0.5, z: n.z * 0.5 };
    }
    ambientPackets();
    draw(time);
    requestAnimationFrame(tick);
  }

  // ---------- Eingabe ----------

  const pointers = new Map();
  let dragging = false;
  let moved = 0;
  let pinchStart = 0;
  let distAtPinch = 0;

  function pick(x, y) {
    let best = null;
    let bestDepth = Infinity;
    for (const n of nodes) {
      if (n.appear < 0.5 || n.scale <= 0) continue;
      const r = n.size * n.scale + 8;
      const dx = x - n.sx, dy = y - n.sy;
      if (dx * dx + dy * dy <= r * r && n.depth < bestDepth) {
        best = n;
        bestDepth = n.depth;
      }
    }
    return best;
  }

  canvas.addEventListener('pointerdown', (ev) => {
    canvas.setPointerCapture(ev.pointerId);
    pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    dragging = true;
    moved = 0;
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      pinchStart = Math.hypot(a.x - b.x, a.y - b.y);
      distAtPinch = distGoal;
    }
    canvas.classList.add('dragging');
  });

  canvas.addEventListener('pointermove', (ev) => {
    const prev = pointers.get(ev.pointerId);
    if (!prev) {
      hovered = pick(ev.clientX, ev.clientY);
      canvas.classList.toggle('hovering', !!hovered);
      return;
    }
    const dx = ev.clientX - prev.x, dy = ev.clientY - prev.y;
    pointers.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
    lastInteraction = performance.now();
    if (pointers.size === 2) {
      const [a, b] = [...pointers.values()];
      const d = Math.hypot(a.x - b.x, a.y - b.y);
      if (pinchStart) distGoal = clampDist(distAtPinch * (pinchStart / d));
      moved += 10;
      return;
    }
    moved += Math.abs(dx) + Math.abs(dy);
    cam.yaw += dx * 0.006;
    cam.pitch = Math.max(-1.3, Math.min(1.3, cam.pitch + dy * 0.006));
  });

  function endPointer(ev) {
    if (!pointers.has(ev.pointerId)) return;
    pointers.delete(ev.pointerId);
    if (pointers.size === 0) {
      dragging = false;
      canvas.classList.remove('dragging');
      if (moved < 6 && ev.type === 'pointerup') {
        const n = pick(ev.clientX, ev.clientY);
        if (n) select(n);
        else if (selected) select(null);
      }
    }
  }
  canvas.addEventListener('pointerup', endPointer);
  canvas.addEventListener('pointercancel', endPointer);
  canvas.addEventListener('pointerleave', () => { hovered = null; });

  const clampDist = (d) => Math.max(220, Math.min(1800, d));
  canvas.addEventListener('wheel', (ev) => {
    ev.preventDefault();
    lastInteraction = performance.now();
    distGoal = clampDist(distGoal * Math.exp(ev.deltaY * 0.001));
  }, { passive: false });

  window.addEventListener('keydown', (ev) => {
    if (ev.key === 'Escape') {
      if (scenario) stopScenario();
      else select(null);
    }
    if (scenario && ev.key === 'ArrowRight') gotoStep(scenario.step + 1, true);
    if (scenario && ev.key === 'ArrowLeft') gotoStep(scenario.step - 1, true);
  });

  // ---------- Ton (optional, standardmäßig aus) ----------

  const SCALE = [261.63, 293.66, 329.63, 392.0, 440.0, 523.25, 587.33];
  function chime(i) {
    if (!soundOn) return;
    audio = audio || new AudioContext();
    const now = audio.currentTime;
    const osc = audio.createOscillator();
    const gain = audio.createGain();
    osc.type = 'sine';
    osc.frequency.value = SCALE[i % SCALE.length];
    gain.gain.setValueAtTime(0, now);
    gain.gain.linearRampToValueAtTime(0.15, now + 0.02);
    gain.gain.exponentialRampToValueAtTime(0.0001, now + 1.4);
    osc.connect(gain).connect(audio.destination);
    osc.start(now);
    osc.stop(now + 1.5);
  }
  $('sound').addEventListener('click', () => {
    soundOn = !soundOn;
    $('sound').textContent = soundOn ? '🔊 Ton an' : '🔇 Ton aus';
    $('sound').setAttribute('aria-pressed', String(soundOn));
    if (soundOn) chime(4);
  });

  // ---------- Stufen ----------

  let autoUnfold = [];
  function setLevel(i, fromUser) {
    level = i;
    if (fromUser) autoUnfold.forEach(clearTimeout);
    $('level-text').textContent = DATA.levels[i].text;
    [...$('level-buttons').children].forEach((b, j) => {
      b.classList.toggle('active', j === i);
      b.setAttribute('aria-pressed', String(j === i));
    });
    distGoal = [380, 540, 680][i] * (W > 820 ? 1 : 1.7);
    if (selected && !visible(selected)) select(null);
  }
  DATA.levels.forEach((l, i) => {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = l.label;
    b.addEventListener('click', () => setLevel(i, true));
    $('level-buttons').append(b);
  });

  // ---------- Auswahl & Panel ----------

  function select(n) {
    selected = n;
    if (n && !visible(n)) setLevel(n.level, true);
    $('panel-empty').hidden = !!n;
    $('panel-node').hidden = !n;
    if (!n) {
      cam.goal = { x: 0, y: 0, z: 0 };
      return;
    }
    $('node-group').textContent = `${n.icon}  ${DATA.groups[n.group].label}`;
    $('node-group').style.color = n.color;
    $('node-title').textContent = n.label;
    $('node-desc').textContent = n.desc;
    const list = $('node-links');
    list.replaceChildren();
    for (const l of n.links) {
      const li = document.createElement('li');
      const b = document.createElement('button');
      b.type = 'button';
      const sw = document.createElement('span');
      sw.className = 'swatch';
      sw.style.color = l.node.color;
      const name = document.createElement('span');
      name.textContent = `${l.node.icon} ${l.node.label}`;
      const rel = document.createElement('span');
      rel.className = 'rel';
      rel.textContent = l.rel;
      b.append(sw, name, rel);
      b.addEventListener('click', () => select(l.node));
      li.append(b);
      list.append(li);
    }
    // ein paar Pakete zum Gegenüber schicken
    n.links.slice(0, 6).forEach((l, i) => setTimeout(() => {
      if (selected === n) spawnPacket(l.edge, l.edge.a === n, { speed: 0.012 });
    }, i * 120));
  }
  $('node-close').addEventListener('click', () => select(null));

  // Legende
  for (const [key, g] of Object.entries(DATA.groups)) {
    const li = document.createElement('li');
    const sw = document.createElement('span');
    sw.className = 'swatch';
    sw.style.color = g.color;
    const t = document.createElement('span');
    t.textContent = g.label;
    li.append(sw, t);
    li.dataset.group = key;
    $('legend').append(li);
  }

  // Liste aller Knoten (Tastatur, Screenreader)
  for (const [key, g] of Object.entries(DATA.groups)) {
    const h = document.createElement('h4');
    h.textContent = g.label;
    $('node-index').append(h);
    for (const n of nodes.filter((x) => x.group === key)) {
      const b = document.createElement('button');
      b.type = 'button';
      b.textContent = `${n.icon} ${n.label}`;
      b.addEventListener('click', () => { stopScenario(); select(n); });
      $('node-index').append(b);
    }
  }

  // ---------- Szenarien ----------

  function edgeBetween(a, b) {
    return edges.find((e) => (e.a === a && e.b === b) || (e.a === b && e.b === a));
  }

  function startScenario(def) {
    stopScenario();
    select(null);
    setLevel(DATA.levels.length - 1, true);
    scenario = { def, path: def.steps.map(([id]) => byId.get(id)), step: -1, timer: 0 };
    [...$('scenario-buttons').children].forEach((b) => b.classList.toggle('active', b.dataset.id === def.id));
    $('story').hidden = false;
    $('story-title').textContent = def.label;
    distGoal = W > 820 ? 520 : 820;
    gotoStep(0);
  }

  function gotoStep(i, fromUser) {
    if (!scenario) return;
    const steps = scenario.def.steps;
    if (i < 0 || i >= steps.length) return;
    clearTimeout(scenario.timer);
    const prev = scenario.path[scenario.step];
    scenario.step = i;
    const n = scenario.path[i];
    $('story-text').textContent = `${n.icon}  ${steps[i][1]}`;
    renderDetails(steps[i][2]);
    $('story-progress').textContent = `Schritt ${i + 1} von ${steps.length}`;
    $('story-prev').disabled = i === 0;
    $('story-next').disabled = i === steps.length - 1;
    chime(i);
    if (prev && prev !== n) {
      const e = edgeBetween(prev, n);
      const edge = e || { a: prev, b: n, virtual: true };
      const forward = e ? e.a === prev : true;
      spawnPacket(edge, forward, { big: true, speed: 0.018, color: '#5ad1ff', glyph: DATA.media[i % DATA.media.length] });
    }
    // automatisch weiter, solange niemand selbst blättert
    if (!fromUser && i < steps.length - 1) {
      const ms = scenario.def.stepMs || 3600;
      scenario.timer = setTimeout(() => gotoStep(i + 1), reducedMotion ? ms * 1.4 : ms);
    }
  }

  // Steckbrief eines Schritts: Technik, Server, Owner, Datensets und die Anzeige als Plugin-Vorschau
  function renderDetails(d) {
    $('story-details').hidden = !d;
    $('story').classList.toggle('wide', !!d);
    if (!d) return;
    $('fact-stack').textContent = d.stack;
    $('fact-server').textContent = d.server;
    $('fact-owner').textContent = d.owner;
    $('fact-data').replaceChildren(...d.data.map((t) => {
      const li = document.createElement('li');
      li.textContent = t;
      if (t.startsWith('+')) li.className = 'extra';
      return li;
    }));
    $('plugin-name').textContent = `${d.plugin.icon} ${d.plugin.name}`;
    $('plugin-body').replaceChildren(...d.plugin.lines.map((t) => {
      const p = document.createElement('p');
      p.textContent = t;
      return p;
    }));
    // Animation neu starten
    const card = $('plugin');
    card.classList.remove('pop');
    void card.offsetWidth;
    card.classList.add('pop');
  }

  function stopScenario() {
    if (!scenario) return;
    clearTimeout(scenario.timer);
    scenario = null;
    $('story').hidden = true;
    [...$('scenario-buttons').children].forEach((b) => b.classList.remove('active'));
    cam.goal = { x: 0, y: 0, z: 0 };
  }

  for (const s of DATA.scenarios) {
    const b = document.createElement('button');
    b.type = 'button';
    b.textContent = s.label;
    b.dataset.id = s.id;
    if (s.featured) b.classList.add('featured');
    b.addEventListener('click', () => (scenario && scenario.def === s ? stopScenario() : startScenario(s)));
    $('scenario-buttons').append(b);
  }
  $('story-prev').addEventListener('click', () => gotoStep(scenario.step - 1, true));
  $('story-next').addEventListener('click', () => gotoStep(scenario.step + 1, true));
  $('story-stop').addEventListener('click', stopScenario);

  // ---------- Start ----------

  window.addEventListener('resize', resize);
  resize();
  cam.dist = 900;
  setLevel(0);
  // Vom einfachen Gedanken zum vollständigen Graphen: die Stufen entfalten sich von selbst
  autoUnfold = [setTimeout(() => setLevel(1), 4500), setTimeout(() => setLevel(2), 9000)];
  requestAnimationFrame(tick);
})();
