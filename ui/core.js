// Wspólny stan i narzędzia interfejsu: wywołania backendu, okna, powiadomienia, formatowanie.
import { t, lang } from './i18n.js';
import { icon } from './icons.js';

export const T = window.__TAURI__;
export const invoke = (cmd, args) => T.core.invoke(cmd, args);
export const listen = (ev, fn) => T.event.listen(ev, fn);
export const dialogApi = T.dialog;
export const opener = T.opener;

export const S = {
  info: null,
  settings: null,
  env: null,
  projects: [],
  project: null,
  view: 'dashboard',
  jobs: new Map(), // id → {project, kind, stage, done, total, message}
  logs: new Map(), // projekt → linie konsoli budowania
  lights: null, // {dots, counts, models} dla bieżącego projektu
  osm: null,
  terrain: null, // podsumowanie terenu
  plan: null,
  pkg: null,
};

let renderer = () => {};
export function setRenderer(fn) { renderer = fn; }
export function renderView() { renderer(); }

export const $ = (sel, root = document) => root.querySelector(sel);
export const $$ = (sel, root = document) => [...root.querySelectorAll(sel)];
export const esc = (s) => String(s ?? '').replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
export const locale = () => (lang() === 'en' ? 'en-GB' : 'pl-PL');
export const num = (v, d = 0) => (v == null || Number.isNaN(+v) ? '–' : (+v).toLocaleString(locale(), { maximumFractionDigits: d, minimumFractionDigits: d }));
export function bytes(b) {
  if (!b) return '0 B';
  const u = ['B', 'KB', 'MB', 'GB', 'TB'];
  const i = Math.min(u.length - 1, Math.floor(Math.log(b) / Math.log(1024)));
  return `${num(b / 1024 ** i, i >= 2 ? 1 : 0)} ${u[i]}`;
}
export const debounce = (fn, ms = 300) => {
  let h;
  return (...a) => { clearTimeout(h); h = setTimeout(() => fn(...a), ms); };
};
export { icon, t };

/** Status etapu zapisany przez backend: "data|wartość|wartość". */
export function stageInfo(key, p = S.project) {
  const raw = p?.status?.[key];
  if (!raw) return null;
  const [at, ...rest] = raw.split('|');
  return { at, rest };
}

// ───────── Powiadomienia ─────────
export function toast(msg, kind = 'ok', ms = 4200) {
  const el = document.createElement('div');
  el.className = `toast ${kind}`;
  el.innerHTML = `${icon(kind === 'bad' ? 'error' : kind === 'warn' ? 'warn' : 'ok')}<div class="grow">${esc(msg)}</div>`;
  $('#dock').append(el);
  setTimeout(() => el.remove(), ms);
}
export const fail = (e) => toast(String(e?.message || e), 'bad', 7000);

// ───────── Okna dialogowe ─────────
export function dialog({ title, body = '', actions = [{ label: t('ok'), kind: 'primary', value: true }], wide = false, onMount }) {
  return new Promise((resolve) => {
    const wrap = document.createElement('div');
    wrap.className = 'dialog-wrap';
    wrap.innerHTML = `<div class="dialog ${wide ? 'wide' : ''}" role="dialog" aria-modal="true"><h2>${esc(title)}</h2><div class="db">${body}</div><div class="df"></div></div>`;
    const df = $('.df', wrap);
    const close = (v) => { wrap.remove(); document.removeEventListener('keydown', onKey, true); resolve(v); };
    for (const a of actions) {
      const b = document.createElement('button');
      b.className = `btn ${a.kind || ''}`;
      b.textContent = a.label;
      b.onclick = () => close(typeof a.value === 'function' ? a.value(wrap) : a.value);
      df.append(b);
    }
    const onKey = (e) => {
      if (e.key === 'Escape') { e.stopPropagation(); close(null); }
      if (e.key === 'Enter' && e.target.tagName !== 'TEXTAREA' && !e.target.closest('.ac-list')) {
        const p = $('.btn.primary', df);
        if (p) { e.preventDefault(); p.click(); }
      }
    };
    document.addEventListener('keydown', onKey, true);
    wrap.addEventListener('mousedown', (e) => { if (e.target === wrap) close(null); });
    $('#layers').append(wrap);
    onMount?.(wrap);
    setTimeout(() => $('input,select,textarea', wrap)?.focus(), 40);
  });
}

export const confirmBox = (title, body, okLabel = t('ok'), danger = false) =>
  dialog({ title, body, actions: [{ label: t('cancel'), value: false }, { label: okLabel, kind: danger ? 'primary danger' : 'primary', value: true }] });

export async function promptBox(title, label, value = '') {
  const v = await dialog({
    title,
    body: `<label class="field">${esc(label)}<input class="inp" id="pb-in" value="${esc(value)}"></label>`,
    actions: [{ label: t('cancel'), value: null }, { label: t('ok'), kind: 'primary', value: (w) => $('#pb-in', w).value }],
  });
  return v == null ? null : String(v).trim();
}

