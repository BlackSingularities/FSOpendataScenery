//! Odwzorowania kartograficzne potrzebne do czytania danych wysokościowych bez GDAL/PROJ:
//! Transverse Mercator (szeregi Krügera – UTM, PL-1992, PL-2000, SWEREF, NZTM, D96…),
//! Web Mercator, szwajcarskie LV95/LV03 i holenderskie RD (wzory przybliżone urzędów,
//! dokładność ~1 m) oraz brytyjski National Grid (Airy + transformacja Helmerta).
//! Wysokości przyjmujemy w metrach nad poziomem morza – tak samo jak ukształtowanie terenu w MSFS.

use crate::geo::LatLon;

#[derive(Debug, Clone, Copy)]
struct Ellipsoid {
    a: f64,
    f: f64,
}

const GRS80: Ellipsoid = Ellipsoid { a: 6_378_137.0, f: 1.0 / 298.257_222_101 };
const WGS84: Ellipsoid = Ellipsoid { a: 6_378_137.0, f: 1.0 / 298.257_223_563 };
const AIRY: Ellipsoid = Ellipsoid { a: 6_377_563.396, f: 1.0 / 299.324_964_6 };

#[derive(Debug, Clone, Copy)]
pub struct TransverseMercator {
    lon0: f64,
    k0: f64,
    fe: f64,
    fn_: f64,
    a_: f64,
    n: f64,
    alpha: [f64; 6],
    beta: [f64; 6],
    delta: [f64; 6],
    xi0: f64,
}

impl TransverseMercator {
    fn new(ell: Ellipsoid, lat0: f64, lon0: f64, k0: f64, fe: f64, fn_: f64) -> Self {
        let f = ell.f;
        let n = f / (2.0 - f);
        let (n2, n3, n4, n5, n6) = (n * n, n.powi(3), n.powi(4), n.powi(5), n.powi(6));
        let a_ = ell.a / (1.0 + n) * (1.0 + n2 / 4.0 + n4 / 64.0 + n6 / 256.0);
        let alpha = [
            n / 2.0 - 2.0 / 3.0 * n2 + 5.0 / 16.0 * n3 + 41.0 / 180.0 * n4 - 127.0 / 288.0 * n5 + 7891.0 / 37800.0 * n6,
            13.0 / 48.0 * n2 - 3.0 / 5.0 * n3 + 557.0 / 1440.0 * n4 + 281.0 / 630.0 * n5 - 1983433.0 / 1935360.0 * n6,
            61.0 / 240.0 * n3 - 103.0 / 140.0 * n4 + 15061.0 / 26880.0 * n5 + 167603.0 / 181440.0 * n6,
            49561.0 / 161280.0 * n4 - 179.0 / 168.0 * n5 + 6601661.0 / 7257600.0 * n6,
            34729.0 / 80640.0 * n5 - 3418889.0 / 1995840.0 * n6,
            212378941.0 / 319334400.0 * n6,
        ];
        let beta = [
            n / 2.0 - 2.0 / 3.0 * n2 + 37.0 / 96.0 * n3 - 1.0 / 360.0 * n4 - 81.0 / 512.0 * n5 + 96199.0 / 604800.0 * n6,
            1.0 / 48.0 * n2 + 1.0 / 15.0 * n3 - 437.0 / 1440.0 * n4 + 46.0 / 105.0 * n5 - 1118711.0 / 3870720.0 * n6,
            17.0 / 480.0 * n3 - 37.0 / 840.0 * n4 - 209.0 / 4480.0 * n5 + 5569.0 / 90720.0 * n6,
            4397.0 / 161280.0 * n4 - 11.0 / 504.0 * n5 - 830251.0 / 7257600.0 * n6,
            4583.0 / 161280.0 * n5 - 108847.0 / 3991680.0 * n6,
            20648693.0 / 638668800.0 * n6,
        ];
        let delta = [
            2.0 * n - 2.0 / 3.0 * n2 - 2.0 * n3 + 116.0 / 45.0 * n4 + 26.0 / 45.0 * n5 - 2854.0 / 675.0 * n6,
            7.0 / 3.0 * n2 - 8.0 / 5.0 * n3 - 227.0 / 45.0 * n4 + 2704.0 / 315.0 * n5 + 2323.0 / 945.0 * n6,
            56.0 / 15.0 * n3 - 136.0 / 35.0 * n4 - 1262.0 / 105.0 * n5 + 73814.0 / 2835.0 * n6,
            4279.0 / 630.0 * n4 - 332.0 / 35.0 * n5 - 399572.0 / 14175.0 * n6,
            4174.0 / 315.0 * n5 - 144838.0 / 6237.0 * n6,
            601676.0 / 22275.0 * n6,
        ];
        let mut tm = Self { lon0, k0, fe, fn_, a_, n, alpha, beta, delta, xi0: 0.0 };
        tm.xi0 = tm.xi_eta(lat0, lon0).0;
        tm
    }

