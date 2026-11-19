//! Projekt MSFS 2024 SDK: plik projektu, definicja pakietu, XML scenerii (teren, światła,
//! roślinność), biblioteka modeli i miniatura. Kompilacja przez `fspackagetool.exe`,
//! instalacja do Community (kopia albo łącze katalogu) i eksport do ZIP.

use crate::geo::LatLon;
use crate::lights::Light;
use crate::models;
use crate::store::{write_atomic, Project};
use crate::terrain::Terrain;
use crate::vegetation::VegPolygon;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Powierzchnia „bez zmian” używana w prostokątach z samą heightmapą.
const SURFACE_NONE: &str = "{47D48287-3ADE-4FC5-8BEC-B6B36901E612}";

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SourceSummary {
    pub work_dir: String,
    pub project_xml: String,
    pub package_name: String,
    pub rectangles: usize,
    pub lights: usize,
    pub models: usize,
    pub vegetation: usize,
    pub files: usize,
    pub bytes: u64,
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn heading180(h: f64) -> f64 {
    let h = h.rem_euclid(360.0);
    if h > 180.0 {
        h - 360.0
    } else {
        h
    }
}

pub fn package_name(p: &Project) -> String {
    let n = crate::store::package_slug(&p.package.name);
    if n.is_empty() {
        format!("fsos-scenery-{}", crate::store::package_slug(&p.name))
    } else {
        n
    }
}

fn version_of(p: &Project) -> String {
    let v = p.package.version.trim();
    let ok = v.split('.').count() == 3 && v.split('.').all(|x| x.parse::<u32>().is_ok());
    if ok {
        v.to_string()
    } else {
        "1.0.0".into()
    }
}

/// Zapis źródeł projektu SDK. Poprzednie źródła są usuwane (powstają od nowa).
pub fn write_sources(work: &Path, p: &Project, terrain: Option<&Terrain>, lights: &[Light], veg: &[VegPolygon], thumbnail: Option<&[u8]>, far_lod: bool) -> Result<SourceSummary, String> {
    let name = package_name(p);
    if name.len() < 3 {
        return Err("Nazwa pakietu jest za krótka".into());
    }
    for sub in ["PackageSources", "PackageDefinitions"] {
        let d = work.join(sub);
        if d.exists() {
            std::fs::remove_dir_all(&d).map_err(|e| format!("{}: {e}", d.display()))?;
        }
    }
    let mut sum = SourceSummary { work_dir: work.to_string_lossy().into_owned(), package_name: name.clone(), ..Default::default() };
    let write = |path: PathBuf, data: &[u8], sum: &mut SourceSummary| -> Result<(), String> {
        sum.files += 1;
        sum.bytes += data.len() as u64;
        write_atomic(&path, data)
    };
    let src = work.join("PackageSources");
    let scenery = src.join("scenery");

    // ── Teren ──
    if let (Some(t), true) = (terrain, p.terrain.enabled) {
        let n = t.grid.n;
        for (part, chunk) in t.tiles.chunks(300).enumerate() {
            let mut x = String::with_capacity(chunk.len() * n * n * 8 + 4096);
            x.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<FSData version=\"9.0\">\n");
            for tile in chunk {
                let b = &tile.bbox;
                let lonc = (b.west + b.east) / 2.0;
                let latc = (b.south + b.north) / 2.0;
                let width = crate::geo::haversine(LatLon::new(latc, b.west), LatLon::new(latc, b.east));
                let _ = write!(
                    x,
                    "\t<Rectangle displayName=\"fsos_terrain_{}_{}\" width=\"{:.4}\" falloff=\"{:.1}\" surface=\"{}\" priority=\"{}\" latitude=\"{:.12}\" longitude=\"{:.12}\" altitude=\"{:.3}\" latitude2=\"{:.12}\" longitude2=\"{:.12}\" altitude2=\"{:.3}\">\n\t\t<Heightmap width=\"{}\" data=\"",
                    tile.i, tile.j, width, p.terrain.falloff.max(0.0), SURFACE_NONE, p.terrain.priority, b.south, lonc, tile.mean, b.north, lonc, tile.mean, n
                );
                for (k, v) in tile.h.iter().enumerate() {
                    if k > 0 {
                        x.push(' ');
                    }
                    let _ = write!(x, "{:.2}", v);
                }
                x.push_str("\"/>\n\t</Rectangle>\n");
            }
            x.push_str("</FSData>\n");
            write(scenery.join(format!("terrain_{:03}.xml", part + 1)), x.as_bytes(), &mut sum)?;
            sum.rectangles += chunk.len();
        }
    }

    // ── Światła i modele ──
    let mut models: BTreeMap<String, models::Model> = BTreeMap::new();
    let mut by_cat: BTreeMap<String, Vec<&Light>> = BTreeMap::new();
    for l in lights {
        let key = l.spec.key();
        models.entry(key).or_insert_with(|| models::make(&l.spec, far_lod));
        by_cat.entry(l.cat.clone()).or_default().push(l);
    }
    for (cat, list) in &by_cat {
        let mut x = String::with_capacity(list.len() * 360 + 256);
        x.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<FSData version=\"9.0\">\n");
        for l in list {
            let m = &models[&l.spec.key()];
            let on_ground = l.alt <= 0.01;
            let _ = write!(
                x,
                "\t<SceneryObject displayName=\"{}_{}\" lat=\"{:.9}\" lon=\"{:.9}\" alt=\"{:.3}\" altitudeIsAgl=\"TRUE\" snapToGround=\"{}\" snapToNormal=\"FALSE\" pitch=\"0.000000\" bank=\"0.000000\" heading=\"{:.4}\" imageComplexity=\"VERY_SPARSE\">\n\t\t<LibraryObject name=\"{}\" scale=\"1.000000\"/>\n\t</SceneryObject>\n",
                esc(cat),
                l.osm,
                l.lat,
                l.lon,
                l.alt.max(0.0),
                if on_ground { "TRUE" } else { "FALSE" },
                heading180(l.heading),
                m.guid
            );
            sum.lights += 1;
        }
        x.push_str("</FSData>\n");
        let safe: String = cat.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        write(scenery.join(format!("lights_{safe}.xml")), x.as_bytes(), &mut sum)?;
    }
    for m in models.values() {
        let dir = src.join("modelLib").join(&m.name);
        write(dir.join(format!("{}.xml", m.name)), m.xml().as_bytes(), &mut sum)?;
        for (_, file, gltf, bin) in &m.lods {
            write(dir.join(format!("{file}.gltf")), gltf, &mut sum)?;
            write(dir.join(format!("{file}.bin")), bin, &mut sum)?;
        }
    }
    sum.models = models.len();

    // ── Roślinność ──
    if !veg.is_empty() {
        let mut x = String::from("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<FSData version=\"9.0\">\n");
        for v in veg {
            let id = format!("{{{}}}", uuid::Uuid::new_v5(&models::NAMESPACE, format!("veg|{}|{:.6}", v.name, v.ring[0].lat).as_bytes())).to_uppercase();
            let _ = writeln!(x, "\t<Polygon displayName=\"{}\" altitude=\"0.000\">", esc(&v.name));
            let _ = writeln!(x, "\t\t<Attribute name=\"UniqueGUID\" guid=\"{{359C73E8-06BE-4FB2-ABCB-EC942F7761D0}}\" type=\"GUID\" value=\"{id}\"/>");
            x.push_str("\t\t<Attribute name=\"VegetationScale\" guid=\"{6A043F59-E6F2-4117-A2E4-D510E7317C29}\" type=\"UINT32\" value=\"0\"/>\n");
            x.push_str("\t\t<Attribute name=\"VegetationDensity\" guid=\"{41EFF715-C392-4B31-A457-50A504353A90}\" type=\"UINT32\" value=\"0\"/>\n");
            x.push_str("\t\t<Attribute name=\"VegetationFalloff\" guid=\"{E82ABE17-FB4C-4F67-A28C-ED41969AEAD6}\" type=\"FLOAT32\" value=\"1.000\"/>\n");
            for q in &v.ring {
                let _ = writeln!(x, "\t\t<Vertex lat=\"{:.9}\" lon=\"{:.9}\"/>", q.lat, q.lon);
            }
            x.push_str("\t</Polygon>\n");
        }
        x.push_str("</FSData>\n");
        write(scenery.join("vegetation.xml"), x.as_bytes(), &mut sum)?;
        sum.vegetation = veg.len();
    }
    if sum.rectangles + sum.lights + sum.vegetation == 0 {
        return Err("Nie ma nic do zapisania – policz teren albo wygeneruj światła".into());
    }

    // ── Definicja pakietu, miniatura, projekt ──
    let has_scenery = sum.rectangles + sum.lights + sum.vegetation > 0;
    let mut groups = String::new();
    let group = |n: &str, t: &str, a: &str, o: &str| {
        format!("\t\t<AssetGroup Name=\"{n}\">\n\t\t\t<Type>{t}</Type>\n\t\t\t<Flags>\n\t\t\t\t<FSXCompatibility>false</FSXCompatibility>\n\t\t\t</Flags>\n\t\t\t<AssetDir>{a}</AssetDir>\n\t\t\t<OutputDir>{o}</OutputDir>\n\t\t</AssetGroup>\n")
    };
    groups.push_str(&group("ContentInfo", "ContentInfo", &format!("PackageDefinitions\\{name}\\ContentInfo\\"), &format!("ContentInfo\\{name}\\")));
    if sum.models > 0 {
        groups.push_str(&group("models", "ModelLib", "PackageSources\\modelLib\\", &format!("scenery\\{name}-models\\")));
    }
    if has_scenery {
        groups.push_str(&group("scenery", "BGL", "PackageSources\\scenery\\", &format!("scenery\\{name}\\")));
    }
    let creator = if p.package.creator.trim().is_empty() { "FS Opendata Scenery".to_string() } else { p.package.creator.trim().to_string() };
    let title = if p.package.title.trim().is_empty() { p.name.clone() } else { p.package.title.trim().to_string() };
    let def = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<AssetPackage Name=\"{name}\" Version=\"{ver}\">\n\t<ItemSettings>\n\t\t<ContentType>SCENERY</ContentType>\n\t\t<Title>{title}</Title>\n\t\t<Manufacturer/>\n\t\t<Creator>{creator}</Creator>\n\t</ItemSettings>\n\t<Flags>\n\t\t<VisibleInStore>false</VisibleInStore>\n\t\t<CanBeReferenced>false</CanBeReferenced>\n\t</Flags>\n\t<AssetGroups>\n{groups}\t</AssetGroups>\n</AssetPackage>\n",
        name = name,
        ver = version_of(p),
        title = esc(&title),
        creator = esc(&creator),
        groups = groups
    );
    let defs = work.join("PackageDefinitions");
    write(defs.join(format!("{name}.xml")), def.as_bytes(), &mut sum)?;
    if let Some(jpg) = thumbnail {
        write(defs.join(&name).join("ContentInfo").join("Thumbnail.jpg"), jpg, &mut sum)?;
    } else {
        std::fs::create_dir_all(defs.join(&name).join("ContentInfo")).map_err(|e| e.to_string())?;
    }
    let proj_name: String = name.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '-').collect();
    let project_xml = work.join(format!("{proj_name}.xml"));
    let proj = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<Project Version=\"2\" Name=\"{proj_name}\" FolderName=\"Packages\" MetadataFolderName=\"PackagesMetadata\">\n\t<OutputDirectory>.</OutputDirectory>\n\t<TemporaryOutputDirectory>_PackageInt</TemporaryOutputDirectory>\n\t<Packages>\n\t\t<Package>PackageDefinitions\\{name}.xml</Package>\n\t</Packages>\n</Project>\n"
    );
    write(project_xml.clone(), proj.as_bytes(), &mut sum)?;
    sum.project_xml = project_xml.to_string_lossy().into_owned();
    Ok(sum)
}

