// Kopiuje biblioteki interfejsu do ui/vendor i składa ui/icons.js z ikon Lucide (ISC).
// Uruchamiane ręcznie po zmianie wersji zależności: `npm run vendor`.
import fs from 'node:fs'
import path from 'node:path'
import { fileURLToPath } from 'node:url'

const root = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const nm = path.join(root, 'node_modules')
const vendor = path.join(root, 'ui', 'vendor')
fs.mkdirSync(path.join(vendor, 'images'), { recursive: true })

const copy = (from, to) => fs.copyFileSync(path.join(nm, from), path.join(vendor, to))
copy('leaflet/dist/leaflet.js', 'leaflet.js')
copy('leaflet/dist/leaflet.css', 'leaflet.css')
for (const f of fs.readdirSync(path.join(nm, 'leaflet/dist/images'))) copy(`leaflet/dist/images/${f}`, `images/${f}`)
// three.js (MIT) – podgląd 3D terenu i świateł.
copy('three/build/three.module.min.js', 'three.module.min.js')
copy('three/examples/jsm/controls/OrbitControls.js', 'OrbitControls.js')
// OrbitControls importuje 'three' – w aplikacji bez bundlera podmieniamy na plik lokalny.
const oc = path.join(vendor, 'OrbitControls.js')
fs.writeFileSync(oc, fs.readFileSync(oc, 'utf8').replace(/from 'three'/g, "from './three.module.min.js'"))

const icons = {
  dashboard: 'layout-dashboard', area: 'square-dashed', terrain: 'mountain', lights: 'lamp-wall-down', lamp: 'lamp-floor',
  objects: 'radio-tower', tree: 'trees', cube: 'box', build: 'hammer', packages: 'package', settings: 'settings',
  search: 'search', close: 'x', check: 'check', folder: 'folder-open', trash: 'trash-2', refresh: 'refresh-cw',
  plus: 'plus', minus: 'minus', download: 'download', upload: 'upload', warn: 'triangle-alert', error: 'circle-alert',
  info: 'info', ok: 'circle-check', play: 'play', stop: 'square', edit: 'pencil', copy: 'copy', zip: 'file-archive',
  external: 'external-link', update: 'circle-arrow-up', more: 'ellipsis-vertical', layers: 'layers', globe: 'globe',
  map: 'map', pin: 'map-pin', target: 'crosshair', sliders: 'sliders-horizontal', save: 'save', coffee: 'coffee',
  note: 'notebook-pen', polygon: 'pentagon', rect: 'rectangle-horizontal', file: 'file', fileUp: 'file-up',
  wind: 'wind', lighthouse: 'lightbulb', parking: 'square-parking', pitch: 'goal', road: 'route', train: 'train-front',
  fuel: 'fuel', bolt: 'zap', eye: 'eye', eyeoff: 'eye-off', sun: 'sun', moon: 'moon', water: 'waves', ruler: 'ruler',
  chart: 'chart-area', cpu: 'cpu', database: 'database', hdd: 'hard-drive', rocket: 'rocket', wand: 'wand-sparkles',
  chevron: 'chevron-right', back: 'arrow-left', undo: 'undo-2', power: 'power', link: 'link', box: 'package-open',
  palette: 'palette', grid: 'grid-3x3', home: 'house', walk: 'footprints', ship: 'anchor', mast: 'tower-control',
  scissors: 'scissors', lock: 'lock', star: 'sparkles', terminal: 'square-terminal', dot: 'circle-dot',
}
let out = '// Wygenerowane przez scripts/vendor.mjs – ikony Lucide (ISC), https://lucide.dev\nexport const ICONS = {\n'
for (const [key, name] of Object.entries(icons)) {
  const svg = fs.readFileSync(path.join(nm, 'lucide-static/icons', `${name}.svg`), 'utf8')
  const body = svg.replace(/<!--.*?-->/gs, '').replace(/^[\s\S]*?<svg[^>]*>/, '').replace(/<\/svg>\s*$/, '').replace(/\s*\n\s*/g, '').trim()
  out += `  ${key}: '${body.replace(/'/g, "\\'")}',\n`
}
out += '};\n\nexport function icon(name, cls = \'\') {\n  return `<svg class="ic ${cls}" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${ICONS[name] || ICONS.dot}</svg>`;\n}\n'
fs.writeFileSync(path.join(root, 'ui', 'icons.js'), out)
console.log(`ui/vendor gotowe, ${Object.keys(icons).length} ikon`)
