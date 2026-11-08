# Plan — den ganzen Dev-Lifecycle so schnell wie möglich abdecken

**Prinzip: Walking Skeleton.** Zuerst wandert *ein einziges, winziges Feature* durch **alle** Stationen:
planen → coden → testen → bauen → releasen → deployen → betreiben → beobachten.
Erst danach wird das Skelett dicker. Jeder Meilenstein endet mit einem **Checkpoint**, den du abnimmst.

## Der Lifecycle und womit wir jede Station abdecken

| Station | Werkzeug | Ab |
|---|---|---|
| Planen | GitHub Issues + Milestones (M0…M6), Entscheidungen in `docs/` | M0 |
| Coden | Cargo-Workspace, `rustfmt`, `clippy` | M0 |
| Testen | `cargo nextest`, `proptest` für den Ledger, axum-Integrationstests | M0 |
| No-std-Garantie | CI baut `symbiot-core` für `riscv32imc-unknown-none-elf` (ESP32-C3) | M0 |
| Bauen (CI) | GitHub Actions: fmt · clippy · test · WASM-UI · Server-Image | M0 |
| Releasen | Tag `v*` → `tauri-action` → Windows-Installer (+ Linux/macOS) als GitHub Release | M1 |
| Container | Server-Image → `ghcr.io/schulzoli/symbiot-server:{main,sha,version}` | M1 |
| Deployen | Komodo-Stack `symbiot` auf dem Heimserver, **Auto-Update** zieht `:main` | M1 |
| Erreichbar | `tailscale serve` → `https://heimserver.example.ts.net:<port>` (nur Tailnet) | M1 |
| Betreiben | `/health`, `/ledger/verify`, Daten-Volume unter `/opt/stacks/symbiot` | M1 |
| Beobachten | JSON-Logs → **Dozzle**-Alarm, Container-Status → **Beszel** → Alarm-Raum in Matrix | M1 |
| Rückkanal | Fehler und Ideen als GitHub Issue, später direkt aus der App | M3 |

## Meilensteine

### M0 — Fundament
- Cargo-Workspace mit `crates/symbiot-core` (`no_std`), `apps/server` (`/health`), `apps/ui` (Leptos-Startseite).
  `apps/desktop` (Tauri) kommt mit M1, wenn es etwas zum Einpacken gibt.
- CI `ci.yml`: fmt, clippy, Tests, `no_std`-Build des Kerns, WASM-Build der UI.
- Lizenz (MIT), README, diese Dokumente, Issue-Vorlagen, Milestones M0–M6 auf GitHub.

> **⛳ Checkpoint M0:** Ein Push auf `main` macht die CI grün. Ein absichtlich kaputter Commit macht sie rot.

### M1 — Walking Skeleton *(F1 + F2, minimal, durch alle Stationen)*
- Kern: `Node`, `LedgerEntry`, `append()`, `verify()` in `no_std`.
- Store: SQLite-Projektion der Knoten.
- Server: `GET /health`, `POST/GET /nodes`, `GET /ledger/verify`, liefert die UI aus.
- UI (Leptos): Knoten anlegen, Liste, Knopf „Kette prüfen“.
- Desktop: Tauri-Hülle um dieselbe UI, lokale SQLite, eigener Schlüssel.
- `image.yml` → GHCR · `release.yml` → Windows-Installer · Komodo-Stack auf dem Heimserver · Dozzle- und Beszel-Alarme.

> **⛳ Checkpoint M1:** Du installierst die Desktop-App aus einem GitHub Release, legst ein Ziel an und siehst „Kette ok“.
> Gleichzeitig läuft die Web-Version auf dem Heimserver über Tailscale. Ein Commit auf `main` landet ohne Handgriff auf dem Heimserver.
> Ein absichtlich erzeugter Fehler erscheint als Alarm im Matrix-Raum.

