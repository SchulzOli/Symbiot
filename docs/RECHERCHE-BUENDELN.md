# Recherche: Bündeln, Aufbewahren, Verdichten und Checkpoints

Stand: 2026-10-30 · Recherche, noch keine Entscheidung · ergänzt [ENTSCHEIDUNGEN.md](ENTSCHEIDUNGEN.md) (E2, E3,
E10) und [RECHERCHE-ADAPTER.md](RECHERCHE-ADAPTER.md) (c1, c3, c8)

**Frage:** Welche Regeln für Bündelung, Aufbewahrung, Verdichtung und Snapshots braucht Symbiot, damit die Ketten
klein bleiben und sich schnell synchronisieren und prüfen lassen, ohne dass die Prüfbarkeit oder das Versprechen
„nichts wird heimlich verändert“ leidet?

**Wie belastbar das ist:** Aus der Recherche-Umgebung waren mehrere Primärseiten nicht abrufbar: c2sp.org,
home-assistant.io, prometheus.io, bitcoincore.org, eips.ethereum.org und inkandswitch.com. Für C2SP, Prometheus,
Home Assistant (Recorder), Bitcoin und EIP-4444 habe ich deshalb die Quelltexte der Spezifikationen und Dokus auf
GitHub gelesen. Das sind dieselben Texte, aber eventuell nicht der neueste veröffentlichte Stand. Für andere Themen
stützen sich die Angaben auf Suchergebnisse. Sekundärquellen sind mit *(sekundär)* markiert. Was ich nicht prüfen
konnte, steht als *ungeprüft* im Text. Die Zahlen für Symbiot (Größen, Zeiten) sind **meine Schätzungen**. Sie
müssen gemessen werden.

---

## (a) Kurzfassung

1. **Die Hash-Kette bleibt die Wahrheit, sie bekommt nur einen „Kopf“ und einen „Inhalt“.** Das ist E2,
   verallgemeinert: Signiert wird der Kopf. Der Kopf enthält einen Merkle-Root über die Teile des Inhalts. Dadurch
   lassen sich einzelne Teile eines Eintrags (ein Kalender-Termin, die Rohwerte einer Minute) entfernen, und
   `verify()` bestätigt die Kette trotzdem. Dieselbe Idee nutzen Certificate Transparency (Blätter sind Hashes),
   Hypercore (`clear()` entfernt Blöcke, der signierte Baum bleibt) und Ethereum (Historie fällt weg, Köpfe und
   Checkpoints bleiben).
2. **Bündeln geschieht nur bei Maschinen, nicht bei Menschen.** Adapter und Sensoren schreiben *einen*
   Bündel-Eintrag pro Zeitfenster (Standard: 60 s für Adapter, 5 min für Sensoren). Menschen-Geräte schreiben
   weiter jede Handlung einzeln. Ohne Bündelung erzeugt ein Sensor über 500 000 Einträge im Jahr, mit 5-min-Bündeln
   rund 105 000.
3. **Drei Aufbewahrungsklassen:** *Roh* (alles), *Zusammenfassung* (Min/Max/Mittel/Anzahl, wie die
   Langzeitstatistik von Home Assistant) und *Nur Beweis* (Kopf und Hashes). Die Fristen legt der Mensch pro
   Adapter in einem signierten Eintrag fest. Der Adapter schlägt Standardwerte vor, und die Regeln werden
   deterministisch angewandt.
4. **Signierte Checkpoints in der eigenen Kette** (alle 1024 Einträge oder alle 24 h) mit einem Merkle-Root nach
   RFC 9162 über alle Eintrags-Hashes. Damit lässt sich ab einem Checkpoint prüfen, und einzelne alte Einträge lassen
   sich mit O(log n) Hashes beweisen. Das Format ist dasselbe wie bei C2SP `tlog-checkpoint` und COSE Receipts
   (RFC 9942). Ein Export als echtes Transparenzlog bleibt also offen.
5. **Die eigenen Geräte bezeugen einander** (Witnessing nach dem Vorbild von C2SP `tlog-witness` und Sigsum). heimserver
   merkt sich für jede Kette den letzten gesehenen Stand und schlägt Alarm, wenn eine Kette gekürzt wird oder sich
   verzweigt. Das macht „nichts wird heimlich verändert“ auch zwischen Geräten prüfbar.
6. **Was nie entfernt wird:** Köpfe, Hashes und Signaturen, Löschmarker, Richtlinien, Grants und Revokes,
   Entscheidungen über Vorschläge, Checkpoints, Gerätebestätigungen sowie Belege, auf die ein Knoten verweist.
   Wegräumen ist nie ein stiller Vorgang. Es folgt einer signierten Regel und wird in einem täglichen
   `Aufgeräumt`-Eintrag festgehalten.
