//! Rzeźba terenu dla MSFS: obszar dzielimy na siatkę prostokątów z heightmapą
//! (`<Rectangle><Heightmap/></Rectangle>` w XML scenerii). Prostokąt ma oś środkową
//! z południa na północ, wiersz 0 heightmapy leży na południu, kolumna 0 na zachodzie;
//! sąsiednie kafle dzielą krawędź (te same próbki), więc nie ma szwów.
//!
//! Obliczenia: próbkowanie mozaiki źródeł, uzupełnianie dziur, wygładzanie (z zapasem
//! wokół kafla – wynik identyczny po obu stronach krawędzi), spłaszczanie jezior z OSM,
//! pomijanie kafli płaskich i morskich, statystyki i podgląd z cieniowaniem.

use crate::dem::{self, Mosaic, Source};
use crate::geo::{self, Area, BBox, LatLon};
use crate::osm::Element;
use crate::store::TerrainCfg;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

const M_PER_DEG: f64 = geo::R_EARTH * std::f64::consts::PI / 180.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GridSpec {
    pub lat0: f64,
    pub lon0: f64,
    pub dlat: f64,
    pub dlon: f64,
    /// Próbek na bok kafla.
    pub n: usize,
    pub resolution: f64,
}

impl GridSpec {
    pub fn new(area: &BBox, resolution: f64, n: usize) -> Self {
        let res = resolution.clamp(0.5, 200.0);
        let dlat = res / M_PER_DEG;
        let dlon = res / (M_PER_DEG * area.center().lat.to_radians().cos());
        // Wyrównanie do wielokrotności kroku – te same ustawienia dają tę samą siatkę.
        let lat0 = (area.south / dlat).floor() * dlat;
        let lon0 = (area.west / dlon).floor() * dlon;
        Self { lat0, lon0, dlat, dlon, n: n.clamp(9, 256), resolution: res }
    }

    pub fn tile_bbox(&self, i: i32, j: i32) -> BBox {
        let span = (self.n - 1) as f64;
        BBox {
            south: self.lat0 + j as f64 * span * self.dlat,
            north: self.lat0 + (j + 1) as f64 * span * self.dlat,
            west: self.lon0 + i as f64 * span * self.dlon,
            east: self.lon0 + (i + 1) as f64 * span * self.dlon,
        }
    }