### M2 — Verknüpfen *(F3)*
Kanten, Graph-Ansicht, erster Playwright-E2E-Test gegen die Web-Version.

### M3 — Mitdenken *(F4)*
KI-Vorschlag über LiteLLM mit Zitaten. Annehmen und Ablehnen werden im Ledger festgehalten. Feedback-Knopf erzeugt ein Issue.

> **⛳ Checkpoint M3:** Der erste geschlossene Kreislauf: beobachten → Ziel → Beleg → KI schlägt vor → du entscheidest.
> Eine Woche lang selbst benutzen (Dogfooding).

### M4 — Synchron *(F5)*
Desktop ↔ Heimserver tauschen Ketten aus. Offline schreiben, später abgleichen.

### M5 — Überall *(F6)*
Android-Build aus derselben Codebasis, signiertes APK im Release. iOS sobald ein Mac oder macOS-Runner bereitsteht.

### M6 — Sensor *(F7)*
ESP32-C3 mit `symbiot-core` signiert einen Beleg (z. B. Raumklima oder Schritte) und schickt ihn an den Heimserver.
Beweis, dass „ein Gerät = eine Kette“ bis hinunter zum Mikrocontroller trägt.

## Lebensgraph *(E14)*

Nach M6 oder parallel dazu. Zwei getrennte Schichten: Integration holt Rohdaten herein, Aufbereitung leitet daraus
Dinge ab (ELT). Die Quellen sind Beispiele, jede ist ein eigener Adapter.

### M7 — Integration *(Rohdaten laden)*
Rohdaten-Format als Vertrag zwischen den Schichten. Erste Import-Adapter mit eigener Kette, z. B. Chat-Export,
mbox und Git-Historie. Idempotenter Import, Datei-Speicher nach Inhalts-Hash, Zeitleiste und Volltextsuche über alle
Belege.

> **⛳ Checkpoint M7:** Ein Jahr Chat und Git ist importiert, ein zweiter Import ändert nichts, die Kette prüft „ok“,
> die Suche findet eine bestimmte Nachricht in unter einer Sekunde.

### M8 — Aufbereitung *(Dinge ableiten)*
Aufbereiter als eigene Geräte mit Herkunft an jedem Ergebnis. Personen, Gespräche, Aussagen und Themen,
lokal herausgezogen. Zusammenführen und Trennen als Einträge. Fragen in Worten, Antworten mit geprüften Quellen.
Neu aufbereiten mit einer neuen Version, ohne die Quellen erneut abzufragen.

> **⛳ Checkpoint M8:** „Wer hat X gut gefunden?“ und „Seit wann arbeite ich an Z?“ liefern richtige Antworten mit
> Zitat und Link zum Original-Beleg.

### M9 — Zeit *(Massendaten und Aktivitäten)*
Live-Adapter für Quellen mit vielen Werten, z. B. Web-Verlauf, Spiele und Smartphone (setzt M5 voraus), dazu Fotos
und Screenshots mit OCR. Massendaten in Parquet, Auswertung mit DuckDB. Kategorien als angenommene Regeln. Ansicht
„Wie verbringe ich meine Zeit“.

### M10 — Gespräche *(Angesicht zu Angesicht)*
Sprachnotizen und Aufnahmen mit Einwilligung, lokal mit Whisper transkribiert. Vorschläge „Warst du mit Y
zusammen?“ aus Kalender, Standort und Fotos.

## Arbeitsweise

- **Trunk-based:** kurze Branches, Pull Request mit grüner CI, Squash-Merge nach `main`.
- **Versionen:** SemVer, Tag `vX.Y.Z` löst den Release aus. `0.x` bis M3.
- **Definition of Done** pro Feature: Tests grün · in der App sichtbar · auf dem Heimserver deployed · im Ledger nachvollziehbar.
- **Regel aus der Vision:** Die KI schlägt vor, der Mensch entscheidet. Nichts wird automatisch ausgeführt.
