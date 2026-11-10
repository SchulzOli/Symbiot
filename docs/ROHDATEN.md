# Rohdaten — der Vertrag zwischen Integration und Aufbereitung

Stand: 2026-11-08 · gehört zu [E14](ENTSCHEIDUNGEN.md) · Code: `symbiot_core::evidence`, `symbiot_store::Store::record_evidence`

Ein Adapter macht aus dem, was seine Quelle liefert, **Belege** (`Evidence`) und deutet nichts. Ein Aufbereiter
liest nur Belege und fragt nie eine Quelle. Dieses Format ist das Einzige, was beide voneinander kennen.

## Ein Beleg

| Feld | Inhalt | Beispiel |
|---|---|---|
| `id` | stabil und eindeutig innerhalb der Herkunft, mit der Herkunft als Präfix | `telegram:chat1/4711`, `mail:<abc@example.org>`, `git:github.com/a/b@3f2c…` |
| `kind` | was es ist, als Begriff aus einem Vokabular | `schema:Message`, `schema:EmailMessage`, `schema:SoftwareSourceCode`, `schema:Photograph`, `schema:WebPage` |
| `at_ms` | wann es in der Quelle geschah (ms seit 1970), nicht wann es geladen wurde | `1730000000000` |
| `actors` | Personen, wie die Quelle sie nennt: Kennung mit Schema, Rolle und angezeigter Name (leer, wenn unbekannt) | `mailto:a@b.de` als `from` mit Name „Anna B.“, `tel:+4930…` als `to` |
| `text` | der Text, unverändert; leer, wenn es keinen gibt | |
| `fields` | weitere Felder der Quelle als Paare | `("subject", …)`, `("thread", …)`, `("url", …)` |
| `files` | angehängte Dateien: SHA-256, Name, MIME-Typ, Größe | Die Datei selbst liegt im Datei-Speicher. |

**Personen-Schemata:** `tel:`, `mailto:`, `telegram:`, `whatsapp:`, `github:`, `steam:`, und `name:` nur dann, wenn
die Quelle nichts anderes hat. Personen zusammenzuführen ist Aufgabe der Aufbereitung, nicht des Adapters.

**Rollen:** `from`, `to`, `cc`, `author`, `mention`, `participant`. Rollen und Arten sind Text, damit ein neuer
Adapter etwas ergänzen kann, ohne dass ältere Geräte seine Bündel nicht mehr lesen können.

**Pflicht** (sonst lehnt der Kern ab): `id` und `kind` mit Präfix und höchstens 512 Bytes, `at_ms` größer als 0,
jede Person mit Schema und Rolle.

## In der Kette

- Belege stehen in Einträgen der Art `EvidenceBatch` (Kennung 24) in der Kette des Adapters. Jeder Adapter ist
  ein eigenes Gerät (E7).
- **Kopf:** pro Beleg ein Schlüssel `sha256("symbiot/evidence/v1" ‖ id)`.
- **Inhalt:** pro Beleg ein Teil. Er lässt sich entfernen (E2), der Schlüssel bleibt.
- **Bündel** (E13): höchstens 256 Belege, geschlossen bei 16 KiB Inhalt. Ein einzelner größerer Beleg bekommt ein
  Bündel für sich.
- **Kein Beleg doppelt:** Was schon einen Schlüssel hat, wird nicht noch einmal geladen. Das gilt auch, wenn ein
  anderer Adapter ihn geladen hat oder sein Inhalt gelöscht wurde. Gelöschte Rohdaten kommen beim nächsten Import
  also nicht zurück.

## Im Speicher

Die Tabelle `evidence` ist eine Projektion wie die Knoten und lässt sich aus den Ketten neu bauen. Belege werden
keine Knoten im Graphen. Aus ihnen leitet die Aufbereitung Dinge ab (M8), und der Mensch macht daraus Bedeutung.

```rust
let mut adapter = Store::open_dir(dir)?;          // eigener Schlüssel = eigenes Gerät
adapter.certify_self(&words, "telegram-import")?; // gehört zur Person (E5)
let r = adapter.record_evidence(items)?;          // Recorded { added, known, entries }
let neueste = home.evidence(&EvidenceQuery { limit: Some(50), ..Default::default() })?;
```

Über den normalen Abgleich (M4) erreicht die Kette des Adapters die anderen Geräte, und deren Tabelle füllt sich.

## Importieren (`symbiot-import`)

Die ersten Adapter lesen einmalige Exporte. Jeder Lauf ist idempotent: Ein zweiter Import derselben Datei fügt nur
Neues hinzu.

