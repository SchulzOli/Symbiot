'use strict';

/*
 * „Ein Tag mit meinem Symbiot“ – Kurzfilm als Canvas-Animation.
 *
 * Links der Mensch, in der Mitte sein Symbiot (die KI, sein Assistent), rechts der Turm der
 * Abstraktionsebenen L0–L5 aus dem Connect-Graphen. Fünf Alltagsszenen vom Aufwachen bis zur Nacht.
 *
 * Alles hängt nur von der Zeit t ab (kein Zufall, kein Zustand): So lässt sich der Film live
 * abspielen und genauso Bild für Bild als Video rendern (?render=1, window.renderFrame(t)).
 */

(() => {
  const W = 1280, H = 720;
  const canvas = document.getElementById('film');
  const ctx = canvas.getContext('2d');
  const params = new URLSearchParams(location.search);
  const RENDER = params.has('render');
  const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, sans-serif';

  // ---------- Inhalt ----------

  const LAYERS = [
    { id: 'L0', name: 'Körper & Sinne', items: ['Puls', 'Schlaf', 'Stimme', 'Blick'], color: '#ff8a5c' },
    { id: 'L1', name: 'Geräte', items: ['Uhr', 'Brille', 'Handy', 'Küche', 'Bahn'], color: '#ffb547' },
    { id: 'L2', name: 'Daten & Signale', items: ['Sensoren', 'APIs', 'Dokumente', 'Web'], color: '#9b8cff' },
    { id: 'L3', name: 'Bausteine', items: ['Absicht', 'Vermittlung', 'Übersetzung', 'Einwilligung'], color: '#5ad1ff' },
    { id: 'L4', name: 'Gedächtnis', items: ['Wissensgraph', 'Signierte Kette', 'Belege'], color: '#5cf2a6' },
    { id: 'L5', name: 'Menschen & Welt', items: ['Lea', 'Mira', 'Stadt', 'Offene Daten'], color: '#ff6fa8' },
  ];

  const SKY = {
    night: ['#03040a', '#0b1430'],
    dawn: ['#160c30', '#c2566a'],
    morning: ['#0a1d3a', '#3f7fb8'],
    day: ['#0b2342', '#2f6d9e'],
    evening: ['#120a2a', '#9c3d6a'],
  };

  const SCENES = [
    {
      dur: 6, kind: 'title', sky: 'night', layers: [],
    },
    {
      dur: 12, clock: '06:42', title: 'Aufwachen', sky: 'dawn', layers: [0, 1, 2, 3],
      lines: [
        [0.8, 'sym', 'Guten Morgen. 7 h 12 min Schlaf, aber wenig Tiefschlaf.'],
        [3.8, 'sym', 'Vorschlag: das schwere Meeting auf 14 Uhr schieben?'],
        [7.2, 'me', 'Ja, mach das.'],
      ],
      decision: [8.2, '✓ Mensch entscheidet · Eintrag signiert'],
      tech: ['Uhr → Matter → Symbiot-Kern (Rust, no_std)', 'lokal auf dem Handy, offline', 'Ich'],
    },
    {
      dur: 12, clock: '08:20', title: 'Unterwegs', sky: 'morning', layers: [1, 2, 3, 5],
      lines: [
        [0.8, 'sym', 'Die S-Bahn hat 12 Minuten Verspätung.'],
        [3.4, 'sym', 'Soll ich Mira fragen, ob das Stand-up um 9:15 beginnen kann?'],
        [6.6, 'me', 'Ja, frag sie.'],
        [9.0, 'other', 'Mira: Passt, bis 9:15!'],
      ],
      decision: [7.6, '✓ Nur die Frage verlässt das Gerät'],
      tech: ['Brillen-HUD · Verkehrs-API · Matrix-Adapter', 'Connect-Knoten zu Hause · Ende-zu-Ende', 'Ich · Mira entscheidet selbst'],
    },
    {
      dur: 12, clock: '11:30', title: 'Arbeit & Wissen', sky: 'day', layers: [2, 3, 4, 5],
      lines: [
        [0.8, 'me', 'Welche Förderung gibt es für Solar auf Altbauten?'],
        [3.6, 'sym', 'Drei Programme passen. Jede Aussage mit Beleg – prüfbar.'],
        [6.8, 'sym', 'Aus deinem Gedächtnis: Gespräch mit dem Energieberater, 12. März.'],
      ],
      decision: [9.2, '✓ Übernommen ins Projekt „Dach“'],
      tech: ['Semantic Similarity · Wissensgraph · offene Daten', 'Home Server · Berechnung lokal', 'Ich · Quellen öffentlich (CC BY)'],
    },
    {
      dur: 13, clock: '18:05', title: 'Abendessen', sky: 'evening', layers: [1, 3, 4, 5],
      lines: [
        [0.8, 'me', 'Was kochen wir heute?'],
        [2.8, 'sym', 'Im Kühlschrank: Linsen, Tomaten, Zucchini. Lea isst kein Fleisch.'],
        [6.0, 'sym', 'Linsen-Bolognese, 25 Minuten. Soll ich Lea fragen, ob sie Käse mitbringt?'],
        [8.8, 'me', 'Ja.'],
        [10.4, 'other', 'Lea: Bin um 18:40 da!'],
      ],
      decision: [9.6, '✓ Einwilligung von Lea · gültig bis 20:00'],
      tech: ['Kühlschrank (Matter) · Rezept-API · ACI-Rückfrage', 'Heimnetz · Leas eigener Knoten', 'Haushalt · Lea · Ich'],
    },
    {
      dur: 12, clock: '22:40', title: 'Erinnern', sky: 'night', layers: [4], chain: true,
      lines: [
        [0.8, 'sym', 'Heute: 3 Entscheidungen, 2 Erkenntnisse, 1 neues Rezept.'],
        [4.0, 'sym', 'Alles in deiner Kette – signiert, lokal, nur für dich.'],
        [7.4, 'me', 'Danke. Gute Nacht.'],
      ],
      decision: [8.4, '✓ 6 Einträge signiert · Kopie auf dem Home Server'],
      tech: ['Hash-Kette + Ed25519 · SQLite', 'lokal + Home Server (Kopie, kein Herr)', 'Ich – exportierbar, löschbar'],
    },
    {
      dur: 10, kind: 'outro', sky: 'night', layers: [0, 1, 2, 3, 4, 5],
    },
  ];
  let acc = 0;
  for (const s of SCENES) { s.t0 = acc; acc += s.dur; }
  const DURATION = acc;

  // ---------- Hilfen ----------

  const clamp = (x, a = 0, b = 1) => Math.max(a, Math.min(b, x));
  const ease = (x) => { x = clamp(x); return x * x * (3 - 2 * x); };
  const lerp = (a, b, k) => a + (b - a) * k;
  const fract = (x) => x - Math.floor(x);
  const hash = (n) => fract(Math.sin(n * 127.1 + 311.7) * 43758.5453);

  function hexRgb(hex) {
    const v = parseInt(hex.slice(1), 16);
    return [(v >> 16) & 255, (v >> 8) & 255, v & 255];
  }
  function rgba(hex, a) {
    const [r, g, b] = hexRgb(hex);
    return `rgba(${r},${g},${b},${clamp(a).toFixed(3)})`;
  }
  function mixHex(a, b, k) {
    const x = hexRgb(a), y = hexRgb(b);
    return '#' + x.map((v, i) => Math.round(lerp(v, y[i], k)).toString(16).padStart(2, '0')).join('');
  }

  function sceneAt(t) {
    for (const s of SCENES) if (t < s.t0 + s.dur) return s;
    return SCENES[SCENES.length - 1];
  }

  // Sichtbarkeit einer Szene mit weicher Blende an den Rändern
  function sceneWeight(s, t) {
    return ease((t - s.t0) / 1.0) * ease((s.t0 + s.dur - t) / 1.0);
  }

  function layerWeight(i, t) {
    let w = 0;
    for (const s of SCENES) if (s.layers.includes(i)) w = Math.max(w, sceneWeight(s, t));
    return w;
  }

  function skyColors(t) {
    const s = sceneAt(t);
    const idx = SCENES.indexOf(s);
    const next = SCENES[idx + 1];
    const k = next ? ease((t - (s.t0 + s.dur - 1.5)) / 1.5) : 0;
    const a = SKY[s.sky], b = next ? SKY[next.sky] : a;
    return [mixHex(a[0], b[0], k), mixHex(a[1], b[1], k)];
  }

  // ---------- Szene: Himmel, Stadt, Boden ----------

  const HORIZON = H * 0.76;

  function drawBackground(t) {
    const [top, bottom] = skyColors(t);
    const g = ctx.createLinearGradient(0, 0, 0, HORIZON);
    g.addColorStop(0, top);
    g.addColorStop(1, bottom);
    ctx.fillStyle = g;
    ctx.fillRect(0, 0, W, HORIZON);

    // Sterne, nur wenn der Himmel dunkel ist
    const lum = hexRgb(bottom).reduce((a, b) => a + b, 0) / 765;
    const starA = clamp(1 - lum * 3);
    for (let i = 0; i < 160; i++) {
      const x = hash(i) * W, y = hash(i + 500) * HORIZON * 0.8;
      const tw = 0.5 + 0.5 * Math.sin(t * 1.3 + i);
      ctx.fillStyle = `rgba(220,230,255,${(0.2 + 0.6 * hash(i + 900)) * tw * starA})`;
      ctx.fillRect(x, y, 1.4, 1.4);
    }

    // Skyline in zwei Ebenen, mit Fenstern (nachts mehr Licht)
    for (const [layer, base, color] of [[0, 0.58, '#0b0f1e'], [1, 0.64, '#060912']]) {
      let x = -20 + (layer * 37);
      let i = layer * 100;
      while (x < W + 20) {
        const w = 26 + hash(i) * 46;
        const h = (layer ? 50 : 80) + hash(i + 1) * (layer ? 120 : 160);
        const y = HORIZON - h * (base + 0.42);
        ctx.fillStyle = color;
        ctx.fillRect(x, y, w, HORIZON - y);
        for (let wy = y + 8; wy < HORIZON - 8; wy += 9) {
          for (let wx = x + 5; wx < x + w - 5; wx += 8) {
            const n = hash(wx * 3.1 + wy * 7.7 + layer);
            if (n > 0.86 - starA * 0.18) {
              ctx.fillStyle = `rgba(255,${200 + n * 40},${140 + n * 80},${0.25 + 0.4 * starA})`;
              ctx.fillRect(wx, wy, 3, 4);
            }
          }
        }
        x += w + 4;
        i += 3;
      }
    }

    // Boden mit Perspektivgitter
    const gg = ctx.createLinearGradient(0, HORIZON, 0, H);
    gg.addColorStop(0, '#070b18');
    gg.addColorStop(1, '#020309');
    ctx.fillStyle = gg;
    ctx.fillRect(0, HORIZON, W, H - HORIZON);
    ctx.strokeStyle = 'rgba(90,209,255,0.13)';
    ctx.lineWidth = 1;
    const vx = W * 0.45;
    for (let i = -16; i <= 16; i++) {
      ctx.beginPath();
      ctx.moveTo(vx + i * 14, HORIZON);
      ctx.lineTo(vx + i * 160, H);
      ctx.stroke();
    }
    for (let i = 0; i < 9; i++) {
      const k = fract(i / 9 + t * 0.05);
      const y = HORIZON + (H - HORIZON) * k * k;
      ctx.globalAlpha = k;
      ctx.beginPath();
      ctx.moveTo(0, y);
      ctx.lineTo(W, y);
      ctx.stroke();
    }
    ctx.globalAlpha = 1;
    ctx.fillStyle = 'rgba(90,209,255,0.35)';
    ctx.fillRect(0, HORIZON, W, 1);
  }

  // ---------- Der Mensch ----------

  const HUMAN = { x: W * 0.25, feet: HORIZON + 6, h: 360 };

  function lastLine(t, who) {
    const s = sceneAt(t);
    if (!s.lines) return null;
    let found = null;
    for (const l of s.lines) if (l[1] === who && t - s.t0 >= l[0]) found = l;
    return found ? { at: s.t0 + found[0], text: found[2] } : null;
  }

  function drawHuman(t, appear) {
    const { x, feet, h } = HUMAN;
    const s = h / 360;
    const top = feet - h;
    const breathe = Math.sin(t * 1.6) * 1.5;
    ctx.save();
    ctx.globalAlpha = appear;

    // Körper als Silhouette: Kopf und Rumpf als Fläche, Arme und Beine als breite Linien.
    // Erst alles mit Kantenlicht nachziehen, dann füllen – so bleibt nur die äußere Kontur sichtbar.
    const hy = top + 34 * s; // Kopfmitte
    const P = (px, py) => [x + px * s, hy + py * s];
    const trunk = new Path2D();
    trunk.ellipse(x, hy + breathe * 0.3, 25 * s, 31 * s, 0, 0, Math.PI * 2);
    trunk.moveTo(...P(-10, 22));
    trunk.lineTo(...P(-12, 40));
    trunk.bezierCurveTo(...P(-40, 44), ...P(-56, 50), ...P(-58, 66));
    trunk.lineTo(...P(-48, 120));
    trunk.lineTo(...P(-42, 172));
    trunk.lineTo(...P(42, 172));
    trunk.lineTo(...P(48, 120));
    trunk.lineTo(...P(58, 66));
    trunk.bezierCurveTo(...P(56, 50), ...P(40, 44), ...P(12, 40));
    trunk.lineTo(...P(10, 22));
    trunk.closePath();
    const sway = Math.sin(t * 1.1) * 2;
    const limbs = [
      [18 * s, [P(-62, 66), P(-74 - sway, 120), P(-78 - sway, 176)]],
      [18 * s, [P(62, 66), P(72 + sway, 120), P(76 + sway, 176)]],
      [30 * s, [P(-21, 160), P(-23, 250), [x - 24 * s, feet - 10]]],
      [30 * s, [P(21, 160), P(23, 250), [x + 24 * s, feet - 10]]],
      [11 * s, [[x - 26 * s, feet - 5], [x - 40 * s, feet - 5]]],
      [11 * s, [[x + 26 * s, feet - 5], [x + 40 * s, feet - 5]]],
    ];
    const limbPath = (pts) => {
      const p = new Path2D();
      p.moveTo(...pts[0]);
      for (const q of pts.slice(1)) p.lineTo(...q);
      return p;
    };
    const fill = ctx.createLinearGradient(x - 80, top, x + 80, feet);
    fill.addColorStop(0, '#1a2440');
    fill.addColorStop(1, '#070a14');
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.shadowColor = '#5ad1ff';
    ctx.shadowBlur = 18;
    ctx.strokeStyle = 'rgba(90,209,255,0.6)';
    ctx.lineWidth = 3.2;
    ctx.stroke(trunk);
    for (const [lw, pts] of limbs) {
      ctx.lineWidth = lw + 3.2;
      ctx.stroke(limbPath(pts));
    }
    ctx.shadowBlur = 0;
    ctx.fillStyle = fill;
    ctx.strokeStyle = fill;
    ctx.fill(trunk);
    for (const [lw, pts] of limbs) {
      ctx.lineWidth = lw;
      ctx.stroke(limbPath(pts));
    }
    ctx.lineCap = 'butt';

    // Kybernetische Akzente: Leiterbahnen von der Schläfe über Hals und Arm
    const pulse = (k) => 0.35 + 0.65 * Math.max(0, Math.sin(t * 2.2 - k));
    ctx.lineWidth = 1.4;
    const traces = [
      [[x + 18, hy - 6], [x + 22, hy + 10], [x + 10, hy + 28], [x + 10, hy + 46], [x + 40, hy + 56], [x + 62, hy + 70], [x + 72, hy + 120], [x + 76, hy + 172]],
      [[x + 18, hy - 6], [x + 8, hy - 26], [x - 8, hy - 30]],
      [[x - 4, hy + 50], [x - 4, hy + 80], [x + 18, hy + 92], [x + 18, hy + 130]],
    ].map((p) => p.map(([px, py]) => [x + (px - x) * s, hy + (py - hy) * s]));
    traces.forEach((pts, k) => {
      ctx.strokeStyle = `rgba(90,209,255,${0.5 * pulse(k)})`;
      ctx.beginPath();
      pts.forEach(([px, py], i) => (i ? ctx.lineTo(px, py) : ctx.moveTo(px, py)));
      ctx.stroke();
      // Lichtpunkt, der die Bahn entlangläuft
      const u = fract(t * 0.45 + k * 0.33) * (pts.length - 1);
      const i = Math.floor(u), f = u - i;
      const [ax, ay] = pts[i], [bx, by] = pts[Math.min(i + 1, pts.length - 1)];
      glow(lerp(ax, bx, f), lerp(ay, by, f), 6, '#5ad1ff', 0.9);
    });

    // Visier / HUD-Auge
    ctx.strokeStyle = 'rgba(120,230,255,0.9)';
    ctx.lineWidth = 3;
    ctx.shadowColor = '#5ad1ff';
    ctx.shadowBlur = 14;
    ctx.beginPath();
    ctx.moveTo(x - 20 * s, hy - 4 * s);
    ctx.quadraticCurveTo(x, hy - 9 * s, x + 24 * s, hy - 4 * s);
    ctx.stroke();
    ctx.shadowBlur = 0;
    // Schläfenknoten = Schnittstelle zum Symbiot
    glow(x + 18 * s, hy - 6 * s, 9, '#5ad1ff', 0.9 + 0.1 * Math.sin(t * 4));

    // Herzschlag (Ebene L0)
    const beat = Math.pow(Math.max(0, Math.sin(t * 7.2)), 12) + 0.25;
    glow(x - 16 * s, hy + 80 * s, 16 + beat * 10, '#ff5c7a', 0.35 + beat * 0.5);
    // Armband
    ctx.strokeStyle = 'rgba(255,181,71,0.9)';
    ctx.lineWidth = 3;
    ctx.beginPath();
    ctx.moveTo(x - 88 * s - sway, hy + 160 * s);
    ctx.lineTo(x - 68 * s - sway, hy + 163 * s);
    ctx.stroke();
    glow(x - 78 * s - sway, hy + 161 * s, 8, '#ffb547', 0.8);

    // Sprechen: Schallwellen vom Mund
    const me = lastLine(t, 'me');
    if (me && t - me.at < 1.8) {
      const k = (t - me.at) / 1.8;
      for (let r = 0; r < 3; r++) {
        const rr = 14 + (k * 40 + r * 12);
        ctx.strokeStyle = `rgba(255,181,71,${(1 - k) * 0.7})`;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(x + 20 * s, hy + 14 * s, rr, -0.6, 0.6);
        ctx.stroke();
      }
    }
    ctx.restore();
    return { temple: [x + 18 * s, hy - 6 * s], heart: [x - 16 * s, hy + 80 * s] };
  }

  function glow(x, y, r, color, a) {
    const g = ctx.createRadialGradient(x, y, 0, x, y, r);
    g.addColorStop(0, rgba(color, a));
    g.addColorStop(0.35, rgba(color, a * 0.5));
    g.addColorStop(1, rgba(color, 0));
    ctx.fillStyle = g;
    ctx.beginPath();
    ctx.arc(x, y, r, 0, Math.PI * 2);
    ctx.fill();
  }

  // ---------- Der Symbiot ----------

  const SYM = { x: W * 0.5, y: H * 0.36, r: 62 };
  const PHI = (1 + Math.sqrt(5)) / 2;
  const ICO = [
    [-1, PHI, 0], [1, PHI, 0], [-1, -PHI, 0], [1, -PHI, 0],
    [0, -1, PHI], [0, 1, PHI], [0, -1, -PHI], [0, 1, -PHI],
    [PHI, 0, -1], [PHI, 0, 1], [-PHI, 0, -1], [-PHI, 0, 1],
  ];
  const ICO_EDGES = [];
  for (let i = 0; i < 12; i++) {
    for (let j = i + 1; j < 12; j++) {
      const d = Math.hypot(...ICO[i].map((v, k) => v - ICO[j][k]));
      if (Math.abs(d - 2) < 0.01) ICO_EDGES.push([i, j]);
    }
  }

  // Lage und Größe des Symbiot hängen nur von t ab; Strahlen und Lichtband brauchen sie vor dem Zeichnen
  function symbiotPose(t, appear) {
    const sym = lastLine(t, 'sym');
    const talk = sym ? clamp(1 - (t - sym.at) / 2.5) : 0;
    SYM.cy = SYM.y + Math.sin(t * 0.9) * 6;
    SYM.size = SYM.r * (0.6 + 0.4 * appear) * (1 + 0.05 * Math.sin(t * 2) + 0.08 * talk);
    return { sym, talk };
  }

  function drawSymbiot(t, appear) {
    if (appear <= 0) return;
    const { x } = SYM;
    const { sym, talk } = symbiotPose(t, appear);
    const y = SYM.cy, size = SYM.size;

    glow(x, y, size * 3.2, '#5ad1ff', 0.18 * appear);
    glow(x, y, size * 1.6, '#9b8cff', 0.25 * appear);

    // Umlaufende Partikel
    for (let i = 0; i < 40; i++) {
      const a = t * (0.4 + hash(i) * 0.6) + i * 2.4;
      const rr = size * (1.3 + hash(i + 40) * 0.9);
      const tilt = hash(i + 80) * 0.9 - 0.45;
      const px = x + Math.cos(a) * rr;
      const py = y + Math.sin(a) * rr * 0.35 + Math.cos(a) * rr * tilt * 0.3;
      ctx.fillStyle = `rgba(180,230,255,${(0.3 + 0.5 * hash(i + 7)) * appear})`;
      ctx.fillRect(px, py, 2, 2);
    }

    // Ikosaeder als Drahtgitter
    const ry = t * 0.5, rx = t * 0.31;
    const pts = ICO.map(([a, b, c]) => {
      const n = Math.hypot(a, b, c);
      a /= n; b /= n; c /= n;
      let x1 = a * Math.cos(ry) - c * Math.sin(ry);
      let z1 = a * Math.sin(ry) + c * Math.cos(ry);
      let y1 = b * Math.cos(rx) - z1 * Math.sin(rx);
      z1 = b * Math.sin(rx) + z1 * Math.cos(rx);
      return [x + x1 * size, y + y1 * size, z1];
    });
    ctx.lineWidth = 1.4;
    for (const [i, j] of ICO_EDGES) {
      const z = (pts[i][2] + pts[j][2]) / 2;
      ctx.strokeStyle = `rgba(150,225,255,${(0.25 + 0.55 * (z + 1) / 2) * appear})`;
      ctx.beginPath();
      ctx.moveTo(pts[i][0], pts[i][1]);
      ctx.lineTo(pts[j][0], pts[j][1]);
      ctx.stroke();
    }
    for (const p of pts) glow(p[0], p[1], 5, '#bff0ff', 0.6 * appear * (p[2] + 1.2) / 2.2);

    // Kern, pulsiert beim Sprechen
    glow(x, y, size * (0.45 + 0.25 * talk), '#ffffff', 0.85 * appear);
    glow(x, y, size * (0.8 + 0.3 * talk), '#5ad1ff', 0.6 * appear);
    if (talk > 0) {
      for (let k = 0; k < 2; k++) {
        const q = fract((t - sym.at) * 0.8 + k * 0.5);
        ctx.strokeStyle = `rgba(90,209,255,${(1 - q) * 0.6 * talk})`;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(x, y, size * (1 + q * 1.2), 0, Math.PI * 2);
        ctx.stroke();
      }
    }

    ctx.shadowColor = 'rgba(0,0,0,0.9)';
    ctx.shadowBlur = 8;
    ctx.fillStyle = `rgba(200,235,255,${0.85 * appear})`;
    ctx.font = `600 13px ${FONT}`;
    ctx.textAlign = 'center';
    ctx.fillText('SYMBIOT', x, y + size * 1.55 + 22);
    ctx.font = `12px ${FONT}`;
    ctx.fillStyle = `rgba(160,180,210,${0.8 * appear})`;
    ctx.fillText('KI · Assistent · Übersetzer', x, y + size * 1.55 + 40);
    ctx.shadowBlur = 0;
  }

  // Lichtband zwischen Mensch und Symbiot (mehrere verflochtene Stränge)
  function drawTether(t, from, appear) {
    if (appear <= 0) return;
    const [ax, ay] = from;
    const bx = SYM.x - SYM.size * 0.6, by = SYM.cy;
    for (let k = 0; k < 4; k++) {
      ctx.strokeStyle = rgba(k % 2 ? '#9b8cff' : '#5ad1ff', 0.35 * appear);
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      for (let i = 0; i <= 40; i++) {
        const u = i / 40;
        const px = lerp(ax, bx, u);
        const wave = Math.sin(u * Math.PI * 3 + t * 2 + k * 1.6) * 10 * Math.sin(u * Math.PI);
        const py = lerp(ay, by, u) - Math.sin(u * Math.PI) * 30 + wave;
        if (i) ctx.lineTo(px, py); else ctx.moveTo(px, py);
      }
      ctx.stroke();
    }
    for (let i = 0; i < 6; i++) {
      const u = fract(t * 0.35 + i / 6);
      const dir = i % 2 ? u : 1 - u;
      const px = lerp(ax, bx, dir);
      const py = lerp(ay, by, dir) - Math.sin(dir * Math.PI) * 30;
      glow(px, py, 7, i % 2 ? '#ffb547' : '#5ad1ff', 0.9 * appear);
    }
  }

  // ---------- Turm der Ebenen ----------

  const TOWER = { x: W * 0.765, yBottom: H * 0.68, yTop: H * 0.14, rx: 118, ry: 17 };

  function layerY(i) {
    return lerp(TOWER.yBottom, TOWER.yTop, i / (LAYERS.length - 1));
  }

  function drawTower(t, appear) {
    if (appear <= 0) return;
    const { x, rx, ry } = TOWER;

    // Säule
    const col = ctx.createLinearGradient(0, TOWER.yTop, 0, TOWER.yBottom);
    col.addColorStop(0, rgba('#ff6fa8', 0.25 * appear));
    col.addColorStop(1, rgba('#ff8a5c', 0.25 * appear));
    ctx.fillStyle = col;
    ctx.fillRect(x - 1.5, TOWER.yTop, 3, TOWER.yBottom - TOWER.yTop);

    LAYERS.forEach((L, i) => {
      const y = layerY(i);
      const w = layerWeight(i, t);
      const a = appear * (0.3 + 0.7 * w);

      // Scheibe
      ctx.fillStyle = rgba(L.color, 0.06 + 0.12 * w);
      ctx.strokeStyle = rgba(L.color, a * 0.9);
      ctx.lineWidth = 1 + w * 1.5;
      if (w > 0.05) { ctx.shadowColor = L.color; ctx.shadowBlur = 16 * w; }
      ctx.beginPath();
      ctx.ellipse(x, y, rx, ry, 0, 0, Math.PI * 2);
      ctx.fill();
      ctx.stroke();
      ctx.shadowBlur = 0;

      // Beschriftung
      ctx.textAlign = 'left';
      ctx.font = `600 12px ${FONT}`;
      ctx.fillStyle = rgba(L.color, a);
      ctx.fillText(`${L.id}`, x + rx + 12, y + 4);
      ctx.font = `12px ${FONT}`;
      ctx.fillStyle = `rgba(225,232,245,${a})`;
      ctx.fillText(L.name, x + rx + 36, y + 4);

      // Knoten auf der Scheibe
      L.items.forEach((name, j) => {
        const ang = t * (0.25 + i * 0.03) + (j / L.items.length) * Math.PI * 2 + i;
        const nx = x + Math.cos(ang) * rx * 0.82;
        const ny = y + Math.sin(ang) * ry * 0.82;
        const front = Math.sin(ang) > 0;
        glow(nx, ny, front ? 9 : 6, L.color, a * (front ? 0.95 : 0.4));
        if (front && w > 0.3) {
          ctx.font = `11px ${FONT}`;
          ctx.textAlign = 'center';
          ctx.fillStyle = `rgba(235,240,250,${w * appear * 0.95})`;
          ctx.fillText(name, nx, ny - 9);
        }
      });

      // Pakete zwischen Ebenen auf der Säule
      if (w > 0.05 && i < LAYERS.length - 1) {
        for (let k = 0; k < 2; k++) {
          const u = fract(t * 0.6 + k * 0.5 + i * 0.13);
          const py = lerp(y, layerY(i + 1), k ? u : 1 - u);
          glow(x, py, 6, L.color, w * appear);
        }
      }

      // Strahl vom Symbiot zur aktiven Ebene
      if (w > 0.02) {
        const sx = SYM.x + SYM.size * 0.7, sy = SYM.cy;
        const ex = x - rx, ey = y;
        const cx = lerp(sx, ex, 0.5), cy = Math.min(sy, ey) - 20;
        ctx.strokeStyle = rgba(L.color, 0.45 * w * appear);
        ctx.lineWidth = 1.6;
        ctx.beginPath();
        ctx.moveTo(sx, sy);
        ctx.quadraticCurveTo(cx, cy, ex, ey);
        ctx.stroke();
        for (let k = 0; k < 3; k++) {
          const u = fract(t * 0.5 + k / 3 + i * 0.21);
          const q = k % 2 ? u : 1 - u;
          const px = (1 - q) * (1 - q) * sx + 2 * (1 - q) * q * cx + q * q * ex;
          const py = (1 - q) * (1 - q) * sy + 2 * (1 - q) * q * cy + q * q * ey;
          glow(px, py, 6, L.color, w * appear);
        }
      }
    });
  }

  // ---------- Kette der signierten Einträge ----------

  function drawChain(t) {
    const s = SCENES.find((x) => x.chain);
    const w = sceneWeight(s, t);
    if (w <= 0) return;
    const local = t - s.t0;
    const n = 6;
    const bw = 64, gap = 22;
    const total = n * bw + (n - 1) * gap;
    const x0 = SYM.x - total / 2 - 40, y0 = H * 0.08;
    const labels = ['Schlaf', 'Termin', 'Mira', 'Solar', 'Rezept', 'Lea'];
    for (let i = 0; i < n; i++) {
      const k = ease((local - 1 - i * 0.9) / 0.6) * w;
      if (k <= 0) continue;
      const x = x0 + i * (bw + gap);
      ctx.globalAlpha = k;
      ctx.strokeStyle = '#5cf2a6';
      ctx.fillStyle = 'rgba(92,242,166,0.1)';
      ctx.lineWidth = 1.5;
      ctx.shadowColor = '#5cf2a6';
      ctx.shadowBlur = 10;
      ctx.beginPath();
      ctx.roundRect(x, y0 + (1 - k) * 12, bw, 40, 6);
      ctx.fill();
      ctx.stroke();
      ctx.shadowBlur = 0;
      ctx.fillStyle = '#d9fff0';
      ctx.font = `600 11px ${FONT}`;
      ctx.textAlign = 'center';
      ctx.fillText(labels[i], x + bw / 2, y0 + 17);
      ctx.font = `10px ui-monospace, monospace`;
      ctx.fillStyle = 'rgba(92,242,166,0.85)';
      ctx.fillText(`#${(hash(i + 3) * 0xffffff | 0).toString(16).padStart(6, '0')}`, x + bw / 2, y0 + 32);
      if (i > 0) {
        ctx.strokeStyle = 'rgba(92,242,166,0.8)';
        ctx.beginPath();
        ctx.moveTo(x - gap, y0 + 20);
        ctx.lineTo(x, y0 + 20);
        ctx.stroke();
      }
      ctx.globalAlpha = 1;
    }
  }

  // ---------- HUD: Uhr, Dialog, Technik ----------

  function wrapText(text, maxW) {
    const words = text.split(' ');
    const out = [];
    let line = '';
    for (const w of words) {
      const test = line ? `${line} ${w}` : w;
      if (ctx.measureText(test).width > maxW && line) { out.push(line); line = w; } else line = test;
    }
    if (line) out.push(line);
    return out;
  }

  function panel(x, y, w, h, a) {
    ctx.fillStyle = `rgba(8,12,24,${0.72 * a})`;
    ctx.strokeStyle = `rgba(255,255,255,${0.14 * a})`;
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.roundRect(x, y, w, h, 14);
    ctx.fill();
    ctx.stroke();
  }

  function drawHud(t) {
    const s = sceneAt(t);
    if (s.kind) return;
    const w = sceneWeight(s, t);
    const local = t - s.t0;

    // Uhr und Titel
    ctx.globalAlpha = w;
    ctx.textAlign = 'left';
    ctx.font = `700 46px ${FONT}`;
    ctx.fillStyle = '#e8ecf5';
    ctx.fillText(s.clock, 40, 74);
    ctx.font = `600 14px ${FONT}`;
    ctx.fillStyle = '#5ad1ff';
    ctx.fillText(s.title.toUpperCase(), 42, 100);
    ctx.font = `12px ${FONT}`;
    ctx.fillStyle = 'rgba(200,210,230,0.8)';
    const act = s.layers.map((i) => LAYERS[i].id).join(' · ');
    ctx.fillText(`aktive Ebenen: ${act}`, 42, 120);
    ctx.globalAlpha = 1;

    // Dialog
    const px = 32, py = H * 0.79, pw = W * 0.6, ph = H * 0.18;
    panel(px, py, pw, ph, w);
    const visible = s.lines.filter((l) => local >= l[0]).slice(-3);
    let y = py + 26;
    ctx.textAlign = 'left';
    for (const [at, who, text] of visible) {
      const chars = Math.floor((local - at) * 45);
      const shown = text.slice(0, chars);
      const color = who === 'sym' ? '#5ad1ff' : who === 'me' ? '#ffb547' : '#ff6fa8';
      const name = who === 'sym' ? 'Symbiot' : who === 'me' ? 'Ich' : '';
      ctx.globalAlpha = w;
      ctx.font = `600 13px ${FONT}`;
      ctx.fillStyle = color;
      if (name) ctx.fillText(name, px + 18, y);
      ctx.font = `15px ${FONT}`;
      ctx.fillStyle = '#e8ecf5';
      const lines = wrapText(shown, pw - 120);
      lines.forEach((ln, i) => ctx.fillText(ln, px + (name ? 84 : 18), y + i * 19));
      y += Math.max(1, lines.length) * 19 + 8;
    }
    ctx.globalAlpha = 1;

    // Entscheidung als Chip über dem Dialog
    if (s.decision && local >= s.decision[0]) {
      const k = ease((local - s.decision[0]) / 0.4) * w;
      ctx.font = `600 13px ${FONT}`;
      const tw = ctx.measureText(s.decision[1]).width + 28;
      const cx = px, cy = py - 40 + (1 - k) * 8;
      ctx.globalAlpha = k;
      ctx.fillStyle = 'rgba(92,242,166,0.16)';
      ctx.strokeStyle = '#5cf2a6';
      ctx.beginPath();
      ctx.roundRect(cx, cy, tw, 28, 14);
      ctx.fill();
      ctx.stroke();
      ctx.fillStyle = '#d9fff0';
      ctx.textAlign = 'left';
      ctx.fillText(s.decision[1], cx + 14, cy + 19);
      ctx.globalAlpha = 1;
    }

    // Technik-Steckbrief
    const tx = W * 0.64, ty = py, tw2 = W - tx - 32;
    panel(tx, ty, tw2, ph, w);
    const rows = [['Stack', s.tech[0]], ['Server', s.tech[1]], ['Owner', s.tech[2]]];
    ctx.globalAlpha = w;
    rows.forEach(([k, v], i) => {
      const ry = ty + 28 + i * 36;
      ctx.font = `600 11px ${FONT}`;
      ctx.fillStyle = '#8a93a8';
      ctx.fillText(k.toUpperCase(), tx + 16, ry);
      ctx.font = `13px ${FONT}`;
      ctx.fillStyle = '#e8ecf5';
      wrapText(v, tw2 - 32).slice(0, 1).forEach((ln) => ctx.fillText(ln, tx + 16, ry + 16));
    });
    ctx.globalAlpha = 1;
  }

  // ---------- Titel und Abspann ----------

  function drawTitle(t) {
    const s = SCENES[0];
    const w = sceneWeight({ t0: s.t0 - 1, dur: s.dur + 1 }, t);
    if (w <= 0) return;
    ctx.globalAlpha = w;
    ctx.textAlign = 'center';
    ctx.font = `600 14px ${FONT}`;
    ctx.fillStyle = '#5ad1ff';
    ctx.fillText('SYMBIOT  ×  CONNECT', W / 2, H * 0.8);
    ctx.font = `700 44px ${FONT}`;
    ctx.fillStyle = '#e8ecf5';
    ctx.fillText('Ein Tag mit meinem Symbiot', W / 2, H * 0.8 + 50);
    ctx.font = `16px ${FONT}`;
    ctx.fillStyle = 'rgba(200,210,230,0.85)';
    ctx.fillText('Mensch · Maschine · alle Ebenen dazwischen', W / 2, H * 0.8 + 80);
    ctx.globalAlpha = 1;
  }

  function drawOutro(t) {
    const s = SCENES[SCENES.length - 1];
    const local = t - s.t0;
    if (local < 0) return;
    const a1 = ease((local - 0.8) / 1);
    const a2 = ease((local - 3.6) / 1);
    ctx.textAlign = 'center';
    ctx.globalAlpha = a1;
    ctx.font = `700 34px ${FONT}`;
    ctx.fillStyle = '#e8ecf5';
    ctx.fillText('Die KI schlägt vor. Der Mensch entscheidet.', W / 2, H * 0.84);
    ctx.globalAlpha = a2;
    ctx.font = `600 18px ${FONT}`;
    ctx.fillStyle = '#5ad1ff';
    ctx.fillText('Connect. Egal mit was.   ·   Deine Daten. Deine Regeln.', W / 2, H * 0.84 + 40);
    ctx.globalAlpha = 1;
  }

  // ---------- Bild zusammensetzen ----------

  function render(t) {
    t = clamp(t, 0, DURATION - 0.001);
    const scale = canvas.width / W;
    ctx.setTransform(scale, 0, 0, scale, 0, 0);
    ctx.clearRect(0, 0, W, H);
    drawBackground(t);

    const humanIn = ease((t - 0.6) / 2);
    const symIn = ease((t - 2) / 2);
    const towerIn = ease((t - 3.2) / 2);
    symbiotPose(t, symIn);
    drawTower(t, towerIn);
    const anchors = drawHuman(t, humanIn);
    drawSymbiot(t, symIn);
    drawTether(t, anchors.temple, symIn);
    // L0 hängt direkt am Körper: Leitung vom Herzen zur untersten Ebene
    if (layerWeight(0, t) > 0.02) drawBodyLink(t, anchors.heart);
    drawChain(t);
    drawHud(t);
    drawTitle(t);
    drawOutro(t);

    // Kino-Vignette und Ein-/Ausblende
    const v = ctx.createRadialGradient(W / 2, H / 2, H * 0.4, W / 2, H / 2, H * 0.95);
    v.addColorStop(0, 'rgba(0,0,0,0)');
    v.addColorStop(1, 'rgba(0,0,0,0.55)');
    ctx.fillStyle = v;
    ctx.fillRect(0, 0, W, H);
    const fade = 1 - ease(t / 1.2) * ease((DURATION - t) / 1.5);
    if (fade > 0) {
      ctx.fillStyle = `rgba(0,0,0,${fade})`;
      ctx.fillRect(0, 0, W, H);
    }
  }

  function drawBodyLink(t, heart) {
    const w0 = layerWeight(0, t);
    const [hx, hy] = heart;
    const ex = TOWER.x - TOWER.rx, ey = layerY(0);
    ctx.strokeStyle = rgba('#ff8a5c', 0.4 * w0);
    ctx.lineWidth = 1.5;
    ctx.setLineDash([4, 6]);
    ctx.lineDashOffset = -t * 30;
    ctx.beginPath();
    ctx.moveTo(hx, hy);
    ctx.quadraticCurveTo((hx + ex) / 2, ey + 80, ex, ey);
    ctx.stroke();
    ctx.setLineDash([]);
  }

  // ---------- Abspielen ----------

  window.FILM_DURATION = DURATION;
  window.renderFrame = render;

  if (RENDER) {
    const scale = Number(params.get('scale')) || 1.5;
    canvas.width = W * scale;
    canvas.height = H * scale;
    document.body.classList.add('render');
    render(Number(params.get('t')) || 0);
    return;
  }

  function fit() {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const box = canvas.getBoundingClientRect();
    canvas.width = Math.round(box.width * dpr);
    canvas.height = Math.round((box.width * dpr * H) / W);
  }
  window.addEventListener('resize', fit);
  fit();

  let start = performance.now();
  let paused = false;
  let pausedAt = 0;
  const btn = document.getElementById('play');
  btn.addEventListener('click', () => {
    if (paused) { start = performance.now() - pausedAt * 1000; paused = false; } else { pausedAt = current(); paused = true; }
    btn.textContent = paused ? '▶ Weiter' : '❚❚ Pause';
  });
  document.getElementById('restart').addEventListener('click', () => {
    start = performance.now();
    paused = false;
    btn.textContent = '❚❚ Pause';
  });
  const current = () => ((performance.now() - start) / 1000) % DURATION;
  const bar = document.getElementById('progress');

  function loop() {
    const t = paused ? pausedAt : current();
    render(t);
    bar.style.width = `${(t / DURATION) * 100}%`;
    requestAnimationFrame(loop);
  }
  requestAnimationFrame(loop);
})();
