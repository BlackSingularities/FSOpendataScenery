//! Wykrywanie Microsoft Flight Simulator 2024 (Steam i Microsoft Store), folderu Community,
//! MSFS 2024 SDK i stanu procesu symulatora.

use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Installation {
    pub id: String,
    pub label: String,
    /// "steam" albo "store".
    pub store: String,
    pub user_cfg: String,
    pub packages_path: String,
    /// Foldery Community czytane przez symulator (pierwszy – domyślny cel instalacji).
    pub communities: Vec<String>,
}

fn env_dir(name: &str) -> Option<PathBuf> {
    std::env::var_os(name).map(PathBuf::from)
}

/// Wartość `InstalledPackagesPath` z pliku `UserCfg.opt`.
pub fn parse_packages_path(cfg: &str) -> Option<String> {
    cfg.lines().find_map(|line| {
        let rest = line.trim().strip_prefix("InstalledPackagesPath")?;
        let value = rest.trim().trim_matches('"').trim();
        (!value.is_empty()).then(|| value.to_string())
    })
}

pub fn detect() -> Vec<Installation> {
    let mut cands: Vec<(&str, &str, &str, PathBuf)> = vec![];
    if let Some(local) = env_dir("LOCALAPPDATA") {
        cands.push(("msfs2024-store", "MSFS 2024 · Microsoft Store / Xbox", "store", local.join(r"Packages\Microsoft.Limitless_8wekyb3d8bbwe\LocalCache")));
    }
    if let Some(roaming) = env_dir("APPDATA") {
        cands.push(("msfs2024-steam", "MSFS 2024 · Steam", "steam", roaming.join("Microsoft Flight Simulator 2024")));
    }
    let mut out = vec![];
    for (id, label, store, base) in cands {
        let cfg_path = base.join("UserCfg.opt");
        let Ok(cfg) = std::fs::read_to_string(&cfg_path) else { continue };
        let Some(packages) = parse_packages_path(&cfg) else { continue };
        let packages = PathBuf::from(packages);
        let mut communities = vec![];
        // MSFS 2024 czyta Community i Community2024 – zaczynamy od tego, który istnieje.
        for name in ["Community", "Community2024"] {
            let p = packages.join(name);
            if p.is_dir() {
                communities.push(p.to_string_lossy().into_owned());
            }
        }
        if communities.is_empty() {
            communities.push(packages.join("Community").to_string_lossy().into_owned());
        }
        out.push(Installation {
            id: id.into(),
            label: label.into(),
            store: store.into(),
            user_cfg: cfg_path.to_string_lossy().into_owned(),
            packages_path: packages.to_string_lossy().into_owned(),
            communities,
        });
    }
    out
}

/// Folder Community, do którego instalujemy (ustawienie użytkownika albo pierwszy wykryty).
pub fn community(custom: &str) -> Option<PathBuf> {
    if !custom.trim().is_empty() {
        return Some(PathBuf::from(custom.trim()));
    }
    detect().into_iter().flat_map(|i| i.communities).map(PathBuf::from).next()
}

static RUNNING: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

/// Czy MSFS 2024 jest uruchomiony (wynik trzymamy 3 s).
pub fn is_running() -> bool {
    let mut guard = RUNNING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((at, value)) = *guard {
        if at.elapsed() < Duration::from_secs(3) {
            return value;
        }
    }
    let mut sys = sysinfo::System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let running = sys.processes().values().any(|p| p.name().to_string_lossy().eq_ignore_ascii_case("flightsimulator2024.exe"));
    *guard = Some((Instant::now(), running));
    running
}

// ───────────────────────────── SDK ─────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Sdk {
    pub root: String,
    pub package_tool: String,
    pub version: String,
}

fn sdk_at(root: &Path) -> Option<Sdk> {
    let tool = root.join("Tools").join("bin").join("fspackagetool.exe");
    if !tool.is_file() {
        return None;
    }
    let version = std::fs::read_to_string(root.join("version.txt")).map(|s| s.trim().to_string()).unwrap_or_default();
    Some(Sdk { root: root.to_string_lossy().into_owned(), package_tool: tool.to_string_lossy().into_owned(), version })
}

/// Szuka MSFS 2024 SDK: własna ścieżka, zmienna środowiskowa instalatora, typowe miejsca na dyskach.
pub fn find_sdk(custom: &str) -> Option<Sdk> {
    if !custom.trim().is_empty() {
        let p = PathBuf::from(custom.trim());
        // Użytkownik mógł wskazać sam plik fspackagetool.exe albo folder bin.
        for cand in [p.clone(), p.join("..").join(".."), p.join(".."), p.parent().and_then(|x| x.parent()).and_then(|x| x.parent()).map(Path::to_path_buf).unwrap_or_default()] {
            if let Some(s) = sdk_at(&cand) {
                return Some(s);
            }
        }
        return None;
    }
    for var in ["MSFS2024_SDK", "MSFS_SDK"] {
        if let Some(p) = env_dir(var) {
            if let Some(s) = sdk_at(&p) {
                return Some(s);
            }
        }
    }
    let drives = ['C', 'D', 'E', 'F', 'G', 'H', 'I'];
    for d in drives {
        for name in ["MSFS 2024 SDK", "MSFS2024 SDK", "MSFS 2024 SDK\\SDK", "Program Files\\MSFS 2024 SDK", "MSFS SDK"] {
            let p = PathBuf::from(format!("{d}:\\{name}"));
            if let Some(s) = sdk_at(&p) {
                return Some(s);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_installed_packages_path() {
        let cfg = "Version 59\n{Graphics\n}\nInstalledPackagesPath \"F:\\Microsoft Flight Simulator\"\n";
        assert_eq!(parse_packages_path(cfg).as_deref(), Some("F:\\Microsoft Flight Simulator"));
        assert_eq!(parse_packages_path("Version 1\n"), None);
    }

    #[test]
    fn sdk_layout() {
        let dir = std::env::temp_dir().join(format!("fsos-sdk-{}", crate::store::new_id()));
        std::fs::create_dir_all(dir.join("Tools").join("bin")).unwrap();
        std::fs::write(dir.join("Tools").join("bin").join("fspackagetool.exe"), b"MZ").unwrap();
        let s = find_sdk(dir.to_str().unwrap()).unwrap();
        assert!(s.package_tool.ends_with("fspackagetool.exe"));
        let tool = dir.join("Tools").join("bin").join("fspackagetool.exe");
        assert!(find_sdk(tool.to_str().unwrap()).is_some());
        let _ = std::fs::remove_dir_all(dir);
    }
}