7. **Vor M4 nötig:** die Trennung in Kopf und Inhalt mit Salz, den Inhalts-Root als Merkle-Baum, Einträge ohne
   Inhalt im Speicher- und Sync-Format, Domänentrennung beim Hashen und feste Op-Kennungen. Bündel-, Checkpoint- und
   Zeugen-Einträge können später kommen, wenn ihr Platz im Format reserviert ist.

---

## (b) Vergleich der Techniken

Legende Passung: ●●● passt direkt · ●●○ passt mit Anpassung · ●○○ nur als Idee.
„Ein Schreiber“ meint die Passung zu Symbiots Modell (ein Schlüssel pro Kette, signierte Hash-Kette).

| Technik | Was sie löst | Kosten | Ein Schreiber | no_std / ESP32 |
|---|---|---|---|---|
| **C2SP tlog-tiles + Checkpoints** (Static CT API, Sunlight, Tessera) | Log als unveränderliche „Kacheln“ zu 256 Hashes. Ein kleiner signierter Checkpoint (Herkunft, Größe, Root) ist das einzige veränderliche Objekt. Gut zu cachen, Prüfung über Beweise | ein Merkle-Baum neben der Kette, O(log n) Zustand beim Anhängen | ●●● Ein Log hat genau einen Signierer, wie bei uns | ●●○ Baum-Zustand ~1 KB (Spitzen), Checkpoint signieren geht; Kacheln ausliefern macht heimserver |
| **Witnessing / Cosignaturen** (C2SP tlog-witness, Sigsum) | Verhindert *Split View* und Rückspulen: Zeugen cosignieren nur, was mit dem letzten Stand konsistent ist | Zeuge speichert pro Log nur den letzten Stand | ●●● Eigene Geräte und heimserver sind natürliche Zeugen | ●○○ Sensor ist kein Zeuge, nur Bezeugter |
| **Merkle Mountain Range** (MMRIVER-Entwurf, ePrint 2025/234) | Anhängbarer Akkumulator, alte Knoten ändern sich nie, Inklusionsbeweise O(log n) | 32 B × Höhe Zustand | ●●○ Gleichwertig zum RFC-9162-Baum; der ist durch C2SP und COSE Receipts besser unterstützt | ●●● sehr klein |
| **Trillian → Tessera** | Bibliothek für Tile-Logs, Bündelung beim Einreihen (`WithBatching`), Witness-Policy | Bündel gehen bei einem Absturz vor dem Einreihen verloren | ●●○ Vorbild für „im Speicher sammeln, dann einmal signieren“ | — (Go, Server) |
| **IETF SCITT + COSE Receipts** (RFC 9943, RFC 9942) | Standardformat für „Eintrag X ist im Log mit Root R“ (Inklusion, Konsistenz) | CBOR/COSE-Abhängigkeit | ●●○ Für Belege nach außen (Phase WIR/ALLE), nicht für den Kern | ●○○ COSE auf ESP32 möglich, aber unnötig |
| **Hypercore / Pear** | Signierter Merkle-Baum über Blöcke, `clear()` entfernt lokale Blöcke, spärliche Replikation | Baum statt reiner Kette | ●●● Ein Schreiber pro Core, genau Symbiots Idee | ●○○ JS/Bare |
| **Secure Scuttlebutt** | Warnung: Feeds wuchsen ohne Grenze, der erste Sync dauerte lange, Löschen fehlte. Später kamen Metafeeds (Teil-Feeds), Partial Replication und Compaction in ssb-db2 | Löschen erst spät und mühsam nachgerüstet | ●●● (gleiches Modell) – Lehre: Löschen und Teil-Replikation **von Anfang an** einbauen | — |
| **PZP (ehem. PPPPP)** | Nachfolger von SSB mit löschbaren Inhalten und aktiver Garbage Collection | DAG statt Kette | ●●○ bestätigt die Richtung „Kopf bleibt, Inhalt geht“ *(sekundär)* | — |
| **Nostr** | NIP-09 Löschanfrage (nur der Autor, Relais *dürfen* folgen), NIP-40 Ablaufzeit, ersetzbare Events | keine Kette, also keine Lückenprüfung | ●○○ Lehre: Löschen ist eine *Bitte* an fremde Kopien *(sekundär)* | ●●● |
| **Keyhive / Beelay (Sedimentree)** | Ältere Historie wird in immer größere Blöcke („Schichten“) gepackt. Die Grenzen ergeben sich aus führenden Nullen im Hash, Peers tauschen zuerst eine Zusammenfassung | Alpha, für DAGs gedacht | ●●○ Idee „Zusammenfassung zuerst, dann fehlende Blöcke“ passt zum Sync | ●○○ |
| **Automerge 3** | Komprimiertes Spaltenformat auch im Speicher (über 10× weniger RAM) | Historie bleibt vollständig, eine offizielle Kürzung habe ich nicht gefunden | ●○○ Lehre: kompakte Kodierung schlägt Löschen bei Größe | — |
| **Git Packfiles / Shallow Clone** | Delta-Kompression; `--depth N` behandelt die Grenze als Wurzel (`.git/shallow`) | Tiefe Historie danach nicht prüfbar, Push aus flachen Klonen unklar | ●●○ Vorbild für „Archiv-Schnitt“ mit ehrlicher Grenze | — |
| **Iroh / iroh-blobs (BAO)** | Inhaltsadressierte Blobs, in 16-KiB-Gruppen verifiziert gestreamt | zusätzliche Abhängigkeit | ●●○ Transport für große Bündel-Inhalte (Fotos, Dateien) | ●○○ |
| **Prometheus / InfluxDB** | Zeitfenster-Blöcke (2 h), Retention 15 Tage Standard, ~1–2 Byte/Messwert; Downsampling als eigener Job in eine zweite Datenbank mit längerer Frist | Downsampling muss vor Ablauf der Rohdaten laufen | ●●○ Vorbild für Klassen „roh kurz, verdichtet lang“ | ●●● |
| **Home Assistant Recorder** | Zustände 10 Tage (`purge_keep_days`), Kurzzeitstatistik alle 5 min, Langzeitstatistik stündlich (Mittel/Min/Max bzw. Summe), **nie automatisch gelöscht** | — | ●●● Vorbild für Sensor-Standards; der HA-Adapter kann die 5-min-Statistik direkt lesen | — |
| **Event-Sourcing-Snapshots** | Zustand alle N Ereignisse speichern, danach nur den Rest abspielen; Snapshots mit Schema-Version | Snapshot ist Cache, nicht Wahrheit | ●●● Projektion in SQLite ist genau das | — |
| **Bitcoin Pruning / assumeutxo** | Pruned Node prüft alles, behält aber nur ≥ 550 MB Blöcke (≥ 288 Blöcke). assumeutxo startet von einem Snapshot, dessen Hash im Code steht, und prüft die Historie im Hintergrund nach | Pruned Nodes liefern keine alten Blöcke aus | ●●● Vorbild für „vom Checkpoint starten, im Hintergrund nachprüfen“ | — |
| **Ethereum EIP-4444** | Clients löschen alte Körper und Belege. Teilweise umgesetzt seit Juli 2025 (Daten vor dem Merge, 300–500 GB weniger), die rollierende Variante ist noch offen | Neue Knoten starten per Checkpoint-Sync; alte Daten nur noch über Archive | ●●○ Lehre: Wer Historie wegwirft, muss sagen, wo sie noch liegt | — |
| **Bündeln auf Mikrocontrollern** (eigene Ableitung) | ein Eintrag pro Fenster mit Werten oder Merkle-Root, Puffer im Flash bei fehlendem Netz | Latenz = Fenster | ●●● | ●●● Signatur pro Fenster statt pro Wert |

