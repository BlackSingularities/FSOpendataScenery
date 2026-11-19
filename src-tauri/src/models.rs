//! Proceduralne modele glTF dla MSFS: latarnie (z wysięgnikiem, parkowe, dwuramienne,
//! kinkiety), maszty z naświetlaczami, same źródła światła, światła przeszkodowe, latarnie
//! morskie i prosta turbina wiatrowa.
//!
//! Konwencje MSFS: metry, oś Y w górę, przód modelu = +Z (przy heading 0 – na północ).
//! Światło `ASOBO_macro_light` świeci wzdłuż lokalnej osi −Z węzła; obrót węzła ustawia
//! kierunek (np. w dół). Oprawy mają materiał świecący tylko nocą
//! (`ASOBO_material_day_night_switch`), a samo światło – `day_night_cycle`.

use crate::geo::kelvin_rgb;
use crate::lights::Spec;
use serde_json::{json, Value};

pub const NAMESPACE: uuid::Uuid = uuid::Uuid::from_u128(0x6f5d_3a2e_91c4_4b7a_9e21_5c0d_8f3b_a417);

#[derive(Clone, Copy)]
enum Mat {
    Pole,
    Housing,
    Lens,
    White,
    Red,
}

#[derive(Default)]
struct Mesh {
    pos: Vec<[f32; 3]>,
    nrm: Vec<[f32; 3]>,
    idx: Vec<u32>,
}

impl Mesh {
    fn add_quad(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
        let n = normal(a, b, c);
        let base = self.pos.len() as u32;
        self.pos.extend([a, b, c, d]);
        self.nrm.extend([n; 4]);
        self.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    fn add_tri(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3]) {
        let n = normal(a, b, c);
        let base = self.pos.len() as u32;
        self.pos.extend([a, b, c]);
        self.nrm.extend([n; 3]);
        self.idx.extend([base, base + 1, base + 2]);
    }

    /// Prostopadłościan wyrównany do osi.
    fn cuboid(&mut self, c: [f32; 3], s: [f32; 3]) {
        self.cuboid_t(c, s, |p| p);
    }

    /// Prostopadłościan po przekształceniu wierzchołków (obrót, przesunięcie).
    fn cuboid_t(&mut self, c: [f32; 3], s: [f32; 3], t: impl Fn([f32; 3]) -> [f32; 3]) {
        let (hx, hy, hz) = (s[0] / 2.0, s[1] / 2.0, s[2] / 2.0);
        let v = |x: f32, y: f32, z: f32| t([c[0] + x, c[1] + y, c[2] + z]);
        let p = [v(-hx, -hy, -hz), v(hx, -hy, -hz), v(hx, hy, -hz), v(-hx, hy, -hz), v(-hx, -hy, hz), v(hx, -hy, hz), v(hx, hy, hz), v(-hx, hy, hz)];
        self.add_quad(p[4], p[5], p[6], p[7]); // +Z
        self.add_quad(p[1], p[0], p[3], p[2]); // −Z
        self.add_quad(p[5], p[1], p[2], p[6]); // +X
        self.add_quad(p[0], p[4], p[7], p[3]); // −X
        self.add_quad(p[7], p[6], p[2], p[3]); // +Y
        self.add_quad(p[0], p[1], p[5], p[4]); // −Y
    }