    fn xi_eta(&self, lat: f64, lon: f64) -> (f64, f64) {
        let phi = lat.to_radians();
        let dl = (lon - self.lon0).to_radians();
        let e = 2.0 * self.n.sqrt() / (1.0 + self.n);
        let t = (phi.sin().atanh() - e * (e * phi.sin()).atanh()).sinh();
        let xi_p = t.atan2(dl.cos());
        let eta_p = (dl.sin() / (1.0 + t * t).sqrt()).atanh();
        let mut xi = xi_p;
        let mut eta = eta_p;
        for (j, a) in self.alpha.iter().enumerate() {
            let k = 2.0 * (j as f64 + 1.0);
            xi += a * (k * xi_p).sin() * (k * eta_p).cosh();
            eta += a * (k * xi_p).cos() * (k * eta_p).sinh();
        }
        (xi, eta)
    }

    pub fn forward(&self, p: LatLon) -> (f64, f64) {
        let (xi, eta) = self.xi_eta(p.lat, p.lon);
        (self.fe + self.k0 * self.a_ * eta, self.fn_ + self.k0 * self.a_ * (xi - self.xi0))
    }

    pub fn inverse(&self, x: f64, y: f64) -> LatLon {
        let xi = (y - self.fn_) / (self.k0 * self.a_) + self.xi0;
        let eta = (x - self.fe) / (self.k0 * self.a_);
        let mut xi_p = xi;
        let mut eta_p = eta;
        for (j, b) in self.beta.iter().enumerate() {
            let k = 2.0 * (j as f64 + 1.0);
            xi_p -= b * (k * xi).sin() * (k * eta).cosh();
            eta_p -= b * (k * xi).cos() * (k * eta).sinh();
        }
        let chi = (xi_p.sin() / eta_p.cosh()).asin();
        let mut phi = chi;
        for (j, d) in self.delta.iter().enumerate() {
            phi += d * (2.0 * (j as f64 + 1.0) * chi).sin();
        }
        let lon = self.lon0 + eta_p.sinh().atan2(xi_p.cos()).to_degrees();
        LatLon::new(phi.to_degrees(), lon)
    }
}

/// Układ współrzędnych źródła danych.
#[derive(Debug, Clone, Copy)]
pub enum Crs {
    Geographic,
    WebMercator,
    Tm(TransverseMercator),
    Swiss { lv95: bool },
    DutchRd,
    BritishGrid(TransverseMercator),
}

impl Crs {
    /// Obsługiwane kody EPSG. `None` – nieznany układ.
    pub fn from_epsg(code: u32) -> Option<Self> {
        let tm = |lat0: f64, lon0: f64, k0: f64, fe: f64, fn_: f64| Crs::Tm(TransverseMercator::new(GRS80, lat0, lon0, k0, fe, fn_));
        Some(match code {
            4326 | 4258 | 4269 | 4283 | 4167 | 4617 => Crs::Geographic,
            3857 | 900913 | 3395 => Crs::WebMercator,
            // Polska: PUWG 1992 i PL-2000 (strefy 5–8).
            2180 => tm(0.0, 19.0, 0.9993, 500_000.0, -5_300_000.0),
            2176..=2179 => {
                let zone = (code - 2176 + 5) as f64;
                tm(0.0, zone * 3.0, 0.999923, zone * 1_000_000.0 + 500_000.0, 0.0)
            }
            // UTM WGS84 północ / południe i ETRS89 UTM.
            32601..=32660 => Crs::Tm(TransverseMercator::new(WGS84, 0.0, utm_lon0(code - 32600), 0.9996, 500_000.0, 0.0)),
            32701..=32760 => Crs::Tm(TransverseMercator::new(WGS84, 0.0, utm_lon0(code - 32700), 0.9996, 500_000.0, 10_000_000.0)),
            25828..=25838 => tm(0.0, utm_lon0(code - 25800), 0.9996, 500_000.0, 0.0),
            3067 => tm(0.0, 27.0, 0.9996, 500_000.0, 0.0),
            3006 => tm(0.0, 15.0, 0.9996, 500_000.0, 0.0),
            3794 => tm(0.0, 15.0, 0.9999, 500_000.0, -5_000_000.0),
            2193 => tm(0.0, 173.0, 0.9996, 1_600_000.0, 10_000_000.0),
            5070..=5070 => return None,
            // Czechy/Słowacja: S-JTSK (Křovák) nieobsługiwany – dane zwykle dostępne też w UTM 33/34.
            2056 => Crs::Swiss { lv95: true },
            21781 => Crs::Swiss { lv95: false },
            28992 => Crs::DutchRd,
            27700 => Crs::BritishGrid(TransverseMercator::new(AIRY, 49.0, -2.0, 0.999_601_271_7, 400_000.0, -100_000.0)),
            _ => return None,
        })
    }