**Wichtigste Lehren:** (1) SSB zeigt, was passiert, wenn Löschen und Teil-Replikation erst nachträglich kommen. (2)
CT, Hypercore und Bitcoin zeigen, dass man Inhalte wegräumen kann, solange Köpfe und Hashes bleiben. (3) Home
Assistant und Prometheus zeigen, welche Fristen sich im Alltag bewährt haben: roh etwa 10–15 Tage, verdichtet
unbegrenzt. (4) Witnessing ist der Standardweg, um Rückspulen und Verzweigen bei einem Log mit nur einem Schreiber
zu erkennen.

---

## (c) Empfohlene Regeln für Symbiot

### c1. Eintrag = Kopf + Inhalt (verallgemeinertes E2)

```
Kopf  (nie entfernt; geht in den Hash ein)
  protocol, author, seq, prev, ts_ms, kind (feste u16-Kennung), refs (Knoten-/Kanten-IDs, Fenster)
  content: { root: Hash, parts: u32, len: u32 }      // leer, wenn der Eintrag keinen Inhalt hat
Inhalt (entfernbar, auch teilweise)
  part[i] = salt_i (16 B) ‖ postcard(Teil i)
hash = sha256("symbiot/entry/v2" ‖ prev ‖ postcard(Kopf))
sig  = ed25519(hash)
content.root = Merkle-Root nach RFC 9162 über leaf_i = sha256(0x00 ‖ part[i])
```

- Bei nur einem Teil (der Normalfall bei Menschen-Einträgen) ist `root = sha256(0x00 ‖ salt ‖ inhalt)`. Das ist
  E2 mit Domänentrennung und Salz.
- **Salz ist Pflicht.** Ohne Salz bleibt nach dem Wegräumen `sha256(inhalt)` übrig, und kurze Inhalte („ja“,
  „Termin bei Dr. X“) lassen sich durch Raten bestätigen. Das Salz wird mit dem Inhalt weggeräumt. Danach ist der
  Hash eine nicht ratbare Verpflichtung.
