//! P5 — placement priors mined from REAL boards.
//!
//! The recipes carry convention constants ("decap within 3mm") that
//! are folklore, not measurement. This module mines actual KiCad
//! layouts (.kicad_pcb) for the distances experienced designers
//! actually ship — decap-to-IC, connector edge inset, crystal-to-IC —
//! and lets the recipe layer replace a folklore constant with a
//! mined median WHEN the sample is large enough (n >= 8), with
//! provenance logged. No priors file → behavior is byte-identical
//! to the constants. Same Real-Data doctrine as everything else:
//! a number must trace to a source, and here the source is a
//! corpus of real boards, recorded in the file itself.
//!
//! Mining is pure statistics over placements: no ML, deterministic,
//! reproducible from the same corpus.

use serde::{Deserialize, Serialize};

// ── Minimal s-expression reader (KiCad 6+ files) ─────────────────────

#[derive(Debug, Clone)]
pub enum Sx {
    Atom(String),
    List(Vec<Sx>),
}

impl Sx {
    fn head(&self) -> Option<&str> {
        match self {
            Sx::List(items) => items.first().and_then(|s| match s {
                Sx::Atom(a) => Some(a.as_str()),
                _ => None,
            }),
            _ => None,
        }
    }
    fn atom(&self) -> Option<&str> {
        match self {
            Sx::Atom(a) => Some(a.as_str()),
            _ => None,
        }
    }
    fn items(&self) -> &[Sx] {
        match self {
            Sx::List(v) => v,
            _ => &[],
        }
    }
    /// First child list with the given head.
    fn child(&self, head: &str) -> Option<&Sx> {
        self.items().iter().find(|c| c.head() == Some(head))
    }
    fn children<'a>(&'a self, head: &'a str) -> impl Iterator<Item = &'a Sx> + 'a {
        self.items().iter().filter(move |c| c.head() == Some(head))
    }
    fn num(&self, idx: usize) -> Option<f64> {
        self.items().get(idx)?.atom()?.parse().ok()
    }
    fn str_at(&self, idx: usize) -> Option<&str> {
        self.items().get(idx)?.atom()
    }
}

pub fn parse_sexpr(text: &str) -> Option<Sx> {
    let b = text.as_bytes();
    let mut i = 0usize;
    fn skip_ws(b: &[u8], i: &mut usize) {
        while *i < b.len() && (b[*i] as char).is_whitespace() {
            *i += 1;
        }
    }
    fn parse(b: &[u8], i: &mut usize) -> Option<Sx> {
        skip_ws(b, i);
        if *i >= b.len() {
            return None;
        }
        if b[*i] == b'(' {
            *i += 1;
            let mut items = Vec::new();
            loop {
                skip_ws(b, i);
                if *i >= b.len() {
                    return None;
                }
                if b[*i] == b')' {
                    *i += 1;
                    return Some(Sx::List(items));
                }
                items.push(parse(b, i)?);
            }
        }
        if b[*i] == b'"' {
            *i += 1;
            let start = *i;
            let mut s = String::new();
            while *i < b.len() && b[*i] != b'"' {
                if b[*i] == b'\\' && *i + 1 < b.len() {
                    *i += 1;
                }
                s.push(b[*i] as char);
                *i += 1;
            }
            let _ = start;
            *i += 1; // closing quote
            return Some(Sx::Atom(s));
        }
        let start = *i;
        while *i < b.len() && !(b[*i] as char).is_whitespace() && b[*i] != b'(' && b[*i] != b')'
        {
            *i += 1;
        }
        Some(Sx::Atom(String::from_utf8_lossy(&b[start..*i]).into_owned()))
    }
    parse(b, &mut i)
}

// ── Extracted placement facts ────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct PartPlacement {
    pub refdes: String,
    pub footprint: String,
    pub x: f64,
    pub y: f64,
    /// Net names touched by this part's pads.
    pub nets: Vec<String>,
    /// Pad GLOBAL positions with their net names (rotation applied).
    pub pad_pts: Vec<(f64, f64, String)>,
    /// The footprint's Value property (e.g. "100nF") when present.
    pub value: String,
}

#[derive(Debug, Clone)]
pub struct PcbSnapshot {
    pub name: String,
    pub parts: Vec<PartPlacement>,
    /// Board bbox from Edge.Cuts graphics (fallback: part extents).
    pub bbox: (f64, f64, f64, f64),
}

