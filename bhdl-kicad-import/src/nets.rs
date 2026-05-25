//! Net topology extraction.
//!
//! Phase C of the KiCad-to-BHDL translator pipeline (plan §5.3).
//! Given a parsed [`Sheet`], walks wires/junctions/pins/labels/power
//! symbols and produces a list of named [`Net`]s — one per electrical
//! equivalence class of pins.
//!
//! ## Algorithm
//!
//! 1. **Quantise** every connection point `(x, y)` to integer micron
//!    coordinates so float jitter doesn't break unification.
//! 2. **Union-find** over those points:
//!     - For each [`Wire`]: union its two endpoints.
//!     - For each [`Junction`]: union every wire endpoint at that
//!       point (already implicit from step 1; junctions also catch
//!       T-intersections where two wires meet end-to-end without a
//!       crossing, which is the common KiCad case).
//!     - For each pin: add it as a point so that wires terminating
//!       on a pin pull the pin into the same component.
//! 3. **Naming**: scan labels, global labels, hierarchical labels,
//!    and power symbols. Whichever component their position lands in
//!    inherits that name. Priority: power > global > hierarchical >
//!    local. Components with no label get auto-named `Net_N`.
//! 4. **Pin attachment**: every `(symbol_uuid, pin_number)` pair is
//!    bucketed into its component's net.
//!
//! ## Simplifications in v0.1
//!
//! - **No mid-wire junction support.** KiCad schematics normally
//!   place junctions at wire-endpoint T-intersections, which the
//!   algorithm handles. The rarer case of a wire passing *through*
//!   a junction point mid-segment (the third wire forming a "+"
//!   intersection) is not yet supported — that needs point-on-segment
//!   testing. Phase F's Arduino Uno benchmark will reveal whether
//!   this matters in practice.
//! - **Hierarchical labels** are recorded as local names per sheet;
//!   merging across the sheet hierarchy happens in a later pass once
//!   we know the parent-child sheet-pin correspondence.
//! - **No-connect** markers attach to the net but do not suppress
//!   warnings yet — that's an enrichment pass.

use std::collections::HashMap;

use crate::ir::{
    NoConnect, PinElectricalType, PowerCategory, PowerSymbol, SchematicSymbol, Sheet,
};

/// Quantum (in millimetres) for coordinate snapping. KiCad's
/// schematic grid is 1.27 mm by default; pins always land on it.
/// 0.001 mm = 1 micron, which is finer than any KiCad placement
/// can express and avoids any float jitter from rotation maths.
const QUANTUM_MM: f64 = 0.001;

/// Integer-quantised 2D point. Two `(f64, f64)` points round to the
/// same `QPoint` iff they're within `QUANTUM_MM` of each other.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct QPoint(i64, i64);

impl QPoint {
    fn from(at: (f64, f64)) -> Self {
        QPoint(
            (at.0 / QUANTUM_MM).round() as i64,
            (at.1 / QUANTUM_MM).round() as i64,
        )
    }
}

/// One connection on a net: which symbol, which pin.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetPin {
    /// UUID of the schematic symbol instance.
    pub symbol_uuid: String,
    /// Pin number (KiCad's perspective; the BHDL importer will
    /// translate to port names via [`crate::symbol_mapping`]).
    pub pin_number: String,
    /// Pin name from the library symbol (useful for diagnostics).
    pub pin_name: String,
    /// Electrical type — used by ERC and to detect missing power
    /// pins or drive contention.
    pub electrical_type: PinElectricalType,
}

/// One electrically-connected group of pins.
#[derive(Debug, Clone)]
pub struct Net {
    /// Canonical net name. Either a label/power name, or
    /// `Net_<id>` for auto-generated names.
    pub name: String,
    /// Pins attached to this net.
    pub pins: Vec<NetPin>,
    /// True if the net inherited its name from a power-flag symbol
    /// (`+5V`, `GND`, …) — emitter treats these as power/ground
    /// declarations rather than ordinary nets.
    pub is_power: bool,
    /// True if any [`NoConnect`] marker landed on this net. Suppresses
    /// "unconnected pin" warnings in later passes.
    pub no_connect: bool,
}