- **Ein Eintrag ohne (oder mit unvollständigem) Inhalt ist gültig**, solange Kopf, Kette und Signatur stimmen.
  Vorhandene Teile werden gegen `content.root` geprüft. Einzelne Teile eines Bündels lassen sich mit einem
  Inklusionsbeweis gegen den Root zeigen.

### c2. Wann wird gebündelt?

| Schreiber | Bündel-Eintrag | Abschluss des Fensters (was zuerst eintritt) |
|---|---|---|
| Menschen-Gerät | **nie**: jede Handlung ein Eintrag | — |
| Adapter, Messwerte (HA) | `EvidenceBatch` | **60 s** · **256 Teile** · **16 KiB** Inhalt |
| Adapter, Ereignisse (Kalender, Matrix) | `EvidenceBatch` mit einem Teil pro Termin/Nachricht | pro Abruf bzw. **60 s** pro Raum · 256 Teile · 16 KiB; **nur Änderungen** (Cursor, Hash pro Termin) |
| ESP32-Sensor | `SensorBatch`, Werte direkt im Inhalt (ein Teil) | **5 min** (z. B. 10 Werte im 30-s-Takt); bei Akku-Geräten bis 15 min |

- **Deadband und Herzschlag** für Messwerte: Ein Wert wird nur aufgenommen, wenn er sich spürbar ändert (z. B.
  ≥ 0,1 °C), mindestens aber einmal pro Fenster. Für numerische HA-Sensoren liest der Adapter besser die
  5-min-Kurzzeitstatistik von HA (Mittel/Min/Max) statt jedes Zustandswechsels.
- **Kein leeres Bündel.** Ohne neue Daten entsteht kein Eintrag. Lebenszeichen zeigen die Checkpoints (c4).
- **Teil 0 ist die Zusammenfassung** (Anzahl, Min/Max/Mittel/Letzter je Messgröße). Teile 1…n sind die Rohdaten.
  Beim ESP32 genügt anfangs ein einziger Teil mit Rohwerten und Zusammenfassung. Er wird dann als Ganzes
  weggeräumt.
- **Sensor offline:** Der Sensor signiert sofort bei Fensterende, puffert die Einträge im Flash (Ringpuffer) und
  sendet sie nach. Die Reihenfolge sichert `seq`, nicht die Uhrzeit.
- **Schätzung Größe:** Ein Kopf kostet gespeichert etwa 120–200 B, denn `author` steht einmal pro Kette und `prev`
  lässt sich neu berechnen. Ein Sensor mit 5-min-Fenstern kommt so auf etwa 105 000 Einträge und 15–25 MB im Jahr,
  mit 1-min-Fenstern auf rund 525 000 Einträge und 80–120 MB. *Schätzung, messen.* Daher der Standard 5 min statt
  der 60 s aus RECHERCHE-ADAPTER c3.

### c3. Aufbewahrungsklassen und wer sie festlegt

| Klasse | Was bleibt | Typischer Einsatz |
|---|---|---|
| **R-Roh** | alles | Menschen-Einträge, Belege mit Verweis |
| **R-Zusammenfassung** | Kopf + Teil 0 | Sensor- und Messwerte nach Ablauf der Rohfrist |
| **R-Beweis** | nur Kopf (Hashes, Signatur) | Ereignisse ohne Übernahme nach Ablauf; gelöschte Knoten (E2) |
| *(Archiv-Schnitt)* | Checkpoint statt alter Köpfe | nur Maschinen-Ketten, nur auf ausdrücklichen Wunsch (c6) |

**Vorgeschlagene Standards** (der Mensch sieht sie beim Grant und kann sie ändern):

| Quelle | Roh | Zusammenfassung | Kopf |
|---|---|---|---|
| ESP32 / HA-Messwerte | 10 Tage (wie HA `purge_keep_days`) | unbegrenzt (wie HA-Langzeitstatistik) | unbegrenzt |
| Kalender | bis Termin-Ende + 90 Tage | — | unbegrenzt |
| Matrix / Nachrichten | 30 Tage | — | unbegrenzt |
| Menschen-Geräte | bis zur Löschung (E2/E3) | — | unbegrenzt, nie Archiv-Schnitt |

- **Wer entscheidet:** Der Mensch, mit einem signierten Eintrag `SetRetention { adapter, klasse, fristen }` in
  *seiner* Kette (eigener Eintrag oder Teil des Grants). Den Standard schlägt das Adapter-Manifest vor, Symbiot
  zeigt ihn beim Erteilen an. Jede Änderung ist ein neuer Eintrag und im Verlauf sichtbar.
- **Deterministisch:** Ob ein Teil weg darf, folgt allein aus der Kette: aus `SetRetention`, `SetPolicy`, den
  Löschmarkern, Verweisen und Zeitstempeln. Die Regel wird im Kern berechnet, nie von der KI.