    /// Rura (stożek ścięty) między dwoma punktami.
    fn tube(&mut self, a: [f32; 3], b: [f32; 3], r0: f32, r1: f32, seg: usize, caps: bool) {
        let axis = sub(b, a);
        let len = length(axis);
        if len < 1e-4 {
            return;
        }
        let w = scale(axis, 1.0 / len);
        let helper = if w[1].abs() < 0.9 { [0.0, 1.0, 0.0] } else { [1.0, 0.0, 0.0] };
        let u = normalize(cross(w, helper));
        let v = cross(w, u);
        let ring = |c: [f32; 3], r: f32, k: usize| {
            let ang = k as f32 / seg as f32 * std::f32::consts::TAU;
            add(c, add(scale(u, r * ang.cos()), scale(v, r * ang.sin())))
        };
        for k in 0..seg {
            let (p0, p1) = (ring(a, r0, k), ring(a, r0, k + 1));
            let (q0, q1) = (ring(b, r1, k), ring(b, r1, k + 1));
            // Gładkie cieniowanie: normalne promieniowe.
            let base = self.pos.len() as u32;
            let n0 = normalize(sub(p0, a));
            let n1 = normalize(sub(p1, a));
            self.pos.extend([p0, p1, q1, q0]);
            self.nrm.extend([n0, n1, n1, n0]);
            self.idx.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
            if caps {
                self.add_tri(b, q0, q1);
                self.add_tri(a, p1, p0);
            }
        }
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn add(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}
fn scale(a: [f32; 3], k: f32) -> [f32; 3] {
    [a[0] * k, a[1] * k, a[2] * k]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn length(a: [f32; 3]) -> f32 {
    dot(a, a).sqrt()
}
fn normalize(a: [f32; 3]) -> [f32; 3] {
    let l = length(a).max(1e-9);
    scale(a, 1.0 / l)
}
fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    normalize(cross(sub(b, a), sub(c, a)))
}

/// Obrót wokół osi Y (kąt w stopniach, dodatni = od +Z w stronę +X).
fn rot_y(p: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0] * c + p[2] * s, p[1], -p[0] * s + p[2] * c]
}

/// Obrót wokół osi X (kąt w stopniach, dodatni = od +Z w dół do −Y).
fn rot_x(p: [f32; 3], deg: f32) -> [f32; 3] {
    let (s, c) = deg.to_radians().sin_cos();
    [p[0], p[1] * c - p[2] * s, p[1] * s + p[2] * c]
}

/// Kwaternion obracający oś −Z na kierunek `d` (konwencja świateł MSFS).
pub fn quat_to(d: [f32; 3]) -> [f32; 4] {
    let a = [0.0f32, 0.0, -1.0];
    let b = normalize(d);
    let c = dot(a, b);
    if c < -0.9999 {
        return [0.0, 1.0, 0.0, 0.0];
    }
    let x = cross(a, b);
    let q = [x[0], x[1], x[2], 1.0 + c];
    let l = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3]).sqrt();
    [q[0] / l, q[1] / l, q[2] / l, q[3] / l]
}

struct LightNode {
    pos: [f32; 3],
    dir: [f32; 3],
}

/// Model gotowy do zapisu: nazwa, GUID i pliki LOD (gltf + bin).
pub struct Model {
    pub name: String,
    pub guid: String,
    pub lods: Vec<(f32, String, Vec<u8>, Vec<u8>)>,
}

impl Model {
    pub fn xml(&self) -> String {
        let mut s = format!("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ModelInfo guid=\"{}\" version=\"1.1\">\n\t<LODS>\n", self.guid);
        for (min, file, _, _) in &self.lods {
            s.push_str(&format!("\t\t<LOD minSize=\"{min:.1}\" ModelFile=\"{file}.gltf\"/>\n"));
        }
        s.push_str("\t</LODS>\n</ModelInfo>\n");
        s
    }
}

pub fn guid_for(spec: &Spec) -> String {
    format!("{{{}}}", uuid::Uuid::new_v5(&NAMESPACE, spec.key().as_bytes())).to_uppercase()
}

pub fn name_for(spec: &Spec) -> String {
    let id = uuid::Uuid::new_v5(&NAMESPACE, spec.key().as_bytes()).simple().to_string();
    format!("fsos_{}_{}", spec.style.replace('_', ""), &id[..10])
}

fn light_color(spec: &Spec) -> [f64; 3] {
    spec.color.unwrap_or_else(|| kelvin_rgb(spec.kelvin))
}

