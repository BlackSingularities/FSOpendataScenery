// Pulpit (projekty, środowisko, jak to działa) i Ustawienia.
import { S, $, $$, esc, t, icon, invoke, num, bytes, toast, fail, confirmBox, promptBox, menu, opener, dialogApi, renderView, stageInfo } from './core.js';
import { setLang } from './i18n.js';
import { openFeedback } from './feedback.js';

function thumbSvg(outer) {
  if (!outer?.length) return `<svg viewBox="0 0 100 50"><text x="50" y="28" text-anchor="middle" fill="currentColor" opacity=".4" font-size="7">${esc(t('noAreaYet'))}</text></svg>`;
  const lats = outer.map((p) => p.lat);
  const lons = outer.map((p) => p.lon);
  const [s, n, w, e] = [Math.min(...lats), Math.max(...lats), Math.min(...lons), Math.max(...lons)];
  const k = Math.cos((((s + n) / 2) * Math.PI) / 180);
  const wx = (e - w) * k || 1e-6;
  const hy = n - s || 1e-6;
  const sc = Math.min(260 / wx, 110 / hy);
  const ox = (300 - wx * sc) / 2;
  const oy = (130 - hy * sc) / 2;
  const pts = outer.map((p) => `${(ox + (p.lon - w) * k * sc).toFixed(1)},${(oy + (n - p.lat) * sc).toFixed(1)}`).join(' ');
  return `<svg viewBox="0 0 300 130" preserveAspectRatio="xMidYMid meet"><defs><pattern id="g" width="12" height="12" patternUnits="userSpaceOnUse"><path d="M12 0H0V12" fill="none" stroke="currentColor" stroke-opacity=".07"/></pattern></defs><rect width="300" height="130" fill="url(#g)"/><polygon points="${pts}" fill="color-mix(in srgb, var(--acc) 18%, transparent)" stroke="var(--acc)" stroke-width="2" stroke-linejoin="round"/></svg>`;
}

function stagesHtml(status) {
  const keys = ['osm', 'terrain', 'lights', 'build'];
  return `<div class="stages">${keys.map((k) => {
    const raw = status?.[k];
    const ok = !!raw && !(k === 'build' && raw.includes('|sources'));
    const warn = k === 'build' && raw?.includes('|sources');
    return `<span class="stage ${ok ? 'ok' : warn ? 'warn' : ''}">${icon(ok ? 'check' : warn ? 'warn' : 'dot')}${esc(t(`st_${k}`))}</span>`;
  }).join('')}</div>`;
}