- **Verweis hält fest:** Wird ein Beleg-Teil per Klick oder Vertrauen (E10) zum Knoten, verweist der Knoten auf
  `(Kette, seq, Teil-Index, Teil-Hash)`. Dieser Teil fällt dann nicht mehr unter die Frist, sondern unter die
  Richtlinie des Knotens (E3).
- **Verkürzen wirkt sofort:** Eine kürzere Frist räumt auch alte Daten ab. Die Oberfläche sagt das vorher.
- **Verdichten über die Zusammenfassung hinaus** (z. B. stündliche Werte aus 5-min-Bündeln) ist ein *neuer*
  Eintrag `Rollup { quelle: (kette, seq-bereich), werte }` in der Kette des Geräts, das rechnet (heimserver). Solange die
  Rohdaten da sind, lässt er sich nachrechnen. Danach ist er eine Aussage von heimserver, nicht vom Sensor. Die Oberfläche
  zeigt das so an.

### c4. Checkpoints und schnelles Prüfen

- **Signierter Checkpoint in der eigenen Kette:** `Checkpoint { size, tree_root }` alle **1024 Einträge** oder
  **24 h** (nur wenn es neue Einträge gab). `tree_root` ist der RFC-9162-Merkle-Root über die Eintrags-Hashes
  0…size−1. Der Zustand dafür sind O(log n) Hashes, das schafft auch der ESP32 (rund 1 KB).
- **Inkrementell prüfen:** Jedes Gerät merkt sich pro Kette „geprüft bis seq N, Hash H“. Beim Sync prüft es nur
  Neues. Ed25519-Prüfen kostet auf dem Desktop etwa 46 µs *(sekundär, x86-Benchmark)*. 100 000 Einträge brauchen damit rund 5 s, auf dem Handy
  eher das Mehrfache *(ungeprüft, messen)*.
- **„Kette prüfen“ ganz:** Läuft weiter von vorn, im Hintergrund, mit Fortschritt. Für Köpfe ohne Inhalt prüft es
  Kette und Signatur, für vorhandene Inhalte zusätzlich die Roots.
- **Neues Gerät (schneller Start, wie assumeutxo):** Es lädt zuerst die Köpfe ab dem letzten Checkpoint, den ein
  eigenes Gerät bezeugt hat (c5), und ist sofort nutzbar. Ältere Köpfe lädt und prüft es im Hintergrund nach. Bis
  dahin zeigt die Oberfläche „geprüft ab Checkpoint X“.
- **Sync-Seiten zu 256 Einträgen** (wie die Kacheln bei C2SP). Volle Seiten ändern sich nie (außer durch
  Wegräumen von Inhalt, nicht von Köpfen) und lassen sich auf heimserver cachen und komprimieren. Ein späterer Export als
  `tlog-tiles` bleibt so einfach.
- **Snapshots der Projektion** (SQLite) sind nur Cache. Sie tragen die Schema-Version und den Stand
  `(Kette → seq)` und werden nie signiert oder synchronisiert. Bei Zweifel baut man sie aus den Ketten neu auf.

### c5. Zeugen: Rückspulen und Verzweigen erkennen

- Jedes Gerät (vor allem heimserver) speichert pro bekannter Kette den letzten Stand `(seq, hash)`. Eine neue Lieferung
  muss daran anschließen. Zwei verschiedene Einträge mit gleicher `seq` und gültiger Signatur sind ein **Beweis
  für Verzweigung**. Sie werden gespeichert, es gibt einen Alarm, und der Schlüssel wird zum Widerruf
  vorgeschlagen (E5).
- heimserver schreibt höchstens einmal pro Sync-Runde bzw. täglich einen Eintrag `Witness { [(kette, seq, hash)] }` in
  *seine* Kette, also gebündelt. Damit verankern sich die Ketten gegenseitig. Wer Geschichte umschreiben will,
  braucht mehr als einen Schlüssel.
- Das deckt auch die Lücke aus dem Proptest auf, der kommentiert: „Dropping the tip … is caught by sync“.

### c6. Wegräumen, Löschmarker und Sync

1. **Auslöser** sind nur: ein Löschmarker auf einem löschbaren Knoten (E2/E3), eine abgelaufene Frist (c3) oder
   ein nicht exportierbarer Inhalt gegenüber anderen Menschen und Backups (E3/E5).
2. **Ausführung:** Inhalt und Salz in SQLite auf NULL setzen, mit `PRAGMA secure_delete=ON` und anschließendem
   Checkpoint der WAL-Datei. Sonst bleiben Reste in freien Seiten bzw. im WAL.
3. **Nichts heimlich:** Das wegräumende Gerät schreibt täglich einen Eintrag
   `Aufgeräumt { regel: Verweis, bereiche: [(kette, seq-von, seq-bis, teile)] }` in seine Kette. Die Oberfläche
   zeigt am Eintrag: „Inhalt entfernt am … nach Regel …“.
