# Erste Features

Reihenfolge = Bauweise. F1 und F2 bilden zusammen das Walking Skeleton (M1).

## F1 — Erfassen *(M1)*
Ich halte fest, was mich beschäftigt.
- Knoten anlegen mit Art (`Ziel`, `Spannung`, `Reflexion`, `Beleg`, `Person`, `Wert`), Titel und Text.
- Liste aller Knoten, filterbar nach Art.
- **Fertig, wenn:** Ein Knoten überlebt einen Neustart der App. Er erscheint in der Desktop-App und in der Web-Version auf dem Heimserver (getrennte Daten bis M4).

## F2 — Beweisbar *(M1)*
Nichts wird heimlich verändert.
- Jede Änderung ist ein signierter Eintrag in der Kette des Geräts. Seit Protokoll 2 (E2, E13) besteht ein Eintrag
  aus einem signierten Kopf und entfernbarem, gesalzenem Inhalt:
  `hash = sha256("symbiot/entry/v2" ‖ postcard(protocol, kopf))` plus Ed25519-Signatur; der Kopf trägt den
  Merkle-Root (RFC 9162) über den Inhalt. Fehlender Inhalt ist gültig, veränderter wird erkannt.
- Knopf „Kette prüfen“ zeigt `ok · N Einträge` oder die genaue Bruchstelle.
- Bearbeiten heißt neuer Eintrag. Die alte Version bleibt sichtbar.
- **Fertig, wenn:** Ein Property-Test beweist, dass jedes veränderte Byte, jeder vertauschte und jeder fehlende Eintrag erkannt wird.

### Löschen und Richtlinien *(E2, E3)*
- Löschen ist ein Löschmarker (`Op::DeleteNode`, `Op::Unlink`). Der Knoten und seine Verbindungen verschwinden.
- Pro Knoten „löschbar“ und „exportierbar“ (`Op::SetPolicy`, Standard: beides ja), beim Anlegen und später
  änderbar. Löschbar: Titel und Text aller Versionen werden weggeräumt (`secure_delete`, WAL-Checkpoint).
  Nicht löschbar: nur ausgeblendet. Nicht exportierbar: geht nie zu einem externen Modell.
- **Fertig, wenn:** Nach dem Löschen steht der Text nicht mehr in der Datenbankdatei und „Kette prüfen“ ist ok.
  ✅ `crates/symbiot-store` (`deleting_removes_the_content_from_the_file`), `apps/server/tests/api.rs`,
  `e2e/tests/verknuepfen.spec.ts`.
- **H1, H2 (2026-11-10):** Dasselbe gilt für jeden Weg, auf dem ein Knoten entsteht oder sich ändert: von Hand,
  als angenommener oder abgelehnter Vorschlag, über den Assistenten, und für Bearbeitungen, die ein anderes Gerät
  nach dem Löschen schrieb. Gelöschte Knoten bleiben als Grabstein bekannt; was später über sie ankommt, verliert
  seinen Inhalt sofort. Was über einen nicht exportierbaren Knoten spricht, verlässt das Gerät nicht.
  ✅ `crates/symbiot-store/tests/zusagen.rs` (Property-Test über drei Geräte und beliebige Reihenfolgen).

## F3 — Verknüpfen *(M2)*
Zusammenhänge sichtbar machen.
- Kanten: `stützt`, `widerspricht`, `bezieht sich auf`, `abgeleitet von`. Jede Kante ist ein signierter Eintrag (`Op::Link`).
  Abgelehnt werden Verbindungen mit sich selbst, zu unbekannten Knoten und doppelte Verbindungen.
- Graph-Ansicht (SVG in Leptos), Klick auf Knoten zeigt seine Herkunft im Ledger.
- Detailansicht: bearbeiten (neue Version), Verbindungen ansehen und anlegen, Verlauf aller Versionen aus dem Ledger.
- **Fertig, wenn:** Ein E2E-Test legt Ziel und Beleg an, verbindet sie und findet die Kante in der Ansicht.
  ✅ `e2e/tests/verknuepfen.spec.ts`, läuft in der CI.

## F4 — Mitdenken *(M3)*
Die KI schlägt den nächsten Schritt vor, ich entscheide.
- Für ein Ziel: „Nächsten Schritt vorschlagen“ → Anfrage über LiteLLM (Standard: lokales `qwen3:8b`).
- Der Vorschlag nennt seine **Quellen** (Knoten-IDs). Ein Vorschlag ohne Quellen wird verworfen.
- Annehmen oder Ablehnen wird als Eintrag ins Ledger geschrieben. Angenommen = neuer Knoten.
- **Fertig, wenn:** Kein Vorschlag ändert Daten ohne deinen Klick (Test). Bei sensiblen Knoten verlässt nichts das Heimnetz.
  ✅ Der Vorschlag lebt nur in der Oberfläche, bis du klickst. Annehmen (`Op::AcceptProposal`) und Ablehnen
  (`Op::RejectProposal`) sind signierte Einträge mit dem ganzen Vorschlag; angenommen = neuer Knoten mit Kante
  „ist abgeleitet von“ zum Ziel. Der Kern lehnt Vorschläge ohne oder mit unbekannten Quellen ab.
  Sensibel markieren geht mit einem Klick an jedem Knoten (`Op::MarkSensitive`, E4); für ein externes Modell
  bleiben sensible Knoten aus dem Kontext, ein sensibles Ziel wird gar nicht gesendet.
  Tests: `apps/server/tests/api.rs` (Fake-KI), `crates/symbiot-ai`, `e2e/tests/mitdenken.spec.ts` (Schein-LLM).
  ✅ E4: Regeln für Sensibles. Das lokale Modell schlägt Regeln vor (Name + Stichwörter); welche Knoten sie
  treffen, rechnet Symbiot selbst nach. Erst „Annehmen“ schreibt `Op::AddSensitivityRule` in die Kette, danach gilt
  die Regel überall gleich, auch für spätere Knoten. Markierung von Hand gilt vor jeder Regel.
  Tests: `crates/symbiot-core` (`sensitivity_rules`), `apps/server/tests/api.rs`, `e2e/tests/sensibel.spec.ts`.

