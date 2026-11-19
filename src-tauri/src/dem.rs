//! Numeryczne modele terenu: wspólny raster, czytniki plików (GeoTIFF, ESRI ASCII Grid, XYZ,
//! SRTM HGT, także w archiwach ZIP) i źródła sieciowe z pamięcią podręczną na dysku:
//!  - NMT GUGiK 1 m (WCS, cała Polska, bez klucza),
//!  - dowolna usługa WCS 1.0 (dla zaawansowanych),
//!  - AWS Terrain Tiles / Terrarium (cały świat; m.in. 3DEP, EU-DEM, LiDAR UK, Austria, Norwegia, NZ),
//!  - Copernicus GLO-30 / GLO-90 (cały świat, model powierzchni – z budynkami i lasem).
//!
//! Źródła składa „mozaika”: w każdym punkcie wygrywa pierwsze źródło, które ma dane.

use crate::geo::{BBox, LatLon};
use crate::net;
use crate::proj::Crs;
use std::collections::{HashMap, VecDeque};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

/// Raster wysokości w dowolnym obsługiwanym układzie. Wiersz 0 jest na górze (północ).
#[derive(Clone)]
pub struct Raster {
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
    pub epsg: u32,
    pub crs: Crs,
    /// Lewy górny róg piksela (0,0) – krawędź, nie środek.
    pub x0: f64,
    pub y0: f64,
    pub dx: f64,
    pub dy: f64,
    pub nodata: Option<f32>,
}

#[inline]
fn valid(v: f32, nodata: Option<f32>) -> bool {
    v.is_finite() && v > -1000.0 && v < 9000.0 && nodata.map(|n| (v - n).abs() > 1e-3).unwrap_or(true)
}

impl Raster {
    #[allow(clippy::too_many_arguments)]
    pub fn new(w: usize, h: usize, data: Vec<f32>, epsg: u32, x0: f64, y0: f64, dx: f64, dy: f64, nodata: Option<f32>) -> Result<Self, String> {
        let crs = Crs::from_epsg(epsg).ok_or_else(|| format!("Nieobsługiwany układ współrzędnych EPSG:{epsg}"))?;
        if w == 0 || h == 0 || data.len() != w * h || dx <= 0.0 || dy <= 0.0 {
            return Err("Uszkodzony raster (wymiary)".into());
        }
        Ok(Self { w, h, data, epsg, crs, x0, y0, dx, dy, nodata })
    }

    #[inline]
    pub fn get(&self, i: isize, j: isize) -> Option<f32> {
        if i < 0 || j < 0 || i as usize >= self.w || j as usize >= self.h {
            return None;
        }
        let v = self.data[j as usize * self.w + i as usize];
        valid(v, self.nodata).then_some(v)
    }

    /// Interpolacja dwuliniowa; przy brakach bierze dostępnych sąsiadów.
    pub fn sample_xy(&self, x: f64, y: f64) -> Option<f32> {
        let fx = (x - self.x0) / self.dx - 0.5;
        let fy = (self.y0 - y) / self.dy - 0.5;
        if fx < -0.5 || fy < -0.5 || fx > self.w as f64 - 0.5 || fy > self.h as f64 - 0.5 {
            return None;
        }
        let i = fx.floor() as isize;
        let j = fy.floor() as isize;
        let tx = (fx - i as f64) as f32;
        let ty = (fy - j as f64) as f32;
        let q = [self.get(i, j), self.get(i + 1, j), self.get(i, j + 1), self.get(i + 1, j + 1)];
        let wts = [(1.0 - tx) * (1.0 - ty), tx * (1.0 - ty), (1.0 - tx) * ty, tx * ty];
        let (mut s, mut ws) = (0.0f32, 0.0f32);
        for k in 0..4 {
            if let Some(v) = q[k] {
                s += v * wts[k];
                ws += wts[k];
            }
        }
        (ws > 0.25).then(|| s / ws)
    }

    pub fn sample(&self, p: LatLon) -> Option<f32> {
        let (x, y) = self.crs.forward(p);
        self.sample_xy(x, y)
    }

    /// Zasięg w stopniach (z rogów i środków krawędzi).
    pub fn footprint(&self) -> BBox {
        let x1 = self.x0 + self.w as f64 * self.dx;
        let y1 = self.y0 - self.h as f64 * self.dy;
        let mut b = BBox::empty();
        for (x, y) in [(self.x0, self.y0), (x1, self.y0), (x1, y1), (self.x0, y1), ((self.x0 + x1) / 2.0, self.y0), ((self.x0 + x1) / 2.0, y1), (self.x0, (self.y0 + y1) / 2.0), (x1, (self.y0 + y1) / 2.0)] {
            b.add(self.crs.inverse(x, y));
        }
        b
    }

    /// Rozdzielczość w metrach.
    pub fn resolution_m(&self) -> f64 {
        if self.crs.is_geographic() {
            let c = self.footprint().center();
            (self.dy * 111_320.0).max(self.dx * 111_320.0 * c.lat.to_radians().cos())
        } else {
            self.dx.max(self.dy)
        }
    }

    pub fn stats(&self) -> (f32, f32, usize) {
        let (mut lo, mut hi, mut n) = (f32::MAX, f32::MIN, 0usize);
        for v in &self.data {
            if valid(*v, self.nodata) {
                lo = lo.min(*v);
                hi = hi.max(*v);
                n += 1;
            }
        }
        (lo, hi, n)
    }

    // ── zapis do pamięci podręcznej (szybki format binarny) ──

    pub fn save_bin(&self, path: &Path) -> Result<(), String> {
        let mut out = Vec::with_capacity(64 + self.data.len() * 4);
        out.extend_from_slice(b"FSOSDEM1");
        for v in [self.w as u32, self.h as u32, self.epsg] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        for v in [self.x0, self.y0, self.dx, self.dy] {
            out.extend_from_slice(&v.to_le_bytes());
        }
        out.extend_from_slice(&self.nodata.unwrap_or(f32::NAN).to_le_bytes());
        for v in &self.data {
            out.extend_from_slice(&v.to_le_bytes());
        }
        crate::store::write_atomic(path, &out)
    }