/// Result of net extraction over a single sheet.
#[derive(Debug, Clone)]
pub struct NetList {
    pub nets: Vec<Net>,
}

impl NetList {
    /// Convenience: look up a net by name.
    pub fn by_name(&self, name: &str) -> Option<&Net> {
        self.nets.iter().find(|n| n.name == name)
    }

    /// Total pin count across all nets (sanity check: should equal
    /// `Σ symbol.pin_positions.len()` minus pins on truly isolated
    /// symbols, which v0.1 doesn't model).
    pub fn total_pins(&self) -> usize {
        self.nets.iter().map(|n| n.pins.len()).sum()
    }
}

// ─── union-find ────────────────────────────────────────────────────

/// Classic union-find with path compression + rank.
struct UnionFind {
    parent: Vec<u32>,
    rank: Vec<u8>,
}

impl UnionFind {
    fn new() -> Self { Self { parent: Vec::new(), rank: Vec::new() } }

    fn make_set(&mut self) -> u32 {
        let id = self.parent.len() as u32;
        self.parent.push(id);
        self.rank.push(0);
        id
    }

    fn find(&mut self, x: u32) -> u32 {
        let mut root = x;
        while self.parent[root as usize] != root {
            root = self.parent[root as usize];
        }
        // Path compression.
        let mut cur = x;
        while self.parent[cur as usize] != root {
            let next = self.parent[cur as usize];
            self.parent[cur as usize] = root;
            cur = next;
        }
        root
    }

    fn union(&mut self, a: u32, b: u32) {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == rb { return; }
        let (small, big) = if self.rank[ra as usize] < self.rank[rb as usize] {
            (ra, rb)
        } else {
            (rb, ra)
        };
        self.parent[small as usize] = big;
        if self.rank[small as usize] == self.rank[big as usize] {
            self.rank[big as usize] += 1;
        }
    }
}

// ─── extraction entry point ───────────────────────────────────────