    /// WGS84 → współrzędne układu (x = wschód / długość, y = północ / szerokość).
    pub fn forward(&self, p: LatLon) -> (f64, f64) {
        match self {
            Crs::Geographic => (p.lon, p.lat),
            Crs::WebMercator => {
                let x = p.lon.to_radians() * 6_378_137.0;
                let y = ((std::f64::consts::FRAC_PI_4 + p.lat.to_radians() / 2.0).tan()).ln() * 6_378_137.0;
                (x, y)
            }
            Crs::Tm(tm) => tm.forward(p),
            Crs::Swiss { lv95 } => {
                let phi = (p.lat * 3600.0 - 169_028.66) / 10_000.0;
                let lam = (p.lon * 3600.0 - 26_782.5) / 10_000.0;
                let e = 2_600_072.37 + 211_455.93 * lam - 10_938.51 * lam * phi - 0.36 * lam * phi * phi - 44.54 * lam.powi(3);
                let n = 1_200_147.07 + 308_807.95 * phi + 3_745.25 * lam * lam + 76.63 * phi * phi - 194.56 * lam * lam * phi + 119.79 * phi.powi(3);
                if *lv95 {
                    (e, n)
                } else {
                    (e - 2_000_000.0, n - 1_000_000.0)
                }
            }
            Crs::DutchRd => rd_forward(p),
            Crs::BritishGrid(tm) => tm.forward(helmert_wgs84_to_osgb(p)),
        }
    }

    pub fn inverse(&self, x: f64, y: f64) -> LatLon {
        match self {
            Crs::Geographic => LatLon::new(y, x),
            Crs::WebMercator => {
                let lon = (x / 6_378_137.0).to_degrees();
                let lat = (2.0 * (y / 6_378_137.0).exp().atan() - std::f64::consts::FRAC_PI_2).to_degrees();
                LatLon::new(lat, lon)
            }
            Crs::Tm(tm) => tm.inverse(x, y),
            Crs::Swiss { lv95 } => {
                let (e, n) = if *lv95 { (x, y) } else { (x + 2_000_000.0, y + 1_000_000.0) };
                let yp = (e - 2_600_000.0) / 1_000_000.0;
                let xp = (n - 1_200_000.0) / 1_000_000.0;
                let lam = 2.677_909_4 + 4.728_982 * yp + 0.791_484 * yp * xp + 0.1306 * yp * xp * xp - 0.0436 * yp.powi(3);
                let phi = 16.902_389_2 + 3.238_272 * xp - 0.270_978 * yp * yp - 0.002_528 * xp * xp - 0.0447 * yp * yp * xp - 0.0140 * xp.powi(3);
                LatLon::new(phi * 100.0 / 36.0, lam * 100.0 / 36.0)
            }
            Crs::DutchRd => rd_inverse(x, y),
            Crs::BritishGrid(tm) => helmert_osgb_to_wgs84(tm.inverse(x, y)),
        }
    }

    /// Czy współrzędne są w stopniach (wpływa na tolerancje i domyślne rozdzielczości).
    pub fn is_geographic(&self) -> bool {
        matches!(self, Crs::Geographic)
    }
}

fn utm_lon0(zone: u32) -> f64 {
    zone as f64 * 6.0 - 183.0
}

