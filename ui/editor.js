// Widoki projektu: Obszar, Teren, Światła, Przeszkody i roślinność, Budowanie.
import {
  S, $, $$, esc, t, icon, invoke, num, bytes, debounce, toast, fail, dialog, confirmBox, saveProjectSoon, saveProjectNow,
  bindInputs, startJob, projectJob, stageInfo, opener, dialogApi, renderView, flushSave,
} from './core.js';
import { mountMap, setTerrainOverlay, setTerrainOpacity, setFootprints, setLights, setLegend, useTool, drawManual, getMap, clearProfile, setHint, areaRing } from './map.js';
import { lang } from './i18n.js';

const shell = (panel, tools = 'workspace') => `<div class="${tools}"><div class="mapslot"></div><aside class="panel">${panel}</aside></div>`;
const sw = (bind) => `<button class="switch" data-bind="${bind}" aria-label="${esc(bind)}"></button>`;
const unit = (bind, u, attrs = '') => `<span class="unit"><input class="inp" data-num data-bind="${bind}" ${attrs}><span>${esc(u)}</span></span>`;
const frow = (label, ctl, hint = '') => `<div class="frow"><span class="fl">${esc(label)}</span>${ctl}${hint ? `<span class="fh">${esc(hint)}</span>` : ''}</div>`;
const sec = (ic, title, body, open = true) => `<details class="sec" ${open ? 'open' : ''}><summary>${icon(ic)}${esc(title)}${icon('chevron', 'chev')}</summary><div class="sb">${body}</div></details>`;
const kpi = (k, v, small = '') => `<div class="kpi"><div class="k">${esc(k)}</div><div class="v">${v}${small ? ` <small>${esc(small)}</small>` : ''}</div></div>`;
const note = (kind, text, ic) => `<div class="note ${kind}">${icon(ic || (kind === 'warn' ? 'warn' : kind === 'bad' ? 'error' : kind === 'ok' ? 'ok' : 'info'))}<div>${text}</div></div>`;
const busy = () => !!projectJob();

function noProject(view) {
  view.innerHTML = `<div class="empty">${icon('box')}<h3>${esc(t('noProject'))}</h3><div>${esc(t('noProjectD'))}</div><div class="empty-actions"><button class="btn primary" id="np-new">${icon('plus')}${esc(t('newProject'))}</button></div></div>`;
  $('#np-new').onclick = () => window.dispatchEvent(new CustomEvent('new-project'));
}

function hasArea() {
  return (S.project?.area?.outer?.length || 0) >= 3;
}

function needArea(view) {
  if (hasArea()) return false;
  view.insertAdjacentHTML('afterbegin', '');
  return true;
}

function areaKm2() {
  const ring = areaRing();
  if (ring.length < 3) return 0;
  const lat0 = ring.reduce((s, p) => s + p[0], 0) / ring.length;
  const k = 111.32;
  let a = 0;
  for (let i = 0; i < ring.length; i++) {
    const [y1, x1] = ring[i];
    const [y2, x2] = ring[(i + 1) % ring.length];
    a += x1 * Math.cos((lat0 * Math.PI) / 180) * k * (y2 * k) - x2 * Math.cos((lat0 * Math.PI) / 180) * k * (y1 * k);
  }
  return Math.abs(a / 2);
}

// ═════════════ Obszar ═════════════
export function renderArea(view) {
  if (!S.project) return noProject(view);
  const p = S.project;
  view.classList.add('flush');
  const km2 = areaKm2();
  view.innerHTML = shell(`
    <div class="ph"><h2>${icon('area')}${esc(t('nav_area'))}</h2><p>${esc(t('areaLead'))}</p></div>
    <div class="pscroll">
      ${sec('edit', t('projectInfo'), `
        <label class="field">${esc(t('projectName'))}<input class="inp full" data-bind="name"></label>
        <label class="field">${esc(t('projectNote'))}<textarea class="inp full" rows="2" data-bind="note"></textarea></label>`)}
      ${sec('polygon', t('drawArea'), `
        <p class="lead">${esc(t('drawAreaD'))}</p>
        <div class="btn-row">
          <button class="btn sm" data-tool="rect">${icon('rect')}${esc(t('tool_rect'))}</button>
          <button class="btn sm" data-tool="polygon">${icon('polygon')}${esc(t('tool_polygon'))}</button>
          <button class="btn sm" data-tool="circle">${icon('target')}${esc(t('tool_circle'))}</button>
          <button class="btn sm" data-tool="import">${icon('fileUp')}${esc(t('tool_import'))}</button>
          <button class="btn sm" id="ar-view">${icon('map')}${esc(t('areaFromView'))}</button>
        </div>
        <div class="sublabel">${esc(t('aroundAirport'))}</div>
        <div class="btn-row"><input class="inp" id="ar-icao" placeholder="EPKK" style="width:110px;text-transform:uppercase">
          <span class="unit"><input class="inp" id="ar-rad" value="5"><span>km</span></span>
          <button class="btn sm" id="ar-apt">${icon('target')}${esc(t('apply'))}</button></div>
        <p class="lead">${esc(t('editHint'))}</p>`)}
      ${sec('ruler', t('areaStats'), hasArea() ? `
        <div class="kpis">${kpi(t('areaKm2'), num(km2, km2 < 10 ? 2 : 1), 'km²')}${kpi(t('vertices'), num(p.area.outer.length))}<div id="ar-plan" style="display:contents"></div></div>
        ${km2 > 2500 ? note('bad', esc(t('areaTooBigOsm'))) : km2 > 400 ? note('warn', esc(t('areaBig'))) : ''}` : note('info', esc(t('noAreaYet'))))}
    </div>
    <div class="pfoot"><span class="grow"></span><button class="btn primary" id="ar-next" ${hasArea() ? '' : 'disabled'}>${esc(t('nextTerrain'))}${icon('chevron')}</button></div>`);
  mountMap($('.mapslot', view), { tools: ['pan', '|', 'rect', 'polygon', 'circle', '|', 'import', 'fit', 'clear'], search: true, edit: true });
  bindInputs(view, (path) => { if (path === 'name') window.dispatchEvent(new CustomEvent('project-renamed')); });
  $$('[data-tool]', view).forEach((b) => (b.onclick = () => useTool(b.dataset.tool)));
  $('#ar-view').onclick = () => {
    const b = getMap().getBounds().pad(-0.1);
    S.project.area = { outer: [b.getSouthWest(), b.getSouthEast(), b.getNorthEast(), b.getNorthWest()].map((x) => ({ lat: x.lat, lon: x.lng })), holes: [] };
    saveProjectSoon(50).then(() => renderView());
  };
  $('#ar-apt').onclick = async () => {
    const code = $('#ar-icao').value.trim().toUpperCase();
    const r = parseFloat($('#ar-rad').value.replace(',', '.')) || 5;
    const hits = await invoke('search_airports', { query: code });
    const a = hits.find((h) => h.icao === code) || hits[0];
    if (!a) return toast(t('airportNotFound'), 'warn');
    const area = await invoke('area_circle', { lat: a.lat, lon: a.lon, radius: r * 1000 });
    S.project.area = area;
    if (!S.project.name || S.project.name === t('newProjectName')) S.project.name = `${a.icao} ${a.name}`;
    await saveProjectSoon(50);
    renderView();
  };
  $('#ar-next').onclick = () => window.dispatchEvent(new CustomEvent('go', { detail: 'terrain' }));
  if (hasArea()) invoke('terrain_plan', { project: S.project }).then((pl) => {
    const el = $('#ar-plan');
    if (el) el.innerHTML = kpi(t('tiles'), num(pl.tiles)) + kpi(t('xmlSize'), num(pl.xmlMb, 1), 'MB');
  }).catch(() => {});
}