/// Extract the net topology of one sheet.
pub fn extract_nets(sheet: &Sheet) -> NetList {
    let mut uf = UnionFind::new();
    // Map: QPoint → union-find node id.
    let mut node_of: HashMap<QPoint, u32> = HashMap::new();

    // Helper to intern a point.
    let intern = |uf: &mut UnionFind, node_of: &mut HashMap<QPoint, u32>, p: QPoint| -> u32 {
        if let Some(&id) = node_of.get(&p) { return id; }
        let id = uf.make_set();
        node_of.insert(p, id);
        id
    };

    // 1. Wires: union endpoints.
    for w in &sheet.wires {
        let a = intern(&mut uf, &mut node_of, QPoint::from(w.start));
        let b = intern(&mut uf, &mut node_of, QPoint::from(w.end));
        uf.union(a, b);
    }

    // 2. Junctions: ensure the point exists (T-intersections of
    //    wire endpoints are already unified; explicit junctions
    //    cover the case where two wires meet head-to-tail with
    //    overlapping endpoints, plus make the point survive even
    //    if no wire is attached yet).
    for j in &sheet.junctions {
        intern(&mut uf, &mut node_of, QPoint::from(j.at));
    }

    // 3. Pins: each pin is a point. Note pin_positions store
    //    schematic-absolute coordinates already (computed during
    //    the read phase by applying symbol_at + rotation to the
    //    library-relative pin position).
    //
    //    We also record a Vec<(symbol_idx, pin_idx, node_id)> to
    //    bucket pins by net after the union-find is done.
    let mut pin_to_node: Vec<(usize, usize, u32)> = Vec::new();
    for (si, sym) in sheet.symbols.iter().enumerate() {
        for (pi, pp) in sym.pin_positions.iter().enumerate() {
            let id = intern(&mut uf, &mut node_of, QPoint::from(pp.at));
            pin_to_node.push((si, pi, id));
        }
    }

    // 3b. Mid-segment attachments. Labels, junctions, power symbols
    //     and no-connect markers in real KiCad files routinely sit
    //     *along* a wire rather than at one of its endpoints. For
    //     every such position, test point-on-segment against every
    //     wire and union the point's node with the wire's endpoint
    //     when it lands on the segment.
    //
    //     O(P·W) — fine for hand-drawn sheets (P, W are both small
    //     hundreds at most).
    let mut aux_points: Vec<(f64, f64)> = Vec::new();
    aux_points.extend(sheet.labels.iter()
        .map(|l| (l.at.0, l.at.1)));
    aux_points.extend(sheet.global_labels.iter()
        .map(|l| (l.at.0, l.at.1)));
    aux_points.extend(sheet.hierarchical_labels.iter()
        .map(|l| (l.at.0, l.at.1)));
    aux_points.extend(sheet.power_symbols.iter()
        .map(|p| (p.at.0, p.at.1)));
    aux_points.extend(sheet.junctions.iter().map(|j| j.at));
    aux_points.extend(sheet.no_connects.iter().map(|n| n.at));

    for ap in aux_points {
        let p_node = intern_into(&mut uf, &mut node_of, QPoint::from(ap));
        for w in &sheet.wires {
            if point_on_segment(ap, w.start, w.end) {
                let wn = intern_into(&mut uf, &mut node_of, QPoint::from(w.start));
                uf.union(p_node, wn);
            }
        }
    }

    // 4. Sheet-pin connections: a (sheet ...) reference's pins
    //    behave like pins on the parent. We intern them so that
    //    wires terminating on a sheet pin keep the net coherent,
    //    but we don't (yet) cross the hierarchy.
    for sr in &sheet.sheet_refs {
        for sp in &sr.pins {
            intern(&mut uf, &mut node_of, QPoint::from((sp.at.0, sp.at.1)));
        }
    }

    // ─── naming pass ─────────────────────────────────────────────
    // Collect (root → name, priority). Higher priority wins.

    #[derive(Clone)]
    struct NameCandidate { name: String, priority: u8, is_power: bool }

    let mut name_of: HashMap<u32, NameCandidate> = HashMap::new();

    let propose = |uf: &mut UnionFind,
                       node_of: &mut HashMap<QPoint, u32>,
                       names: &mut HashMap<u32, NameCandidate>,
                       point: (f64, f64),
                       cand: NameCandidate| {
        let p = QPoint::from(point);
        let id = intern_into(uf, node_of, p);
        let root = uf.find(id);
        match names.get(&root) {
            Some(existing) if existing.priority >= cand.priority => {}
            _ => { names.insert(root, cand); }
        }
    };

    // Power symbols (priority 4, marks is_power=true).
    for ps in &sheet.power_symbols {
        propose(
            &mut uf, &mut node_of, &mut name_of,
            (ps.at.0, ps.at.1),
            NameCandidate {
                name: canonical_power_name(ps),
                priority: 4,
                is_power: true,
            },
        );
    }
    // Global labels (priority 3).
    for gl in &sheet.global_labels {
        propose(
            &mut uf, &mut node_of, &mut name_of,
            (gl.at.0, gl.at.1),
            NameCandidate { name: gl.text.clone(), priority: 3, is_power: false },
        );
    }
    // Hierarchical labels (priority 2).
    for hl in &sheet.hierarchical_labels {
        propose(
            &mut uf, &mut node_of, &mut name_of,
            (hl.at.0, hl.at.1),
            NameCandidate { name: hl.text.clone(), priority: 2, is_power: false },
        );
    }
    // Local labels (priority 1).
    for l in &sheet.labels {
        propose(
            &mut uf, &mut node_of, &mut name_of,
            (l.at.0, l.at.1),
            NameCandidate { name: l.text.clone(), priority: 1, is_power: false },
        );
    }

    // No-connect markers — mark the net but don't name it.
    let nc_roots: std::collections::HashSet<u32> = sheet.no_connects.iter()
        .map(|nc: &NoConnect| {
            let p = QPoint::from(nc.at);
            let id = intern_into(&mut uf, &mut node_of, p);
            uf.find(id)
        })
        .collect();

    // ─── build output nets ───────────────────────────────────────
    // Group pins by net root.
    let mut by_root: HashMap<u32, Vec<NetPin>> = HashMap::new();
    for (si, pi, node) in pin_to_node {
        let root = uf.find(node);
        let sym: &SchematicSymbol = &sheet.symbols[si];
        let pp = &sym.pin_positions[pi];
        by_root.entry(root).or_default().push(NetPin {
            symbol_uuid: sym.uuid.clone(),
            pin_number: pp.pin_number.clone(),
            pin_name: pp.pin_name.clone(),
            electrical_type: pp.electrical_type,
        });
    }

    // Sort pin lists inside each net first, then sort the *entries*
    // by smallest pin, so auto-name numbering (Net_1, Net_2, …) is
    // deterministic across HashMap iteration orders. Without this
    // step, two extractions of the same sheet would assign different
    // numeric suffixes — fatal for canonical-netlist round-tripping.
    let mut entries: Vec<(u32, Vec<NetPin>)> = by_root.into_iter().map(|(root, mut pins)| {
        pins.sort_by(|a, b| {
            a.symbol_uuid.cmp(&b.symbol_uuid).then(a.pin_number.cmp(&b.pin_number))
        });
        (root, pins)
    }).collect();
    entries.sort_by(|(_, a), (_, b)| {
        let ka = a.first().map(|p| (&p.symbol_uuid, &p.pin_number));
        let kb = b.first().map(|p| (&p.symbol_uuid, &p.pin_number));
        ka.cmp(&kb)
    });

    let mut auto_counter: u32 = 0;
    let mut nets: Vec<Net> = entries.into_iter().map(|(root, pins)| {
        let (name, is_power) = match name_of.get(&root) {
            Some(c) => (c.name.clone(), c.is_power),
            None => {
                auto_counter += 1;
                (format!("Net_{}", auto_counter), false)
            }
        };
        Net { name, pins, is_power, no_connect: nc_roots.contains(&root) }
    }).collect();

    // Sort each net's pin list by (symbol_uuid, pin_number) for
    // stable test output.
    for net in nets.iter_mut() {
        net.pins.sort_by(|a, b| {
            a.symbol_uuid.cmp(&b.symbol_uuid).then(a.pin_number.cmp(&b.pin_number))
        });
    }
    nets.sort_by(|a, b| a.name.cmp(&b.name));

    NetList { nets }
}