/// Buduje geometrię i światła modelu. `far` – uproszczona wersja do oglądania z daleka.
fn build(spec: &Spec, far: bool) -> (Vec<(Mat, Mesh)>, Vec<LightNode>) {
    let mut pole = Mesh::default();
    let mut housing = Mesh::default();
    let mut lens = Mesh::default();
    let mut white = Mesh::default();
    let mut red = Mesh::default();
    let mut lights = vec![];
    let h = spec.height as f32;
    let arm = spec.arm as f32;
    let down = [0.0, -1.0, 0.0];
    let seg = if far { 4 } else { 8 };

    match spec.style.as_str() {
        "cobra" | "double" => {
            let heads: &[f32] = if spec.style == "double" || spec.heads >= 2 { &[0.0, 180.0] } else { &[0.0] };
            let r0 = (0.07 + h * 0.008).min(0.16);
            if !far {
                pole.tube([0.0, 0.0, 0.0], [0.0, h - 0.15, 0.0], r0, r0 * 0.55, seg, true);
                pole.cuboid([0.0, 0.25, 0.0], [r0 * 3.2, 0.5, r0 * 3.2]);
            }
            for &yaw in heads {
                let tip = rot_y([0.0, h, arm.max(0.4)], yaw);
                if !far {
                    // Wysięgnik łukiem: od słupa lekko w górę, potem poziomo.
                    let mid = rot_y([0.0, h + 0.12, arm.max(0.4) * 0.45], yaw);
                    pole.tube([0.0, h - 0.35, 0.0], mid, r0 * 0.55, 0.035, 6, false);
                    pole.tube(mid, tip, 0.035, 0.03, 6, true);
                    housing.cuboid_t([0.0, h - 0.02, arm.max(0.4) + 0.3], [0.32, 0.11, 0.72], |p| rot_y(p, yaw));
                }
                lens.cuboid_t([0.0, h - 0.085, arm.max(0.4) + 0.3], [0.26, 0.02, 0.58], |p| rot_y(p, yaw));
                lights.push(LightNode { pos: rot_y([0.0, h - 0.12, arm.max(0.4) + 0.3], yaw), dir: down });
            }
        }
        "post_top" => {
            if !far {
                pole.tube([0.0, 0.0, 0.0], [0.0, h - 0.5, 0.0], 0.07, 0.05, seg, true);
                pole.cuboid([0.0, 0.2, 0.0], [0.22, 0.4, 0.22]);
                housing.tube([0.0, h - 0.5, 0.0], [0.0, h - 0.42, 0.0], 0.06, 0.22, seg, false);
                housing.tube([0.0, h - 0.05, 0.0], [0.0, h + 0.08, 0.0], 0.24, 0.05, seg, true);
            }
            lens.tube([0.0, h - 0.42, 0.0], [0.0, h - 0.05, 0.0], 0.2, 0.22, seg, false);
            lights.push(LightNode { pos: [0.0, h - 0.3, 0.0], dir: down });
        }
        "wall" => {
            // Model stoi na wysokości montażu (alt w scenerii) – sam kinkiet przy ścianie.
            if !far {
                pole.tube([0.0, 0.0, -0.05], [0.0, 0.1, arm.max(0.5)], 0.03, 0.03, 6, true);
                housing.cuboid([0.0, 0.05, arm.max(0.5) + 0.2], [0.28, 0.1, 0.5]);
            }
            lens.cuboid([0.0, -0.005, arm.max(0.5) + 0.2], [0.22, 0.02, 0.4]);
            lights.push(LightNode { pos: [0.0, -0.03, arm.max(0.5) + 0.2], dir: down });
        }
        "flood" => {
            if !far {
                pole.tube([0.0, 0.0, 0.0], [0.0, h, 0.0], 0.22, 0.12, seg, true);
                pole.cuboid([0.0, h + 0.1, 0.25], [1.8, 0.12, 0.7]);
            }
            let n = spec.heads.max(1) as usize;
            let tilt = spec.tilt as f32;
            for k in 0..n {
                let off = if n == 1 { 0.0 } else { (k as f32 / (n - 1) as f32 - 0.5) * 1.4 };
                let yaw = off * 12.0;
                let c = [off, h + 0.45, 0.45];
                let t = |p: [f32; 3]| {
                    let local = sub(p, c);
                    add(c, rot_y(rot_x(local, tilt), yaw))
                };
                if !far {
                    housing.cuboid_t(c, [0.55, 0.45, 0.18], t);
                }
                lens.cuboid_t([c[0], c[1], c[2] + 0.1], [0.46, 0.36, 0.02], t);
                let dir = rot_y(rot_x([0.0, 0.0, 1.0], tilt), yaw);
                lights.push(LightNode { pos: add(c, scale(dir, 0.15)), dir });
            }
        }
        "obstruction" | "beacon" | "none" => {
            let s = if spec.style == "beacon" { 0.6 } else { 0.18 };
            if spec.color.is_some() {
                red.cuboid([0.0, 0.0, 0.0], [s, s, s]);
            } else {
                lens.cuboid([0.0, 0.0, 0.0], [0.3, 0.03, 0.3]);
            }
            let dir = if spec.cone >= 300.0 { [0.0, 0.0, 1.0] } else { down };
            lights.push(LightNode { pos: [0.0, -0.05, 0.0], dir });
            for l in &spec.levels {
                red.cuboid([0.0, *l as f32, 0.0], [s, s, s]);
                lights.push(LightNode { pos: [0.0, *l as f32, 0.0], dir });
            }
        }
        "turbine" => {
            let rotor = arm.max(10.0);
            if !far {
                white.tube([0.0, 0.0, 0.0], [0.0, h - 1.5, 0.0], (h * 0.022).max(1.6), (h * 0.012).max(1.0), 14, true);
                white.cuboid([0.0, h, -1.5], [3.2, 3.4, 10.0]);
                white.tube([0.0, h, 3.4], [0.0, h, 5.6], 1.4, 0.2, 10, true);
                for k in 0..3 {
                    let ang = 90.0 + k as f32 * 120.0;
                    let (s, c) = ang.to_radians().sin_cos();
                    let root = [0.0, h, 4.0];
                    let tipp = [c * rotor, h + s * rotor, 4.2];
                    let mid = [c * rotor * 0.25, h + s * rotor * 0.25, 4.1];
                    white.tube(root, mid, 0.9, 1.1, 6, false);
                    white.tube(mid, tipp, 1.1, 0.15, 6, true);
                }
            }
            red.cuboid([0.0, h + 1.9, -4.0], [0.4, 0.4, 0.4]);
            lights.push(LightNode { pos: [0.0, h + 1.9, -4.0], dir: [0.0, 0.0, 1.0] });
        }
        _ => {
            lens.cuboid([0.0, h, 0.0], [0.3, 0.05, 0.3]);
            lights.push(LightNode { pos: [0.0, h - 0.1, 0.0], dir: down });
        }
    }
    let mut out = vec![];
    for (m, mesh) in [(Mat::Pole, pole), (Mat::Housing, housing), (Mat::Lens, lens), (Mat::White, white), (Mat::Red, red)] {
        if !mesh.idx.is_empty() {
            out.push((m, mesh));
        }
    }
    (out, lights)
}