pub fn parse_kicad_pcb(name: &str, text: &str) -> Option<PcbSnapshot> {
    let root = parse_sexpr(text)?;
    if root.head() != Some("kicad_pcb") {
        return None;
    }
    let mut parts = Vec::new();
    let (mut ex0, mut ey0, mut ex1, mut ey1) =
        (f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY);
    let mut saw_edge = false;
    for item in root.items() {
        match item.head() {
            Some("footprint") | Some("module") => {
                let footprint =
                    item.str_at(1).unwrap_or("").to_string();
                let at = item.child("at");
                let (x, y, rot) = match at {
                    Some(a) => (
                        a.num(1).unwrap_or(0.0),
                        a.num(2).unwrap_or(0.0),
                        a.num(3).unwrap_or(0.0).to_radians(),
                    ),
                    None => continue,
                };
                let mut refdes = String::new();
                let mut value = String::new();
                for prop in item.children("property") {
                    if prop.str_at(1) == Some("Reference") {
                        refdes = prop.str_at(2).unwrap_or("").to_string();
                    }
                    if prop.str_at(1) == Some("Value") {
                        value = prop.str_at(2).unwrap_or("").to_string();
                    }
                }
                for t in item.children("fp_text") {
                    if refdes.is_empty() && t.str_at(1) == Some("reference") {
                        refdes = t.str_at(2).unwrap_or("").to_string();
                    }
                    if value.is_empty() && t.str_at(1) == Some("value") {
                        value = t.str_at(2).unwrap_or("").to_string();
                    }
                }
                let mut nets = Vec::new();
                let mut pad_pts = Vec::new();
                let (c, sn) = (rot.cos(), rot.sin());
                for pad in item.children("pad") {
                    let Some(n) = pad.child("net") else { continue };
                    let Some(nm) = n.str_at(2) else { continue };
                    if !nets.iter().any(|e| e == nm) {
                        nets.push(nm.to_string());
                    }
                    if let Some(pa) = pad.child("at") {
                        let (dx, dy) = (pa.num(1).unwrap_or(0.0), pa.num(2).unwrap_or(0.0));
                        // KiCad fp pad offsets rotate WITH the footprint
                        // (screen coords, y-down): global = at + R(rot)·d.
                        pad_pts.push((
                            x + dx * c + dy * sn,
                            y - dx * sn + dy * c,
                            nm.to_string(),
                        ));
                    }
                }
                parts.push(PartPlacement { refdes, footprint, x, y, nets, pad_pts, value });
            }
            Some("gr_line") | Some("gr_rect") | Some("gr_poly") | Some("gr_arc") => {
                let on_edge = item
                    .child("layer")
                    .and_then(|l| l.str_at(1))
                    .map(|l| l == "Edge.Cuts")
                    .unwrap_or(false);
                if !on_edge {
                    continue;
                }
                saw_edge = true;
                for key in ["start", "end", "center", "mid"] {
                    if let Some(p) = item.child(key) {
                        if let (Some(x), Some(y)) = (p.num(1), p.num(2)) {
                            ex0 = ex0.min(x);
                            ey0 = ey0.min(y);
                            ex1 = ex1.max(x);
                            ey1 = ey1.max(y);
                        }
                    }
                }
                if let Some(pts) = item.child("pts") {
                    for xy in pts.children("xy") {
                        if let (Some(x), Some(y)) = (xy.num(1), xy.num(2)) {
                            ex0 = ex0.min(x);
                            ey0 = ey0.min(y);
                            ex1 = ex1.max(x);
                            ey1 = ey1.max(y);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if !saw_edge {
        for p in &parts {
            ex0 = ex0.min(p.x);
            ey0 = ey0.min(p.y);
            ex1 = ex1.max(p.x);
            ey1 = ey1.max(p.y);
        }
    }
    if parts.is_empty() || !ex0.is_finite() {
        return None;
    }
    Some(PcbSnapshot {
        name: name.to_string(),
        parts,
        bbox: (ex0, ey0, ex1, ey1),
    })
}

// ── Classification (footprint name first, refdes fallback) ──────────

fn is_cap(p: &PartPlacement) -> bool {
    let f = p.footprint.to_ascii_lowercase();
    f.contains("c_0") || f.contains("capacitor") || p.refdes.starts_with('C')
        && p.refdes[1..].chars().all(|c| c.is_ascii_digit())
}

fn is_ic(p: &PartPlacement) -> bool {
    let f = p.footprint.to_ascii_uppercase();
    ["QFP", "QFN", "SOIC", "SSOP", "TSSOP", "DIP", "SOT", "BGA", "LQFP"]
        .iter()
        .any(|k| f.contains(k))
        || (p.refdes.starts_with('U')
            && p.refdes[1..].chars().all(|c| c.is_ascii_digit()))
}

fn is_connector(p: &PartPlacement) -> bool {
    let f = p.footprint.to_ascii_lowercase();
    f.contains("pinheader")
        || f.contains("conn")
        || f.contains("usb")
        || f.contains("jack")
        || f.contains("header")
        || (p.refdes.starts_with('J')
            && p.refdes[1..].chars().all(|c| c.is_ascii_digit()))
}

fn is_crystal(p: &PartPlacement) -> bool {
    let f = p.footprint.to_ascii_lowercase();
    f.contains("crystal") || f.contains("resonator")
        || (p.refdes.starts_with('Y')
            && p.refdes[1..].chars().all(|c| c.is_ascii_digit()))
}

/// Parse a capacitor Value ("100nF", "4.7u", "22p") to farads.
fn cap_farads(v: &str) -> Option<f64> {
    let t = v.trim().trim_end_matches(['F', 'f']).trim();
    let (num, mult) = if let Some(n) = t.strip_suffix(['p', 'P']) {
        (n, 1e-12)
    } else if let Some(n) = t.strip_suffix(['n', 'N']) {
        (n, 1e-9)
    } else if let Some(n) = t.strip_suffix(['u', 'U', 'µ']) {
        (n, 1e-6)
    } else if let Some(n) = t.strip_suffix(['m', 'M']) {
        (n, 1e-3)
    } else {
        (t, 1.0)
    };
    num.trim().replace(',', ".").parse::<f64>().ok().map(|x| x * mult)
}

/// A DECOUPLING cap, not bulk/filter: value known and <= 1uF. Caps
/// with unparseable values are excluded — mining bulk electrolytics
/// as "decap distance" corrupted the prior (first real harvest:
/// median 9.2mm over 70 mixed samples).
fn is_decap(p: &PartPlacement) -> bool {
    is_cap(p) && cap_farads(&p.value).map_or(false, |f| f <= 1.0e-6)
}

fn is_power_net(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    u.starts_with("VCC")
        || u.starts_with("VDD")
        || u.starts_with("AVCC")
        || u.starts_with("VBUS")
        || u.starts_with('+')
        || u.contains("3V3")
        || u.contains("5V")
        || u.contains("PWR")
}

fn is_gnd_net(name: &str) -> bool {
    let u = name.to_ascii_uppercase();
    u.contains("GND") || u == "0" || u.contains("VSS")
}

// ── Priors ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prior {
    pub median_mm: f64,
    pub n: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlacementPriors {
    /// Center distance from a decoupling cap to the nearest IC
    /// sharing its power net.
    pub decap_to_ic: Option<Prior>,
    /// Connector center inset from the nearest board edge.
    pub connector_edge_inset: Option<Prior>,
    /// Crystal center distance to the nearest IC sharing a net.
    pub crystal_to_ic: Option<Prior>,
    /// Boards the medians were mined from.
    pub source_boards: Vec<String>,
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    Some(v[v.len() / 2])
}

pub fn mine(boards: &[PcbSnapshot]) -> PlacementPriors {
    let mut decap = Vec::new();
    let mut conn = Vec::new();
    let mut xtal = Vec::new();
    for b in boards {
        let ics: Vec<&PartPlacement> = b.parts.iter().filter(|p| is_ic(p)).collect();
        for p in &b.parts {
            if is_decap(p)
                && p.nets.iter().any(|n| is_power_net(n))
                && p.nets.iter().any(|n| is_gnd_net(n))
            {
                let pow: Vec<&String> =
                    p.nets.iter().filter(|n| is_power_net(n)).collect();
                // Cap center to the nearest matching IC PAD — the pin
                // the decap serves, not the package centroid (center-
                // to-center inflated large QFPs to an 11mm "median").
                let d = ics
                    .iter()
                    .flat_map(|ic| ic.pad_pts.iter())
                    .filter(|(_, _, n)| pow.iter().any(|q| *q == n))
                    .map(|(px, py, _)| (px - p.x).hypot(py - p.y))
                    .fold(f64::INFINITY, f64::min);
                if d.is_finite() && d < 25.0 {
                    decap.push(d);
                }
            }
            if is_connector(p) {
                let (x0, y0, x1, y1) = b.bbox;
                let inset = (p.x - x0)
                    .min(x1 - p.x)
                    .min(p.y - y0)
                    .min(y1 - p.y)
                    .max(0.0);
                conn.push(inset);
            }
            if is_crystal(p) {
                let d = ics
                    .iter()
                    .filter(|ic| ic.nets.iter().any(|n| p.nets.contains(n)))
                    .map(|ic| (ic.x - p.x).hypot(ic.y - p.y))
                    .fold(f64::INFINITY, f64::min);
                if d.is_finite() && d < 30.0 {
                    xtal.push(d);
                }
            }
        }
    }
    let prior = |v: Vec<f64>| {
        let n = v.len();
        median(v).map(|m| Prior { median_mm: (m * 100.0).round() / 100.0, n })
    };
    PlacementPriors {
        decap_to_ic: prior(decap),
        connector_edge_inset: prior(conn),
        crystal_to_ic: prior(xtal),
        source_boards: boards.iter().map(|b| b.name.clone()).collect(),
    }
}

// ── Recipe seam ──────────────────────────────────────────────────────

use std::sync::OnceLock;
static LOADED: OnceLock<Option<PlacementPriors>> = OnceLock::new();

fn loaded() -> &'static Option<PlacementPriors> {
    LOADED.get_or_init(|| {
        let path = std::env::var("BHDL_PLACEMENT_PRIORS").ok()?;
        let text = std::fs::read_to_string(&path).ok()?;
        let p: PlacementPriors = serde_json::from_str(&text).ok()?;
        log::info!(
            "placement priors loaded from {path} ({} board(s))",
            p.source_boards.len()
        );
        Some(p)
    })
}

/// The recipe seam: a convention constant, REPLACED by the mined
/// median when a priors file is loaded and the sample is honest
/// (n >= 8). Without a priors file this returns `convention`
/// untouched — byte-identical behavior.
pub fn convention_mm(key: &str, convention: f64) -> f64 {
    let Some(p) = loaded() else { return convention };
    let prior = match key {
        "decap_to_ic" => &p.decap_to_ic,
        "connector_edge_inset" => &p.connector_edge_inset,
        "crystal_to_ic" => &p.crystal_to_ic,
        _ => &None,
    };
    match prior {
        Some(pr) if pr.n >= 8 => {
            log::info!(
                "prior '{key}': mined median {:.2}mm (n={}) replaces convention {convention}mm",
                pr.median_mm, pr.n
            );
            pr.median_mm
        }
        _ => convention,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TINY_PCB: &str = r#"(kicad_pcb (version 20221018)
  (gr_rect (start 0 0) (end 50 40) (layer "Edge.Cuts"))
  (footprint "Package_QFP:LQFP-32" (at 25 20)
    (property "Reference" "U1")
    (pad "1" smd rect (at -3 0) (net 1 "VCC"))
    (pad "2" smd rect (at 3 0) (net 2 "GND"))
    (pad "3" smd rect (at 0 3) (net 3 "XTAL1")))
  (footprint "Capacitor_SMD:C_0603" (at 27 22)
    (property "Reference" "C1")
    (property "Value" "100nF")
    (pad "1" smd rect (at -0.7 0) (net 1 "VCC"))
    (pad "2" smd rect (at 0.7 0) (net 2 "GND")))
  (footprint "Crystal:Crystal_SMD" (at 25 26)
    (property "Reference" "Y1")
    (pad "1" smd rect (at -1 0) (net 3 "XTAL1"))
    (pad "2" smd rect (at 1 0) (net 2 "GND")))
  (footprint "Connector_PinHeader:PinHeader_1x04" (at 2.5 20)
    (property "Reference" "J1")
    (pad "1" thru_hole circle (at 0 0) (net 2 "GND"))))"#;

    #[test]
    fn mines_the_three_priors_from_a_real_shaped_pcb() {
        let snap = parse_kicad_pcb("tiny", TINY_PCB).unwrap();
        assert_eq!(snap.parts.len(), 4);
        assert_eq!(snap.bbox, (0.0, 0.0, 50.0, 40.0));
        let p = mine(&[snap]);
        let d = p.decap_to_ic.unwrap();
        assert_eq!(d.n, 1);
        // Pad-level: C1 center (27,22) to U1's VCC pad at (22,20).
        assert!((d.median_mm - (5.0f64 * 5.0 + 2.0 * 2.0).sqrt()).abs() < 0.01, "{d:?}");
        let c = p.connector_edge_inset.unwrap();
        assert!((c.median_mm - 2.5).abs() < 0.01, "{c:?}");
        let x = p.crystal_to_ic.unwrap();
        assert!((x.median_mm - 6.0).abs() < 0.01, "{x:?}");
    }

    #[test]
    fn convention_stands_without_a_priors_file() {
        // No BHDL_PLACEMENT_PRIORS in the test env: the seam is inert.
        assert_eq!(convention_mm("decap_to_ic", 3.0), 3.0);
    }
}
