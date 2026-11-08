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
| `actors` | Personen, wie die Quelle sie nennt, je mit Schema und Rolle | `mailto:a@b.de` als `from`, `tel:+4930…` als `to` |
| `text` | der Text, unverändert; leer, wenn es keinen gibt | |
| `fields` | weitere Felder der Quelle als Paare | `("subject", …)`, `("thread", …)`, `("url", …)` |
| `files` | angehängte Dateien: SHA-256, Name, MIME-Typ, Größe | Die Datei selbst liegt im Datei-Speicher (#28). |

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

## Noch nicht enthalten

- Einzelne Belege löschen (das Entfernen eines Teils statt des ganzen Bündels). Kommt mit „Aufgeräumt“ (E13).
- Freigabe pro Adapter (`Grant`, E7, E10). Bis dahin gilt: Ein Adapter ist ein bestätigtes Gerät der Person.
- Volltextsuche und Zeitleiste (#30), Datei-Speicher (#28), die ersten Adapter (#29).