export async function renderDashboard(view) {
  view.classList.remove('flush');
  const env = S.env;
  const chk = (ok, title, desc, warn) => `<div class="check ${ok ? 'ok' : warn ? 'warn' : 'bad'}">${icon(ok ? 'ok' : warn ? 'warn' : 'error')}<div class="grow"><div class="t">${esc(title)}</div><div class="d">${desc}</div></div></div>`;
  view.innerHTML = `<div class="dash">
    <div class="hero">
      <div class="hero-main"><div class="glow"></div>
        <h2>${esc(t('heroTitle'))}</h2><p>${esc(t('heroText'))}</p>
        <div class="acts"><button class="btn primary" id="d-new">${icon('plus')}${esc(t('newProject'))}</button><button class="btn" id="d-import">${icon('upload')}${esc(t('importProject'))}</button>${S.project ? `<button class="btn" id="d-cont">${icon('play')}${esc(t('continueProject', S.project.name))}</button>` : ''}</div>
      </div>
      <div class="card env"><h3>${icon('cpu')}${esc(t('environment'))}<span class="grow"></span><button class="btn sm ghost icon" id="d-env" title="${esc(t('refresh'))}">${icon('refresh')}</button></h3>
        ${env ? `${chk(env.installs.length > 0, t('chkMsfs'), env.installs.length ? esc(env.installs.map((i) => i.label).join(' · ')) : esc(t('msfsMissing')), true)}
        ${chk(env.communityOk, t('chkCommunity'), esc(env.community || t('communityMissing')), true)}
        ${chk(!!env.sdk && !env.sdk.legacy, t('chkSdk'), env.sdk && !env.sdk.legacy ? esc(env.sdk.root) : `${esc(env.sdk?.legacy ? t('sdkLegacy', env.sdk.version, env.sdk.root) : t('sdkMissing'))} <a href="#" id="d-sdk">${esc(t('sdkHow'))}</a>`, true)}
        ${chk(!env.simRunning, t('chkSimClosed'), env.simRunning ? esc(t('simRunningD')) : esc(t('simClosedD')))}` : `<div class="faint">${esc(t('checking'))}</div>`}
      </div>
    </div>
    <div><div class="section-title">${icon('layers')}${esc(t('projects'))}<span class="grow"></span><span class="faint">${num(S.projects.length)}</span></div>
      <div class="proj-grid">${S.projects.map((p) => `<div class="proj ${S.project?.id === p.id ? 'cur' : ''}" data-p="${p.id}"><div class="thumb">${thumbSvg(p.outer)}</div>
        <div class="pb"><div class="pt"><span class="grow">${esc(p.name)}</span><button class="btn sm icon ghost" data-more="${p.id}">${icon('more')}</button></div>
        <div class="pd">${p.areaKm2 ? `${num(p.areaKm2, p.areaKm2 < 10 ? 2 : 1)} km² · ` : ''}${esc(p.package)}</div>${stagesHtml(p.status)}<div class="pd">${esc(t('updated'))}: ${esc(p.updated)}</div></div></div>`).join('')}
        <div class="proj new" id="d-new2">${icon('plus')}<div>${esc(t('newProject'))}</div></div></div></div>
    <div id="d-installed"></div>
    <div><div class="section-title">${icon('info')}${esc(t('howItWorks'))}</div>
      <div class="how">${[1, 2, 3, 4, 5].map((i) => `<div class="st"><b>${i}</b><div class="t">${esc(t(`how${i}`))}</div><div class="d">${esc(t(`how${i}d`))}</div></div>`).join('')}</div></div>
    <div><div class="section-title">${icon('database')}${esc(t('dataCredits'))}</div>
      <div class="sources-list">${['gugik', 'terrarium', 'copernicus', 'osm', 'ourairports', 'nominatim'].map((k) => `<div class="s"><b>${esc(t(`cr_${k}`))}</b>${esc(t(`cr_${k}d`))}</div>`).join('')}</div></div>
  </div>`;
  const newP = () => window.dispatchEvent(new CustomEvent('new-project'));
  $('#d-new').onclick = newP;
  $('#d-new2').onclick = newP;
  $('#d-cont')?.addEventListener('click', () => window.dispatchEvent(new CustomEvent('go', { detail: S.project.area?.outer?.length ? 'terrain' : 'area' })));
  $('#d-env').onclick = async () => { S.env = await invoke('env_status'); renderDashboard(view); };
  $('#d-sdk')?.addEventListener('click', (e) => { e.preventDefault(); opener.openUrl('https://docs.flightsimulator.com/msfs2024/html/1_Introduction/SDK_Overview.htm'); });
  $('#d-import').onclick = async () => {
    const path = await dialogApi.open({ filters: [{ name: 'FS Opendata Scenery', extensions: ['json'] }] });
    if (!path) return;
    try {
      const p = await invoke('import_project', { path });
      window.dispatchEvent(new CustomEvent('open-project', { detail: p.id }));
      toast(t('projectImported'));
    } catch (e) { fail(e); }
  };
  renderInstalled();
  $$('[data-p]', view).forEach((c) => (c.onclick = (e) => {
    if (e.target.closest('[data-more]')) return;
    window.dispatchEvent(new CustomEvent('open-project', { detail: c.dataset.p, bubbles: false }));
  }));
  $$('[data-more]', view).forEach((b) => (b.onclick = () => {
    const id = b.dataset.more;
    const card = S.projects.find((p) => p.id === id);
    menu(b, [
      { icon: 'play', label: t('open'), run: () => window.dispatchEvent(new CustomEvent('open-project', { detail: id })) },
      { icon: 'edit', label: t('rename'), run: async () => {
        const name = await promptBox(t('rename'), t('projectName'), card.name);
        if (!name) return;
        const p = await invoke('get_project', { id });
        p.name = name;
        await invoke('save_project', { project: p });
        if (S.project?.id === id) S.project.name = name;
        window.dispatchEvent(new CustomEvent('projects-changed'));
      } },
      { icon: 'copy', label: t('duplicate'), run: async () => { await invoke('duplicate_project', { id }); window.dispatchEvent(new CustomEvent('projects-changed')); } },
      '-',
      { icon: 'trash', label: t('delete'), danger: true, run: async () => {
        if (!(await confirmBox(t('deleteProjectQ'), `<div>${esc(t('deleteProjectD', card.name))}</div>`, t('delete'), true))) return;
        try {
          await invoke('delete_project', { id });
          if (S.project?.id === id) S.project = null;
          window.dispatchEvent(new CustomEvent('projects-changed'));
        } catch (e) { fail(e); }
      } },
    ]);
  }));
}

