# Betrieb — Symbiot auf dem Heimserver

## Weg eines Commits

```
Commit auf main ─► GitHub Actions: CI (fmt · clippy · test · no_std · WASM · Desktop · E2E · Android)
                └► Image: ghcr.io/schulzoli/symbiot-server:{main, sha-…}
                          │
                Komodo auf dem Heimserver („Poll for Updates“ + „Auto Update“) zieht :main nachts, startet neu
                          │
                https://heimserver.example.ts.net:8449  (nur Tailnet)

Tag vX.Y.Z ─► Release: Installer für Windows, Linux, macOS und das Android-APK als GitHub Release
            └► Image zusätzlich als :X.Y.Z
```

## Stack auf dem Heimserver

| Was | Wo |
|---|---|
| Compose | `/opt/stacks/symbiot/compose.yaml` (Vorlage: [deploy/compose.yaml](../deploy/compose.yaml)) |
| Daten | `/opt/stacks/symbiot/data`: `symbiot.db`, `device.key` und `files/` (Fotos und Anhänge der Rohdaten). Gehört uid 65532. **Gehört ins Backup**, denn `device.key` ist die Identität des Servers. |
| Lokal | `http://127.0.0.1:8100` (`/health`, `/api/…`, UI) |
| Tailnet | `https://heimserver.example.ts.net:8449`, Eintrag in `/opt/stacks/tailscale/config/serve.json` |
| Gesundheit | Docker-HEALTHCHECK `symbiot-server --health` → Komodo und Beszel sehen `healthy`/`unhealthy` |
| Logs | JSON nach stdout → Dozzle. Level `ERROR` bei Speicherfehlern und **gebrochener Kette** |

**Backup zurückspielen (2026-11-10, PLAN H5):** Nur gefahrlos, wenn der Heimserver seit dem Backup nichts selbst
geschrieben hat (keine Bedienung über die Web-Oberfläche, kein Import dort). Sonst kennen die Geräte schon spätere
Einträge seiner Kette, und was er danach schreibt, passt nicht mehr dazu: Die Geräte lehnen seine Kette dann
dauerhaft ab. Einen Weg zurück gibt es in der App noch nicht. Daten und Dateien liegen bis H5 unverschlüsselt im
Datenordner.

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
3. **Datenordner der Desktop-App:** Er heißt nach der App-Kennung `io.github.schulzoli.symbiot`, unter Windows
   `%APPDATA%\io.github.schulzoli.symbiot`. Bis v0.3.0 hatte die App eine andere Kennung. Wer sie schon benutzt hat,
   verschiebt den alten Ordner (mit `symbiot.db`, `device.key`, `sync-url.txt`) vor dem ersten Start unter den
   neuen Namen. Sonst startet die App leer, und das Gerät braucht einen neuen Schlüssel.
4. **Verlorenes Gerät:** auf einem anderen Gerät unter „Geräte“ widerrufen (braucht die Wörter).

**Android (M5):** Das APK aus dem Release installieren (Android fragt einmal nach der Erlaubnis für Apps aus
dieser Quelle). Einrichten wie am Desktop: „Geräte“ → vorhandenen Schlüssel eingeben, Adresse des Heimservers
eintragen. Das Handy muss den Heimserver erreichen, also im Tailnet sein (Tailscale-App). Die Daten liegen im
privaten App-Speicher; deinstallieren löscht sie.

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
3. **Dozzle-Alarm** (noch offen, PLAN H7): Container `symbiot`, Bedingung Level `error` → Alarm-Raum in Matrix.
4. **Beszel-Alarm** (noch offen, PLAN H7): Container `symbiot` *unhealthy/stopped* → Alarm-Raum in Matrix.
5. **Android-Signaturschlüssel:** Einmal erzeugen und gut sichern (Passwort-Manager). Mit einem neuen Schlüssel
   kann ein installiertes APK nicht mehr aktualisiert werden, nur neu installiert (Daten weg).
   ```bash
   keytool -genkeypair -keystore symbiot-release.jks -storetype PKCS12 -alias symbiot -keyalg RSA -keysize 4096 -validity 10000 -dname "CN=Symbiot"
   ```
   Dann unter *GitHub → Settings → Secrets and variables → Actions* drei Secrets anlegen:
   `ANDROID_KEYSTORE_BASE64` (Ausgabe von `base64 -w0 symbiot-release.jks`), `ANDROID_KEYSTORE_PASSWORD` (das
   Passwort von oben) und `ANDROID_KEY_ALIAS` (`symbiot`). Ohne diese Secrets erscheint der Release ohne APK.

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
Die Version im Tag muss zur Version in `Cargo.toml` passen (`[workspace.package] version`). Der Release enthält
die Desktop-Installer und, sobald die Secrets aus Schritt 5 angelegt sind, das signierte Android-APK (arm64).
Jeder Pull Request baut ein Test-APK (Artefakt `android-apk-test`) mit einem Wegwerf-Schlüssel: zum Ausprobieren,
nicht als Update für die Release-App.
