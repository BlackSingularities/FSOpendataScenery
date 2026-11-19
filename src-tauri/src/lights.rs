//! Rozmieszczenie świateł z OpenStreetMap:
//!  - prawdziwe latarnie (`highway=street_lamp`) – wysokość, mocowanie, barwa z tagów,
//!    wysięgnik obrócony nad najbliższą jezdnię,
//!  - oświetlone drogi (`lit=*`, opcjonalnie zgadywane w zabudowie) – rozstaw i strona
//!    zależne od klasy drogi, bez dublowania lamp z OSM i bez stawiania słupów na jezdni,
//!  - parkingi, boiska i stadiony (maszty celujące w środek), perony, stacje paliw, pomosty,
//!  - światła przeszkodowe (maszty, kominy, wieże, turbiny wiatrowe) i latarnie morskie.

use crate::geo::{self, Area, BBox, LatLon, PointGrid, SegmentIndex};
use crate::osm::{parse_height, Element};
use crate::store::{LightCat, LightsCfg, ManualLight, ObstaclesCfg};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Parametry modelu (z nich powstaje plik glTF – takie same parametry = ten sam model).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Spec {
    /// "cobra", "post_top", "double", "flood", "wall", "none", "obstruction", "beacon", "turbine"
    pub style: String,
    pub height: f64,
    pub arm: f64,
    pub kelvin: f64,
    /// Kolor wprost (np. czerwone światło przeszkodowe) zamiast temperatury barwowej.
    pub color: Option<[f64; 3]>,
    pub intensity: f64,
    pub cone: f64,
    /// Liczba opraw (maszty, wysięgniki podwójne).
    pub heads: u8,
    /// Pochylenie opraw naświetlaczy w dół od poziomu, stopnie.
    pub tilt: f64,
    /// Błyski: (częstotliwość Hz, czas trwania s).
    pub flash: Option<(f64, f64)>,
    /// Dodatkowe poziomy świateł (wysokości względem podstawy modelu).
    pub levels: Vec<f64>,
}

impl Spec {
    /// Klucz po zaokrągleniu – ogranicza liczbę różnych modeli.
    pub fn quantized(&self) -> Spec {
        let q = |v: f64, s: f64| (v / s).round() * s;
        Spec {
            style: self.style.clone(),
            height: q(self.height, 0.5),
            arm: q(self.arm, 0.25),
            kelvin: q(self.kelvin, 100.0),
            color: self.color.map(|c| c.map(|x| q(x, 0.05))),
            intensity: q(self.intensity, if self.intensity > 50.0 { 5.0 } else { 1.0 }),
            cone: q(self.cone, 5.0),
            heads: self.heads,
            tilt: q(self.tilt, 5.0),
            flash: self.flash.map(|(f, d)| (q(f, 0.05), q(d, 0.05))),
            levels: self.levels.iter().map(|l| q(*l, 1.0)).collect(),
        }
    }

