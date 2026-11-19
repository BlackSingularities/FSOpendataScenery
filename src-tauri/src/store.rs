//! Trwałe dane aplikacji: ustawienia i projekty. Każdy plik zapisujemy atomowo
//! (plik tymczasowy + zmiana nazwy), żeby awaria w trakcie zapisu nie zostawiła połówki.

use crate::geo::{Area, LatLon};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub fn now() -> String {
    chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string()
}

pub fn new_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_string()
}

pub fn write_atomic(path: &Path, data: &[u8]) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, data).map_err(|e| format!("{}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Settings {
    pub language: String,
    pub theme: String,
    pub accent: String,
    pub auto_update_check: bool,
    /// Własna ścieżka MSFS 2024 SDK (pusta = wykrywanie: MSFS2024_SDK, C:\MSFS 2024 SDK…).
    pub sdk_path: String,
    /// Własny folder Community (pusty = z UserCfg.opt).
    pub community_path: String,
    /// Kompilacja: "auto" | "steam" | "store" (fspackagetool -forcesteam).
    pub build_store: String,
    /// Instalacja zbudowanego pakietu: "copy" albo "link" (łącze katalogu do folderu roboczego).
    pub install_mode: String,
    pub overpass_url: String,
    pub map_style: String,
    pub creator: String,
    pub last_project: String,
    pub onboarded: bool,
    /// Zachowanie plików źródłowych SDK po kompilacji (do ręcznej edycji w Developer Mode).
    pub keep_sources: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: "pl".into(),
            theme: "dark".into(),
            accent: "amber".into(),
            auto_update_check: true,
            sdk_path: String::new(),
            community_path: String::new(),
            build_store: "auto".into(),
            install_mode: "copy".into(),
            overpass_url: String::new(),
            map_style: "dark".into(),
            creator: String::new(),
            last_project: String::new(),
            onboarded: false,
            keep_sources: true,
        }
    }
}

/// Plik wysokościowy dodany przez użytkownika.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct DemFile {
    pub id: String,
    pub path: String,
    /// 0 = wykryj automatycznie (GeoTIFF) albo zgadnij (ASC/XYZ).
    pub epsg: u32,
    /// Plik XYZ z kolumnami w kolejności północ-wschód.
    pub swap_xy: bool,
    /// Przesunięcie pionowe tego pliku (np. różnica układów wysokości), m.
    pub offset: f64,
    pub enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TerrainCfg {
    pub enabled: bool,
    /// Kolejność źródeł: pierwsze, które ma dane w danym punkcie, wygrywa.
    /// "files", "gugik", "wcs", "terrarium", "copernicus30", "copernicus90".
    pub sources: Vec<String>,
    pub files: Vec<DemFile>,
    /// Docelowy krok siatki w metrach.
    pub resolution: f64,
    /// Liczba próbek na bok jednego prostokąta (≤ 256).
    pub tile: u32,
    pub falloff: f64,
    pub priority: i32,
    /// Wygładzanie (sigma rozmycia Gaussa w metrach, 0 = bez).
    pub smooth: f64,
    pub offset: f64,
    /// Spłaszczanie wód z OSM do jednego poziomu.
    pub hydro: bool,
    /// Wyrównanie terenu pod pasami startowymi z OSM.
    pub flatten_runways: bool,
    /// Pomijanie płaskich kafli, które nie dodają szczegółów.
    pub skip_flat: bool,
    pub skip_threshold: f64,
    /// Własna usługa WCS 1.0 (dla zaawansowanych).
    pub wcs_url: String,
    pub wcs_coverage: String,
    pub wcs_epsg: u32,
}

impl Default for TerrainCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            sources: vec!["files".into(), "gugik".into(), "terrarium".into()],
            files: vec![],
            resolution: 5.0,
            tile: 128,
            falloff: 100.0,
            priority: 0,
            smooth: 0.0,
            offset: 0.0,
            hydro: true,
            flatten_runways: true,
            skip_flat: false,
            skip_threshold: 0.25,
            wcs_url: String::new(),
            wcs_coverage: String::new(),
            wcs_epsg: 0,
        }
    }
}

