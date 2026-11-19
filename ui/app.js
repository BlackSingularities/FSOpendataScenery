// FS Opendata Scenery – szkielet aplikacji: nawigacja, pasek górny, zdarzenia zadań, aktualizacje.
import { S, $, $$, esc, t, icon, invoke, listen, toast, fail, dialog, setRenderer, renderView, renderJobs, startJob, projectJob, saveProjectNow, flushSave, T } from './core.js';
import { setLang, lang } from './i18n.js';
import { initFeedback } from './feedback.js';
import { renderDashboard, renderSettings } from './views.js';
import { renderArea, renderTerrain, renderLights, renderObjects, renderBuild, paintLog } from './editor.js';
import { render3d, leave3d } from './view3d.js';
import { drawArea } from './map.js';

initFeedback({ app: 'fsopendatascenery', appName: 'FS Opendata Scenery', version: () => T.app.getVersion(), lang: () => lang(), platform: 'windows' });

const NAV = [
  ['dashboard', 'dashboard'],
  ['area', 'area'],
  ['terrain', 'terrain'],
  ['lights', 'lights'],
  ['objects', 'objects'],
  ['view3d', 'cube'],
  ['build', 'build'],
];

function applyTheme() {
  const s = S.settings;
  const theme = s.theme === 'auto' ? (matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark') : s.theme;
  document.documentElement.dataset.theme = theme;
  document.documentElement.dataset.accent = s.accent || 'amber';
}

function renderRail() {
  const rail = $('#rail');
  rail.innerHTML = `<div class="brand"><img src="assets/icon.png" alt=""></div>
    ${NAV.map(([id, ic], i) => `<button class="nav-btn ${S.view === id ? 'active' : ''}" data-v="${id}" title="${esc(t(`nav_${id}`))} (${i + 1})"><span class="pill">${icon(ic)}</span>${esc(t(`nav_${id}`))}</button>`).join('')}
    <div class="spacer"></div>
    <button class="nav-btn ${S.view === 'settings' ? 'active' : ''}" data-v="settings"><span class="pill">${icon('settings')}</span>${esc(t('nav_settings'))}</button>`;
  $$('[data-v]', rail).forEach((b) => (b.onclick = () => go(b.dataset.v)));
}

function renderTopbar() {
  const bar = $('#topbar');
  const sub = S.view === 'dashboard' ? t('tagline') : S.view === 'settings' ? '' : t(`sub_${S.view}`);
  bar.innerHTML = `<h1>${esc(t(`nav_${S.view}`))}</h1><div class="sub">${esc(sub)}</div><div class="grow"></div>
    <span class="job-pill" id="job-pill" hidden></span>
    ${S.view !== 'settings' ? `<button class="proj-pick" id="pick">${icon('layers')}<span>${esc(S.project?.name || t('chooseProject'))}</span>${icon('chevron')}</button>` : ''}`;
  $('#pick')?.addEventListener('click', pickProject);
  renderJobs();
}

async function pickProject(e) {
  const btn = e.currentTarget;
  const { menu } = await import('./core.js');
  menu(btn, [
    ...S.projects.slice(0, 12).map((p) => ({ icon: p.id === S.project?.id ? 'check' : 'layers', label: p.name, run: () => openProject(p.id) })),
    '-',
    { icon: 'plus', label: t('newProject'), run: newProject },
    { icon: 'dashboard', label: t('allProjects'), run: () => go('dashboard') },
  ]);
}

let current = null;
async function go(view) {
  if (current === 'view3d' && view !== 'view3d') leave3d();
  await flushSave();
  S.view = view;
  current = view;
  renderRail();
  renderTopbar();
  const el = $('#view');
  el.className = '';
  el.innerHTML = '';
  el.scrollTop = 0;
  const r = { dashboard: renderDashboard, area: renderArea, terrain: renderTerrain, lights: renderLights, objects: renderObjects, view3d: render3d, build: renderBuild, settings: renderSettings }[view];
  try { await r(el); } catch (e) { console.error(e); fail(e); }
}

setRenderer(() => go(S.view));

async function refreshProjects() {
  S.projects = await invoke('list_projects');
}

async function openProject(id) {
  try {
    await flushSave();
    S.project = await invoke('get_project', { id });
    S.lights = null;
    S.terrain = null;
    S.settings.lastProject = id;
    invoke('save_settings', { settings: S.settings }).catch(() => {});
    go(S.view === 'dashboard' || S.view === 'settings' ? (S.project.area?.outer?.length ? 'terrain' : 'area') : S.view);
  } catch (e) { fail(e); }
}

async function newProject() {
  const name = await dialog({
    title: t('newProject'),
    body: `<label class="field">${esc(t('projectName'))}<input class="inp" id="np-name" placeholder="${esc(t('newProjectName'))}"></label><div class="faint">${esc(t('newProjectD'))}</div>`,
    actions: [{ label: t('cancel'), value: null }, { label: t('create'), kind: 'primary', value: (w) => $('#np-name', w).value.trim() || t('newProjectName') }],
  });
  if (name == null) return;
  try {
    S.project = await invoke('create_project', { name, area: null });
    await refreshProjects();
    go('area');
  } catch (e) { fail(e); }
}

// ───────── Zdarzenia zadań ─────────
listen('job', async (ev) => {
  const j = ev.payload;
  if (j.state === 'running') {
    S.jobs.set(j.id, { project: j.project, kind: j.kind, stage: j.stage, done: j.done, total: j.total, message: j.message });
    renderJobs();
    return;
  }
  S.jobs.delete(j.id);
  renderJobs();
  const pname = S.projects.find((p) => p.id === j.project)?.name || '';
  if (j.state === 'done') {
    const r = j.result || {};
    if (j.kind === 'build' || j.kind === 'all') {
      const b = j.kind === 'all' ? r.build : r;
      if (b?.compiled) toast(b.installed ? t('buildInstalled', pname) : t('buildDone', pname));
      else if (b?.reason === 'sdk') toast(t('sourcesOnly'), 'warn', 9000);
    } else toast(t(`done_${j.kind}`, pname));
    (r.warnings || []).slice(0, 3).forEach((w) => toast(w, 'warn', 8000));
  } else if (j.state === 'error') {
    toast(`${t(`job_${j.kind}`)}: ${j.message}`, 'bad', 10000);
  } else toast(t('cancelled'), 'warn');
  if (S.project?.id === j.project) {
    try { S.project = await invoke('get_project', { id: j.project }); } catch { /* projekt usunięty */ }
  }
  await refreshProjects();
  // Łańcuch: „pobierz OSM i generuj”.
  const ch = window.__chain;
  if (ch && j.state === 'done' && ch.project === j.project && ch.after === j.kind) {
    window.__chain = null;
    if (S.project?.id === j.project) return startJob(ch.next);
  } else if (ch && ch.project === j.project) window.__chain = null;
  if (S.project?.id === j.project || S.view === 'dashboard') renderView();
});

listen('build-log', (ev) => {
  const { project, line } = ev.payload;
  const arr = S.logs.get(project) || [];
  arr.push(line);
  if (arr.length > 4000) arr.splice(0, arr.length - 4000);
  S.logs.set(project, arr);
  if (S.view === 'build' && S.project?.id === project) paintLog(arr);
});

// ───────── Aktualizacje ─────────
async function checkUpdate(silent) {
  try {
    const u = await invoke('check_update');
    if (!u.available) { if (!silent) toast(t('upToDate', S.info.version)); return; }
    const ok = await dialog({
      title: t('updateAvailable', u.latest),
      body: `<div class="muted">${esc(t('updateNewVersion'))}</div>`,
      actions: [{ label: t('later'), value: false }, ...(u.canInstall && u.download ? [{ label: t('installUpdate'), kind: 'primary', value: true }] : [])],
    });
    if (ok === true) { toast(t('downloading')); await invoke('install_update', { url: u.download }); }
  } catch (e) { if (!silent) fail(e); }
}

// ───────── Start ─────────
window.addEventListener('go', (e) => go(e.detail));
window.addEventListener('open-project', (e) => openProject(e.detail));
window.addEventListener('new-project', newProject);
window.addEventListener('projects-changed', async () => { await refreshProjects(); renderView(); });
window.addEventListener('project-renamed', () => { renderTopbar(); refreshProjects(); });
window.addEventListener('area-changed', async () => { await refreshProjects(); if (S.view === 'area') renderView(); });
window.addEventListener('relayout', () => { renderRail(); renderView(); });
window.addEventListener('apply-theme', () => { applyTheme(); drawArea(); });
window.addEventListener('check-update', () => checkUpdate(false));
matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => { if (S.settings?.theme === 'auto') applyTheme(); });

