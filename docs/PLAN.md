# Plan — den ganzen Dev-Lifecycle so schnell wie möglich abdecken

Stand: 2026-11-10 · Reihenfolge nach [E18](ENTSCHEIDUNGEN.md): erst härten, dann weiterbauen.

**Prinzip: Walking Skeleton.** Zuerst wandert *ein einziges, winziges Feature* durch **alle** Stationen:
planen → coden → testen → bauen → releasen → deployen → betreiben → beobachten.
Erst danach wird das Skelett dicker. Jeder Meilenstein endet mit einem **Checkpoint**, den du abnimmst.

## Der Lifecycle und womit wir jede Station abdecken

| Station | Werkzeug | Ab |
|---|---|---|
| Planen | GitHub Issues + Milestones, Entscheidungen in `docs/` | M0 |
| Coden | Cargo-Workspace, `rustfmt`, `clippy` | M0 |
| Testen | `cargo test`, `proptest` für den Ledger, axum-Integrationstests, Playwright-E2E mit axe-core | M0 |
| No-std-Garantie | CI baut `symbiot-core` für `riscv32imc-unknown-none-elf` (ESP32-C3) | M0 |
| Bauen (CI) | GitHub Actions: fmt · clippy · test · WASM-UI · Desktop · Android-APK · Server-Image | M0 |
| Releasen | Tag `v*` → `tauri-action` → Installer für Windows, Linux, macOS und Android-APK als GitHub Release | M1 |
| Container | Server-Image → `ghcr.io/schulzoli/symbiot-server:{main,sha,version}` | M1 |
| Deployen | Komodo-Stack `symbiot` auf dem Heimserver, **Auto-Update** zieht `:main` | M1 |
| Erreichbar | `tailscale serve` → `https://heimserver.example.ts.net:<port>` (nur Tailnet) | M1 |
| Betreiben | `/health`, `/ledger/verify`, Daten-Volume unter `/opt/stacks/symbiot` | M1 |
| Beobachten | JSON-Logs und Healthcheck sind da. Die Alarme (Dozzle, Beszel → Matrix) sind noch nicht eingerichtet | M1 |
| Rückkanal | Feedback-Link in der App öffnet ein GitHub Issue | M3 |

## Stand