// ═════════════ Teren ═════════════
const SOURCES = {
  files: { icon: 'file', res: '' },
  gugik: { icon: 'database', res: '1 m' },
  wcs: { icon: 'link', res: '' },
  terrarium: { icon: 'globe', res: '3–30 m' },
  copernicus30: { icon: 'globe', res: '30 m' },
  copernicus90: { icon: 'globe', res: '90 m' },
};
const POLAND = { south: 48.64, west: 13.7, north: 55.04, east: 24.87 };
function inPoland() {
  const r = areaRing();
  return r.length && r.every(([la, lo]) => la >= POLAND.south && la <= POLAND.north && lo >= POLAND.west && lo <= POLAND.east);
}

const fileInfo = new Map();

export function renderTerrain(view) {
  if (!S.project) return noProject(view);
  const p = S.project;
  const tc = p.terrain;
  view.classList.add('flush');
  const all = Object.keys(SOURCES);
  const order = [...tc.sources.filter((s) => all.includes(s)), ...all.filter((s) => !tc.sources.includes(s))];
  // Nazwy układów z backendu są po polsku – w angielskim interfejsie tłumaczymy dwa słowa.
  const crsName = (n) => (lang() === 'pl' ? n : n.replace('(stopnie)', '(degrees)').replace('strefa', 'zone'));
  const crsOpts = (sel) => `<option value="0" ${!sel ? 'selected' : ''}>${esc(t('auto'))}</option>${S.info.crs.map(([c, n]) => `<option value="${c}" ${+sel === c ? 'selected' : ''}>EPSG:${c} · ${esc(crsName(n))}</option>`).join('')}`;
  const ti = stageInfo('terrain');
  view.innerHTML = shell(`
    <div class="ph"><h2>${icon('terrain')}${esc(t('nav_terrain'))}<span class="grow"></span>${sw('terrain.enabled')}</h2><p>${esc(t('terrainLead'))}</p></div>
    <div class="pscroll">
      ${hasArea() ? '' : note('warn', esc(t('needArea')))}
      ${sec('layers', t('dataSources'), `
        <p class="lead">${esc(t('sourcesLead'))}</p>
        <div class="src-list">${order.map((s, i) => {
          const on = tc.sources.includes(s);
          const out = s === 'gugik' && hasArea() && !inPoland();
          return `<div class="src ${on ? '' : 'off'}" data-s="${s}"><span class="no">${on ? tc.sources.indexOf(s) + 1 : '–'}</span>
            <div class="grow"><div class="t">${esc(t(`srcname_${s}`))} ${SOURCES[s].res ? `<span class="tag">${SOURCES[s].res}</span>` : ''}</div><div class="d">${esc(t(`srcdesc_${s}`))}${out ? ` · <span class="c-bad">${esc(t('outsidePoland'))}</span>` : ''}</div></div>
            <div class="ord"><button data-up="${s}" ${i === 0 ? 'disabled' : ''}>▲</button><button data-down="${s}">▼</button></div>
            <button class="switch ${on ? 'on' : ''}" data-src="${s}"></button></div>`;
        }).join('')}</div>
        <label class="toggle-row"><div class="grow"><div class="t">${esc(t('fillGaps'))}</div><div class="d">${esc(t('fillGapsD'))}</div></div>${sw('terrain.fillGaps')}</label>`)}
      ${sec('fileUp', t('ownFiles'), `
        <p class="lead">${esc(t('ownFilesD'))}</p>
        <div id="tf-files" class="src-list"></div>
        <div class="btn-row"><button class="btn sm" id="tf-add">${icon('plus')}${esc(t('addFiles'))}</button></div>`, tc.files.length > 0 || tc.sources.includes('files'))}
      ${sec('grid', t('gridSettings'), `
        ${frow(t('resolution'), `<select class="inp" data-bind="terrain.resolution" data-num>${[1, 2, 3, 5, 7.5, 10, 15, 20, 30].map((r) => `<option value="${r}">${num(r, r % 1 ? 1 : 0)} m</option>`).join('')}</select>`, t('resolutionD'))}
        ${frow(t('tileSize'), `<select class="inp" data-bind="terrain.tile" data-num>${[32, 64, 128, 192, 256].map((n) => `<option value="${n}">${n} × ${n}</option>`).join('')}</select>`, t('tileSizeD'))}
        ${frow(t('falloff'), unit('terrain.falloff', 'm', 'min="0" max="2000"'), t('falloffD'))}
        ${frow(t('priority'), unit('terrain.priority', '', 'min="0" max="50"'), t('priorityD'))}
        <div id="tf-plan" class="kpis"></div><div id="tf-planw"></div>`)}
      ${sec('sliders', t('processing'), `
        ${frow(t('smooth'), unit('terrain.smooth', 'm', 'min="0" max="60"'), t('smoothD'))}
        ${frow(t('vOffset'), unit('terrain.offset', 'm', 'min="-200" max="200"'), t('vOffsetD'))}
        <label class="toggle-row"><div class="grow"><div class="t">${esc(t('hydro'))}</div><div class="d">${esc(t('hydroD'))}</div></div>${sw('terrain.hydro')}</label>
        <label class="toggle-row"><div class="grow"><div class="t">${esc(t('flattenRunways'))}</div><div class="d">${esc(t('flattenRunwaysD'))}</div></div>${sw('terrain.flattenRunways')}</label>
        <label class="toggle-row"><div class="grow"><div class="t">${esc(t('skipFlat'))}</div><div class="d">${esc(t('skipFlatD'))}</div></div>${sw('terrain.skipFlat')}</label>
        ${frow(t('skipThreshold'), unit('terrain.skipThreshold', 'm', 'min="0" max="10"'))}`, false)}
      ${sec('link', t('wcsAdvanced'), `
        <p class="lead">${esc(t('wcsD'))}</p>
        <label class="field">URL<input class="inp full" data-bind="terrain.wcsUrl" placeholder="https://…/wcs"></label>
        <div class="grid2"><label>${esc(t('coverage'))}<input class="inp" data-bind="terrain.wcsCoverage"></label><label>EPSG<input class="inp" data-num data-bind="terrain.wcsEpsg" placeholder="2180"></label></div>`, false)}
      <div id="tf-result"></div>
    </div>
    <div class="pfoot">
      <button class="btn" id="tf-dl" ${busy() || !hasArea() ? 'disabled' : ''}>${icon('download')}${esc(t('downloadData'))}</button>
      <span class="grow"></span>
      <button class="btn primary" id="tf-run" ${busy() || !hasArea() ? 'disabled' : ''}>${icon('cpu')}${esc(t('computeTerrain'))}</button>
    </div>`);
  mountMap($('.mapslot', view), { tools: ['pan', '|', 'profile', 'fit'], keepTerrain: false });
  bindInputs(view, (path) => { if (path.startsWith('terrain.')) refreshPlan(); });
  $$('[data-src]', view).forEach((b) => (b.onclick = () => {
    const s = b.dataset.src;
    const on = !tc.sources.includes(s);
    if (on) {
      const idx = order.indexOf(s);
      const after = order.slice(0, idx).filter((x) => tc.sources.includes(x));
      const pos = after.length ? tc.sources.indexOf(after.at(-1)) + 1 : 0;
      tc.sources.splice(pos, 0, s);
    } else tc.sources = tc.sources.filter((x) => x !== s);
    saveProjectSoon(100);
    renderTerrain(view);
  }));
  const move = (s, d) => {
    const idx = order.indexOf(s);
    const j = idx + d;
    if (j < 0 || j >= order.length) return;
    [order[idx], order[j]] = [order[j], order[idx]];
    tc.sources = order.filter((x) => tc.sources.includes(x));
    saveProjectSoon(100);
    renderTerrain(view);
  };
  $$('[data-up]', view).forEach((b) => (b.onclick = () => move(b.dataset.up, -1)));
  $$('[data-down]', view).forEach((b) => (b.onclick = () => move(b.dataset.down, 1)));
  $('#tf-add').onclick = addFiles;
  $('#tf-dl').onclick = () => startJob('dem');
  $('#tf-run').onclick = () => startJob('terrain');
  renderFiles(crsOpts);
  refreshPlan();
  loadTerrainResult();
  // Profil wysokości – narzędzie mapy.
  const toolBtn = $('.map-tools [data-t="profile"]');
  if (toolBtn) toolBtn.onclick = () => useTool('profile', showProfile);
  void ti;
}

