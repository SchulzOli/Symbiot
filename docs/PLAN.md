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
| Deployen | Komodo-Stack `symbiot` auf heimserver, **Auto-Update** zieht `:main` | M1 |
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
- `image.yml` → GHCR · `release.yml` → Windows-Installer · Komodo-Stack auf heimserver · Dozzle- und Beszel-Alarme.

> **⛳ Checkpoint M1:** Du installierst die Desktop-App aus einem GitHub Release, legst ein Ziel an und siehst „Kette ok“.
> Gleichzeitig läuft die Web-Version auf heimserver über Tailscale. Ein Commit auf `main` landet ohne Handgriff auf heimserver.
> Ein absichtlich erzeugter Fehler erscheint als Alarm im Matrix-Raum.

### M2 — Verknüpfen *(F3)*
Kanten, Graph-Ansicht, erster Playwright-E2E-Test gegen die Web-Version.

### M3 — Mitdenken *(F4)*
KI-Vorschlag über LiteLLM mit Zitaten. Annehmen und Ablehnen werden im Ledger festgehalten. Feedback-Knopf erzeugt ein Issue.

> **⛳ Checkpoint M3:** Der erste geschlossene Kreislauf: beobachten → Ziel → Beleg → KI schlägt vor → du entscheidest.
> Eine Woche lang selbst benutzen (Dogfooding).

### M4 — Synchron *(F5)*
Desktop ↔ heimserver tauschen Ketten aus. Offline schreiben, später abgleichen.

### M5 — Überall *(F6)*
Android-Build aus derselben Codebasis, signiertes APK im Release. iOS sobald ein Mac oder macOS-Runner bereitsteht.

### M6 — Sensor *(F7)*
ESP32-C3 mit `symbiot-core` signiert einen Beleg (z. B. Raumklima oder Schritte) und schickt ihn an heimserver.
Beweis, dass „ein Gerät = eine Kette“ bis hinunter zum Mikrocontroller trägt.

## Lebensgraph *(Vorschlag, E14)*

Nach M6 oder parallel dazu. Die Reihenfolge folgt den Fragen aus E14: zuerst die Quellen mit den meisten
Personen, Gesprächen und Themen.

### M7 — Erinnern *(Importe und Beleg-Schicht)*
Einmalige Importe als Adapter mit eigener Kette: Telegram-Export (JSON), WhatsApp-Chat-Export, Mail (mbox oder
IMAP), GitHub-Historie. Blob-Speicher für Anhänge. Zeitleiste und Volltextsuche über alle Belege.

> **⛳ Checkpoint M7:** Ein Jahr Telegram und GitHub ist importiert, die Kette prüft „ok“, die Suche findet eine
> bestimmte Nachricht in unter einer Sekunde.

### M8 — Wer und was *(Dinge-Schicht)*
Personen, Gespräche, Aussagen und Themen aus den Belegen, lokal herausgezogen. Zusammenführen und Trennen als
Einträge. Fragen in Worten, Antworten mit geprüften Quellen.

> **⛳ Checkpoint M8:** „Wer hat X gut gefunden?“ und „Seit wann arbeite ich an Z?“ liefern richtige Antworten mit
> Zitat und Link zur Originalnachricht.

### M9 — Zeit *(Aktivitäten)*
Live-Adapter für Web-Verlauf, Steam und Android (App-Nutzung, Standort, setzt M5 voraus), dazu Fotos und
Screenshots mit OCR. Kategorien als angenommene Regeln. Ansicht „Wie verbringe ich meine Zeit“.

### M10 — Gespräche *(Angesicht zu Angesicht)*
Sprachnotizen und Aufnahmen mit Einwilligung, lokal mit Whisper transkribiert. Vorschläge „Warst du mit Anna
zusammen?“ aus Kalender, Standort und Fotos.

## Arbeitsweise

- **Trunk-based:** kurze Branches, Pull Request mit grüner CI, Squash-Merge nach `main`.
- **Versionen:** SemVer, Tag `vX.Y.Z` löst den Release aus. `0.x` bis M3.
- **Definition of Done** pro Feature: Tests grün · in der App sichtbar · auf heimserver deployed · im Ledger nachvollziehbar.
- **Regel aus der Vision:** Die KI schlägt vor, der Mensch entscheidet. Nichts wird automatisch ausgeführt.