/// Miniatura 412×170 do Content Managera: podgląd terenu albo „nocna mapa” świateł.
pub fn thumbnail(preview_png: Option<&[u8]>, lights: &[Light], title: &str) -> Option<Vec<u8>> {
    let (w, h) = (412u32, 170u32);
    let mut img = image::RgbImage::from_pixel(w, h, image::Rgb([10, 16, 30]));
    if let Some(png) = preview_png.and_then(|b| image::load_from_memory(b).ok()) {
        let p = png.resize_to_fill(w, h, image::imageops::FilterType::Triangle).to_rgba8();
        for (x, y, px) in p.enumerate_pixels() {
            if px[3] > 0 {
                img.put_pixel(x, y, image::Rgb([px[0], px[1], px[2]]));
            }
        }
    } else if !lights.is_empty() {
        let pts: Vec<LatLon> = lights.iter().map(|l| LatLon::new(l.lat, l.lon)).collect();
        let b = crate::geo::BBox::of(&pts);
        let (sx, sy) = ((b.east - b.west).max(1e-6), (b.north - b.south).max(1e-6));
        for l in lights {
            let x = ((l.lon - b.west) / sx * (w as f64 - 20.0) + 10.0) as u32;
            let y = ((b.north - l.lat) / sy * (h as f64 - 20.0) + 10.0) as u32;
            let c = l.spec.color.unwrap_or_else(|| crate::geo::kelvin_rgb(l.spec.kelvin));
            for dy in 0..2 {
                for dx in 0..2 {
                    if x + dx < w && y + dy < h {
                        img.put_pixel(x + dx, y + dy, image::Rgb([(c[0] * 255.0) as u8, (c[1] * 230.0) as u8, (c[2] * 160.0) as u8]));
                    }
                }
            }
        }
    }
    let _ = title;
    let mut out = std::io::Cursor::new(vec![]);
    image::DynamicImage::ImageRgb8(img).write_to(&mut out, image::ImageFormat::Jpeg).ok()?;
    Some(out.into_inner())
}