function renderFiles(crsOpts) {
  const el = $('#tf-files');
  if (!el) return;
  const tc = S.project.terrain;
  if (!tc.files.length) {
    el.innerHTML = `<div class="note">${icon('info')}<div>${esc(t('noFiles'))}</div></div>`;
    setFootprints([]);
    return;
  }
  el.innerHTML = tc.files.map((f, i) => {
    const info = fileInfo.get(`${f.path}|${f.epsg}|${f.swapXy}`);
    const name = f.path.split(/[\\/]/).pop();
    const meta = info?.error ? `<span class="c-bad">${esc(info.error)}</span>` : info ? `${num(info.w)} × ${num(info.h)} · ${num(info.resolution, info.resolution < 10 ? 2 : 0)} m · EPSG:${info.epsg} · ${num(info.min, 0)}–${num(info.max, 0)} ${t('unitAsl')}` : esc(t('reading'));
    return `<div class="file-row"><div class="top">${icon('file')}<span class="name" title="${esc(f.path)}">${esc(name)}</span><button class="switch ${f.enabled ? 'on' : ''}" data-fon="${i}"></button><button class="btn sm icon ghost" data-fdel="${i}" title="${esc(t('remove'))}">${icon('trash')}</button></div>
      <div class="meta">${meta}</div>
      <div class="grid3"><label>${esc(t('crs'))}<select class="inp" data-fepsg="${i}">${crsOpts(f.epsg)}</select></label>
      <label>${esc(t('vOffset'))}<input class="inp" data-foff="${i}" value="${f.offset || 0}"></label>
      <label>${esc(t('swapXy'))}<select class="inp" data-fswap="${i}"><option value="0">X Y</option><option value="1" ${f.swapXy ? 'selected' : ''}>Y X</option></select></label></div></div>`;
  }).join('');
  const update = (i, fn) => { fn(tc.files[i]); saveProjectSoon(100); renderFiles(crsOpts); probeFiles(crsOpts); };
  $$('[data-fon]', el).forEach((b) => (b.onclick = () => update(+b.dataset.fon, (f) => (f.enabled = !f.enabled))));
  $$('[data-fdel]', el).forEach((b) => (b.onclick = () => update(+b.dataset.fdel, () => tc.files.splice(+b.dataset.fdel, 1))));
  $$('[data-fepsg]', el).forEach((s) => (s.onchange = () => update(+s.dataset.fepsg, (f) => (f.epsg = +s.value))));
  $$('[data-fswap]', el).forEach((s) => (s.onchange = () => update(+s.dataset.fswap, (f) => (f.swapXy = s.value === '1'))));
  $$('[data-foff]', el).forEach((s) => (s.onchange = () => { tc.files[+s.dataset.foff].offset = parseFloat(s.value.replace(',', '.')) || 0; saveProjectSoon(); }));
  probeFiles(crsOpts);
}

async function probeFiles(crsOpts) {
  const tc = S.project.terrain;
  let changed = false;
  for (const f of tc.files) {
    const key = `${f.path}|${f.epsg}|${f.swapXy}`;
    if (fileInfo.has(key)) continue;
    fileInfo.set(key, null);
    try {
      fileInfo.set(key, await invoke('dem_file_info', { path: f.path, epsg: f.epsg, swapXy: !!f.swapXy }));
    } catch (e) { fileInfo.set(key, { error: String(e) }); }
    changed = true;
  }
  const foot = tc.files.map((f) => ({ f, i: fileInfo.get(`${f.path}|${f.epsg}|${f.swapXy}`) })).filter((x) => x.i?.footprint).map((x) => ({ name: x.f.path.split(/[\\/]/).pop(), bbox: x.i.footprint }));
  setFootprints(foot);
  if (changed && S.view === 'terrain') renderFiles(crsOpts);
}

