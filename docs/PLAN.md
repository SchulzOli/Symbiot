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
| M1 Walking Skeleton | fertig (v0.1.0) | Alarme im Matrix-Raum (H7) |
| M2 Verknüpfen | fertig (v0.2.0) | – |
| M3 Mitdenken | fertig (v0.2.0) | Checkpoint: eine Woche selbst benutzen (#20) |
| M4 Synchron | fertig (v0.3.0) | Checkpoints und Zeugen (#21) |
| M5 Überall | in v0.4.0, APK baut in der CI | Release mit signiertem APK, Probe am echten Handy (#23) |
| H Härtung | H1–H4, H6a, H5a (Heimserver), H5b umgesetzt | H5a in den Apps, H5c, H6b (Entscheidung), H7 |
| M6 Sensor | nicht begonnen | alles (#24) |
| M7 Integration | in v0.4.0: Format, vier Import-Adapter, Zeitleiste, Suche, Dateien, Quellen-Verwaltung, Import aus der App | Checkpoint mit echten Daten |
| M8 Aufbereitung | Personen in v0.4.0 | Aussagen, Themen, Fragen in Worten (#32) |
| E15 Oberfläche, E16 Assistent | in v0.4.0 | Vorschläge im Canvas, MCP (#25) |

Die Version auf `main` ist 0.4.0. Der Heimserver zieht `main` von selbst; den Tag `v0.4.0` für Installer und APK
setzt du (siehe H7).

## Reihenfolge ab jetzt *(E18)*

In wenigen Tagen waren M5, M7, das halbe M8, E15 und E16 entstanden. Liegen geblieben waren Zusagen aus den
Entscheidungen: wirklich löschen (E2), nicht exportieren (E3), verschlüsseln (E6), Backup (E5) und ein Abgleich,
der mit der Datenmenge nicht langsamer wird. Die Härtung hat das nachgeholt; was davon noch offen ist, steht unten
bei H.

1. **Heimserver verschlüsseln und Daten anschließen.** Schlüssel eintragen
   ([BETRIEB.md](BETRIEB.md#verschlüsselung-h5-e6)), dann importieren. Das ist zugleich der Checkpoint M7.
2. **Release v0.4.0** mit signiertem APK, F6 am echten Handy abnehmen (H7).
3. **Checkpoints nachholen:** M3 (eine Woche selbst benutzen) und M7 (ein echtes Jahr Chat und Git).
4. **Rest der Härtung:** Schlüsselbund in den Apps (H5a), Adapter ohne Wörter (H6b, nach deiner Entscheidung).
5. **M8 fertig**, beginnend mit einer Messung, ob das lokale Modell Aussagen zuverlässig herauszieht.
6. **MCP-Server** (#25) auf dem Aktionskatalog aus E16.
7. **M9**, erst nach einer Messung, ob SQLite für die Auswertungen reicht.
8. **M10**, dann **M6**. Der Sensor beantwortet keine der Fragen aus E14; die `no_std`-Garantie hält die CI auch so.

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

### H — Härtung *(E18 · weitgehend umgesetzt am 2026-11-10)*

Was die Entscheidungen zusagen, muss stimmen, bevor mehr Daten hereinkommen. H1 bis H3 waren mit Experimenten
nachgewiesen, die übrigen aus dem Code gelesen. Jedes Paket ist ein Pull Request mit einem Test, der die Zusage
prüft. Offen sind: der Schlüsselbund in den Apps (H5a), die Backup-Datei (H5c), Adapter ohne Wörter (H6b) und was
bei H7 bei dir liegt.

#### H1 — Löschen löscht alles *(E2, #47 · umgesetzt)*
**War:** Der Text eines gelöschten Knotens blieb in der Kette: im Vorschlag des Assistenten (Art 28), im
abgelehnten Vorschlag (Art 5) und in einer Bearbeitung, die ein anderes Gerät nach dem Löschen schrieb (Art 2).
- Der Kern hat eine Regel „dieser Eintrag trägt Inhalt über diese Knoten“ (`LedgerEntry::about`). Sie gilt für alle
  Arten mit Inhalt.
- Gelöschte Knoten bleiben als **Grabstein** bekannt. Ein Eintrag über einen gelöschten Knoten, der später ankommt
  oder später einsortiert wird, verliert seinen Inhalt sofort, statt übersprungen liegen zu bleiben.
- `DecideSuggestion` nennt im Kopf die Einträge, die es ausgeführt hat. So steht fest, welche Knoten ein Vorschlag
  angelegt hat. Einträge im alten Format bleiben lesbar.
- **Abgenommen durch:** `crates/symbiot-store/tests/zusagen.rs`: ein Property-Test über drei Geräte, alle Wege und
  beliebige Reihenfolgen, dazu der Test auf die Datenbankdatei. Ohne die Regel werden beide rot.

#### H2 — „Nicht exportierbar“ bleibt auf dem Gerät *(E3, #47 · umgesetzt)*
**War:** Entstand der Knoten über den Assistenten, ging sein Text mit dem Vorschlag (Art 28) an die anderen Geräte.
- Der Abgleich benutzt dieselbe Regel wie H1: Inhalt jedes Eintrags, der über einen nicht exportierbaren Knoten
  spricht, bleibt zu Hause. Das gilt auch, wenn der Knoten nur ausgeblendet wurde (nicht löschbar).
- **Abgenommen durch:** `not_exportable_text_stays_home_whatever_made_it` in `zusagen.rs`.

#### H3 — Abgleich hängt nicht an der Datenmenge *(E14, #48 · umgesetzt)*
**War:** Jeder Empfang prüfte die ganze Kette von vorn, lud alle Ketten samt Rohdaten in den Arbeitsspeicher und
baute die Projektion neu. Ein einzelner neuer Eintrag kostete 0,2 s bei 50 000 Nachrichten und 0,5 s bei 200 000.
- Neue Einträge werden nur gegen die bekannte Spitze der Kette geprüft (`verify_after`). „Kette prüfen“ bleibt die
  volle Prüfung.
- `ledger` kennt Art und Zeit jedes Eintrags, `chain_tip` die Spitze jeder Kette (Schema 11). Neuaufbau und
  Vertrauen lesen nur Einträge des Graphen bzw. Bestätigungen und nie den Inhalt von Rohdaten-Bündeln. Ein Bündel
  geht direkt in seine Tabelle; nur Einträge des Graphen bauen die Projektion neu.
- Ketten gehen in beiden Richtungen in Seiten zu 500 Einträgen.
- Der Server erledigt Import, Empfang und Seiten außerhalb der async-Threads, `/health` antwortet währenddessen.
- **Gemessen:** derselbe einzelne Eintrag kostet jetzt 0,5 ms bei 50 000 und 2,5 ms bei 200 000 Nachrichten.
- **Abgenommen durch:** `crates/symbiot-store/tests/abgleich.rs` (Kosten bei achtfacher Datenmenge, Seiten,
  Upgrade einer älteren Datenbank).

#### H4 — Web-API nimmt nichts von fremden Seiten *(E12, #49 · umgesetzt)*
**War:** `POST /api/import` prüfte keine Herkunft. Eine fremde Webseite im Browser eines Tailnet-Geräts konnte
Rohdaten in die Kette des Heimservers schreiben, wenn sie die Adresse kannte.
- Jede schreibende Route unter `/api` außer dem signierten Abgleich verlangt den Header `x-symbiot`, den nur die
  eigene Oberfläche schickt. Eine fremde Seite müsste dafür erst fragen (CORS-Preflight), und der Server erlaubt
  das nie. Meldet der Browser eine fremde Herkunft (`Sec-Fetch-Site`), wird die Anfrage ebenfalls abgelehnt.
- Die Web-Version bekommt eine Content-Security-Policy wie die Desktop-App: Skripte nur von hier, das kleine
  Start-Skript der Seite über seinen Hash. Antworten dürfen nicht in fremde Seiten eingebettet werden.
- **Abgenommen durch:** `writes_from_outside_the_interface_are_refused` in `apps/server/tests/api.rs`; die
  E2E-Tests laufen mit der Policy.

#### H6 — Die 24 Wörter bleiben selten *(E5, E7, #50 · H6a umgesetzt, H6b wartet auf deine Entscheidung)*
**War:** Die Web-Oberfläche schickte die Wörter an den Heimserver, der sie auch erzeugte. Jeder Import-Adapter
braucht sie einmal. Der Import aus der App schreibt in die Kette des Menschen-Geräts.
- **H6a (umgesetzt):** Die Wörter entstehen im Browser und werden dort zur Signatur. Der Heimserver bekommt nur die
  fertige Bestätigung oder den fertigen Widerruf und prüft sie; eine Anfrage, die Wörter enthält, lehnt er ab. In
  der Desktop- und Android-App bleiben die Wörter im Prozess der App. Eine Stelle für die Wörter:
  `crates/symbiot-words`.
- **H6b (Vorschlag, braucht deine Entscheidung):** Ein bestätigtes Gerät darf Adapter bestätigen und widerrufen.
  Dann braucht kein Adapter die Wörter, und die App kann pro Quelle einen eigenen Adapter-Schlüssel führen. Bis
  dahin bleibt der Import aus der App in der Kette des Geräts, und `symbiot-import` liest die Wörter einmal aus
  `SYMBIOT_WORDS` (sie bleiben auf dem Rechner, auf dem das Werkzeug läuft).
- **Abgenommen durch:** `e2e/tests/geraete.spec.ts` (keine Anfrage enthält eines der 24 Wörter),
  `person_key_signs_in_the_browser_and_the_words_stay_there` (API), `signed_elsewhere_is_checked_here` (Store).

#### H5 — Verschlüsselt, mit Weg zurück *(E5, E6, #22 · auf dem Heimserver umgesetzt, Apps offen)*
**War:** Datenbank, Dateien und Geräteschlüssel lagen im Klartext. Ein älteres Backup des Heimservers
zurückzuspielen und weiterzuschreiben machte dessen Kette für alle Geräte unbrauchbar. Geräte sendeten nur ihre
eigene Kette; ein neu aufgesetzter Heimserver bekam die Ketten der Adapter nicht zurück.
- **H5a Speicher (Heimserver umgesetzt):** Mit `SYMBIOT_DB_KEY` verschlüsselt SQLCipher die Datenbank, die Dateien
  sind einzeln versiegelt (XChaCha20-Poly1305), der Geräteschlüssel liegt in der Datenbank. Ein unverschlüsselter
  Bestand wird beim ersten Start mit Schlüssel umgestellt: Nur die Ketten werden kopiert, alles andere daraus neu
  gebaut, die alte Datei überschrieben und entfernt. Ohne Schlüssel startet eine verschlüsselte Datenbank nicht.
  Einrichten: [BETRIEB.md](BETRIEB.md#verschlüsselung-h5-e6).
- **H5a Apps (offen):** Desktop und Android bauen ohne SQLCipher, damit die Installer nicht an OpenSSL hängen. Es
  fehlt der Schlüsselbund des Betriebssystems als Ablage. Bis dahin schützt dort die Verschlüsselung des
  Betriebssystems.
- **H5b Weg zurück (umgesetzt):** Geräte senden dem Heimserver jede Kette, die ihm fehlt, nicht nur die eigene.
  Damit ist jedes Gerät eine vollständige Kopie. Ein Heimserver mit neuem Schlüssel wird bestätigt und bekommt von
  einem einzigen Gerät alles wieder. Ablauf: [BETRIEB.md](BETRIEB.md#heimserver-verloren-oder-neu-aufsetzen-h5).
- **H5c Backup-Datei (offen, nachrangig):** alle Ketten und Dateien in einer Datei, verschlüsselt mit einem
  Schlüssel aus den 24 Wörtern. Seit H5b ist jedes Gerät schon eine Kopie; die Datei wäre eine zusätzliche.
- **Abgenommen durch:** `crates/symbiot-store/tests/verschluesselt.rs` (kein Klartext im Datenordner, Umstellung,
  unterbrochene Umstellung), `a_new_home_server_gets_everything_back_from_one_device` in
  `apps/server/tests/sync.rs`, E2E gegen einen verschlüsselten Server.

#### H7 — Release v0.4.0 und Alarme *(M1, M5, #23 · vorbereitet, der Rest liegt bei dir)*
- Version ist 0.4.0. **Offen, bei dir:** Tag `v0.4.0` setzen; vorher die drei Android-Secrets anlegen, sonst
  erscheint der Release ohne APK.
- Probe am echten Handy: Ziel am Handy anlegen, erscheint am PC (F6).
- Dozzle- und Beszel-Alarm einrichten, einmal absichtlich auslösen (Checkpoint M1).
- **Geprüft für den Betrieb:** Ein Bestand, der mit dem Stand vor der Härtung angelegt wurde (Einrichtung, Knoten,
  Bearbeitung, Löschung, Importe, widerrufene Quelle, eigene Adapter-Kette), ist nach dem Update unverändert da;
  der alte Import-Client gleicht weiter ab; die Umstellung auf Verschlüsselung behält alles.

> **⛳ Checkpoint H:** Ein gelöschter Text steht in keiner Kette und in keiner Datei mehr, auf keinem Gerät. Ein
> Abgleich ohne neue Rohdaten dauert bei 200 000 Belegen so lange wie bei 50 000. Die Datenbankdatei enthält keinen
> lesbaren Text. Der Heimserver lässt sich mit neuem Schlüssel neu aufsetzen, und nach einem Abgleich ist alles
> wieder da.

#### Was nur du tun kannst
| Was | Wofür | Wo beschrieben |
|---|---|---|
| Android-Signaturschlüssel erzeugen, drei Secrets anlegen | APK im Release (H7) | [BETRIEB.md](BETRIEB.md#einmalig-einrichten-von-hand-braucht-deine-anmeldungen), Schritt 5 |
| Tag `v0.4.0` setzen | Release (H7) | [BETRIEB.md](BETRIEB.md#release) |
| Dozzle- und Beszel-Alarm einrichten | Checkpoint M1 (H7) | BETRIEB.md, Schritte 3 und 4 |
| Entscheiden: dürfen Geräte Adapter bestätigen? | H6b | E5, E7 |
| Schlüssel für die Datenbank des Heimservers erzeugen und in `.env` eintragen | H5a | [BETRIEB.md](BETRIEB.md#verschlüsselung-h5-e6) |

### Zusagen und ihre Tests

Jede Zusage aus den Entscheidungen bekommt einen Test, der sie über alle Wege prüft. „Offen“ heißt: Der Test fehlt
oder deckt einen Weg nicht ab.

| Zusage | Entscheidung | Test | Stand |
|---|---|---|---|
| Jede Veränderung der Kette wird erkannt | F2 | `any_tampering_is_detected` (Kern) | hält |
| Gelöscht ist gelöscht, auf jedem Gerät | E2 | `crates/symbiot-store/tests/zusagen.rs` (Property-Test über drei Geräte, Test auf die Datei) | hält (H1) |
| Nicht exportierbar verlässt das Gerät nie | E3 | `zusagen.rs`, `apps/server/tests/sync.rs` | hält (H2) |
| Sensibles erreicht kein externes Modell | E4 | `crates/symbiot-ai`, `apps/server/tests/api.rs` | hält für Knoten |
| Nur Geräte der Person gleichen ab, Widerruf gilt | E5 | `apps/server/tests/sync.rs` | hält |
| Die Wörter verlassen das Gerät nicht, an dem sie getippt werden | E5 | `e2e/tests/geraete.spec.ts`, `apps/server/tests/api.rs` | hält (H6a); Adapter brauchen sie noch (H6b) |
| Der Speicher ist verschlüsselt | E6 | `crates/symbiot-store/tests/verschluesselt.rs` | hält auf dem Heimserver mit Schlüssel (H5a); Apps offen |
| Ein verlorener Heimserver kostet keine Daten | E5 | `apps/server/tests/sync.rs` | hält (H5b) |
| Texte von außen sind Daten, nie Auftrag | E12 | – | offen, wird mit M8 Pflicht |
| Fremde Webseiten können nichts schreiben | E12 | `apps/server/tests/api.rs` | hält (H4) |
| Kein Vorschlag ändert Daten ohne Klick | F4, E16 | `api.rs`, `assistent.spec.ts` | hält |
| Ein zweiter Import ändert nichts | E14 | `crates/symbiot-import/tests/cli.rs` | hält |
| Abgleich hängt nicht an der Datenmenge | E14 | `crates/symbiot-store/tests/abgleich.rs` | hält (H3) |

### Danach, in dieser Reihenfolge

#### K — Checkpoints nachholen
- **K1 (M3, #20):** eine Woche selbst benutzen. Jeden Stolperstein als Issue.
- **K2 (M7):** ein echtes Jahr Chat und Git importieren. Abnahme wie im Checkpoint M7, dazu die Dauer eines
  Abgleichs auf Handy und Desktop notieren.

#### M8 — Aufbereitung fertig *(#32)*
1. **Messung zuerst.** Etwa 200 echte Nachrichten, von Hand markiert (Person · Thema · Haltung · Zitat). Die Daten
   bleiben im Datenordner und kommen nie ins Repository. Ein kleines Werkzeug lässt Modelle dagegen laufen und
   zählt richtige und falsche Aussagen. Jedes Zitat muss wörtlich im Beleg stehen. Ergebnis: welches Modell, lokal
   oder gemischt. Das beantwortet die offene Frage aus E14.
2. **KI-Anbindung erneuern.** Strukturierte Ausgabe über LiteLLM (JSON-Schema) statt Suchen nach `{…}` im Text.
   Modell pro Aufgabe einstellbar. Der Sonderfall für `qwen3` wird eine Einstellung.
3. **E12 umsetzen.** Texte von außen stehen im Prompt als gekennzeichnete Daten. Jede Aussage wird gegen ihren
   Beleg geprüft. Testdaten mit eingebetteten Anweisungen gehören in die Tests.
4. **Aufbereiter als Gerät** (E14): eigener Schlüssel, eigene Kette, Herkunft an jedem Ergebnis, neue Version
   ersetzt alte per Eintrag.
5. **Aussagen und Themen**, dann **Fragen in Worten** mit Quellen, deren Zitate Symbiot prüft.

#### E16 fertig und MCP *(#25)*
- Der Assistent bekommt, was seine Aktionen brauchen: Personen und Quellen im Kontext, oder die Aktionen
  verschwinden aus seinem Katalog.
- Vorschläge von außen landen in einem Eingang und warten auf die Entscheidung des Menschen.
- MCP-Server: lesen (Knoten, Suche in Rohdaten) und vorschlagen (derselbe Katalog). Nie ausführen.

#### M9 — Zeit
1. **Messung zuerst.** Fünf Millionen künstliche Belege in SQLite: Zeitleiste, Suche und „Zeit pro Kategorie und
   Woche“. Nur wenn das zu langsam ist, kommen Parquet und DuckDB dazu.
2. Erster Live-Adapter: Web-Verlauf (liest eine lokale Datei, braucht keine API).
3. Kategorien als Regeln wie in E4, Ansicht „Wie verbringe ich meine Zeit“.

#### E13 — Checkpoints, Zeugen, Aufräumen *(#21)*
Nach H3 braucht es Checkpoints nur noch für den schnellen Start neuer Geräte. Zeugen schützen vor einer gekürzten
oder neu geschriebenen Kette eines gestohlenen Geräts. Einzelne Teile eines Eintrags entfernen (ein Beleg aus einem
Bündel, eine Aktion aus einem Vorschlag) gehört hierher.

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