    /// Kafle przecinające obszar.
    pub fn tiles_for(&self, area: &Area) -> Vec<(i32, i32)> {
        let b = area.bbox();
        let span = (self.n - 1) as f64;
        let i1 = ((b.east - self.lon0) / (span * self.dlon)).floor() as i32;
        let j1 = ((b.north - self.lat0) / (span * self.dlat)).floor() as i32;
        let mut out = vec![];
        for j in 0..=j1.max(0) {
            for i in 0..=i1.max(0) {
                if geo::polygon_intersects_bbox(&area.outer, &self.tile_bbox(i, j)) {
                    out.push((i, j));
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tile {
    pub i: i32,
    pub j: i32,
    pub bbox: BBox,
    pub mean: f32,
    pub min: f32,
    pub max: f32,
    /// Średnie odchylenie od wersji zgrubnej (ile szczegółu dodaje kafel), m.
    pub detail: f32,
    #[serde(skip)]
    pub h: Vec<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    pub computed: String,
    pub tiles: usize,
    pub skipped_flat: usize,
    pub skipped_sea: usize,
    pub skipped_nodata: usize,
    pub lakes: usize,
    pub runways: usize,
    /// Opis wyrównanych pasów: oznaczenie, długość, nachylenie.
    #[serde(default)]
    pub runway_info: Vec<String>,
    pub min: f32,
    pub max: f32,
    pub resolution: f64,
    pub tile_size_m: f64,
    pub samples: usize,
    pub xml_mb: f64,
    /// Udział źródeł w próbkach (nazwa, procent).
    pub sources: Vec<(String, f64)>,
    pub warnings: Vec<String>,
}

pub struct Terrain {
    pub grid: GridSpec,
    pub tiles: Vec<Tile>,
    pub summary: Summary,
    index: HashMap<(i32, i32), usize>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Plan {
    pub tiles: usize,
    pub tile_size_m: f64,
    pub area_km2: f64,
    pub samples: usize,
    pub xml_mb: f64,
    pub too_big: bool,
}

pub fn plan(cfg: &TerrainCfg, area: &Area) -> Plan {
    let g = GridSpec::new(&area.bbox(), cfg.resolution, cfg.tile as usize);
    let tiles = g.tiles_for(area).len();
    let samples = tiles * g.n * g.n;
    Plan {
        tiles,
        tile_size_m: (g.n - 1) as f64 * g.resolution,
        area_km2: area.area_m2() / 1e6,
        samples,
        xml_mb: samples as f64 * 7.5 / 1e6,
        too_big: tiles > 6000 || samples > 120_000_000,
    }
}

/// Mozaika źródeł zgodna z ustawieniami projektu.
pub fn build_mosaic(cfg: &TerrainCfg, cache: &Path, area: &BBox, warnings: &mut Vec<String>) -> Result<Mosaic, String> {
    let mut sources: Vec<Box<dyn Source>> = vec![];
    let lat = area.center().lat;
    for s in &cfg.sources {
        match s.as_str() {
            "files" => {
                for f in cfg.files.iter().filter(|f| f.enabled) {
                    match dem::read_file(Path::new(&f.path), f.epsg, f.swap_xy) {
                        Ok(r) => {
                            let fp = r.footprint();
                            if !fp.intersects(area) {
                                warnings.push(format!("Plik {} leży poza obszarem projektu", short_name(&f.path)));
                                continue;
                            }
                            sources.push(Box::new(dem::FileSource { name: short_name(&f.path), raster: r, footprint: fp, offset: f.offset as f32 }));
                        }
                        Err(e) => warnings.push(format!("{}: {e}", short_name(&f.path))),
                    }
                }
            }
            "gugik" => {
                if dem::POLAND.intersects(area) {
                    sources.push(Box::new(dem::Wcs::gugik(cache, cfg.resolution)));
                }
            }
            "wcs" => {
                if !cfg.wcs_url.trim().is_empty() && !cfg.wcs_coverage.trim().is_empty() && cfg.wcs_epsg > 0 {
                    match dem::Wcs::new("wcs", "WCS", cfg.wcs_url.trim(), cfg.wcs_coverage.trim(), cfg.wcs_epsg, cache, cfg.resolution, None) {
                        Ok(w) => sources.push(Box::new(w)),
                        Err(e) => warnings.push(e),
                    }
                }
            }
            "terrarium" => sources.push(Box::new(dem::Terrarium::new(cache, cfg.resolution, lat))),
            "copernicus30" => sources.push(Box::new(dem::Copernicus::new(cache, false))),
            "copernicus90" => sources.push(Box::new(dem::Copernicus::new(cache, true))),
            _ => {}
        }
    }
    if sources.is_empty() {
        return Err("Brak źródeł danych wysokościowych dla tego obszaru – włącz AWS Terrain Tiles albo dodaj własny plik".into());
    }
    Ok(Mosaic { sources, offset: cfg.offset as f32 })
}

fn short_name(p: &str) -> String {
    Path::new(p).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| p.to_string())
}

/// Jeziora z OSM: wielokąt + poziom wody (10. percentyl wysokości wewnątrz).
pub struct Lake {
    pub ring: Vec<LatLon>,
    pub holes: Vec<Vec<LatLon>>,
    pub bbox: BBox,
    pub level: f32,
}

pub fn lakes(elements: &[Element], area: &BBox, mosaic: &Mosaic, res: f64) -> Vec<Lake> {
    let mut out = vec![];
    for e in elements {
        let water = e.is("natural", "water") || e.is("waterway", "riverbank") || matches!(e.tag("landuse"), Some("reservoir") | Some("basin"));
        if !water || !e.is_area() {
            continue;
        }
        let flowing = matches!(e.tag("water"), Some("river") | Some("stream") | Some("canal") | Some("rapids")) || e.is("waterway", "riverbank");
        for ring in &e.outers {
            let b = BBox::of(ring);
            if !b.intersects(area) {
                continue;
            }
            let a = geo::polygon_area(ring);
            // Rzeki opadają wzdłuż biegu – spłaszczamy tylko krótkie odcinki.
            let diag = (b.width_m().powi(2) + b.height_m().powi(2)).sqrt();
            if a < 400.0 || (flowing && diag > 1500.0) {
                continue;
            }
            let step = (a.sqrt() / 40.0).max(res);
            let loc = geo::Local::new(b.center());
            let (w, h) = (b.width_m(), b.height_m());
            let mut vals = vec![];
            let mut y = -h / 2.0;
            while y <= h / 2.0 {
                let mut x = -w / 2.0;
                while x <= w / 2.0 {
                    let p = loc.to_ll(x, y);
                    if geo::point_in_polygon(p, ring) {
                        if let Some((v, _)) = mosaic.sample(p) {
                            vals.push(v);
                        }
                    }
                    x += step;
                }
                y += step;
            }
            if vals.len() < 4 {
                continue;
            }
            vals.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            let level = vals[vals.len() / 10];
            out.push(Lake { ring: ring.clone(), holes: e.inners.clone(), bbox: b, level });
        }
    }
    out
}

/// Pas startowy z OSM: oś, szerokość i wyrównany profil podłużny (prosta dopasowana
/// metodą najmniejszych kwadratów do wysokości wzdłuż osi). Teren pod pasem i poboczem
/// staje się gładki, a dalej łagodnie wraca do danych.
pub struct Runway {
    pub ident: String,
    loc: geo::Local,
    ax: (f64, f64),
    dir: (f64, f64),
    len: f64,
    half: f64,
    h0: f64,
    slope: f64,
    pub bbox: BBox,
}

/// Pobocze płaskie jak pas i strefa przejścia do terenu z danych (m).
const RWY_SHOULDER: f64 = 12.0;
const RWY_BLEND: f64 = 60.0;

impl Runway {
    /// Wysokość wyrównana i waga (1 – w pełni płasko, 0 – bez zmian) w punkcie.
    pub fn flatten(&self, p: LatLon) -> Option<(f32, f32)> {
        let (x, y) = self.loc.to_xy(p);
        let (rx, ry) = (x - self.ax.0, y - self.ax.1);
        let s = rx * self.dir.0 + ry * self.dir.1;
        let d = (rx * -self.dir.1 + ry * self.dir.0).abs();
        let ext = RWY_SHOULDER + RWY_BLEND;
        if s < -ext || s > self.len + ext || d > self.half + ext {
            return None;
        }
        let sc = s.clamp(0.0, self.len);
        let h = self.h0 + self.slope * sc;
        // Odległość od prostokąta pasa (z poboczem) – w poprzek i wzdłuż za progami.
        let over_d = (d - self.half - RWY_SHOULDER).max(0.0);
        let over_s = if s < 0.0 { -s } else if s > self.len { s - self.len } else { 0.0 };
        let over = (over_d * over_d + over_s * over_s).sqrt();
        if over >= RWY_BLEND {
            return None;
        }
        let t = over / RWY_BLEND;
        let w = 1.0 - t * t * (3.0 - 2.0 * t);
        Some((h as f32, w as f32))
    }
}

pub fn runways(elements: &[Element], area: &BBox, mosaic: &Mosaic) -> Vec<Runway> {
    let mut out = vec![];
    for e in elements.iter().filter(|e| e.is("aeroway", "runway") && !e.is_area() && e.line.len() >= 2) {
        let a = e.line[0];
        let b = *e.line.last().unwrap();
        let bb = BBox::of(&[a, b]).grow_m(200.0);
        if !bb.intersects(area) {
            continue;
        }
        let loc = geo::Local::new(a);
        let (bx, by) = loc.to_xy(b);
        let len = (bx * bx + by * by).sqrt();
        if len < 150.0 {
            continue;
        }
        let dir = (bx / len, by / len);
        let grass = matches!(e.tag("surface"), Some("grass") | Some("dirt") | Some("ground") | Some("gravel") | Some("unpaved"));
        let width = e.tag("width").and_then(crate::osm::parse_height).filter(|w| (8.0..90.0).contains(w)).unwrap_or(if grass { 25.0 } else { 40.0 });
        // Profil: wysokości co ~10 m po osi i w dwóch liniach równoległych, prosta MNK.
        let n = ((len / 10.0) as usize).clamp(8, 600);
        let (mut sx, mut sy, mut sxx, mut sxy, mut cnt) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for k in 0..=n {
            let s = len * k as f64 / n as f64;
            for off in [-width / 4.0, 0.0, width / 4.0] {
                let p = loc.to_ll(dir.0 * s - dir.1 * off, dir.1 * s + dir.0 * off);
                if let Some((v, _)) = mosaic.sample(p) {
                    let v = v as f64;
                    sx += s;
                    sy += v;
                    sxx += s * s;
                    sxy += s * v;
                    cnt += 1.0;
                }
            }
        }
        if cnt < 8.0 {
            continue;
        }
        let den = cnt * sxx - sx * sx;
        let slope = if den.abs() > 1e-9 { (cnt * sxy - sx * sy) / den } else { 0.0 };
        // Pasy mają nachylenie do ~2% – większe to szum danych albo błędna oś.
        let slope = slope.clamp(-0.02, 0.02);
        let h0 = (sy - slope * sx) / cnt;
        let ident = e.tag("ref").unwrap_or("").to_string();
        out.push(Runway { ident, loc, ax: (0.0, 0.0), dir, len, half: width / 2.0, h0: h0 - mosaic.offset as f64, slope, bbox: BBox::of(&[a, b]).grow_m(width / 2.0 + RWY_SHOULDER + RWY_BLEND) });
    }
    out
}

fn gaussian_blur(data: &mut [f32], w: usize, h: usize, sigma: f64) {
    if sigma < 0.3 {
        return;
    }
    let r = (sigma * 2.5).ceil() as isize;
    let k: Vec<f32> = (-r..=r).map(|x| (-(x * x) as f64 / (2.0 * sigma * sigma)).exp() as f32).collect();
    let ks: f32 = k.iter().sum();
    let k: Vec<f32> = k.into_iter().map(|v| v / ks).collect();
    let mut tmp = vec![0f32; data.len()];
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for (o, kv) in k.iter().enumerate() {
                let xx = (x as isize + o as isize - r).clamp(0, w as isize - 1) as usize;
                s += data[y * w + xx] * kv;
            }
            tmp[y * w + x] = s;
        }
    }
    for y in 0..h {
        for x in 0..w {
            let mut s = 0.0;
            for (o, kv) in k.iter().enumerate() {
                let yy = (y as isize + o as isize - r).clamp(0, h as isize - 1) as usize;
                s += tmp[yy * w + x] * kv;
            }
            data[y * w + x] = s;
        }
    }
}

/// Uzupełnianie dziur średnią z sąsiadów (kolejne przejścia od krawędzi dziury do środka).
fn fill_holes(data: &mut [f32], w: usize, h: usize) -> usize {
    let mut missing = data.iter().filter(|v| v.is_nan()).count();
    let mut pass = 0;
    while missing > 0 && pass < 200 {
        let src = data.to_vec();
        let mut changed = 0;
        for y in 0..h {
            for x in 0..w {
                if !src[y * w + x].is_nan() {
                    continue;
                }
                let (mut s, mut n) = (0.0f32, 0);
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1), (-1, 1), (1, -1)] {
                    let (xx, yy) = (x as i32 + dx, y as i32 + dy);
                    if xx >= 0 && yy >= 0 && (xx as usize) < w && (yy as usize) < h {
                        let v = src[yy as usize * w + xx as usize];
                        if !v.is_nan() {
                            s += v;
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    data[y * w + x] = s / n as f32;
                    changed += 1;
                }
            }
        }
        if changed == 0 {
            break;
        }
        missing -= changed;
        pass += 1;
    }
    missing
}

/// Szczegółowość kafla: RMS różnicy względem wersji próbkowanej co 8 punktów.
fn detail(h: &[f32], n: usize) -> f32 {
    let step = 8usize.min(n - 1);
    let coarse = |x: usize, y: usize| -> f32 {
        let (x0, y0) = ((x / step) * step, (y / step) * step);
        let (x1, y1) = ((x0 + step).min(n - 1), (y0 + step).min(n - 1));
        let tx = if x1 > x0 { (x - x0) as f32 / (x1 - x0) as f32 } else { 0.0 };
        let ty = if y1 > y0 { (y - y0) as f32 / (y1 - y0) as f32 } else { 0.0 };
        let g = |xx: usize, yy: usize| h[yy * n + xx];
        g(x0, y0) * (1.0 - tx) * (1.0 - ty) + g(x1, y0) * tx * (1.0 - ty) + g(x0, y1) * (1.0 - tx) * ty + g(x1, y1) * tx * ty
    };
    let mut s = 0.0f64;
    for y in 0..n {
        for x in 0..n {
            let d = (h[y * n + x] - coarse(x, y)) as f64;
            s += d * d;
        }
    }
    (s / (n * n) as f64).sqrt() as f32
}

pub struct Progress<'a> {
    pub done: &'a AtomicUsize,
    pub cancel: &'a AtomicBool,
}

pub fn compute(cfg: &TerrainCfg, area: &Area, mosaic: &Mosaic, lakes: &[Lake], runways: &[Runway], prog: Progress, mut warnings: Vec<String>) -> Result<Terrain, String> {
    let grid = GridSpec::new(&area.bbox(), cfg.resolution, cfg.tile as usize);
    let list = grid.tiles_for(area);
    if list.is_empty() {
        return Err("Obszar projektu jest pusty".into());
    }
    let n = grid.n;
    let sigma_px = cfg.smooth / grid.resolution;
    let pad = if sigma_px >= 0.3 { (sigma_px * 2.5).ceil() as usize } else { 0 };
    let src_count: Vec<AtomicUsize> = (0..mosaic.sources.len()).map(|_| AtomicUsize::new(0)).collect();
    let lake_hits = AtomicUsize::new(0);

    let results: Vec<Result<Option<(Tile, &'static str)>, String>> = list
        .par_iter()
        .map(|&(ti, tj)| {
            if prog.cancel.load(Ordering::Relaxed) {
                return Err("Przerwano".into());
            }
            let w = n + 2 * pad;
            let mut buf = vec![f32::NAN; w * w];
            let base_i = ti as i64 * (n as i64 - 1) - pad as i64;
            let base_j = tj as i64 * (n as i64 - 1) - pad as i64;
            let mut local_src = vec![0usize; mosaic.sources.len()];
            let tb = grid.tile_bbox(ti, tj).grow_m(pad as f64 * grid.resolution + 1.0);
            let near_lakes: Vec<&Lake> = lakes.iter().filter(|l| l.bbox.intersects(&tb)).collect();
            for y in 0..w {
                for x in 0..w {
                    let lat = grid.lat0 + (base_j + y as i64) as f64 * grid.dlat;
                    let lon = grid.lon0 + (base_i + x as i64) as f64 * grid.dlon;
                    let p = LatLon::new(lat, lon);
                    if let Some(l) = near_lakes.iter().find(|l| l.bbox.contains(p) && geo::point_in_polygon(p, &l.ring) && !l.holes.iter().any(|h| geo::point_in_polygon(p, h))) {
                        buf[y * w + x] = l.level + mosaic.offset;
                        lake_hits.fetch_add(1, Ordering::Relaxed);
                        continue;
                    }
                    if let Some((v, si)) = mosaic.sample(p) {
                        buf[y * w + x] = v;
                        local_src[si] += 1;
                    }
                }
            }
            for (i, c) in local_src.iter().enumerate() {
                src_count[i].fetch_add(*c, Ordering::Relaxed);
            }
            let valid = buf.iter().filter(|v| !v.is_nan()).count();
            prog.done.fetch_add(1, Ordering::Relaxed);
            if valid * 50 < buf.len() {
                return Ok(Some((Tile { i: ti, j: tj, bbox: grid.tile_bbox(ti, tj), mean: 0.0, min: 0.0, max: 0.0, detail: 0.0, h: vec![] }, "nodata")));
            }
            fill_holes(&mut buf, w, w);
            if pad > 0 {
                gaussian_blur(&mut buf, w, w, sigma_px);
            }
            let mut h = Vec::with_capacity(n * n);
            for y in 0..n {
                h.extend_from_slice(&buf[(y + pad) * w + pad..(y + pad) * w + pad + n]);
            }
            // Wody z OSM po wygładzeniu muszą zostać płaskie.
            if !near_lakes.is_empty() && pad > 0 {
                for y in 0..n {
                    for x in 0..n {
                        let p = LatLon::new(grid.lat0 + (tj as i64 * (n as i64 - 1) + y as i64) as f64 * grid.dlat, grid.lon0 + (ti as i64 * (n as i64 - 1) + x as i64) as f64 * grid.dlon);
                        if let Some(l) = near_lakes.iter().find(|l| l.bbox.contains(p) && geo::point_in_polygon(p, &l.ring)) {
                            h[y * n + x] = l.level + mosaic.offset;
                        }
                    }
                }
            }
            // Pasy startowe: gładki profil podłużny, płasko w poprzek, łagodne przejście.
            let near_rwy: Vec<&Runway> = runways.iter().filter(|r| r.bbox.intersects(&tb)).collect();
            if !near_rwy.is_empty() {
                for y in 0..n {
                    for x in 0..n {
                        let p = LatLon::new(grid.lat0 + (tj as i64 * (n as i64 - 1) + y as i64) as f64 * grid.dlat, grid.lon0 + (ti as i64 * (n as i64 - 1) + x as i64) as f64 * grid.dlon);
                        let mut best: Option<(f32, f32)> = None;
                        for r in &near_rwy {
                            if let Some(f) = r.flatten(p) {
                                if best.map(|b| f.1 > b.1).unwrap_or(true) {
                                    best = Some(f);
                                }
                            }
                        }
                        if let Some((rh, wgt)) = best {
                            let v = &mut h[y * n + x];
                            *v = *v * (1.0 - wgt) + (rh + mosaic.offset) * wgt;
                        }
                    }
                }
            }
            let (mut lo, mut hi, mut sum) = (f32::MAX, f32::MIN, 0.0f64);
            for v in &h {
                lo = lo.min(*v);
                hi = hi.max(*v);
                sum += *v as f64;
            }
            let mean = (sum / h.len() as f64) as f32;
            let det = detail(&h, n);
            let tile = Tile { i: ti, j: tj, bbox: grid.tile_bbox(ti, tj), mean, min: lo, max: hi, detail: det, h };
            if hi < 0.5 && hi - lo < 1.0 {
                return Ok(Some((tile, "sea")));
            }
            if cfg.skip_flat && det < cfg.skip_threshold as f32 {
                return Ok(Some((tile, "flat")));
            }
            Ok(Some((tile, "")))
        })
        .collect();

    let mut summary = Summary { resolution: grid.resolution, tile_size_m: (n - 1) as f64 * grid.resolution, ..Default::default() };
    let mut tiles = vec![];
    for r in results {
        let Some((t, why)) = r? else { continue };
        match why {
            "nodata" => summary.skipped_nodata += 1,
            "sea" => summary.skipped_sea += 1,
            "flat" => summary.skipped_flat += 1,
            _ => tiles.push(t),
        }
    }
    tiles.sort_by_key(|t| (t.j, t.i));
    if tiles.is_empty() {
        warnings.push("Żaden kafel nie wymaga zmian – sprawdź źródła danych albo próg pomijania płaskich kafli".into());
    }
    summary.min = tiles.iter().map(|t| t.min).fold(f32::MAX, f32::min);
    summary.max = tiles.iter().map(|t| t.max).fold(f32::MIN, f32::max);
    if tiles.is_empty() {
        summary.min = 0.0;
        summary.max = 0.0;
    }
    summary.tiles = tiles.len();
    summary.samples = tiles.len() * n * n;
    summary.xml_mb = summary.samples as f64 * 7.5 / 1e6;
    summary.lakes = lakes.len();
    summary.runways = runways.len();
    summary.runway_info = runways.iter().map(|r| format!("{} · {:.2} km · {:.2}%", if r.ident.is_empty() { "RWY" } else { &r.ident }, r.len / 1000.0, r.slope * 100.0)).collect();
    let total: usize = src_count.iter().map(|c| c.load(Ordering::Relaxed)).sum::<usize>() + lake_hits.load(Ordering::Relaxed);
    if total > 0 {
        for (i, s) in mosaic.sources.iter().enumerate() {
            let c = src_count[i].load(Ordering::Relaxed);
            if c > 0 {
                summary.sources.push((s.label(), c as f64 * 100.0 / total as f64));
            }
        }
        let lh = lake_hits.load(Ordering::Relaxed);
        if lh > 0 {
            summary.sources.push(("Wody OSM (spłaszczone)".into(), lh as f64 * 100.0 / total as f64));
        }
    }
    summary.computed = crate::store::now();
    summary.warnings = warnings;
    Ok(Terrain::new(grid, tiles, summary))
}

impl Terrain {
    pub fn new(grid: GridSpec, tiles: Vec<Tile>, summary: Summary) -> Self {
        let index = tiles.iter().enumerate().map(|(k, t)| ((t.i, t.j), k)).collect();
        Self { grid, tiles, summary, index }
    }

    /// Wysokość z policzonej siatki (interpolacja dwuliniowa w kaflu).
    pub fn sample(&self, p: LatLon) -> Option<f32> {
        let n = self.grid.n;
        let gi = (p.lon - self.grid.lon0) / self.grid.dlon;
        let gj = (p.lat - self.grid.lat0) / self.grid.dlat;
        let span = (n - 1) as f64;
        let (ti, tj) = ((gi / span).floor() as i32, (gj / span).floor() as i32);
        let t = &self.tiles[*self.index.get(&(ti, tj))?];
        let (li, lj) = (gi - ti as f64 * span, gj - tj as f64 * span);
        let (x0, y0) = (li.floor().clamp(0.0, span - 1.0) as usize, lj.floor().clamp(0.0, span - 1.0) as usize);
        let (tx, ty) = ((li - x0 as f64) as f32, (lj - y0 as f64) as f32);
        let g = |x: usize, y: usize| t.h[y * n + x];
        Some(g(x0, y0) * (1.0 - tx) * (1.0 - ty) + g(x0 + 1, y0) * tx * (1.0 - ty) + g(x0, y0 + 1) * (1.0 - tx) * ty + g(x0 + 1, y0 + 1) * tx * ty)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        #[derive(Serialize)]
        struct Head<'a> {
            grid: &'a GridSpec,
            tiles: &'a [Tile],
            summary: &'a Summary,
        }
        let head = serde_json::to_vec(&Head { grid: &self.grid, tiles: &self.tiles, summary: &self.summary }).map_err(|e| e.to_string())?;
        let mut out = Vec::with_capacity(16 + head.len() + self.tiles.len() * self.grid.n * self.grid.n * 4);
        out.extend_from_slice(b"FSOSTER1");
        out.extend_from_slice(&(head.len() as u64).to_le_bytes());
        out.extend_from_slice(&head);
        for t in &self.tiles {
            for v in &t.h {
                out.extend_from_slice(&v.to_le_bytes());
            }
        }
        crate::store::write_atomic(path, &out)
    }

    pub fn load(path: &Path) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct Head {
            grid: GridSpec,
            tiles: Vec<Tile>,
            summary: Summary,
        }
        let b = std::fs::read(path).map_err(|_| "Teren nie został jeszcze policzony".to_string())?;
        if b.len() < 16 || &b[..8] != b"FSOSTER1" {
            return Err("Uszkodzony plik terenu".into());
        }
        let hl = u64::from_le_bytes(b[8..16].try_into().unwrap()) as usize;
        let head: Head = serde_json::from_slice(&b[16..16 + hl]).map_err(|e| e.to_string())?;
        let n = head.grid.n;
        let mut off = 16 + hl;
        let mut tiles = head.tiles;
        for t in tiles.iter_mut() {
            let end = off + n * n * 4;
            if end > b.len() {
                return Err("Uszkodzony plik terenu".into());
            }
            t.h = b[off..end].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
            off = end;
        }
        Ok(Self::new(head.grid, tiles, head.summary))
    }

    /// Podgląd: barwy hipsometryczne + cieniowanie, przezroczystość poza obszarem. PNG.
    pub fn preview_png(&self, area: &Area, max_px: u32) -> Result<(Vec<u8>, BBox), String> {
        let b = area.bbox();
        let (wm, hm) = (b.width_m().max(1.0), b.height_m().max(1.0));
        let scale = (max_px as f64 / wm.max(hm)).min(1.0 / (self.grid.resolution * 0.5));
        let w = ((wm * scale).round() as u32).clamp(16, max_px);
        let h = ((hm * scale).round() as u32).clamp(16, max_px);
        let px_m = wm / w as f64;
        let mut heights = vec![f32::NAN; (w * h) as usize];
        heights.par_chunks_mut(w as usize).enumerate().for_each(|(y, row)| {
            for (x, v) in row.iter_mut().enumerate() {
                let p = LatLon::new(b.north - (y as f64 + 0.5) / h as f64 * (b.north - b.south), b.west + (x as f64 + 0.5) / w as f64 * (b.east - b.west));
                if area.contains(p) {
                    *v = self.sample(p).unwrap_or(f32::NAN);
                }
            }
        });
        let (lo, hi) = (self.summary.min, self.summary.max.max(self.summary.min + 1.0));
        let mut img = image::RgbaImage::new(w, h);
        for y in 0..h as usize {
            for x in 0..w as usize {
                let v = heights[y * w as usize + x];
                if v.is_nan() {
                    img.put_pixel(x as u32, y as u32, image::Rgba([0, 0, 0, 0]));
                    continue;
                }
                let at = |xx: usize, yy: usize| {
                    let q = heights[yy.min(h as usize - 1) * w as usize + xx.min(w as usize - 1)];
                    if q.is_nan() {
                        v
                    } else {
                        q
                    }
                };
                let dzdx = (at(x + 1, y) - at(x.saturating_sub(1), y)) as f64 / (2.0 * px_m);
                let dzdy = (at(x, y.saturating_sub(1)) - at(x, y + 1)) as f64 / (2.0 * px_m);
                let shade = hillshade(dzdx * 1.6, dzdy * 1.6);
                let c = hypso(((v - lo) / (hi - lo)) as f64);
                let k = 0.35 + 0.75 * shade;
                img.put_pixel(x as u32, y as u32, image::Rgba([(c[0] * k).min(255.0) as u8, (c[1] * k).min(255.0) as u8, (c[2] * k).min(255.0) as u8, 235]));
            }
        }
        let mut out = std::io::Cursor::new(vec![]);
        img.write_to(&mut out, image::ImageFormat::Png).map_err(|e| e.to_string())?;
        Ok((out.into_inner(), b))
    }

    /// Zapis policzonego terenu jako GeoTIFF (WGS 84, float32, krok siatki projektu,
    /// −9999 poza kaflami). Zwraca wymiary obrazu.
    pub fn write_geotiff(&self, path: &Path) -> Result<(u32, u32), String> {
        use tiff::encoder::{colortype, TiffEncoder};
        use tiff::tags::Tag;
        let span = (self.grid.n - 1) as i32;
        let (i0, i1) = (self.tiles.iter().map(|t| t.i).min().ok_or("Brak policzonego terenu")?, self.tiles.iter().map(|t| t.i).max().unwrap_or(0));
        let (j0, j1) = (self.tiles.iter().map(|t| t.j).min().unwrap_or(0), self.tiles.iter().map(|t| t.j).max().unwrap_or(0));
        let w = ((i1 - i0 + 1) * span + 1) as u32;
        let h = ((j1 - j0 + 1) * span + 1) as u32;
        if (w as u64) * (h as u64) > 400_000_000 {
            return Err("Teren jest za duży na jeden plik GeoTIFF".into());
        }
        let mut data = vec![-9999f32; (w * h) as usize];
        for t in &self.tiles {
            let n = self.grid.n;
            for y in 0..n {
                for x in 0..n {
                    let gx = ((t.i - i0) * span) as usize + x;
                    let gy_s = ((t.j - j0) * span) as usize + y;
                    let row = h as usize - 1 - gy_s;
                    data[row * w as usize + gx] = t.h[y * n + x];
                }
            }
        }
        let lon0 = self.grid.lon0 + (i0 * span) as f64 * self.grid.dlon;
        let lat_top = self.grid.lat0 + ((j1 + 1) * span) as f64 * self.grid.dlat;
        let f = std::fs::File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let mut enc = TiffEncoder::new(std::io::BufWriter::new(f)).map_err(|e| e.to_string())?;
        let mut img = enc.new_image::<colortype::Gray32Float>(w, h).map_err(|e| e.to_string())?;
        let e = img.encoder();
        // Próbki są w węzłach siatki – RasterPixelIsPoint.
        e.write_tag(Tag::from_u16_exhaustive(33550), &[self.grid.dlon, self.grid.dlat, 0.0][..]).map_err(|e| e.to_string())?;
        e.write_tag(Tag::from_u16_exhaustive(33922), &[0.0, 0.0, 0.0, lon0, lat_top, 0.0][..]).map_err(|e| e.to_string())?;
        e.write_tag(Tag::from_u16_exhaustive(34735), &[1u16, 1, 0, 3, 1024, 0, 1, 2, 1025, 0, 1, 2, 2048, 0, 1, 4326][..]).map_err(|e| e.to_string())?;
        e.write_tag(Tag::from_u16_exhaustive(42113), "-9999").map_err(|e| e.to_string())?;
        img.write_data(&data).map_err(|e| e.to_string())?;
        Ok((w, h))
    }

    /// Siatka do podglądu 3D (co najwyżej `max_n` próbek na bok).
    pub fn grid3d(&self, area: &Area, max_n: usize) -> Grid3d {
        let b = area.bbox();
        let (wm, hm) = (b.width_m().max(1.0), b.height_m().max(1.0));
        let step = (wm.max(hm) / max_n as f64).max(self.grid.resolution);
        let w = ((wm / step).round() as usize).clamp(8, max_n);
        let h = ((hm / step).round() as usize).clamp(8, max_n);
        let mut z = vec![f32::NAN; w * h];
        z.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, v) in row.iter_mut().enumerate() {
                let p = LatLon::new(b.north - y as f64 / (h - 1) as f64 * (b.north - b.south), b.west + x as f64 / (w - 1) as f64 * (b.east - b.west));
                *v = self.sample(p).unwrap_or(f32::NAN);
            }
        });
        let fill = z.iter().cloned().filter(|v| !v.is_nan()).fold(f32::MAX, f32::min);
        let fill = if fill == f32::MAX { 0.0 } else { fill };
        let mask: Vec<u8> = (0..w * h)
            .map(|k| {
                let (x, y) = (k % w, k / w);
                let p = LatLon::new(b.north - y as f64 / (h - 1) as f64 * (b.north - b.south), b.west + x as f64 / (w - 1) as f64 * (b.east - b.west));
                area.contains(p) as u8
            })
            .collect();
        for v in z.iter_mut() {
            if v.is_nan() {
                *v = fill;
            }
        }
        Grid3d { w, h, width_m: wm, height_m: hm, bbox: b, z, mask, min: self.summary.min, max: self.summary.max }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Grid3d {
    pub w: usize,
    pub h: usize,
    pub width_m: f64,
    pub height_m: f64,
    pub bbox: BBox,
    pub z: Vec<f32>,
    pub mask: Vec<u8>,
    pub min: f32,
    pub max: f32,
}