async function addFiles() {
  const paths = await dialogApi.open({ multiple: true, filters: [{ name: t('demFiles'), extensions: ['tif', 'tiff', 'asc', 'xyz', 'txt', 'hgt', 'zip'] }] });
  if (!paths?.length) return;
  const tc = S.project.terrain;
  for (const path of [].concat(paths)) {
    if (tc.files.some((f) => f.path === path)) continue;
    tc.files.push({ id: Math.random().toString(36).slice(2, 10), path, epsg: 0, swapXy: false, offset: 0, enabled: true });
  }
  if (!tc.sources.includes('files')) tc.sources.unshift('files');
  await saveProjectNow();
  renderView();
}

const refreshPlan = debounce(async () => {
  const el = $('#tf-plan');
  if (!el || !hasArea()) return;
  try {
    const pl = await invoke('terrain_plan', { project: S.project });
    el.innerHTML = kpi(t('tiles'), num(pl.tiles)) + kpi(t('tileEdge'), num(pl.tileSizeM), 'm') + kpi(t('samples'), num(pl.samples / 1e6, 1), t('unitMln')) + kpi(t('xmlSize'), num(pl.xmlMb, 0), 'MB');
    $('#tf-planw').innerHTML = pl.tooBig ? note('bad', esc(t('planTooBig'))) : pl.xmlMb > 400 ? note('warn', esc(t('planLarge'))) : '';
  } catch { /* plan to tylko podpowiedź */ }
}, 250);

// Nazwy źródeł przychodzą z backendu – znane tłumaczymy, pliki użytkownika zostają bez zmian.
const srcLabel = (n) => (n === 'Wody OSM (spłaszczone)' || n === 'osm-water' ? t('srcLakesFlat') : n === 'NMT GUGiK' ? t('srcname_gugik') : n);

async function loadTerrainResult() {
  const el = $('#tf-result');
  if (!el) return;
  const [sum, prev] = await Promise.all([invoke('terrain_summary', { id: S.project.id }).catch(() => null), invoke('terrain_preview', { id: S.project.id }).catch(() => null)]);
  S.terrain = sum;
  if (!$('#tf-result')) return;
  if (!sum) {
    el.innerHTML = note('info', esc(t('terrainNotComputed')));
    return;
  }
  if (prev) setTerrainOverlay(prev.url, prev.bbox, 0.85);
  el.innerHTML = sec('chart', t('terrainResult'), `
    <div class="kpis">${kpi(t('tiles'), num(sum.tiles))}${kpi(t('minH'), num(sum.min, 0), 'm')}${kpi(t('maxH'), num(sum.max, 0), 'm')}${kpi(t('xmlSize'), num(sum.xmlMb, 0), 'MB')}
      ${sum.skippedFlat ? kpi(t('skippedFlat'), num(sum.skippedFlat)) : ''}${sum.skippedSea ? kpi(t('skippedSea'), num(sum.skippedSea)) : ''}${sum.skippedNodata ? kpi(t('skippedNodata'), num(sum.skippedNodata)) : ''}${sum.lakes ? kpi(t('lakes'), num(sum.lakes)) : ''}${sum.runways ? kpi(t('runways'), num(sum.runways)) : ''}</div>
    ${(sum.runwayInfo || []).length ? `<div class="sublabel">${esc(t('runwaysFlattened'))}</div><div class="chips">${sum.runwayInfo.map((r) => `<span class="tag">${esc(r)}</span>`).join('')}</div>` : ''}
    <div class="sublabel">${esc(t('sourceShare'))}</div>
    <div class="share">${sum.sources.map(([n, pct]) => `<div class="sr"><span>${esc(srcLabel(n))}</span><span class="pct">${num(pct, 1)}%</span><div class="bar"><span style="width:${pct}%"></span></div></div>`).join('')}</div>
    ${(sum.warnings || []).map((w) => note('warn', esc(w))).join('')}
    <div class="sublabel">${esc(t('preview'))}</div>
    <div class="range-row"><input type="range" min="0" max="100" value="85" id="tf-op"><output id="tf-opv">85%</output></div>
    <div class="btn-row"><button class="btn sm" id="tf-prof">${icon('chart')}${esc(t('tool_profile'))}</button><button class="btn sm" id="tf-3d">${icon('cube')}${esc(t('nav_view3d'))}</button><button class="btn sm" id="tf-tif">${icon('download')}GeoTIFF</button></div>
    <div id="tf-profile" class="profile"></div>
    <div class="faint">${esc(t('computedAt'))}: ${esc(sum.computed)}</div>`);
  $('#tf-op').oninput = (e) => { setTerrainOpacity(e.target.value / 100); $('#tf-opv').textContent = `${e.target.value}%`; };
  $('#tf-prof').onclick = () => useTool('profile', showProfile);
  $('#tf-3d').onclick = () => window.dispatchEvent(new CustomEvent('go', { detail: 'view3d' }));
  $('#tf-tif').onclick = async () => {
    const path = await dialogApi.save({ defaultPath: `${S.project.package.name || 'teren'}.tif`, filters: [{ name: 'GeoTIFF', extensions: ['tif'] }] });
    if (!path) return;
    try { const [w, h] = await invoke('export_terrain_tiff', { id: S.project.id, path }); toast(t('tiffSaved', `${num(w)} × ${num(h)}`)); } catch (e) { fail(e); }
  };
}

async function showProfile(a, b) {
  const el = $('#tf-profile');
  if (!el) return;
  try {
    const pts = await invoke('terrain_profile', { id: S.project.id, a, b });
    if (pts.length < 2) { el.innerHTML = note('warn', esc(t('profileNoData'))); return; }
    const W = 380;
    const H = 150;
    const maxD = pts.at(-1)[0];
    const hs = pts.map((p) => p[1]);
    const lo = Math.min(...hs);
    const hi = Math.max(...hs, lo + 1);
    const x = (d) => 30 + (d / maxD) * (W - 40);
    const y = (h) => 10 + (1 - (h - lo) / (hi - lo)) * (H - 30);
    const path = pts.map((p, i) => `${i ? 'L' : 'M'}${x(p[0]).toFixed(1)},${y(p[1]).toFixed(1)}`).join('');
    el.innerHTML = `<svg viewBox="0 0 ${W} ${H}"><path d="${path} L${x(maxD)},${H - 20} L30,${H - 20} Z" fill="color-mix(in srgb, var(--acc) 18%, transparent)"/><path d="${path}" fill="none" stroke="var(--acc)" stroke-width="2"/>
      <text class="axis" x="2" y="14">${num(hi, 0)} m</text><text class="axis" x="2" y="${H - 22}">${num(lo, 0)} m</text><text class="axis" x="${W - 60}" y="${H - 4}">${num(maxD / 1000, 2)} km</text></svg>`;
  } catch (e) { fail(e); }
}

