// Zrzuty prezentacyjne: node shoot.mjs <outdir> [scena...]   (najpierw: node server.mjs ui <ui-data.json>)
// Wymaga playwright-core i Microsoft Edge.
import { chromium } from 'playwright-core';
import fs from 'node:fs';

const out = process.argv[2];
const only = process.argv.slice(3);
fs.mkdirSync(out, { recursive: true });
const BASE = process.env.BASE || 'http://localhost:5183/';
const browser = await chromium.launch({ executablePath: 'C:/Program Files (x86)/Microsoft/Edge/Application/msedge.exe', args: ['--use-angle=d3d11', '--enable-webgl', '--ignore-gpu-blocklist'] });
const ctx = await browser.newContext({ viewport: { width: 1600, height: 900 }, deviceScaleFactor: 1.2, locale: 'en-GB', timezoneId: 'Europe/Warsaw' });

const settle = async (page, ms = 900) => {
  await page.waitForTimeout(ms);
  await page.waitForLoadState('networkidle').catch(() => {});
  await page.waitForTimeout(400);
};
const nav = async (p, v) => { await p.click(`.nav-btn[data-v="${v}"]`); await settle(p, 1200); };
const LANG = process.env.LANG_UI === 'pl' ? 'lang=pl' : '';
const open = async (p, q = '') => { await p.goto(BASE + (q ? q + (LANG ? '&' + LANG : '') : LANG ? '?' + LANG : '')); await settle(p, 1500); };
// Przybliżenie mapy Leaflet kółkiem w danym punkcie.
const zoom = async (p, x, y, steps) => {
  await p.mouse.move(x, y);
  for (let k = 0; k < Math.abs(steps); k++) { await p.mouse.wheel(0, steps > 0 ? -120 : 120); await p.waitForTimeout(450); }
  await settle(p, 2500);
};

const x2 = async (p) => {
  await p.locator('button', { hasText: '×2' }).first().click(); await settle(p, 1500);
  // Bliżej: Wawel i Stare Miasto wypełniają kadr.
  await p.mouse.move(820, 470); for (let k = 0; k < 4; k++) { await p.mouse.wheel(0, -200); await p.waitForTimeout(250); }
  await settle(p, 2000);
};
// Rynek Główny w dopasowanym widoku obszaru (mapa 1600×900, obszar e2e).
const RYNEK = [588, 315];
const scenes = {
  '01-lights-night': async (p) => { await open(p); await nav(p, 'lights'); },
  '02-terrain-lidar': async (p) => { await open(p); await nav(p, 'terrain'); },
  '03-3d-night': async (p) => { await p.addInitScript(() => localStorage.setItem('fsos-3d-night', '1')); await open(p); await nav(p, 'view3d'); await x2(p); },
  '04-3d-day': async (p) => { await p.addInitScript(() => localStorage.setItem('fsos-3d-night', '0')); await open(p); await nav(p, 'view3d'); await x2(p); },
  '05-dashboard': async (p) => { await open(p); },
  '06-area': async (p) => { await open(p); await nav(p, 'area'); },
  '07-lights-satellite': async (p) => { await open(p); await nav(p, 'lights'); await p.locator('button', { hasText: process.env.LANG_UI === 'pl' ? 'Satelita' : 'Satellite' }).first().click(); await settle(p, 1500); await zoom(p, RYNEK[0], RYNEK[1], 2); },
  '08-build': async (p) => { await open(p); await nav(p, 'build'); },
  '09-terrain-light': async (p) => { await open(p, '?theme=light&map=light'); await nav(p, 'terrain'); },
  '10-objects': async (p) => { await open(p); await nav(p, 'objects'); },
  '11-settings': async (p) => { await open(p); await nav(p, 'settings'); },
};

for (const [name, run] of Object.entries(scenes)) {
  if (only.length && !only.includes(name)) continue;
  const page = await ctx.newPage();
  page.on('console', (m) => { if (m.type() === 'error') console.log(name, 'console:', m.text()); });
  page.on('pageerror', (e) => console.log(name, 'pageerror:', e.message));
  await run(page);
  await settle(page, 1200);
  await page.mouse.move(1595, 895);
  await page.screenshot({ path: `${out}/${name}.png` });
  console.log('ok', name);
  await page.close();
}
await browser.close();
