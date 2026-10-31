'use strict';

/*
 * Minimaler Webserver für den Connect-Prototyp – ohne Abhängigkeiten.
 *
 * - liefert die statischen Dateien aus ./public aus (komprimiert, mit ETag)
 * - GET /healthz für Health-Checks (z. B. Coolify, Docker)
 *
 * Es wird nichts gespeichert und nichts protokolliert.
 */

const http = require('node:http');
const fs = require('node:fs/promises');
const path = require('node:path');
const crypto = require('node:crypto');
const zlib = require('node:zlib');

const PORT = Number(process.env.PORT) || 3000;
const HOST = process.env.HOST || '0.0.0.0';
const PUBLIC_DIR = path.join(__dirname, 'public');

const MIME_TYPES = {
  '.html': 'text/html; charset=utf-8',
  '.css': 'text/css; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json; charset=utf-8',
  '.svg': 'image/svg+xml',
  '.png': 'image/png',
  '.ico': 'image/x-icon',
  '.txt': 'text/plain; charset=utf-8',
};
const COMPRESSIBLE = /^(text\/|application\/json|image\/svg)/;

const SECURITY_HEADERS = {
  'Content-Security-Policy': "default-src 'self'; img-src 'self' data:; script-src 'self'; style-src 'self'; "
    + "connect-src 'self'; base-uri 'none'; form-action 'none'; frame-ancestors 'none'",
  'Referrer-Policy': 'no-referrer',
  'X-Content-Type-Options': 'nosniff',
  'Permissions-Policy': 'geolocation=(), camera=(), microphone=(), payment=(), usb=()',
};

const TEXT = { 'Content-Type': 'text/plain; charset=utf-8' };

function send(res, status, headers, body) {
  res.writeHead(status, { ...SECURITY_HEADERS, ...headers });
  res.end(res.req.method === 'HEAD' ? undefined : body);
}

// Zwischenspeicher für Dateien: Inhalt, komprimierte Fassungen und ETag. Ändert sich
// eine Datei auf der Platte (mtime/Größe), wird sie neu eingelesen.
const fileCache = new Map();

async function loadFile(filePath) {
  const stat = await fs.stat(filePath);
  if (stat.isDirectory()) {
    const err = new Error('directory');
    err.code = 'EISDIR';
    throw err;
  }
  const cached = fileCache.get(filePath);
  if (cached && cached.mtimeMs === stat.mtimeMs && cached.size === stat.size) return cached;

  const data = await fs.readFile(filePath);
  const type = MIME_TYPES[path.extname(filePath).toLowerCase()] || 'application/octet-stream';
  const entry = {
    mtimeMs: stat.mtimeMs,
    size: stat.size,
    type,
    data,
    etag: `"${crypto.createHash('sha256').update(data).digest('base64url').slice(0, 27)}"`,
  };
  if (COMPRESSIBLE.test(type) && data.length > 512) {
    entry.br = zlib.brotliCompressSync(data, { params: { [zlib.constants.BROTLI_PARAM_QUALITY]: 11 } });
    entry.gzip = zlib.gzipSync(data, { level: 9 });
  }
  fileCache.set(filePath, entry);
  return entry;
}

function pickEncoding(req, file) {
  const accepted = req.headers['accept-encoding'] || '';
  if (file.br && /\bbr\b/.test(accepted)) return 'br';
  if (file.gzip && /\bgzip\b/.test(accepted)) return 'gzip';
  return null;
}

async function handleStatic(req, res, pathname) {
  let relative;
  try {
    relative = decodeURIComponent(pathname);
  } catch {
    send(res, 400, TEXT, 'Bad request');
    return;
  }
  if (relative.includes('\0')) {
    send(res, 400, TEXT, 'Bad request');
    return;
  }
  if (relative.endsWith('/')) relative += 'index.html';

  const filePath = path.join(PUBLIC_DIR, relative);
  if (!filePath.startsWith(PUBLIC_DIR + path.sep)) {
    send(res, 403, TEXT, 'Forbidden');
    return;
  }

  let file;
  try {
    file = await loadFile(filePath);
  } catch {
    send(res, 404, TEXT, 'Not found');
    return;
  }

  const headers = { 'Content-Type': file.type, 'Cache-Control': 'no-cache', ETag: file.etag };
  if (file.br) headers.Vary = 'Accept-Encoding';

  const ifNoneMatch = req.headers['if-none-match'] || '';
  if (ifNoneMatch.split(',').some((tag) => tag.trim().replace(/^W\//, '') === file.etag)) {
    send(res, 304, headers);
    return;
  }

  const encoding = pickEncoding(req, file);
  if (encoding) headers['Content-Encoding'] = encoding;
  const body = encoding ? file[encoding] : file.data;
  headers['Content-Length'] = body.length;
  send(res, 200, headers, body);
}

const server = http.createServer((req, res) => {
  if (req.method !== 'GET' && req.method !== 'HEAD') {
    send(res, 405, { ...TEXT, Allow: 'GET, HEAD' }, 'Method not allowed');
    return;
  }
  const { pathname } = new URL(req.url, 'http://localhost');
  if (pathname === '/healthz') {
    send(res, 200, { ...TEXT, 'Cache-Control': 'no-store' }, 'ok');
  } else {
    handleStatic(req, res, pathname).catch(() => {
      if (!res.headersSent) send(res, 500, TEXT, 'Internal server error');
    });
  }
});

server.listen(PORT, HOST, () => {
  console.log(`Connect läuft auf http://localhost:${PORT}`);
});

// Sauber beenden, wenn Docker/Coolify den Container stoppt oder neu startet
for (const signal of ['SIGTERM', 'SIGINT']) {
  process.on(signal, () => {
    server.close(() => process.exit(0));
    setTimeout(() => process.exit(0), 5000).unref();
  });
}
