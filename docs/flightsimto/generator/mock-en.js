// Atrapa backendu Tauri do zrzutów prezentacyjnych: angielski interfejs, projekt „Kraków – Old Town”
// z prawdziwymi danymi z przebiegu e2e (/__data.json), pozostałe projekty są przykładowe.
(() => {
  const qs = new URLSearchParams(location.search);
  // ?lang=pl – polski interfejs i polskie nazwy projektów (zrzuty na redzikowski.com).
  const PL = qs.get('lang') === 'pl';
  const xhr = new XMLHttpRequest();
  xhr.open('GET', '/__data.json', false);
  xhr.send();
  const D = JSON.parse(xhr.responseText);
  const rect = (s, w, n, e) => [[s, w], [s, e], [n, e], [n, w]].map(([lat, lon]) => ({ lat, lon }));
  const area = (r) => {
    const lat0 = r[0].lat * Math.PI / 180; let a = 0;
    for (let i = 0; i < r.length; i++) { const p = r[i]; const q = r[(i + 1) % r.length]; a += (p.lon * Math.cos(lat0) * q.lat - q.lon * Math.cos(lat0) * p.lat); }
    return Math.abs(a / 2) * 111.32 * 111.32;
  };
  const center = (r) => ({ lat: r.reduce((s, p) => s + p.lat, 0) / r.length, lon: r.reduce((s, p) => s + p.lon, 0) / r.length });

  const sum = D.terrainSummary;
  sum.computed = '2026-11-19 20:17:31';
  const krk = D.project;
  Object.assign(krk, {
    id: 'krk', name: 'Kraków – Old Town & Wawel', note: 'Real street lamps from OSM + 1 m LiDAR terrain', created: '2026-11-18 21:02:11', updated: '2026-11-19 20:18:02',
    status: { osm: '2026-11-19 20:16:48|12124', dem: '2026-11-19 20:17:02|6|41.3', terrain: `2026-11-19 20:17:31|${sum.tiles}|${Math.round(sum.min)}|${Math.round(sum.max)}`, lights: `2026-11-19 20:17:40|${D.lights.dots.length}|${D.lights.models}` },
  });
  krk.package = { ...(krk.package || {}), name: 'fsos-scenery-krakow-old-town', title: 'Kraków – Old Town & Wawel (lights + LiDAR terrain)', creator: 'FS Opendata Scenery', version: '1.0.0' };
  krk.vegetation = { ...(krk.vegetation || {}), enabled: true };

  const samples = [
    { id: 'epkk', name: 'EPKK Kraków-Balice – LiDAR terrain', note: 'Runway 07/25 flattened', ring: rect(50.066, 19.752, 50.092, 19.823), status: { osm: '2026-11-17 19:40:12|5212', terrain: '2026-11-17 19:44:51|118|219|276', lights: '2026-11-17 19:45:30|1288|31', build: '2026-11-17 19:52:10|ok|61234112' }, pkg: 'fsos-scenery-epkk-terrain' },
    { id: 'zak', name: 'Zakopane & Gubałówka', note: 'Mountain terrain, 2 m grid', ring: [[49.283, 19.925], [49.283, 19.995], [49.318, 20.002], [49.322, 19.948], [49.305, 19.918]].map(([lat, lon]) => ({ lat, lon })), status: { osm: '2026-11-16 18:02:44|8810', terrain: '2026-11-16 18:09:20|204|781|1123', lights: '2026-11-16 18:10:02|2410|38' }, pkg: 'fsos-scenery-zakopane' },
    { id: 'gdn', name: 'Gdańsk – Motława waterfront', note: '', ring: rect(54.343, 18.640, 54.360, 18.672), status: { osm: '2026-11-15 22:11:09|9634', lights: '2026-11-15 22:12:30|2875|44' }, pkg: 'fsos-scenery-gdansk-waterfront' },
    { id: 'hel', name: 'Hel Peninsula lighthouses', note: 'Obstruction lights and lighthouses only', ring: rect(54.585, 18.700, 54.640, 18.830), status: { osm: '2026-11-14 20:30:00|2140', lights: '2026-11-14 20:31:12|64|9' }, pkg: 'fsos-scenery-hel-lighthouses' },
  ];
  const projects = { krk };
  for (const s of samples) projects[s.id] = { ...JSON.parse(JSON.stringify(krk)), id: s.id, name: s.name, note: s.note, area: { outer: s.ring, holes: [] }, status: s.status, updated: s.status.lights?.split('|')[0] || s.status.osm.split('|')[0], package: { ...krk.package, name: s.pkg, title: s.name } };
  if (PL) {
    const names = { krk: ['Kraków – Stare Miasto i Wawel', 'Prawdziwe latarnie z OSM i teren z LiDAR 1 m'], epkk: ['EPKK Kraków-Balice – teren LiDAR', 'Wyrównany pas 07/25'], zak: ['Zakopane i Gubałówka', 'Teren górski, siatka 2 m'], gdn: ['Gdańsk – nabrzeże Motławy', ''], hel: ['Półwysep Helski – latarnie morskie', 'Tylko światła przeszkodowe i latarnie'] };
    for (const [id, [n, note]] of Object.entries(names)) { projects[id].name = n; projects[id].note = note; projects[id].package.title = n; }
  }
  const card = (p) => ({ id: p.id, name: p.name, note: p.note, updated: p.updated, areaKm2: area(p.area.outer), center: center(p.area.outer), outer: p.area.outer, status: p.status, package: p.package.name });

  // Profil wysokości z prawdziwej siatki podglądu 3D.
  const g = D.preview3d.grid;
  const zAt = (lat, lon) => {
    const b = g.bbox;
    const x = Math.round((lon - b.west) / (b.east - b.west) * (g.w - 1));
    const y = Math.round((b.north - lat) / (b.north - b.south) * (g.h - 1));
    if (x < 0 || y < 0 || x >= g.w || y >= g.h) return null;
    return g.z[y * g.w + x];
  };
  const profile = ({ a, b }) => {
    const out = []; const n = 160;
    const dist = Math.hypot((b.lat - a.lat) * 111320, (b.lon - a.lon) * 111320 * Math.cos(a.lat * Math.PI / 180));
    for (let i = 0; i <= n; i++) { const f = i / n; const z = zAt(a.lat + (b.lat - a.lat) * f, a.lon + (b.lon - a.lon) * f); if (z != null) out.push([dist * f, z]); }
    return out;
  };

  let settings = { language: PL ? 'pl' : 'en', theme: qs.get('theme') || 'dark', accent: qs.get('accent') || 'amber', autoUpdateCheck: false, sdkPath: '', communityPath: '', buildStore: 'auto', installMode: 'link', overpassUrl: '', mapStyle: qs.get('map') || 'dark', creator: 'FS Opendata Scenery', lastProject: 'krk', onboarded: !qs.has('welcome'), keepSources: true };
  const community = 'F:\\MSFS 2024\\Community';
  const R = {
    app_info: { version: '1.0.0', installKind: 'nsis', dataDir: 'C:\\Users\\pilot\\AppData\\Roaming\\com.blacksingularities.fsopendatascenery', cacheDir: 'C:\\Users\\pilot\\AppData\\Local\\FS Opendata Scenery\\cache', airports: 83112, crs: [[4326, 'WGS 84'], [2180, 'PL-1992'], [32634, 'UTM 34N'], [2056, 'CH1903+ / LV95'], [28992, 'RD New'], [27700, 'British National Grid']] },
    get_settings: () => settings,
    save_settings: (a) => (settings = a.settings),
    env_status: { installs: [{ label: 'MSFS 2024 · Steam', store: 'steam', root: 'F:\\MSFS 2024' }], community, communityOk: true, sdk: { root: 'C:\\MSFS 2024 SDK', packageTool: 'C:\\MSFS 2024 SDK\\Tools\\bin\\fspackagetool.exe', version: '1.4.12' }, simRunning: false },
    list_projects: () => Object.values(projects).map(card),
    get_project: (a) => projects[a.id] || krk,
    save_project: (a) => (projects[a.project.id] = a.project),
    create_project: () => krk,
    terrain_plan: { tiles: sum.tiles, tileSizeM: sum.tileSizeM, areaKm2: area(krk.area.outer), samples: sum.samples, xmlMb: sum.xmlMb, tooBig: false },
    terrain_summary: (a) => (a.id === 'krk' ? sum : null),
    terrain_preview: (a) => (a.id === 'krk' ? D.terrainPreview : null),
    terrain_profile: profile,
    lights_data: (a) => (a.id === 'krk' ? D.lights : { dots: [], counts: {}, models: 0 }),
    preview_3d: () => D.preview3d,
    osm_summary: { fetched: '2026-11-19 20:16:48', layers: ['lamps', 'roads', 'water', 'landuse', 'parking', 'pitches', 'railways', 'obstacles'], counts: { lamps: 1802, litRoads: 2316, roads: 4127, landuse: 512, parking: 198, pitches: 241, obstacles: 96, water: 214, railways: 1904, other: 714 }, total: 12124 },
    package_status: { name: krk.package.name, built: false, builtPath: '', builtBytes: 0, sources: true, workDir: 'C:\\Users\\pilot\\AppData\\Roaming\\com.blacksingularities.fsopendatascenery\\projects\\krk\\msfs', installed: null, linked: false, community, log: false, testDir: '', testBuilt: false },
    installed_packages: [
      { name: 'fsos-scenery-epkk-terrain', title: 'EPKK Kraków-Balice – LiDAR terrain', path: `${community}\\fsos-scenery-epkk-terrain`, bytes: 61234112, linked: true, project: 'epkk', projectName: 'EPKK Kraków-Balice – LiDAR terrain', test: false },
      { name: 'fsos-scenery-krakow-old-town-lighttest', title: 'Light calibration pattern', path: `${community}\\fsos-scenery-krakow-old-town-lighttest`, bytes: 412331, linked: true, project: 'krk', projectName: krk.name, test: true },
    ],
    build_log: '',
    cache_info: [['wcs-gugik-2180', 186e6], ['terrarium', 41e6], ['copernicus30', 12e6], ['overpass', 22e6]],
    search_airports: [], search_places: [], dem_file_info: null,
    check_update: { available: false, version: '1.0.0', current: '1.0.0', notes: '', url: '' },
  };
  window.__TAURI__ = {
    core: { invoke: async (cmd, args) => { const v = R[cmd]; if (v === undefined) { console.warn('mock: no', cmd); return null; } return JSON.parse(JSON.stringify(typeof v === 'function' ? v(args || {}) : v) ?? 'null'); } },
    event: { listen: async () => () => {} },
    dialog: { open: async () => null, save: async () => null, ask: async () => false, message: async () => {} },
    opener: { openUrl: async () => {}, openPath: async () => {}, revealItemInDir: async () => {} },
    app: { getVersion: async () => '1.0.0' },
  };
})();
