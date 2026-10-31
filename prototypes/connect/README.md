# Connect

**Egal mit was.** Prototyp der Interoperabilitätsplattform Connect – als interaktiver 3D-Graph.

> Das ist, was ich will. Das ist, was der andere will.
> Austausch zwischen Akteuren: Menschen und Menschen, Menschen und Maschinen, Menschen und Daten,
> Menschen und sich selbst – um mehr über die Welt zu erfahren, Wissen zu speichern und ein
> eigenes Gedächtnis zu besitzen.

Connect ist der Teil von Symbiot, der über einen einzelnen Menschen hinausgeht (siehe
[E1](../../docs/ENTSCHEIDUNGEN.md#e1--symbiot-und-connect-sind-ein-system--entschieden)).
Die Seite zeigt die **Idee**, nicht die fertige Plattform. Grundsatz: eine einfache Idee, dargestellt
in einem komplexen Graphen – aber die Idee bleibt sichtbar.

## Was man sieht

- **Drei Stufen**, die sich beim Laden von selbst entfalten (und per Knopf umschaltbar sind):
  1. *Die Idee* – Ich ⇄ Connect ⇄ andere Menschen, Maschinen, Daten, mein künftiges Ich
  2. *Die Plattform* – die Bausteine: Absicht, Gegenseite, Vermittlung, Übersetzung, Identität,
     Einwilligung, Vertrauen & Nachweis, Gedächtnis, offene Protokolle
  3. *Das Ökosystem* – Kanäle und Adapter („egal mit was“), die Zwecke und die vorhandenen
     Werkzeuge aus `oli-tech-tools` (Semantic Similarity, ACI, Open Avatar, OnePDF/TeXify, DR-Cockpit)
- **Multimedial:** Jeder Knoten hat ein Symbol, über die Kanten wandern Pakete verschiedener Medien
  (Sprache, Text, Bild, Dokument, Messwert, Erinnerung …). Optional leise Töne beim Abspielen
  (standardmäßig aus).
- **Szenarien** zum Abspielen, Schritt für Schritt: Mensch ↔ Mensch, Mensch ↔ Maschine,
  Mensch ↔ Daten, Mensch ↔ sich selbst.
- **Alltag: Abendessen planen** – ein alltäglicher Ablauf in 9 Schritten (Sprachbefehl → Absicht →
  Gedächtnis → Kühlschrank → Rezept-Datenbanken → Vermittlung → Rückfrage an Lea → Einwilligung →
  Kochmodus). Zu jedem Schritt ein Steckbrief: **Technik** (Stack), **Server** (Ort, Verschlüsselung,
  Latenz), **Owner** (wem die Daten gehören), **Datensets** (grün = nachrüstbar) und die **Anzeige**
  als Plugin-Vorschau. Die Angaben sind ein Entwurf der Zielarchitektur, keine laufende Technik.
- **Film „Ein Tag mit meinem Symbiot“** (`/film.html`, 77 s): ein Mensch und sein Symbiot – die KI an
  seiner Seite – durch fünf Alltagsszenen vom Aufwachen bis zur Nacht. Rechts der Turm der Ebenen
  L0 Körper & Sinne · L1 Geräte · L2 Daten & Signale · L3 Bausteine · L4 Gedächtnis · L5 Menschen & Welt;
  je Szene leuchten die beteiligten Ebenen auf, dazu Dialog, Entscheidung des Menschen und Steckbrief
  (Stack, Server, Owner). Die Regeln stammen aus Symbiot ([README](../../README.md#die-regeln)): die KI schlägt vor, der Mensch
  entscheidet; jede Änderung ist ein signierter Eintrag; local-first. Der Film hängt nur von der Zeit ab
  und lässt sich mit `film.html?render=1&scale=1.5` und `window.renderFrame(t)` Bild für Bild als Video
  rendern.
- **Erkunden:** Ziehen dreht, Mausrad/Pinch zoomt, Klick auf einen Knoten zeigt Beschreibung und
  Verbindungen. Alle Knoten gibt es auch als Liste (Tastatur, Screenreader). `Esc` schließt,
  `←`/`→` blättern im Szenario.

## Aufbau

```
server.js          Minimaler Node-Server ohne Abhängigkeiten (statische Dateien, /healthz)
Dockerfile         Container-Image, läuft als Nicht-root-Benutzer
public/
  index.html       Seite
  style.css        Styles
  data.js          Die Idee als Daten: Gruppen, Knoten, Kanten, Symbole, Stufen, Szenarien
  graph.js         Eigener 3D-Renderer auf Canvas 2D: Kräfte-Layout, Projektion, Pakete, Bedienung
  film.html/.js    Kurzfilm „Ein Tag mit meinem Symbiot“ (Canvas, deterministisch)
  film.css         Styles des Films
```

Inhalte ändern = `public/data.js` bearbeiten. Keine Bibliotheken, nichts von fremden Servern; die
Symbole sind Emojis aus den Systemschriften. Bei `prefers-reduced-motion` dreht sich der Graph nicht
von selbst.

## Starten

```sh
npm start
# → http://localhost:3000
```

Oder als Container:

```sh
docker build -t connect .
docker run -p 3000:3000 connect
```

## Deployment

Der Prototyp ist eigenständig und gehört nicht zum Server-Image von Symbiot (`prototypes/` steht in der
`.dockerignore` im Wurzelverzeichnis). Er bringt ein eigenes `Dockerfile` mit:

| Einstellung | Wert |
| --- | --- |
| Base Directory | `/prototypes/connect` |
| Build Pack | Dockerfile |
| Port | `3000` (`$PORT`) |
| Health-Check-Pfad | `/healthz` |
| Watch Paths | `prototypes/connect/**` |
| Umgebungsvariablen | keine nötig |

`public/` funktioniert auch rein statisch (z. B. GitHub Pages); die Sicherheits-Header aus
`server.js` sollten dann beim Hoster gesetzt werden.
