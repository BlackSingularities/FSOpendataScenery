//! Obszar projektu: import z plików (KML, GeoJSON, GPX, WKT), okrąg wokół punktu
//! i wyszukiwanie miejsc w Nominatim (OpenStreetMap).

use crate::geo::{self, Area, LatLon};
use serde::Serialize;

/// Okrąg (wielokąt 48-boczny) o promieniu `radius_m`.
pub fn circle(c: LatLon, radius_m: f64) -> Area {
    let outer = (0..48).map(|k| geo::destination(c, k as f64 * 7.5, radius_m)).collect();
    Area { outer, holes: vec![] }
}

fn parse_coords_kml(s: &str) -> Vec<LatLon> {
    s.split_whitespace()
        .filter_map(|t| {
            let mut it = t.split(',');
            let lon: f64 = it.next()?.parse().ok()?;
            let lat: f64 = it.next()?.parse().ok()?;
            Some(LatLon::new(lat, lon))
        })
        .collect()
}

fn close_ring(mut r: Vec<LatLon>) -> Vec<LatLon> {
    if r.len() > 3 && r.first() == r.last() {
        r.pop();
    }
    r
}

/// Największy wielokąt z pliku (albo obrys linii/punktów, gdy brak wielokątów).
pub fn import(path: &std::path::Path) -> Result<Area, String> {
    let text = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let ext = path.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let mut rings: Vec<Vec<LatLon>> = vec![];
    let mut points: Vec<LatLon> = vec![];
    match ext.as_str() {
        "kml" => {
            let doc = roxmltree::Document::parse(&text).map_err(|e| format!("KML: {e}"))?;
            for n in doc.descendants().filter(|n| n.has_tag_name("Polygon")) {
                if let Some(c) = n.descendants().find(|d| d.has_tag_name("outerBoundaryIs")).and_then(|o| o.descendants().find(|d| d.has_tag_name("coordinates"))) {
                    rings.push(close_ring(parse_coords_kml(c.text().unwrap_or(""))));
                }
            }
            for c in doc.descendants().filter(|n| n.has_tag_name("coordinates")) {
                points.extend(parse_coords_kml(c.text().unwrap_or("")));
            }
        }
        "gpx" => {
            let doc = roxmltree::Document::parse(&text).map_err(|e| format!("GPX: {e}"))?;
            for n in doc.descendants().filter(|n| n.has_tag_name("trkpt") || n.has_tag_name("rtept") || n.has_tag_name("wpt")) {
                if let (Some(a), Some(o)) = (n.attribute("lat").and_then(|v| v.parse().ok()), n.attribute("lon").and_then(|v| v.parse().ok())) {
                    points.push(LatLon::new(a, o));
                }
            }
        }
        "wkt" | "txt" => {
            let up = text.to_uppercase();
            let start = up.find("((").ok_or("WKT: brak wielokąta")?;
            let body = &text[start + 2..];
            let end = body.find(')').ok_or("WKT: niepełny wielokąt")?;
            let ring = body[..end]
                .split(',')
                .filter_map(|p| {
                    let mut it = p.split_whitespace();
                    let lon: f64 = it.next()?.parse().ok()?;
                    let lat: f64 = it.next()?.parse().ok()?;
                    Some(LatLon::new(lat, lon))
                })
                .collect();
            rings.push(close_ring(ring));
        }
        _ => {
            let v: serde_json::Value = serde_json::from_str(&text).map_err(|e| format!("GeoJSON: {e}"))?;
            collect_geojson(&v, &mut rings, &mut points);
        }
    }
    let best = rings.into_iter().filter(|r| r.len() >= 3).max_by(|a, b| geo::polygon_area(a).partial_cmp(&geo::polygon_area(b)).unwrap_or(std::cmp::Ordering::Equal));
    if let Some(r) = best {
        return Ok(Area { outer: r, holes: vec![] });
    }
    if points.len() >= 2 {
        // Ślad trasy: prostokąt z marginesem 1 km.
        let b = geo::BBox::of(&points).grow_m(1000.0);
        return Ok(Area { outer: vec![LatLon::new(b.south, b.west), LatLon::new(b.south, b.east), LatLon::new(b.north, b.east), LatLon::new(b.north, b.west)], holes: vec![] });
    }
    if let Some(p) = points.first() {
        return Ok(circle(*p, 2000.0));
    }
    Err("W pliku nie ma wielokąta ani punktów".into())
}