4. **Sync überträgt Köpfe immer, Inhalte nur, wenn erlaubt.** Fehlt beim Empfänger ein Teil, stellt seine eigene
   Projektion den Grund fest: Löschmarker, Frist oder nicht exportierbar. Gibt es keinen Grund, gilt der Teil als
   **„fehlt – nachfordern“** und wird *nicht* als gelöscht angezeigt. So kann ein Sender Inhalte nicht unbemerkt
   zurückhalten.
5. **Der Empfänger räumt beim Sync selbst auf**, sobald er den Löschmarker verarbeitet (E2). Fremde Geräte können
   nicht gezwungen werden. Die Oberfläche sagt das ehrlich (wie Nostr NIP-09: Löschen ist dort eine Bitte).
6. **Nie entfernt werden:** alle Köpfe mit Hash und Signatur; die *Inhalte* von Löschmarkern, `SetPolicy`,
   `SetRetention`, `Grant`/`Revoke`, Entscheidungen über Vorschläge, `Checkpoint`, `Witness`, `Aufgeräumt` und
   Gerätebestätigungen; Belege mit Verweis von einem Knoten. Diese Einträge sind klein und tragen die Prüfbarkeit.
7. **Archiv-Schnitt** (Köpfe vor einem Checkpoint verwerfen, wie Git `--depth` oder EIP-4444) gibt es nur für
   Maschinen-Ketten und nur auf ausdrücklichen Wunsch. Die alten Köpfe wandern zuvor in ein verschlüsseltes Archiv
   (heimserver oder Backup). Danach steht an der Kette: „geprüft ab Checkpoint X, ältere Historie im Archiv Y“.
   Menschen-Ketten nie.

---

## (d) Folgen für das Kettenformat

Heute gilt `hash = sha256(postcard(protocol, author, seq, prev, ts_ms, op))`, und `Op` enthält Titel und Text
direkt (`crates/symbiot-core/src/ledger.rs`). Damit kann man keinen Inhalt entfernen, ohne `verify()` zu brechen.

| Nr. | Änderung | Vor M4? | Warum |
|---|---|---|---|
| F1 | `Op` aufteilen in **Kopf** (Art, IDs) und **Inhalt** (Titel, Text, Werte); der Kopf trägt `content: { root, parts, len }` | **ja** | Kern von E2. Nach M4 liegen Ketten auf mehreren Geräten, dann wird jede Änderung eine Migration |
| F2 | **Salz** pro Teil, **Merkle-Root nach RFC 9162** als Inhalts-Root (ein Teil = Sonderfall) | **ja** | ohne Salz ratbar; Teil-Löschung im Bündel braucht den Baum, und der Root-Begriff muss von Anfang an feststehen |
| F3 | **Domänentrennung** `"symbiot/entry/v2"` im Hash, `PROTOCOL_VERSION = 2` | **ja** | verhindert Verwechslung mit Checkpoint-, Teil- und Baum-Hashes |
| F4 | Speicher- und Sync-Format erlauben **Einträge ohne Inhalt** (`Option`), `verify()` prüft dann nur Kopf, Kette und Signatur | **ja** | sonst lässt sich nichts weggeräumtes synchronisieren |
| F5 | **Feste u16-Kennungen** für Eintragsarten statt Enum-Reihenfolge; Kennungen für `SetPolicy`, `SetRetention`, `DeleteNode`, `Unlink`, `Grant`, `Revoke`, `Checkpoint`, `Witness`, `Aufgeräumt`, `EvidenceBatch`, `SensorBatch`, `Rollup` jetzt reservieren | **ja** | postcard kodiert den Variant-Index. Reihenfolge-Fehler brechen sonst Hashes; unbekannte Arten müssen als „unbekannt, aber gültig signiert“ durchgereicht werden können |
| F6 | `Checkpoint` mit inkrementellem RFC-9162-Baum (`no_std`, O(log n) Zustand) | M4 (mit Sync) | schneller Start, Prüfen ab Checkpoint |
| F7 | `Witness` und Speicherung des „letzten Stands“ pro Kette | M4 | Rückspulen und Verzweigung erkennen |
| F8 | `EvidenceBatch`, `SetRetention`, `Rollup`, `Aufgeräumt` | mit dem ersten Adapter | erst nötig, wenn Maschinen schreiben |
| F9 | `SensorBatch` (ein Teil, Werte + Zusammenfassung) | M6 | Sensor |

**Übergang:** Bis M3 ist Symbiot `0.x`. Die bestehenden Ketten (heimserver, PC) lassen sich einmalig in v2 überführen: Jeder
alte Eintrag wird neu signiert, und der erste v2-Eintrag verweist auf den alten Tip. Das ist billiger als zwei
Formate dauerhaft zu tragen. Weil `NodeId` aus dem Eintrags-Hash abgeleitet wird (`from_entry_hash`), ändern sich
dabei alle IDs. Deshalb sollte die Umstellung **vor** echten Daten aus Adaptern und vor M4 passieren.