/// Lista obsługiwanych układów do pokazania w interfejsie.
pub fn supported() -> Vec<(u32, &'static str)> {
    vec![
        (4326, "WGS 84 (stopnie)"),
        (3857, "Web Mercator"),
        (2180, "PL-1992 (PUWG 1992)"),
        (2176, "PL-2000 strefa 5"),
        (2177, "PL-2000 strefa 6"),
        (2178, "PL-2000 strefa 7"),
        (2179, "PL-2000 strefa 8"),
        (32633, "UTM 33N (WGS 84)"),
        (32634, "UTM 34N (WGS 84)"),
        (25832, "ETRS89 / UTM 32N"),
        (25833, "ETRS89 / UTM 33N"),
        (25834, "ETRS89 / UTM 34N"),
        (25835, "ETRS89 / UTM 35N"),
        (3067, "ETRS-TM35FIN"),
        (3006, "SWEREF99 TM"),
        (3794, "Slovenia 1996 (D96/TM)"),
        (2056, "CH1903+ / LV95"),
        (21781, "CH1903 / LV03"),
        (28992, "Amersfoort / RD New"),
        (27700, "British National Grid"),
        (2193, "NZGD2000 / NZTM"),
    ]
}

const RD_LAT0: f64 = 52.155_174_40;
const RD_LON0: f64 = 5.387_206_21;

fn rd_forward(p: LatLon) -> (f64, f64) {
    let dp = 0.36 * (p.lat - RD_LAT0);
    let dl = 0.36 * (p.lon - RD_LON0);
    let r: [(i32, i32, f64); 9] = [
        (0, 1, 190_094.945),
        (1, 1, -11_832.228),
        (2, 1, -114.221),
        (0, 3, -32.391),
        (1, 0, -0.705),
        (3, 1, -2.340),
        (1, 3, -0.608),
        (0, 2, -0.008),
        (2, 3, 0.148),
    ];
    let s: [(i32, i32, f64); 10] = [
        (1, 0, 309_056.544),
        (0, 2, 3_638.893),
        (2, 0, 73.077),
        (1, 2, -157.984),
        (3, 0, 59.788),
        (0, 1, 0.433),
        (2, 2, -6.439),
        (1, 1, -0.032),
        (0, 4, 0.092),
        (1, 4, -0.054),
    ];
    let x = 155_000.0 + r.iter().map(|(a, b, c)| c * dp.powi(*a) * dl.powi(*b)).sum::<f64>();
    let y = 463_000.0 + s.iter().map(|(a, b, c)| c * dp.powi(*a) * dl.powi(*b)).sum::<f64>();
    (x, y)
}

fn rd_inverse(x: f64, y: f64) -> LatLon {
    let dx = (x - 155_000.0) * 1e-5;
    let dy = (y - 463_000.0) * 1e-5;
    let k: [(i32, i32, f64); 11] = [
        (0, 1, 3_235.653_89),
        (2, 0, -32.582_97),
        (0, 2, -0.247_50),
        (2, 1, -0.849_78),
        (0, 3, -0.065_50),
        (2, 2, -0.017_09),
        (1, 0, -0.007_38),
        (4, 0, 0.005_30),
        (2, 3, -0.000_39),
        (4, 1, 0.000_33),
        (1, 1, -0.000_12),
    ];
    let l: [(i32, i32, f64); 12] = [
        (1, 0, 5_260.529_16),
        (1, 1, 105.946_84),
        (1, 2, 2.456_56),
        (3, 0, -0.818_85),
        (1, 3, 0.055_94),
        (3, 1, -0.056_07),
        (0, 1, 0.011_99),
        (3, 2, -0.002_56),
        (1, 4, 0.001_28),
        (0, 2, 0.000_22),
        (2, 0, -0.000_22),
        (5, 0, 0.000_26),
    ];
    let lat = RD_LAT0 + k.iter().map(|(a, b, c)| c * dx.powi(*a) * dy.powi(*b)).sum::<f64>() / 3600.0;
    let lon = RD_LON0 + l.iter().map(|(a, b, c)| c * dx.powi(*a) * dy.powi(*b)).sum::<f64>() / 3600.0;
    LatLon::new(lat, lon)
}

fn to_ecef(p: LatLon, ell: Ellipsoid) -> [f64; 3] {
    let e2 = ell.f * (2.0 - ell.f);
    let (phi, lam) = (p.lat.to_radians(), p.lon.to_radians());
    let nu = ell.a / (1.0 - e2 * phi.sin().powi(2)).sqrt();
    [nu * phi.cos() * lam.cos(), nu * phi.cos() * lam.sin(), nu * (1.0 - e2) * phi.sin()]
}

fn from_ecef(v: [f64; 3], ell: Ellipsoid) -> LatLon {
    let e2 = ell.f * (2.0 - ell.f);
    let p = (v[0] * v[0] + v[1] * v[1]).sqrt();
    let mut phi = (v[2] / (p * (1.0 - e2))).atan();
    for _ in 0..6 {
        let nu = ell.a / (1.0 - e2 * phi.sin().powi(2)).sqrt();
        phi = (v[2] + e2 * nu * phi.sin()).atan2(p);
    }
    LatLon::new(phi.to_degrees(), v[1].atan2(v[0]).to_degrees())
}