async function renderInstalled() {
  const el = $('#d-installed');
  if (!el) return;
  const list = await invoke('installed_packages').catch(() => []);
  if (!$('#d-installed')) return;
  if (!list.length) { el.innerHTML = ''; return; }
  el.innerHTML = `<div class="section-title">${icon('packages')}${esc(t('installedPkgs'))}<span class="grow"></span><span class="faint">${num(list.length)}</span></div>
    <div class="card list">${list.map((p, i) => `<div class="row"><span class="tag ${p.test ? 'warn' : 'acc'}">${esc(p.test ? t('testTag') : 'Community')}</span>
      <div class="grow"><div class="t">${esc(p.title)}</div><div class="d">${esc(p.name)}${p.projectName ? ` · ${esc(t('fromProject', p.projectName))}` : ` · ${esc(t('noProjectLink'))}`}${p.linked ? ` · ${esc(t('linked'))}` : p.bytes ? ` · ${bytes(p.bytes)}` : ''}</div></div>
      <button class="btn sm ghost icon" data-open="${i}" title="${esc(t('open'))}">${icon('folder')}</button>
      <button class="btn sm ghost icon danger" data-rm="${i}" title="${esc(t('uninstall'))}">${icon('trash')}</button></div>`).join('')}</div>`;
  $$('[data-open]', el).forEach((b) => (b.onclick = () => opener.openPath(list[+b.dataset.open].path)));
  $$('[data-rm]', el).forEach((b) => (b.onclick = async () => {
    const p = list[+b.dataset.rm];
    if (!(await confirmBox(t('uninstallQ'), `<div>${esc(t('uninstallD', p.path))}</div>`, t('uninstall'), true))) return;
    try { await invoke('remove_installed_package', { path: p.path }); toast(t('uninstalled')); renderInstalled(); } catch (e) { fail(e); }
  }));
}

// ═════════════ Ustawienia ═════════════
const ACCENTS = { amber: '#ffc35a', sky: '#5cc4ff', green: '#5fe0a0', violet: '#b7a2ff', rose: '#ff95b5' };

