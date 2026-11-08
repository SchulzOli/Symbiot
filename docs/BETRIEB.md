# Betrieb — Symbiot auf dem Heimserver

## Weg eines Commits

```
Commit auf main ─► GitHub Actions: CI (fmt · clippy · test · no_std · WASM · Desktop)
                └► Image: ghcr.io/schulzoli/symbiot-server:{main, sha-…}
                          │
                Komodo auf dem Heimserver („Poll for Updates“ + „Auto Update“) zieht :main nachts, startet neu
                          │
                https://heimserver.example.ts.net:8449  (nur Tailnet)

Tag vX.Y.Z ─► Release: Installer für Windows, Linux, macOS als GitHub Release
            └► Image zusätzlich als :X.Y.Z
```

## Stack auf dem Heimserver

| Was | Wo |
|---|---|
| Compose | `/opt/stacks/symbiot/compose.yaml` (Vorlage: [deploy/compose.yaml](../deploy/compose.yaml)) |
| Daten | `/opt/stacks/symbiot/data`: `symbiot.db` + `device.key`. Gehört uid 65532. **Gehört ins Backup**, denn `device.key` ist die Identität des Servers. |
| Lokal | `http://127.0.0.1:8100` (`/health`, `/api/…`, UI) |
| Tailnet | `https://heimserver.example.ts.net:8449`, Eintrag in `/opt/stacks/tailscale/config/serve.json` |
| Gesundheit | Docker-HEALTHCHECK `symbiot-server --health` → Komodo und Beszel sehen `healthy`/`unhealthy` |
| Logs | JSON nach stdout → Dozzle. Level `ERROR` bei Speicherfehlern und **gebrochener Kette** |

## KI (M3)

Der Server fragt LiteLLM nur, wenn `SYMBIOT_AI_URL` gesetzt ist. Ohne KI läuft alles andere weiter, nur
„Nächsten Schritt vorschlagen“ meldet „KI ist nicht eingerichtet“.

| Variable | Bedeutung |
|---|---|
| `SYMBIOT_AI_URL` | OpenAI-kompatible Basis-URL, auf dem Heimserver `http://127.0.0.1:8085/v1` (LiteLLM) |
| `SYMBIOT_AI_KEY` | LiteLLM-Schlüssel, **nur** in `/opt/stacks/symbiot/.env`, nie im Repo |
| `SYMBIOT_AI_MODEL` | Modell im Heimnetz, Standard `qwen3:8b` (Name wie in LiteLLM) |
| `SYMBIOT_AI_REMOTE_MODEL` | optional: externes Modell hinter dem Masken-Filter. Bekommt nie sensible Knoten |
| `SYMBIOT_AI_TIMEOUT_S` | Zeitlimit pro Vorschlag, Standard 120 |

Die Desktop-App liest dieselben Variablen beim Start. Ins Log schreibt der Server nur Metadaten (Modell,
Anzahl Quellen, zurückgehaltene sensible Knoten), nie den Inhalt eines Vorschlags.

## Abgleich (M4)

Die Geräte gleichen über den Heimserver ab. Auf dem Heimserver selbst ist nichts einzustellen; die Endpunkte liegen unter `/api/sync`.

1. **Heimserver bestätigen:** `https://heimserver.example.ts.net:8449` öffnen, unter „Geräte“ den Personen-Schlüssel
   anlegen. Die 24 Wörter aufschreiben oder in den Passwort-Manager legen; sie werden nirgends gespeichert.
2. **Desktop:** Unter „Geräte“ (steht oben, solange das Gerät nicht eingerichtet ist) „Vorhandenen Schlüssel
   eingeben“ und einen Namen vergeben, dann bei „Adresse des Heimservers“ `https://heimserver.example.ts.net:8449` eintragen
   und speichern. Die App merkt sich die Adresse (`sync-url.txt` im Datenordner) und gleicht jede Minute ab,
   oder mit „Jetzt abgleichen“. `SYMBIOT_SYNC_URL` gilt nur, solange in der App keine Adresse gespeichert ist.
3. **Verlorenes Gerät:** auf einem anderen Gerät unter „Geräte“ widerrufen (braucht die Wörter).

Im Log stehen nur Metadaten (Gerät, Anzahl Einträge), nie Wörter oder Inhalte. Eine Verzweigung (ein
Geräteschlüssel schreibt zwei Geschichten) wird mit Level `ERROR` geloggt.

## Einmalig einrichten (von Hand, braucht deine Anmeldungen)

0. **Stack anlegen:** [deploy/install.sh](../deploy/install.sh) mit `compose.yaml` zum Heimserver kopieren und mit sudo starten.
   Das Skript legt `/opt/stacks/symbiot` an, trägt Port 8449 in `tailscale serve` ein (mit Sicherung) und startet den Container.
   ```bash
   scp deploy/compose.yaml deploy/install.sh heimserver:symbiot-deploy/
   ```
   ```bash
   ssh -t heimserver 'sudo bash ~/symbiot-deploy/install.sh'
   ```
1. **Komodo:** Stack `symbiot` als *Files on Server* anlegen, Server = Heimserver, Run Directory `/opt/stacks/symbiot`.
   Danach *Poll for Updates* und *Auto Update* einschalten.
2. **GHCR:** Ist das Paket `symbiot-server` privat, unter *GitHub → Packages → symbiot-server → Package settings* auf **Public** stellen.
   Alternative: in Komodo eine Registry-Anmeldung für ghcr.io hinterlegen.
3. **Dozzle-Alarm:** Container `symbiot`, Bedingung Level `error` → Alarm-Raum in Matrix.
4. **Beszel-Alarm:** Container `symbiot` *unhealthy/stopped* → Alarm-Raum in Matrix.

## Prüfen

```bash
ssh heimserver 'curl -s http://127.0.0.1:8100/health; echo; curl -s http://127.0.0.1:8100/api/ledger/verify'
```
```bash
ssh heimserver 'docker inspect --format "{{.State.Health.Status}}" symbiot'
```

## Release

```bash
git tag v0.3.0
```
```bash
git push origin v0.3.0
```
Die Version im Tag muss zur Version in `Cargo.toml` passen (`[workspace.package] version`).
