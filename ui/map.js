// Jedna, współdzielona mapa Leaflet przenoszona między widokami (zachowuje widok i warstwy).
import { S, $, $$, esc, t, icon, invoke, num, debounce, saveProjectSoon, fail, toast, confirmBox, opener, dialogApi } from './core.js';

let map = null;
let box = null;
let tiles = null;
let areaLayer = null;
let vertexLayer = null;
let drawLayer = null;
let terrainLayer = null;
let footLayer = null;
let lightLayer = null;
let manualLayer = null;
let profileLayer = null;
let tool = 'pan';
let draw = null; // stan rysowania
let editing = false;
let onToolDone = null;
const canvas = () => L.canvas({ padding: 0.3 });

export function getMap() { return map; }

export function tileLayer(style) {
  const dark = document.documentElement.dataset.theme !== 'light';
  if (style === 'satellite') return L.tileLayer('https://server.arcgisonline.com/ArcGIS/rest/services/World_Imagery/MapServer/tile/{z}/{y}/{x}', { maxZoom: 19, attribution: 'Esri, Maxar, Earthstar Geographics' });
  if (style === 'osm') return L.tileLayer('https://tile.openstreetmap.org/{z}/{x}/{y}.png', { maxZoom: 19, attribution: '&copy; OpenStreetMap' });
  if (style === 'topo') return L.tileLayer('https://server.arcgisonline.com/ArcGIS/rest/services/World_Topo_Map/MapServer/tile/{z}/{y}/{x}', { maxZoom: 19, attribution: 'Esri, HERE, Garmin, &copy; OpenStreetMap' });
  return L.tileLayer(`https://server.arcgisonline.com/ArcGIS/rest/services/Canvas/${dark ? 'World_Dark_Gray_Base' : 'World_Light_Gray_Base'}/MapServer/tile/{z}/{y}/{x}`, { maxZoom: 16, attribution: 'Esri, HERE, Garmin, &copy; OpenStreetMap' });
}

export function setStyle(style) {
  if (!map) return;
  tiles?.remove();
  tiles = tileLayer(style).addTo(map);
  tiles.bringToBack();
}

function ensure() {
  if (map) return;
  box = document.createElement('div');
  box.id = 'mapbox';
  box.innerHTML = `<div id="map"></div>
    <div class="map-tools"></div>
    <div class="map-search"></div>
    <div class="map-layers"></div>
    <div class="map-hint" hidden></div>
    <div class="map-legend" hidden></div>`;
  document.body.append(box);
  map = L.map($('#map', box), { zoomControl: false, preferCanvas: true, worldCopyJump: true, doubleClickZoom: false }).setView([52.1, 19.4], 6);
  L.control.zoom({ position: 'bottomleft' }).addTo(map);
  L.control.scale({ position: 'bottomleft', imperial: false }).addTo(map);
  setStyle(S.settings?.mapStyle || 'dark');
  terrainLayer = L.layerGroup().addTo(map);
  footLayer = L.layerGroup().addTo(map);
  areaLayer = L.layerGroup().addTo(map);
  lightLayer = L.layerGroup().addTo(map);
  manualLayer = L.layerGroup().addTo(map);
  profileLayer = L.layerGroup().addTo(map);
  drawLayer = L.layerGroup().addTo(map);
  vertexLayer = L.layerGroup().addTo(map);
  map.on('mousedown', onDown);
  map.on('mousemove', onMove);
  map.on('mouseup', onUp);
  map.on('click', onClick);
  map.on('dblclick', onDbl);
  map.on('zoomend', () => restyleLights());
  document.addEventListener('keydown', (e) => {
    if (!draw || !box.isConnected) return;
    if (e.key === 'Escape') { cancelDraw(); }
    if (e.key === 'Enter' && draw.kind === 'polygon') finishPolygon();
    if (e.key === 'Backspace' && draw.kind === 'polygon' && draw.pts.length) { draw.pts.pop(); paintDraw(); e.preventDefault(); }
  });
  renderLayerSwitch();
}