---

## (e) Risiken und offene Fragen für den Besitzer

- **Fristen-Standards:** Passen 10 Tage roh und unbegrenzte Zusammenfassung für Messwerte bzw. 30 Tage für
  Nachrichten? Sollen Gesundheitsdaten (E4) kürzere Standards haben?
- **Archiv-Schnitt ja oder nein?** Er spart Platz bei Sensoren. Danach ist die alte Historie aber nur noch aus dem
  Archiv prüfbar. Empfehlung: anbieten, nie automatisch, nie für Menschen-Ketten.
- **Metadaten bleiben sichtbar:** Kopf, Zeitfenster und Teil-Anzahl bleiben auch nach dem Wegräumen. Daran lässt
  sich z. B. ablesen, dass um 23:10 drei Nachrichten kamen. Reicht das, oder soll die Teil-Anzahl aufgefüllt
  werden (Padding)?
- **Backups:** Ältere verschlüsselte Backups enthalten Inhalte, die inzwischen weggeräumt sind. Die Optionen:
  Backups nach Frist rotieren, oder Inhalte mit einem Schlüssel pro Knoten verschlüsseln und zum Löschen den
  Schlüssel vernichten (Crypto-Shredding). Das wäre gründlicher, macht aber Sync und Wiederherstellung
  aufwendiger. *Bewusst entscheiden.*
- **Nicht exportierbar und Sync-Zeitpunkt:** Weil die `NodeId` aus dem Hash des Erstell-Eintrags folgt, kann
  `SetPolicy` erst danach stehen. Vorschlag: Die Oberfläche schreibt Knoten und Richtlinie in einer Transaktion,
  und der Sender filtert Inhalte immer nach der Projektion zum Zeitpunkt des Syncs. Dann geht der Inhalt nie
  „zu früh“ hinaus.
- **Abgeleitete Knoten:** Wird ein Beleg zum Knoten, wird sein Inhalt kopiert. Soll das Löschen des Knotens auch
  den Beleg-Teil wegräumen (Kaskade)? Und umgekehrt: Soll die Frist des Belegs nie den Knoten berühren?
  Empfehlung: Kaskade ja, aber nur in Richtung Knoten → Beleg.
- **Rollups von heimserver** sind Aussagen von heimserver, keine Messungen des Sensors. Ist das für Belege („bewiesen“) genug?
- **Vertrauen in Checkpoints:** Prüfen ab Checkpoint vertraut dem eigenen Gerät, das ihn bezeugt hat. Das
  schützt nicht, wenn der Personen-Schlüssel (E5) kompromittiert ist.
- **ESP32-Kosten ungeprüft:** Wie lange Ed25519-Signieren auf dem ESP32-C3 dauert (32-bit-Backend von
  `ed25519-dalek`) und wie stark die Flash-Puffer den Speicher abnutzen, ist nicht gemessen. Vor M6 messen.
- **SQLite-Reste:** `secure_delete`, WAL und SQLCipher (E6) müssen zusammen getestet werden, sonst liegt
  weggeräumter Klartext noch in freien Seiten.
- **Zeitstempel:** Fenster richten sich nach der Uhr des Schreibers. Sensoren ohne gestellte Uhr (SNTP) liefern
  falsche Fenster. Die Reihenfolge sichert `seq`, die Zuordnung zur Zeit aber nicht.

---

## (f) Quellen

**Transparenzlogs, Checkpoints, Zeugen**
- C2SP tlog-tiles: https://github.com/C2SP/C2SP/blob/main/tlog-tiles.md (Original: https://c2sp.org/tlog-tiles, nicht abrufbar)
- C2SP tlog-checkpoint: https://github.com/C2SP/C2SP/blob/main/tlog-checkpoint.md
- C2SP tlog-witness: https://github.com/C2SP/C2SP/blob/main/tlog-witness.md
- Sunlight-Spezifikation: https://github.com/C2SP/C2SP/blob/main/sunlight.md
- Transparency.dev, Ausblick 2025 (Static CT): https://blog.transparency.dev/what-2025-holds-for-certificate-transparency-and-the-transparencydev-ecosystem
- Tessera: https://github.com/transparency-dev/tessera
- Cloudflare-Vorfall mit fehlenden Kacheln *(sekundär, Mailingliste)*: https://groups.google.com/a/chromium.org/g/ct-policy/c/dVz6tMLWsRY
- Sigsum API und Design: https://git.sigsum.org/sigsum/plain/doc/api.md · Diskussion zu Cosignaturen: https://lists.sigsum.org/mailman3/hyperkitty/list/sigsum-general@lists.sigsum.org/message/DBWCT2AMV2A3VQFG5CKQA4GEE6LJ4HNV
- Merkle Mountain Ranges, IETF-Entwurf MMRIVER: https://datatracker.ietf.org/doc/html/draft-bryce-cose-merkle-mountain-range-proofs-02
- Optimalität von MMR-Varianten (IACR ePrint 2025/234): https://eprint.iacr.org/2025/234
- COSE Receipts (RFC 9942, früher draft-ietf-cose-merkle-tree-proofs): https://ftp.belnet.be/pub/ietf-ftp/internet-drafts/draft-ietf-cose-merkle-tree-proofs-05.html · IESG-Freigabe: https://mid.mail-archive.com/cose@ietf.org/msg04829.html
- SCITT-Architektur RFC 9943: https://www.rfc-editor.org/rfc/rfc9943.html · CCF-Profil (Entwurf 2026): https://www.ietf.org/ietf-ftp/internet-drafts/draft-ietf-scitt-receipts-ccf-profile-02.html

