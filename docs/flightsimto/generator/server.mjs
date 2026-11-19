// Podgląd UI z angielską atrapą backendu i prawdziwymi danymi z przebiegu e2e.
//   cargo test --release --lib -- --ignored krakow_pipeline   (w src-tauri, zapisuje %TEMP%/fsos-e2e/ui-data.json)
//   node server.mjs <ui dir> <ui-data.json> [port]
import http from 'node:http';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const ui = path.resolve(process.argv[2]);
const data = path.resolve(process.argv[3]);
const port = +(process.argv[4] || 5183);
const here = path.dirname(fileURLToPath(import.meta.url));
const types = { '.html': 'text/html; charset=utf-8', '.js': 'text/javascript; charset=utf-8', '.css': 'text/css', '.png': 'image/png', '.svg': 'image/svg+xml', '.json': 'application/json' };

http.createServer((req, res) => {
  const u = new URL(req.url, 'http://x');
  if (u.pathname === '/__data.json') {
    res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'max-age=3600' });
    return fs.createReadStream(data).pipe(res);
  }
  if (u.pathname === '/__mock.js') {
    res.writeHead(200, { 'content-type': types['.js'], 'cache-control': 'no-store' });
    return res.end(fs.readFileSync(path.join(here, 'mock-en.js')));
  }
  const p = path.join(ui, decodeURIComponent(u.pathname === '/' ? '/index.html' : u.pathname));
  if (!p.startsWith(ui) || !fs.existsSync(p)) { res.writeHead(404); return res.end(); }
  let body = fs.readFileSync(p);
  if (p.endsWith('index.html')) {
    body = String(body).replace('<script type="module" src="app.js">', '<script src="/__mock.js"></script><script type="module" src="app.js">');
  }
  res.writeHead(200, { 'content-type': types[path.extname(p)] || 'application/octet-stream', 'cache-control': 'no-store' });
  res.end(body);
}).listen(port, () => console.log(`preview on http://localhost:${port}`));