| Quelle | Export | Kennung | Art |
|---|---|---|---|
| `telegram` | Telegram Desktop → Chat oder Konto exportieren → Format **JSON** (`result.json`) | `telegram:<chat>/<nachricht>` | `schema:Message` |
| `whatsapp` | Chat → Mehr → **Chat exportieren** (`.txt`) | `whatsapp:<chat>/<fingerabdruck>-<n>` | `schema:Message` |
| `mbox` | mbox-Datei, z. B. aus Google Takeout oder Thunderbird | `mail:<Message-ID>` | `schema:EmailMessage` |
| `git` | ein lokales Repository, alle Branches | `git:<origin>@<commit>` | `symbiot:Commit` |

```bash
cargo build --release -p symbiot-import
export SYMBIOT_WORDS="…24 Wörter…"   # nur beim ersten Lauf, bestätigt den Adapter als Gerät der Person
symbiot-import whatsapp "WhatsApp-Chat mit Anna.txt" --utc-offset 60 --abgleich https://heimserver.example.ts.net:8449
symbiot-import telegram ~/Downloads/Telegram/result.json --abgleich https://heimserver.example.ts.net:8449
symbiot-import mbox ~/Takeout/Mail/Alle.mbox
symbiot-import git ~/code/symbiot
```

- `--daten ORDNER`: Schlüssel und Kette des Adapters, Standard `symbiot-import/<quelle>`. Jede Quelle ist ein
  eigenes Gerät (E7). Diesen Ordner aufheben, sonst erzeugt der nächste Lauf einen neuen Adapter.
- `--abgleich URL`: Sendet nur die eigene Kette an den Heimserver und holt nichts.
- `--utc-offset MINUTEN`: WhatsApp exportiert Ortszeit ohne Zeitzone. Für MEZ 60, für MESZ 120.
- `--chat NAME`: Name des WhatsApp-Chats, Standard ist der Dateiname. Der Name geht in die Kennung ein, also bei
  jedem Import desselben Chats gleich lassen.
- WhatsApp hat keine Nachrichten-Kennungen. Der Fingerabdruck aus Zeit, Absender und Text bleibt bei einem neuen
  Export desselben Chats gleich.
- **Anhänge** kommen in den Datei-Speicher: Telegram-Fotos und -Dateien neben `result.json`, WhatsApp-Medien neben
  der `.txt` (Export „mit Medien“), Anhänge aus Mails. Nicht exportierte Medien bleiben ein Name im Feld
  `attachment`. Dateien über 512 MB werden ausgelassen.

## Dateien

- **Ort:** `files/<zwei Zeichen>/<sha256>` neben der Datenbank. Jede Datei liegt nur einmal da, wie oft sie auch
  vorkommt. Beim Lesen wird sie gegen ihren Hash geprüft.
- **Abgleich:** Nach den Ketten tauschen Gerät und Heimserver die Dateien aus, die ihre Rohdaten nennen, die
  andere Seite aber noch nicht hat. Ketten gehen in Stücken zu 500 Einträgen. Der Heimserver nimmt nur Dateien
  an, auf die Rohdaten verweisen, und prüft den Hash.
- **Anzeigen:** Fotos (PNG, JPEG, GIF, WebP) zeigt die Zeitleiste direkt an, andere Dateien als Link zum
  Herunterladen. Der Server liefert alle anderen Typen nur als Download aus (`Content-Security-Policy: sandbox`),
  damit ein Anhang von außen nie als Seite der App laufen kann. In der Desktop-App erscheinen Fotos bis 8 MB.
- **Löschen (E2):** `prune_files` entfernt Dateien, auf die keine Rohdaten mit Inhalt mehr verweisen.

## Ansehen und suchen

In der App zeigt der Reiter **Zeitleiste** die Rohdaten nach Tagen, die neuesten zuerst. Die Suche findet Wörter
im Text, in Namen und Kennungen der Personen und in den Feldern (Betreff, Chat, Repository). Jedes Wort muss
vorkommen, Wortanfänge genügen. Die Filter-Knöpfe beschränken die Liste auf eine Herkunft.

- Server: `GET /api/evidence?q=&origin=&kind=&from=&to=&limit=&offset=` und `GET /api/evidence/origins`
- Speicher: `evidence_fts` (SQLite FTS5, ohne eigene Kopie des Texts), Schema 6

## Personen (Aufbereitung, #31)

Aus den Kennungen der Rohdaten werden Personen. Der Reiter **Personen** zeigt sie mit ihren Kennungen, Quellen und
der Zahl der Belege. „Zeitleiste zeigen“ öffnet alles von dieser Person über alle Quellen.

