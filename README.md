# Symbiot

> **Ein privater Graph deiner Ziele, Beziehungen und Belege. Eine KI denkt mit und schlägt dir den nächsten Schritt vor.
> Alles, was sie sagt, ist überprüfbar, und du entscheidest.**

Hervorgegangen aus dem Ideengarten *TrustInAI*. Benannt nach der *Symbiose Mensch ↔ Maschine*.

## Die Regeln
- **Die KI schlägt vor, der Mensch entscheidet.** Nichts wird automatisch ausgeführt.
- **Nichts wird heimlich verändert.** Jede Änderung ist ein signierter Eintrag in einer Hash-Kette.
- **Deine Daten. Deine Regeln.** Local-first. Der Server auf deinem Home Server ist eine Kopie, kein Herr.
- **Ein Kern für alles.** Dieselbe Rust-Logik läuft auf Desktop, Handy, Server und Mikrocontroller.

## Dokumente
- [docs/TECH-STACK.md](docs/TECH-STACK.md): Stack, Architektur, Entwicklungsumgebung
- [docs/PLAN.md](docs/PLAN.md): Stand, Reihenfolge, Meilensteine M0–M10 und Härtung, Lifecycle, Checkpoints
- [docs/FEATURES.md](docs/FEATURES.md): erste Features F1–F7 mit Abnahmekriterien und bekannten Lücken
- [docs/BETRIEB.md](docs/BETRIEB.md): Weg eines Commits bis zum Heimserver, Release, Einrichtung, Prüfen
- [docs/ENTSCHEIDUNGEN.md](docs/ENTSCHEIDUNGEN.md): Entscheidungen E1–E18 (Löschen, Schlüssel, Verschlüsselung, Adapter, Bündeln, Symbiot + Connect, Lebensgraph, Oberfläche, KI-Gleichstellung, Langflow, Härtung vor neuen Features)
- [docs/ROHDATEN.md](docs/ROHDATEN.md): Format der Rohdaten, der Vertrag zwischen Integration und Aufbereitung (E14)

## Aufbau
| Teil | Rolle |
|---|---|
| `crates/symbiot-core` | `no_std`-Kern: Knoten, signierte Hash-Kette (Kopf + entfernbarer Inhalt), Prüfung, Projektion, Rohdaten-Format (E14) |
| `crates/symbiot-store` | SQLite: Kette als Wahrheit, Knoten und Rohdaten als Projektion, Geräteschlüssel |
| `crates/symbiot-words` | Die 24 Wörter des Personen-Schlüssels (E5): dieselbe Stelle auf dem Gerät und im Browser |
| `crates/symbiot-sync` | Abgleich (M4): signierte Anfragen, Client, der die eigene Kette sendet und fehlende holt |
| `crates/symbiot-import` | Import-Adapter (E14): Telegram, WhatsApp, mbox, Git. Werkzeug `symbiot-import`, siehe [ROHDATEN.md](docs/ROHDATEN.md#importieren-symbiot-import) |
| `crates/symbiot-ai` | „Nächsten Schritt vorschlagen“: Kontext, Anfrage an LiteLLM, Prüfung der Quellen. Schreibt nie |
| `apps/ui` | Leptos-Oberfläche, im Browser über `/api`, in Tauri über `invoke` |
| `apps/desktop` | Tauri 2: Windows, macOS, Linux und Android (später iOS), Daten lokal |
| `apps/server` | axum auf dem Heimserver: API, liefert die UI aus, `/health` |
| `prototypes/connect` | Prototyp der Idee Connect als 3D-Graph mit Kurzfilm. Eigenständig, Node ohne Abhängigkeiten, nicht im Server-Image. [README](prototypes/connect/README.md) |

## Entwickeln
Voraussetzungen und Einrichtung: [docs/TECH-STACK.md](docs/TECH-STACK.md#entwicklungsumgebung-windows).

```bash
cargo test --workspace
```
Der Server baut SQLCipher mit OpenSSL aus den Quellen; das braucht `perl` und `make` (unter Windows baut er ohne).
Web: Server und UI getrennt starten. Die UI leitet `/api` noch nicht weiter, deshalb nach `trunk build` die UI über den Server öffnen:
```bash
cd apps/ui && trunk build
```
```bash
SYMBIOT_UI=apps/ui/dist cargo run -p symbiot-server
```
Dann `http://127.0.0.1:8100` öffnen.

Desktop mit Hot Reload:
```bash
cd apps/desktop && cargo tauri dev
```

## Status
- **M0 Fundament:** fertig
- **M1 Walking Skeleton:** fertig (v0.1.0, läuft auf dem Heimserver; Alarme bewusst weggelassen)
- **M2 Verknüpfen:** fertig (v0.2.0): Kanten, Graph, Bearbeiten mit Verlauf, E2E-Test
- **M3 Mitdenken:** fertig (v0.2.0): KI-Vorschlag mit Quellen, Annehmen/Ablehnen im Ledger, sensible Knoten, Feedback-Link. Offen: Woche Dogfooding (Checkpoint)
- **Vor M4:** fertig (v0.2.0): Kettenformat v2 (Protokoll 2) mit Löschen, „löschbar“ und „exportierbar“ pro Knoten (E2, E3, E13)
- **E4 Sensibel:** Regeln für Sensibles, von der KI vorgeschlagen, von dir angenommen
- **M4 Synchron:** fertig (v0.3.0): Personen-Schlüssel (E5), Abgleich über den Heimserver, neueste Version gewinnt mit Hinweis, Widerruf. Einrichtung: [BETRIEB.md](docs/BETRIEB.md#abgleich-m4)

In v0.4.0 (auf `main`, Tag steht noch aus):
- **M5 Überall:** Android-App aus derselben Codebasis, APK baut in der CI. Offen: Probe am echten Handy
- **M7 Integration:** Rohdaten-Format, Import aus Telegram, WhatsApp, mbox und Git (Werkzeug und aus der App), Zeitleiste, Volltextsuche, Datei-Speicher, Quellen-Verwaltung. Offen: Checkpoint mit echten Daten
- **M8 Aufbereitung:** Personen über alle Quellen, Zusammenführen als Entscheidung. Offen: Aussagen, Themen, Fragen in Worten
- **E15 Oberfläche, E16 Assistent:** Canvas als Startansicht, barrierefrei; die KI schlägt Aktionen vor, du entscheidest
- **H Härtung (E18):** Löschen löscht alles, „nicht exportierbar“ bleibt auf dem Gerät, der Abgleich hängt nicht mehr an
  der Datenmenge, fremde Webseiten können nichts schreiben, die 24 Wörter verlassen den Browser nicht, der Heimserver
  ist mit Schlüssel verschlüsselt, und jedes Gerät bringt einen neuen Heimserver zurück. Offen: Schlüsselbund in den
  Apps, Adapter ohne Wörter (wartet auf eine Entscheidung)

**Bevor du Daten anschließt:** den Heimserver verschlüsseln ([BETRIEB.md](docs/BETRIEB.md#verschlüsselung-h5-e6)).
Was als Nächstes kommt, steht in [PLAN.md](docs/PLAN.md#reihenfolge-ab-jetzt-e18).

End-to-End-Test lokal (braucht `trunk build --release` in `apps/ui`):
```bash
cd e2e && npm ci && npx playwright install chromium && npx playwright test
```

## Lizenz
MIT
