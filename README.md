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
- [docs/PLAN.md](docs/PLAN.md): Meilensteine M0–M6, Lifecycle, Checkpoints
- [docs/FEATURES.md](docs/FEATURES.md): erste Features F1–F7 mit Abnahmekriterien
- [docs/BETRIEB.md](docs/BETRIEB.md): Weg eines Commits bis heimserver, Release, Einrichtung, Prüfen
- [docs/ENTSCHEIDUNGEN.md](docs/ENTSCHEIDUNGEN.md): Entscheidungen E1–E13 (Löschen, Schlüssel, Verschlüsselung, Adapter, Bündeln, Symbiot + Connect)

## Aufbau
| Teil | Rolle |
|---|---|
| `crates/symbiot-core` | `no_std`-Kern: Knoten, signierte Hash-Kette, Prüfung, Projektion |
| `crates/symbiot-store` | SQLite: Kette als Wahrheit, Knoten als Projektion, Geräteschlüssel |
| `crates/symbiot-ai` | „Nächsten Schritt vorschlagen“: Kontext, Anfrage an LiteLLM, Prüfung der Quellen. Schreibt nie |
| `apps/ui` | Leptos-Oberfläche, im Browser über `/api`, in Tauri über `invoke` |
| `apps/desktop` | Tauri 2: Windows, macOS, Linux (später Android, iOS), Daten lokal |
| `apps/server` | axum auf heimserver: API, liefert die UI aus, `/health` |
| `prototypes/connect` | Prototyp der Idee Connect als 3D-Graph mit Kurzfilm. Eigenständig, Node ohne Abhängigkeiten, nicht im Server-Image. [README](prototypes/connect/README.md) |

## Entwickeln
Voraussetzungen und Einrichtung: [docs/TECH-STACK.md](docs/TECH-STACK.md#entwicklungsumgebung-windows-pc).

```bash
cargo test --workspace
```
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
- **M1 Walking Skeleton:** fertig (v0.1.0, läuft auf heimserver; Alarme bewusst weggelassen)
- **M2 Verknüpfen:** Kanten, Graph, Bearbeiten mit Verlauf, E2E-Test
- **M3 Mitdenken:** KI-Vorschlag mit Quellen, Annehmen/Ablehnen im Ledger, sensible Knoten, Feedback-Link. Offen: Woche Dogfooding (Checkpoint)

End-to-End-Test lokal (braucht `trunk build --release` in `apps/ui`):
```bash
cd e2e && npm ci && npx playwright install chromium && npx playwright test
```

## Lizenz
MIT