// ═════════════ Światła ═════════════
const CAT_ORDER = ['mapped', 'motorway', 'primary', 'secondary', 'residential', 'service', 'paths', 'parking', 'pitches', 'platforms', 'fuel', 'piers'];
const STYLES = ['cobra', 'double', 'post_top', 'wall', 'flood', 'none'];
const SPACED = new Set(['motorway', 'primary', 'secondary', 'residential', 'service', 'paths', 'parking', 'platforms', 'fuel', 'piers']);
const ROADS = new Set(['motorway', 'primary', 'secondary', 'residential', 'service']);
const KELVIN = [[2000, 'kSodium'], [2700, 'kWarm'], [3000, 'kLedWarm'], [4000, 'kNeutral'], [5000, 'kCool']];
let catOpen = new Set(['mapped']);
let catFilter = null;
let manualCat = 'residential';

export function kelvinHex(k) {
  const tt = Math.min(Math.max(k, 1000), 40000) / 100;
  const r = tt <= 66 ? 255 : 329.698727446 * (tt - 60) ** -0.1332047592;
  const g = tt <= 66 ? 99.4708025861 * Math.log(tt) - 161.1195681661 : 288.1221695283 * (tt - 60) ** -0.0755148492;
  const b = tt >= 66 ? 255 : tt <= 19 ? 0 : 138.5177312231 * Math.log(tt - 10) - 305.0447927307;
  const h = (v) => Math.round(Math.min(255, Math.max(0, v))).toString(16).padStart(2, '0');
  return `#${h(r)}${h(g)}${h(b)}`;
}

export function renderLights(view) {
  if (!S.project) return noProject(view);
  const p = S.project;
  view.classList.add('flush');
  const osm = stageInfo('osm');
  const li = stageInfo('lights');
  const counts = S.lights?.counts || {};
  const cats = p.lights.categories;
  const card = (id) => {
    const c = cats[id];
    if (!c) return '';
    const open = catOpen.has(id);
    const fields = `
      <div class="grid3">
        <label>${esc(t('height'))} (m)<input class="inp" data-num data-bind="lights.categories.${id}.height" min="1" max="60"></label>
        ${SPACED.has(id) ? `<label>${esc(t('spacing'))} (m)<input class="inp" data-num data-bind="lights.categories.${id}.spacing" min="4" max="200"></label>` : '<span></span>'}
        <label>${esc(t('intensity'))}<input class="inp" data-num data-bind="lights.categories.${id}.intensity" min="0.5" max="5000"></label>
        <label>${esc(t('kelvin'))} (K)<input class="inp" data-num data-bind="lights.categories.${id}.kelvin" min="1500" max="8000"></label>
        <label>${esc(t('cone'))} (°)<input class="inp" data-num data-bind="lights.categories.${id}.cone" min="10" max="360"></label>
        <label>${esc(t('style'))}<select class="inp" data-bind="lights.categories.${id}.style">${STYLES.map((s) => `<option value="${s}">${esc(t(`style_${s}`))}</option>`).join('')}</select></label>
        ${id !== 'pitches' && id !== 'fuel' ? `<label>${esc(t('arm'))} (m)<input class="inp" data-num data-bind="lights.categories.${id}.arm" min="0" max="6"></label>` : ''}
        ${ROADS.has(id) || id === 'paths' || id === 'pitches' ? `<label>${esc(t('offset'))} (m)<input class="inp" data-num data-bind="lights.categories.${id}.offset" min="0" max="15"></label>` : ''}
      </div>
      <div class="kelvin">${KELVIN.map(([k, n]) => `<button data-k="${id}:${k}" style="--sw:${kelvinHex(k)}"><i></i>${esc(t(n))}</button>`).join('')}</div>
      ${ROADS.has(id) ? `<label class="toggle-row"><div class="grow"><div class="t">${esc(t('bothSides'))}</div><div class="d">${esc(t('bothSidesD'))}</div></div>${sw(`lights.categories.${id}.bothSides`)}</label>` : ''}`;
    return `<div class="cat ${c.enabled ? '' : 'off'}" data-cat="${id}">
      <div class="ch" data-open="${id}"><span class="swatch" style="--sw:${kelvinHex(c.kelvin)}"></span><div class="grow"><div class="t">${esc(t(`lc_${id}`))}</div><div class="d">${esc(t(`lcd_${id}`))}</div></div>
        ${counts[id] ? `<span class="n">${num(counts[id])}</span>` : ''}<button class="switch ${c.enabled ? 'on' : ''}" data-con="${id}"></button></div>
      ${open ? `<div class="cb">${fields}</div>` : ''}</div>`;
  };
  view.innerHTML = shell(`
    <div class="ph"><h2>${icon('lights')}${esc(t('nav_lights'))}<span class="grow"></span>${sw('lights.enabled')}</h2><p>${esc(t('lightsLead'))}</p></div>
    <div class="pscroll">
      ${hasArea() ? '' : note('warn', esc(t('needArea')))}
      ${sec('globe', t('osmData'), `
        ${osm ? note('ok', esc(t('osmFetched', osm.at, num(+osm.rest[0])))) : note('info', esc(t('osmNotFetched')))}
        <div class="btn-row"><button class="btn sm" id="li-osm" ${busy() || !hasArea() ? 'disabled' : ''}>${icon('download')}${esc(osm ? t('refetchOsm') : t('fetchOsm'))}</button></div>`)}
      ${sec('palette', t('lightPresets'), `
        <p class="lead">${esc(t('lightPresetsD'))}</p>
        <div class="kelvin">${KELVIN.map(([k, n]) => `<button data-kall="${k}" style="--sw:${kelvinHex(k)}"><i></i>${esc(t(n))}</button>`).join('')}</div>
        <div class="btn-row"><button class="btn sm" data-int="0.8">${icon('minus')}${esc(t('dimmer'))}</button><button class="btn sm" data-int="1.25">${icon('plus')}${esc(t('brighter'))}</button></div>`, false)}
      ${sec('sliders', t('lightOptions'), `
        <label class="toggle-row"><div class="grow"><div class="t">${esc(t('guessUrban'))}</div><div class="d">${esc(t('guessUrbanD'))}</div></div>${sw('lights.guessUrban')}</label>
        ${frow(t('avoidMapped'), unit('lights.avoidMapped', 'm', 'min="0" max="80"'), t('avoidMappedD'))}
        <label class="toggle-row"><div class="grow"><div class="t">${esc(t('farLod'))}</div><div class="d">${esc(t('farLodD'))}</div></div>${sw('lights.farLod')}</label>
        <label class="toggle-row"><div class="grow"><div class="t">${esc(t('flipArm'))}</div><div class="d">${esc(t('flipArmD'))}</div></div>${sw('lights.flipArm')}</label>`, false)}
      <div class="sublabel">${esc(t('categories'))}</div>
      ${CAT_ORDER.map(card).join('')}
      ${sec('lamp', t('manualLamps'), `
        <p class="lead">${esc(t('manualLampsD'))}</p>
        <div class="btn-row"><select class="inp" id="li-mcat">${CAT_ORDER.filter((c) => c !== 'mapped').map((c) => `<option value="${c}" ${c === manualCat ? 'selected' : ''}>${esc(t(`lc_${c}`))}</option>`).join('')}</select>
        <button class="btn sm" id="li-mtool">${icon('plus')}${esc(t('tool_lamp'))}</button></div>
        <div class="faint">${esc(t('manualCount', num((p.manual || []).length)))}</div>`, (p.manual || []).length > 0)}
      <div id="li-result"></div>
    </div>
    <div class="pfoot"><button class="btn" id="li-geo" ${li ? '' : 'disabled'}>${icon('download')}GeoJSON</button><span class="grow"></span>
      <button class="btn primary" id="li-run" ${busy() || !hasArea() ? 'disabled' : ''}>${icon('wand')}${esc(osm ? t('generateLights') : t('fetchAndGenerate'))}</button></div>`);
  mountMap($('.mapslot', view), { tools: ['pan', '|', 'lamp', 'fit'], dim: true, manual: true, keepLights: true });
  bindInputs(view, (path, val, el) => {
    const m = path.match(/^lights\.categories\.(\w+)\.kelvin$/);
    if (m) $(`.cat[data-cat="${m[1]}"] .swatch`, view)?.style.setProperty('--sw', kelvinHex(val));
  });
  $$('[data-open]', view).forEach((h) => (h.onclick = (e) => {
    if (e.target.closest('.switch')) return;
    const id = h.dataset.open;
    if (catOpen.has(id)) catOpen.delete(id); else catOpen.add(id);
    renderLights(view);
  }));
  $$('[data-con]', view).forEach((b) => (b.onclick = () => { const c = cats[b.dataset.con]; c.enabled = !c.enabled; saveProjectSoon(100); renderLights(view); }));
  $$('[data-k]', view).forEach((b) => (b.onclick = () => { const [id, k] = b.dataset.k.split(':'); cats[id].kelvin = +k; saveProjectSoon(100); renderLights(view); }));
  $$('[data-kall]', view).forEach((b) => (b.onclick = () => {
    for (const id of CAT_ORDER) if (id !== 'pitches' && id !== 'fuel') cats[id].kelvin = +b.dataset.kall;
    saveProjectSoon(100);
    renderLights(view);
  }));
  $$('[data-int]', view).forEach((b) => (b.onclick = () => {
    for (const id of CAT_ORDER) cats[id].intensity = Math.round(cats[id].intensity * +b.dataset.int * 10) / 10;
    saveProjectSoon(100);
    toast(t('intensityChanged'));
    renderLights(view);
  }));
  $('#li-osm').onclick = () => startJob('osm');
  $('#li-run').onclick = () => {
    if (!osm) { window.__chain = { project: S.project.id, after: 'osm', next: 'lights' }; startJob('osm'); } else startJob('lights');
  };
  $('#li-mcat').onchange = (e) => (manualCat = e.target.value);
  const addLamp = () => useTool('lamp', (pt) => {
    S.project.manual = S.project.manual || [];
    S.project.manual.push({ lat: pt.lat, lon: pt.lon, heading: 0, category: manualCat });
    saveProjectSoon(200);
    drawManual(true);
  });
  $('#li-mtool').onclick = addLamp;
  const mt = $('.map-tools [data-t="lamp"]');
  if (mt) mt.onclick = addLamp;
  $('#li-geo').onclick = async () => {
    const path = await dialogApi.save({ defaultPath: `${S.project.package.name || 'lights'}.geojson`, filters: [{ name: 'GeoJSON', extensions: ['geojson'] }] });
    if (!path) return;
    try { toast(t('exported', num(await invoke('export_lights_geojson', { id: S.project.id, path })))); } catch (e) { fail(e); }
  };
  loadLights(view, (d) => CAT_ORDER.includes(d[3]));
}