document.addEventListener('keydown', (e) => {
  if (e.target.closest('input,textarea,select,[contenteditable]') || $('.dialog-wrap')) return;
  if (!e.ctrlKey && !e.altKey && /^[1-7]$/.test(e.key)) go(NAV[+e.key - 1][0]);
  if (e.ctrlKey && e.key.toLowerCase() === 'n') { e.preventDefault(); newProject(); }
  if (e.key === 'F5') { e.preventDefault(); renderView(); }
});

(async function boot() {
  try {
    const [info, settings] = await Promise.all([invoke('app_info'), invoke('get_settings')]);
    S.info = info;
    S.settings = settings;
    setLang(settings.language);
    applyTheme();
    await refreshProjects();
    if (settings.lastProject && S.projects.some((p) => p.id === settings.lastProject)) {
      S.project = await invoke('get_project', { id: settings.lastProject });
    }
    renderRail();
    await go('dashboard');
    invoke('env_status').then((env) => { S.env = env; if (S.view === 'dashboard') renderView(); }).catch(() => {});
    if (settings.autoUpdateCheck) setTimeout(() => checkUpdate(true), 4000);
    if (!settings.onboarded) {
      settings.onboarded = true;
      invoke('save_settings', { settings }).catch(() => {});
    }
  } catch (e) {
    $('#view').innerHTML = `<div class="loading">${icon('error')}<div>${esc(String(e))}</div></div>`;
  }
})();

export { saveProjectNow, projectJob };