    pub fn key(&self) -> String {
        let s = self.quantized();
        format!(
            "{}|h{:.1}|a{:.2}|k{:.0}|c{:?}|i{:.0}|cone{:.0}|n{}|t{:.0}|f{:?}|l{:?}",
            s.style, s.height, s.arm, s.kelvin, s.color, s.intensity, s.cone, s.heads, s.tilt, s.flash, s.levels
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Light {
    pub lat: f64,
    pub lon: f64,
    /// Kierunek wysięgnika / przodu modelu (stopnie od północy).
    pub heading: f64,
    pub cat: String,
    /// 'o' – z OSM, 'g' – wygenerowane, 'm' – ręczne.
    pub src: char,
    pub osm: i64,
    /// Wysokość podstawy modelu nad terenem (0 – stoi na ziemi).
    pub alt: f64,
    pub spec: Spec,
}

/// Domyślne kategorie oświetlenia.
pub fn default_categories() -> BTreeMap<String, LightCat> {
    let c = |enabled: bool, height: f64, spacing: f64, kelvin: f64, intensity: f64, cone: f64, style: &str, arm: f64, offset: f64, both: bool| LightCat {
        enabled,
        height,
        spacing,
        kelvin,
        intensity,
        cone,
        style: style.into(),
        arm,
        offset,
        both_sides: both,
    };
    let mut m = BTreeMap::new();
    m.insert("mapped".into(), c(true, 8.0, 0.0, 3000.0, 40.0, 140.0, "cobra", 1.5, 0.0, false));
    m.insert("motorway".into(), c(true, 12.0, 45.0, 4000.0, 70.0, 150.0, "cobra", 2.5, 1.5, true));
    m.insert("primary".into(), c(true, 10.0, 38.0, 4000.0, 55.0, 145.0, "cobra", 2.0, 1.2, true));
    m.insert("secondary".into(), c(true, 9.0, 35.0, 3500.0, 45.0, 140.0, "cobra", 1.8, 1.0, false));
    m.insert("residential".into(), c(true, 7.0, 32.0, 3000.0, 30.0, 140.0, "cobra", 1.2, 0.8, false));
    m.insert("service".into(), c(false, 6.0, 28.0, 3000.0, 22.0, 140.0, "cobra", 0.8, 0.6, false));
    m.insert("paths".into(), c(true, 4.5, 24.0, 3000.0, 16.0, 160.0, "post_top", 0.0, 0.6, false));
    m.insert("parking".into(), c(true, 10.0, 35.0, 4000.0, 60.0, 150.0, "double", 1.2, 0.0, false));
    m.insert("pitches".into(), c(true, 18.0, 0.0, 5000.0, 220.0, 60.0, "flood", 0.0, 4.0, false));
    m.insert("platforms".into(), c(true, 5.0, 18.0, 4000.0, 22.0, 150.0, "post_top", 0.0, 0.0, false));
    m.insert("fuel".into(), c(true, 5.2, 7.0, 5000.0, 30.0, 150.0, "none", 0.0, 0.0, false));
    m.insert("piers".into(), c(true, 4.5, 20.0, 3000.0, 18.0, 160.0, "post_top", 0.0, 0.0, false));
    m
}

/// Uzupełnienie kategorii brakujących w starszym projekcie.
pub fn merge_defaults(cfg: &mut LightsCfg) {
    for (k, v) in default_categories() {
        cfg.categories.entry(k).or_insert(v);
    }
}

fn spec_of(cat: &LightCat) -> Spec {
    Spec {
        style: cat.style.clone(),
        height: cat.height,
        arm: cat.arm,
        kelvin: cat.kelvin,
        color: None,
        intensity: cat.intensity,
        cone: cat.cone,
        heads: if cat.style == "double" { 2 } else { 1 },
        tilt: 0.0,
        flash: None,
        levels: vec![],
    }
}

pub fn road_category(e: &Element) -> Option<&'static str> {
    let hw = e.tag("highway")?;
    Some(match hw {
        "motorway" | "motorway_link" => "motorway",
        "trunk" | "trunk_link" | "primary" | "primary_link" => "primary",
        "secondary" | "secondary_link" | "tertiary" | "tertiary_link" => "secondary",
        "residential" | "unclassified" | "living_street" => "residential",
        "service" => match e.tag("service") {
            Some("driveway") | Some("parking_aisle") | Some("emergency_access") => return None,
            _ => "service",
        },
        "pedestrian" | "footway" | "cycleway" | "path" | "steps" => "paths",
        _ => return None,
    })
}

/// Szerokość jezdni w metrach (tag `width`, liczba pasów albo typowa dla klasy).
pub fn road_width(e: &Element) -> f64 {
    if let Some(w) = e.tag("width").and_then(parse_height) {
        if (1.0..60.0).contains(&w) {
            return w;
        }
    }
    let oneway = matches!(e.tag("oneway"), Some("yes") | Some("1") | Some("-1"));
    if let Some(l) = e.tag("lanes").and_then(|v| v.parse::<f64>().ok()) {
        if (1.0..12.0).contains(&l) {
            return l * 3.2;
        }
    }
    match e.tag("highway").unwrap_or("") {
        "motorway" => {
            if oneway {
                8.0
            } else {
                22.0
            }
        }
        "trunk" | "primary" => {
            if oneway {
                6.5
            } else {
                8.0
            }
        }
        "motorway_link" | "trunk_link" | "primary_link" | "secondary_link" | "tertiary_link" => 5.0,
        "secondary" => 7.0,
        "tertiary" => 6.5,
        "residential" | "living_street" => 5.5,
        "unclassified" => 5.0,
        "service" => 3.5,
        "pedestrian" => 4.0,
        "footway" | "cycleway" | "steps" => 2.0,
        "path" => 1.5,
        _ => 4.0,
    }
}

/// Czy droga jest oświetlona: Some(true/false) z tagu `lit`, None – brak informacji.
pub fn lit(e: &Element) -> Option<bool> {
    let v = e.tag("lit")?;
    Some(match v {
        "no" | "disused" | "false" => false,
        "yes" | "24/7" | "automatic" | "sunset-sunrise" | "limited" | "interval" | "dusk-dawn" | "true" => true,
        other => other.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false),
    })
}

/// Indeks wielokątów (zabudowa) do szybkiego testu „czy punkt jest w mieście”.
struct PolyIndex {
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
    polys: Vec<(BBox, Vec<LatLon>)>,
    cell: f64,
}

impl PolyIndex {
    fn new(polys: Vec<Vec<LatLon>>) -> Self {
        let cell = 0.01;
        let mut idx = Self { cells: Default::default(), polys: vec![], cell };
        for p in polys {
            let b = BBox::of(&p);
            let k = idx.polys.len();
            for x in (b.west / cell).floor() as i64..=(b.east / cell).floor() as i64 {
                for y in (b.south / cell).floor() as i64..=(b.north / cell).floor() as i64 {
                    idx.cells.entry((x, y)).or_default().push(k);
                }
            }
            idx.polys.push((b, p));
        }
        idx
    }
    fn contains(&self, p: LatLon) -> bool {
        let k = ((p.lon / self.cell).floor() as i64, (p.lat / self.cell).floor() as i64);
        self.cells.get(&k).map(|v| v.iter().any(|&i| self.polys[i].0.contains(p) && geo::point_in_polygon(p, &self.polys[i].1))).unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GenResult {
    pub lights: Vec<Light>,
    pub counts: BTreeMap<String, usize>,
    pub models: usize,
    pub warnings: Vec<String>,
}

/// Główny generator świateł ulicznych i obiektów oświetlanych.
pub fn generate(els: &[Element], area: &Area, cfg: &LightsCfg, manual: &[ManualLight], excluded: &[i64]) -> GenResult {
    let mut cfg = cfg.clone();
    merge_defaults(&mut cfg);
    let cats = &cfg.categories;
    let center = area.bbox().center();
    let mut out: Vec<Light> = vec![];
    let mut roads: SegmentIndex<(&'static str, f64)> = SegmentIndex::new(center, 40.0);
    let mut urban: Vec<Vec<LatLon>> = vec![];
    for e in els {
        if let Some(cat) = road_category(e) {
            if e.tag("tunnel").map(|v| v != "no").unwrap_or(false) || e.is("covered", "yes") {
                continue;
            }
            roads.insert_line(&e.line, (cat, road_width(e) / 2.0));
        }
        if matches!(e.tag("landuse"), Some("residential") | Some("commercial") | Some("retail") | Some("industrial")) {
            urban.extend(e.outers.iter().cloned());
        }
    }
    let urban = PolyIndex::new(urban);
    let heading_fix = if cfg.flip_arm { 180.0 } else { 0.0 };

    // ── 1. Latarnie z OSM ──
    let mut mapped = PointGrid::new(center, 20.0);
    if let Some(cat) = cats.get("mapped").filter(|c| c.enabled) {
        for e in els.iter().filter(|e| e.t == 'n') {
            let is_lamp = e.is("highway", "street_lamp") || e.is("man_made", "street_lamp") || e.tag("light_source").is_some();
            let Some(p) = e.p.filter(|_| is_lamp) else { continue };
            if !area.contains(p) || excluded.contains(&e.id) {
                continue;
            }
            mapped.insert(p);
            let near = roads.nearest(p, 30.0);
            let mut spec = spec_of(cat);
            // Wysokość: tag > klasa drogi > domyślna.
            spec.height = e.tag("height").and_then(parse_height).filter(|h| (2.0..40.0).contains(h)).unwrap_or_else(|| {
                near.as_ref().and_then(|n| cats.get(n.3 .0)).map(|c| c.height).unwrap_or(cat.height)
            });
            let mount = e.tag("lamp_mount").or(e.tag("light:mount")).unwrap_or("");
            let support = e.tag("support").unwrap_or("");
            let pedestrian = near.as_ref().map(|n| n.3 .0 == "paths").unwrap_or(false);
            spec.style = if mount.contains("wall") || support.contains("wall") {
                "wall".into()
            } else if mount.contains("suspended") || support.contains("suspended") || support == "catenary" || support == "wire" {
                "none".into()
            } else if mount == "post_top" || matches!(e.tag("lamp_type"), Some("lantern")) || matches!(e.tag("light_source"), Some("lantern")) || (pedestrian && mount.is_empty()) {
                "post_top".into()
            } else if e.tag("light:count").and_then(|v| v.parse::<u32>().ok()).unwrap_or(1) >= 2 {
                "double".into()
            } else if e.is("light_source", "floodlight") {
                "flood".into()
            } else {
                "cobra".into()
            };
            if spec.style == "post_top" {
                spec.height = e.tag("height").and_then(parse_height).unwrap_or(4.5).min(spec.height);
                spec.cone = 160.0;
                spec.intensity = cat.intensity * 0.55;
            }
            if spec.style == "flood" {
                spec.tilt = 35.0;
                spec.cone = 70.0;
            }
            // Barwa: sód (pomarańczowy) / LED / rtęć.
            let colour = format!("{} {} {}", e.tag("light:colour").unwrap_or(""), e.tag("lamp_type").unwrap_or(""), e.tag("light:method").unwrap_or("")).to_lowercase();
            if colour.contains("sodium") || colour.contains("orange") || colour.contains("sod") {
                spec.kelvin = 2000.0;
            } else if colour.contains("yellow") {
                spec.kelvin = 2400.0;
            } else if colour.contains("mercury") || colour.contains("metal") {
                spec.kelvin = 4500.0;
            } else if colour.contains("led") && colour.contains("white") {
                spec.kelvin = 4000.0;
            }
            let mut heading = 0.0;
            if let Some((q, dist, _, (_, half))) = &near {
                heading = geo::bearing(p, *q);
                if spec.style == "cobra" || spec.style == "wall" {
                    spec.arm = (dist - half * 0.3).clamp(0.6, (cat.arm * 1.6).max(1.0));
                }
            }
            if let Some(d) = e.tag("light:direction").or(e.tag("direction")).and_then(|v| v.parse::<f64>().ok()) {
                heading = d;
            }
            let alt = if spec.style == "wall" || spec.style == "none" { spec.height } else { 0.0 };
            out.push(Light { lat: p.lat, lon: p.lon, heading: (heading + heading_fix) % 360.0, cat: "mapped".into(), src: 'o', osm: e.id, alt, spec });
        }
    }

    // ── 2. Drogi i ścieżki ──
    let mut placed = PointGrid::new(center, 20.0);
    // Najpierw jezdnie (od najważniejszych), potem ścieżki – chodnik przy oświetlonej ulicy
    // korzysta z jej lamp i nie dostaje drugiego rzędu słupów.
    let rank = |c: &str| ["motorway", "primary", "secondary", "residential", "service", "paths"].iter().position(|x| *x == c).unwrap_or(9);
    let mut ways: Vec<(&Element, &'static str)> = els.iter().filter(|e| e.t == 'w').filter_map(|e| road_category(e).map(|c| (e, c))).collect();
    ways.sort_by_key(|(_, c)| rank(c));
    // Jezdnie, przy których stanęły już latarnie – ścieżka w ich pasie (chodnik bez tagu
    // sidewalk, ścieżka rowerowa wzdłuż ulicy) nie dostaje osobnego rzędu słupów.
    let mut lit_streets: SegmentIndex<()> = SegmentIndex::new(center, 40.0);
    for (e, cat_id) in ways {
        let Some(cat) = cats.get(cat_id).filter(|c| c.enabled) else { continue };
        if e.tag("tunnel").map(|v| v != "no").unwrap_or(false) || e.is("covered", "yes") || e.is("area", "yes") {
            continue;
        }
        if cat_id == "paths" && matches!(e.tag("footway").or(e.tag("cycleway")), Some("sidewalk") | Some("crossing") | Some("traffic_island")) {
            continue;
        }
        let line = &e.line;
        if line.len() < 2 || !line.iter().any(|p| area.contains(*p)) {
            continue;
        }
        let is_lit = match lit(e) {
            Some(v) => v,
            None => cfg.guess_urban && cat_id != "paths" && cat_id != "service" && cat_id != "motorway" && urban.contains(line[line.len() / 2]),
        };
        if !is_lit {
            continue;
        }
        let len = geo::line_length(line);
        if len < 12.0 {
            continue;
        }
        let spacing = cat.spacing.max(8.0);
        let count = ((len / spacing).floor() as usize).max(1);
        let step = len / count as f64;
        let half = road_width(e) / 2.0;
        let wide = cat.both_sides && (half * 2.0 >= 9.0 || cat_id == "motorway" || cat_id == "primary");
        let dist = half + cat.offset.max(0.3) + if cat.style == "post_top" { 0.0 } else { 0.2 };
        let spec = spec_of(cat);
        let mut any = false;
        for k in 0..count {
            let s = (k as f64 + 0.5) * step;
            let Some((q, brg)) = geo::point_along(line, s) else { continue };
            let side = if wide && k % 2 == 1 { -90.0 } else { 90.0 };
            let pos = geo::destination(q, brg + side, dist);
            if !area.contains(pos) {
                continue;
            }
            if mapped.nearest_within(pos, cfg.avoid_mapped.max(spacing * 0.4)).is_some() {
                continue;
            }
            // Ścieżka tuż przy ulicy jest już oświetlona latarniami jezdni.
            let gap = if cat_id == "paths" { (spacing * 0.45).max(14.0) } else { spacing * 0.45 };
            if placed.nearest_within(pos, gap).is_some() {
                continue;
            }
            if cat_id == "paths" && lit_streets.nearest(pos, 14.0).is_some() {
                continue;
            }
            // Słup nie może stać na innej jezdni (skrzyżowania, drogi równoległe).
            if let Some((_, d, _, (_, other_half))) = roads.nearest(pos, 15.0) {
                if d < other_half + 0.4 {
                    continue;
                }
            }
            placed.insert(pos);
            let heading = (brg + side + 180.0 + heading_fix).rem_euclid(360.0);
            out.push(Light { lat: pos.lat, lon: pos.lon, heading, cat: cat_id.into(), src: 'g', osm: e.id, alt: 0.0, spec: spec.clone() });
            any = true;
        }
        if any && cat_id != "paths" {
            lit_streets.insert_line(line, ());
        }
    }

    // ── 3. Obszary: parkingi, boiska, perony, stacje paliw, pomosty ──
    for e in els {
        if e.is("amenity", "parking") && e.is_area() {
            let Some(cat) = cats.get("parking").filter(|c| c.enabled) else { continue };
            if matches!(e.tag("parking"), Some("underground") | Some("multi-storey") | Some("rooftop")) || lit(e) == Some(false) {
                continue;
            }
            let ring = &e.outers[0];
            let a = geo::polygon_area(ring);
            if lit(e).is_none() && (a < 1500.0 || !cfg.guess_urban) {
                continue;
            }
            let pts = grid_in_polygon(ring, cat.spacing.max(15.0), 3.0, 300);
            let axis = main_axis(ring);
            for p in pts.into_iter().filter(|p| area.contains(*p)) {
                if mapped.nearest_within(p, 10.0).is_some() {
                    continue;
                }
                out.push(Light { lat: p.lat, lon: p.lon, heading: axis, cat: "parking".into(), src: 'g', osm: e.id, alt: 0.0, spec: spec_of(cat) });
            }
        }
        if matches!(e.tag("leisure"), Some("pitch") | Some("stadium") | Some("track")) && e.is_area() && lit(e) == Some(true) {
            let Some(cat) = cats.get("pitches").filter(|c| c.enabled) else { continue };
            let ring = &e.outers[0];
            if !ring.iter().any(|p| area.contains(*p)) {
                continue;
            }
            for (p, heading, d) in pitch_masts(ring, cat.offset.max(2.0)) {
                let mut spec = spec_of(cat);
                spec.heads = if e.is("leisure", "stadium") { 4 } else { 3 };
                spec.tilt = (cat.height / (d * 0.55).max(5.0)).atan().to_degrees().clamp(15.0, 60.0);
                out.push(Light { lat: p.lat, lon: p.lon, heading, cat: "pitches".into(), src: 'g', osm: e.id, alt: 0.0, spec });
            }
        }
        let platform = e.is("railway", "platform") || (e.is("public_transport", "platform") && (e.tags.contains_key("railway") || e.is("train", "yes")));
        if platform && e.t != 'n' && lit(e) != Some(false) {
            let Some(cat) = cats.get("platforms").filter(|c| c.enabled) else { continue };
            let line = if e.is_area() { centerline(&e.outers[0]) } else { e.line.clone() };
            place_along(&line, cat, "platforms", e.id, area, &mapped, &mut out);
        }
        if e.is("man_made", "pier") && lit(e) == Some(true) && !e.is_area() {
            let Some(cat) = cats.get("piers").filter(|c| c.enabled) else { continue };
            place_along(&e.line, cat, "piers", e.id, area, &mapped, &mut out);
        }
        if e.is("amenity", "fuel") {
            let Some(cat) = cats.get("fuel").filter(|c| c.enabled) else { continue };
            let pts: Vec<LatLon> = if e.is_area() {
                grid_in_polygon(&e.outers[0], cat.spacing.max(4.0), 2.0, 20)
            } else if let Some(p) = e.p {
                [45.0, 135.0, 225.0, 315.0].iter().map(|b| geo::destination(p, *b, 4.0)).collect()
            } else {
                vec![]
            };
            let mut spec = spec_of(cat);
            spec.style = "none".into();
            for p in pts.into_iter().filter(|p| area.contains(*p)) {
                out.push(Light { lat: p.lat, lon: p.lon, heading: 0.0, cat: "fuel".into(), src: 'g', osm: e.id, alt: cat.height, spec: spec.clone() });
            }
        }
    }

    // ── 4. Lampy postawione ręcznie ──
    for m in manual {
        let Some(cat) = cats.get(&m.category).or(cats.get("residential")) else { continue };
        let spec = spec_of(cat);
        let alt = if spec.style == "wall" || spec.style == "none" { spec.height } else { 0.0 };
        out.push(Light { lat: m.lat, lon: m.lon, heading: m.heading, cat: m.category.clone(), src: 'm', osm: 0, alt, spec });
    }

    let mut res = GenResult { lights: out, ..Default::default() };
    for l in &res.lights {
        *res.counts.entry(l.cat.clone()).or_default() += 1;
    }
    res.models = count_models(&res.lights);
    res
}

/// Wzór do kalibracji w symulatorze: po trzy lampy każdej włączonej kategorii w rzędach
/// co 30 m (wysięgniki na północ), na końcu światło przeszkodowe i latarnia morska.
pub fn test_pattern(center: LatLon, cfg: &LightsCfg, obs: &ObstaclesCfg) -> Vec<Light> {
    let mut cfg = cfg.clone();
    merge_defaults(&mut cfg);
    let order = ["mapped", "motorway", "primary", "secondary", "residential", "service", "paths", "parking", "pitches", "platforms", "fuel", "piers"];
    let mut out = vec![];
    let mut row = 0usize;
    for id in order {
        let Some(cat) = cfg.categories.get(id).filter(|c| c.enabled) else { continue };
        let start = geo::destination(center, 0.0, row as f64 * 30.0 - 120.0);
        for k in 0..3 {
            let p = geo::destination(start, 90.0, k as f64 * 20.0 - 20.0);
            let mut spec = spec_of(cat);
            if spec.style == "flood" {
                spec.tilt = 30.0;
            }
            let alt = if spec.style == "wall" || spec.style == "none" { spec.height } else { 0.0 };
            let heading = if cfg.flip_arm { 180.0 } else { 0.0 };
            out.push(Light { lat: p.lat, lon: p.lon, heading, cat: id.into(), src: 'm', osm: 0, alt, spec });
        }
        row += 1;
    }
    let east = geo::destination(center, 90.0, 80.0);
    out.push(obstruction(east, 60.0, "obstacle", 0, obs, obs.flashing, 1.0));
    out.push(obstruction(geo::destination(east, 0.0, 30.0), 30.0, "obstacle", 0, obs, false, 0.4));
    let beacon = geo::destination(east, 0.0, 70.0);
    out.push(Light {
        lat: beacon.lat,
        lon: beacon.lon,
        heading: 0.0,
        cat: "lighthouse".into(),
        src: 'm',
        osm: 0,
        alt: 25.0,
        spec: Spec { style: "beacon".into(), height: 0.0, arm: 0.0, kelvin: 0.0, color: Some([1.0, 0.97, 0.88]), intensity: 600.0, cone: 360.0, heads: 1, tilt: 0.0, flash: light_character("Fl", Some(5.0)), levels: vec![] },
    });
    out
}

pub fn count_models(lights: &[Light]) -> usize {
    let mut keys: Vec<String> = lights.iter().map(|l| l.spec.key()).collect();
    keys.sort();
    keys.dedup();
    keys.len()
}

fn place_along(line: &[LatLon], cat: &LightCat, id: &str, osm: i64, area: &Area, mapped: &PointGrid, out: &mut Vec<Light>) {
    let len = geo::line_length(line);
    if len < 8.0 {
        return;
    }
    let count = ((len / cat.spacing.max(6.0)).floor() as usize).max(1);
    let step = len / count as f64;
    for k in 0..count {
        let Some((p, brg)) = geo::point_along(line, (k as f64 + 0.5) * step) else { continue };
        if !area.contains(p) || mapped.nearest_within(p, 8.0).is_some() {
            continue;
        }
        out.push(Light { lat: p.lat, lon: p.lon, heading: (brg + 90.0) % 360.0, cat: id.into(), src: 'g', osm, alt: 0.0, spec: spec_of(cat) });
    }
}

/// Kierunek najdłuższej krawędzi wielokąta (oś główna), stopnie.
pub fn main_axis(ring: &[LatLon]) -> f64 {
    let mut best = (0.0, 0.0);
    for i in 0..ring.len() {
        let (a, b) = (ring[i], ring[(i + 1) % ring.len()]);
        let d = geo::haversine(a, b);
        if d > best.0 {
            best = (d, geo::bearing(a, b));
        }
    }
    best.1
}

/// Prostokąt opisany zorientowany wzdłuż osi głównej: (środek, oś, długość, szerokość).
fn oriented_box(ring: &[LatLon]) -> (LatLon, f64, f64, f64) {
    let axis = main_axis(ring);
    let c = geo::polygon_centroid(ring);
    let loc = geo::Local::new(c);
    let (s, co) = axis.to_radians().sin_cos();
    let (mut u0, mut u1, mut v0, mut v1) = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for p in ring {
        let (x, y) = loc.to_xy(*p);
        let u = x * s + y * co;
        let v = x * co - y * s;
        u0 = u0.min(u);
        u1 = u1.max(u);
        v0 = v0.min(v);
        v1 = v1.max(v);
    }
    let (uc, vc) = ((u0 + u1) / 2.0, (v0 + v1) / 2.0);
    let center = loc.to_ll(uc * s + vc * co, uc * co - vc * s);
    (center, axis, u1 - u0, v1 - v0)
}

/// Oś środkowa długiego obszaru (np. peronu).
fn centerline(ring: &[LatLon]) -> Vec<LatLon> {
    let (c, axis, len, _) = oriented_box(ring);
    let half = (len / 2.0 - 2.0).max(1.0);
    vec![geo::destination(c, axis + 180.0, half), geo::destination(c, axis, half)]
}

/// Maszty boiska: narożniki prostokąta (odsunięte na zewnątrz) i środki dłuższych boków.
/// Zwraca (pozycja, kierunek na środek, odległość od środka).
fn pitch_masts(ring: &[LatLon], out_off: f64) -> Vec<(LatLon, f64, f64)> {
    let (c, axis, len, wid) = oriented_box(ring);
    let (hl, hw) = (len / 2.0 + out_off, wid / 2.0 + out_off);
    let mut pts = vec![];
    for (su, sv) in [(1.0, 1.0), (1.0, -1.0), (-1.0, 1.0), (-1.0, -1.0)] {
        let p = geo::destination(geo::destination(c, axis, su * hl * 0.82), axis + 90.0, sv * hw);
        pts.push(p);
    }
    if len > 80.0 {
        pts.push(geo::destination(c, axis + 90.0, hw));
        pts.push(geo::destination(c, axis - 90.0, hw));
    }
    pts.into_iter().map(|p| (p, geo::bearing(p, c), geo::haversine(p, c))).collect()
}

/// Siatka punktów wewnątrz wielokąta, wyrównana do osi głównej, z marginesem od krawędzi.
pub fn grid_in_polygon(ring: &[LatLon], spacing: f64, inset: f64, cap: usize) -> Vec<LatLon> {
    let (c, axis, len, wid) = oriented_box(ring);
    let mut out = vec![];
    let nu = ((len - 2.0 * inset) / spacing).floor().max(0.0) as i64;
    let nv = ((wid - 2.0 * inset) / spacing).floor().max(0.0) as i64;
    for iu in 0..=nu {
        for iv in 0..=nv {
            let u = (iu as f64 - nu as f64 / 2.0) * spacing;
            let v = (iv as f64 - nv as f64 / 2.0) * spacing;
            let p = geo::destination(geo::destination(c, axis, u), axis + 90.0, v);
            if geo::point_in_polygon(p, ring) && edge_distance(p, ring) >= inset * 0.7 {
                out.push(p);
                if out.len() >= cap {
                    return out;
                }
            }
        }
    }
    if out.is_empty() && geo::point_in_polygon(c, ring) {
        out.push(c);
    }
    out
}

fn edge_distance(p: LatLon, ring: &[LatLon]) -> f64 {
    let loc = geo::Local::new(p);
    (0..ring.len()).map(|i| geo::nearest_on_segment(&loc, p, ring[i], ring[(i + 1) % ring.len()]).1).fold(f64::MAX, f64::min)
}

// ───────────────────────────── Przeszkody i latarnie morskie ─────────────────────────────

const RED: [f64; 3] = [1.0, 0.05, 0.02];

fn obstruction(p: LatLon, alt: f64, cat: &str, osm: i64, cfg: &ObstaclesCfg, flashing: bool, intensity_k: f64) -> Light {
    Light {
        lat: p.lat,
        lon: p.lon,
        heading: 0.0,
        cat: cat.into(),
        src: 'o',
        osm,
        alt,
        spec: Spec {
            style: "obstruction".into(),
            height: 0.0,
            arm: 0.0,
            kelvin: 0.0,
            color: Some(RED),
            intensity: cfg.intensity * intensity_k,
            cone: 360.0,
            heads: 1,
            tilt: 0.0,
            // Światło średniej intensywności typu B: ok. 40 błysków na minutę.
            flash: flashing.then_some((0.67, 0.5)),
            levels: vec![],
        },
    }
}

/// Charakterystyka latarni morskiej → (częstotliwość, czas błysku); None = światło stałe.
pub fn light_character(character: &str, period: Option<f64>) -> Option<(f64, f64)> {
    let ch = character.trim().to_uppercase();
    let per = period.filter(|p| *p > 0.1).unwrap_or(5.0);
    let base = ch.split('(').next().unwrap_or("").trim().to_string();
    let group = ch.split('(').nth(1).and_then(|r| r.split(')').next()).and_then(|g| g.split('+').map(|x| x.trim().parse::<f64>().ok()).sum::<Option<f64>>()).unwrap_or(1.0);
    match base.as_str() {
        "F" | "" => None,
        "VQ" | "IVQ" => Some((2.0, 0.2)),
        "Q" | "IQ" => Some((1.0, 0.3)),
        "UQ" => Some((4.0, 0.1)),
        "LFL" => Some((1.0 / per, 2.0f64.min(per * 0.4))),
        "ISO" => Some((1.0 / per, per / 2.0)),
        "OC" => Some((1.0 / per, per * 0.75)),
        _ => Some((group / per, (0.5f64).min(per / (group * 2.0)))),
    }
}

pub fn obstacles(els: &[Element], area: &Area, cfg: &ObstaclesCfg) -> Vec<Light> {
    let mut out = vec![];
    for e in els {
        let Some(p) = e.anchor() else { continue };
        if !area.contains(p) {
            continue;
        }
        let mm = e.tag("man_made").unwrap_or("");
        let turbine = (e.is("power", "generator") && e.is("generator:source", "wind")) || e.is("generator:method", "wind_turbine");
        let seamark = matches!(e.tag("seamark:type"), Some("light_major") | Some("light_minor"));
        if (mm == "lighthouse" || seamark) && cfg.lighthouses {
            let colour = e.tag("seamark:light:colour").or(e.tag("seamark:light:1:colour")).unwrap_or("white").to_lowercase();
            let color = match colour.split(';').next().unwrap_or("white") {
                "red" => [1.0, 0.1, 0.05],
                "green" => [0.1, 1.0, 0.25],
                "yellow" | "amber" => [1.0, 0.75, 0.2],
                _ => [1.0, 0.97, 0.88],
            };
            let ch = e.tag("seamark:light:character").or(e.tag("seamark:light:1:character")).unwrap_or("Fl");
            let per = e.tag("seamark:light:period").or(e.tag("seamark:light:1:period")).and_then(|v| v.parse::<f64>().ok());
            let range = e.tag("seamark:light:range").or(e.tag("seamark:light:1:range")).and_then(|v| v.parse::<f64>().ok()).unwrap_or(10.0);
            let h = e.tag("height").and_then(parse_height).or_else(|| e.tag("seamark:light:height").and_then(parse_height)).unwrap_or(if mm == "lighthouse" { 25.0 } else { 8.0 });
            out.push(Light {
                lat: p.lat,
                lon: p.lon,
                heading: 0.0,
                cat: "lighthouse".into(),
                src: 'o',
                osm: e.id,
                alt: h,
                spec: Spec {
                    style: "beacon".into(),
                    height: 0.0,
                    arm: 0.0,
                    kelvin: 0.0,
                    color: Some(color),
                    intensity: (range * 40.0).clamp(150.0, 1500.0),
                    cone: 360.0,
                    heads: 1,
                    tilt: 0.0,
                    flash: light_character(ch, per),
                    levels: vec![],
                },
            });
            continue;
        }
        let height = e.tag("height").and_then(parse_height).or_else(|| e.tag("est_height").and_then(parse_height));
        if turbine {
            if !cfg.turbines {
                continue;
            }
            let rotor = e.tag("rotor:diameter").and_then(parse_height);
            let hub = e
                .tag("hub_height")
                .or(e.tag("generator:hub_height"))
                .and_then(parse_height)
                .or_else(|| height.map(|h| h - rotor.unwrap_or(h * 0.7) / 2.0))
                .unwrap_or(if cfg.default_height > 0.0 { cfg.default_height } else { 100.0 });
            if cfg.turbine_models {
                out.push(Light {
                    lat: p.lat,
                    lon: p.lon,
                    heading: 270.0,
                    cat: "turbine".into(),
                    src: 'o',
                    osm: e.id,
                    alt: 0.0,
                    spec: Spec {
                        style: "turbine".into(),
                        height: hub,
                        arm: rotor.unwrap_or(hub * 0.9) / 2.0,
                        kelvin: 0.0,
                        color: Some(RED),
                        intensity: cfg.intensity,
                        cone: 360.0,
                        heads: 1,
                        tilt: 0.0,
                        flash: cfg.flashing.then_some((0.67, 0.5)),
                        levels: vec![],
                    },
                });
            } else {
                out.push(obstruction(p, hub + 3.0, "turbine", e.id, cfg, cfg.flashing, 1.0));
                if cfg.intermediate && hub > 90.0 {
                    out.push(obstruction(p, hub * 0.5, "turbine", e.id, cfg, false, 0.4));
                }
            }
            continue;
        }
        let wanted = match mm {
            "mast" | "communications_tower" => cfg.masts,
            "tower" => cfg.towers,
            "chimney" | "flare" => cfg.chimneys,
            "water_tower" => cfg.towers,
            "crane" => cfg.cranes,
            _ => e.is("power", "tower") && cfg.power_towers,
        };
        if !wanted {
            continue;
        }
        let Some(h) = height.or((cfg.default_height > 0.0).then_some(cfg.default_height)) else { continue };
        if h < cfg.min_height {
            continue;
        }
        let flashing = cfg.flashing && h >= 45.0;
        out.push(obstruction(p, h + 0.5, "obstacle", e.id, cfg, flashing, 1.0));
        if cfg.intermediate && h > 90.0 {
            let levels = (h / 52.0).floor() as usize;
            for k in 1..=levels {
                let lh = h * k as f64 / (levels + 1) as f64;
                out.push(obstruction(p, lh, "obstacle", e.id, cfg, false, 0.4));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn tags(kv: &[(&str, &str)]) -> HashMap<String, String> {
        kv.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn area() -> Area {
        Area { outer: vec![LatLon::new(50.0, 19.0), LatLon::new(50.0, 19.02), LatLon::new(50.02, 19.02), LatLon::new(50.02, 19.0)], holes: vec![] }
    }

    #[test]
    fn lit_road_gets_lamps_beside_it() {
        let road = Element {
            t: 'w',
            id: 10,
            tags: tags(&[("highway", "residential"), ("lit", "yes")]),
            line: vec![LatLon::new(50.01, 19.002), LatLon::new(50.01, 19.012)],
            ..Default::default()
        };
        let r = generate(&[road], &area(), &LightsCfg::default(), &[], &[]);
        let n = r.lights.len();
        // ~715 m drogi co 32 m
        assert!((18..=24).contains(&n), "{n}");
        for l in &r.lights {
            let d = geo::haversine(LatLon::new(l.lat, l.lon), LatLon::new(50.01, l.lon));
            assert!((3.0..5.0).contains(&d), "odległość od osi {d}");
            // droga biegnie na wschód, lampa po prawej (południe) – wysięgnik na północ
            assert!(l.heading < 10.0 || l.heading > 350.0, "{}", l.heading);
        }
    }

    #[test]
    fn mapped_lamp_faces_road_and_blocks_generated() {
        let road = Element {
            t: 'w',
            id: 10,
            tags: tags(&[("highway", "secondary"), ("lit", "yes")]),
            line: vec![LatLon::new(50.01, 19.002), LatLon::new(50.01, 19.004)],
            ..Default::default()
        };
        let lamp_pos = geo::destination(LatLon::new(50.01, 19.003), 0.0, 5.0);
        let lamp = Element { t: 'n', id: 1, tags: tags(&[("highway", "street_lamp"), ("height", "9"), ("lamp_type", "sodium")]), p: Some(lamp_pos), ..Default::default() };
        let r = generate(&[road, lamp], &area(), &LightsCfg::default(), &[], &[]);
        let m: Vec<_> = r.lights.iter().filter(|l| l.src == 'o').collect();
        assert_eq!(m.len(), 1);
        assert!((m[0].heading - 180.0).abs() < 2.0, "{}", m[0].heading);
        assert_eq!(m[0].spec.height, 9.0);
        assert_eq!(m[0].spec.kelvin, 2000.0);
        let near: Vec<_> = r.lights.iter().filter(|l| l.src == 'g' && geo::haversine(LatLon::new(l.lat, l.lon), lamp_pos) < 12.0).collect();
        assert!(near.is_empty());
    }

    #[test]
    fn unlit_and_guessing() {
        let road = Element { t: 'w', id: 10, tags: tags(&[("highway", "residential")]), line: vec![LatLon::new(50.01, 19.002), LatLon::new(50.01, 19.006)], ..Default::default() };
        assert!(generate(std::slice::from_ref(&road), &area(), &LightsCfg::default(), &[], &[]).lights.is_empty());
        let town = Element {
            t: 'w',
            id: 11,
            tags: tags(&[("landuse", "residential")]),
            outers: vec![vec![LatLon::new(50.005, 19.0), LatLon::new(50.005, 19.01), LatLon::new(50.015, 19.01), LatLon::new(50.015, 19.0)]],
            ..Default::default()
        };
        assert!(!generate(&[road, town], &area(), &LightsCfg::default(), &[], &[]).lights.is_empty());
    }

    #[test]
    fn sidewalk_reuses_street_lamps() {
        let road = Element { t: 'w', id: 10, tags: tags(&[("highway", "residential"), ("lit", "yes")]), line: vec![LatLon::new(50.01, 19.002), LatLon::new(50.01, 19.008)], ..Default::default() };
        // Chodnik oznaczony jako sidewalk – bez własnych lamp.
        let walk = Element { t: 'w', id: 11, tags: tags(&[("highway", "footway"), ("footway", "sidewalk"), ("lit", "yes")]), line: vec![LatLon::new(50.01005, 19.002), LatLon::new(50.01005, 19.008)], ..Default::default() };
        // Ścieżka równoległa bez tagu sidewalk, 6 m od jezdni – też korzysta z latarni ulicy.
        let path = Element { t: 'w', id: 12, tags: tags(&[("highway", "footway"), ("lit", "yes")]), line: vec![LatLon::new(50.01008, 19.002), LatLon::new(50.01008, 19.008)], ..Default::default() };
        let r = generate(&[walk, path, road], &area(), &LightsCfg::default(), &[], &[]);
        let paths = r.lights.iter().filter(|l| l.cat == "paths").count();
        assert!(paths <= 2, "za dużo lamp przy chodniku: {paths}");
        assert!(r.lights.iter().filter(|l| l.cat == "residential").count() > 8);
    }

    #[test]
    fn pitch_and_obstacles() {
        let ring = vec![LatLon::new(50.01, 19.01), LatLon::new(50.01, 19.0115), LatLon::new(50.0106, 19.0115), LatLon::new(50.0106, 19.01)];
        let pitch = Element { t: 'w', id: 5, tags: tags(&[("leisure", "pitch"), ("lit", "yes")]), outers: vec![ring.clone()], line: ring, ..Default::default() };
        let r = generate(&[pitch], &area(), &LightsCfg::default(), &[], &[]);
        assert!(r.lights.len() >= 4);
        assert!(r.lights.iter().all(|l| l.spec.tilt >= 15.0 && l.spec.heads == 3));
        let mast = Element { t: 'n', id: 7, tags: tags(&[("man_made", "mast"), ("height", "160")]), p: Some(LatLon::new(50.01, 19.01)), ..Default::default() };
        let o = obstacles(&[mast], &area(), &ObstaclesCfg::default());
        assert_eq!(o.iter().filter(|l| l.spec.flash.is_some()).count(), 1);
        assert!(o.len() >= 3, "{}", o.len());
        assert!(o.iter().all(|l| l.spec.color == Some(RED)));
    }

    #[test]
    fn lighthouse_characters() {
        assert_eq!(light_character("F", None), None);
        let (f, d) = light_character("Fl", Some(10.0)).unwrap();
        assert!((f - 0.1).abs() < 1e-9 && d <= 0.5);
        let (f2, _) = light_character("Fl(2)", Some(10.0)).unwrap();
        assert!((f2 - 0.2).abs() < 1e-9);
        assert_eq!(light_character("Q", None), Some((1.0, 0.3)));
    }

    #[test]
    fn spec_quantization_merges_models() {
        let a = Spec { style: "cobra".into(), height: 8.1, arm: 1.49, kelvin: 3010.0, color: None, intensity: 40.2, cone: 141.0, heads: 1, tilt: 0.0, flash: None, levels: vec![] };
        let mut b = a.clone();
        b.height = 7.9;
        assert_eq!(a.key(), b.key());
        b.height = 9.0;
        assert_ne!(a.key(), b.key());
    }
}