function renderLayerSwitch() {
  const el = $('.map-layers', box);
  const st = S.settings?.mapStyle || 'dark';
  el.innerHTML = `<div class="seg">${['dark', 'topo', 'satellite', 'osm'].map((s) => `<button data-s="${s}" class="${s === st ? 'on' : ''}">${esc(t(`map_${s}`))}</button>`).join('')}</div>`;
  $$('button', el).forEach((b) => (b.onclick = async () => {
    S.settings.mapStyle = b.dataset.s;
    setStyle(b.dataset.s);
    renderLayerSwitch();
    try { await invoke('save_settings', { settings: S.settings }); } catch { /* bez znaczenia */ }
  }));
}

/** Wstawia mapę do miejsca w widoku i ustawia narzędzia dla tego widoku. */
export function mountMap(slot, opts = {}) {
  ensure();
  slot.append(box);
  setTimeout(() => map.invalidateSize(), 0);
  setTools(opts.tools || []);
  setSearch(!!opts.search);
  setHint('');
  editing = !!opts.edit;
  map.getContainer().classList.toggle('map-dim', !!opts.dim);
  drawArea();
  drawManual(!!opts.manual);
  if (!opts.keepLights) lightLayer.clearLayers();
  if (!opts.keepTerrain) terrainLayer.clearLayers();
  if (!opts.keepFoot) footLayer.clearLayers();
  profileLayer.clearLayers();
  setLegend(null);
  if (opts.fit !== false) fitArea(true);
  return map;
}

export function setHint(text) {
  const h = $('.map-hint', box);
  if (!h) return;
  h.hidden = !text;
  h.textContent = text || '';
}

export function setLegend(items) {
  const el = $('.map-legend', box);
  if (!el) return;
  el.hidden = !items?.length;
  el.innerHTML = (items || []).map((i) => `<span><i style="background:${i.color};box-shadow:0 0 6px ${i.color}"></i>${esc(i.label)}</span>`).join('');
}

// ───────── Narzędzia ─────────
const TOOL_DEF = {
  pan: { icon: 'map', tip: 'tool_pan' },
  rect: { icon: 'rect', tip: 'tool_rect' },
  polygon: { icon: 'polygon', tip: 'tool_polygon' },
  circle: { icon: 'target', tip: 'tool_circle' },
  import: { icon: 'fileUp', tip: 'tool_import' },
  fit: { icon: 'area', tip: 'tool_fit' },
  clear: { icon: 'trash', tip: 'tool_clear' },
  profile: { icon: 'chart', tip: 'tool_profile' },
  lamp: { icon: 'lamp', tip: 'tool_lamp' },
};

function setTools(list) {
  const el = $('.map-tools', box);
  const groups = [];
  let cur = [];
  for (const k of list) {
    if (k === '|') { groups.push(cur); cur = []; } else cur.push(k);
  }
  if (cur.length) groups.push(cur);
  el.innerHTML = groups.map((g) => `<div class="grp">${g.map((k) => `<button data-t="${k}" title="${esc(t(TOOL_DEF[k].tip))}" class="${tool === k ? 'on' : ''}">${icon(TOOL_DEF[k].icon)}</button>`).join('')}</div>`).join('');
  $$('button', el).forEach((b) => (b.onclick = () => useTool(b.dataset.t)));
  if (!list.includes(tool)) tool = 'pan';
}

function markTool() {
  $$('.map-tools button', box).forEach((b) => b.classList.toggle('on', b.dataset.t === tool));
}

export function useTool(k, done) {
  onToolDone = done || null;
  if (k === 'fit') return fitArea();
  if (k === 'clear') return clearArea();
  if (k === 'import') return importArea();
  cancelDraw();
  tool = k;
  markTool();
  map.getContainer().style.cursor = k === 'pan' ? '' : 'crosshair';
  const hints = { rect: 'hint_rect', polygon: 'hint_polygon', circle: 'hint_circle', profile: 'hint_profile', lamp: 'hint_lamp' };
  setHint(hints[k] ? t(hints[k]) : '');
  if (k !== 'pan') draw = { kind: k, pts: [] };
}

function cancelDraw() {
  draw = null;
  drawLayer.clearLayers();
  if (map) map.dragging.enable();
}

function endTool() {
  cancelDraw();
  tool = 'pan';
  markTool();
  map.getContainer().style.cursor = '';
  setHint('');
}