fn material(m: Mat, light: [f64; 3]) -> Value {
    match m {
        Mat::Pole => json!({"name": "Pole", "pbrMetallicRoughness": {"baseColorFactor": [0.42, 0.44, 0.46, 1.0], "metallicFactor": 0.6, "roughnessFactor": 0.45}}),
        Mat::Housing => json!({"name": "Housing", "pbrMetallicRoughness": {"baseColorFactor": [0.13, 0.14, 0.15, 1.0], "metallicFactor": 0.3, "roughnessFactor": 0.5}}),
        Mat::White => json!({"name": "White", "pbrMetallicRoughness": {"baseColorFactor": [0.86, 0.87, 0.88, 1.0], "metallicFactor": 0.1, "roughnessFactor": 0.4}}),
        Mat::Lens => json!({
            "name": "Lens",
            "pbrMetallicRoughness": {"baseColorFactor": [0.9, 0.9, 0.86, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.2},
            "emissiveFactor": light,
            "extensions": {"ASOBO_material_day_night_switch": {}, "ASOBO_material_shadow_options": {"noCastShadow": true}}
        }),
        Mat::Red => json!({
            "name": "RedLamp",
            "pbrMetallicRoughness": {"baseColorFactor": [0.6, 0.05, 0.03, 1.0], "metallicFactor": 0.0, "roughnessFactor": 0.3},
            "emissiveFactor": light,
            "extensions": {"ASOBO_material_day_night_switch": {}, "ASOBO_material_shadow_options": {"noCastShadow": true}}
        }),
    }
}

fn pad4(buf: &mut Vec<u8>) {
    while buf.len() % 4 != 0 {
        buf.push(0);
    }
}

/// Zapis glTF 2.0 (JSON + osobny plik .bin).
fn write_gltf(name: &str, spec: &Spec, far: bool) -> (Vec<u8>, Vec<u8>) {
    let (meshes, lights) = build(spec, far);
    let color = light_color(spec);
    let mut bin: Vec<u8> = vec![];
    let mut views = vec![];
    let mut accessors = vec![];
    let mut prims = vec![];
    let mut materials = vec![];
    for (k, (m, mesh)) in meshes.iter().enumerate() {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &mesh.pos {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        let mut push_view = |data: &[u8], target: u32, bin: &mut Vec<u8>| -> usize {
            pad4(bin);
            let off = bin.len();
            bin.extend_from_slice(data);
            views.push(json!({"buffer": 0, "byteOffset": off, "byteLength": data.len(), "target": target}));
            views.len() - 1
        };
        let pos: Vec<u8> = mesh.pos.iter().flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes())).collect();
        let nrm: Vec<u8> = mesh.nrm.iter().flat_map(|p| p.iter().flat_map(|v| v.to_le_bytes())).collect();
        let uv: Vec<u8> = mesh.pos.iter().flat_map(|p| [p[0] * 0.5, p[1] * 0.5].into_iter().flat_map(|v| v.to_le_bytes())).collect();
        let big = mesh.pos.len() > 65_535;
        let idx: Vec<u8> = if big { mesh.idx.iter().flat_map(|v| v.to_le_bytes()).collect() } else { mesh.idx.iter().flat_map(|v| (*v as u16).to_le_bytes()).collect() };
        let vp = push_view(&pos, 34962, &mut bin);
        let vn = push_view(&nrm, 34962, &mut bin);
        let vu = push_view(&uv, 34962, &mut bin);
        let vi = push_view(&idx, 34963, &mut bin);
        let n = mesh.pos.len();
        let a = accessors.len();
        accessors.push(json!({"bufferView": vp, "componentType": 5126, "count": n, "type": "VEC3", "min": lo, "max": hi}));
        accessors.push(json!({"bufferView": vn, "componentType": 5126, "count": n, "type": "VEC3"}));
        accessors.push(json!({"bufferView": vu, "componentType": 5126, "count": n, "type": "VEC2"}));
        accessors.push(json!({"bufferView": vi, "componentType": if big { 5125 } else { 5123 }, "count": mesh.idx.len(), "type": "SCALAR"}));
        materials.push(material(*m, color));
        prims.push(json!({"attributes": {"POSITION": a, "NORMAL": a + 1, "TEXCOORD_0": a + 2}, "indices": a + 3, "material": k}));
    }
    pad4(&mut bin);
    let mut nodes = vec![json!({"name": format!("{name}_mesh"), "mesh": 0})];
    let (freq, dur) = spec.flash.unwrap_or((0.0, 0.0));
    for (k, l) in lights.iter().enumerate() {
        let q = quat_to(l.dir);
        nodes.push(json!({
            "name": format!("{name}_light{k}"),
            "translation": l.pos,
            "rotation": q,
            "extensions": {"ASOBO_macro_light": {
                "color": color,
                "intensity": spec.intensity,
                "cone_angle": spec.cone.clamp(1.0, 360.0),
                "has_symmetry": false,
                "flash_frequency": freq,
                "flash_duration": dur,
                "flash_phase": 0.0,
                "rotation_speed": 0.0,
                "day_night_cycle": true
            }}
        }));
    }
    let root_children: Vec<usize> = (1..=nodes.len()).collect();
    let mut all_nodes = vec![json!({"name": name, "children": root_children})];
    all_nodes.extend(nodes);
    let doc = json!({
        "asset": {"version": "2.0", "generator": concat!("FS Opendata Scenery ", env!("CARGO_PKG_VERSION")), "extensions": {"ASOBO_normal_map_convention": {"tangent_space_convention": "DirectX"}}},
        "extensionsUsed": ["ASOBO_normal_map_convention", "ASOBO_macro_light", "ASOBO_material_day_night_switch", "ASOBO_material_shadow_options"],
        "scene": 0,
        "scenes": [{"nodes": [0]}],
        "nodes": all_nodes,
        "meshes": [{"name": format!("{name}_mesh"), "primitives": prims}],
        "materials": materials,
        "accessors": accessors,
        "bufferViews": views,
        "buffers": [{"byteLength": bin.len(), "uri": format!("{name}.bin")}]
    });
    (serde_json::to_vec_pretty(&doc).unwrap_or_default(), bin)
}

