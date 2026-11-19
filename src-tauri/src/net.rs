//! Klient HTTP dla zadań w tle (pobieranie danych wysokościowych, OSM, wyszukiwanie miejsc).
//! Jeden klient na proces, przedstawiający się nazwą aplikacji – tak wymagają zasady
//! korzystania z Overpass API i Nominatim.

use once_cell::sync::Lazy;
use std::time::Duration;

pub const USER_AGENT: &str = concat!("FSOpendataScenery/", env!("CARGO_PKG_VERSION"), " (+https://redzikowski.com)");

static CLIENT: Lazy<reqwest::blocking::Client> = Lazy::new(|| {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(300))
        .gzip(true)
        .build()
        .expect("klient HTTP")
});

pub fn client() -> &'static reqwest::blocking::Client {
    &CLIENT
}

pub enum Fetched {
    Ok(Vec<u8>),
    /// Brak danych pod tym adresem (404) – np. kafel Copernicus nad oceanem.
    Missing,
}

/// GET z ponowieniem przy błędach sieci i odpowiedziach 429/5xx.
pub fn get(url: &str) -> Result<Fetched, String> {
    let mut last = String::new();
    for attempt in 0..4u32 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_millis(800 * 2u64.pow(attempt)));
        }
        match client().get(url).send() {
            Ok(r) => {
                let status = r.status();
                // S3 odpowiada 403 zamiast 404 dla nieistniejących obiektów.
                if status.as_u16() == 404 || (status.as_u16() == 403 && url.contains("amazonaws.com")) {
                    return Ok(Fetched::Missing);
                }
                if status.is_success() {
                    return r.bytes().map(|b| Fetched::Ok(b.to_vec())).map_err(|e| e.to_string());
                }
                last = format!("HTTP {status}");
                if !(status.as_u16() == 429 || status.is_server_error()) {
                    break;
                }
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(format!("{last} – {}", short(url)))
}

/// POST formularza (Overpass) z ponowieniem.
pub fn post_form(url: &str, field: &str, value: &str) -> Result<Vec<u8>, String> {
    let mut last = String::new();
    // Dwie próby na serwer – przy przeciążeniu szybciej przechodzimy na zapasowy.
    for attempt in 0..2u32 {
        if attempt > 0 {
            std::thread::sleep(Duration::from_secs(5));
        }
        match client().post(url).form(&[(field, value)]).timeout(Duration::from_secs(240)).send() {
            Ok(r) => {
                let status = r.status();
                if status.is_success() {
                    return r.bytes().map(|b| b.to_vec()).map_err(|e| e.to_string());
                }
                last = format!("HTTP {status}");
                if !(status.as_u16() == 429 || status.as_u16() == 504 || status.is_server_error()) {
                    break;
                }
            }
            Err(e) => last = e.to_string(),
        }
    }
    Err(last)
}

fn short(url: &str) -> String {
    url.split('?').next().unwrap_or(url).chars().take(120).collect()
}