/// Ustawienia jednej kategorii oświetlenia.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct LightCat {
    pub enabled: bool,
    /// Wysokość źródła światła nad terenem, m.
    pub height: f64,
    /// Odstęp między lampami, m (kategorie liniowe i powierzchniowe).
    pub spacing: f64,
    /// Temperatura barwowa, K.
    pub kelvin: f64,
    /// Natężenie światła w jednostkach MSFS (ASOBO_macro_light.intensity).
    pub intensity: f64,
    /// Kąt stożka, stopnie.
    pub cone: f64,
    /// Wygląd słupa: "cobra", "post_top", "double", "flood", "wall", "none".
    pub style: String,
    /// Długość wysięgnika, m.
    pub arm: f64,
    /// Odsunięcie słupa od krawędzi jezdni, m.
    pub offset: f64,
    /// Lampy po obu stronach (naprzemiennie) dla szerokich dróg.
    pub both_sides: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct LightsCfg {
    pub enabled: bool,
    pub categories: BTreeMap<String, LightCat>,
    /// Oświetlenie dróg w zabudowie, gdy w OSM brak znacznika `lit`.
    pub guess_urban: bool,
    /// Lampy wygenerowane bliżej niż tyle metrów od lampy z OSM są pomijane.
    pub avoid_mapped: f64,
    /// Odwrócenie kierunku wysięgnika (gdyby w symulatorze lampy patrzyły od jezdni).
    pub flip_arm: bool,
    /// Druga, uproszczona wersja modelu widoczna z daleka (tylko światło i świecąca oprawa).
    pub far_lod: bool,
}