fn collect_geojson(v: &serde_json::Value, rings: &mut Vec<Vec<LatLon>>, points: &mut Vec<LatLon>) {
    let ring_of = |arr: &serde_json::Value| -> Vec<LatLon> {
        close_ring(arr.as_array().map(|a| a.iter().filter_map(|p| Some(LatLon::new(p.get(1)?.as_f64()?, p.get(0)?.as_f64()?))).collect()).unwrap_or_default())
    };
    match v.get("type").and_then(|t| t.as_str()) {
        Some("FeatureCollection") => {
            for f in v.get("features").and_then(|f| f.as_array()).into_iter().flatten() {
                collect_geojson(f, rings, points);
            }
        }
        Some("Feature") => {
            if let Some(g) = v.get("geometry") {
                collect_geojson(g, rings, points);
            }
        }
        Some("Polygon") => {
            if let Some(r) = v.get("coordinates").and_then(|c| c.get(0)) {
                rings.push(ring_of(r));
            }
        }
        Some("MultiPolygon") => {
            for poly in v.get("coordinates").and_then(|c| c.as_array()).into_iter().flatten() {
                if let Some(r) = poly.get(0) {
                    rings.push(ring_of(r));
                }
            }
        }
        Some("LineString") | Some("MultiPoint") => {
            points.extend(ring_of(v.get("coordinates").unwrap_or(&serde_json::Value::Null)));
        }
        Some("Point") => {
            if let Some(c) = v.get("coordinates") {
                if let (Some(lon), Some(lat)) = (c.get(0).and_then(|x| x.as_f64()), c.get(1).and_then(|x| x.as_f64())) {
                    points.push(LatLon::new(lat, lon));
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Place {
    pub name: String,
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    /// Obrys miejscowości (jeśli Nominatim go zwrócił).
    pub area: Option<Area>,
    pub bbox: Option<[f64; 4]>,
}

/// Wyszukiwanie miejsc w Nominatim (z obrysem administracyjnym, gdy jest).
pub fn search_places(q: &str, lang: &str) -> Result<Vec<Place>, String> {
    let url = format!(
        "https://nominatim.openstreetmap.org/search?format=jsonv2&limit=8&polygon_geojson=1&polygon_threshold=0.0005&accept-language={}&q={}",
        urlencoding::encode(lang),
        urlencoding::encode(q)
    );
    let body = match crate::net::get(&url)? {
        crate::net::Fetched::Ok(b) => b,
        crate::net::Fetched::Missing => return Ok(vec![]),
    };
    let v: serde_json::Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
    let mut out = vec![];
    for r in v.as_array().into_iter().flatten() {
        let lat = r.get("lat").and_then(|x| x.as_str()).and_then(|x| x.parse().ok());
        let lon = r.get("lon").and_then(|x| x.as_str()).and_then(|x| x.parse().ok());
        let (Some(lat), Some(lon)) = (lat, lon) else { continue };
        let mut rings = vec![];
        let mut pts = vec![];
        if let Some(g) = r.get("geojson") {
            collect_geojson(g, &mut rings, &mut pts);
        }
        let area = rings.into_iter().filter(|r| r.len() >= 3).max_by(|a, b| geo::polygon_area(a).partial_cmp(&geo::polygon_area(b)).unwrap_or(std::cmp::Ordering::Equal)).map(|r| Area { outer: geo::simplify(&r, 15.0), holes: vec![] });
        let bbox = r.get("boundingbox").and_then(|b| b.as_array()).and_then(|b| {
            let f: Vec<f64> = b.iter().filter_map(|x| x.as_str().and_then(|s| s.parse().ok())).collect();
            (f.len() == 4).then(|| [f[0], f[2], f[1], f[3]])
        });
        out.push(Place {
            name: r.get("display_name").and_then(|x| x.as_str()).unwrap_or("").to_string(),
            kind: format!("{}/{}", r.get("category").and_then(|x| x.as_str()).unwrap_or(""), r.get("type").and_then(|x| x.as_str()).unwrap_or("")),
            lat,
            lon,
            area,
            bbox,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports() {
        let dir = std::env::temp_dir().join(format!("fsos-area-{}", crate::store::new_id()));
        std::fs::create_dir_all(&dir).unwrap();
        let kml = dir.join("a.kml");
        std::fs::write(&kml, r#"<kml><Placemark><Polygon><outerBoundaryIs><LinearRing><coordinates>19,50,0 19.1,50,0 19.1,50.1,0 19,50,0</coordinates></LinearRing></outerBoundaryIs></Polygon></Placemark></kml>"#).unwrap();
        assert_eq!(import(&kml).unwrap().outer.len(), 3);
        let gj = dir.join("a.geojson");
        std::fs::write(&gj, r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Polygon","coordinates":[[[19,50],[19.2,50],[19.2,50.2],[19,50.2],[19,50]]]}}]}"#).unwrap();
        assert_eq!(import(&gj).unwrap().outer.len(), 4);
        let wkt = dir.join("a.wkt");
        std::fs::write(&wkt, "POLYGON ((19 50, 19.1 50, 19.1 50.1, 19 50))").unwrap();
        assert_eq!(import(&wkt).unwrap().outer.len(), 3);
        let c = circle(LatLon::new(50.0, 19.0), 1000.0);
        assert!((c.area_m2() - std::f64::consts::PI * 1e6).abs() < 2e4);
        let _ = std::fs::remove_dir_all(dir);
    }
}