export async function renderSettings(view) {
  view.classList.remove('flush');
  const s = S.settings;
  const row = (title, desc, ctl) => `<div class="set-row"><div class="grow"><div class="t">${title}</div>${desc ? `<div class="d">${desc}</div>` : ''}</div>${ctl}</div>`;
  const sw = (key) => `<button class="switch ${s[key] ? 'on' : ''}" data-sw="${key}"></button>`;
  const sel = (key, opts) => `<select class="inp" data-sel="${key}" style="width:220px">${opts.map(([v, l]) => `<option value="${v}" ${s[key] === v ? 'selected' : ''}>${esc(l)}</option>`).join('')}</select>`;
  view.innerHTML = `<div class="settings">
    <div class="card set-card"><h3>${icon('palette')}${esc(t('appearance'))}</h3>
      ${row(esc(t('language')), '', sel('language', [['pl', 'Polski'], ['en', 'English']]))}
      ${row(esc(t('theme')), '', sel('theme', [['dark', t('theme_dark')], ['amoled', t('theme_amoled')], ['light', t('theme_light')], ['auto', t('theme_auto')]]))}
      ${row(esc(t('accent')), '', `<div class="accent-pick">${Object.entries(ACCENTS).map(([k, c]) => `<button data-acc="${k}" class="${s.accent === k ? 'on' : ''}" style="background:${c}" title="${k}"></button>`).join('')}</div>`)}
    </div>
    <div class="card set-card"><h3>${icon('rocket')}${esc(t('msfsSdk'))}</h3>
      ${row(esc(t('sdkPath')), esc(t('sdkPathD')), `<div class="btn-row"><input class="inp" data-txt="sdkPath" value="${esc(s.sdkPath)}" placeholder="${esc(S.env?.sdk?.root || 'C:\\MSFS 2024 SDK')}"><button class="btn sm icon" data-browse="sdkPath">${icon('folder')}</button></div>`)}
      ${row(esc(t('communityPath')), esc(t('communityPathD')), `<div class="btn-row"><input class="inp" data-txt="communityPath" value="${esc(s.communityPath)}" placeholder="${esc(S.env?.community || '')}"><button class="btn sm icon" data-browse="communityPath">${icon('folder')}</button></div>`)}
      ${row(esc(t('buildStore')), esc(t('buildStoreD')), sel('buildStore', [['auto', t('auto')], ['store', 'Microsoft Store / Xbox'], ['steam', 'Steam']]))}
      ${row(esc(t('installMode')), esc(t('installModeD')), sel('installMode', [['copy', t('modeCopy')], ['link', t('modeLink')]]))}
      ${row(esc(t('keepSources')), esc(t('keepSourcesD')), sw('keepSources'))}
      ${row(esc(t('defaultCreator')), esc(t('defaultCreatorD')), `<input class="inp" data-txt="creator" value="${esc(s.creator)}">`)}
    </div>
    <div class="card set-card"><h3>${icon('database')}${esc(t('dataAndCache'))}</h3>
      ${row(esc(t('overpassUrl')), esc(t('overpassUrlD')), `<input class="inp" data-txt="overpassUrl" value="${esc(s.overpassUrl)}" placeholder="https://overpass-api.de/api/interpreter">`)}
      <div id="st-cache"></div>
      ${row(esc(t('dataDir')), `<span class="mono selectable">${esc(S.info.dataDir)}</span>`, `<button class="btn sm" id="st-data">${icon('folder')}${esc(t('open'))}</button>`)}
    </div>
    <div class="card set-card"><h3>${icon('info')}${esc(t('about'))}</h3>
      ${row(esc(t('autoUpdate')), esc(t('autoUpdateD')), sw('autoUpdateCheck'))}
      ${row(`FS Opendata Scenery ${esc(S.info.version)}${S.info.installKind === 'portable' ? ` <span class="tag">${esc(t('portable'))}</span>` : ''}`, esc(t('aboutD')), `<button class="btn sm" id="st-upd">${icon('update')}${esc(t('checkUpdates'))}</button>`)}
      ${row(esc(t('feedback')), esc(t('feedbackD')), `<button class="btn sm" id="st-feedback">${icon('note')}${esc(t('feedback'))}</button>`)}
      ${row(esc(t('author')), 'Adam Rędzikowski · <a href="#" id="st-site">redzikowski.com</a> · <a href="#" id="st-mail">kontakt@redzikowski.com</a>', `<button class="btn sm" id="st-coffee">${icon('coffee')}${esc(t('coffee'))}</button>`)}
      ${row(esc(t('licenses')), esc(t('licensesD')), '')}
    </div></div>`;
  const save = async () => {
    try { S.settings = await invoke('save_settings', { settings: S.settings }); } catch (e) { fail(e); }
  };
  $$('[data-sw]', view).forEach((b) => (b.onclick = async () => { s[b.dataset.sw] = !s[b.dataset.sw]; b.classList.toggle('on'); await save(); }));
  $$('[data-sel]', view).forEach((el) => (el.onchange = async () => {
    s[el.dataset.sel] = el.value;
    await save();
    if (el.dataset.sel === 'language') { setLang(el.value); window.dispatchEvent(new CustomEvent('relayout')); }
    if (el.dataset.sel === 'theme') window.dispatchEvent(new CustomEvent('apply-theme'));
  }));
  $$('[data-txt]', view).forEach((el) => (el.onchange = async () => {
    s[el.dataset.txt] = el.value.trim();
    await save();
    if (el.dataset.txt === 'sdkPath' || el.dataset.txt === 'communityPath') S.env = await invoke('env_status');
  }));
  $$('[data-browse]', view).forEach((b) => (b.onclick = async () => {
    const dir = await dialogApi.open({ directory: true });
    if (!dir) return;
    s[b.dataset.browse] = dir;
    await save();
    S.env = await invoke('env_status');
    if (b.dataset.browse === 'sdkPath' && !S.env.sdk) toast(t('sdkNotThere'), 'warn');
    renderSettings(view);
  }));
  $$('[data-acc]', view).forEach((b) => (b.onclick = async () => { s.accent = b.dataset.acc; await save(); window.dispatchEvent(new CustomEvent('apply-theme')); renderSettings(view); }));
  $('#st-data').onclick = () => opener.openPath(S.info.dataDir);
  $('#st-upd').onclick = () => window.dispatchEvent(new CustomEvent('check-update'));
  $('#st-site').onclick = (e) => { e.preventDefault(); opener.openUrl('https://redzikowski.com'); };
  $('#st-mail').onclick = (e) => { e.preventDefault(); opener.openUrl('mailto:kontakt@redzikowski.com'); };
  $('#st-coffee').onclick = () => opener.openUrl('https://buycoffee.to/adamredzikowski');
  $('#st-feedback').onclick = () => openFeedback();
  renderCache();
}