**P2P-Logs und local-first**
- Hypercore: https://github.com/holepunchto/hypercore · Pear-Doku: https://docs.pears.com/reference/building-blocks/hypercore/ (Hypercore 11 mit RocksDB *(sekundär)*)
- SSB Meta Feeds: https://github.com/ssbc/ssb-meta-feeds-spec · Audit Partial Replication: https://ssb-ngi-pointer.github.io/Audit%20Report_%20Secure%20Scuttlebutt%20Partial%20Replication%20and%20Fusion%20Identity.html
- SSB Wachstum und erster Sync *(sekundär)*: https://gitea.it/devol/ecosistema-social-decentralizzato/src/commit/1ccc0a97044e1746091f9817fe26f96e490ac822/protocols/ssb.md
- Manyverse, Compaction in ssb-db2 (08/2022): https://manyver.se/blog/2022-08-update
- PPPPP/PZP: https://manyver.se/blog/2023-05-05 · https://www.manyver.se/blog/2024-07-03
- Nostr NIPs: https://github.com/nostr-protocol/nips · Diskussion Löschen/Undelete *(sekundär)*: https://github.com/nostr-protocol/nips/issues/236
- Beelay Sedimentree: https://github.com/automerge/beelay/blob/main/docs/sedimentree.md · Protokoll: https://github.com/automerge/beelay/blob/main/docs/protocol.md
- Automerge 3.0: https://automerge.org/blog/automerge-3/
- Git shallow: https://github.com/git/git/blob/master/Documentation/technical/shallow.adoc
- iroh-blobs Design (16-KiB-Gruppen): https://docs.rs/crate/iroh-blobs/0.99.0/source/DESIGN.md · bao-tree: https://docs.rs/crate/bao-tree/latest/source/src/lib.rs

**Zeitreihen und Aufbewahrung**
- Prometheus Storage: https://github.com/prometheus/prometheus/blob/main/docs/storage.md
- InfluxDB Downsampling v2: https://docs.influxdata.com/influxdb/v2/process-data/common-tasks/downsample-data · InfluxDB 3 *(Hersteller-Blog)*: https://www.influxdata.com/blog/downsampling-guide-influxdb-3/
- Home Assistant Recorder (Quelltext der Doku): https://github.com/home-assistant/home-assistant.io/blob/current/source/_integrations/recorder.markdown
- Home Assistant Langzeitstatistik (Original nicht abrufbar, Inhalt aus Suchergebnissen): https://www.home-assistant.io/docs/configuration/long-term-statistics/ · Forum *(sekundär)*: https://community.home-assistant.io/t/short-vs-long-term-statistics/800602

**Event Sourcing**
- Snapshots in Event Sourcing *(Hersteller-Blog)*: https://www.eventstore.com/blog/snapshots-in-event-sourcing · *(sekundär)*: https://codeopinion.com/snapshots-in-event-sourcing-for-rehydrating-aggregates/

**Blockchains**
- Bitcoin Core 0.11 Pruning: https://github.com/bitcoin/bitcoin/blob/master/doc/release-notes/release-notes-0.11.0.md
- assumeutxo Design: https://github.com/bitcoin/bitcoin/blob/master/doc/design/assumeutxo.md · Snapshot-Höhen je Version *(sekundär)*: https://www.spark.money/research/bitcoin-assumeutxo-fast-sync
- EIP-4444: https://github.com/ethereum/EIPs/blob/master/EIPS/eip-4444.md
- Partial History Expiry (EF, 2025-07-08): https://blog.ethereum.org/2025/07/08/partial-history-exp · Umsetzungsplan: https://hackmd.io/Dobc38YVQ1qmbbyI6LcFqA

**Kryptografie auf kleinen Geräten**
- ed25519-dalek (32-bit-Backend, Desktop-Benchmarks): https://docs.rs/crate/ed25519-dalek/ · Benchmarks *(sekundär, x86)*: https://github.com/namuyan/ed25519-dalek
