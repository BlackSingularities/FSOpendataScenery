//! OpenStreetMap przez Overpass API: zapytanie składane z włączonych warstw projektu,
//! odczyt geometrii (`out geom`), składanie wielokątów z relacji i zapis w pamięci projektu.

use crate::geo::{BBox, LatLon};
use crate::net;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub const OVERPASS: [&str; 3] = [
    "https://overpass-api.de/api/interpreter",
    "https://overpass.private.coffee/api/interpreter",
    "https://maps.mail.ru/osm/tools/overpass/api/interpreter",
];

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Element {
    /// "n", "w", "r"
    pub t: char,
    pub id: i64,
    pub tags: HashMap<String, String>,
    /// Węzeł: położenie. Linia / obszar: pusty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub p: Option<LatLon>,
    /// Linia (droga) albo zewnętrzny obrys obszaru (zamknięta droga).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line: Vec<LatLon>,
    /// Pierścienie zewnętrzne wielokąta z relacji (lub zamkniętej drogi).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub outers: Vec<Vec<LatLon>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inners: Vec<Vec<LatLon>>,
}

impl Element {
    pub fn tag(&self, k: &str) -> Option<&str> {
        self.tags.get(k).map(String::as_str)
    }
    pub fn is(&self, k: &str, v: &str) -> bool {
        self.tag(k) == Some(v)
    }
    /// Zamknięta linia albo relacja – obszar.
    pub fn is_area(&self) -> bool {
        !self.outers.is_empty()
    }
    /// Punkt reprezentatywny (węzeł, środek obszaru albo środek linii).
    pub fn anchor(&self) -> Option<LatLon> {
        if let Some(p) = self.p {
            return Some(p);
        }
        if let Some(o) = self.outers.first() {
            return Some(crate::geo::polygon_centroid(o));
        }
        let n = self.line.len();
        (n > 0).then(|| self.line[n / 2])
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct OsmData {
    pub fetched: String,
    pub bbox: BBox,
    pub layers: Vec<String>,
    pub elements: Vec<Element>,
}

/// Warstwy danych do pobrania (wynikają z ustawień projektu).
pub fn layer_queries(layers: &[String]) -> Vec<&'static str> {
    let mut q: Vec<&'static str> = vec![];
    for l in layers {
        match l.as_str() {
            "lamps" => q.extend([
                r#"node["highway"="street_lamp"];"#,
                r#"node["man_made"="street_lamp"];"#,
                r#"node["light_source"~"^(lantern|floodlight|spotlight)$"];"#,
            ]),
            "roads" => q.push(r#"way["highway"~"^(motorway|trunk|primary|secondary|tertiary|motorway_link|trunk_link|primary_link|secondary_link|tertiary_link|residential|unclassified|living_street|service|pedestrian|footway|cycleway|path|steps|track)$"];"#),
            "urban" => q.extend([r#"way["landuse"~"^(residential|commercial|retail|industrial)$"];"#, r#"relation["landuse"~"^(residential|commercial|retail|industrial)$"];"#]),
            "parking" => q.extend([r#"way["amenity"="parking"];"#, r#"relation["amenity"="parking"];"#]),
            "pitches" => q.extend([r#"way["leisure"~"^(pitch|stadium|track)$"];"#, r#"relation["leisure"~"^(pitch|stadium|track)$"];"#]),
            "platforms" => q.extend([r#"way["railway"="platform"];"#, r#"way["public_transport"="platform"]["railway"];"#, r#"way["public_transport"="platform"]["train"="yes"];"#]),
            "fuel" => q.extend([r#"node["amenity"="fuel"];"#, r#"way["amenity"="fuel"];"#]),
            "piers" => q.push(r#"way["man_made"="pier"];"#),
            "obstacles" => q.extend([
                r#"nwr["man_made"~"^(mast|tower|chimney|communications_tower|water_tower|crane|flare|lighthouse)$"];"#,
                r#"nwr["power"="generator"]["generator:source"="wind"];"#,
                r#"node["generator:method"="wind_turbine"];"#,
                r#"node["seamark:type"~"^(light_major|light_minor|landmark)$"];"#,
            ]),
            "power_towers" => q.push(r#"node["power"="tower"]["height"];"#),
            "water" => q.extend([
                r#"way["natural"="water"];"#,
                r#"relation["natural"="water"];"#,
                r#"way["waterway"="riverbank"];"#,
                r#"relation["waterway"="riverbank"];"#,
                r#"way["landuse"~"^(reservoir|basin)$"];"#,
            ]),
            "railways" => q.push(r#"way["railway"~"^(rail|light_rail|tram|subway|narrow_gauge)$"]["tunnel"!="yes"];"#),
            "runways" => q.extend([r#"way["aeroway"~"^(runway|taxiway|apron)$"];"#]),
            "landuse" => q.extend([r#"way["landuse"~"^(quarry|construction|railway|brownfield|landfill)$"];"#]),
            _ => {}
        }
    }
    q.sort();
    q.dedup();
    q
}

pub fn build_query(b: &BBox, layers: &[String]) -> String {
    let parts = layer_queries(layers).join("\n  ");
    format!(
        "[out:json][timeout:180][maxsize:536870912][bbox:{:.6},{:.6},{:.6},{:.6}];\n(\n  {}\n);\nout geom;",
        b.south, b.west, b.north, b.east, parts
    )
}

#[derive(Deserialize)]
struct RawPt {
    lat: f64,
    lon: f64,
}

#[derive(Deserialize)]
struct RawMember {
    #[serde(rename = "type")]
    t: String,
    #[serde(default)]
    role: String,
    #[serde(default)]
    geometry: Vec<Option<RawPt>>,
}

#[derive(Deserialize)]
struct RawEl {
    #[serde(rename = "type")]
    t: String,
    id: i64,
    lat: Option<f64>,
    lon: Option<f64>,
    #[serde(default)]
    tags: HashMap<String, String>,
    #[serde(default)]
    geometry: Vec<Option<RawPt>>,
    #[serde(default)]
    members: Vec<RawMember>,
}

#[derive(Deserialize)]
struct RawResp {
    #[serde(default)]
    elements: Vec<RawEl>,
    #[serde(default)]
    remark: Option<String>,
}

fn pts(g: &[Option<RawPt>]) -> Vec<LatLon> {
    g.iter().flatten().map(|p| LatLon::new(p.lat, p.lon)).collect()
}

/// Składanie pierścieni z kawałków linii (członkowie relacji wielokąta).
pub fn assemble_rings(mut parts: Vec<Vec<LatLon>>) -> Vec<Vec<LatLon>> {
    let same = |a: LatLon, b: LatLon| (a.lat - b.lat).abs() < 1e-7 && (a.lon - b.lon).abs() < 1e-7;
    let mut rings = vec![];
    parts.retain(|p| p.len() >= 2);
    while let Some(mut cur) = parts.pop() {
        loop {
            if cur.len() > 3 && same(cur[0], *cur.last().unwrap()) {
                break;
            }
            let end = *cur.last().unwrap();
            let Some(i) = parts.iter().position(|p| same(p[0], end) || same(*p.last().unwrap(), end)) else { break };
            let mut next = parts.swap_remove(i);
            if !same(next[0], end) {
                next.reverse();
            }
            cur.extend(next.into_iter().skip(1));
        }
        if cur.len() >= 4 {
            if same(cur[0], *cur.last().unwrap()) {
                cur.pop();
            }
            rings.push(cur);
        }
    }
    rings
}

pub fn parse(json: &[u8]) -> Result<Vec<Element>, String> {
    let resp: RawResp = serde_json::from_slice(json).map_err(|e| format!("Odpowiedź Overpass: {e}"))?;
    if let Some(r) = &resp.remark {
        if r.contains("runtime error") || r.contains("timed out") || r.contains("out of memory") {
            return Err(format!("Overpass: {r}"));
        }
    }
    let mut out = Vec::with_capacity(resp.elements.len());
    for e in resp.elements {
        match e.t.as_str() {
            "node" => {
                if let (Some(lat), Some(lon)) = (e.lat, e.lon) {
                    out.push(Element { t: 'n', id: e.id, tags: e.tags, p: Some(LatLon::new(lat, lon)), ..Default::default() });
                }
            }
            "way" => {
                let line = pts(&e.geometry);
                if line.len() < 2 {
                    continue;
                }
                let closed = line.len() >= 4 && line[0] == *line.last().unwrap();
                // Zamknięta droga (rondo) albo tor to nadal linia, chyba że oznaczono area=yes.
                let tag = |k: &str| e.tags.get(k).map(String::as_str);
                let area_tag = tag("area") == Some("yes");
                let road = tag("highway").is_some() && !area_tag;
                let rail = tag("railway").map(|v| v != "platform").unwrap_or(false);
                let aero = matches!(tag("aeroway"), Some("runway") | Some("taxiway")) && !area_tag;
                let is_line_feature = road || rail || aero;
                let mut el = Element { t: 'w', id: e.id, tags: e.tags, ..Default::default() };
                if closed && !is_line_feature {
                    let mut ring = line.clone();
                    ring.pop();
                    el.outers = vec![ring];
                }
                el.line = line;
                out.push(el);
            }
            "relation" => {
                let mut outer = vec![];
                let mut inner = vec![];
                for m in &e.members {
                    if m.t != "way" {
                        continue;
                    }
                    let g = pts(&m.geometry);
                    if m.role == "inner" {
                        inner.push(g);
                    } else {
                        outer.push(g);
                    }
                }
                let outers = assemble_rings(outer);
                if outers.is_empty() {
                    continue;
                }
                out.push(Element { t: 'r', id: e.id, tags: e.tags, outers, inners: assemble_rings(inner), ..Default::default() });
            }
            _ => {}
        }
    }
    Ok(out)
}

/// Pobranie danych z Overpass (z zapasowymi serwerami).
pub fn fetch(custom: &str, b: &BBox, layers: &[String]) -> Result<(Vec<Element>, usize), String> {
    let query = build_query(b, layers);
    let mut servers: Vec<String> = vec![];
    if !custom.trim().is_empty() {
        servers.push(custom.trim().to_string());
    }
    servers.extend(OVERPASS.iter().map(|s| s.to_string()));
    let mut last = String::new();
    for s in servers {
        match net::post_form(&s, "data", &query) {
            Ok(body) => {
                let n = body.len();
                return parse(&body).map(|e| (e, n));
            }
            Err(e) => last = format!("{s}: {e}"),
        }
    }
    Err(format!("Overpass API nie odpowiada ({last})"))
}

/// Wysokość w metrach ze znaczników `height`, `est_height` itp. („35”, „35 m”, „120 ft”).
pub fn parse_height(v: &str) -> Option<f64> {
    let s = v.trim().replace(',', ".");
    let lower = s.to_lowercase();
    let (num, ft) = if let Some(x) = lower.strip_suffix("ft").or_else(|| lower.strip_suffix('\'')) {
        (x.trim().to_string(), true)
    } else {
        (lower.trim_end_matches('m').trim().to_string(), false)
    };
    let n: f64 = num.split([';', ' ']).next()?.parse().ok()?;
    let m = if ft { n * 0.3048 } else { n };
    (m.is_finite() && m > 0.0 && m < 1000.0).then_some(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heights() {
        assert_eq!(parse_height("35"), Some(35.0));
        assert_eq!(parse_height("35 m"), Some(35.0));
        assert!((parse_height("100 ft").unwrap() - 30.48).abs() < 1e-9);
        assert_eq!(parse_height("12,5"), Some(12.5));
        assert_eq!(parse_height("abc"), None);
    }

    #[test]
    fn query_and_parse() {
        let b = BBox { south: 50.0, west: 19.0, north: 50.1, east: 19.1 };
        let q = build_query(&b, &["lamps".into(), "roads".into()]);
        assert!(q.contains("street_lamp") && q.contains("highway"));
        let json = br#"{"elements":[
          {"type":"node","id":1,"lat":50.05,"lon":19.05,"tags":{"highway":"street_lamp"}},
          {"type":"way","id":2,"tags":{"highway":"residential","lit":"yes"},"geometry":[{"lat":50.0,"lon":19.0},{"lat":50.01,"lon":19.0}]},
          {"type":"way","id":3,"tags":{"amenity":"parking"},"geometry":[{"lat":50.0,"lon":19.0},{"lat":50.0,"lon":19.001},{"lat":50.001,"lon":19.001},{"lat":50.0,"lon":19.0}]},
          {"type":"relation","id":4,"tags":{"natural":"water"},"members":[
             {"type":"way","role":"outer","geometry":[{"lat":0.0,"lon":0.0},{"lat":0.0,"lon":1.0}]},
             {"type":"way","role":"outer","geometry":[{"lat":1.0,"lon":1.0},{"lat":0.0,"lon":1.0}]},
             {"type":"way","role":"outer","geometry":[{"lat":1.0,"lon":1.0},{"lat":0.0,"lon":0.0}]}]}
        ]}"#;
        let els = parse(json).unwrap();
        assert_eq!(els.len(), 4);
        assert!(els[0].p.is_some());
        assert!(!els[1].is_area());
        assert!(els[2].is_area());
        assert_eq!(els[3].outers.len(), 1);
        assert_eq!(els[3].outers[0].len(), 3);
    }
}
