//! FS Opendata Scenery – polecenia dla interfejsu, zadania w tle i stan aplikacji.
//!
//! Etapy pracy nad projektem (każdy jako zadanie w tle z postępem i możliwością przerwania):
//!   osm      – pobranie danych OpenStreetMap dla włączonych warstw,
//!   dem      – pobranie danych wysokościowych do pamięci podręcznej,
//!   terrain  – policzenie siatki prostokątów z heightmapą i podglądu,
//!   lights   – rozmieszczenie świateł i przeszkód,
//!   build    – zapis projektu SDK, kompilacja fspackagetool, opcjonalnie instalacja,
//!   all      – wszystko po kolei.

mod airports;
mod area;
mod dem;
mod geo;
mod lights;
mod models;
mod net;
mod osm;
mod package;
mod proj;
mod sim;
mod store;
mod terrain;
mod update;
mod vegetation;

use geo::{Area, LatLon};
use rayon::prelude::*;
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use store::{Project, Settings, Store};
use tauri::{AppHandle, Emitter, Manager, State};

struct AppState {
    store: Store,
    cache: PathBuf,
    jobs: Mutex<HashMap<String, Arc<AtomicBool>>>,
    busy: Mutex<HashSet<String>>,
    /// PID uruchomionego fspackagetool (do przerwania kompilacji).
    child: Mutex<Option<u32>>,
}

type St<'a> = State<'a, AppState>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

// ───────────────────────────── Informacje i ustawienia ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    install_kind: String,
    data_dir: String,
    cache_dir: String,
    airports: usize,
    crs: Vec<(u32, &'static str)>,
}

#[tauri::command]
fn app_info(state: St) -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").into(),
        install_kind: update::install_kind().into(),
        data_dir: state.store.dir.to_string_lossy().into_owned(),
        cache_dir: state.cache.to_string_lossy().into_owned(),
        airports: airports::count(),
        crs: proj::supported(),
    }
}

#[tauri::command]
fn get_settings(state: St) -> Settings {
    state.store.settings()
}