    pub fn load_bin(path: &Path) -> Result<Self, String> {
        let b = std::fs::read(path).map_err(|e| e.to_string())?;
        if b.len() < 56 || &b[..8] != b"FSOSDEM1" {
            return Err("Uszkodzony plik pamięci podręcznej".into());
        }
        let u = |o: usize| u32::from_le_bytes(b[o..o + 4].try_into().unwrap());
        let f = |o: usize| f64::from_le_bytes(b[o..o + 8].try_into().unwrap());
        let (w, h, epsg) = (u(8) as usize, u(12) as usize, u(16));
        let (x0, y0, dx, dy) = (f(20), f(28), f(36), f(44));
        let nd = f32::from_le_bytes(b[52..56].try_into().unwrap());
        if b.len() != 56 + w * h * 4 {
            return Err("Uszkodzony plik pamięci podręcznej".into());
        }
        let data = b[56..].chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect();
        Raster::new(w, h, data, epsg, x0, y0, dx, dy, (!nd.is_nan()).then_some(nd))
    }
}

// ───────────────────────────── Czytniki plików ─────────────────────────────

/// Odczyt dowolnego obsługiwanego pliku (także z archiwum ZIP).
pub fn read_file(path: &Path, epsg: u32, swap_xy: bool) -> Result<Raster, String> {
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if ext == "zip" {
        let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut z = zip::ZipArchive::new(f).map_err(|e| format!("ZIP: {e}"))?;
        let prj = (0..z.len()).find_map(|i| {
            let mut e = z.by_index(i).ok()?;
            if e.name().to_lowercase().ends_with(".prj") {
                let mut s = String::new();
                e.read_to_string(&mut s).ok()?;
                Some(s)
            } else {
                None
            }
        });
        for i in 0..z.len() {
            let mut e = z.by_index(i).map_err(|e| e.to_string())?;
            let inner = e.name().to_lowercase();
            let iext = inner.rsplit('.').next().unwrap_or("").to_string();
            if ["tif", "tiff", "asc", "xyz", "txt", "hgt"].contains(&iext.as_str()) {
                let mut buf = vec![];
                e.read_to_end(&mut buf).map_err(|e| e.to_string())?;
                let epsg = if epsg == 0 { prj.as_deref().and_then(epsg_from_wkt).unwrap_or(0) } else { epsg };
                return read_bytes(&buf, &iext, e.name(), epsg, swap_xy);
            }
        }
        return Err("Archiwum nie zawiera pliku z wysokościami (TIF, ASC, XYZ, HGT)".into());
    }
    let buf = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let epsg = if epsg == 0 {
        let prj = path.with_extension("prj");
        std::fs::read_to_string(prj).ok().as_deref().and_then(epsg_from_wkt).unwrap_or(0)
    } else {
        epsg
    };
    read_bytes(&buf, &ext, &name, epsg, swap_xy)
}

pub fn read_bytes(buf: &[u8], ext: &str, name: &str, epsg: u32, swap_xy: bool) -> Result<Raster, String> {
    match ext {
        "tif" | "tiff" => read_geotiff(buf, epsg, None),
        "asc" => read_asc(buf, epsg),
        "hgt" => read_hgt(buf, name),
        "xyz" | "txt" | "csv" => read_xyz(buf, epsg, swap_xy),
        _ => {
            if buf.starts_with(b"II*\0") || buf.starts_with(b"MM\0*") {
                read_geotiff(buf, epsg, None)
            } else {
                Err(format!("Nieobsługiwany format pliku: .{ext}"))
            }
        }
    }
}

/// Rozpoznanie układu z opisu WKT (plik .prj) po typowych nazwach.
pub fn epsg_from_wkt(wkt: &str) -> Option<u32> {
    let w = wkt.to_lowercase().replace([' ', '_', '-'], "");
    if let Some(i) = w.rfind("authority[\"epsg\",\"") {
        let rest = &w[i + 18..];
        if let Some(end) = rest.find('"') {
            if let Ok(code) = rest[..end].parse::<u32>() {
                if Crs::from_epsg(code).is_some() {
                    return Some(code);
                }
            }
        }
    }
    let has = |s: &str| w.contains(s);
    if has("cs92") || has("1992") || has("puwg92") {
        return Some(2180);
    }
    for (z, code) in [(5u32, 2176u32), (6, 2177), (7, 2178), (8, 2179)] {
        if has(&format!("cs2000zone{z}")) || has(&format!("2000/zone{z}")) || has(&format!("2000zone{z}")) {
            return Some(code);
        }
    }
    if has("lv95") || has("ch1903+") {
        return Some(2056);
    }
    if has("lv03") || has("ch1903") {
        return Some(21781);
    }
    if has("amersfoort") || has("rdnew") {
        return Some(28992);
    }
    if has("britishnationalgrid") || has("osgb") {
        return Some(27700);
    }
    for zone in 28..=38u32 {
        if has(&format!("utmzone{zone}n")) {
            return Some(if has("etrs") { 25800 + zone } else { 32600 + zone });
        }
    }
    if has("geogcs") && !has("projcs") {
        return Some(4326);
    }
    None
}

/// Zgadywanie układu po zakresie współrzędnych (gdy brak .prj).
pub fn guess_epsg(x: f64, y: f64) -> Option<u32> {
    if (-180.0..=180.0).contains(&x) && (-90.0..=90.0).contains(&y) {
        return Some(4326);
    }
    if (140_000.0..=900_000.0).contains(&x) && (90_000.0..=800_000.0).contains(&y) {
        return Some(2180);
    }
    if (5_200_000.0..=8_800_000.0).contains(&x) && (5_300_000.0..=6_200_000.0).contains(&y) {
        let zone = (x / 1_000_000.0).floor() as u32;
        if (5..=8).contains(&zone) {
            return Some(2176 + zone - 5);
        }
    }
    if (2_480_000.0..=2_850_000.0).contains(&x) && (1_070_000.0..=1_310_000.0).contains(&y) {
        return Some(2056);
    }
    None
}

