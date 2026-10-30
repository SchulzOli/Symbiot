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

## Entwickeln
Voraussetzungen und Einrichtung: [docs/TECH-STACK.md](docs/TECH-STACK.md#entwicklungsumgebung-windows-pc).

```bash
cargo test --workspace
```
```bash
cargo run -p symbiot-server
```
```bash
cd apps/ui && trunk serve
```
Der Server antwortet unter `http://127.0.0.1:8100/health`, die UI unter `http://127.0.0.1:8080`.

## Status
- **M0 Fundament:** in Arbeit

## Lizenz
MIT
