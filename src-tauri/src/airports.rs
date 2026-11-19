//! Wbudowana baza lotnisk (OurAirports, domena publiczna) – szybkie ustawianie obszaru
//! projektu wokół lotniska po kodzie ICAO, nazwie albo mieście.

use once_cell::sync::Lazy;
use serde::Serialize;
use std::collections::HashMap;
use std::io::Read;

static RAW: &[u8] = include_bytes!("../data/airports.tsv.gz");

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Airport {
    pub icao: String,
    /// L – duże, M – średnie, S – małe, H – lądowisko dla śmigłowców, W – wodne.
    pub kind: String,
    pub lat: f64,
    pub lon: f64,
    pub country: String,
    pub name: String,
    pub city: String,
}

static DB: Lazy<HashMap<String, Airport>> = Lazy::new(|| {
    let mut text = String::new();
    let _ = flate2::read::GzDecoder::new(RAW).read_to_string(&mut text);
    let mut map = HashMap::with_capacity(80_000);
    for line in text.lines() {
        let f: Vec<&str> = line.split('\t').collect();
        if f.len() < 7 {
            continue;
        }
        let (Ok(lat), Ok(lon)) = (f[2].parse(), f[3].parse()) else { continue };
        map.insert(
            f[0].to_string(),
            Airport { icao: f[0].into(), kind: f[1].into(), lat, lon, country: f[4].into(), name: f[5].into(), city: f[6].into() },
        );
    }
    map
});

pub fn get(icao: &str) -> Option<&'static Airport> {
    DB.get(&icao.trim().to_uppercase())
}

pub fn count() -> usize {
    DB.len()
}

/// Wyszukiwanie po kodzie, nazwie albo mieście (planowanie lotu).
pub fn search(query: &str, limit: usize) -> Vec<Airport> {
    let q = query.trim().to_uppercase();
    if q.len() < 2 {
        return vec![];
    }
    if let Some(a) = get(&q) {
        return vec![a.clone()];
    }
    let mut hits: Vec<&Airport> = DB
        .values()
        .filter(|a| a.icao.starts_with(&q) || a.name.to_uppercase().contains(&q) || a.city.to_uppercase().contains(&q))
        .collect();
    hits.sort_by_key(|a| (!a.icao.starts_with(&q), "LMSWH".find(&a.kind).unwrap_or(9), a.icao.len(), a.icao.clone()));
    hits.into_iter().take(limit).cloned().collect()
}

/// Odległość po wielkim kole w kilometrach.
#[cfg(test)]
pub fn distance_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    6371.0 * 2.0 * a.sqrt().atan2((1.0 - a).sqrt())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn database_loads() {
        assert!(count() > 40_000);
        let wa = get("epwa").unwrap();
        assert_eq!(wa.country, "PL");
        assert!((wa.lat - 52.16).abs() < 0.1);
    }

    #[test]
    fn distance() {
        let d = distance_km(52.1657, 20.9671, 50.0777, 19.7848);
        assert!((d - 250.0).abs() < 20.0);
        assert_eq!(search("EPKK", 5)[0].icao, "EPKK");
    }
}