#[tauri::command]
fn save_settings(state: St, settings: Settings) -> Result<Settings, String> {
    state.store.save("settings.json", &settings)?;
    Ok(settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EnvStatus {
    installs: Vec<sim::Installation>,
    community: Option<String>,
    community_ok: bool,
    sdk: Option<sim::Sdk>,
    sim_running: bool,
}

#[tauri::command]
async fn env_status(state: St<'_>) -> Result<EnvStatus, String> {
    let s = state.store.settings();
    let community = sim::community(&s.community_path);
    let sdk_path = s.sdk_path.clone();
    let (installs, sdk, running) = tauri::async_runtime::spawn_blocking(move || (sim::detect(), sim::find_sdk(&sdk_path), sim::is_running())).await.map_err(|e| e.to_string())?;
    Ok(EnvStatus {
        installs,
        community_ok: community.as_ref().map(|c| c.is_dir()).unwrap_or(false),
        community: community.map(|c| c.to_string_lossy().into_owned()),
        sdk,
        sim_running: running,
    })
}

// ───────────────────────────── Projekty ─────────────────────────────

fn normalize(mut p: Project) -> Project {
    lights::merge_defaults(&mut p.lights);
    if p.package.version.is_empty() {
        p.package.version = "1.0.0".into();
    }
    p.terrain.tile = p.terrain.tile.clamp(17, 256);
    p
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectCard {
    id: String,
    name: String,
    note: String,
    updated: String,
    area_km2: f64,
    center: LatLon,
    outer: Vec<LatLon>,
    status: BTreeMap<String, String>,
    package: String,
}

#[tauri::command]
fn list_projects(state: St) -> Vec<ProjectCard> {
    state
        .store
        .projects()
        .into_iter()
        .map(|p| ProjectCard {
            area_km2: p.area.area_m2() / 1e6,
            center: if p.area.outer.is_empty() { LatLon::default() } else { p.center() },
            outer: geo::simplify(&p.area.outer, 30.0),
            package: package::package_name(&p),
            id: p.id,
            name: p.name,
            note: p.note,
            updated: p.updated,
            status: p.status,
        })
        .collect()
}

#[tauri::command]
fn get_project(state: St, id: String) -> Result<Project, String> {
    state.store.project(&id).map(normalize)
}

#[tauri::command]
fn create_project(state: St, name: String, area: Option<Area>) -> Result<Project, String> {
    let s = state.store.settings();
    let mut p = Project { id: store::new_id(), name: if name.trim().is_empty() { "Nowy projekt".into() } else { name.trim().into() }, created: store::now(), updated: store::now(), ..Default::default() };
    if let Some(a) = area {
        p.area = a;
    }
    p.package.creator = s.creator.clone();
    p.package.version = "1.0.0".into();
    p.package.title = p.name.clone();
    p.package.name = format!("fsos-{}", store::package_slug(&p.name));
    apply_regional_defaults(&mut p);
    let p = normalize(p);
    state.store.save_project(&p)?;
    let mut s = s;
    s.last_project = p.id.clone();
    let _ = state.store.save("settings.json", &s);
    Ok(p)
}

/// Domyślne źródła i rozdzielczość zależnie od położenia (w Polsce NMT GUGiK 1 m).
fn apply_regional_defaults(p: &mut Project) {
    if p.area.outer.is_empty() {
        return;
    }
    let poland = dem::POLAND.contains(p.center());
    p.terrain.sources = if poland { vec!["files".into(), "gugik".into(), "terrarium".into()] } else { vec!["files".into(), "terrarium".into()] };
    p.terrain.resolution = if poland { 3.0 } else { 10.0 };
}

#[tauri::command]
fn save_project(state: St, project: Project) -> Result<Project, String> {
    let mut p = normalize(project);
    if p.id.is_empty() {
        return Err("Brak identyfikatora projektu".into());
    }
    let had_area = state.store.project(&p.id).map(|old| !old.area.outer.is_empty()).unwrap_or(false);
    if !had_area && !p.area.outer.is_empty() {
        apply_regional_defaults(&mut p);
    }
    p.updated = store::now();
    state.store.save_project(&p)?;
    Ok(p)
}

#[tauri::command]
fn delete_project(state: St, id: String) -> Result<(), String> {
    if lock(&state.busy).contains(&id) {
        return Err("Projekt jest w trakcie pracy – najpierw przerwij zadanie".into());
    }
    state.store.delete_project(&id)
}

#[tauri::command]
fn duplicate_project(state: St, id: String) -> Result<Project, String> {
    let mut p = state.store.project(&id)?;
    p.id = store::new_id();
    p.name = format!("{} (kopia)", p.name);
    p.package.name = format!("{}-kopia", p.package.name);
    p.created = store::now();
    p.updated = store::now();
    p.status.clear();
    state.store.save_project(&p)?;
    Ok(normalize(p))
}

#[tauri::command]
fn export_project(state: St, id: String, path: String) -> Result<(), String> {
    let p = state.store.project(&id)?;
    let data = serde_json::to_vec_pretty(&json!({"fsosProject": 1, "project": p})).map_err(|e| e.to_string())?;
    store::write_atomic(Path::new(&path), &data)
}

#[tauri::command]
fn import_project(state: St, path: String) -> Result<Project, String> {
    let v: Value = store::read_json(Path::new(&path)).ok_or("To nie jest plik projektu FS Opendata Scenery")?;
    let mut p: Project = serde_json::from_value(v.get("project").cloned().unwrap_or(v)).map_err(|e| format!("Plik projektu: {e}"))?;
    p.id = store::new_id();
    p.status.clear();
    p.updated = store::now();
    // Pliki DEM z innego komputera mogą nie istnieć – zostawiamy je wyłączone.
    for f in p.terrain.files.iter_mut() {
        if !Path::new(&f.path).exists() {
            f.enabled = false;
        }
    }
    let p = normalize(p);
    state.store.save_project(&p)?;
    Ok(p)
}

// ───────────────────────────── Obszar ─────────────────────────────

#[tauri::command]
async fn search_places(query: String, lang: String) -> Result<Vec<area::Place>, String> {
    tauri::async_runtime::spawn_blocking(move || area::search_places(&query, &lang)).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn search_airports(query: String) -> Vec<airports::Airport> {
    airports::search(&query, 12)
}

#[tauri::command]
fn area_import(path: String) -> Result<Area, String> {
    area::import(Path::new(&path))
}

#[tauri::command]
fn area_circle(lat: f64, lon: f64, radius: f64) -> Area {
    area::circle(LatLon::new(lat, lon), radius.clamp(100.0, 100_000.0))
}

// ───────────────────────────── Dane wysokościowe ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DemInfo {
    footprint: geo::BBox,
    w: usize,
    h: usize,
    epsg: u32,
    resolution: f64,
    min: f32,
    max: f32,
    valid: f64,
}

#[tauri::command]
async fn dem_file_info(path: String, epsg: u32, swap_xy: bool) -> Result<DemInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let r = dem::read_file(Path::new(&path), epsg, swap_xy)?;
        let (min, max, n) = r.stats();
        Ok(DemInfo { footprint: r.footprint(), w: r.w, h: r.h, epsg: r.epsg, resolution: r.resolution_m(), min, max, valid: n as f64 / (r.w * r.h) as f64 })
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn terrain_plan(project: Project) -> terrain::Plan {
    terrain::plan(&project.terrain, &project.area)
}

// ───────────────────────────── Zadania w tle ─────────────────────────────

#[derive(Serialize, Clone)]
#[serde(rename_all = "camelCase")]
struct JobEvent {
    id: String,
    project: String,
    kind: String,
    stage: String,
    done: usize,
    total: usize,
    message: String,
    state: String,
    result: Option<Value>,
}

struct Ctx {
    app: AppHandle,
    id: String,
    project: String,
    kind: String,
    cancel: Arc<AtomicBool>,
}

impl Ctx {
    fn emit(&self, stage: &str, done: usize, total: usize, message: &str, state: &str, result: Option<Value>) {
        let _ = self.app.emit(
            "job",
            JobEvent { id: self.id.clone(), project: self.project.clone(), kind: self.kind.clone(), stage: stage.into(), done, total, message: message.into(), state: state.into(), result },
        );
    }
    fn progress(&self, stage: &str, done: usize, total: usize, message: &str) {
        self.emit(stage, done, total, message, "running", None);
    }
    fn log(&self, line: &str) {
        let _ = self.app.emit("build-log", json!({"id": self.id, "project": self.project, "line": line}));
    }
    fn check(&self) -> Result<(), String> {
        if self.cancel.load(Ordering::Relaxed) {
            Err("Przerwano".into())
        } else {
            Ok(())
        }
    }
    fn state(&self) -> tauri::State<'_, AppState> {
        self.app.state::<AppState>()
    }
}

#[tauri::command]
fn start_job(app: AppHandle, state: St, project: String, kind: String, install: Option<bool>) -> Result<String, String> {
    if !matches!(kind.as_str(), "osm" | "dem" | "terrain" | "lights" | "build" | "testlights" | "all") {
        return Err("Nieznane zadanie".into());
    }
    state.store.project(&project)?;
    if !lock(&state.busy).insert(project.clone()) {
        return Err("Ten projekt już ma uruchomione zadanie".into());
    }
    let id = store::new_id();
    let cancel = Arc::new(AtomicBool::new(false));
    lock(&state.jobs).insert(id.clone(), cancel.clone());
    let ctx = Ctx { app: app.clone(), id: id.clone(), project: project.clone(), kind: kind.clone(), cancel };
    let install = install.unwrap_or(false);
    std::thread::spawn(move || {
        ctx.progress("start", 0, 0, "");
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run_job(&ctx, install))).unwrap_or_else(|p| {
            let msg = p.downcast_ref::<String>().cloned().or_else(|| p.downcast_ref::<&str>().map(|s| s.to_string())).unwrap_or_default();
            Err(format!("Błąd wewnętrzny: {msg}"))
        });
        let st = ctx.state();
        lock(&st.busy).remove(&ctx.project);
        lock(&st.jobs).remove(&ctx.id);
        match r {
            Ok(v) => ctx.emit("done", 1, 1, "", "done", Some(v)),
            Err(e) if ctx.cancel.load(Ordering::Relaxed) => ctx.emit("done", 0, 0, &e, "cancelled", None),
            Err(e) => ctx.emit("done", 0, 0, &e, "error", None),
        }
    });
    Ok(id)
}

#[tauri::command]
fn cancel_job(state: St, id: String) {
    if let Some(c) = lock(&state.jobs).get(&id) {
        c.store(true, Ordering::Relaxed);
    }
    if let Some(pid) = lock(&state.child).take() {
        kill_tree(pid);
    }
}