function onDown(e) {
  if (draw?.kind === 'rect' && e.originalEvent.button === 0) {
    map.dragging.disable();
    draw.start = e.latlng;
  }
}
function onMove(e) {
  if (!draw) return;
  draw.cursor = e.latlng;
  if (draw.kind === 'rect' && draw.start) paintDraw();
  if (draw.kind === 'polygon' && draw.pts.length) paintDraw();
  if (draw.kind === 'circle' && draw.center) paintDraw();
  if (draw.kind === 'profile' && draw.pts.length === 1) paintDraw();
}
function onUp(e) {
  if (draw?.kind === 'rect' && draw.start) {
    const a = draw.start;
    const b = e.latlng;
    map.dragging.enable();
    if (Math.abs(a.lat - b.lat) < 1e-5 || Math.abs(a.lng - b.lng) < 1e-5) { draw.start = null; return; }
    const ring = [[a.lat, a.lng], [a.lat, b.lng], [b.lat, b.lng], [b.lat, a.lng]];
    endTool();
    setArea(ring);
  }
}
function onClick(e) {
  if (!draw) return;
  if (draw.kind === 'polygon') {
    if (draw.pts.length >= 3) {
      const first = map.latLngToContainerPoint(draw.pts[0]);
      if (first.distanceTo(e.containerPoint) < 10) return finishPolygon();
    }
    draw.pts.push(e.latlng);
    paintDraw();
  } else if (draw.kind === 'circle') {
    if (!draw.center) { draw.center = e.latlng; return; }
    const r = map.distance(draw.center, e.latlng);
    const c = draw.center;
    endTool();
    invoke('area_circle', { lat: c.lat, lon: c.lng, radius: r }).then((a) => setArea(a.outer.map((p) => [p.lat, p.lon]))).catch(fail);
  } else if (draw.kind === 'profile') {
    draw.pts.push(e.latlng);
    if (draw.pts.length === 2) {
      const [a, b] = draw.pts;
      endTool();
      profileLayer.clearLayers();
      L.polyline([a, b], { color: '#fff', weight: 2, dashArray: '6 6' }).addTo(profileLayer);
      for (const p of [a, b]) L.circleMarker(p, { radius: 5, color: '#fff', weight: 2, fillColor: 'var(--acc)', fillOpacity: 1 }).addTo(profileLayer);
      onToolDone?.({ lat: a.lat, lon: a.lng }, { lat: b.lat, lon: b.lng });
    } else paintDraw();
  } else if (draw.kind === 'lamp') {
    onToolDone?.({ lat: e.latlng.lat, lon: e.latlng.lng });
  }
}
function onDbl() {
  if (draw?.kind === 'polygon') finishPolygon();
}
function finishPolygon() {
  if (!draw || draw.pts.length < 3) return;
  const ring = draw.pts.map((p) => [p.lat, p.lng]);
  endTool();
  setArea(ring);
}

function paintDraw() {
  drawLayer.clearLayers();
  const st = { color: getComputedStyle(document.documentElement).getPropertyValue('--acc').trim() || '#ffc35a', weight: 2, dashArray: '6 5', fillOpacity: 0.08 };
  if (draw.kind === 'rect' && draw.start && draw.cursor) L.rectangle([draw.start, draw.cursor], st).addTo(drawLayer);
  if (draw.kind === 'polygon') {
    const pts = draw.cursor ? [...draw.pts, draw.cursor] : draw.pts;
    L.polygon(pts, st).addTo(drawLayer);
    draw.pts.forEach((p) => L.circleMarker(p, { radius: 4, color: '#fff', weight: 2, fillColor: st.color, fillOpacity: 1 }).addTo(drawLayer));
  }
  if (draw.kind === 'circle' && draw.center && draw.cursor) {
    const r = map.distance(draw.center, draw.cursor);
    L.circle(draw.center, { ...st, radius: r }).addTo(drawLayer);
    setHint(`${t('hint_circle2')} ${num(r / 1000, 2)} km`);
  }
  if (draw.kind === 'profile' && draw.pts.length === 1 && draw.cursor) L.polyline([draw.pts[0], draw.cursor], { color: '#fff', weight: 2, dashArray: '6 6' }).addTo(drawLayer);
}