async function loadLights(view, filter) {
  try {
    S.lights = await invoke('lights_data', { id: S.project.id });
  } catch { S.lights = null; }
  const el = $('#li-result', view) || $('#ob-result', view);
  const data = S.lights;
  if (!data?.dots?.length) {
    setLights([], null);
    if (el) el.innerHTML = note('info', esc(t('lightsNotGenerated')));
    return;
  }
  const exclude = async (d) => {
    S.project.excluded = [...new Set([...(S.project.excluded || []), d[7]])];
    await saveProjectNow();
    toast(t('excludedLamp'));
    S.lights.dots = S.lights.dots.filter((x) => !(x[3] === 'mapped' && x[7] === d[7]));
    setLights(S.lights.dots, (x) => filter(x) && (!catFilter || x[3] === catFilter), exclude);
  };
  setLights(data.dots, (d) => filter(d) && (!catFilter || d[3] === catFilter), exclude);
  const shown = Object.entries(data.counts).filter(([k]) => filter([0, 0, 0, k]));
  const legend = shown.map(([k]) => {
    const d = data.dots.find((x) => x[3] === k);
    return { color: d?.[5] || '#fc6', label: `${t(`lc_${k}`)} · ${num(data.counts[k])}` };
  });
  setLegend(legend);
  if (!el) return;
  const total = shown.reduce((s, [, n]) => s + n, 0);
  el.innerHTML = sec('chart', t('lightsResult'), `
    <div class="kpis">${kpi(t('lightsTotal'), num(total))}${kpi(t('models'), num(data.models))}</div>
    <div class="chips"><button class="chip ${!catFilter ? 'on' : ''}" data-f="">${esc(t('all'))}</button>${shown.map(([k, n]) => `<button class="chip ${catFilter === k ? 'on' : ''}" data-f="${k}">${esc(t(`lc_${k}`))}<span class="n">${num(n)}</span></button>`).join('')}</div>
    <p class="lead">${esc(t('lightsMapHint'))}</p>`);
  $$('[data-f]', el).forEach((b) => (b.onclick = () => { catFilter = b.dataset.f || null; loadLights(view, filter); }));
}

