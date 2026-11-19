// Plakat w 1080p i 4K z poster.html: node poster.mjs <outdir>
import { chromium } from 'playwright-core';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const out = path.resolve(process.argv[2] || '.');
fs.mkdirSync(out, { recursive: true });
const src = pathToFileURL(path.join(path.dirname(fileURLToPath(import.meta.url)), 'poster.html')).href;
const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe' });
for (const [scale, name] of [[1, 'FSOpendataScenery-poster-1920x1080.png'], [2, 'FSOpendataScenery-poster-3840x2160.png']]) {
  const page = await browser.newPage({ viewport: { width: 1920, height: 1080 }, deviceScaleFactor: scale });
  await page.goto(src);
  await page.waitForLoadState('networkidle');
  await page.waitForTimeout(500);
  await page.screenshot({ path: path.join(out, name) });
  console.log('ok', name);
  await page.close();
}
await browser.close();