fn helmert(v: [f64; 3], t: [f64; 3], s_ppm: f64, r_sec: [f64; 3]) -> [f64; 3] {
    let s = 1.0 + s_ppm * 1e-6;
    let r = r_sec.map(|x| (x / 3600.0).to_radians());
    [
        t[0] + s * (v[0] - r[2] * v[1] + r[1] * v[2]),
        t[1] + s * (r[2] * v[0] + v[1] - r[0] * v[2]),
        t[2] + s * (-r[1] * v[0] + r[0] * v[1] + v[2]),
    ]
}

fn helmert_wgs84_to_osgb(p: LatLon) -> LatLon {
    let v = helmert(to_ecef(p, WGS84), [-446.448, 125.157, -542.060], 20.4894, [-0.1502, -0.2470, -0.8421]);
    from_ecef(v, AIRY)
}

fn helmert_osgb_to_wgs84(p: LatLon) -> LatLon {
    let v = helmert(to_ecef(p, AIRY), [446.448, -125.157, 542.060], -20.4894, [0.1502, 0.2470, 0.8421]);
    from_ecef(v, WGS84)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: (f64, f64), b: (f64, f64), tol: f64) {
        assert!((a.0 - b.0).abs() < tol && (a.1 - b.1).abs() < tol, "{a:?} != {b:?}");
    }

    #[test]
    fn puwg92_roundtrip_and_known_point() {
        let c = Crs::from_epsg(2180).unwrap();
        // Południk osiowy 19°E: x = 500 000, y = 0,9993 · łuk południka(52°) − 5 300 000
        let (x, y) = c.forward(LatLon::new(52.0, 19.0));
        assert!((x - 500_000.0).abs() < 0.01);
        assert!((y - 459_309.0).abs() < 60.0, "{y}");
        let p = LatLon::new(50.0777, 19.7848);
        let (x, y) = c.forward(p);
        let back = c.inverse(x, y);
        close((back.lat, back.lon), (p.lat, p.lon), 1e-8);
    }

    #[test]
    fn utm_known_point() {
        // 21°E to południk osiowy strefy 34: E = 500 000, N = 0,9996 · łuk południka(52°)
        let c = Crs::from_epsg(32634).unwrap();
        let (x, y) = c.forward(LatLon::new(52.0, 21.0));
        close((x, y), (500_000.0, 5_761_038.0), 60.0);
        let (x2, _) = c.forward(LatLon::new(52.0, 19.0));
        assert!((x2 - 362_705.0).abs() < 400.0, "{x2}");
        let back = c.inverse(x, y);
        close((back.lat, back.lon), (52.0, 21.0), 1e-8);
    }

    #[test]
    fn swiss_and_dutch() {
        // Berno, dawne obserwatorium (początek LV95): 46°57'03.9" N, 7°26'19.1" E ≈ (2600000, 1200000)
        let c = Crs::from_epsg(2056).unwrap();
        let (e, n) = c.forward(LatLon::new(46.951_081, 7.438_637));
        close((e, n), (2_600_000.0, 1_200_000.0), 8.0);
        let back = c.inverse(e, n);
        close((back.lat, back.lon), (46.951_081, 7.438_637), 2e-5);
        // Amersfoort – punkt główny RD
        let rd = Crs::from_epsg(28992).unwrap();
        close(rd.forward(LatLon::new(RD_LAT0, RD_LON0)), (155_000.0, 463_000.0), 1.0);
        let p = rd.inverse(121_000.0, 487_000.0);
        let q = rd.forward(p);
        close(q, (121_000.0, 487_000.0), 1.0);
    }

    #[test]
    fn british_grid() {
        // Big Ben ≈ 51.5007 N, −0.1246 E → (530 268, 179 640)
        let c = Crs::from_epsg(27700).unwrap();
        close(c.forward(LatLon::new(51.500_729, -0.124_625)), (530_268.0, 179_640.0), 8.0);
    }

    #[test]
    fn web_mercator() {
        let c = Crs::from_epsg(3857).unwrap();
        let p = LatLon::new(50.0, 20.0);
        let (x, y) = c.forward(p);
        let b = c.inverse(x, y);
        close((b.lat, b.lon), (50.0, 20.0), 1e-9);
    }
}
