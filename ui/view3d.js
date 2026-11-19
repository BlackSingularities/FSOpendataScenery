// Podgląd 3D: policzony teren (z przewyższeniem) i rozmieszczone światła nocą albo w dzień.
import * as THREE from './vendor/three.module.min.js';
import { OrbitControls } from './vendor/OrbitControls.js';
import { S, $, $$, esc, t, icon, invoke, num } from './core.js';

let ctx = null;

function dispose() {
  if (!ctx) return;
  cancelAnimationFrame(ctx.raf);
  ctx.ro.disconnect();
  ctx.renderer.dispose();
  ctx.scene.traverse((o) => { o.geometry?.dispose(); if (o.material) [].concat(o.material).forEach((m) => { m.map?.dispose(); m.dispose(); }); });
  ctx = null;
}

function glowTexture() {
  const c = document.createElement('canvas');
  c.width = c.height = 64;
  const g = c.getContext('2d');
  const grd = g.createRadialGradient(32, 32, 0, 32, 32, 32);
  grd.addColorStop(0, 'rgba(255,255,255,1)');
  grd.addColorStop(0.18, 'rgba(255,255,255,.85)');
  grd.addColorStop(0.45, 'rgba(255,255,255,.22)');
  grd.addColorStop(1, 'rgba(255,255,255,0)');
  g.fillStyle = grd;
  g.fillRect(0, 0, 64, 64);
  return new THREE.CanvasTexture(c);
}

function hypso(f) {
  const stops = [[0, [58, 120, 82]], [0.2, [110, 160, 92]], [0.45, [196, 190, 120]], [0.65, [190, 150, 98]], [0.85, [160, 130, 118]], [1, [240, 240, 240]]];
  f = Math.min(1, Math.max(0, f));
  for (let i = 1; i < stops.length; i++) {
    if (f <= stops[i][0]) {
      const k = (f - stops[i - 1][0]) / (stops[i][0] - stops[i - 1][0]);
      return [0, 1, 2].map((j) => (stops[i - 1][1][j] + (stops[i][1][j] - stops[i - 1][1][j]) * k) / 255);
    }
  }
  return [0.94, 0.94, 0.94];
}

