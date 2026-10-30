# Betrieb — Symbiot auf sz

## Weg eines Commits

```
Commit auf main ─► GitHub Actions: CI (fmt · clippy · test · no_std · WASM · Desktop)
                └► Image: ghcr.io/schulzoli/symbiot-server:{main, sha-…}
                          │
                Komodo auf sz („Poll for Updates“ + „Auto Update“) zieht :main, startet neu
                          │
                https://sz.tailacb899.ts.net:8449  (nur Tailnet)

Tag vX.Y.Z ─► Release: Installer für Windows, Linux, macOS als GitHub Release
            └► Image zusätzlich als :X.Y.Z
```

## Stack auf sz

| Was | Wo |
|---|---|
| Compose | `/opt/stacks/symbiot/compose.yaml` (Vorlage: [deploy/compose.yaml](../deploy/compose.yaml)) |
| Daten | `/opt/stacks/symbiot/data`: `symbiot.db` + `device.key`. Gehört uid 65532. **Gehört ins Backup**, denn `device.key` ist die Identität des Servers. |
| Lokal | `http://127.0.0.1:8100` (`/health`, `/api/…`, UI) |
| Tailnet | `https://sz.tailacb899.ts.net:8449`, Eintrag in `/opt/stacks/tailscale/config/serve.json` |
| Gesundheit | Docker-HEALTHCHECK `symbiot-server --health` → Komodo und Beszel sehen `healthy`/`unhealthy` |
| Logs | JSON nach stdout → Dozzle. Level `ERROR` bei Speicherfehlern und **gebrochener Kette** |

## Einmalig einrichten (von Hand, braucht deine Anmeldungen)

0. **Stack anlegen:** [deploy/install-sz.sh](../deploy/install-sz.sh) mit `compose.yaml` nach sz kopieren und mit sudo starten.
   Das Skript legt `/opt/stacks/symbiot` an, trägt Port 8449 in `tailscale serve` ein (mit Sicherung) und startet den Container.
   ```bash
   scp deploy/compose.yaml deploy/install-sz.sh sz:symbiot-deploy/
   ```
   ```bash
   ssh -t sz 'sudo bash ~/symbiot-deploy/install-sz.sh'
   ```
1. **Komodo:** Stack `symbiot` als *Files on Server* anlegen, Server `sz`, Run Directory `/opt/stacks/symbiot`.
   Danach *Poll for Updates* und *Auto Update* einschalten.
2. **GHCR:** Ist das Paket `symbiot-server` privat, unter *GitHub → Packages → symbiot-server → Package settings* auf **Public** stellen.
   Alternative: in Komodo eine Registry-Anmeldung für ghcr.io hinterlegen.
3. **Dozzle-Alarm:** Container `symbiot`, Bedingung Level `error` → Matrix-Raum „Homelab“.
4. **Beszel-Alarm:** Container `symbiot` *unhealthy/stopped* → Matrix-Raum „Homelab“.
5. **HomeLab-Repo** auffrischen: `scripts\sync-from-servers.ps1`, Diff prüfen, committen.

## Prüfen

```bash
ssh sz 'curl -s http://127.0.0.1:8100/health; echo; curl -s http://127.0.0.1:8100/api/ledger/verify'
```
```bash
ssh sz 'docker inspect --format "{{.State.Health.Status}}" symbiot'
```

## Release

```bash
git tag v0.1.0
```
```bash
git push origin v0.1.0
```
Die Version im Tag muss zur Version in `Cargo.toml` passen (`[workspace.package] version`).
