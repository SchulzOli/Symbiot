# Connect – schlankes Image ohne Abhängigkeiten.
# Läuft auf amd64 und arm64 (z. B. Hetzner CX und CAX).
FROM node:22-alpine

ENV NODE_ENV=production \
    PORT=3000 \
    HOST=0.0.0.0

WORKDIR /app
COPY --chown=node:node package.json server.js ./
COPY --chown=node:node public ./public

# Nicht als root laufen
USER node
EXPOSE 3000

HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
  CMD wget -q -O /dev/null "http://127.0.0.1:${PORT}/healthz" || exit 1

CMD ["node", "server.js"]
