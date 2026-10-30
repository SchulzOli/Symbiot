#!/usr/bin/env bash
# Symbiot auf heimserver einrichten (angelegt per Claude, 2026-10-30).
# Aufruf auf heimserver:  sudo bash install.sh   (liegt neben compose.yaml)
# Legt /opt/stacks/symbiot an, trägt Port 8449 in tailscale serve ein und startet den Stack.
# Mehrfach ausführbar: vorhandene Daten und Einträge bleiben unverändert.
set -euo pipefail

SRC="$(cd "$(dirname "$0")" && pwd)"
DIR=/opt/stacks/symbiot
SERVE=/opt/stacks/tailscale/config/serve.json
PORT=8449

[ "$(id -u)" -eq 0 ] || { echo "Bitte mit sudo starten." >&2; exit 1; }

echo "1/3 Stack-Ordner $DIR"
install -d -m 755 "$DIR"
install -m 644 "$SRC/compose.yaml" "$DIR/compose.yaml"
# Daten gehören dem nonroot-Nutzer im Image (uid 65532).
install -d -m 700 -o 65532 -g 65532 "$DIR/data"

echo "2/3 tailscale serve :$PORT -> 127.0.0.1:8100"
if grep -q "\"$PORT\"" "$SERVE"; then
  echo "    schon eingetragen"
else
  cp -p "$SERVE" "$SERVE.bak-$(date +%Y%m%d-%H%M%S)"
  python3 - "$SERVE" "$PORT" <<'PY'
import json, sys
path, port = sys.argv[1], sys.argv[2]
with open(path) as f:
    cfg = json.load(f)
cfg["TCP"][port] = {"HTTPS": True}
cfg["Web"]["${TS_CERT_DOMAIN}:" + port] = {"Handlers": {"/": {"Proxy": "http://127.0.0.1:8100"}}}
with open(path, "w") as f:
    json.dump(cfg, f, indent=2)
    f.write("\n")
PY
  echo "    eingetragen (Sicherung: $SERVE.bak-*)"
fi

echo "3/3 Stack starten"
cd "$DIR"
docker compose pull
docker compose up -d

status=starting
for _ in $(seq 1 30); do
  status=$(docker inspect -f '{{.State.Health.Status}}' symbiot 2>/dev/null || echo missing)
  [ "$status" = healthy ] && break
  sleep 2
done
echo
echo "Container: $status"
curl -s http://127.0.0.1:8100/health && echo
if docker exec tailscale tailscale serve status 2>/dev/null | grep -q ":$PORT"; then
  echo "Tailscale: https://heimserver.example.ts.net:$PORT aktiv"
else
  echo "Tailscale: :$PORT noch nicht aktiv. Einmal 'docker restart tailscale' (kurze Unterbrechung des Tailnet-Zugriffs auf heimserver)."
fi