export async function render3d(view) {
  dispose();
  view.classList.add('flush');
  if (!S.project) {
    view.innerHTML = `<div class="empty">${icon('cube')}<h3>${esc(t('noProject'))}</h3></div>`;
    return;
  }
  const night = localStorage.getItem('fsos-3d-night') !== '0';
  const exag = +(localStorage.getItem('fsos-3d-exag') || 1.5);
  view.innerHTML = `<div class="v3d"><div class="over">
      <div class="seg"><button data-n="1" class="${night ? 'on' : ''}">${icon('moon')}${esc(t('night'))}</button><button data-n="0" class="${night ? '' : 'on'}">${icon('sun')}${esc(t('day'))}</button></div>
      <div class="seg">${[1, 1.5, 2, 3].map((x) => `<button data-x="${x}" class="${x === exag ? 'on' : ''}">×${num(x, x % 1 ? 1 : 0)}</button>`).join('')}</div>
      <button class="btn sm" id="v3-reset">${icon('target')}${esc(t('resetView'))}</button></div>
    <div class="info" id="v3-info">${esc(t('loading'))}</div></div>`;
  const host = $('.v3d', view);
  let data;
  try {
    data = await invoke('preview_3d', { id: S.project.id });
  } catch (e) {
    $('#v3-info').textContent = String(e);
    return;
  }
  if (S.view !== 'view3d') return;
  if (!data.grid && !data.lights.length) {
    host.insertAdjacentHTML('beforeend', `<div class="empty">${icon('cube')}<h3>${esc(t('nothing3d'))}</h3><div>${esc(t('nothing3dD'))}</div></div>`);
    $('#v3-info').hidden = true;
    return;
  }
  const renderer = new THREE.WebGLRenderer({ antialias: true, alpha: false });
  renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
  host.prepend(renderer.domElement);
  const scene = new THREE.Scene();
  const camera = new THREE.PerspectiveCamera(50, 1, 1, 400000);
  const controls = new OrbitControls(camera, renderer.domElement);
  controls.enableDamping = true;
  controls.maxPolarAngle = Math.PI * 0.495;
  const W = data.widthM;
  const H = data.heightM;
  const g = data.grid;
  const base = g ? g.min : 0;
  const hemi = new THREE.HemisphereLight(0xbfd8ff, 0x203018, 0.9);
  const sun = new THREE.DirectionalLight(0xffffff, 1.6);
  sun.position.set(-W, Math.max(W, H) * 0.9, -H * 0.6);
  scene.add(hemi, sun);

  let terrain = null;
  if (g) {
    const geo = new THREE.PlaneGeometry(W, H, g.w - 1, g.h - 1);
    geo.rotateX(-Math.PI / 2);
    const pos = geo.attributes.position;
    const colors = new Float32Array(pos.count * 3);
    for (let i = 0; i < pos.count; i++) {
      const z = g.z[i];
      pos.setY(i, (z - base) * exag);
      const c = hypso((z - g.min) / Math.max(1, g.max - g.min));
      const inside = g.mask[i];
      colors.set(inside ? c : c.map((v) => v * 0.35), i * 3);
    }
    geo.setAttribute('color', new THREE.BufferAttribute(colors, 3));
    geo.computeVertexNormals();
    terrain = new THREE.Mesh(geo, new THREE.MeshStandardMaterial({ vertexColors: true, roughness: 0.95, metalness: 0 }));
    // Plane jest wyśrodkowany – przesuwamy tak, by (0,0) był w rogu NW jak współrzędne świateł.
    terrain.position.set(W / 2, 0, H / 2);
    scene.add(terrain);
  } else {
    const plane = new THREE.Mesh(new THREE.PlaneGeometry(W, H), new THREE.MeshStandardMaterial({ color: 0x2a3a2a }));
    plane.rotateX(-Math.PI / 2);
    plane.position.set(W / 2, 0, H / 2);
    scene.add(plane);
  }

  // Światła: kropki z poświatą (addytywnie) + słaba „plama” na ziemi.
  const n = data.lights.length;
  const pts = new Float32Array(n * 3);
  const cols = new Float32Array(n * 3);
  const pool = new Float32Array(n * 3);
  for (let i = 0; i < n; i++) {
    const [x, y, top, ground, c] = data.lights[i];
    // Teren jest przewyższony, ale wysokość słupa – nie (lampa zostaje 8 m nad ziemią).
    pts.set([x, (ground - base) * exag + (top - ground), y], i * 3);
    pool.set([x, (ground - base) * exag + 0.5, y], i * 3);
    cols.set(c, i * 3);
  }
  const tex = glowTexture();
  const pGeo = new THREE.BufferGeometry();
  pGeo.setAttribute('position', new THREE.BufferAttribute(pts, 3));
  pGeo.setAttribute('color', new THREE.BufferAttribute(cols, 3));
  const lamps = new THREE.Points(pGeo, new THREE.PointsMaterial({ size: Math.max(6, Math.min(W, H) / 400), map: tex, vertexColors: true, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending, sizeAttenuation: true }));
  const gGeo = new THREE.BufferGeometry();
  gGeo.setAttribute('position', new THREE.BufferAttribute(pool, 3));
  gGeo.setAttribute('color', new THREE.BufferAttribute(cols.map((v) => v * 0.45), 3));
  const pools = new THREE.Points(gGeo, new THREE.PointsMaterial({ size: Math.max(22, Math.min(W, H) / 120), map: tex, vertexColors: true, transparent: true, depthWrite: false, blending: THREE.AdditiveBlending }));
  scene.add(lamps, pools);

  const setNight = (on) => {
    scene.background = new THREE.Color(on ? 0x05070c : 0x9cc4e8);
    scene.fog = new THREE.Fog(on ? 0x05070c : 0x9cc4e8, Math.max(W, H) * 1.2, Math.max(W, H) * 4);
    hemi.intensity = on ? 0.12 : 0.9;
    sun.intensity = on ? 0.05 : 1.6;
    pools.visible = on;
    lamps.material.opacity = on ? 1 : 0.55;
    renderer.toneMappingExposure = on ? 1.2 : 1;
  };
  setNight(night);
  const reset = () => {
    const d = Math.max(W, H);
    camera.position.set(W / 2 - d * 0.15, d * 0.55, H + d * 0.35);
    controls.target.set(W / 2, g ? ((g.max - base) * exag) / 4 : 0, H / 2);
    controls.update();
  };
  reset();
  const resize = () => {
    const r = host.getBoundingClientRect();
    renderer.setSize(r.width, r.height, false);
    camera.aspect = r.width / Math.max(1, r.height);
    camera.updateProjectionMatrix();
  };
  const ro = new ResizeObserver(resize);
  ro.observe(host);
  resize();
  const loop = () => {
    ctx.raf = requestAnimationFrame(loop);
    controls.update();
    renderer.render(scene, camera);
  };
  ctx = { renderer, scene, ro, raf: 0 };
  loop();
  $('#v3-info').textContent = `${num(W / 1000, 2)} × ${num(H / 1000, 2)} km · ${g ? `${num(g.min, 0)}–${num(g.max, 0)} m n.p.m. · ` : ''}${t('lightsTotal')}: ${num(n)}`;
  $$('[data-n]', view).forEach((b) => (b.onclick = () => { localStorage.setItem('fsos-3d-night', b.dataset.n); $$('[data-n]', view).forEach((x) => x.classList.toggle('on', x === b)); setNight(b.dataset.n === '1'); }));
  $$('[data-x]', view).forEach((b) => (b.onclick = () => { localStorage.setItem('fsos-3d-exag', b.dataset.x); render3d(view); }));
  $('#v3-reset').onclick = reset;
}

export function leave3d() { dispose(); }