impl Default for LightsCfg {
    fn default() -> Self {
        Self { enabled: true, categories: BTreeMap::new(), guess_urban: true, avoid_mapped: 12.0, flip_arm: false, far_lod: true }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ObstaclesCfg {
    pub enabled: bool,
    /// Minimalna wysokość obiektu, od której dodajemy światła, m.
    pub min_height: f64,
    pub masts: bool,
    pub chimneys: bool,
    pub towers: bool,
    pub turbines: bool,
    /// Prosty model turbiny (wieża, gondola, łopaty) tam, gdzie symulator jej nie ma.
    pub turbine_models: bool,
    pub power_towers: bool,
    pub cranes: bool,
    pub lighthouses: bool,
    /// Wysokość domyślna, gdy w OSM brak `height`, m (0 = pomiń taki obiekt).
    pub default_height: f64,
    pub intensity: f64,
    /// Światła średniej intensywności migają (obiekty ≥ 45 m), niższe świecą stale.
    pub flashing: bool,
    /// Pośrednie poziomy świateł co ~45 m na wysokich konstrukcjach.
    pub intermediate: bool,
}

impl Default for ObstaclesCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            min_height: 45.0,
            masts: true,
            chimneys: true,
            towers: true,
            turbines: true,
            turbine_models: false,
            power_towers: false,
            cranes: false,
            lighthouses: true,
            default_height: 0.0,
            intensity: 12.0,
            flashing: true,
            intermediate: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VegetationCfg {
    pub enabled: bool,
    pub roads: bool,
    pub railways: bool,
    pub water: bool,
    pub runways: bool,
    pub pitches: bool,
    pub parking: bool,
    pub landuse: bool,
    /// Dodatkowy margines wokół dróg i torów, m.
    pub margin: f64,
}

impl Default for VegetationCfg {
    fn default() -> Self {
        Self { enabled: false, roads: true, railways: true, water: true, runways: true, pitches: true, parking: true, landuse: false, margin: 2.0 }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct PackageCfg {
    /// Nazwa folderu w Community, np. „redzik-scenery-krakow-lights”.
    pub name: String,
    pub title: String,
    pub creator: String,
    pub version: String,
    pub description: String,
}

/// Lampa postawiona ręcznie na mapie.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct ManualLight {
    pub lat: f64,
    pub lon: f64,
    pub heading: f64,
    pub category: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub note: String,
    pub created: String,
    pub updated: String,
    pub area: Area,
    pub terrain: TerrainCfg,
    pub lights: LightsCfg,
    pub obstacles: ObstaclesCfg,
    pub vegetation: VegetationCfg,
    pub package: PackageCfg,
    pub manual: Vec<ManualLight>,
    /// Lampy z OSM wykluczone ręcznie (identyfikatory węzłów).
    pub excluded: Vec<i64>,
    /// Ostatni wynik etapów – do pokazania na pulpicie.
    pub status: BTreeMap<String, String>,
}

impl Project {
    pub fn center(&self) -> LatLon {
        self.area.bbox().center()
    }
}

/// Katalog danych: settings.json, projects/<id>/project.json + wyniki etapów.
pub struct Store {
    pub dir: PathBuf,
}

impl Store {
    pub fn new(dir: PathBuf) -> Self {
        let _ = std::fs::create_dir_all(dir.join("projects"));
        Self { dir }
    }

    pub fn load<T: DeserializeOwned + Default>(&self, name: &str) -> T {
        read_json(&self.dir.join(name)).unwrap_or_default()
    }

    pub fn save<T: Serialize>(&self, name: &str, value: &T) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(value).map_err(|e| e.to_string())?;
        write_atomic(&self.dir.join(name), &data)
    }

    pub fn settings(&self) -> Settings {
        self.load("settings.json")
    }

    pub fn project_dir(&self, id: &str) -> PathBuf {
        // Identyfikator pochodzi z interfejsu – nie pozwalamy wyjść poza folder projektów.
        let safe: String = id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
        self.dir.join("projects").join(safe)
    }

    pub fn projects(&self) -> Vec<Project> {
        let mut out = vec![];
        let Ok(rd) = std::fs::read_dir(self.dir.join("projects")) else { return out };
        for e in rd.flatten() {
            if let Some(p) = read_json::<Project>(&e.path().join("project.json")) {
                out.push(p);
            }
        }
        out.sort_by(|a, b| b.updated.cmp(&a.updated));
        out
    }

    pub fn project(&self, id: &str) -> Result<Project, String> {
        read_json(&self.project_dir(id).join("project.json")).ok_or_else(|| "Nie znaleziono projektu".to_string())
    }

    pub fn save_project(&self, p: &Project) -> Result<(), String> {
        let data = serde_json::to_vec_pretty(p).map_err(|e| e.to_string())?;
        write_atomic(&self.project_dir(&p.id).join("project.json"), &data)
    }

    pub fn delete_project(&self, id: &str) -> Result<(), String> {
        let dir = self.project_dir(id);
        if dir.is_dir() && dir != self.dir.join("projects") {
            std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

pub fn read_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let data = std::fs::read(path).ok()?;
    serde_json::from_slice(&data).ok()
}

/// Rozmiar folderu w bajtach.
pub fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path).into_iter().filter_map(Result::ok).filter_map(|e| e.metadata().ok()).filter(|m| m.is_file()).map(|m| m.len()).sum()
}

/// Bezpieczna nazwa pakietu MSFS: małe litery, cyfry i myślniki.
pub fn package_slug(s: &str) -> String {
    let mut out = String::new();
    for c in s.to_lowercase().chars() {
        let c = match c {
            'ą' => 'a',
            'ć' => 'c',
            'ę' => 'e',
            'ł' => 'l',
            'ń' => 'n',
            'ó' => 'o',
            'ś' => 's',
            'ź' | 'ż' => 'z',
            'ä' => 'a',
            'ö' => 'o',
            'ü' => 'u',
            'é' | 'è' | 'ê' => 'e',
            c => c,
        };
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_matches('-').chars().take(60).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug() {
        assert_eq!(package_slug("Kraków – Łagiewniki, oświetlenie!"), "krakow-lagiewniki-oswietlenie");
        assert_eq!(package_slug("  --A  b--"), "a-b");
    }

    #[test]
    fn project_roundtrip() {
        let dir = std::env::temp_dir().join(format!("fsos-store-{}", new_id()));
        let st = Store::new(dir.clone());
        let mut p = Project { id: "abc".into(), name: "Test".into(), ..Default::default() };
        p.area.outer = vec![LatLon::new(50.0, 19.0), LatLon::new(50.0, 19.1), LatLon::new(50.1, 19.1)];
        st.save_project(&p).unwrap();
        let q = st.project("abc").unwrap();
        assert_eq!(q.name, "Test");
        assert_eq!(q.area.outer.len(), 3);
        assert!(q.terrain.enabled);
        assert_eq!(st.projects().len(), 1);
        st.delete_project("abc").unwrap();
        assert!(st.projects().is_empty());
        assert_eq!(st.project_dir("../../x"), dir.join("projects").join("x"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