async function renderCache() {
  const el = $('#st-cache');
  if (!el) return;
  const list = await invoke('cache_info').catch(() => []);
  const total = list.reduce((a, [, b]) => a + b, 0);
  const label = (n) => (n.startsWith('wcs-gugik') ? t('srcname_gugik') : n.startsWith('wcs-') ? 'WCS' : n === 'terrarium' ? 'AWS Terrain Tiles' : n === 'copernicus30' ? 'Copernicus GLO-30' : n === 'copernicus90' ? 'Copernicus GLO-90' : n);
  el.innerHTML = `<div class="set-row"><div class="grow"><div class="t">${esc(t('demCache'))} · ${bytes(total)}</div><div class="d">${list.length ? list.map(([n, b]) => `${esc(label(n))}: ${bytes(b)}`).join(' · ') : esc(t('cacheEmpty'))}</div></div>
    <div class="btn-row">${list.map(([n]) => `<button class="btn sm ghost" data-clr="${esc(n)}" title="${esc(t('clear'))}">${icon('trash')}${esc(label(n))}</button>`).join('')}<button class="btn sm danger" id="st-clr" ${list.length ? '' : 'disabled'}>${icon('trash')}${esc(t('clearAll'))}</button></div></div>`;
  $$('[data-clr]', el).forEach((b) => (b.onclick = async () => { try { await invoke('clear_cache', { name: b.dataset.clr }); renderCache(); } catch (e) { fail(e); } }));
  $('#st-clr').onclick = async () => {
    if (!(await confirmBox(t('clearCacheQ'), `<div>${esc(t('clearCacheD', bytes(total)))}</div>`, t('clearAll'), true))) return;
    try { await invoke('clear_cache', { name: null }); renderCache(); } catch (e) { fail(e); }
  };
}

export { stageInfo, renderView };