// ───────── Obszar projektu ─────────
const undoStack = [];
export function undoArea() {
  if (!S.project || !undoStack.length) return false;
  S.project.area = undoStack.pop();
  drawArea();
  saveProjectSoon(50);
  window.dispatchEvent(new CustomEvent('area-changed'));
  return true;
}

async function setArea(ring) {
  if (!S.project) return;
  undoStack.push(JSON.parse(JSON.stringify(S.project.area || { outer: [], holes: [] })));
  if (undoStack.length > 50) undoStack.shift();
  S.project.area = { outer: ring.map(([lat, lon]) => ({ lat, lon })), holes: [] };
  drawArea();
  fitArea();
  await saveProjectSoon(50);
  window.dispatchEvent(new CustomEvent('area-changed'));
}

async function clearArea() {
  if (!S.project?.area?.outer?.length) return;
  if (!(await confirmBox(t('clearAreaQ'), `<div>${esc(t('clearAreaD'))}</div>`, t('delete'), true))) return;
  undoStack.push(JSON.parse(JSON.stringify(S.project.area)));
  S.project.area = { outer: [], holes: [] };
  drawArea();
  await saveProjectSoon(50);
  window.dispatchEvent(new CustomEvent('area-changed'));
}

async function importArea() {
  const path = await dialogApi.open({ multiple: false, filters: [{ name: 'KML, GeoJSON, GPX, WKT', extensions: ['kml', 'geojson', 'json', 'gpx', 'wkt', 'txt'] }] });
  if (!path) return;
  try {
    const a = await invoke('area_import', { path });
    await setArea(a.outer.map((p) => [p.lat, p.lon]));
    toast(t('areaImported'));
  } catch (e) { fail(e); }
}

export function areaRing() {
  return (S.project?.area?.outer || []).map((p) => [p.lat, p.lon]);
}

export function drawArea() {
  if (!areaLayer) return;
  areaLayer.clearLayers();
  vertexLayer.clearLayers();
  const ring = areaRing();
  if (ring.length < 3) return;
  const acc = getComputedStyle(document.documentElement).getPropertyValue('--acc').trim() || '#ffc35a';
  const poly = L.polygon(ring, { color: acc, weight: 2.5, fillColor: acc, fillOpacity: editing ? 0.08 : 0.03, interactive: false }).addTo(areaLayer);
  if (!editing) return;
  // Wierzchołki do przeciągania, punkty środkowe do dodawania nowych, prawy klik usuwa.
  ring.forEach((p, i) => {
    const m = L.marker(p, { draggable: true, icon: L.divIcon({ className: '', html: '<div class="vx"></div>', iconSize: [12, 12], iconAnchor: [6, 6] }) }).addTo(vertexLayer);
    m.on('drag', (e) => { ring[i] = [e.latlng.lat, e.latlng.lng]; poly.setLatLngs(ring); });
    m.on('dragend', () => setArea(ring));
    m.on('contextmenu', () => { if (ring.length > 3) { ring.splice(i, 1); setArea(ring); } });
    const n = ring[(i + 1) % ring.length];
    const mid = [(p[0] + n[0]) / 2, (p[1] + n[1]) / 2];
    const mm = L.marker(mid, { draggable: true, icon: L.divIcon({ className: '', html: '<div class="vx mid"></div>', iconSize: [9, 9], iconAnchor: [4.5, 4.5] }) }).addTo(vertexLayer);
    mm.on('dragend', (e) => { ring.splice(i + 1, 0, [e.target.getLatLng().lat, e.target.getLatLng().lng]); setArea(ring); });
  });
}

export function fitArea(quiet) {
  const ring = areaRing();
  if (ring.length >= 3) map.fitBounds(L.latLngBounds(ring).pad(0.08), { animate: !quiet });
}