fn kill_tree(pid: u32) {
    let mut cmd = std::process::Command::new("taskkill");
    cmd.args(["/PID", &pid.to_string(), "/T", "/F"]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    let _ = cmd.output();
}

fn run_job(ctx: &Ctx, install: bool) -> Result<Value, String> {
    match ctx.kind.as_str() {
        "osm" => stage_osm(ctx),
        "dem" => stage_dem(ctx),
        "terrain" => stage_terrain(ctx),
        "lights" => stage_lights(ctx),
        "build" => stage_build(ctx, install),
        "testlights" => stage_testlights(ctx, install),
        _ => {
            let p = ctx.state().store.project(&ctx.project)?;
            let mut out = json!({});
            if needs_osm(&p) {
                out["osm"] = stage_osm(ctx)?;
            }
            if p.terrain.enabled {
                out["dem"] = stage_dem(ctx)?;
                out["terrain"] = stage_terrain(ctx)?;
            }
            if p.lights.enabled || p.obstacles.enabled {
                out["lights"] = stage_lights(ctx)?;
            }
            out["build"] = stage_build(ctx, install)?;
            Ok(out)
        }
    }
}

fn set_status(ctx: &Ctx, key: &str, value: String) {
    let st = ctx.state();
    if let Ok(mut p) = st.store.project(&ctx.project) {
        p.status.insert(key.into(), value);
        let _ = st.store.save_project(&p);
    }
}

fn require_area(p: &Project) -> Result<(), String> {
    if p.area.outer.len() < 3 {
        return Err("Najpierw wyznacz obszar projektu na mapie".into());
    }
    Ok(())
}

// ── OSM ──

fn osm_layers(p: &Project) -> Vec<String> {
    let mut l: Vec<String> = vec![];
    if p.lights.enabled {
        l.extend(["lamps", "roads"].map(String::from));
        let c = |k: &str| p.lights.categories.get(k).map(|c| c.enabled).unwrap_or(true);
        if p.lights.guess_urban {
            l.push("urban".into());
        }
        for (cat, layer) in [("parking", "parking"), ("pitches", "pitches"), ("platforms", "platforms"), ("fuel", "fuel"), ("piers", "piers")] {
            if c(cat) {
                l.push(layer.into());
            }
        }
    }
    if p.obstacles.enabled {
        l.push("obstacles".into());
        if p.obstacles.power_towers {
            l.push("power_towers".into());
        }
    }
    if p.terrain.enabled && p.terrain.hydro {
        l.push("water".into());
    }
    if p.terrain.enabled && p.terrain.flatten_runways {
        l.push("runways".into());
    }
    if p.vegetation.enabled {
        let v = &p.vegetation;
        for (on, layer) in [(v.roads, "roads"), (v.railways, "railways"), (v.water, "water"), (v.runways, "runways"), (v.pitches, "pitches"), (v.parking, "parking"), (v.landuse, "landuse")] {
            if on {
                l.push(layer.into());
            }
        }
    }
    l.sort();
    l.dedup();
    l
}

fn needs_osm(p: &Project) -> bool {
    !osm_layers(p).is_empty()
}

fn osm_path(st: &AppState, id: &str) -> PathBuf {
    st.store.project_dir(id).join("osm.json")
}

fn load_osm(st: &AppState, id: &str) -> Option<osm::OsmData> {
    store::read_json(&osm_path(st, id))
}

fn stage_osm(ctx: &Ctx) -> Result<Value, String> {
    let st = ctx.state();
    let p = st.store.project(&ctx.project)?;
    require_area(&p)?;
    let layers = osm_layers(&p);
    if layers.is_empty() {
        return Ok(json!({"skipped": true}));
    }
    let b = p.area.bbox().grow_m(150.0);
    if b.width_m() * b.height_m() > 2.5e9 {
        return Err("Obszar jest za duży dla jednego zapytania OSM (ponad 2500 km²) – podziel projekt".into());
    }
    ctx.progress("osm", 0, 1, "Overpass API…");
    let settings = st.store.settings();
    let (elements, bytes) = osm::fetch(&settings.overpass_url, &b, &layers)?;
    ctx.check()?;
    let n = elements.len();
    let data = osm::OsmData { fetched: store::now(), bbox: b, layers: layers.clone(), elements };
    let bytes_json = serde_json::to_vec(&data).map_err(|e| e.to_string())?;
    store::write_atomic(&osm_path(&st, &ctx.project), &bytes_json)?;
    set_status(ctx, "osm", format!("{}|{}", store::now(), n));
    ctx.progress("osm", 1, 1, "");
    Ok(json!({"elements": n, "bytes": bytes, "layers": layers}))
}

#[tauri::command]
fn osm_summary(state: St, id: String) -> Value {
    let Some(d) = load_osm(&state, &id) else { return json!(null) };
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for e in &d.elements {
        let k = if e.is("highway", "street_lamp") || e.is("man_made", "street_lamp") || e.tag("light_source").is_some() {
            "lamps"
        } else if e.tag("highway").is_some() {
            if lights::lit(e) == Some(true) {
                "litRoads"
            } else {
                "roads"
            }
        } else if e.tag("landuse").is_some() && !e.is("landuse", "reservoir") && !e.is("landuse", "basin") {
            "landuse"
        } else if e.is("amenity", "parking") {
            "parking"
        } else if e.tag("leisure").is_some() {
            "pitches"
        } else if e.tag("man_made").is_some() || e.tag("power").is_some() || e.tag("seamark:type").is_some() || e.tag("generator:method").is_some() {
            "obstacles"
        } else if e.tag("natural").is_some() || e.tag("waterway").is_some() || e.tag("landuse").is_some() {
            "water"
        } else if e.tag("railway").is_some() || e.tag("public_transport").is_some() {
            "railways"
        } else if e.tag("aeroway").is_some() {
            "aeroways"
        } else {
            "other"
        };
        *counts.entry(k).or_default() += 1;
    }
    json!({"fetched": d.fetched, "layers": d.layers, "counts": counts, "total": d.elements.len()})
}

// ── DEM ──

fn stage_dem(ctx: &Ctx) -> Result<Value, String> {
    let st = ctx.state();
    let p = st.store.project(&ctx.project)?;
    require_area(&p)?;
    if !p.terrain.enabled {
        return Ok(json!({"skipped": true}));
    }
    let b = p.area.bbox().grow_m(p.terrain.resolution * 8.0 + p.terrain.smooth * 3.0);
    let mut warnings = vec![];
    ctx.progress("dem", 0, 0, "Wczytywanie plików…");
    let mosaic = terrain::build_mosaic(&p.terrain, &st.cache.join("dem"), &b, &mut warnings)?;
    // Źródła sieciowe: pobieramy tylko to, czego wyższe w kolejności źródła nie pokrywają w całości.
    let mut jobs: Vec<(usize, String)> = vec![];
    for (i, s) in mosaic.sources.iter().enumerate() {
        let chunks = s.chunks(&b);
        if chunks.is_empty() {
            continue;
        }
        jobs.extend(chunks.into_iter().map(|c| (i, c)));
        // GUGiK pokrywa całą Polskę – gdy obszar leży w całości w kraju, niższe źródła są zbędne.
        if s.id() == "gugik" && dem::POLAND.contains(LatLon::new(b.south, b.west)) && dem::POLAND.contains(LatLon::new(b.north, b.east)) {
            break;
        }
    }
    let total = jobs.len();
    let done = AtomicUsize::new(0);
    let bytes = AtomicUsize::new(0);
    let errors = Mutex::new(Vec::<String>::new());
    let pool = rayon::ThreadPoolBuilder::new().num_threads(6).build().map_err(|e| e.to_string())?;
    pool.install(|| {
        jobs.par_iter().for_each(|(i, key)| {
            if ctx.cancel.load(Ordering::Relaxed) {
                return;
            }
            let s = &mosaic.sources[*i];
            if !s.cached(key) {
                match s.fetch(key) {
                    Ok(n) => {
                        bytes.fetch_add(n as usize, Ordering::Relaxed);
                    }
                    Err(e) => lock(&errors).push(format!("{}: {e}", s.label())),
                }
            }
            let d = done.fetch_add(1, Ordering::Relaxed) + 1;
            if d % 2 == 0 || d == total {
                ctx.progress("dem", d, total, &s.label());
            }
        });
    });
    ctx.check()?;
    let errors = lock(&errors).clone();
    if !errors.is_empty() && errors.len() * 2 > total.max(1) {
        return Err(format!("Nie udało się pobrać danych: {}", errors[0]));
    }
    warnings.extend(errors.into_iter().take(5));
    let mb = bytes.load(Ordering::Relaxed) as f64 / 1e6;
    set_status(ctx, "dem", format!("{}|{}|{:.1}", store::now(), total, mb));
    Ok(json!({"chunks": total, "downloadedMb": mb, "warnings": warnings, "sources": mosaic.sources.iter().map(|s| json!({"label": s.label(), "resolution": s.resolution()})).collect::<Vec<_>>()}))
}

// ── Teren ──

fn terrain_path(st: &AppState, id: &str) -> PathBuf {
    st.store.project_dir(id).join("terrain.bin")
}

fn stage_terrain(ctx: &Ctx) -> Result<Value, String> {
    let st = ctx.state();
    let p = st.store.project(&ctx.project)?;
    require_area(&p)?;
    if !p.terrain.enabled {
        return Ok(json!({"skipped": true}));
    }
    let plan = terrain::plan(&p.terrain, &p.area);
    if plan.too_big {
        return Err(format!("Za dużo danych ({} prostokątów) – zwiększ krok siatki albo zmniejsz obszar", plan.tiles));
    }
    let b = p.area.bbox().grow_m(p.terrain.resolution * 8.0 + p.terrain.smooth * 3.0);
    let mut warnings = vec![];
    ctx.progress("terrain", 0, plan.tiles, "Wczytywanie źródeł…");
    let mosaic = terrain::build_mosaic(&p.terrain, &st.cache.join("dem"), &b, &mut warnings)?;
    let lakes = if p.terrain.hydro {
        match load_osm(&st, &ctx.project) {
            Some(d) if d.layers.iter().any(|l| l == "water") => {
                ctx.progress("terrain", 0, plan.tiles, "Poziomy wód…");
                terrain::lakes(&d.elements, &b, &mosaic, p.terrain.resolution)
            }
            _ => {
                warnings.push("Spłaszczanie wód wymaga danych OSM – pobierz je i policz teren ponownie".into());
                vec![]
            }
        }
    } else {
        vec![]
    };
    let runways = if p.terrain.flatten_runways {
        match load_osm(&st, &ctx.project) {
            Some(d) if d.layers.iter().any(|l| l == "runways") => terrain::runways(&d.elements, &b, &mosaic),
            _ => {
                warnings.push("Wyrównanie pasów startowych wymaga danych OSM – pobierz je i policz teren ponownie".into());
                vec![]
            }
        }
    } else {
        vec![]
    };
    let done = AtomicUsize::new(0);
    let finished = AtomicBool::new(false);
    let result = std::thread::scope(|scope| {
        scope.spawn(|| {
            while !finished.load(Ordering::Relaxed) {
                std::thread::sleep(std::time::Duration::from_millis(400));
                ctx.progress("terrain", done.load(Ordering::Relaxed), plan.tiles, "");
            }
        });
        let r = terrain::compute(&p.terrain, &p.area, &mosaic, &lakes, &runways, terrain::Progress { done: &done, cancel: &ctx.cancel }, warnings);
        finished.store(true, Ordering::Relaxed);
        r
    })?;
    ctx.check()?;
    ctx.progress("terrain", plan.tiles, plan.tiles, "Zapis…");
    result.save(&terrain_path(&st, &ctx.project))?;
    if let Ok((png, _)) = result.preview_png(&p.area, 1600) {
        let _ = store::write_atomic(&st.store.project_dir(&ctx.project).join("terrain.png"), &png);
    }
    let s = &result.summary;
    set_status(ctx, "terrain", format!("{}|{}|{:.0}|{:.0}", store::now(), s.tiles, s.min, s.max));
    serde_json::to_value(s).map_err(|e| e.to_string())
}

#[tauri::command]
async fn terrain_summary(state: St<'_>, id: String) -> Result<Option<terrain::Summary>, String> {
    let path = terrain_path(&state, &id);
    tauri::async_runtime::spawn_blocking(move || {
        // Podsumowanie leży na początku pliku – czytamy nagłówek bez wysokości.
        let b = std::fs::read(&path).ok()?;
        if b.len() < 16 || &b[..8] != b"FSOSTER1" {
            return None;
        }
        let hl = u64::from_le_bytes(b[8..16].try_into().ok()?) as usize;
        let v: Value = serde_json::from_slice(b.get(16..16 + hl)?).ok()?;
        serde_json::from_value(v.get("summary")?.clone()).ok()
    })
    .await
    .map_err(|e| e.to_string())
}

#[tauri::command]
async fn terrain_preview(state: St<'_>, id: String) -> Result<Option<Value>, String> {
    use base64::Engine;
    let p = state.store.project(&id)?;
    let png = state.store.project_dir(&id).join("terrain.png");
    let tpath = terrain_path(&state, &id);
    tauri::async_runtime::spawn_blocking(move || {
        let data = match std::fs::read(&png) {
            Ok(d) => d,
            Err(_) => {
                let Ok(t) = terrain::Terrain::load(&tpath) else { return Ok(None) };
                t.preview_png(&p.area, 1600)?.0
            }
        };
        let b = p.area.bbox();
        Ok(Some(json!({"url": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&data)), "bbox": b})))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn terrain_profile(state: St<'_>, id: String, a: LatLon, b: LatLon) -> Result<Vec<(f64, f32)>, String> {
    let path = terrain_path(&state, &id);
    tauri::async_runtime::spawn_blocking(move || {
        let t = terrain::Terrain::load(&path)?;
        let len = geo::haversine(a, b);
        let n = 300usize;
        Ok((0..=n)
            .filter_map(|k| {
                let f = k as f64 / n as f64;
                let p = LatLon::new(a.lat + (b.lat - a.lat) * f, a.lon + (b.lon - a.lon) * f);
                t.sample(p).map(|h| (len * f, h))
            })
            .collect())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn export_terrain_tiff(state: St<'_>, id: String, path: String) -> Result<(u32, u32), String> {
    let tpath = terrain_path(&state, &id);
    tauri::async_runtime::spawn_blocking(move || terrain::Terrain::load(&tpath)?.write_geotiff(Path::new(&path))).await.map_err(|e| e.to_string())?
}

// ── Światła ──

fn lights_path(st: &AppState, id: &str) -> PathBuf {
    st.store.project_dir(id).join("lights.json")
}

fn stage_lights(ctx: &Ctx) -> Result<Value, String> {
    let st = ctx.state();
    let p = st.store.project(&ctx.project)?;
    require_area(&p)?;
    let data = load_osm(&st, &ctx.project).ok_or("Najpierw pobierz dane OpenStreetMap")?;
    ctx.progress("lights", 0, 1, "");
    let mut res = if p.lights.enabled { lights::generate(&data.elements, &p.area, &p.lights, &p.manual, &p.excluded) } else { lights::GenResult::default() };
    if p.obstacles.enabled {
        let obs = lights::obstacles(&data.elements, &p.area, &p.obstacles);
        for l in &obs {
            *res.counts.entry(l.cat.clone()).or_default() += 1;
        }
        res.lights.extend(obs);
    }
    res.models = lights::count_models(&res.lights);
    if res.models > 400 {
        res.warnings.push(format!("Dużo różnych modeli ({}) – wydłuży to kompilację", res.models));
    }
    let bytes = serde_json::to_vec(&res.lights).map_err(|e| e.to_string())?;
    store::write_atomic(&lights_path(&st, &ctx.project), &bytes)?;
    set_status(ctx, "lights", format!("{}|{}|{}", store::now(), res.lights.len(), res.models));
    ctx.progress("lights", 1, 1, "");
    Ok(json!({"lights": res.lights.len(), "models": res.models, "counts": res.counts, "warnings": res.warnings}))
}

fn load_lights(st: &AppState, id: &str) -> Vec<lights::Light> {
    store::read_json(&lights_path(st, id)).unwrap_or_default()
}

#[tauri::command]
async fn lights_data(state: St<'_>, id: String) -> Result<Value, String> {
    let path = lights_path(&state, &id);
    tauri::async_runtime::spawn_blocking(move || {
        let list: Vec<lights::Light> = store::read_json(&path).unwrap_or_default();
        Ok(lights_value(&list))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Kropki świateł dla mapy: [lat, lon, kierunek, kategoria, źródło, kolor, natężenie, osm, alt, styl, wysokość, błyska].
fn lights_value(list: &[lights::Light]) -> Value {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    let dots: Vec<Value> = list
        .iter()
        .map(|l| {
            *counts.entry(l.cat.clone()).or_default() += 1;
            let c = l.spec.color.unwrap_or_else(|| geo::kelvin_rgb(l.spec.kelvin));
            let hex = format!("#{:02x}{:02x}{:02x}", (c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8);
            json!([l.lat, l.lon, l.heading.round(), l.cat, l.src.to_string(), hex, l.spec.intensity.round(), l.osm, l.alt, l.spec.style, l.spec.height, l.spec.flash.is_some()])
        })
        .collect();
    json!({"dots": dots, "counts": counts, "models": lights::count_models(list)})
}

/// Teren i światła dla podglądu 3D (współrzędne w metrach od narożnika NW obszaru).
fn preview3d_value(area: &Area, t: Option<&terrain::Terrain>, lights: &[lights::Light]) -> Value {
    let b = area.bbox();
    let loc = geo::Local::new(LatLon::new(b.north, b.west));
    let grid = t.map(|t| t.grid3d(area, 360));
    let lamp: Vec<Value> = lights
        .iter()
        .take(60_000)
        .map(|l| {
            let (x, y) = loc.to_xy(LatLon::new(l.lat, l.lon));
            let ground = t.and_then(|t| t.sample(LatLon::new(l.lat, l.lon))).unwrap_or(0.0) as f64;
            let top = if l.alt > 0.0 { l.alt } else { l.spec.height.max(0.5) };
            let c = l.spec.color.unwrap_or_else(|| geo::kelvin_rgb(l.spec.kelvin));
            json!([x.round(), (-y).round(), (ground + top).round(), ground.round(), c, l.spec.intensity, l.spec.flash.is_some()])
        })
        .collect();
    json!({"grid": grid, "lights": lamp, "widthM": b.width_m(), "heightM": b.height_m()})
}

#[tauri::command]
fn export_lights_geojson(state: St, id: String, path: String) -> Result<usize, String> {
    let list = load_lights(&state, &id);
    let feats: Vec<Value> = list
        .iter()
        .map(|l| {
            json!({"type": "Feature", "geometry": {"type": "Point", "coordinates": [l.lon, l.lat]},
                "properties": {"category": l.cat, "source": l.src.to_string(), "osm_id": l.osm, "heading": l.heading, "style": l.spec.style,
                    "height": l.spec.height, "altitude_agl": l.alt, "kelvin": l.spec.kelvin, "intensity": l.spec.intensity, "flashing": l.spec.flash.is_some()}})
        })
        .collect();
    let data = serde_json::to_vec(&json!({"type": "FeatureCollection", "features": feats})).map_err(|e| e.to_string())?;
    store::write_atomic(Path::new(&path), &data)?;
    Ok(list.len())
}

// ── Podgląd 3D ──

#[tauri::command]
async fn preview_3d(state: St<'_>, id: String) -> Result<Value, String> {
    let p = state.store.project(&id)?;
    let tpath = terrain_path(&state, &id);
    let lpath = lights_path(&state, &id);
    tauri::async_runtime::spawn_blocking(move || {
        let t = terrain::Terrain::load(&tpath).ok();
        let lights: Vec<lights::Light> = store::read_json(&lpath).unwrap_or_default();
        // Bez policzonego terenu – płaska podstawa na wysokości 0.
        Ok(preview3d_value(&p.area, t.as_ref(), &lights))
    })
    .await
    .map_err(|e| e.to_string())?
}

// ── Budowanie i instalacja ──

fn work_dir(st: &AppState, id: &str) -> PathBuf {
    st.store.project_dir(id).join("msfs")
}

fn built_package(st: &AppState, p: &Project) -> PathBuf {
    work_dir(st, &p.id).join("Packages").join(package::package_name(p))
}

fn stage_build(ctx: &Ctx, install: bool) -> Result<Value, String> {
    let st = ctx.state();
    let p = st.store.project(&ctx.project)?;
    require_area(&p)?;
    let settings = st.store.settings();
    ctx.progress("build", 0, 3, "Zapis źródeł…");
    let terrain = if p.terrain.enabled { terrain::Terrain::load(&terrain_path(&st, &ctx.project)).ok() } else { None };
    let lights = if p.lights.enabled || p.obstacles.enabled { load_lights(&st, &ctx.project) } else { vec![] };
    let veg = if p.vegetation.enabled {
        load_osm(&st, &ctx.project).map(|d| vegetation::polygons(&d.elements, &p.area, &p.vegetation)).unwrap_or_default()
    } else {
        vec![]
    };
    let preview = std::fs::read(st.store.project_dir(&ctx.project).join("terrain.png")).ok();
    let thumb = package::thumbnail(preview.as_deref(), &lights, &p.name);
    let work = work_dir(&st, &ctx.project);
    let sum = package::write_sources(&work, &p, terrain.as_ref(), &lights, &veg, thumb.as_deref(), p.lights.far_lod)?;
    ctx.log(&format!("Źródła SDK: {} plików, {:.1} MB · prostokąty terenu: {} · światła: {} · modele: {} · roślinność: {}", sum.files, sum.bytes as f64 / 1e6, sum.rectangles, sum.lights, sum.models, sum.vegetation));
    ctx.check()?;

    match sim::find_sdk(&settings.sdk_path) {
        None => {
            set_status(ctx, "build", format!("{}|sources", store::now()));
            return Ok(json!({"sources": sum, "compiled": false, "reason": "sdk"}));
        }
        Some(sdk) if sdk.legacy => {
            set_status(ctx, "build", format!("{}|sources", store::now()));
            let msg = sim::legacy_message(&sdk);
            ctx.log(&msg);
            return Err(format!("{msg}. Źródła projektu zapisano."));
        }
        _ => {}
    }
    ctx.progress("build", 1, 3, "Kompilacja (fspackagetool)…");
    let pkg = built_package(&st, &p);
    let secs = compile_with_sdk(ctx, &work, Path::new(&sum.project_xml), &pkg)?;
    set_status(ctx, "build", format!("{}|ok|{}", store::now(), store::dir_size(&pkg)));
    let mut installed = Value::Null;
    if install {
        ctx.progress("build", 2, 3, "Instalacja w Community…");
        installed = json!(install_dir(&st, &pkg, &package::package_name(&p))?);
    }
    Ok(json!({"sources": sum, "compiled": true, "package": pkg, "seconds": secs, "installed": installed}))
}

/// Kompilacja projektu SDK przez fspackagetool z dziennikiem na żywo. Zwraca czas w sekundach.
fn compile_with_sdk(ctx: &Ctx, work: &Path, project_xml: &Path, pkg: &Path) -> Result<u64, String> {
    let st = ctx.state();
    let settings = st.store.settings();
    let sdk = sim::find_sdk(&settings.sdk_path).ok_or("Nie znaleziono MSFS 2024 SDK – wskaż jego folder w ustawieniach")?;
    if sdk.legacy {
        return Err(sim::legacy_message(&sdk));
    }
    if sim::is_running() {
        return Err("Zamknij Microsoft Flight Simulator 2024 – kompilator pakietów uruchamia własną instancję symulatora".into());
    }
    let project_xml = package::short_path(project_xml);
    let work_s = package::short_path(work);
    let mut cmd = std::process::Command::new(&sdk.package_tool);
    cmd.arg(&project_xml).arg("-outputdir").arg(&work_s).arg("-rebuild").arg("-nopause");
    let force_steam = match settings.build_store.as_str() {
        "steam" => true,
        "store" => false,
        _ => {
            let inst = sim::detect();
            !inst.is_empty() && inst.iter().all(|i| i.store == "steam")
        }
    };
    if force_steam {
        cmd.arg("-forcesteam");
    }
    cmd.current_dir(&work_s).stdout(std::process::Stdio::piped()).stderr(std::process::Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    ctx.log(&format!("> {} {} -outputdir {} -rebuild -nopause{}", sdk.package_tool, project_xml.display(), work_s.display(), if force_steam { " -forcesteam" } else { "" }));
    let started = std::time::Instant::now();
    let mut child = cmd.spawn().map_err(|e| format!("Nie udało się uruchomić fspackagetool: {e}"))?;
    *lock(&st.child) = Some(child.id());
    let stderr = child.stderr.take();
    let err_lines = Arc::new(Mutex::new(Vec::<String>::new()));
    let err_thread = {
        let err_lines = err_lines.clone();
        let app = ctx.app.clone();
        let (id, proj) = (ctx.id.clone(), ctx.project.clone());
        std::thread::spawn(move || {
            if let Some(e) = stderr {
                for line in std::io::BufReader::new(e).lines().map_while(Result::ok) {
                    let _ = app.emit("build-log", json!({"id": id, "project": proj, "line": line}));
                    lock(&err_lines).push(line);
                }
            }
        })
    };
    let mut log = String::new();
    if let Some(out) = child.stdout.take() {
        for line in std::io::BufReader::new(out).lines().map_while(Result::ok) {
            ctx.log(&line);
            log.push_str(&line);
            log.push('\n');
        }
    }
    let status = child.wait().map_err(|e| e.to_string())?;
    let _ = err_thread.join();
    *lock(&st.child) = None;
    for l in lock(&err_lines).iter() {
        log.push_str(l);
        log.push('\n');
    }
    let _ = std::fs::write(work.join("build.log"), &log);
    ctx.check()?;
    if !status.success() || !pkg.join("layout.json").exists() {
        let mut hint = log.lines().rev().find(|l| l.to_lowercase().contains("error")).unwrap_or("").trim().to_string();
        if log.trim().is_empty() {
            // fspackagetool nic nie wypisał – zwykle nie mógł uruchomić symulatora (inne konto, brak licencji, zła edycja).
            hint = "fspackagetool nie wypisał żadnego błędu – zwykle nie mógł uruchomić MSFS 2024. Sprawdź w Ustawieniach edycję symulatora do kompilacji (Steam / Microsoft Store) i czy MSFS 2024 startuje z tego konta".into();
            ctx.log(&hint);
        }
        return Err(format!("Kompilacja nie powiodła się (kod {}){}", status.code().unwrap_or(-1), if hint.is_empty() { String::new() } else { format!(": {hint}") }));
    }
    let secs = started.elapsed().as_secs();
    ctx.log(&format!("Gotowe w {secs} s: {}", pkg.display()));
    if !settings.keep_sources {
        let _ = std::fs::remove_dir_all(work.join("_PackageInt"));
    }
    Ok(secs)
}

// ── Pakiet testowy oświetlenia ──

fn test_dir(st: &AppState, id: &str) -> PathBuf {
    st.store.project_dir(id).join("msfs-test")
}

/// Mały pakiet z rzędem każdego rodzaju lampy w środku obszaru – szybka kalibracja
/// jasności, barwy i kierunku wysięgników w symulatorze.
fn stage_testlights(ctx: &Ctx, install: bool) -> Result<Value, String> {
    let st = ctx.state();
    let p = st.store.project(&ctx.project)?;
    require_area(&p)?;
    ctx.progress("build", 0, 3, "Zapis źródeł…");
    let lights = lights::test_pattern(p.center(), &p.lights, &p.obstacles);
    let mut tp = p.clone();
    tp.package.name = format!("{}-test", package::package_name(&p));
    tp.package.title = format!("{} – test oświetlenia", if p.package.title.is_empty() { &p.name } else { &p.package.title });
    tp.terrain.enabled = false;
    let work = test_dir(&st, &ctx.project);
    let thumb = package::thumbnail(None, &lights, &tp.name);
    let sum = package::write_sources(&work, &tp, None, &lights, &[], thumb.as_deref(), tp.lights.far_lod)?;
    ctx.log(&format!("Pakiet testowy: {} lamp, {} modeli w środku obszaru ({:.5}, {:.5})", sum.lights, sum.models, p.center().lat, p.center().lon));
    if sim::find_sdk(&st.store.settings().sdk_path).is_none() {
        return Ok(json!({"sources": sum, "compiled": false, "reason": "sdk", "test": true}));
    }
    ctx.progress("build", 1, 3, "Kompilacja (fspackagetool)…");
    let pkg = work.join("Packages").join(package::package_name(&tp));
    let secs = compile_with_sdk(ctx, &work, Path::new(&sum.project_xml), &pkg)?;
    let mut installed = Value::Null;
    if install {
        ctx.progress("build", 2, 3, "Instalacja w Community…");
        installed = json!(install_dir(&st, &pkg, &package::package_name(&tp))?);
    }
    Ok(json!({"sources": sum, "compiled": true, "package": pkg, "seconds": secs, "installed": installed, "test": true, "center": p.center()}))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PackageStatus {
    name: String,
    built: bool,
    built_path: String,
    built_bytes: u64,
    sources: bool,
    work_dir: String,
    installed: Option<String>,
    linked: bool,
    community: Option<String>,
    log: bool,
    test_dir: String,
    test_built: bool,
}

#[tauri::command]
async fn package_status(state: St<'_>, id: String) -> Result<PackageStatus, String> {
    let p = state.store.project(&id)?;
    let settings = state.store.settings();
    let pkg = built_package(&state, &p);
    let work = work_dir(&state, &id);
    let name = package::package_name(&p);
    let tdir = test_dir(&state, &id);
    let tname = format!("{name}-test");
    tauri::async_runtime::spawn_blocking(move || {
        let community = sim::community(&settings.community_path);
        let target = community.as_ref().map(|c| c.join(&name));
        let installed = target.as_ref().filter(|t| t.exists() || package::is_link(t)).map(|t| t.to_string_lossy().into_owned());
        let linked = target.as_ref().map(|t| package::is_link(t)).unwrap_or(false);
        Ok(PackageStatus {
            built: pkg.join("layout.json").exists(),
            built_bytes: if pkg.exists() { store::dir_size(&pkg) } else { 0 },
            built_path: pkg.to_string_lossy().into_owned(),
            sources: work.join("PackageDefinitions").exists(),
            log: work.join("build.log").exists(),
            work_dir: work.to_string_lossy().into_owned(),
            name,
            installed,
            linked,
            community: community.map(|c| c.to_string_lossy().into_owned()),
            test_built: tdir.join("Packages").join(&tname).join("layout.json").exists(),
            test_dir: tdir.to_string_lossy().into_owned(),
        })
    })
    .await
    .map_err(|e| e.to_string())?
}

fn do_install(st: &AppState, p: &Project) -> Result<String, String> {
    install_dir(st, &built_package(st, p), &package::package_name(p))
}

/// Instalacja zbudowanego pakietu w Community (kopia albo łącze katalogu).
fn install_dir(st: &AppState, pkg: &Path, name: &str) -> Result<String, String> {
    let settings = st.store.settings();
    if !pkg.join("layout.json").exists() {
        return Err("Pakiet nie jest jeszcze zbudowany".into());
    }
    let community = sim::community(&settings.community_path).ok_or("Nie znaleziono folderu Community – wskaż go w ustawieniach")?;
    if !community.is_dir() {
        return Err(format!("Folder Community nie istnieje: {}", community.display()));
    }
    let target = community.join(name);
    package::remove_installed(&target)?;
    if settings.install_mode == "link" {
        package::make_link(&target, pkg)?;
    } else {
        package::copy_dir(pkg, &target)?;
    }
    Ok(target.to_string_lossy().into_owned())
}

#[tauri::command]
async fn install_package(state: St<'_>, id: String) -> Result<String, String> {
    let p = state.store.project(&id)?;
    if sim::is_running() {
        return Err("Zamknij symulator przed instalacją – MSFS blokuje pliki pakietów w Community".into());
    }
    let st: &AppState = &state;
    // Kopiowanie może potrwać – ale stan nie jest 'static, więc robimy to w bieżącym wątku asynchronicznym.
    do_install(st, &p)
}

#[tauri::command]
fn uninstall_package(state: St, id: String) -> Result<(), String> {
    let p = state.store.project(&id)?;
    if sim::is_running() {
        return Err("Zamknij symulator przed usunięciem pakietu".into());
    }
    let community = sim::community(&state.store.settings().community_path).ok_or("Nie znaleziono folderu Community")?;
    package::remove_installed(&community.join(package::package_name(&p)))
}

#[tauri::command]
async fn export_zip(state: St<'_>, id: String, path: String) -> Result<u64, String> {
    let p = state.store.project(&id)?;
    let pkg = built_package(&state, &p);
    if !pkg.exists() {
        return Err("Pakiet nie jest jeszcze zbudowany".into());
    }
    tauri::async_runtime::spawn_blocking(move || package::zip_dir(&pkg, Path::new(&path))).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn build_log(state: St, id: String) -> String {
    std::fs::read_to_string(work_dir(&state, &id).join("build.log")).unwrap_or_default()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct InstalledPackage {
    name: String,
    title: String,
    path: String,
    bytes: u64,
    linked: bool,
    project: Option<String>,
    project_name: Option<String>,
    test: bool,
}

/// Nazwy pakietów, które należą do projektów aplikacji (z wariantem testowym).
fn own_packages(st: &AppState) -> HashMap<String, (String, String)> {
    let mut m = HashMap::new();
    for p in st.store.projects() {
        let n = package::package_name(&p);
        m.insert(format!("{n}-test"), (p.id.clone(), p.name.clone()));
        m.insert(n, (p.id.clone(), p.name.clone()));
    }
    m
}

#[tauri::command]
async fn installed_packages(state: St<'_>) -> Result<Vec<InstalledPackage>, String> {
    let own = own_packages(&state);
    let custom = state.store.settings().community_path;
    tauri::async_runtime::spawn_blocking(move || {
        let mut dirs: Vec<PathBuf> = sim::detect().into_iter().flat_map(|i| i.communities).map(PathBuf::from).collect();
        if let Some(c) = sim::community(&custom) {
            dirs.push(c);
        }
        dirs.sort();
        dirs.dedup();
        let mut out = vec![];
        for d in dirs {
            let Ok(rd) = std::fs::read_dir(&d) else { continue };
            for e in rd.flatten() {
                let name = e.file_name().to_string_lossy().into_owned();
                let mine = own.get(&name);
                // Pakiety z projektów aplikacji oraz każde „fsos-…” (np. z usuniętego projektu).
                if mine.is_none() && !name.starts_with("fsos-") {
                    continue;
                }
                let path = e.path();
                let manifest: Value = store::read_json(&path.join("manifest.json")).unwrap_or(Value::Null);
                out.push(InstalledPackage {
                    title: manifest.get("title").and_then(|t| t.as_str()).unwrap_or(&name).to_string(),
                    linked: package::is_link(&path),
                    bytes: if package::is_link(&path) { 0 } else { store::dir_size(&path) },
                    path: path.to_string_lossy().into_owned(),
                    project: mine.map(|m| m.0.clone()),
                    project_name: mine.map(|m| m.1.clone()),
                    test: name.ends_with("-test"),
                    name,
                });
            }
        }
        out.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn remove_installed_package(state: St<'_>, path: String) -> Result<(), String> {
    if sim::is_running() {
        return Err("Zamknij symulator przed usunięciem pakietu".into());
    }
    let list = installed_packages(state).await?;
    // Usuwamy wyłącznie pakiety z listy aplikacji – nigdy dowolną ścieżkę.
    if !list.iter().any(|p| p.path == path) {
        return Err("To nie jest pakiet utworzony przez aplikację".into());
    }
    tauri::async_runtime::spawn_blocking(move || package::remove_installed(Path::new(&path))).await.map_err(|e| e.to_string())?
}

// ───────────────────────────── Pamięć podręczna ─────────────────────────────

#[tauri::command]
async fn cache_info(state: St<'_>) -> Result<Vec<(String, u64)>, String> {
    let dir = state.cache.join("dem");
    tauri::async_runtime::spawn_blocking(move || {
        let mut out = vec![];
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                if e.path().is_dir() {
                    out.push((e.file_name().to_string_lossy().into_owned(), store::dir_size(&e.path())));
                }
            }
        }
        out.sort();
        Ok(out)
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
fn clear_cache(state: St, name: Option<String>) -> Result<(), String> {
    if !lock(&state.busy).is_empty() {
        return Err("Poczekaj na zakończenie zadań w tle".into());
    }
    let dir = state.cache.join("dem");
    match name {
        Some(n) => {
            let safe: String = n.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_').collect();
            let p = dir.join(safe);
            if p.is_dir() && p != dir {
                std::fs::remove_dir_all(p).map_err(|e| e.to_string())?;
            }
        }
        None => {
            if dir.is_dir() {
                std::fs::remove_dir_all(&dir).map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

// ───────────────────────────── Aktualizacje ─────────────────────────────

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateInfo {
    available: bool,
    latest: String,
    url: String,
    download: Option<String>,
    notes: String,
    can_install: bool,
}

fn version_tuple(v: &str) -> Vec<u64> {
    v.trim_start_matches('v').split(['.', '-']).filter_map(|x| x.parse().ok()).collect()
}

const UPDATE_FEED: &str = "https://redzikowski.com/api/releases/repos/BlackSingularities/FSOpendataScenery/releases/latest";
const UPDATE_ASSETS: &str = "https://redzikowski.com/api/projects/fsopendatascenery/asset/";

#[tauri::command]
async fn check_update() -> Result<UpdateInfo, String> {
    let client = reqwest::Client::builder()
        .user_agent(concat!("FSOpendataScenery/", env!("CARGO_PKG_VERSION")))
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|e| e.to_string())?;
    let r = client.get(UPDATE_FEED).send().await.map_err(|e| format!("Brak połączenia z redzikowski.com: {e}"))?;
    if !r.status().is_success() {
        return Err(format!("Serwer aktualizacji odpowiedział kodem {}", r.status()));
    }
    let v: Value = r.json().await.map_err(|e| e.to_string())?;
    let tag = v.get("tag_name").and_then(|x| x.as_str()).unwrap_or("").to_string();
    let kind = update::install_kind();
    let want = update::asset_suffix(kind);
    let download = v.get("assets").and_then(|a| a.as_array()).and_then(|a| {
        a.iter()
            .find(|x| x.get("name").and_then(|n| n.as_str()).map(|n| n.ends_with(want)).unwrap_or(false))
            .and_then(|x| x.get("browser_download_url").and_then(|u| u.as_str()).map(String::from))
    });
    Ok(UpdateInfo {
        available: version_tuple(&tag) > version_tuple(env!("CARGO_PKG_VERSION")),
        latest: tag,
        url: String::new(),
        notes: String::new(),
        can_install: matches!(kind, "portable" | "installed"),
        download,
    })
}

#[tauri::command]
async fn install_update(app: AppHandle, url: String) -> Result<(), String> {
    if !url.starts_with(UPDATE_ASSETS) {
        return Err("Nieprawidłowy adres aktualizacji".into());
    }
    let run = update::install(&url).await?;
    std::process::Command::new(&run).spawn().map_err(|e| format!("Uruchomienie nowej wersji: {e}"))?;
    app.exit(0);
    Ok(())
}

// ───────────────────────────── Start ─────────────────────────────

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(w) = app.get_webview_window("main") {
                let _ = w.unminimize();
                let _ = w.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data = app.path().app_data_dir().unwrap_or_else(|_| std::env::temp_dir().join("FSOpendataScenery"));
            let cache = app.path().app_cache_dir().unwrap_or_else(|_| data.join("cache"));
            let _ = std::fs::create_dir_all(&cache);
            app.manage(AppState { store: Store::new(data), cache, jobs: Mutex::new(HashMap::new()), busy: Mutex::new(HashSet::new()), child: Mutex::new(None) });
            update::cleanup_old();
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            app_info,
            get_settings,
            save_settings,
            env_status,
            list_projects,
            get_project,
            create_project,
            save_project,
            delete_project,
            duplicate_project,
            export_project,
            import_project,
            search_places,
            search_airports,
            area_import,
            area_circle,
            dem_file_info,
            terrain_plan,
            start_job,
            cancel_job,
            osm_summary,
            terrain_summary,
            terrain_preview,
            terrain_profile,
            export_terrain_tiff,
            lights_data,
            export_lights_geojson,
            preview_3d,
            package_status,
            install_package,
            uninstall_package,
            export_zip,
            build_log,
            installed_packages,
            remove_installed_package,
            cache_info,
            clear_cache,
            check_update,
            install_update,
        ])
        .run(tauri::generate_context!())
        .expect("błąd uruchomienia aplikacji");
}

#[cfg(test)]
mod e2e {
    use super::*;

    /// Pełny przebieg na prawdziwych danych (sieć): `cargo test -- --ignored e2e --nocapture`.
    #[test]
    #[ignore]
    fn krakow_pipeline() {
        let work = std::env::temp_dir().join("fsos-e2e");
        let _ = std::fs::remove_dir_all(&work);
        // Okolice Plant i Rynku w Krakowie, ~1,2 × 1 km.
        let ring = vec![LatLon::new(50.0470, 19.9260), LatLon::new(50.0470, 19.9520), LatLon::new(50.0675, 19.9520), LatLon::new(50.0675, 19.9260)];
        let mut p = Project { id: "e2e".into(), name: "Kraków – Stare Miasto i Wawel".into(), ..Default::default() };
        p.area = Area { outer: ring, holes: vec![] };
        apply_regional_defaults(&mut p);
        p.vegetation.enabled = true;
        let p = normalize(p);
        let layers = osm_layers(&p);
        let (els, bytes) = osm::fetch("", &p.area.bbox().grow_m(150.0), &layers).expect("OSM");
        println!("OSM: {} elementów, {} kB", els.len(), bytes / 1024);
        let cache = work.join("cache");
        let mut warnings = vec![];
        let b = p.area.bbox().grow_m(50.0);
        let mosaic = terrain::build_mosaic(&p.terrain, &cache, &b, &mut warnings).expect("mozaika");
        let lakes = terrain::lakes(&els, &b, &mosaic, p.terrain.resolution);
        let rwys = terrain::runways(&els, &b, &mosaic);
        let done = AtomicUsize::new(0);
        let cancel = AtomicBool::new(false);
        let t0 = std::time::Instant::now();
        let ter = terrain::compute(&p.terrain, &p.area, &mosaic, &lakes, &rwys, terrain::Progress { done: &done, cancel: &cancel }, warnings).expect("teren");
        let s = &ter.summary;
        println!("Teren: {} kafli, {:.1}–{:.1} m, {:?}, {:.1}s", s.tiles, s.min, s.max, s.sources, t0.elapsed().as_secs_f64());
        assert!(s.tiles > 0 && s.min > 180.0 && s.max < 280.0);
        let mut gen = lights::generate(&els, &p.area, &p.lights, &[], &[]);
        gen.lights.extend(lights::obstacles(&els, &p.area, &p.obstacles));
        println!("Światła: {} {:?}, modele: {}", gen.lights.len(), gen.counts, lights::count_models(&gen.lights));
        assert!(gen.lights.len() > 100);
        let veg = vegetation::polygons(&els, &p.area, &p.vegetation);
        let (png, _) = ter.preview_png(&p.area, 800).unwrap();
        std::fs::write(work.join("preview.png"), &png).unwrap();
        let thumb = package::thumbnail(Some(&png), &gen.lights, "x");
        let sum = package::write_sources(&work.join("msfs"), &p, Some(&ter), &gen.lights, &veg, thumb.as_deref(), true).expect("źródła");
        println!("Źródła: {:?}", sum);
        use base64::Engine;
        let ui = json!({
            "project": p,
            "lights": lights_value(&gen.lights),
            "preview3d": preview3d_value(&p.area, Some(&ter), &gen.lights),
            "terrainSummary": ter.summary,
            "terrainPreview": {"url": format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(&png)), "bbox": p.area.bbox()},
        });
        std::fs::write(work.join("ui-data.json"), serde_json::to_vec(&ui).unwrap()).unwrap();
    }
}
