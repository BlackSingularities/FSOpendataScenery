//! Wielokąty usuwające roślinność (autogen drzew) tam, gdzie drzew być nie powinno:
//! drogi i tory (pas o szerokości jezdni + margines), wody, pasy startowe, boiska,
//! parkingi oraz wybrane formy użytkowania terenu (kamieniołomy, budowy, tereny kolejowe).

use crate::geo::{self, Area, LatLon};
use crate::lights::road_width;
use crate::osm::Element;
use crate::store::VegetationCfg;

pub struct VegPolygon {
    pub name: String,
    pub ring: Vec<LatLon>,
}

pub fn polygons(els: &[Element], area: &Area, cfg: &VegetationCfg) -> Vec<VegPolygon> {
    let b = area.bbox();
    let mut out = vec![];
    let mut push = |name: String, ring: Vec<LatLon>| {
        if ring.len() >= 3 && ring.iter().any(|p| b.contains(*p)) && geo::polygon_area(&ring) >= 20.0 {
            out.push(VegPolygon { name, ring });
        }
    };
    for e in els {
        let id = e.id;
        if cfg.roads && !e.is_area() {
            if let Some(hw) = e.tag("highway") {
                let major = matches!(hw, "motorway" | "trunk" | "primary" | "secondary" | "tertiary" | "motorway_link" | "trunk_link" | "primary_link" | "secondary_link" | "tertiary_link" | "residential" | "unclassified" | "living_street" | "service");
                if major && !e.is("tunnel", "yes") {
                    let line = geo::simplify(&e.line, 1.0);
                    push(format!("road_{id}"), geo::buffer_line(&line, road_width(e) / 2.0 + cfg.margin));
                }
            }
        }
        if cfg.railways && !e.is_area() && matches!(e.tag("railway"), Some("rail") | Some("light_rail") | Some("tram") | Some("subway") | Some("narrow_gauge")) {
            let line = geo::simplify(&e.line, 1.0);
            let half = if e.is("railway", "tram") { 1.5 } else { 2.5 };
            push(format!("rail_{id}"), geo::buffer_line(&line, half + cfg.margin + 1.5));
        }
        if cfg.runways && matches!(e.tag("aeroway"), Some("runway") | Some("taxiway") | Some("apron")) {
            if e.is_area() {
                for r in &e.outers {
                    push(format!("aero_{id}"), r.clone());
                }
            } else {
                let w = e.tag("width").and_then(crate::osm::parse_height).unwrap_or(if e.is("aeroway", "runway") { 30.0 } else { 15.0 });
                push(format!("aero_{id}"), geo::buffer_line(&e.line, w / 2.0 + cfg.margin + 5.0));
            }
        }
        if !e.is_area() {
            continue;
        }
        let water = e.is("natural", "water") || e.is("waterway", "riverbank") || matches!(e.tag("landuse"), Some("reservoir") | Some("basin"));
        let pitch = matches!(e.tag("leisure"), Some("pitch") | Some("stadium") | Some("track"));
        let parking = e.is("amenity", "parking") && !matches!(e.tag("parking"), Some("underground") | Some("multi-storey"));
        let landuse = matches!(e.tag("landuse"), Some("quarry") | Some("construction") | Some("railway") | Some("brownfield") | Some("landfill"));
        let take = (cfg.water && water) || (cfg.pitches && pitch) || (cfg.parking && parking) || (cfg.landuse && landuse);
        if take {
            for r in &e.outers {
                push(format!("area_{id}"), r.clone());
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn roads_and_water() {
        let area = Area { outer: vec![LatLon::new(50.0, 19.0), LatLon::new(50.0, 19.1), LatLon::new(50.1, 19.1), LatLon::new(50.1, 19.0)], holes: vec![] };
        let mut tags = HashMap::new();
        tags.insert("highway".to_string(), "primary".to_string());
        let road = Element { t: 'w', id: 1, tags, line: vec![LatLon::new(50.05, 19.01), LatLon::new(50.05, 19.02)], ..Default::default() };
        let mut wt = HashMap::new();
        wt.insert("natural".to_string(), "water".to_string());
        let lake = Element { t: 'w', id: 2, tags: wt, outers: vec![vec![LatLon::new(50.02, 19.02), LatLon::new(50.02, 19.03), LatLon::new(50.03, 19.03)]], ..Default::default() };
        let p = polygons(&[road, lake], &area, &VegetationCfg { enabled: true, ..Default::default() });
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].ring.len(), 4);
    }
}