/// Krótka ścieżka 8.3 (fspackagetool źle znosi spacje w ścieżce projektu).
pub fn short_path(p: &Path) -> PathBuf {
    let s = p.to_string_lossy();
    if !s.contains(' ') || !cfg!(windows) {
        return p.to_path_buf();
    }
    let mut cmd = std::process::Command::new("cmd");
    cmd.args(["/C", &format!("for %I in (\"{}\") do @echo %~sI", s)]);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    match cmd.output() {
        Ok(o) if o.status.success() => {
            let t = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !t.is_empty() && !t.contains(' ') {
                PathBuf::from(t)
            } else {
                p.to_path_buf()
            }
        }
        _ => p.to_path_buf(),
    }
}

/// Kopiowanie folderu (instalacja do Community).
pub fn copy_dir(from: &Path, to: &Path) -> Result<u64, String> {
    let mut bytes = 0;
    for e in walkdir::WalkDir::new(from).into_iter().filter_map(Result::ok) {
        let rel = e.path().strip_prefix(from).map_err(|e| e.to_string())?;
        let dst = to.join(rel);
        if e.file_type().is_dir() {
            std::fs::create_dir_all(&dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        } else {
            if let Some(d) = dst.parent() {
                std::fs::create_dir_all(d).map_err(|e| e.to_string())?;
            }
            bytes += std::fs::copy(e.path(), &dst).map_err(|e| format!("{}: {e}", dst.display()))?;
        }
    }
    Ok(bytes)
}

/// Czy ścieżka jest łączem katalogu (junction/symlink).
pub fn is_link(p: &Path) -> bool {
    std::fs::symlink_metadata(p).map(|m| m.file_type().is_symlink()).unwrap_or(false)
        || std::fs::read_link(p).is_ok()
}

pub fn remove_installed(target: &Path) -> Result<(), String> {
    if !target.exists() && std::fs::symlink_metadata(target).is_err() {
        return Ok(());
    }
    if is_link(target) {
        // Łącze katalogu usuwamy jak pusty folder – zawartość źródła zostaje.
        return std::fs::remove_dir(target).map_err(|e| format!("{}: {e}", target.display()));
    }
    std::fs::remove_dir_all(target).map_err(|e| format!("{}: {e}", target.display()))
}

pub fn make_link(target: &Path, source: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let out = std::process::Command::new("cmd")
            .creation_flags(0x0800_0000)
            .args(["/C", "mklink", "/J", &target.to_string_lossy(), &source.to_string_lossy()])
            .output()
            .map_err(|e| e.to_string())?;
        if out.status.success() {
            return Ok(());
        }
        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
    }
    #[cfg(not(windows))]
    {
        std::os::unix::fs::symlink(source, target).map_err(|e| e.to_string())
    }
}