// ───────── Wyszukiwarka ─────────
function setSearch(on) {
  const el = $('.map-search', box);
  el.hidden = !on;
  if (!on || el.dataset.ready) return;
  el.dataset.ready = '1';
  el.innerHTML = `<div class="search">${icon('search')}<input placeholder="${esc(t('searchPlace'))}"><kbd>Enter</kbd></div><div class="ac-list" hidden></div>`;
  const input = $('input', el);
  const list = $('.ac-list', el);
  let results = [];
  const show = () => {
    list.hidden = !results.length;
    list.innerHTML = results.map((r, i) => `<div class="it" data-i="${i}">${icon(r.type === 'airport' ? 'globe' : 'pin')}<div class="grow"><div class="t">${esc(r.title)}</div><div class="d">${esc(r.sub)}</div></div></div>`).join('');
    $$('.it', list).forEach((d) => (d.onmousedown = (e) => { e.preventDefault(); pick(results[+d.dataset.i]); }));
  };
  const airports = async (q) => {
    if (q.length < 3) return [];
    const a = await invoke('search_airports', { query: q });
    return a.map((x) => ({ type: 'airport', title: `${x.icao} · ${x.name}`, sub: `${x.city || ''} ${x.country}`.trim(), lat: x.lat, lon: x.lon }));
  };
  const places = async (q) => {
    const p = await invoke('search_places', { query: q, lang: S.settings?.language || 'pl' });
    return p.map((x) => ({ type: 'place', title: x.name.split(',')[0], sub: x.name, lat: x.lat, lon: x.lon, area: x.area, bbox: x.bbox }));
  };
  const run = async (full) => {
    const q = input.value.trim();
    if (q.length < 2) { results = []; return show(); }
    try {
      const a = await airports(q);
      results = a;
      show();
      if (full || a.length === 0) {
        results = [...a.slice(0, 4), ...(await places(q))];
        show();
      }
    } catch (e) { fail(e); }
  };
  const live = debounce(() => run(false), 250);
  input.oninput = live;
  input.onkeydown = (e) => { if (e.key === 'Enter') run(true); if (e.key === 'Escape') { results = []; show(); } };
  input.onblur = () => setTimeout(() => { list.hidden = true; }, 150);
  input.onfocus = () => { if (results.length) list.hidden = false; };
  async function pick(r) {
    list.hidden = true;
    if (r.bbox) map.fitBounds([[r.bbox[0], r.bbox[1]], [r.bbox[2], r.bbox[3]]]);
    else map.setView([r.lat, r.lon], 13);
    if (!editing || !S.project) return;
    const has = (S.project.area?.outer?.length || 0) >= 3;
    if (r.type === 'airport') {
      if (has && !(await confirmBox(t('replaceAreaQ'), `<div>${esc(t('replaceAreaAirport', r.title))}</div>`, t('replace')))) return;
      const a = await invoke('area_circle', { lat: r.lat, lon: r.lon, radius: 5000 });
      await setArea(a.outer.map((p) => [p.lat, p.lon]));
    } else if (r.area?.outer?.length >= 3) {
      if (!(await confirmBox(t('usePlaceQ'), `<div>${esc(t('usePlaceD', r.title))}</div>`, t('useOutline')))) return;
      await setArea(r.area.outer.map((p) => [p.lat, p.lon]));
    }
  }
}

// ───────── Nakładki ─────────
export function setTerrainOverlay(url, bbox, opacity = 0.85) {
  terrainLayer.clearLayers();
  if (!url) return;
  L.imageOverlay(url, [[bbox.south, bbox.west], [bbox.north, bbox.east]], { opacity, interactive: false }).addTo(terrainLayer);
}
export function setTerrainOpacity(o) {
  terrainLayer.eachLayer((l) => l.setOpacity?.(o));
}

export function setFootprints(list) {
  footLayer.clearLayers();
  for (const f of list) {
    const b = f.bbox;
    L.rectangle([[b.south, b.west], [b.north, b.east]], { color: '#5cc4ff', weight: 1.5, dashArray: '4 4', fillOpacity: 0.05, interactive: false }).addTo(footLayer);
    L.tooltip({ permanent: true, direction: 'center', className: 'foot-tip' }).setLatLng([b.north, (b.west + b.east) / 2]).setContent(esc(f.name)).addTo(footLayer);
  }
}

let dots = [];
let dotsFilter = null;
function radius() {
  const z = map.getZoom();
  return z < 12 ? 1.4 : z < 14 ? 2.2 : z < 16 ? 3.2 : z < 17 ? 4.5 : 6;
}
function restyleLights() {
  const r = radius();
  lightLayer.eachLayer((l) => l.setRadius?.(l.options.big ? r * 1.8 : r));
}