fn tag_f64(dec: &mut tiff::decoder::Decoder<std::io::Cursor<&[u8]>>, code: u16) -> Option<Vec<f64>> {
    dec.get_tag_f64_vec(tiff::tags::Tag::from_u16_exhaustive(code)).ok()
}

/// GeoTIFF: georeferencja z ModelTiepoint + ModelPixelScale albo ModelTransformation,
/// układ z GeoKeyDirectory (ProjectedCSType / GeographicType), brak danych z GDAL_NODATA.
/// `fallback` – (x0, y0, dx, dy), gdy plik nie ma georeferencji (np. odpowiedź WCS).
pub fn read_geotiff(buf: &[u8], epsg_override: u32, fallback: Option<(f64, f64, f64, f64)>) -> Result<Raster, String> {
    use tiff::decoder::{Decoder, DecodingResult, Limits};
    let mut dec = Decoder::new(std::io::Cursor::new(buf)).map_err(|e| format!("TIFF: {e}"))?.with_limits(Limits::unlimited());
    let (w, h) = dec.dimensions().map_err(|e| format!("TIFF: {e}"))?;
    let (w, h) = (w as usize, h as usize);

    let mut epsg = epsg_override;
    let mut point = false;
    if let Ok(keys) = dec.get_tag_u16_vec(tiff::tags::Tag::from_u16_exhaustive(34735)) {
        if keys.len() >= 4 {
            let n = keys[3] as usize;
            for k in 0..n {
                let base = 4 + k * 4;
                if base + 3 >= keys.len() {
                    break;
                }
                let (id, loc, val) = (keys[base], keys[base + 1], keys[base + 3]);
                if loc != 0 {
                    continue;
                }
                match id {
                    3072 if epsg_override == 0 && val != 32767 => epsg = val as u32,
                    2048 if epsg_override == 0 && epsg == 0 && val != 32767 => epsg = 4326,
                    1025 => point = val == 2,
                    _ => {}
                }
            }
        }
    }
    let nodata = dec
        .get_tag_ascii_string(tiff::tags::Tag::from_u16_exhaustive(42113))
        .ok()
        .and_then(|s| s.trim_matches(char::from(0)).trim().parse::<f32>().ok());

    let scale = tag_f64(&mut dec, 33550);
    let tie = tag_f64(&mut dec, 33922);
    let trans = tag_f64(&mut dec, 34264);
    let (mut x0, mut y0, dx, dy) = if let (Some(s), Some(t)) = (&scale, &tie) {
        if s.len() < 2 || t.len() < 6 {
            return Err("GeoTIFF: niepełna georeferencja".into());
        }
        (t[3] - t[0] * s[0], t[4] + t[1] * s[1], s[0], s[1])
    } else if let Some(m) = &trans {
        if m.len() < 16 || m[1].abs() > 1e-12 || m[4].abs() > 1e-12 {
            return Err("GeoTIFF: obrócona siatka nie jest obsługiwana".into());
        }
        (m[3], m[7], m[0], -m[5])
    } else if let Some(fb) = fallback {
        fb
    } else {
        return Err("Plik TIFF nie ma georeferencji (to nie jest GeoTIFF)".into());
    };
    if point {
        x0 -= dx / 2.0;
        y0 += dy / 2.0;
    }
    if epsg == 0 {
        epsg = guess_epsg(x0, y0).ok_or("Nie rozpoznano układu współrzędnych pliku – wybierz EPSG ręcznie")?;
    }
    let data: Vec<f32> = match dec.read_image().map_err(|e| format!("TIFF: {e}"))? {
        DecodingResult::F32(v) => v,
        DecodingResult::F64(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::I16(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::U16(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::I32(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::U32(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::U8(v) => v.into_iter().map(|x| x as f32).collect(),
        DecodingResult::I8(v) => v.into_iter().map(|x| x as f32).collect(),
        _ => return Err("TIFF: nieobsługiwany typ danych".into()),
    };
    // Wielokanałowe pliki: bierzemy pierwszy kanał.
    let data = if data.len() == w * h {
        data
    } else if data.len() % (w * h) == 0 {
        let ch = data.len() / (w * h);
        data.into_iter().step_by(ch).collect()
    } else {
        return Err("TIFF: niezgodny rozmiar danych".into());
    };
    Raster::new(w, h, data, epsg, x0, y0, dx.abs(), dy.abs(), nodata)
}

pub fn read_asc(buf: &[u8], epsg: u32) -> Result<Raster, String> {
    let text = String::from_utf8_lossy(buf);
    let mut head: HashMap<String, f64> = HashMap::new();
    let mut lines = text.lines();
    let mut first_data = None;
    for line in lines.by_ref() {
        let mut it = line.split_whitespace();
        let Some(k) = it.next() else { continue };
        let kl = k.to_lowercase();
        if kl.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false) {
            let v: f64 = it.next().and_then(|v| v.parse().ok()).ok_or_else(|| format!("ASC: zły nagłówek {k}"))?;
            head.insert(kl, v);
        } else {
            first_data = Some(line.to_string());
            break;
        }
    }
    let g = |k: &str| head.get(k).copied();
    let w = g("ncols").ok_or("ASC: brak ncols")? as usize;
    let h = g("nrows").ok_or("ASC: brak nrows")? as usize;
    let cs = g("cellsize").or(g("dx")).ok_or("ASC: brak cellsize")?;
    let csy = g("dy").unwrap_or(cs);
    let x0 = g("xllcorner").or(g("xllcenter").map(|v| v - cs / 2.0)).ok_or("ASC: brak xllcorner")?;
    let yll = g("yllcorner").or(g("yllcenter").map(|v| v - csy / 2.0)).ok_or("ASC: brak yllcorner")?;
    let nodata = g("nodata_value").map(|v| v as f32);
    let mut data = Vec::with_capacity(w * h);
    for line in first_data.iter().map(String::as_str).chain(lines) {
        for t in line.split_whitespace() {
            data.push(t.parse::<f32>().unwrap_or(f32::NAN));
        }
    }
    data.truncate(w * h);
    if data.len() < w * h {
        return Err("ASC: za mało danych".into());
    }
    let y0 = yll + h as f64 * csy;
    let epsg = if epsg == 0 { guess_epsg(x0, yll).ok_or("Nie rozpoznano układu współrzędnych pliku ASC – wybierz EPSG")? } else { epsg };
    Raster::new(w, h, data, epsg, x0, y0, cs, csy, nodata)
}

/// Chmura punktów XYZ na regularnej siatce (np. NMT GUGiK w formacie tekstowym).
pub fn read_xyz(buf: &[u8], epsg: u32, swap_xy: bool) -> Result<Raster, String> {
    let text = String::from_utf8_lossy(buf);
    let mut pts: Vec<(f64, f64, f32)> = Vec::with_capacity(buf.len() / 24);
    for line in text.lines() {
        let mut it = line.split(|c: char| c.is_whitespace() || c == ',' || c == ';').filter(|s| !s.is_empty());
        let (Some(a), Some(b), Some(c)) = (it.next(), it.next(), it.next()) else { continue };
        let (Ok(a), Ok(b), Ok(c)) = (a.parse::<f64>(), b.parse::<f64>(), c.parse::<f32>()) else { continue };
        pts.push(if swap_xy { (b, a, c) } else { (a, b, c) });
    }
    if pts.len() < 4 {
        return Err("XYZ: za mało punktów".into());
    }
    let step = |vals: &mut Vec<f64>| -> f64 {
        vals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        vals.dedup_by(|a, b| (*a - *b).abs() < 1e-6);
        let mut best = f64::MAX;
        for w in vals.windows(2) {
            let d = w[1] - w[0];
            if d > 1e-6 && d < best {
                best = d;
            }
        }
        best
    };
    let mut xs: Vec<f64> = pts.iter().map(|p| p.0).collect();
    let mut ys: Vec<f64> = pts.iter().map(|p| p.1).collect();
    let sx = step(&mut xs);
    let sy = step(&mut ys);
    if !sx.is_finite() || sx == f64::MAX || !sy.is_finite() || sy == f64::MAX {
        return Err("XYZ: punkty nie tworzą siatki".into());
    }
    let (minx, maxx) = (xs[0], *xs.last().unwrap());
    let (miny, maxy) = (ys[0], *ys.last().unwrap());
    let w = ((maxx - minx) / sx).round() as usize + 1;
    let h = ((maxy - miny) / sy).round() as usize + 1;
    if w * h > 400_000_000 {
        return Err("XYZ: siatka za duża".into());
    }
    let mut data = vec![f32::NAN; w * h];
    for (x, y, z) in pts {
        let i = ((x - minx) / sx).round() as usize;
        let j = ((maxy - y) / sy).round() as usize;
        if i < w && j < h {
            data[j * w + i] = z;
        }
    }
    let epsg = if epsg == 0 { guess_epsg(minx, miny).ok_or("Nie rozpoznano układu współrzędnych pliku XYZ – wybierz EPSG")? } else { epsg };
    Raster::new(w, h, data, epsg, minx - sx / 2.0, maxy + sy / 2.0, sx, sy, None)
}

/// SRTM .hgt: kwadrat 1201² albo 3601², big-endian i16, nazwa = róg SW (np. N50E019.hgt).
pub fn read_hgt(buf: &[u8], name: &str) -> Result<Raster, String> {
    let n = ((buf.len() / 2) as f64).sqrt() as usize;
    if n * n * 2 != buf.len() || n < 2 {
        return Err("HGT: nieznany rozmiar pliku".into());
    }
    let up = name.to_uppercase();
    let lat_sign = if up.starts_with('S') { -1.0 } else { 1.0 };
    let lat: f64 = up.get(1..3).and_then(|s| s.parse().ok()).ok_or("HGT: nazwa pliku musi mieć postać N50E019.hgt")?;
    let lon_sign = if up.get(3..4) == Some("W") { -1.0 } else { 1.0 };
    let lon: f64 = up.get(4..7).and_then(|s| s.parse().ok()).ok_or("HGT: nazwa pliku musi mieć postać N50E019.hgt")?;
    let d = 1.0 / (n - 1) as f64;
    let data = buf.chunks_exact(2).map(|c| i16::from_be_bytes([c[0], c[1]])).map(|v| if v == -32768 { f32::NAN } else { v as f32 }).collect();
    Raster::new(n, n, data, 4326, lon_sign * lon - d / 2.0, lat_sign * lat + 1.0 + d / 2.0, d, d, None)
}

// ───────────────────────────── Pamięć podręczna fragmentów ─────────────────────────────

type Slot = Arc<OnceLock<Option<Arc<Raster>>>>;

/// Ograniczona pamięć wczytanych fragmentów (LRU), bezpieczna dla wielu wątków:
/// fragment ładuje się raz, inne wątki czekają na wynik.
pub struct ChunkCache {
    map: Mutex<(HashMap<String, Slot>, VecDeque<String>)>,
    cap: usize,
}

impl ChunkCache {
    pub fn new(cap: usize) -> Self {
        Self { map: Mutex::new((HashMap::new(), VecDeque::new())), cap }
    }

    pub fn get(&self, key: &str, load: impl FnOnce() -> Option<Raster>) -> Option<Arc<Raster>> {
        let slot = {
            let mut g = self.map.lock().unwrap_or_else(|e| e.into_inner());
            if let Some(s) = g.0.get(key) {
                s.clone()
            } else {
                let s: Slot = Arc::new(OnceLock::new());
                g.0.insert(key.to_string(), s.clone());
                g.1.push_back(key.to_string());
                while g.1.len() > self.cap {
                    if let Some(old) = g.1.pop_front() {
                        g.0.remove(&old);
                    }
                }
                s
            }
        };
        slot.get_or_init(|| load().map(Arc::new)).clone()
    }
}

// ───────────────────────────── Źródła ─────────────────────────────

pub trait Source: Send + Sync {
    fn id(&self) -> String;
    fn label(&self) -> String;
    /// Fragmenty potrzebne dla obszaru (klucze własne źródła).
    fn chunks(&self, b: &BBox) -> Vec<String>;
    /// Czy fragment jest już na dysku.
    fn cached(&self, key: &str) -> bool;
    /// Pobranie fragmentu na dysk; zwraca liczbę bajtów (0 – brak danych w tym miejscu).
    fn fetch(&self, key: &str) -> Result<u64, String>;
    fn sample(&self, p: LatLon) -> Option<f32>;
    /// Czy źródło w ogóle obejmuje ten obszar (szybki test zasięgu).
    fn covers(&self, b: &BBox) -> bool;
    /// Rzeczywista rozdzielczość danych w metrach (przybliżona).
    fn resolution(&self) -> f64;
}

/// Plik użytkownika wczytany w całości.
pub struct FileSource {
    pub name: String,
    pub raster: Raster,
    pub footprint: BBox,
    pub offset: f32,
}

impl Source for FileSource {
    fn id(&self) -> String {
        format!("file:{}", self.name)
    }
    fn label(&self) -> String {
        self.name.clone()
    }
    fn chunks(&self, _b: &BBox) -> Vec<String> {
        vec![]
    }
    fn cached(&self, _key: &str) -> bool {
        true
    }
    fn fetch(&self, _key: &str) -> Result<u64, String> {
        Ok(0)
    }
    fn sample(&self, p: LatLon) -> Option<f32> {
        if !self.footprint.contains(p) {
            return None;
        }
        self.raster.sample(p).map(|v| v + self.offset)
    }
    fn covers(&self, b: &BBox) -> bool {
        self.footprint.intersects(b)
    }
    fn resolution(&self) -> f64 {
        self.raster.resolution_m()
    }
}

/// Ścieżka pliku w pamięci podręcznej z bezpieczną nazwą.
fn cache_file(dir: &Path, key: &str, ext: &str) -> PathBuf {
    let safe: String = key.chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '.' { c } else { '_' }).collect();
    dir.join(format!("{safe}.{ext}"))
}

fn mark_missing(path: &Path) -> Result<u64, String> {
    crate::store::write_atomic(&path.with_extension("missing"), b"")?;
    Ok(0)
}

// ── AWS Terrain Tiles (Terrarium) ──

pub struct Terrarium {
    dir: PathBuf,
    z: u32,
    cache: ChunkCache,
}

impl Terrarium {
    pub fn new(cache_root: &Path, resolution: f64, lat: f64) -> Self {
        // Rozdzielczość piksela kafla na danej szerokości: 156543 · cos(φ) / 2^z.
        let want = (156_543.03 * lat.to_radians().cos() / resolution.max(1.0)).log2().ceil() as i64;
        let z = want.clamp(8, 15) as u32;
        Self { dir: cache_root.join("terrarium").join(z.to_string()), z, cache: ChunkCache::new(96) }
    }

    fn tile_xy(&self, p: LatLon) -> (f64, f64) {
        let n = 2f64.powi(self.z as i32) * 256.0;
        let x = (p.lon + 180.0) / 360.0 * n;
        let lat = p.lat.clamp(-85.05, 85.05).to_radians();
        let y = (1.0 - (lat.tan() + 1.0 / lat.cos()).ln() / std::f64::consts::PI) / 2.0 * n;
        (x, y)
    }

    fn tile(&self, tx: i64, ty: i64) -> Option<Arc<Raster>> {
        let key = format!("{}-{}", tx, ty);
        self.cache.get(&key, || {
            let path = cache_file(&self.dir, &key, "png");
            if !path.exists() && !path.with_extension("missing").exists() {
                let _ = self.fetch(&key);
            }
            let img = image::open(&path).ok()?.to_rgb8();
            let data: Vec<f32> = img.pixels().map(|p| p[0] as f32 * 256.0 + p[1] as f32 + p[2] as f32 / 256.0 - 32768.0).collect();
            // Układ „pikselowy” – georeferencja nieużywana (próbkujemy po indeksach).
            Raster::new(256, 256, data, 4326, 0.0, 0.0, 1.0, 1.0, None).ok()
        })
    }

    fn px(&self, gx: i64, gy: i64) -> Option<f32> {
        let max = 2i64.pow(self.z) * 256;
        if gy < 0 || gy >= max {
            return None;
        }
        let gx = gx.rem_euclid(max);
        let t = self.tile(gx.div_euclid(256), gy.div_euclid(256))?;
        t.get(gx.rem_euclid(256) as isize, gy.rem_euclid(256) as isize)
    }
}

impl Source for Terrarium {
    fn id(&self) -> String {
        "terrarium".into()
    }
    fn label(&self) -> String {
        format!("AWS Terrain Tiles (z{})", self.z)
    }
    fn chunks(&self, b: &BBox) -> Vec<String> {
        let (x0, y0) = self.tile_xy(LatLon::new(b.north, b.west));
        let (x1, y1) = self.tile_xy(LatLon::new(b.south, b.east));
        let mut out = vec![];
        for ty in (y0 / 256.0).floor() as i64..=(y1 / 256.0).floor() as i64 {
            for tx in (x0 / 256.0).floor() as i64..=(x1 / 256.0).floor() as i64 {
                out.push(format!("{tx}-{ty}"));
            }
        }
        out
    }
    fn cached(&self, key: &str) -> bool {
        let p = cache_file(&self.dir, key, "png");
        p.exists() || p.with_extension("missing").exists()
    }
    fn fetch(&self, key: &str) -> Result<u64, String> {
        let path = cache_file(&self.dir, key, "png");
        if path.exists() {
            return Ok(0);
        }
        let (tx, ty) = key.split_once('-').ok_or("zły klucz kafla")?;
        let url = format!("https://s3.amazonaws.com/elevation-tiles-prod/terrarium/{}/{}/{}.png", self.z, tx, ty);
        match net::get(&url)? {
            net::Fetched::Ok(b) => {
                let n = b.len() as u64;
                crate::store::write_atomic(&path, &b)?;
                Ok(n)
            }
            net::Fetched::Missing => mark_missing(&path),
        }
    }
    fn sample(&self, p: LatLon) -> Option<f32> {
        let (x, y) = self.tile_xy(p);
        let (fx, fy) = (x - 0.5, y - 0.5);
        let (i, j) = (fx.floor() as i64, fy.floor() as i64);
        let (tx, ty) = ((fx - i as f64) as f32, (fy - j as f64) as f32);
        let q = [self.px(i, j)?, self.px(i + 1, j)?, self.px(i, j + 1)?, self.px(i + 1, j + 1)?];
        Some(q[0] * (1.0 - tx) * (1.0 - ty) + q[1] * tx * (1.0 - ty) + q[2] * (1.0 - tx) * ty + q[3] * tx * ty)
    }
    fn covers(&self, _b: &BBox) -> bool {
        true
    }
    fn resolution(&self) -> f64 {
        156_543.03 / 2f64.powi(self.z as i32) * 0.65
    }
}

// ── Copernicus GLO-30 / GLO-90 ──

pub struct Copernicus {
    dir: PathBuf,
    arc: u32,
    cache: ChunkCache,
}

impl Copernicus {
    pub fn new(cache_root: &Path, glo90: bool) -> Self {
        let arc = if glo90 { 30 } else { 10 };
        Self { dir: cache_root.join(if glo90 { "copernicus90" } else { "copernicus30" }), arc, cache: ChunkCache::new(if glo90 { 16 } else { 6 }) }
    }

    fn key_of(lat: f64, lon: f64) -> String {
        let la = lat.floor() as i32;
        let lo = lon.floor() as i32;
        format!("{}{:02}_00_{}{:03}_00", if la < 0 { 'S' } else { 'N' }, la.abs(), if lo < 0 { 'W' } else { 'E' }, lo.abs())
    }

    fn tile(&self, key: &str) -> Option<Arc<Raster>> {
        self.cache.get(key, || {
            let path = cache_file(&self.dir, key, "tif");
            if !path.exists() && !path.with_extension("missing").exists() {
                let _ = self.fetch(key);
            }
            let bin = path.with_extension("bin");
            if let Ok(r) = Raster::load_bin(&bin) {
                return Some(r);
            }
            let b = std::fs::read(&path).ok()?;
            let r = read_geotiff(&b, 4326, None).ok()?;
            let _ = r.save_bin(&bin);
            let _ = std::fs::remove_file(&path);
            let _ = std::fs::write(&path, b"converted");
            Some(r)
        })
    }
}

impl Source for Copernicus {
    fn id(&self) -> String {
        if self.arc == 10 { "copernicus30".into() } else { "copernicus90".into() }
    }
    fn label(&self) -> String {
        if self.arc == 10 { "Copernicus GLO-30".into() } else { "Copernicus GLO-90".into() }
    }
    fn chunks(&self, b: &BBox) -> Vec<String> {
        let mut out = vec![];
        for la in b.south.floor() as i32..=b.north.floor() as i32 {
            for lo in b.west.floor() as i32..=b.east.floor() as i32 {
                out.push(Self::key_of(la as f64, lo as f64));
            }
        }
        out
    }
    fn cached(&self, key: &str) -> bool {
        let p = cache_file(&self.dir, key, "tif");
        p.exists() || p.with_extension("missing").exists()
    }
    fn fetch(&self, key: &str) -> Result<u64, String> {
        let path = cache_file(&self.dir, key, "tif");
        if path.exists() {
            return Ok(0);
        }
        let name = format!("Copernicus_DSM_COG_{}_{}_DEM", self.arc, key);
        let bucket = if self.arc == 10 { "copernicus-dem-30m" } else { "copernicus-dem-90m" };
        let url = format!("https://{bucket}.s3.amazonaws.com/{name}/{name}.tif");
        match net::get(&url)? {
            net::Fetched::Ok(b) => {
                let n = b.len() as u64;
                crate::store::write_atomic(&path, &b)?;
                Ok(n)
            }
            net::Fetched::Missing => mark_missing(&path),
        }
    }
    fn sample(&self, p: LatLon) -> Option<f32> {
        let key = Self::key_of(p.lat, p.lon);
        if cache_file(&self.dir, &key, "missing").exists() {
            // Brak kafla = morze.
            return Some(0.0);
        }
        self.tile(&key)?.sample(p)
    }
    fn covers(&self, _b: &BBox) -> bool {
        true
    }
    fn resolution(&self) -> f64 {
        self.arc as f64 * 3.0
    }
}

// ── WCS 1.0 (NMT GUGiK i usługi własne) ──

pub struct Wcs {
    id: String,
    label: String,
    url: String,
    coverage: String,
    epsg: u32,
    crs: Crs,
    res: f64,
    px: usize,
    dir: PathBuf,
    envelope: Option<BBox>,
    cache: ChunkCache,
}

pub const GUGIK_URL: &str = "https://mapy.geoportal.gov.pl/wss/service/PZGIK/NMT/GRID1/WCS/DigitalTerrainModelFormatTIFF";
pub const GUGIK_COVERAGE: &str = "DTM_PL-KRON86-NH_TIFF";
pub const POLAND: BBox = BBox { south: 48.64, west: 13.70, north: 55.04, east: 24.87 };

impl Wcs {
    pub fn gugik(cache_root: &Path, resolution: f64) -> Self {
        Self::new("gugik", "NMT GUGiK", GUGIK_URL, GUGIK_COVERAGE, 2180, cache_root, resolution, Some(POLAND)).expect("EPSG:2180")
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(id: &str, label: &str, url: &str, coverage: &str, epsg: u32, cache_root: &Path, resolution: f64, envelope: Option<BBox>) -> Result<Self, String> {
        let crs = Crs::from_epsg(epsg).ok_or_else(|| format!("Nieobsługiwany układ EPSG:{epsg}"))?;
        // Dane pobieramy gęściej niż docelowa siatka (bez schodków), ale nie gęściej niż 1 m.
        let res = [1.0, 2.0, 5.0, 10.0, 25.0].into_iter().find(|r| *r * 2.0 >= resolution).unwrap_or(25.0);
        let res = if crs.is_geographic() { res / 111_320.0 } else { res };
        let tag: String = format!("{url}|{coverage}|{epsg}").bytes().fold(0u64, |h, b| h.wrapping_mul(131).wrapping_add(b as u64)).to_string();
        Ok(Self {
            id: id.into(),
            label: label.into(),
            url: url.trim_end_matches('?').into(),
            coverage: coverage.into(),
            epsg,
            crs,
            res,
            px: 1000,
            dir: cache_root.join(format!("wcs-{}-{}", id, &tag[..tag.len().min(10)])).join(format!("{}", (res * 1000.0).round() as u64)),
            envelope,
            cache: ChunkCache::new(48),
        })
    }

    fn size(&self) -> f64 {
        self.res * self.px as f64
    }

    fn chunk(&self, key: &str) -> Option<Arc<Raster>> {
        self.cache.get(key, || {
            let path = cache_file(&self.dir, key, "bin");
            if !path.exists() && !path.with_extension("missing").exists() {
                let _ = self.fetch(key);
            }
            Raster::load_bin(&path).ok()
        })
    }
}

impl Source for Wcs {
    fn id(&self) -> String {
        self.id.clone()
    }
    fn label(&self) -> String {
        self.label.clone()
    }
    fn chunks(&self, b: &BBox) -> Vec<String> {
        if !self.covers(b) {
            return vec![];
        }
        let mut xs = vec![];
        let mut ys = vec![];
        for lat in [b.south, b.north, (b.south + b.north) / 2.0] {
            for lon in [b.west, b.east, (b.west + b.east) / 2.0] {
                let (x, y) = self.crs.forward(LatLon::new(lat, lon));
                xs.push(x);
                ys.push(y);
            }
        }
        let s = self.size();
        let (ix0, ix1) = ((xs.iter().cloned().fold(f64::MAX, f64::min) / s).floor() as i64, (xs.iter().cloned().fold(f64::MIN, f64::max) / s).floor() as i64);
        let (iy0, iy1) = ((ys.iter().cloned().fold(f64::MAX, f64::min) / s).floor() as i64, (ys.iter().cloned().fold(f64::MIN, f64::max) / s).floor() as i64);
        let mut out = vec![];
        for iy in iy0..=iy1 {
            for ix in ix0..=ix1 {
                out.push(format!("{ix}_{iy}"));
            }
        }
        out
    }
    fn cached(&self, key: &str) -> bool {
        let p = cache_file(&self.dir, key, "bin");
        p.exists() || p.with_extension("missing").exists()
    }
    fn fetch(&self, key: &str) -> Result<u64, String> {
        let path = cache_file(&self.dir, key, "bin");
        if path.exists() {
            return Ok(0);
        }
        let (ix, iy) = key.split_once('_').and_then(|(a, b)| Some((a.parse::<i64>().ok()?, b.parse::<i64>().ok()?))).ok_or("zły klucz")?;
        let s = self.size();
        let (x0, y0) = (ix as f64 * s, iy as f64 * s);
        let (x1, y1) = (x0 + s, y0 + s);
        let url = format!(
            "{}?SERVICE=WCS&VERSION=1.0.0&REQUEST=GetCoverage&COVERAGE={}&CRS=EPSG:{}&BBOX={},{},{},{}&WIDTH={}&HEIGHT={}&FORMAT=image/tiff",
            self.url, self.coverage, self.epsg, x0, y0, x1, y1, self.px, self.px
        );
        match net::get(&url)? {
            net::Fetched::Ok(b) => {
                if !(b.starts_with(b"II*\0") || b.starts_with(b"MM\0*")) {
                    // Odpowiedź XML (np. poza zasięgiem usługi) – zapamiętujemy brak danych.
                    return mark_missing(&path);
                }
                let n = b.len() as u64;
                let mut r = read_geotiff(&b, self.epsg, Some((x0, y1, self.res, self.res)))?;
                // Niektóre serwery zwracają georeferencję środków pikseli albo bez niej – ufamy zapytaniu.
                if (r.x0 - x0).abs() > s * 0.01 || (r.y0 - y1).abs() > s * 0.01 || r.w != self.px {
                    r.x0 = x0;
                    r.y0 = y1;
                    r.dx = s / r.w as f64;
                    r.dy = s / r.h as f64;
                }
                let (_, _, valid_n) = r.stats();
                if valid_n == 0 {
                    return mark_missing(&path);
                }
                r.save_bin(&path)?;
                Ok(n)
            }
            net::Fetched::Missing => mark_missing(&path),
        }
    }
    fn sample(&self, p: LatLon) -> Option<f32> {
        if let Some(e) = &self.envelope {
            if !e.contains(p) {
                return None;
            }
        }
        let (x, y) = self.crs.forward(p);
        let s = self.size();
        let key = format!("{}_{}", (x / s).floor() as i64, (y / s).floor() as i64);
        let c = self.chunk(&key)?;
        c.sample_xy(x, y).or_else(|| {
            // Na samej krawędzi fragmentu interpolacja może sięgać sąsiada – próbujemy bez niej.
            let i = ((x - c.x0) / c.dx).floor() as isize;
            let j = ((c.y0 - y) / c.dy).floor() as isize;
            c.get(i.clamp(0, c.w as isize - 1), j.clamp(0, c.h as isize - 1))
        })
    }
    fn covers(&self, b: &BBox) -> bool {
        self.envelope.map(|e| e.intersects(b)).unwrap_or(true)
    }
    fn resolution(&self) -> f64 {
        if self.crs.is_geographic() {
            self.res * 111_320.0
        } else {
            self.res
        }
    }
}

/// Zestaw źródeł w kolejności ważności.
pub struct Mosaic {
    pub sources: Vec<Box<dyn Source>>,
    pub offset: f32,
}

impl Mosaic {
    /// Wysokość i indeks źródła, które ją dało.
    pub fn sample(&self, p: LatLon) -> Option<(f32, usize)> {
        for (i, s) in self.sources.iter().enumerate() {
            if let Some(v) = s.sample(p) {
                return Some((v + self.offset, i));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raster_sampling_and_cache() {
        // 3×3 w PUWG 1992, krok 10 m
        let data = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0];
        let r = Raster::new(3, 3, data, 2180, 500_000.0, 300_030.0, 10.0, 10.0, None).unwrap();
        // środek piksela (1,1) = (500015, 300015)
        assert_eq!(r.sample_xy(500_015.0, 300_015.0), Some(5.0));
        let v = r.sample_xy(500_010.0, 300_015.0).unwrap();
        assert!((v - 4.5).abs() < 1e-5);
        assert_eq!(r.sample_xy(400_000.0, 300_015.0), None);
        let p = std::env::temp_dir().join(format!("fsos-r-{}.bin", crate::store::new_id()));
        r.save_bin(&p).unwrap();
        let q = Raster::load_bin(&p).unwrap();
        assert_eq!(q.data, r.data);
        assert_eq!(q.epsg, 2180);
        let _ = std::fs::remove_file(p);
    }

    #[test]
    fn asc_and_xyz() {
        let asc = b"ncols 3\nnrows 2\nxllcorner 500000\nyllcorner 300000\ncellsize 1\nNODATA_value -9999\n1 2 3\n4 -9999 6\n";
        let r = read_asc(asc, 2180).unwrap();
        assert_eq!((r.w, r.h), (3, 2));
        assert_eq!(r.get(0, 0), Some(1.0));
        assert_eq!(r.get(1, 1), None);
        let xyz = b"500000.5 300001.5 10\n500001.5 300001.5 11\n500000.5 300000.5 12\n500001.5 300000.5 13\n";
        let r = read_xyz(xyz, 0, false).unwrap();
        assert_eq!(r.epsg, 2180);
        assert_eq!((r.w, r.h), (2, 2));
        assert_eq!(r.get(0, 0), Some(10.0));
        assert_eq!(r.get(1, 1), Some(13.0));
        assert!((r.x0 - 500_000.0).abs() < 1e-9);
    }

    #[test]
    fn hgt_and_wkt() {
        let n = 3usize;
        let mut buf = vec![];
        for v in 0..(n * n) as i16 {
            buf.extend_from_slice(&(v * 10).to_be_bytes());
        }
        let r = read_hgt(&buf, "N50E019.hgt").unwrap();
        let v = r.sample(LatLon::new(51.0, 19.0)).unwrap();
        assert!(v.abs() < 1e-3, "{v}");
        let v = r.sample(LatLon::new(50.0, 20.0)).unwrap();
        assert!((v - 80.0).abs() < 1e-3, "{v}");
        assert_eq!(epsg_from_wkt("PROJCS[\"ETRS89 / Poland CS92\",GEOGCS[...]]"), Some(2180));
        assert_eq!(epsg_from_wkt("PROJCS[\"ETRS89_UTM_zone_33N\"]"), Some(25833));
        assert_eq!(guess_epsg(19.5, 50.1), Some(4326));
        assert_eq!(guess_epsg(7_450_000.0, 5_550_000.0), Some(2178));
    }

    /// Prawdziwe źródła sieciowe (uruchamiane ręcznie: `cargo test -- --ignored network`).
    #[test]
    #[ignore]
    fn network_sources() {
        let dir = std::env::temp_dir().join("fsos-net-test");
        // Lotnisko Kraków-Balice ~ 241 m n.p.m.
        let p = LatLon::new(50.0777, 19.7848);
        let g = Wcs::gugik(&dir, 2.0);
        let v = g.sample(p).expect("GUGiK");
        assert!((200.0..280.0).contains(&v), "GUGiK {v}");
        let t = Terrarium::new(&dir, 10.0, p.lat);
        let v2 = t.sample(p).expect("Terrarium");
        assert!((200.0..280.0).contains(&v2), "Terrarium {v2}");
        let c = Copernicus::new(&dir, true);
        let v3 = c.sample(p).expect("Copernicus");
        assert!((200.0..290.0).contains(&v3), "Copernicus {v3}");
        println!("GUGiK {v} · Terrarium {v2} · Copernicus90 {v3}");
    }

    #[test]
    fn geotiff_roundtrip() {
        use tiff::encoder::{colortype, TiffEncoder};
        let mut buf = std::io::Cursor::new(Vec::new());
        {
            let mut enc = TiffEncoder::new(&mut buf).unwrap();
            let mut img = enc.new_image::<colortype::Gray32Float>(2, 2).unwrap();
            img.encoder().write_tag(tiff::tags::Tag::from_u16_exhaustive(33550), &[1.0f64, 1.0, 0.0][..]).unwrap();
            img.encoder().write_tag(tiff::tags::Tag::from_u16_exhaustive(33922), &[0.0f64, 0.0, 0.0, 500_000.0, 300_002.0, 0.0][..]).unwrap();
            img.encoder().write_tag(tiff::tags::Tag::from_u16_exhaustive(34735), &[1u16, 1, 0, 1, 3072, 0, 1, 2180][..]).unwrap();
            img.write_data(&[1.0f32, 2.0, 3.0, 4.0]).unwrap();
        }
        let r = read_geotiff(buf.get_ref(), 0, None).unwrap();
        assert_eq!(r.epsg, 2180);
        assert_eq!(r.sample_xy(500_001.5, 300_000.5), Some(4.0));
    }
}