// Helper, free function (closures can't be re-borrowed mutably
// across nested calls easily). Equivalent to the `intern` closure
// at the top of `extract_nets` — kept as a free fn so the naming
// pass can use it without lifetime fights.
fn intern_into(
    uf: &mut UnionFind,
    node_of: &mut HashMap<QPoint, u32>,
    p: QPoint,
) -> u32 {
    if let Some(&id) = node_of.get(&p) { return id; }
    let id = uf.make_set();
    node_of.insert(p, id);
    id
}

/// True if `p` lies on the line segment `a-b` within a small
/// tolerance. Uses the standard 2D cross-product (zero ⇒ collinear)
/// plus a dot-product parameter test (0 ≤ t ≤ 1 ⇒ within segment).
fn point_on_segment(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> bool {
    let ax = b.0 - a.0;
    let ay = b.1 - a.1;
    let px = p.0 - a.0;
    let py = p.1 - a.1;
    // Cross product = 2× signed-area of the triangle (a, b, p);
    // zero ⇒ collinear. Tolerance scaled by segment length so very
    // long segments don't accept far-off-axis points.
    let cross = ax * py - ay * px;
    let len2 = ax * ax + ay * ay;
    if len2 < 1e-9 { return false; }
    // Threshold: 1 micron² · length² ⇒ point must be within ~1 µm
    // of the line. Anything looser risks false positives on
    // near-parallel wires.
    if cross * cross > 1e-6 * len2 { return false; }
    // Parameter along the segment.
    let dot = px * ax + py * ay;
    dot >= -1e-6 && dot <= len2 + 1e-6
}

/// Map a power-symbol label to its canonical net name. Same
/// convention as the `_net:` entries in `kicad-symbol-mapping.toml`.
fn canonical_power_name(ps: &PowerSymbol) -> String {
    canonical_power_name_for_test(&ps.label, ps.category)
}

/// Standalone form usable by other modules (the emitter cross-
/// references this to decide ground-vs-supply for power-decl
/// emission).
pub fn canonical_power_name_for_test(label: &str, category: PowerCategory) -> String {
    let ps_like = (label, category);
    let label = ps_like.0;
    let category = ps_like.1;
    match label {
        "GND" | "Earth" => "GND".to_string(),
        "+5V"  => "VCC_5V".to_string(),
        "+3V3" | "+3.3V" => "VCC_3V3".to_string(),
        "+12V" => "VCC_12V".to_string(),
        "-12V" => "VEE_12V".to_string(),
        "VCC" => "VCC".to_string(),
        "VDD" => "VDD".to_string(),
        "VEE" => "VEE".to_string(),
        other => match category {
            PowerCategory::Ground => format!("GND_{}", sanitise(other)),
            _ => sanitise(other),
        }
    }
}

fn sanitise(s: &str) -> String {
    // Strip leading `+`/`-`, replace `.` with `V` (KiCad style: 3.3V → 3V3),
    // upper-case. Keep alnum + underscore.
    let s = s.trim_start_matches(['+']);
    let s = s.replace('.', "V").replace('-', "_NEG_");
    s.chars().map(|c| if c.is_alphanumeric() || c == '_' { c.to_ascii_uppercase() } else { '_' }).collect()
}

// ─────────────────────────────────────────────────────────────────
// Unit tests
// ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::*;
    use std::collections::HashMap;

    fn pin(num: &str, name: &str, x: f64, y: f64) -> PinPosition {
        PinPosition {
            pin_number: num.into(),
            pin_name: name.into(),
            electrical_type: PinElectricalType::Passive,
            at: (x, y),
        }
    }

    fn sym(uuid: &str, x: f64, y: f64, pins: Vec<PinPosition>) -> SchematicSymbol {
        SchematicSymbol {
            lib_id: "Device:R".into(),
            uuid: uuid.into(),
            at: (x, y, 0.0),
            mirror: None,
            unit: 1,
            properties: HashMap::new(),
            pin_positions: pins,
            in_bom: true,
            on_board: true,
            dnp: false,
        }
    }

    fn empty_sheet() -> Sheet {
        Sheet {
            path: std::path::PathBuf::from("test.kicad_sch"),
            uuid: "test-uuid".into(),
            lib_symbols: vec![],
            symbols: vec![],
            wires: vec![],
            junctions: vec![],
            no_connects: vec![],
            labels: vec![],
            global_labels: vec![],
            hierarchical_labels: vec![],
            power_symbols: vec![],
            sheet_refs: vec![],
            title_block: None,
        }
    }

    #[test]
    fn single_wire_connects_two_pins() {
        // R1.2 ──── R2.1, both at y=0
        let mut sheet = empty_sheet();
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![pin("2", "~", 10.0, 0.0)]));
        sheet.symbols.push(sym("R2", 20.0, 0.0, vec![pin("1", "~", 10.0, 0.0)]));
        // Both pins are at (10, 0) — they should unify without
        // even needing a wire (coincident pins).
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].pins.len(), 2);
    }

    #[test]
    fn wire_joins_distant_pins() {
        let mut sheet = empty_sheet();
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![pin("2", "~", 10.0, 0.0)]));
        sheet.symbols.push(sym("R2", 0.0, 0.0, vec![pin("1", "~", 30.0, 0.0)]));
        sheet.wires.push(Wire {
            start: (10.0, 0.0), end: (30.0, 0.0), uuid: "w1".into(),
        });
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 1);
        let n = &nl.nets[0];
        assert!(n.name.starts_with("Net_"));
        assert_eq!(n.pins.len(), 2);
    }

    #[test]
    fn label_names_a_net() {
        let mut sheet = empty_sheet();
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![pin("2", "~", 10.0, 0.0)]));
        sheet.symbols.push(sym("R2", 0.0, 0.0, vec![pin("1", "~", 30.0, 0.0)]));
        sheet.wires.push(Wire {
            start: (10.0, 0.0), end: (30.0, 0.0), uuid: "w1".into(),
        });
        sheet.labels.push(Label {
            text: "SIGNAL_A".into(),
            at: (20.0, 0.0, 0.0),
            uuid: "l1".into(),
        });
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "SIGNAL_A");
    }

    #[test]
    fn power_symbol_wins_over_label() {
        let mut sheet = empty_sheet();
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![pin("1", "~", 10.0, 0.0)]));
        sheet.power_symbols.push(PowerSymbol {
            label: "+5V".into(),
            at: (10.0, 0.0, 0.0),
            category: PowerCategory::Power,
            voltage: Some(5.0),
            uuid: "p1".into(),
        });
        sheet.labels.push(Label {
            text: "WRONG_NAME".into(),
            at: (10.0, 0.0, 0.0),
            uuid: "l1".into(),
        });
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].name, "VCC_5V");
        assert!(nl.nets[0].is_power);
    }

    #[test]
    fn no_connect_flagged() {
        let mut sheet = empty_sheet();
        sheet.symbols.push(sym("U1", 0.0, 0.0, vec![pin("7", "NC", 5.0, 5.0)]));
        sheet.no_connects.push(NoConnect {
            at: (5.0, 5.0), uuid: "nc1".into(),
        });
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 1);
        assert!(nl.nets[0].no_connect);
    }

    #[test]
    fn two_disjoint_nets() {
        let mut sheet = empty_sheet();
        // Net A: R1.1 -- R1.2
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![
            pin("1", "~", 0.0, 0.0),
            pin("2", "~", 10.0, 0.0),
        ]));
        // Net B: R2.1 -- R2.2, disjoint
        sheet.symbols.push(sym("R2", 0.0, 0.0, vec![
            pin("1", "~", 100.0, 100.0),
            pin("2", "~", 110.0, 100.0),
        ]));
        sheet.wires.push(Wire { start: (0.0, 0.0),     end: (10.0, 0.0),     uuid: "w1".into() });
        sheet.wires.push(Wire { start: (100.0, 100.0), end: (110.0, 100.0), uuid: "w2".into() });
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 2);
        for n in &nl.nets {
            assert_eq!(n.pins.len(), 2);
        }
    }

    #[test]
    fn junction_merges_t_intersection() {
        let mut sheet = empty_sheet();
        // Three wire endpoints all meet at (10, 0). KiCad places
        // a junction there to indicate the connection.
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![pin("1", "~", 0.0,  0.0)]));
        sheet.symbols.push(sym("R2", 0.0, 0.0, vec![pin("1", "~", 20.0, 0.0)]));
        sheet.symbols.push(sym("R3", 0.0, 0.0, vec![pin("1", "~", 10.0, 10.0)]));
        sheet.wires.push(Wire { start: (0.0,  0.0), end: (10.0, 0.0),  uuid: "w1".into() });
        sheet.wires.push(Wire { start: (10.0, 0.0), end: (20.0, 0.0),  uuid: "w2".into() });
        sheet.wires.push(Wire { start: (10.0, 0.0), end: (10.0, 10.0), uuid: "w3".into() });
        sheet.junctions.push(Junction { at: (10.0, 0.0), uuid: "j1".into() });
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].pins.len(), 3);
    }

    #[test]
    fn global_label_beats_local_label() {
        let mut sheet = empty_sheet();
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![pin("1", "~", 5.0, 5.0)]));
        sheet.labels.push(Label {
            text: "LOCAL".into(), at: (5.0, 5.0, 0.0), uuid: "l".into(),
        });
        sheet.global_labels.push(GlobalLabel {
            text: "GLOBAL".into(), at: (5.0, 5.0, 0.0),
            shape: GlobalLabelShape::Passive, uuid: "g".into(),
        });
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets[0].name, "GLOBAL");
    }

    #[test]
    fn pins_within_quantum_unify() {
        let mut sheet = empty_sheet();
        // Two pins separated by < 1 micron: should still unify.
        sheet.symbols.push(sym("R1", 0.0, 0.0, vec![pin("1", "~", 10.0, 0.0)]));
        sheet.symbols.push(sym("R2", 0.0, 0.0, vec![pin("1", "~", 10.0 + 1e-7, 0.0)]));
        let nl = extract_nets(&sheet);
        assert_eq!(nl.nets.len(), 1);
        assert_eq!(nl.nets[0].pins.len(), 2);
    }
}