/// Model z poziomami szczegółów: LOD0 pełny, LOD1 (z daleka) tylko oprawy i światło.
pub fn make(spec: &Spec, far_lod: bool) -> Model {
    let spec = spec.quantized();
    let name = name_for(&spec);
    let guid = guid_for(&spec);
    let mut lods = vec![];
    let simple = matches!(spec.style.as_str(), "obstruction" | "beacon" | "none");
    let l0 = format!("{name}_LOD0");
    let (g0, b0) = write_gltf(&l0, &spec, false);
    lods.push((if simple || !far_lod { 0.0 } else { 6.0 }, l0, g0, b0));
    if far_lod && !simple {
        let l1 = format!("{name}_LOD1");
        let (g1, b1) = write_gltf(&l1, &spec, true);
        lods.push((0.0, l1, g1, b1));
    }
    Model { name, guid, lods }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(style: &str) -> Spec {
        Spec { style: style.into(), height: 8.0, arm: 1.5, kelvin: 3000.0, color: None, intensity: 40.0, cone: 140.0, heads: 3, tilt: 30.0, flash: None, levels: vec![] }
    }

    #[test]
    fn down_quaternion_matches_asobo_convention() {
        // Obrót −Z → −Y: dokładnie jak światła w modelach eksportowanych dla MSFS.
        let q = quat_to([0.0, -1.0, 0.0]);
        let v = rotate([0.0, 0.0, -1.0], q);
        assert!((v[1] + 1.0).abs() < 1e-5, "{v:?}");
        let q2 = quat_to([0.0, 0.0, -1.0]);
        assert!((q2[3] - 1.0).abs() < 1e-6);
    }

    fn rotate(v: [f32; 3], q: [f32; 4]) -> [f32; 3] {
        let u = [q[0], q[1], q[2]];
        let s = q[3];
        let a = scale(u, 2.0 * dot(u, v));
        let b = scale(v, s * s - dot(u, u));
        let c = scale(cross(u, v), 2.0 * s);
        add(add(a, b), c)
    }

    #[test]
    fn all_styles_produce_valid_gltf() {
        for style in ["cobra", "double", "post_top", "wall", "flood", "none", "obstruction", "beacon", "turbine"] {
            let mut s = spec(style);
            if style == "obstruction" || style == "turbine" {
                s.color = Some([1.0, 0.0, 0.0]);
                s.flash = Some((0.67, 0.5));
            }
            let m = make(&s, true);
            assert!(m.guid.starts_with('{') && m.guid.ends_with('}'));
            for (_, file, gltf, bin) in &m.lods {
                let v: serde_json::Value = serde_json::from_slice(gltf).unwrap();
                assert_eq!(v["buffers"][0]["byteLength"].as_u64().unwrap() as usize, bin.len(), "{style}");
                assert_eq!(v["buffers"][0]["uri"].as_str().unwrap(), format!("{file}.bin"));
                assert!(!v["meshes"][0]["primitives"].as_array().unwrap().is_empty(), "{style}");
                let lights = v["nodes"].as_array().unwrap().iter().filter(|n| n["extensions"]["ASOBO_macro_light"].is_object()).count();
                assert!(lights >= 1, "{style}");
                // Wszystkie widoki bufora mieszczą się w buforze i są wyrównane do 4 bajtów.
                for bv in v["bufferViews"].as_array().unwrap() {
                    let off = bv["byteOffset"].as_u64().unwrap() as usize;
                    let len = bv["byteLength"].as_u64().unwrap() as usize;
                    assert!(off % 4 == 0 && off + len <= bin.len());
                }
            }
            assert!(m.xml().contains(&m.guid));
        }
        let flood = make(&spec("flood"), false);
        let v: serde_json::Value = serde_json::from_slice(&flood.lods[0].2).unwrap();
        assert_eq!(v["nodes"].as_array().unwrap().iter().filter(|n| n["extensions"]["ASOBO_macro_light"].is_object()).count(), 3);
        assert_eq!(make(&spec("cobra"), true).guid, make(&spec("cobra"), true).guid);
    }
}