- **Regeln** (deterministisch, auf jedem Gerät gleich, `symbiot_core::identity::normalize`): Mail-Adressen in
  Kleinschreibung, Telefonnummern mit `00` werden zu `+`, Leerzeichen und Striche fallen weg, die
  GitHub-Noreply-Adresse wird zum GitHub-Konto. Was danach gleich ist, ist eine Kennung.
- **Vorschläge:** Personen mit gleichem Namen oder gleichem Vornamen unter verschiedenen Kennungen. Sie werden
  berechnet und nicht gespeichert.
- **Entscheidungen** sind Einträge in der Kette des Menschen (Art 27, `DecideIdentity`): *Zusammenführen*,
  *Trennen* (eine Kennung wird wieder eigene Person), *Nicht dieselbe* (wird nicht mehr vorgeschlagen). Die
  Kennungen stehen im entfernbaren Inhalt (E2). Spätere Entscheidungen heben frühere auf, nichts wird
  überschrieben. Entscheidungen gleichen ab wie alles andere.
- Personen sind keine Knoten im Graphen. Wer zu einem Ziel gehört, entscheidet weiter der Mensch.
- Aufbereiter als eigene Geräte (E14) kommen mit den ersten nicht-deterministischen Ergebnissen (Aussagen,
  Themen, #32). Personen brauchen sie nicht: Regeln lassen sich jederzeit neu rechnen, und alles andere entscheidet
  der Mensch.

## Aus der App importieren (#44)

Unter **Quellen** → *Importieren* eine Exportdatei wählen: WhatsApp-Chat (`.txt`), Telegram (`result.json`) oder
Mails (`mbox`). Das geht auch auf dem Handy, z. B. direkt nach „Chat exportieren“ (ohne Medien). Die Datei wird auf
dem Gerät gelesen und als Rohdaten in dessen eigene Kette geladen; ein zweiter Import derselben Datei fügt nichts
hinzu. Für WhatsApp gilt die Zeitzone des Geräts. Server: `POST /api/import?source=…&name=…&chat=…&utc_offset=…`
mit der Datei als Body (bis 512 MB).

## Quellen: was genutzt wird (#44)

Der Reiter **Quellen** (Taste 5) zeigt jede Quelle als Weg der Daten: Adapter → Quelle → genutzt. Eine Quelle ist
die Herkunft der Rohdaten (der Teil der Id vor `:`, z. B. `whatsapp`, `mail`, `git`). Pro Quelle entscheidet der
Mensch:

- **Pausieren:** Die Rohdaten bleiben, werden aber nirgends genutzt (Zeitleiste, Suche, Personen, KI).
- **Auslassen:** einzelne Chats oder Repositories (Feld `chat` bzw. `repo` des Belegs).
- **Zeitraum:** nur die letzten N Tage.
- **Widerrufen:** Die Rohdaten, die bis dahin geladen waren, werden auf allen Geräten gelöscht (E2): aus der
  Kette, wo ein ganzes Bündel zur Quelle gehört, sonst aus der Ansicht; nicht mehr verwendete Dateien ebenso. Die
  Schlüssel bleiben, dieselben Belege werden also nicht erneut geladen. Eine spätere Freigabe lässt neue Daten zu.

Die Entscheidungen sind Einträge in der Kette (Art 17 `GrantSource`, Art 18 `RevokeSource`); was ausgelassen wird,
steht im entfernbaren Inhalt. Die neueste Entscheidung gilt, egal in welcher Reihenfolge sie beim Abgleich
ankommt. Adapter laden weiter; was nicht genutzt wird, wird nur nicht gezeigt. Der Assistent kann dieselben
Entscheidungen vorschlagen (E16), ausgeführt werden sie erst nach Bestätigung.

## Noch nicht enthalten

- Einzelne Belege löschen (das Entfernen eines Teils statt des ganzen Bündels). Kommt mit „Aufgeräumt“ (E13).
- Pro Quelle festlegen, ob Rohdaten das Gerät verlassen dürfen (#44).
- Import aus der App lädt keine Medien neben dem Export (Fotos eines WhatsApp-Exports); dafür weiter
  `symbiot-import` mit dem Ordner. Git braucht den Ordner des Repositorys und geht nur über `symbiot-import`.
- Live-Adapter (M9).
- Der Abgleich wird mit der Menge der Rohdaten langsamer (PLAN H3).
- Der Import aus der App lädt in die Kette des Geräts, an dem man sitzt, nicht in eine eigene Adapter-Kette wie
  `symbiot-import` (Abweichung von E7 und E13, PLAN H6).
- Dateien sind wie die Datenbank noch unverschlüsselt (E6, #22).
- Jedes Gerät lädt beim Abgleich alle fehlenden Dateien. Eine Auswahl (z. B. Fotos nur auf dem Heimserver) fehlt.
