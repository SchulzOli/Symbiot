# Tech-Stack — Entscheidungen

Stand: 2026-10-30 · ersetzt die Supabase-Entscheidung vom 2026-08-09 (`TrustInAI/_Overview/TECH-STACK.md`).

## Vorgaben

1. **Rust**, neu begonnen. Kein Code aus den alten Prototypen wird übernommen, nur Ideen.
2. **Plattformunabhängig:** Desktop (Windows, macOS, Linux), Mobile (Android, iOS), später **IoT** (Embedded).
3. **Deploy auf den Heimserver**, mit Komodo, Tailscale, LiteLLM, Beszel und Dozzle.
4. **Local-first:** Die Daten liegen auf deinem Gerät. Der Server ist eine Kopie und Helfer, nicht der Herr.

## Der Stack auf einen Blick

| Schicht | Wahl | Warum |
|---|---|---|
| Sprache | **Rust (stable, Edition 2024)** | eine Sprache von Mikrocontroller bis Server |
| Kern | **`symbiot-core`**: `#![no_std]` + `alloc` | dieselbe Logik läuft auf ESP32, Handy, PC und Server |
| Hash / Signatur | `sha2` + `ed25519-dalek` | beide `no_std`-fähig, geprüft, Standard |
| Kanonische Bytes | `serde` + `postcard` | deterministisch und `no_std`, damit Hashes überall gleich sind |
| IDs | `uuid` v7 | zeitlich sortierbar, ohne Koordination |
| Speicher | **SQLite** über `rusqlite` (`bundled`) | läuft auf Desktop, Android, iOS und Server ohne Installation |
| Massendaten (E14) | **Parquet + DuckDB**, Datei-Speicher nach Inhalts-Hash, Volltext mit FTS5 (später Tantivy), Ähnlichkeit mit `sqlite-vec` (später LanceDB) | alles eingebettet. Nur Ketten und Dateien sind Wahrheit, der Rest lässt sich neu bauen |
| UI | **Leptos** (CSR → WASM) | UI in Rust, teilt die Typen direkt mit dem Kern, kein TS-Zwilling |
| App-Hülle | **Tauri 2** | ein Projekt für Windows, macOS, Linux, Android und iOS. Mit Updater, Signierung und fertiger GitHub Action |
| Server | **axum** + `tokio` | Sync-API, liefert dieselbe UI als Web-App aus, KI-Proxy |
| KI | **LiteLLM auf dem Heimserver** (OpenAI-kompatibel) | lokale Modelle (z. B. qwen3) als Standard. Claude nur über einen Masken-Filter |
| Logs | `tracing` (JSON) | zum Beispiel Dozzle liest und alarmiert |
| Lint / Format | `rustfmt` + `clippy -D warnings` | ein Werkzeug, keine Diskussion |
| Tests | `cargo test` + `proptest` (Ledger) + axum-Integrationstests | Playwright-E2E ab M2 |
| CI/CD | **GitHub Actions** → **GHCR** → **Komodo** (Auto-Update) | GitHub erreicht den Heimserver nicht, also zieht Komodo das Image selbst |
| Netz | **Tailscale** (nur Tailnet) | keine offenen Ports nach außen |

## Architektur

```
                ┌──────────────────────── symbiot-core (no_std) ────────────────────────┐
                │ Node · Edge · Evidence · Proposal · Consent · LedgerEntry · verify()   │
                └───────────────┬───────────────────────┬───────────────────────┬────────┘
                                │                       │                       │
   ESP32 (später)          symbiot-store            symbiot-ai              (alle teilen
   signiert Belege         SQLite + Projektion      LiteLLM-Client           dieselben Typen)
        │                       │                       │
        │          ┌────────────┴────────────┐          │
        │          │                         │          │
        │    apps/desktop (Tauri 2)     apps/server (axum) ──► LiteLLM :8085 ──► Ollama / Claude
        │    Win · mac · Linux          auf dem Heimserver, Docker            (Masken-Filter)
        │    Android · iOS                   │
        │          │    UI: apps/ui (Leptos) │ liefert apps/ui auch als Web-App aus
        │          └──────── Sync ───────────┘
        └──────────────────── Sync ──────────┘          alles nur über Tailscale
```

### Die eine Idee, die alles einfach macht: Jedes Gerät hat eine eigene Kette

- Jedes Gerät (PC, Handy, Sensor, Server) besitzt einen **eigenen Ed25519-Schlüssel** und schreibt eine **eigene Hash-Kette**, die nur angehängt wird.
- **Sync = Ketten austauschen.** Ketten verschiedener Geräte kollidieren nie, weil jede nur einen Schreiber hat. Zusammenführen heißt fremde Einträge prüfen und anhängen.
- **Zustand = Projektion** aller bekannten Ketten in SQLite (Knoten, Kanten, Vorschläge).
- Ein IoT-Sensor ist damit **einfach ein weiteres Gerät**. Er braucht nur `symbiot-core`, keine Datenbank.

So bleiben drei Versprechen technisch eingelöst: *nichts wird heimlich verändert*, *deine Daten liegen bei dir*, *jede Aussage ist prüfbar*.

## Was wir bewusst (noch) nicht machen

| Nicht | Warum | Wann wieder prüfen |
|---|---|---|
| Neo4j / Graph-DB | SQLite mit rekursiven CTEs reicht für einen Menschen | wenn Abfragen messbar zu langsam werden |
| CRDTs | Ketten mit je einem Schreiber kollidieren nicht | wenn zwei Geräte *denselben* Knoten gleichzeitig bearbeiten |
| Eigenes Login | Tailnet und Geräte-Schlüssel genügen für einen Nutzer | Phase WIR (zweiter Mensch) |
| Supabase / Cloud | widerspricht local-first | nie für private Daten |
| Dioxus statt Tauri + Leptos | Tauri ist bei Mobile, Signierung und Releases reifer | falls Tauri mobil hakt |

## Entwicklungsumgebung (Windows)

Getestet mit: Rust 1.99 (Ziel `x86_64-pc-windows-gnu`), Node 24, Docker 29, `gh` angemeldet.

Noch nötig:
```bash
rustup toolchain install stable-x86_64-pc-windows-msvc
```
```bash
rustup default stable-x86_64-pc-windows-msvc
```
```bash
rustup target add wasm32-unknown-unknown riscv32imc-unknown-none-elf
```
```bash
cargo install --locked trunk tauri-cli cargo-nextest
```
Für Tauri unter Windows ist das MSVC-Ziel empfohlen. Dafür braucht es die *Visual Studio Build Tools* (C++). WebView2 bringt Windows 11 schon mit.
Für Android später: Android Studio mit SDK und NDK. **iOS braucht einen Mac** (oder einen macOS-Runner in GitHub Actions).