## F5 — Synchron *(M4)*
Ein Gedächtnis auf allen Geräten.
- Geräte gehören zu einem **Personen-Schlüssel** (E5, 24 Wörter). Er bestätigt jedes Gerät mit einem signierten
  Eintrag in dessen Kette (`Op::CertifyDevice`) und widerruft verlorene (`Op::RevokeDevice`). Der Heimserver nimmt nur Ketten
  von Geräten derselben Person an; ein neues Gerät stellt sich mit seinem ersten Push selbst vor.
- Austausch über den Heimserver (Stern): `GET /api/sync/heads`, `GET /api/sync/chains/{gerät}?after=N`,
  `POST /api/sync/chains/{gerät}`. Jede Anfrage ist mit dem Geräteschlüssel signiert. Fremde Einträge werden vor
  dem Speichern geprüft (Kette, Anschluss, Person). Eine Verzweigung wird als Beweis gespeichert und abgelehnt.
- Alle Ketten werden deterministisch gemischt (`replay`): Die neueste Version gewinnt, eine gleichzeitige Änderung
  zeigt „gleichzeitig geändert“, bis „Passt so“ oder eine neue Version kommt. Löschen gewinnt.
- Inhalt nicht exportierbarer Knoten verlässt das Gerät nicht; solche Knoten erscheinen woanders nicht.
- **Fertig, wenn:** Offline am PC angelegte Knoten nach dem Wiederverbinden auf dem Heimserver und auf dem zweiten Gerät erscheinen. Eine manipulierte Kette wird abgelehnt.
  ✅ `apps/server/tests/sync.rs` (Heimserver als echter Server, zwei Geräte), `e2e/tests/geraete.spec.ts` (Einrichtung).
  Nicht in M4: Checkpoints und `Witness`-Einträge (E13), Verschlüsselung (E6).
- **H5b (2026-11-10):** Geräte senden dem Heimserver auch die Ketten anderer Geräte und Adapter, die ihm fehlen.
  Ein neu aufgesetzter Heimserver hat nach einem Abgleich mit einem Gerät alles wieder.
  ✅ `a_new_home_server_gets_everything_back_from_one_device` in `apps/server/tests/sync.rs`.
- **H6a (2026-11-10):** Im Browser entstehen die Wörter in der Seite und werden dort zur Signatur
  (`POST /api/person/certify` und `/revoke` nehmen nur die fertige Signatur). ✅ `e2e/tests/geraete.spec.ts`.
- **H3 (2026-11-10):** Ein Empfang prüft nur die neuen Einträge gegen die Spitze der Kette und liest keine
  Rohdaten. Ketten gehen in beiden Richtungen in Seiten zu 500 Einträgen
  (`GET /api/sync/chains/{gerät}?after=N&limit=500`). ✅ `crates/symbiot-store/tests/abgleich.rs`.

## F6 — Überall *(M5)*
Symbiot auf dem Handy.
- Android-Build aus derselben Codebasis, im Release als signiertes APK.
- **Fertig, wenn:** Ein Ziel am Handy angelegt wird und am PC erscheint.
  Stand: Die App baut in der CI (`apps/desktop/android.sh`), jeder Pull Request liefert ein Test-APK. Offen: Release
  mit dem signierten APK und die Probe am echten Handy (PLAN H7).

## F7 — Sensor *(M6)*
Belege, die kein Mensch eintippen muss.
- ESP32-C3 mit `symbiot-core`, eigener Schlüssel, sendet signierte Messwerte an den Heimserver.
- **Fertig, wenn:** Ein Messwert als Beleg-Knoten erscheint und `Kette prüfen` die Kette des Sensors bestätigt.

## Lebensgraph und Assistent *(M7, M8, E16)*
Rohdaten, Importe, Zeitleiste, Suche, Personen und Quellen stehen in [ROHDATEN.md](ROHDATEN.md), der Assistent in
[E16](ENTSCHEIDUNGEN.md).

## Später (Phase WIR / ALLE)
Personen-Graph und Vertrauensräume (GraphFamiliy, TrustInHumanity) · Einwilligungen pro Knoten und Person · Konsens-Werkzeug (IchWillNichtStreiten) · gemeinsamer Ledger (AGORA).