/// Pakowanie folderu do ZIP (udostępnianie zbudowanej scenerii).
pub fn zip_dir(dir: &Path, out: &Path) -> Result<u64, String> {
    let f = std::fs::File::create(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let root = dir.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    for e in walkdir::WalkDir::new(dir).into_iter().filter_map(Result::ok) {
        let rel = e.path().strip_prefix(dir).map_err(|e| e.to_string())?;
        let name = format!("{}/{}", root, rel.to_string_lossy().replace('\\', "/"));
        if e.file_type().is_dir() {
            if !rel.as_os_str().is_empty() {
                z.add_directory(name, opts).map_err(|e| e.to_string())?;
            }
        } else {
            z.start_file(name, opts).map_err(|e| e.to_string())?;
            let data = std::fs::read(e.path()).map_err(|e| e.to_string())?;
            std::io::Write::write_all(&mut z, &data).map_err(|e| e.to_string())?;
        }
    }
    z.finish().map_err(|e| e.to_string())?;
    Ok(std::fs::metadata(out).map(|m| m.len()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lights::Spec;

    #[test]
    fn writes_complete_sdk_project() {
        let work = std::env::temp_dir().join(format!("fsos-pkg-{}", crate::store::new_id()));
        let mut p = Project { id: "x".into(), name: "Kraków test".into(), ..Default::default() };
        p.package.creator = "Tester <&>".into();
        let spec = Spec { style: "cobra".into(), height: 8.0, arm: 1.5, kelvin: 3000.0, color: None, intensity: 40.0, cone: 140.0, heads: 1, tilt: 0.0, flash: None, levels: vec![] };
        let lights: Vec<Light> = (0..3).map(|k| Light { lat: 50.0 + k as f64 * 1e-4, lon: 19.0, heading: 270.0, cat: "residential".into(), src: 'g', osm: 5, alt: 0.0, spec: spec.clone() }).collect();
        let veg = vec![VegPolygon { name: "road_1".into(), ring: vec![LatLon::new(50.0, 19.0), LatLon::new(50.0, 19.001), LatLon::new(50.001, 19.001)] }];
        let thumb = thumbnail(None, &lights, "t");
        let s = write_sources(&work, &p, None, &lights, &veg, thumb.as_deref(), true).unwrap();
        assert_eq!(s.package_name, "fsos-scenery-krakow-test");
        assert_eq!((s.lights, s.models, s.vegetation), (3, 1, 1));
        let def = std::fs::read_to_string(work.join("PackageDefinitions").join("fsos-scenery-krakow-test.xml")).unwrap();
        assert!(def.contains("<Type>ModelLib</Type>") && def.contains("<Type>BGL</Type>") && def.contains("Tester &lt;&amp;&gt;"));
        let lx = std::fs::read_to_string(work.join("PackageSources").join("scenery").join("lights_residential.xml")).unwrap();
        assert!(lx.contains("heading=\"-90.0000\"") && lx.contains("snapToGround=\"TRUE\""));
        assert!(work.join("PackageDefinitions").join("fsos-scenery-krakow-test").join("ContentInfo").join("Thumbnail.jpg").exists());
        assert!(std::path::Path::new(&s.project_xml).exists());
        let z = work.join("t.zip");
        assert!(zip_dir(&work.join("PackageSources"), &z).unwrap() > 0);
        let _ = std::fs::remove_dir_all(work);
    }
}