// ═════════════ Przeszkody i roślinność ═════════════
export function renderObjects(view) {
  if (!S.project) return noProject(view);
  view.classList.add('flush');
  const tog = (bind, label, desc) => `<label class="toggle-row"><div class="grow"><div class="t">${esc(t(label))}</div>${desc ? `<div class="d">${esc(t(desc))}</div>` : ''}</div>${sw(bind)}</label>`;
  view.innerHTML = shell(`
    <div class="ph"><h2>${icon('objects')}${esc(t('nav_objects'))}</h2><p>${esc(t('objectsLead'))}</p></div>
    <div class="pscroll">
      ${hasArea() ? '' : note('warn', esc(t('needArea')))}
      ${sec('mast', t('obstacles'), `
        ${tog('obstacles.enabled', 'obstaclesOn', 'obstaclesOnD')}
        <div class="sublabel">${esc(t('obstacleKinds'))}</div>
        ${tog('obstacles.masts', 'obMasts')}${tog('obstacles.chimneys', 'obChimneys')}${tog('obstacles.towers', 'obTowers')}${tog('obstacles.turbines', 'obTurbines', 'obTurbinesD')}
        ${tog('obstacles.turbineModels', 'obTurbineModels', 'obTurbineModelsD')}${tog('obstacles.cranes', 'obCranes')}${tog('obstacles.powerTowers', 'obPower', 'obPowerD')}
        ${tog('obstacles.lighthouses', 'obLighthouses', 'obLighthousesD')}
        <div class="sublabel">${esc(t('obstacleLights'))}</div>
        ${frow(t('minHeight'), unit('obstacles.minHeight', 'm', 'min="0" max="500"'), t('minHeightD'))}
        ${frow(t('defaultHeight'), unit('obstacles.defaultHeight', 'm', 'min="0" max="500"'), t('defaultHeightD'))}
        ${frow(t('intensity'), unit('obstacles.intensity', '', 'min="0.5" max="500"'))}
        ${tog('obstacles.flashing', 'obFlashing', 'obFlashingD')}${tog('obstacles.intermediate', 'obIntermediate', 'obIntermediateD')}`)}
      ${sec('tree', t('vegetation'), `
        ${tog('vegetation.enabled', 'vegOn', 'vegOnD')}
        ${tog('vegetation.roads', 'vegRoads')}${tog('vegetation.railways', 'vegRail')}${tog('vegetation.water', 'vegWater')}${tog('vegetation.runways', 'vegRunways')}
        ${tog('vegetation.pitches', 'vegPitches')}${tog('vegetation.parking', 'vegParking')}${tog('vegetation.landuse', 'vegLanduse', 'vegLanduseD')}
        ${frow(t('vegMargin'), unit('vegetation.margin', 'm', 'min="0" max="30"'))}
        ${note('info', esc(t('vegNote')))}`)}
      <div id="ob-result"></div>
    </div>
    <div class="pfoot"><span class="grow"></span><button class="btn primary" id="ob-run" ${busy() || !hasArea() ? 'disabled' : ''}>${icon('wand')}${esc(t('generateObjects'))}</button></div>`);
  mountMap($('.mapslot', view), { tools: ['pan', '|', 'fit'], dim: true, keepLights: true });
  bindInputs(view);
  $('#ob-run').onclick = () => {
    if (!stageInfo('osm')) { window.__chain = { project: S.project.id, after: 'osm', next: 'lights' }; startJob('osm'); } else startJob('lights');
  };
  loadLights(view, (d) => ['obstacle', 'turbine', 'lighthouse'].includes(d[3]));
}

