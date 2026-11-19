//! Geometria na kuli ziemskiej i w lokalnym układzie metrycznym: odległości, azymuty,
//! przesunięcia, wielokąty (pole, zawieranie, przecięcia z prostokątem), linie (próbkowanie,
//! odsunięcie na bok) i siatka przestrzenna do szybkiego wyszukiwania sąsiadów.

use serde::{Deserialize, Serialize};

pub const R_EARTH: f64 = 6_371_008.8;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

impl LatLon {
    pub const fn new(lat: f64, lon: f64) -> Self {
        Self { lat, lon }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct BBox {
    pub south: f64,
    pub west: f64,
    pub north: f64,
    pub east: f64,
}

impl BBox {
    pub fn empty() -> Self {
        Self { south: f64::INFINITY, west: f64::INFINITY, north: f64::NEG_INFINITY, east: f64::NEG_INFINITY }
    }
    pub fn of(points: &[LatLon]) -> Self {
        let mut b = Self::empty();
        for p in points {
            b.add(*p);
        }
        b
    }
    pub fn add(&mut self, p: LatLon) {
        self.south = self.south.min(p.lat);
        self.north = self.north.max(p.lat);
        self.west = self.west.min(p.lon);
        self.east = self.east.max(p.lon);
    }
    pub fn center(&self) -> LatLon {
        LatLon::new((self.south + self.north) / 2.0, (self.west + self.east) / 2.0)
    }
    pub fn contains(&self, p: LatLon) -> bool {
        p.lat >= self.south && p.lat <= self.north && p.lon >= self.west && p.lon <= self.east
    }
    pub fn intersects(&self, o: &BBox) -> bool {
        !(o.west > self.east || o.east < self.west || o.south > self.north || o.north < self.south)
    }
    /// Powiększenie o margines w metrach.
    pub fn grow_m(&self, m: f64) -> Self {
        let dlat = m / R_EARTH * 180.0 / std::f64::consts::PI;
        let c = self.center().lat.to_radians().cos().max(0.01);
        let dlon = dlat / c;
        Self { south: self.south - dlat, north: self.north + dlat, west: self.west - dlon, east: self.east + dlon }
    }
    pub fn width_m(&self) -> f64 {
        let c = self.center();
        haversine(LatLon::new(c.lat, self.west), LatLon::new(c.lat, self.east))
    }
    pub fn height_m(&self) -> f64 {
        haversine(LatLon::new(self.south, self.west), LatLon::new(self.north, self.west))
    }
}

pub fn haversine(a: LatLon, b: LatLon) -> f64 {
    let p1 = a.lat.to_radians();
    let p2 = b.lat.to_radians();
    let dp = (b.lat - a.lat).to_radians();
    let dl = (b.lon - a.lon).to_radians();
    let h = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * R_EARTH * h.sqrt().atan2((1.0 - h).sqrt())
}

/// Azymut (0–360°, zgodnie z ruchem wskazówek zegara od północy) z `a` do `b`.
pub fn bearing(a: LatLon, b: LatLon) -> f64 {
    let p1 = a.lat.to_radians();
    let p2 = b.lat.to_radians();
    let dl = (b.lon - a.lon).to_radians();
    let y = dl.sin() * p2.cos();
    let x = p1.cos() * p2.sin() - p1.sin() * p2.cos() * dl.cos();
    (y.atan2(x).to_degrees() + 360.0) % 360.0
}

/// Punkt odległy o `dist` metrów w kierunku `brg` stopni.
pub fn destination(p: LatLon, brg: f64, dist: f64) -> LatLon {
    let d = dist / R_EARTH;
    let b = brg.to_radians();
    let p1 = p.lat.to_radians();
    let l1 = p.lon.to_radians();
    let p2 = (p1.sin() * d.cos() + p1.cos() * d.sin() * b.cos()).asin();
    let l2 = l1 + (b.sin() * d.sin() * p1.cos()).atan2(d.cos() - p1.sin() * p2.sin());
    LatLon::new(p2.to_degrees(), ((l2.to_degrees() + 540.0) % 360.0) - 180.0)
}

/// Lokalny układ metryczny (rzut równoodległościowy wokół punktu) – wystarczająco dokładny
/// dla obszarów do kilkudziesięciu kilometrów.
#[derive(Debug, Clone, Copy)]
pub struct Local {
    pub origin: LatLon,
    kx: f64,
    ky: f64,
}

impl Local {
    pub fn new(origin: LatLon) -> Self {
        let ky = R_EARTH * std::f64::consts::PI / 180.0;
        let kx = ky * origin.lat.to_radians().cos();
        Self { origin, kx, ky }
    }
    /// (x na wschód, y na północ) w metrach.
    pub fn to_xy(self, p: LatLon) -> (f64, f64) {
        ((p.lon - self.origin.lon) * self.kx, (p.lat - self.origin.lat) * self.ky)
    }
    pub fn to_ll(self, x: f64, y: f64) -> LatLon {
        LatLon::new(self.origin.lat + y / self.ky, self.origin.lon + x / self.kx)
    }
}

/// Pole wielokąta w m² (rzut lokalny).
pub fn polygon_area(poly: &[LatLon]) -> f64 {
    if poly.len() < 3 {
        return 0.0;
    }
    let loc = Local::new(BBox::of(poly).center());
    let pts: Vec<(f64, f64)> = poly.iter().map(|p| loc.to_xy(*p)).collect();
    let mut s = 0.0;
    for i in 0..pts.len() {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % pts.len()];
        s += x1 * y2 - x2 * y1;
    }
    (s / 2.0).abs()
}

pub fn polygon_centroid(poly: &[LatLon]) -> LatLon {
    if poly.is_empty() {
        return LatLon::default();
    }
    let loc = Local::new(BBox::of(poly).center());
    let pts: Vec<(f64, f64)> = poly.iter().map(|p| loc.to_xy(*p)).collect();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for i in 0..pts.len() {
        let (x1, y1) = pts[i];
        let (x2, y2) = pts[(i + 1) % pts.len()];
        let c = x1 * y2 - x2 * y1;
        a += c;
        cx += (x1 + x2) * c;
        cy += (y1 + y2) * c;
    }
    if a.abs() < 1e-9 {
        let n = pts.len() as f64;
        let (sx, sy) = pts.iter().fold((0.0, 0.0), |s, p| (s.0 + p.0, s.1 + p.1));
        return loc.to_ll(sx / n, sy / n);
    }
    loc.to_ll(cx / (3.0 * a), cy / (3.0 * a))
}

/// Czy punkt leży w wielokącie (zasada parzystości, w stopniach – wystarczy dla małych obszarów).
pub fn point_in_polygon(p: LatLon, poly: &[LatLon]) -> bool {
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut inside = false;
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.lat > p.lat) != (b.lat > p.lat) {
            let x = (b.lon - a.lon) * (p.lat - a.lat) / (b.lat - a.lat) + a.lon;
            if p.lon < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Wielokąt z dziurami (pierścienie wewnętrzne).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Area {
    pub outer: Vec<LatLon>,
    #[serde(default)]
    pub holes: Vec<Vec<LatLon>>,
}

impl Area {
    pub fn contains(&self, p: LatLon) -> bool {
        point_in_polygon(p, &self.outer) && !self.holes.iter().any(|h| point_in_polygon(p, h))
    }
    pub fn bbox(&self) -> BBox {
        BBox::of(&self.outer)
    }
    pub fn area_m2(&self) -> f64 {
        polygon_area(&self.outer) - self.holes.iter().map(|h| polygon_area(h)).sum::<f64>()
    }
}

/// Czy wielokąt przecina prostokąt (w stopniach): wierzchołek w środku, róg prostokąta
/// w wielokącie albo przecięcie krawędzi.
pub fn polygon_intersects_bbox(poly: &[LatLon], b: &BBox) -> bool {
    if poly.iter().any(|p| b.contains(*p)) {
        return true;
    }
    let corners = [
        LatLon::new(b.south, b.west),
        LatLon::new(b.south, b.east),
        LatLon::new(b.north, b.east),
        LatLon::new(b.north, b.west),
    ];
    if corners.iter().any(|c| point_in_polygon(*c, poly)) {
        return true;
    }
    for i in 0..poly.len() {
        let a = poly[i];
        let c = poly[(i + 1) % poly.len()];
        for k in 0..4 {
            if segments_cross(a, c, corners[k], corners[(k + 1) % 4]) {
                return true;
            }
        }
    }
    false
}

fn orient(a: LatLon, b: LatLon, c: LatLon) -> f64 {
    (b.lon - a.lon) * (c.lat - a.lat) - (b.lat - a.lat) * (c.lon - a.lon)
}

pub fn segments_cross(a: LatLon, b: LatLon, c: LatLon, d: LatLon) -> bool {
    let o1 = orient(a, b, c);
    let o2 = orient(a, b, d);
    let o3 = orient(c, d, a);
    let o4 = orient(c, d, b);
    (o1 > 0.0) != (o2 > 0.0) && (o3 > 0.0) != (o4 > 0.0)
}

/// Długość linii w metrach.
pub fn line_length(line: &[LatLon]) -> f64 {
    line.windows(2).map(|w| haversine(w[0], w[1])).sum()
}

/// Punkt na linii w odległości `s` od początku wraz z azymutem odcinka.
pub fn point_along(line: &[LatLon], s: f64) -> Option<(LatLon, f64)> {
    let mut acc = 0.0;
    for w in line.windows(2) {
        let len = haversine(w[0], w[1]);
        if len <= 0.0 {
            continue;
        }
        if acc + len >= s {
            let t = ((s - acc) / len).clamp(0.0, 1.0);
            let p = LatLon::new(w[0].lat + (w[1].lat - w[0].lat) * t, w[0].lon + (w[1].lon - w[0].lon) * t);
            return Some((p, bearing(w[0], w[1])));
        }
        acc += len;
    }
    let n = line.len();
    (n >= 2).then(|| (line[n - 1], bearing(line[n - 2], line[n - 1])))
}

/// Najbliższy punkt odcinka AB do P (w lokalnym układzie) i odległość w metrach.
pub fn nearest_on_segment(loc: &Local, p: LatLon, a: LatLon, b: LatLon) -> (LatLon, f64) {
    let (px, py) = loc.to_xy(p);
    let (ax, ay) = loc.to_xy(a);
    let (bx, by) = loc.to_xy(b);
    let (dx, dy) = (bx - ax, by - ay);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((px - ax) * dx + (py - ay) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (qx, qy) = (ax + dx * t, ay + dy * t);
    (loc.to_ll(qx, qy), ((px - qx).powi(2) + (py - qy).powi(2)).sqrt())
}

/// Bufor wokół linii jako wielokąt (lewa krawędź + odwrócona prawa, ze ściętymi narożami).
pub fn buffer_line(line: &[LatLon], half_width: f64) -> Vec<LatLon> {
    if line.len() < 2 || half_width <= 0.0 {
        return vec![];
    }
    let loc = Local::new(line[0]);
    let pts: Vec<(f64, f64)> = line.iter().map(|p| loc.to_xy(*p)).collect();
    let mut left = vec![];
    let mut right = vec![];
    for i in 0..pts.len() {
        let dir_in = if i > 0 { norm((pts[i].0 - pts[i - 1].0, pts[i].1 - pts[i - 1].1)) } else { None };
        let dir_out = if i + 1 < pts.len() { norm((pts[i + 1].0 - pts[i].0, pts[i + 1].1 - pts[i].1)) } else { None };
        let d = match (dir_in, dir_out) {
            (Some(a), Some(b)) => norm((a.0 + b.0, a.1 + b.1)).unwrap_or(a),
            (Some(a), None) => a,
            (None, Some(b)) => b,
            (None, None) => continue,
        };
        // Wydłużenie w narożu (miter), ograniczone, żeby ostre kąty nie strzelały w dal.
        let miter = match (dir_in, dir_out) {
            (Some(a), Some(_)) => {
                let cos = (a.0 * d.0 + a.1 * d.1).abs().max(0.35);
                half_width / cos
            }
            _ => half_width,
        };
        let n = (-d.1, d.0);
        left.push(loc.to_ll(pts[i].0 + n.0 * miter, pts[i].1 + n.1 * miter));
        right.push(loc.to_ll(pts[i].0 - n.0 * miter, pts[i].1 - n.1 * miter));
    }
    right.reverse();
    left.extend(right);
    left
}

fn norm(v: (f64, f64)) -> Option<(f64, f64)> {
    let l = (v.0 * v.0 + v.1 * v.1).sqrt();
    (l > 1e-9).then(|| (v.0 / l, v.1 / l))
}

/// Uproszczenie linii algorytmem Douglasa-Peuckera (tolerancja w metrach).
pub fn simplify(line: &[LatLon], tol: f64) -> Vec<LatLon> {
    if line.len() < 3 {
        return line.to_vec();
    }
    let loc = Local::new(line[0]);
    let mut keep = vec![false; line.len()];
    keep[0] = true;
    keep[line.len() - 1] = true;
    let mut stack = vec![(0usize, line.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let mut best = (0.0, 0usize);
        for i in a + 1..b {
            let (_, d) = nearest_on_segment(&loc, line[i], line[a], line[b]);
            if d > best.0 {
                best = (d, i);
            }
        }
        if best.0 > tol {
            keep[best.1] = true;
            stack.push((a, best.1));
            stack.push((best.1, b));
        }
    }
    line.iter().zip(keep).filter(|(_, k)| *k).map(|(p, _)| *p).collect()
}

/// Siatka przestrzenna punktów do szybkiego sprawdzania „czy w promieniu R jest już coś”.
pub struct PointGrid {
    loc: Local,
    cell: f64,
    cells: std::collections::HashMap<(i64, i64), Vec<(f64, f64)>>,
}

impl PointGrid {
    pub fn new(origin: LatLon, cell: f64) -> Self {
        Self { loc: Local::new(origin), cell: cell.max(1.0), cells: Default::default() }
    }
    fn key(&self, x: f64, y: f64) -> (i64, i64) {
        ((x / self.cell).floor() as i64, (y / self.cell).floor() as i64)
    }
    pub fn insert(&mut self, p: LatLon) {
        let (x, y) = self.loc.to_xy(p);
        let k = self.key(x, y);
        self.cells.entry(k).or_default().push((x, y));
    }
    /// Odległość do najbliższego punktu, jeśli jest bliżej niż `r`.
    pub fn nearest_within(&self, p: LatLon, r: f64) -> Option<f64> {
        let (x, y) = self.loc.to_xy(p);
        let (cx, cy) = self.key(x, y);
        let span = (r / self.cell).ceil() as i64;
        let mut best: Option<f64> = None;
        for gx in cx - span..=cx + span {
            for gy in cy - span..=cy + span {
                if let Some(v) = self.cells.get(&(gx, gy)) {
                    for (px, py) in v {
                        let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
                        if d <= r && best.map(|b| d < b).unwrap_or(true) {
                            best = Some(d);
                        }
                    }
                }
            }
        }
        best
    }
}

/// Indeks odcinków (np. dróg) do wyszukiwania najbliższego odcinka.
pub struct SegmentIndex<T: Clone> {
    loc: Local,
    cell: f64,
    cells: std::collections::HashMap<(i64, i64), Vec<usize>>,
    segs: Vec<(LatLon, LatLon, T)>,
}

impl<T: Clone> SegmentIndex<T> {
    pub fn new(origin: LatLon, cell: f64) -> Self {
        Self { loc: Local::new(origin), cell: cell.max(5.0), cells: Default::default(), segs: vec![] }
    }
    pub fn insert_line(&mut self, line: &[LatLon], data: T) {
        for w in line.windows(2) {
            let idx = self.segs.len();
            self.segs.push((w[0], w[1], data.clone()));
            let (ax, ay) = self.loc.to_xy(w[0]);
            let (bx, by) = self.loc.to_xy(w[1]);
            let (x0, x1) = (ax.min(bx), ax.max(bx));
            let (y0, y1) = (ay.min(by), ay.max(by));
            let (kx0, ky0) = ((x0 / self.cell).floor() as i64, (y0 / self.cell).floor() as i64);
            let (kx1, ky1) = ((x1 / self.cell).floor() as i64, (y1 / self.cell).floor() as i64);
            // Długie odcinki (autostrady w polu) mogą przejść przez wiele komórek – ograniczamy.
            if (kx1 - kx0 + 1) * (ky1 - ky0 + 1) > 4000 {
                continue;
            }
            for kx in kx0..=kx1 {
                for ky in ky0..=ky1 {
                    self.cells.entry((kx, ky)).or_default().push(idx);
                }
            }
        }
    }
    /// Najbliższy odcinek w promieniu `r`: (punkt na odcinku, odległość, azymut odcinka, dane).
    pub fn nearest(&self, p: LatLon, r: f64) -> Option<(LatLon, f64, f64, T)> {
        let (x, y) = self.loc.to_xy(p);
        let (cx, cy) = ((x / self.cell).floor() as i64, (y / self.cell).floor() as i64);
        let span = (r / self.cell).ceil() as i64;
        let mut best: Option<(LatLon, f64, f64, T)> = None;
        let mut seen = std::collections::HashSet::new();
        for gx in cx - span..=cx + span {
            for gy in cy - span..=cy + span {
                let Some(v) = self.cells.get(&(gx, gy)) else { continue };
                for &i in v {
                    if !seen.insert(i) {
                        continue;
                    }
                    let (a, b, d) = &self.segs[i];
                    let (q, dist) = nearest_on_segment(&self.loc, p, *a, *b);
                    if dist <= r && best.as_ref().map(|bst| dist < bst.1).unwrap_or(true) {
                        best = Some((q, dist, bearing(*a, *b), d.clone()));
                    }
                }
            }
        }
        best
    }
}

/// Temperatura barwowa (K) → kolor RGB 0–1 (przybliżenie Tannera Hellanda).
pub fn kelvin_rgb(k: f64) -> [f64; 3] {
    let t = (k.clamp(1000.0, 40000.0)) / 100.0;
    let r = if t <= 66.0 { 255.0 } else { 329.698_727_446 * (t - 60.0).powf(-0.133_204_759_2) };
    let g = if t <= 66.0 { 99.470_802_586_1 * t.ln() - 161.119_568_166_1 } else { 288.122_169_528_3 * (t - 60.0).powf(-0.075_514_849_2) };
    let b = if t >= 66.0 {
        255.0
    } else if t <= 19.0 {
        0.0
    } else {
        138.517_731_223_1 * (t - 10.0).ln() - 305.044_792_730_7
    };
    [(r / 255.0).clamp(0.0, 1.0), (g / 255.0).clamp(0.0, 1.0), (b / 255.0).clamp(0.0, 1.0)]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distances_and_bearings() {
        let waw = LatLon::new(52.2297, 21.0122);
        let krk = LatLon::new(50.0647, 19.9450);
        let d = haversine(waw, krk);
        assert!((d - 252_000.0).abs() < 2_000.0, "{d}");
        let b = bearing(waw, krk);
        assert!(b > 190.0 && b < 205.0, "{b}");
        let p = destination(waw, 90.0, 1000.0);
        assert!((haversine(waw, p) - 1000.0).abs() < 0.5);
    }

    #[test]
    fn polygons() {
        let sq = vec![LatLon::new(0.0, 0.0), LatLon::new(0.0, 0.01), LatLon::new(0.01, 0.01), LatLon::new(0.01, 0.0)];
        assert!(point_in_polygon(LatLon::new(0.005, 0.005), &sq));
        assert!(!point_in_polygon(LatLon::new(0.02, 0.005), &sq));
        let a = polygon_area(&sq);
        assert!((a - 1_236_000.0).abs() < 10_000.0, "{a}");
        let b = BBox { south: 0.004, west: 0.004, north: 0.02, east: 0.02 };
        assert!(polygon_intersects_bbox(&sq, &b));
        let far = BBox { south: 1.0, west: 1.0, north: 1.1, east: 1.1 };
        assert!(!polygon_intersects_bbox(&sq, &far));
    }

    #[test]
    fn line_tools() {
        let line = vec![LatLon::new(52.0, 21.0), LatLon::new(52.0, 21.01)];
        let len = line_length(&line);
        let (mid, brg) = point_along(&line, len / 2.0).unwrap();
        assert!((mid.lon - 21.005).abs() < 1e-6);
        assert!((brg - 90.0).abs() < 0.1);
        let buf = buffer_line(&line, 5.0);
        assert_eq!(buf.len(), 4);
        let area = polygon_area(&buf);
        assert!((area - len * 10.0).abs() < 20.0, "{area}");
        let mut idx = SegmentIndex::new(line[0], 50.0);
        idx.insert_line(&line, 7u8);
        let (_, d, _, v) = idx.nearest(destination(mid, 0.0, 12.0), 30.0).unwrap();
        assert!((d - 12.0).abs() < 0.2);
        assert_eq!(v, 7);
    }

    #[test]
    fn colour_temperature() {
        let warm = kelvin_rgb(2200.0);
        let cool = kelvin_rgb(6500.0);
        assert!(warm[0] > warm[2]);
        assert!(cool[2] > 0.9);
    }
}