| Meilenstein | Stand | Offen |
|---|---|---|
| M0 Fundament | fertig | – |
| M1 Walking Skeleton | fertig (v0.1.0) | Alarme im Matrix-Raum (Teil des Checkpoints, bisher weggelassen) |
| M2 Verknüpfen | fertig (v0.2.0) | – |
| M3 Mitdenken | fertig (v0.2.0) | Checkpoint: eine Woche selbst benutzen (#20) |
| M4 Synchron | fertig (v0.3.0) | Checkpoints und Zeugen (#21), Verschlüsselung (#22) |
| M5 Überall | auf `main`, APK baut in der CI | Release mit signiertem APK, Probe am echten Handy (#23) |
| M6 Sensor | nicht begonnen | alles (#24) |
| M7 Integration | auf `main`: Format, vier Import-Adapter, Zeitleiste, Suche, Dateien, Quellen-Verwaltung, Import aus der App | Checkpoint mit echten Daten |
| M8 Aufbereitung | Personen auf `main` | Aussagen, Themen, Fragen in Worten (#32) |
| E15 Oberfläche, E16 Assistent | auf `main` | Vorschläge im Canvas, MCP (#25) |

Seit v0.3.0 gab es keinen Release. Alles ab M5 läuft nur auf `main` und auf dem Heimserver.

## Reihenfolge ab jetzt *(E18)*

In wenigen Tagen sind M5, M7, das halbe M8, E15 und E16 entstanden. Liegen geblieben sind Zusagen aus den
Entscheidungen: wirklich löschen (E2), nicht exportieren (E3), verschlüsseln (E6), Backup (E5) und ein Abgleich,
der mit der Datenmenge nicht langsamer wird. Seit den Importen liegen echte Nachrichten und Mails anderer Menschen
unverschlüsselt auf Heimserver und Handy. Deshalb:

1. **H — Härtung.** Keine neue Datenquelle und keine neue Aufbereitung, solange H offen ist.
2. **Release v0.4.0** mit signiertem APK, F6 am echten Handy abnehmen.
3. **Checkpoints nachholen:** M3 (eine Woche selbst benutzen) und M7 (ein echtes Jahr Chat und Git).
4. **M8 fertig**, beginnend mit einer Messung, ob das lokale Modell Aussagen zuverlässig herauszieht.
5. **MCP-Server** (#25) auf dem Aktionskatalog aus E16.
6. **M9**, erst nach einer Messung, ob SQLite für die Auswertungen reicht.
7. **M10**, dann **M6**. Der Sensor beantwortet keine der Fragen aus E14; die `no_std`-Garantie hält die CI auch so.

## Meilensteine

### M0 — Fundament *(fertig)*
- Cargo-Workspace mit `crates/symbiot-core` (`no_std`), `apps/server` (`/health`), `apps/ui` (Leptos-Startseite).
  `apps/desktop` (Tauri) kommt mit M1, wenn es etwas zum Einpacken gibt.
- CI `ci.yml`: fmt, clippy, Tests, `no_std`-Build des Kerns, WASM-Build der UI.
- Lizenz (MIT), README, diese Dokumente, Issue-Vorlagen, Milestones M0–M6 auf GitHub.

> **⛳ Checkpoint M0:** Ein Push auf `main` macht die CI grün. Ein absichtlich kaputter Commit macht sie rot.

### M1 — Walking Skeleton *(F1 + F2, minimal, durch alle Stationen · fertig)*
- Kern: `Node`, `LedgerEntry`, `append()`, `verify()` in `no_std`.
- Store: SQLite-Projektion der Knoten.
- Server: `GET /health`, `POST/GET /nodes`, `GET /ledger/verify`, liefert die UI aus.
- UI (Leptos): Knoten anlegen, Liste, Knopf „Kette prüfen“.
- Desktop: Tauri-Hülle um dieselbe UI, lokale SQLite, eigener Schlüssel.
- `image.yml` → GHCR · `release.yml` → Windows-Installer · Komodo-Stack auf dem Heimserver · Dozzle- und Beszel-Alarme.

> **⛳ Checkpoint M1:** Du installierst die Desktop-App aus einem GitHub Release, legst ein Ziel an und siehst „Kette ok“.
> Gleichzeitig läuft die Web-Version auf dem Heimserver über Tailscale. Ein Commit auf `main` landet ohne Handgriff auf dem Heimserver.
> Ein absichtlich erzeugter Fehler erscheint als Alarm im Matrix-Raum. *(Der Alarm ist noch offen, siehe H7.)*

### M2 — Verknüpfen *(F3 · fertig)*
Kanten, Graph-Ansicht, erster Playwright-E2E-Test gegen die Web-Version.

### M3 — Mitdenken *(F4 · fertig, Checkpoint offen)*
KI-Vorschlag über LiteLLM mit Zitaten. Annehmen und Ablehnen werden im Ledger festgehalten. Feedback-Knopf erzeugt ein Issue.

> **⛳ Checkpoint M3:** Der erste geschlossene Kreislauf: beobachten → Ziel → Beleg → KI schlägt vor → du entscheidest.
> Eine Woche lang selbst benutzen (Dogfooding).

### M4 — Synchron *(F5 · fertig)*
Desktop ↔ Heimserver tauschen Ketten aus. Offline schreiben, später abgleichen.

### M5 — Überall *(F6 · auf `main`, Abnahme offen)*
Android-Build aus derselben Codebasis, signiertes APK im Release. iOS sobald ein Mac oder macOS-Runner bereitsteht.

### H — Härtung *(E18 · als Nächstes)*

Was die Entscheidungen zusagen, muss stimmen, bevor mehr Daten hereinkommen. Die ersten drei Punkte sind mit
Experimenten nachgewiesen, die übrigen aus dem Code gelesen.

| Nr. | Paket | Zusage | Heute |
|---|---|---|---|
| H1 | Löschen löscht alles | E2 | Der Text eines gelöschten Knotens bleibt in der Kette: im Vorschlag des Assistenten (Art 28), im abgelehnten Vorschlag (Art 5) und in einer Bearbeitung, die ein anderes Gerät nach dem Löschen schrieb (Art 2). |
| H2 | „Nicht exportierbar“ bleibt da | E3 | Entstand der Knoten über den Assistenten, geht sein Text mit dem Vorschlag (Art 28) an die anderen Geräte. |
| H3 | Abgleich hängt nicht an der Datenmenge | E14 | Jeder Empfang prüft die ganze Kette von vorn, lädt alle Ketten samt Rohdaten in den Arbeitsspeicher und baut die Projektion neu. Ein einzelner neuer Eintrag kostet 0,2 s bei 50 000 Nachrichten und 0,5 s bei 200 000. |
| H4 | Web-API nimmt nichts von fremden Seiten | E12 | `POST /api/import` prüft keine Herkunft. Eine fremde Webseite im Browser eines Tailnet-Geräts kann Rohdaten in die Kette des Heimservers schreiben, wenn sie die Adresse kennt. |
| H5 | Verschlüsselt, mit Backup und Weg zurück | E5, E6 | Datenbank, Dateien und Geräteschlüssel liegen im Klartext (#22). Ein älteres Backup des Heimservers zurückzuspielen und weiterzuschreiben macht dessen Kette für alle Geräte unbrauchbar. |
| H6 | Die 24 Wörter bleiben selten | E5, E7 | Jeder Import-Adapter braucht die Wörter, und die Web-Oberfläche schickt sie an den Heimserver. Der Import aus der App schreibt in die Kette des Menschen-Geräts statt in eine eigene Adapter-Kette. |
| H7 | Release v0.4.0 und Alarme | M1, M5 | Kein Release seit v0.3.0, also kein APK. Die Alarme aus dem Checkpoint M1 fehlen. |

> **⛳ Checkpoint H:** Ein gelöschter Text steht in keiner Kette und in keiner Datei mehr, auf keinem Gerät. Ein
> Abgleich ohne neue Rohdaten dauert bei 200 000 Belegen so lange wie bei 50 000. Die Datenbankdatei enthält keinen
> lesbaren Text. Der Heimserver lässt sich mit neuem Schlüssel neu aufsetzen, und nach einem Abgleich ist alles
> wieder da.

### M6 — Sensor *(F7 · zurückgestellt bis nach M10)*
ESP32-C3 mit `symbiot-core` signiert einen Beleg (z. B. Raumklima oder Schritte) und schickt ihn an den Heimserver.
Beweis, dass „ein Gerät = eine Kette“ bis hinunter zum Mikrocontroller trägt.

## Lebensgraph *(E14)*

Zwei getrennte Schichten: Integration holt Rohdaten herein, Aufbereitung leitet daraus Dinge ab (ELT). Die Quellen
sind Beispiele, jede ist ein eigener Adapter.

### M7 — Integration *(Rohdaten laden · auf `main`, Checkpoint offen)*
Rohdaten-Format als Vertrag zwischen den Schichten. Erste Import-Adapter mit eigener Kette, z. B. Chat-Export,
mbox und Git-Historie. Idempotenter Import, Datei-Speicher nach Inhalts-Hash, Zeitleiste und Volltextsuche über alle
Belege.

> **⛳ Checkpoint M7:** Ein Jahr Chat und Git ist importiert, ein zweiter Import ändert nichts, die Kette prüft „ok“,
> die Suche findet eine bestimmte Nachricht in unter einer Sekunde.

### M8 — Aufbereitung *(Dinge ableiten · Personen auf `main`)*
Aufbereiter als eigene Geräte mit Herkunft an jedem Ergebnis. Personen, Gespräche, Aussagen und Themen,
lokal herausgezogen. Zusammenführen und Trennen als Einträge. Fragen in Worten, Antworten mit geprüften Quellen.
Neu aufbereiten mit einer neuen Version, ohne die Quellen erneut abzufragen.

Vor dem Bauen steht eine Messung: echte Nachrichten, von Hand markiert, dagegen das lokale Modell. Sie beantwortet
die offene Frage aus E14, ob ein kleines lokales Modell Aussagen zuverlässig herauszieht. Mit Texten von außen im
Prompt wird E12 Pflicht: Eine Nachricht ist Quelle, nie Auftrag.

> **⛳ Checkpoint M8:** „Wer hat X gut gefunden?“ und „Seit wann arbeite ich an Z?“ liefern richtige Antworten mit
> Zitat und Link zum Original-Beleg.

### M9 — Zeit *(Massendaten und Aktivitäten)*
Live-Adapter für Quellen mit vielen Werten, z. B. Web-Verlauf, Spiele und Smartphone (setzt M5 voraus), dazu Fotos
und Screenshots mit OCR. Massendaten in Parquet, Auswertung mit DuckDB, falls eine Messung zeigt, dass SQLite nicht
reicht. Kategorien als angenommene Regeln. Ansicht „Wie verbringe ich meine Zeit“. Setzt H3 voraus.

### M10 — Gespräche *(Angesicht zu Angesicht)*
Sprachnotizen und Aufnahmen mit Einwilligung, lokal mit Whisper transkribiert. Vorschläge „Warst du mit Y
zusammen?“ aus Kalender, Standort und Fotos.

## Arbeitsweise

- **Trunk-based:** kurze Branches, Pull Request mit grüner CI, Squash-Merge nach `main`.
- **Versionen:** SemVer, Tag `vX.Y.Z` löst den Release aus. `0.x`, solange sich das Kettenformat noch ändern kann.
- **Definition of Done** pro Feature: Tests grün · in der App sichtbar · auf dem Heimserver deployed · im Ledger nachvollziehbar.
- **Zusagen sind Tests.** Was eine Entscheidung verspricht (gelöscht ist gelöscht, Sensibles bleibt im Heimnetz),
  bekommt einen Test, der genau das prüft, über alle Wege, auf denen Daten entstehen.
- **Regel aus der Vision:** Die KI schlägt vor, der Mensch entscheidet. Nichts wird automatisch ausgeführt.