/** Kropki świateł: [lat, lon, heading, cat, src, kolor, natężenie, osm, alt, styl, wysokość, błyska]. */
export function setLights(list, filter, onExclude) {
  dots = list || [];
  dotsFilter = filter;
  lightLayer.clearLayers();
  const rend = canvas();
  const r = radius();
  for (const d of dots) {
    if (dotsFilter && !dotsFilter(d)) continue;
    const big = d[9] === 'obstruction' || d[9] === 'beacon' || d[9] === 'turbine' || d[9] === 'flood';
    const m = L.circleMarker([d[0], d[1]], { renderer: rend, radius: big ? r * 1.8 : r, stroke: d[4] === 'o', color: '#fff', weight: d[4] === 'o' ? 1 : 0, fillColor: d[5], fillOpacity: 0.95, big });
    m.on('click', () => {
      const osmType = d[3] === 'mapped' || d[4] === 'o' && (d[9] === 'obstruction' || d[9] === 'beacon') ? 'node' : 'way';
      const html = `<div class="pop"><h4><span class="swatch" style="--sw:${d[5]}"></span>${esc(t(`lc_${d[3]}`))}</h4>
        <div class="kv2"><span>${esc(t('source'))}</span><b>${esc(t(`src_${d[4]}`))}</b>
        <span>${esc(t('style'))}</span><b>${esc(t(`style_${d[9]}`))}</b>
        ${d[10] ? `<span>${esc(t('height'))}</span><b>${num(d[10], 1)} m</b>` : ''}
        ${d[8] ? `<span>${esc(t('altAgl'))}</span><b>${num(d[8], 1)} m</b>` : ''}
        <span>${esc(t('heading'))}</span><b>${num(d[2])}°</b>
        <span>${esc(t('intensity'))}</span><b>${num(d[6])}${d[11] ? ` · ${esc(t('flashing'))}` : ''}</b></div>
        <div class="acts">${d[7] ? `<button class="btn sm" data-a="osm">${icon('external')}OSM</button>` : ''}${d[4] === 'o' && d[3] === 'mapped' && onExclude ? `<button class="btn sm danger" data-a="ex">${icon('eyeoff')}${esc(t('exclude'))}</button>` : ''}</div></div>`;
      const pop = L.popup({ maxWidth: 320 }).setLatLng([d[0], d[1]]).setContent(html).openOn(map);
      const el = pop.getElement();
      $('[data-a="osm"]', el)?.addEventListener('click', () => opener.openUrl(`https://www.openstreetmap.org/${osmType}/${d[7]}`));
      $('[data-a="ex"]', el)?.addEventListener('click', () => { map.closePopup(); onExclude(d); });
    });
    m.addTo(lightLayer);
  }
}

export function refilterLights(filter) { setLights(dots, filter); }

/** Lampy postawione ręcznie (białe pierścienie, klik – usuń/obróć). */
export function drawManual(show) {
  manualLayer.clearLayers();
  if (!show || !S.project) return;
  (S.project.manual || []).forEach((m, i) => {
    const mk = L.circleMarker([m.lat, m.lon], { radius: 7, color: '#fff', weight: 2, fillColor: '#ffc35a', fillOpacity: 0.9 }).addTo(manualLayer);
    mk.on('click', () => {
      const html = `<div class="pop"><h4>${icon('lamp')}${esc(t('manualLamp'))}</h4><div class="kv2"><span>${esc(t('category'))}</span><b>${esc(t(`lc_${m.category}`))}</b></div>
        <label class="field" style="margin-top:8px">${esc(t('heading'))} (°)<input class="inp" type="number" min="0" max="359" value="${Math.round(m.heading)}"></label>
        <div class="acts"><button class="btn sm danger" data-a="del">${icon('trash')}${esc(t('delete'))}</button></div></div>`;
      const pop = L.popup().setLatLng([m.lat, m.lon]).setContent(html).openOn(map);
      const el = pop.getElement();
      $('input', el).oninput = (e) => { m.heading = +e.target.value || 0; saveProjectSoon(); };
      $('[data-a="del"]', el).onclick = () => { S.project.manual.splice(i, 1); saveProjectSoon(50); map.closePopup(); drawManual(true); };
    });
  });
}

export function clearProfile() { profileLayer?.clearLayers(); }