/** Menu kontekstowe przy elemencie. */
export function menu(anchor, items) {
  document.querySelectorAll('.menu').forEach((m) => m.remove());
  const m = document.createElement('div');
  m.className = 'menu';
  for (const it of items) {
    if (it === '-') { m.append(document.createElement('hr')); continue; }
    const b = document.createElement('button');
    b.className = it.danger ? 'danger' : '';
    b.innerHTML = `${icon(it.icon || 'dot')}${esc(it.label)}`;
    b.onclick = () => { m.remove(); it.run(); };
    m.append(b);
  }
  document.body.append(m);
  const r = anchor.getBoundingClientRect();
  m.style.position = 'fixed';
  m.style.top = `${Math.min(r.bottom + 6, innerHeight - m.offsetHeight - 10)}px`;
  m.style.left = `${Math.min(r.left, innerWidth - m.offsetWidth - 10)}px`;
  m.style.zIndex = 90;
  setTimeout(() => document.addEventListener('mousedown', (e) => { if (!m.contains(e.target)) m.remove(); }, { once: true }), 0);
}

// ───────── Projekt ─────────
let saveTimer = null;
let savePending = null;
/** Zapis projektu z opóźnieniem (pola formularzy). */
export function saveProjectSoon(ms = 500) {
  clearTimeout(saveTimer);
  savePending = new Promise((res) => {
    saveTimer = setTimeout(async () => {
      try {
        if (S.project) S.project = await invoke('save_project', { project: S.project });
      } catch (e) { fail(e); }
      res();
    }, ms);
  });
  return savePending;
}
export async function saveProjectNow() {
  clearTimeout(saveTimer);
  if (S.project) S.project = await invoke('save_project', { project: S.project });
}
export async function flushSave() { if (savePending) await savePending; }

/** Ustawia wartość w projekcie po ścieżce „terrain.resolution”. */
export function setPath(obj, path, value) {
  const keys = path.split('.');
  let o = obj;
  for (const k of keys.slice(0, -1)) o = o[k] ??= {};
  o[keys.at(-1)] = value;
}
export function getPath(obj, path) {
  return path.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);
}

/** Wiąże pola formularza z atrybutem data-bind z bieżącym projektem. */
export function bindInputs(root, onChange) {
  for (const el of $$('[data-bind]', root)) {
    const path = el.dataset.bind;
    const v = getPath(S.project, path);
    if (el.type === 'checkbox') el.checked = !!v;
    else if (el.classList.contains('switch')) el.classList.toggle('on', !!v);
    else if (v != null) el.value = v;
    const handler = () => {
      let val;
      if (el.type === 'checkbox') val = el.checked;
      else if (el.classList.contains('switch')) { el.classList.toggle('on'); val = el.classList.contains('on'); }
      else if (el.type === 'number' || el.dataset.num != null) { val = parseFloat(String(el.value).replace(',', '.')); if (Number.isNaN(val)) return; if (el.min !== '' && el.min != null && val < +el.min) val = +el.min; if (el.max !== '' && el.max != null && val > +el.max) val = +el.max; }
      else val = el.value;
      setPath(S.project, path, val);
      saveProjectSoon();
      onChange?.(path, val, el);
    };
    if (el.classList.contains('switch')) el.addEventListener('click', handler);
    else el.addEventListener(el.tagName === 'SELECT' || el.type === 'checkbox' ? 'change' : 'input', handler);
  }
}

// ───────── Zadania w tle ─────────
export function projectJob(pid = S.project?.id) {
  for (const [id, j] of S.jobs) if (j.project === pid) return { id, ...j };
  return null;
}

export async function startJob(kind, opts = {}) {
  if (!S.project) return;
  await flushSave();
  await saveProjectNow();
  try {
    const id = await invoke('start_job', { project: S.project.id, kind, install: !!opts.install });
    S.jobs.set(id, { project: S.project.id, kind, stage: 'start', done: 0, total: 0, message: '' });
    if (kind === 'build' || kind === 'all' || kind === 'testlights') S.logs.set(S.project.id, []);
    renderJobs();
    renderView();
  } catch (e) { fail(e); }
}

export function renderJobs() {
  const dock = $('#dock');
  $$('.job', dock).forEach((j) => j.remove());
  for (const [id, j] of S.jobs) {
    const el = document.createElement('div');
    el.className = 'job';
    const pct = j.total ? Math.round((j.done / j.total) * 100) : null;
    const proj = S.projects.find((p) => p.id === j.project)?.name || S.project?.name || '';
    el.innerHTML = `<div class="t">${icon('refresh', 'spin')}${esc(t(`job_${j.kind}`))}<span class="faint">· ${esc(proj)}</span></div>
      <div class="d">${esc(t(`stage_${j.stage}`) || j.stage)}${j.total ? ` · ${num(j.done)} / ${num(j.total)}` : ''}${j.message ? ` · ${esc(j.message)}` : ''}</div>
      <div class="bar"><span style="width:${pct ?? 100}%;${pct == null ? 'opacity:.35' : ''}"></span></div>
      <div class="acts"><button class="btn sm ghost">${icon('stop')}${esc(t('cancel'))}</button></div>`;
    $('.btn', el).onclick = () => invoke('cancel_job', { id });
    dock.prepend(el);
  }
  const pill = $('#job-pill');
  if (pill) {
    const j = projectJob();
    pill.hidden = !j;
    if (j) pill.innerHTML = `${icon('refresh', 'spin')}${esc(t(`job_${j.kind}`))}${j.total ? ` ${Math.round((j.done / j.total) * 100)}%` : ''}`;
  }
}