// ═════════════ Budowanie ═════════════
export async function renderBuild(view) {
  if (!S.project) return noProject(view);
  const p = S.project;
  view.classList.remove('flush');
  const [env, pkg, tsum, lights] = await Promise.all([
    invoke('env_status').catch(() => null),
    invoke('package_status', { id: p.id }).catch(() => null),
    invoke('terrain_summary', { id: p.id }).catch(() => null),
    invoke('lights_data', { id: p.id }).catch(() => null),
  ]);
  if (S.view !== 'build' || S.project?.id !== p.id) return;
  S.env = env;
  // SDK do MSFS 2020 (wersja 0.x) nie zbuduje pakietu 2024 – traktujemy je jak brak SDK.
  const sdkOk = !!env?.sdk && !env.sdk.legacy;
  S.pkg = pkg;
  const job = projectJob();
  const chk = (ok, title, desc, warn) => `<div class="check ${ok ? 'ok' : warn ? 'warn' : 'bad'}">${icon(ok ? 'ok' : warn ? 'warn' : 'error')}<div class="grow"><div class="t">${title}</div>${desc ? `<div class="d">${desc}</div>` : ''}</div></div>`;
  const nLights = lights?.dots?.length || 0;
  const osm = stageInfo('osm');
  view.innerHTML = `<div class="settings">
    <div class="card set-card"><h3>${icon('packages')}${esc(t('packageInfo'))}</h3>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('pkgName'))}</div><div class="d">${esc(t('pkgNameD'))}</div></div><input class="inp" data-bind="package.name" placeholder="fsos-scenery-…"></div>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('pkgTitle'))}</div><div class="d">${esc(t('pkgTitleD'))}</div></div><input class="inp" data-bind="package.title"></div>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('pkgCreator'))}</div></div><input class="inp" data-bind="package.creator"></div>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('pkgVersion'))}</div><div class="d">${esc(t('pkgVersionD'))}</div></div><input class="inp" data-bind="package.version" placeholder="1.0.0"></div>
    </div>
    <div class="card set-card"><h3>${icon('ok')}${esc(t('readiness'))}</h3><div class="set-row"><div class="grow">
      ${chk(hasArea(), esc(t('chkArea')), hasArea() ? esc(t('areaKm2Val', num(areaKm2(), 2))) : esc(t('needArea')))}
      ${chk(!!osm || !(p.lights.enabled || p.obstacles.enabled || p.vegetation.enabled), esc(t('chkOsm')), osm ? esc(t('osmFetched', osm.at, num(+osm.rest[0]))) : esc(t('osmNotFetched')), true)}
      ${p.terrain.enabled ? chk(!!tsum, esc(t('chkTerrain')), tsum ? esc(t('terrainReady', num(tsum.tiles), tsum.computed)) : esc(t('terrainNotComputed')), true) : ''}
      ${p.lights.enabled || p.obstacles.enabled ? chk(nLights > 0, esc(t('chkLights')), nLights ? esc(t('lightsReady', num(nLights), num(lights.models))) : esc(t('lightsNotGenerated')), true) : ''}
      ${chk(!!env?.installs?.length, esc(t('chkMsfs')), env?.installs?.length ? esc(env.installs.map((i) => i.label).join(' · ')) : esc(t('msfsMissing')), true)}
      ${chk(!!env?.communityOk, esc(t('chkCommunity')), esc(env?.community || t('communityMissing')), true)}
      ${chk(sdkOk, esc(t('chkSdk')), sdkOk ? esc(env.sdk.packageTool) : `${esc(env?.sdk?.legacy ? t('sdkLegacy', env.sdk.version, env.sdk.root) : t('sdkMissing'))} <a href="#" id="bd-sdkdoc">${esc(t('sdkHow'))}</a>`, true)}
      ${chk(!env?.simRunning, esc(t('chkSimClosed')), env?.simRunning ? esc(t('simRunningD')) : '')}
    </div></div></div>
    <div class="card set-card"><h3>${icon('build')}${esc(t('buildTitle'))}</h3>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('buildAll'))}</div><div class="d">${esc(t('buildAllD'))}</div></div><button class="btn primary" id="bd-all" ${job || !hasArea() ? 'disabled' : ''}>${icon('rocket')}${esc(t('buildAllBtn'))}</button></div>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('buildOnly'))}</div><div class="d">${esc(t('buildOnlyD'))}</div></div>
        <div class="btn-row"><button class="btn" id="bd-build" ${job || !hasArea() ? 'disabled' : ''}>${icon('build')}${esc(t('build'))}</button><button class="btn" id="bd-binst" ${job || !hasArea() || !sdkOk ? 'disabled' : ''}>${icon('download')}${esc(t('buildInstall'))}</button></div></div>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('testPkg'))}</div><div class="d">${esc(t('testPkgD'))}${pkg?.testBuilt ? ` · <span class="c-ok">${esc(t('testBuilt'))}</span>` : ''}</div></div>
        <div class="btn-row"><button class="btn sm" id="bd-test" ${job || !hasArea() ? 'disabled' : ''}>${icon('lamp')}${esc(sdkOk ? t('testBuildInstall') : t('testSources'))}</button><button class="btn sm icon" id="bd-testdir" title="${esc(t('openSources'))}" ${pkg?.testDir ? '' : 'disabled'}>${icon('folder')}</button></div></div>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('installTitle'))}</div><div class="d">${pkg?.installed ? `${esc(t('installedAt'))}: ${esc(pkg.installed)}${pkg.linked ? ` (${esc(t('linked'))})` : ''}` : pkg?.built ? esc(t('builtNotInstalled', bytes(pkg.builtBytes))) : esc(t('notBuilt'))}</div></div>
        <div class="btn-row">${pkg?.installed ? `<button class="btn danger" id="bd-uninst">${icon('trash')}${esc(t('uninstall'))}</button>` : ''}<button class="btn" id="bd-inst" ${pkg?.built && !job ? '' : 'disabled'}>${icon('download')}${esc(pkg?.installed ? t('reinstall') : t('install'))}</button></div></div>
      <div class="set-row"><div class="grow"><div class="t">${esc(t('filesTitle'))}</div><div class="d">${esc(t('filesD'))}</div></div>
        <div class="btn-row"><button class="btn sm" id="bd-work" ${pkg?.sources ? '' : 'disabled'}>${icon('folder')}${esc(t('openSources'))}</button><button class="btn sm" id="bd-pkg" ${pkg?.built ? '' : 'disabled'}>${icon('folder')}${esc(t('openPackage'))}</button>
        <button class="btn sm" id="bd-zip" ${pkg?.built ? '' : 'disabled'}>${icon('zip')}ZIP</button><button class="btn sm" id="bd-exp">${icon('save')}${esc(t('exportProject'))}</button></div></div>
      <div class="set-row"><div class="grow"><div class="sublabel">${esc(t('buildLog'))}</div><div class="console" id="bd-log"></div></div></div>
    </div></div>`;
  bindInputs(view);
  $('#bd-sdkdoc')?.addEventListener('click', (e) => { e.preventDefault(); opener.openUrl('https://docs.flightsimulator.com/msfs2024/'); });
  $('#bd-all').onclick = () => startJob('all', { install: sdkOk });
  $('#bd-build').onclick = () => startJob('build');
  $('#bd-test').onclick = () => startJob('testlights', { install: sdkOk });
  $('#bd-testdir').onclick = () => opener.openPath(pkg.testDir);
  $('#bd-binst').onclick = () => startJob('build', { install: true });
  $('#bd-inst').onclick = async () => {
    try { const path = await invoke('install_package', { id: p.id }); toast(t('installedOk', path)); renderBuild(view); } catch (e) { fail(e); }
  };
  $('#bd-uninst')?.addEventListener('click', async () => {
    if (!(await confirmBox(t('uninstallQ'), `<div>${esc(t('uninstallD', pkg.installed))}</div>`, t('uninstall'), true))) return;
    try { await invoke('uninstall_package', { id: p.id }); toast(t('uninstalled')); renderBuild(view); } catch (e) { fail(e); }
  });
  $('#bd-work').onclick = () => opener.openPath(pkg.workDir);
  $('#bd-pkg').onclick = () => opener.openPath(pkg.builtPath);
  $('#bd-zip').onclick = async () => {
    const path = await dialogApi.save({ defaultPath: `${pkg.name}.zip`, filters: [{ name: 'ZIP', extensions: ['zip'] }] });
    if (!path) return;
    try { toast(t('zipDone', bytes(await invoke('export_zip', { id: p.id, path })))); } catch (e) { fail(e); }
  };
  $('#bd-exp').onclick = async () => {
    const path = await dialogApi.save({ defaultPath: `${pkg?.name || 'projekt'}.fsos.json`, filters: [{ name: 'FS Opendata Scenery', extensions: ['json'] }] });
    if (!path) return;
    try { await invoke('export_project', { id: p.id, path }); toast(t('projectExported')); } catch (e) { fail(e); }
  };
  const lines = S.logs.get(p.id);
  if (lines?.length) paintLog(lines);
  else invoke('build_log', { id: p.id }).then((txt) => { if (txt) paintLog(txt.split('\n')); else $('#bd-log').innerHTML = `<span class="faint">${esc(t('logEmpty'))}</span>`; }).catch(() => {});
}

export function paintLog(lines) {
  const el = $('#bd-log');
  if (!el) return;
  const cls = (l) => (/error|failed|błąd/i.test(l) ? 'err' : /^> /.test(l) ? 'cmd' : /success|gotowe|completed/i.test(l) ? 'ok' : '');
  el.innerHTML = lines.slice(-2000).map((l) => `<div class="${cls(l)}">${esc(l)}</div>`).join('');
  el.scrollTop = el.scrollHeight;
}

export { needArea, flushSave, setHint, clearProfile };