fn hillshade(dzdx: f64, dzdy: f64) -> f64 {
    let (az, alt) = (315f64.to_radians(), 45f64.to_radians());
    let slope = (dzdx * dzdx + dzdy * dzdy).sqrt().atan();
    let aspect = dzdy.atan2(-dzdx);
    (alt.sin() * slope.cos() + alt.cos() * slope.sin() * (az - std::f64::consts::FRAC_PI_2 - aspect).cos()).clamp(0.0, 1.0)
}

/// Barwy hipsometryczne (nizina zielona → wyżyna żółtobrązowa → góry jasne).
fn hypso(t: f64) -> [f64; 3] {
    let stops: [(f64, [f64; 3]); 6] = [
        (0.0, [58.0, 120.0, 82.0]),
        (0.2, [110.0, 160.0, 92.0]),
        (0.45, [196.0, 190.0, 120.0]),
        (0.65, [190.0, 150.0, 98.0]),
        (0.85, [160.0, 130.0, 118.0]),
        (1.0, [240.0, 240.0, 240.0]),
    ];
    let t = t.clamp(0.0, 1.0);
    for w in stops.windows(2) {
        if t <= w[1].0 {
            let k = (t - w[0].0) / (w[1].0 - w[0].0);
            return [0, 1, 2].map(|i| w[0].1[i] + (w[1].1[i] - w[0].1[i]) * k);
        }
    }
    stops[5].1
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dem::Raster;

    fn square(lat: f64, lon: f64, d: f64) -> Area {
        Area { outer: vec![LatLon::new(lat, lon), LatLon::new(lat, lon + d), LatLon::new(lat + d, lon + d), LatLon::new(lat + d, lon)], holes: vec![] }
    }

    #[test]
    fn grid_tiles_share_edges() {
        let area = square(50.0, 19.0, 0.01);
        let g = GridSpec::new(&area.bbox(), 5.0, 65);
        let tiles = g.tiles_for(&area);
        assert!(!tiles.is_empty());
        let a = g.tile_bbox(0, 0);
        let b = g.tile_bbox(1, 0);
        assert!((a.east - b.west).abs() < 1e-12);
        let span_m = (a.north - a.south) * M_PER_DEG;
        assert!((span_m - 64.0 * 5.0).abs() < 0.01, "{span_m}");
    }

    #[test]
    fn compute_from_synthetic_dem() {
        // Pochyła płaszczyzna w WGS84: z = 100 + 1000·(lon − 19)
        let (w, h) = (200usize, 200usize);
        let d = 0.0001;
        let mut data = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                let lon = 19.0 + (x as f64 + 0.5) * d;
                data[y * w + x] = (100.0 + 1000.0 * (lon - 19.0)) as f32;
            }
        }
        let r = Raster::new(w, h, data, 4326, 19.0, 50.02, d, d, None).unwrap();
        let fp = r.footprint();
        let mosaic = Mosaic { sources: vec![Box::new(dem::FileSource { name: "t".into(), raster: r, footprint: fp, offset: 0.0 })], offset: 0.0 };
        let area = square(50.002, 19.002, 0.01);
        let cfg = TerrainCfg { resolution: 5.0, tile: 33, ..Default::default() };
        let done = AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        let t = compute(&cfg, &area, &mosaic, &[], &[], Progress { done: &done, cancel: &cancel }, vec![]).unwrap();
        assert!(t.summary.tiles > 4);
        let p = LatLon::new(50.007, 19.006);
        let v = t.sample(p).unwrap();
        assert!((v - 106.0).abs() < 0.05, "{v}");
        // Zapis i odczyt
        let path = std::env::temp_dir().join(format!("fsos-t-{}.bin", crate::store::new_id()));
        t.save(&path).unwrap();
        let t2 = Terrain::load(&path).unwrap();
        assert_eq!(t2.tiles.len(), t.tiles.len());
        assert!((t2.sample(p).unwrap() - v).abs() < 1e-6);
        let (png, _) = t2.preview_png(&area, 256).unwrap();
        assert!(png.starts_with(&[0x89, b'P', b'N', b'G']));
        // GeoTIFF czyta się z powrotem naszym czytnikiem w tym samym miejscu.
        let tif = path.with_extension("tif");
        t2.write_geotiff(&tif).unwrap();
        let back = dem::read_file(&tif, 0, false).unwrap();
        assert_eq!(back.epsg, 4326);
        assert!((back.sample(p).unwrap() - v).abs() < 0.05, "{:?}", back.sample(p));
        let _ = std::fs::remove_file(tif);
        let _ = std::fs::remove_file(path);
        // Płaszczyzna jest „bez szczegółów” – przy pomijaniu płaskich kafli nic nie zostaje.
        let cfg2 = TerrainCfg { skip_flat: true, skip_threshold: 0.05, ..cfg };
        let t3 = compute(&cfg2, &area, &mosaic, &[], &[], Progress { done: &done, cancel: &cancel }, vec![]).unwrap();
        assert_eq!(t3.summary.tiles, 0);
        assert!(t3.summary.skipped_flat > 0);
    }

    #[test]
    fn runway_is_flattened() {
        // Teren z „garbami” co kilka metrów; pas wschód–zachód przez środek.
        let (w, h) = (300usize, 300usize);
        let d = 0.0001;
        let mut data = vec![0f32; w * h];
        for y in 0..h {
            for x in 0..w {
                data[y * w + x] = (200.0 + 3.0 * ((x as f64 * 1.7).sin() * (y as f64 * 1.3).cos())) as f32;
            }
        }
        let r = Raster::new(w, h, data, 4326, 19.0, 50.03, d, d, None).unwrap();
        let fp = r.footprint();
        let mosaic = Mosaic { sources: vec![Box::new(dem::FileSource { name: "t".into(), raster: r, footprint: fp, offset: 0.0 })], offset: 0.0 };
        let mut tags = std::collections::HashMap::new();
        tags.insert("aeroway".to_string(), "runway".to_string());
        tags.insert("width".to_string(), "30".to_string());
        let rwy = Element { t: 'w', id: 1, tags, line: vec![LatLon::new(50.015, 19.006), LatLon::new(50.015, 19.024)], ..Default::default() };
        let area = square(50.006, 19.004, 0.018);
        let rws = runways(&[rwy], &area.bbox(), &mosaic);
        assert_eq!(rws.len(), 1);
        let cfg = TerrainCfg { resolution: 3.0, tile: 65, ..Default::default() };
        let done = AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        let t = compute(&cfg, &area, &mosaic, &[], &rws, Progress { done: &done, cancel: &cancel }, vec![]).unwrap();
        let hs: Vec<f32> = (0..50).map(|k| t.sample(LatLon::new(50.015 + 0.00005 * ((k % 3) as f64 - 1.0), 19.007 + k as f64 * 0.0003)).unwrap()).collect();
        let spread = hs.iter().cloned().fold(f32::MIN, f32::max) - hs.iter().cloned().fold(f32::MAX, f32::min);
        assert!(spread < 0.3, "pas nierówny: {spread}");
        // 300 m od pasa teren zostaje z danych.
        let far = LatLon::new(50.0123, 19.015);
        let orig = mosaic.sample(far).unwrap().0;
        assert!((t.sample(far).unwrap() - orig).abs() < 0.2);
    }

    #[test]
    fn hole_filling_and_blur() {
        let mut d = vec![1.0, 1.0, 1.0, 1.0, f32::NAN, 1.0, 1.0, 1.0, 1.0];
        assert_eq!(fill_holes(&mut d, 3, 3), 0);
        assert!((d[4] - 1.0).abs() < 1e-6);
        let mut e = vec![0.0, 0.0, 0.0, 0.0, 9.0, 0.0, 0.0, 0.0, 0.0];
        gaussian_blur(&mut e, 3, 3, 1.0);
        assert!(e[4] < 9.0 && e[0] > 0.0);
    }
}
