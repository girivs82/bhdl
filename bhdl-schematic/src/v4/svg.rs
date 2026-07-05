//! V4 SVG renderer — draws the classified SheetPlan as a real schematic
//! (docs/spec/Schematic_V4.md §3.2–3.3). All geometry is computed here, in
//! Rust, deterministically; the output is a self-contained SVG string.
//!
//! Idiom geometry (per stage):
//!   source flag ── source bus ──[IC box w/ pin stubs]── series ── target bus ── target flag
//!        shunt columns hang below their bus with ground symbols;
//!        the feedback divider taps the target bus and routes back into the
//!        IC's FB stub on an orthogonal path below the spine;
//!        residue parts render in a labeled fallback row above the spine
//!        with net flags — ugly but honest, and COUNTED.

use std::collections::HashMap;
use std::fmt::Write as _;

use bhdl_netlist::netlist::Netlist;
use bhdl_netlist::types::{NetClass, NetId};

use super::classify::{classify_sheet, BackboneElem, SheetPlan};
use crate::types::SimulationAnnotations;
use bhdl_common::symbol::{PinSide, SymbolDefinition};

/// Optional decoration inputs: GLACIER-solved values and stdlib-declared
/// symbol geometry (the parts' own `symbol { left {…} right {…} }` blocks —
/// idioms give PLACEMENT, symbol declarations give per-part pin GEOMETRY).
#[derive(Default)]
pub struct SheetDecor<'a> {
    pub sim: Option<&'a SimulationAnnotations>,
    pub symbols: Option<&'a std::collections::HashMap<String, SymbolDefinition>>,
    /// handle → synthesis refdes (R1/C3/U1…). Sheets label parts by REFDES —
    /// BHDL handles are long and descriptive (good for source, bad for ink).
    pub refdes: Option<&'a HashMap<String, String>>,
}

fn label_of<'a>(decor: &'a SheetDecor, inst: &'a str) -> &'a str {
    decor
        .refdes
        .and_then(|m| m.get(inst))
        .map(String::as_str)
        .unwrap_or(inst)
}

const SHUNT_PITCH: f64 = 70.0;
const IC_W: f64 = 130.0;
const IC_H: f64 = 100.0;
const SERIES_W: f64 = 70.0;
const STAGE_GAP: f64 = 300.0;

#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

impl Rect {
    fn pad(&self, p: f64) -> Rect {
        Rect { x0: self.x0 - p, y0: self.y0 - p, x1: self.x1 + p, y1: self.y1 + p }
    }
    fn hit(&self, x: f64, y: f64) -> bool {
        x >= self.x0 && x <= self.x1 && y >= self.y0 && y <= self.y1
    }
    fn overlaps(&self, o: &Rect) -> bool {
        self.x0 < o.x1 && o.x0 < self.x1 && self.y0 < o.y1 && o.y0 < self.y1
    }
}

struct Svg {
    body: String,
    w: f64,
    h: f64,
    /// Solid obstacles (symbol glyphs, text): wires must route AROUND,
    /// labels must not overlap.
    solids: Vec<Rect>,
    /// Wire segments: labels avoid them; routes may CROSS at a cost
    /// (a dotless crossing is legitimate schematic vocabulary).
    wire_segs: Vec<Rect>,
}

impl Svg {
    fn new() -> Self {
        Svg { body: String::new(), w: 0.0, h: 0.0, solids: Vec::new(), wire_segs: Vec::new() }
    }
    fn solid(&mut self, r: Rect) {
        self.solids.push(r);
    }
    fn grow(&mut self, x: f64, y: f64) {
        self.w = self.w.max(x);
        self.h = self.h.max(y);
    }
    fn wire(&mut self, pts: &[(f64, f64)]) {
        let s: Vec<String> = pts.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect();
        let _ = writeln!(
            self.body,
            r##"<polyline points="{}" fill="none" stroke="#222" stroke-width="1.6"/>"##,
            s.join(" ")
        );
        for &(x, y) in pts {
            self.grow(x + 20.0, y + 20.0);
        }
        for w in pts.windows(2) {
            let (a, b) = (w[0], w[1]);
            self.wire_segs.push(Rect {
                x0: a.0.min(b.0) - 1.0,
                y0: a.1.min(b.1) - 1.0,
                x1: a.0.max(b.0) + 1.0,
                y1: a.1.max(b.1) + 1.0,
            });
        }
    }
    fn dot(&mut self, x: f64, y: f64) {
        let _ = writeln!(self.body, r##"<circle cx="{x:.1}" cy="{y:.1}" r="3" fill="#222"/>"##);
    }
    fn text(&mut self, x: f64, y: f64, t: &str, cls: &str) {
        let esc = t.replace('&', "&amp;").replace('<', "&lt;");
        let _ = writeln!(self.body, r#"<text x="{x:.1}" y="{y:.1}" class="{cls}">{esc}</text>"#);
        self.grow(x + 8.0 * t.len() as f64, y + 14.0);
        self.solid(Rect { x0: x - 1.0, y0: y - 10.0, x1: x + 6.8 * t.len() as f64, y1: y + 3.0 });
    }
    fn text_rect(t: &str, x: f64, y: f64) -> Rect {
        Rect { x0: x - 1.0, y0: y - 10.0, x1: x + 6.8 * t.len() as f64, y1: y + 3.0 }
    }
    /// PLACE a label: try candidate offsets around the anchor until one
    /// neither overlaps a solid nor sits on a wire — no more fixed-offset
    /// collisions. Falls back to the first candidate if all collide
    /// (never drops information).
    fn place_label(&mut self, ax: f64, ay: f64, t: &str, cls: &str) {
        const CAND: [(f64, f64); 8] = [
            (8.0, 4.0),
            (8.0, -8.0),
            (8.0, 16.0),
            (-8.0, 4.0),   // left-of (adjusted by len below)
            (8.0, 28.0),
            (-8.0, -8.0),
            (20.0, 4.0),
            (8.0, 40.0),
        ];
        let len_w = 6.8 * t.len() as f64;
        for (dx, dy) in CAND {
            let x = if dx < 0.0 { ax + dx - len_w } else { ax + dx };
            let y = ay + dy;
            let r = Self::text_rect(t, x, y);
            let clear = !self.solids.iter().any(|s| s.overlaps(&r.pad(1.0)))
                && !self.wire_segs.iter().any(|w| w.overlaps(&r));
            if clear {
                self.text(x, y, t, cls);
                return;
            }
        }
        self.text(ax + 8.0, ay + 4.0, t, cls);
    }
    /// Place a SIM decoration near its subject or not at all: only the
    /// close candidate slots are tried — a solved value that drifts into
    /// another net's lane reads as annotating THAT net (misplacement is
    /// worse than absence for decorations; the report still carries the
    /// number).
    fn place_sim_label(&mut self, ax: f64, ay: f64, t: &str, cls: &str) {
        const NEAR: [(f64, f64); 3] = [(8.0, 4.0), (8.0, -8.0), (-8.0, 4.0)];
        let len_w = 6.8 * t.len() as f64;
        for (dx, dy) in NEAR {
            let x = if dx < 0.0 { ax + dx - len_w } else { ax + dx };
            let y = ay + dy;
            let r = Self::text_rect(t, x, y);
            let clear = !self.solids.iter().any(|s| s.overlaps(&r.pad(1.0)))
                && !self.wire_segs.iter().any(|w| w.overlaps(&r));
            if clear {
                self.text(x, y, t, cls);
                return;
            }
        }
        // No clear near slot — drop the decoration.
    }
    /// Route an orthogonal wire from `from` to `to`: BFS on an 8px grid.
    /// Solids are WALLS (minus the endpoints' own hosts); existing wires
    /// cross at a cost; bends cost extra. Falls back to a plain L if the
    /// search fails — drawn anyway, never dropped.
    fn route(&mut self, from: (f64, f64), to: (f64, f64)) {
        const G: f64 = 8.0;
        let snap = |v: f64| (v / G).round() as i32;
        let (sx, sy) = (snap(from.0), snap(from.1));
        let (tx, ty) = (snap(to.0), snap(to.1));
        let solids: Vec<Rect> = self
            .solids
            .iter()
            .filter(|r| !r.pad(2.0).hit(from.0, from.1) && !r.pad(2.0).hit(to.0, to.1))
            .cloned()
            .collect();
        let blocked = |x: i32, y: i32| -> bool {
            let (px, py) = (x as f64 * G, y as f64 * G);
            solids.iter().any(|r| r.pad(3.0).hit(px, py))
        };
        let wire_cost = |x: i32, y: i32| -> u32 {
            let (px, py) = (x as f64 * G, y as f64 * G);
            if self.wire_segs.iter().any(|r| r.hit(px, py)) { 6 } else { 0 }
        };
        use std::cmp::Reverse;
        use std::collections::{BinaryHeap, HashMap as Map};
        let mut best: Map<(i32, i32, u8), u32> = Map::new();
        let mut prev: Map<(i32, i32, u8), (i32, i32, u8)> = Map::new();
        let mut heap = BinaryHeap::new();
        for d in 0..4u8 {
            heap.push(Reverse((0u32, sx, sy, d)));
            best.insert((sx, sy, d), 0);
        }
        const DIRS: [(i32, i32); 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
        let mut goal: Option<(i32, i32, u8)> = None;
        let mut expanded = 0usize;
        while let Some(Reverse((c, x, y, d))) = heap.pop() {
            if best.get(&(x, y, d)).copied().unwrap_or(u32::MAX) < c {
                continue;
            }
            if x == tx && y == ty {
                goal = Some((x, y, d));
                break;
            }
            expanded += 1;
            if expanded > 40_000 {
                break;
            }
            for (nd, (dx, dy)) in DIRS.iter().enumerate() {
                let (nx, ny) = (x + dx, y + dy);
                if (nx - sx).abs() > 200 || (ny - sy).abs() > 200 {
                    continue;
                }
                if blocked(nx, ny) && !(nx == tx && ny == ty) {
                    continue;
                }
                let bend = if nd as u8 != d { 5 } else { 0 };
                let nc = c + 1 + bend + wire_cost(nx, ny);
                let key = (nx, ny, nd as u8);
                if nc < best.get(&key).copied().unwrap_or(u32::MAX) {
                    best.insert(key, nc);
                    prev.insert(key, (x, y, d));
                    heap.push(Reverse((nc, nx, ny, nd as u8)));
                }
            }
        }
        let pts: Vec<(f64, f64)> = if let Some(mut cur) = goal {
            let mut cells = vec![(cur.0, cur.1)];
            while let Some(&p) = prev.get(&cur) {
                if (p.0, p.1) != (*cells.last().unwrap()) {
                    cells.push((p.0, p.1));
                }
                if p.0 == sx && p.1 == sy {
                    break;
                }
                cur = p;
            }
            cells.push((sx, sy));
            cells.reverse();
            // Collapse collinear runs.
            let mut out: Vec<(f64, f64)> = vec![from];
            for w in cells.windows(3) {
                let (a, b, c2) = (w[0], w[1], w[2]);
                let col = (a.0 == b.0 && b.0 == c2.0) || (a.1 == b.1 && b.1 == c2.1);
                if !col {
                    out.push((b.0 as f64 * G, b.1 as f64 * G));
                }
            }
            out.push(to);
            out
        } else {
            vec![from, (from.0, to.1), to]
        };
        // ORTHOGONALITY GUARANTEE: the exact endpoints don't lie on the
        // routing grid, and a degenerate search (start and goal snapping
        // into adjacent cells) can yield a direct segment — insert the
        // missing corner wherever a step moves in BOTH axes. Schematic
        // wires are orthogonal, always.
        let mut ortho: Vec<(f64, f64)> = Vec::with_capacity(pts.len() + 2);
        ortho.push(pts[0]);
        for &p in &pts[1..] {
            let l = *ortho.last().unwrap();
            if (l.0 - p.0).abs() > 0.01 && (l.1 - p.1).abs() > 0.01 {
                ortho.push((p.0, l.1));
            }
            if (ortho.last().unwrap().0 - p.0).abs() > 0.01
                || (ortho.last().unwrap().1 - p.1).abs() > 0.01
            {
                ortho.push(p);
            }
        }
        self.wire(&ortho);
    }
    /// Ground symbol, stem entering at (x, y) from above.
    fn ground(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x - 13.0, y0: y + 9.0, x1: x + 13.0, y1: y + 22.0 });
        self.wire(&[(x, y), (x, y + 10.0)]);
        for (i, w) in [12.0, 8.0, 4.0].iter().enumerate() {
            let yy = y + 10.0 + i as f64 * 5.0;
            self.wire(&[(x - w, yy), (x + w, yy)]);
        }
    }
    /// Rail flag: a labeled arrow-tag at (x, y) opening toward +x or −x.
    fn rail_flag(&mut self, x: f64, y: f64, label: &str, leftward: bool) {
        let d = if leftward { -1.0 } else { 1.0 };
        self.wire(&[(x, y), (x + d * 14.0, y - 10.0)]);
        self.wire(&[(x, y), (x + d * 14.0, y + 10.0)]);
        // Label always sits INSIDE the sheet: above the flag, start-anchored
        // just past the glyph (a leftward flag at the sheet edge would
        // otherwise clip its label off-canvas).
        let tx = x + d.min(0.0) * 0.0 + 4.0;
        self.text(tx, y - 14.0, label, "rail");
    }
    /// Capacitor drawn vertically: wire in at top (x, y), out at (x, y+34).
    fn cap_v(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x - 12.0, y0: y + 11.0, x1: x + 12.0, y1: y + 23.0 });
        self.wire(&[(x, y), (x, y + 13.0)]);
        self.wire(&[(x - 11.0, y + 13.0), (x + 11.0, y + 13.0)]);
        self.wire(&[(x - 11.0, y + 21.0), (x + 11.0, y + 21.0)]);
        self.wire(&[(x, y + 21.0), (x, y + 34.0)]);
    }
    /// Resistor drawn vertically (IEC box): in top (x,y), out (x, y+40).
    fn res_v(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x - 9.0, y0: y + 7.0, x1: x + 9.0, y1: y + 33.0 });
        self.wire(&[(x, y), (x, y + 8.0)]);
        let _ = writeln!(
            self.body,
            r##"<rect x="{:.1}" y="{:.1}" width="16" height="24" fill="none" stroke="#222" stroke-width="1.6"/>"##,
            x - 8.0,
            y + 8.0
        );
        self.wire(&[(x, y + 32.0), (x, y + 40.0)]);
    }
    /// Inductor drawn horizontally: in left (x,y), out (x+56, y).
    fn ind_h(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x + 7.0, y0: y - 7.0, x1: x + 49.0, y1: y + 2.0 });
        self.wire(&[(x, y), (x + 8.0, y)]);
        for k in 0..4 {
            let cx = x + 8.0 + 5.0 + k as f64 * 10.0;
            let _ = writeln!(
                self.body,
                r##"<path d="M {:.1} {:.1} A 5 5 0 0 1 {:.1} {:.1}" fill="none" stroke="#222" stroke-width="1.6"/>"##,
                cx - 5.0, y, cx + 5.0, y
            );
        }
        self.wire(&[(x + 48.0, y), (x + 56.0, y)]);
    }
    /// Diode drawn vertically, anode at top (x, y), cathode at (x, y+34).
    fn diode_v(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x - 10.0, y0: y + 9.0, x1: x + 10.0, y1: y + 25.0 });
        self.wire(&[(x, y), (x, y + 10.0)]);
        let _ = writeln!(
            self.body,
            r##"<path d="M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z" fill="none" stroke="#222" stroke-width="1.6"/>"##,
            x - 9.0, y + 10.0, x + 9.0, y + 10.0, x, y + 24.0
        );
        self.wire(&[(x - 9.0, y + 24.0), (x + 9.0, y + 24.0)]);
        self.wire(&[(x, y + 24.0), (x, y + 34.0)]);
    }
    /// Diode drawn vertically with CATHODE at top (x, y) — the catch-diode
    /// orientation: current flows from ground up into the node.
    fn diode_v_up(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x - 10.0, y0: y + 9.0, x1: x + 10.0, y1: y + 25.0 });
        self.wire(&[(x, y), (x, y + 10.0)]);
        self.wire(&[(x - 9.0, y + 10.0), (x + 9.0, y + 10.0)]);
        let _ = writeln!(
            self.body,
            r##"<path d="M {:.1} {:.1} L {:.1} {:.1} L {:.1} {:.1} Z" fill="none" stroke="#222" stroke-width="1.6"/>"##,
            x - 9.0, y + 24.0, x + 9.0, y + 24.0, x, y + 10.0
        );
        self.wire(&[(x, y + 24.0), (x, y + 34.0)]);
    }
    /// Capacitor drawn horizontally: in left (x, y), out at (x+34, y).
    fn cap_h(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x + 11.0, y0: y - 12.0, x1: x + 23.0, y1: y + 12.0 });
        self.wire(&[(x, y), (x + 13.0, y)]);
        self.wire(&[(x + 13.0, y - 11.0), (x + 13.0, y + 11.0)]);
        self.wire(&[(x + 21.0, y - 11.0), (x + 21.0, y + 11.0)]);
        self.wire(&[(x + 21.0, y), (x + 34.0, y)]);
    }
    /// Generic 2-terminal fallback (horizontal box).
    fn box_h(&mut self, x: f64, y: f64, w: f64) {
        self.solid(Rect { x0: x, y0: y - 10.0, x1: x + w, y1: y + 10.0 });
        let _ = writeln!(
            self.body,
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="18" fill="none" stroke="#222" stroke-width="1.6"/>"##,
            x, y - 9.0, w
        );
    }

    fn finish(self, title: &str) -> String {
        format!(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w:.0} {h:.0}\" \
             width=\"{w:.0}\" height=\"{h:.0}\" font-family=\"sans-serif\">\n\
             <style>text{{font-size:11px;fill:#333}}.rail{{fill:#a00;font-weight:600}}\
             .ref{{font-weight:600}}.val{{fill:#555}}.part{{fill:#333;font-weight:600}}\
             .absent{{fill:#a60;font-style:italic}}.sim{{fill:#06c;font-style:italic}}</style>\n\
             <rect width=\"{w:.0}\" height=\"{h:.0}\" fill=\"white\"/>\n\
             <text x=\"16\" y=\"22\" class=\"part\">{title}</text>\n{body}</svg>\n",
            w = self.w + 30.0,
            h = self.h + 30.0,
            title = title,
            body = self.body
        )
    }
}

fn solved_v(decor: &SheetDecor, netlist: &Netlist, id: NetId) -> Option<f64> {
    let name = netlist.nets.get(id)?.name.clone()?;
    decor.sim?.net_voltages.get(&name).copied()
}

/// Engineering current format. Returns None below 1µA — a numerically-zero
/// annotation conveys nothing (and a fixed "%.2fA" format truncated a real
/// 80µA divider-only load into a meaningless 0.00A).
fn fmt_sim_i(i: f64) -> Option<String> {
    let a = i.abs();
    if a >= 1.0 {
        Some(format!("{a:.2}A"))
    } else if a >= 1e-3 {
        Some(format!("{:.1}mA", a * 1e3))
    } else if a >= 1e-6 {
        Some(format!("{:.0}µA", a * 1e6))
    } else {
        None
    }
}

fn fmt_sim_v(v: f64) -> String {
    if v.abs() >= 1.0 { format!("{v:.2}V") } else { format!("{:.0}mV", v * 1000.0) }
}

fn net_label(netlist: &Netlist, id: NetId) -> String {
    let net = match netlist.nets.get(id) {
        Some(n) => n,
        None => return String::new(),
    };
    let name = net.name.clone().unwrap_or_default();
    match net.net_class {
        NetClass::Power { voltage, .. } => format!("{name} ({voltage:.1}V)"),
        _ => name,
    }
}

fn value_of(netlist: &Netlist, inst: &str) -> String {
    netlist
        .instances
        .values()
        .find(|i| i.name == inst)
        .and_then(|i| i.attributes.get("value").cloned())
        .unwrap_or_default()
}

fn class_of_name(netlist: &Netlist, inst: &str) -> String {
    netlist
        .instances
        .values()
        .find(|i| i.name == inst)
        .and_then(|i| i.attributes.get("component_class").cloned())
        .unwrap_or_default()
}

/// A shunt column: stem from the bus at (x, y_bus), symbol, ground.
fn draw_shunt(svg: &mut Svg, netlist: &Netlist, decor: &SheetDecor, inst: &str, x: f64, y_bus: f64) {
    svg.dot(x, y_bus);
    let class = class_of_name(netlist, inst);
    let sym_top = y_bus + 16.0;
    svg.wire(&[(x, y_bus), (x, sym_top)]);
    let sym_bot = match class.as_str() {
        "resistor" => {
            svg.res_v(x, sym_top);
            sym_top + 40.0
        }
        "diode" => {
            // Orientation from the netlist: if the CATHODE (K) sits on the
            // tap net, current flows ground→node (catch diode) — bar at
            // the top. Otherwise anode-at-top.
            let cathode_up = netlist
                .instances
                .iter()
                .find(|(_, i)| i.name == inst)
                .map(|(iid, _)| {
                    netlist.pin_instances.values().any(|pi| {
                        pi.instance == iid
                            && netlist
                                .pins
                                .get(pi.pin_def)
                                .map(|p| p.name == "K")
                                .unwrap_or(false)
                            && pi.net.is_some()
                            && netlist
                                .nets
                                .get(pi.net.unwrap())
                                .map(|n| !matches!(n.net_class, NetClass::Ground))
                                .unwrap_or(false)
                    })
                })
                .unwrap_or(false);
            if cathode_up {
                svg.diode_v_up(x, sym_top);
            } else {
                svg.diode_v(x, sym_top);
            }
            sym_top + 34.0
        }
        _ => {
            svg.cap_v(x, sym_top);
            sym_top + 34.0
        }
    };
    svg.wire(&[(x, sym_bot), (x, sym_bot + 10.0)]);
    svg.ground(x, sym_bot + 10.0);
    svg.place_label(x, sym_top + 8.0, label_of(decor, inst), "ref");
    let v = value_of(netlist, inst);
    if !v.is_empty() {
        svg.place_label(x, sym_top + 24.0, &v, "val");
    }
}

/// Render one stage; returns the height consumed.
#[allow(clippy::too_many_lines)]
fn draw_stage(
    svg: &mut Svg,
    netlist: &Netlist,
    plan: &SheetPlan,
    stage_idx: usize,
    y0: f64,
    decor: &SheetDecor,
) -> f64 {
    let stage = &plan.stages[stage_idx];
    let spine = y0 + 120.0;
    let mut x = 40.0;

    // Source flag + bus (+ solved operating point when GLACIER ran).
    svg.rail_flag(x, spine, &net_label(netlist, stage.source_rail), true);
    if let Some(v) = solved_v(decor, netlist, stage.source_rail) {
        svg.place_sim_label(x + 4.0, spine + 20.0, &format!("= {}", fmt_sim_v(v)), "sim");
    }
    x += 20.0;
    let src_bus_start = x;

    // Source-rail shunts (input bank).
    let src_shunts: Vec<&str> = stage
        .shunts
        .iter()
        .filter(|s| s.tap == stage.source_rail)
        .map(|s| s.inst.as_str())
        .collect();
    for inst in &src_shunts {
        x += SHUNT_PITCH;
        draw_shunt(svg, netlist, decor, inst, x, spine);
    }
    x += SHUNT_PITCH;

    // Backbone.
    let mut fb_stub: Option<(f64, f64)> = None;
    let mut ic_right = x;
    let mut mid_shunt_zone: Vec<(String, f64)> = Vec::new();
    for elem in &stage.backbone {
        match elem {
            BackboneElem::Ic { inst, in_pin, out_pin } => {
                // Bus into the IC.
                svg.wire(&[(src_bus_start, spine), (x, spine)]);
                let bx = x;
                let by = spine - IC_H / 2.0;
                let _ = writeln!(
                    svg.body,
                    r##"<rect x="{bx:.1}" y="{by:.1}" width="{IC_W:.1}" height="{IC_H:.1}" fill="#f7f7f2" stroke="#222" stroke-width="1.8"/>"##
                );
                svg.grow(bx + IC_W, by + IC_H);
                svg.solid(Rect { x0: bx, y0: by, x1: bx + IC_W, y1: by + IC_H });
                svg.text(bx + 8.0, by - 6.0, label_of(decor, inst), "ref");
                // Part name inside.
                let part = netlist
                    .instances
                    .values()
                    .find(|i| i.name == *inst)
                    .and_then(|i| netlist.modules.get(i.definition).map(|m| m.name.clone()))
                    .unwrap_or_default();
                svg.text(bx + 8.0, by + 16.0, &part, "part");
                if let Some(p) = decor.sim.and_then(|s| s.instance_power.get(inst)) {
                    if *p > 1e-3 {
                        svg.place_sim_label(bx + 8.0, by + 30.0, &format!("{:.2}W", p), "sim");
                    }
                }
                // in pin stub (left mid) — flow pins are IDIOM-owned: the
                // sheet reads left→right, so in/out stay on the flow sides
                // regardless of declaration.
                svg.text(bx + 6.0, spine + 4.0, in_pin, "val");
                // out pin stub (right, upper-mid).
                let out_y = spine;
                svg.text(bx + IC_W - 8.0 * out_pin.len() as f64, out_y - 5.0, out_pin, "val");
                // GND stub (bottom center) + ground symbol.
                let gx = bx + IC_W / 2.0;
                svg.wire(&[(gx, by + IC_H), (gx, by + IC_H + 10.0)]);
                svg.ground(gx, by + IC_H + 10.0);
                svg.text(gx + 6.0, by + IC_H + 12.0, "GND", "val");

                // ── Aux pins: DECLARED sides drive the stubs ──
                // Every CONNECTED pin that isn't flow or ground gets a stub.
                // Side = the part's `symbol {}` declaration (stdlib-assisted
                // drawing); heuristic fallback: loop→Right, strap→Top,
                // In→Left, else Right. Slots fill outward from the flow row
                // in declaration order.
                let declared_side = |pin: &str| -> Option<PinSide> {
                    decor
                        .symbols
                        .and_then(|m| m.get(&part))
                        .and_then(|sd| sd.pin_sides().get(pin).copied())
                };
                let loop_pin: Option<&str> = stage
                    .loops
                    .iter()
                    .find(|l| l.into_inst == *inst)
                    .map(|l| l.into_pin.as_str());
                let strap_pins: Vec<&str> =
                    stage.straps.iter().map(|st| st.ic_pin.as_str()).collect();
                let aux: Vec<(String, PinSide, Option<NetId>)> = netlist
                    .instances
                    .iter()
                    .find(|(_, i)| i.name == *inst)
                    .map(|(iid, _)| {
                        netlist
                            .pin_instances
                            .values()
                            .filter(|pi| pi.instance == iid && pi.net.is_some())
                            .filter_map(|pi| {
                                let pin = netlist.pins.get(pi.pin_def)?;
                                if pin.is_virtual {
                                    // Virtual pins model post-network nodes
                                    // (TPS54331's VOUT = the rail after the
                                    // LC) — they are not package pins and
                                    // never draw on the body; the rail flag
                                    // already represents that node.
                                    return None;
                                }
                                if pin.name == *in_pin || pin.name == *out_pin {
                                    return None;
                                }
                                if matches!(
                                    pin.direction,
                                    bhdl_netlist::types::PinDirection::Ground
                                ) {
                                    return None;
                                }
                                let side = declared_side(&pin.name).unwrap_or_else(|| {
                                    if Some(pin.name.as_str()) == loop_pin {
                                        PinSide::Right
                                    } else if strap_pins.contains(&pin.name.as_str()) {
                                        PinSide::Top
                                    } else if matches!(
                                        pin.direction,
                                        bhdl_netlist::types::PinDirection::In
                                    ) {
                                        PinSide::Left
                                    } else {
                                        PinSide::Right
                                    }
                                });
                                Some((pin.name.clone(), side, pi.net))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                // Slot coordinates per side; record each stub endpoint.
                let mut slot: HashMap<&'static str, usize> = HashMap::new();
                let mut stubs: HashMap<String, (f64, f64, PinSide)> = HashMap::new();
                for (pname, side, _net) in &aux {
                    let k = {
                        let key = match side {
                            PinSide::Left => "L",
                            PinSide::Right => "R",
                            PinSide::Top => "T",
                            PinSide::Bottom => "B",
                        };
                        let e = slot.entry(key).or_insert(0);
                        let k = *e;
                        *e += 1;
                        k
                    };
                    let (sx, sy, lx, ly) = match side {
                        PinSide::Left => {
                            let y = spine + 24.0 + k as f64 * 20.0;
                            (bx - 14.0, y, bx + 6.0, y + 4.0)
                        }
                        PinSide::Right => {
                            let y = spine + 24.0 + k as f64 * 20.0;
                            (bx + IC_W + 14.0, y, bx + IC_W - 8.0 * pname.len() as f64, y + 4.0)
                        }
                        PinSide::Top => {
                            let xx = bx + IC_W * 0.72 - k as f64 * 24.0;
                            (xx, by - 14.0, xx - 8.0 * pname.len() as f64 + 6.0, by + 12.0)
                        }
                        PinSide::Bottom => {
                            let xx = gx + 26.0 + k as f64 * 24.0;
                            (xx, by + IC_H + 14.0, xx + 4.0, by + IC_H - 4.0)
                        }
                    };
                    // Stub line from the box edge to the endpoint.
                    match side {
                        PinSide::Left => svg.wire(&[(bx, sy), (sx, sy)]),
                        PinSide::Right => svg.wire(&[(bx + IC_W, sy), (sx, sy)]),
                        PinSide::Top => svg.wire(&[(sx, by), (sx, sy)]),
                        PinSide::Bottom => svg.wire(&[(sx, by + IC_H), (sx, sy)]),
                    }
                    svg.text(lx, ly, pname, "val");
                    stubs.insert(pname.clone(), (sx, sy, *side));
                }

                if let Some(lp) = loop_pin {
                    fb_stub = stubs.get(lp).map(|(x, y, _)| (*x, *y));
                }
                x = bx + IC_W + 34.0; // room after the box for strap taps
                svg.wire(&[(bx + IC_W, out_y), (x, out_y)]);
                ic_right = x;

                // Intermediate-net shunts (the catch diode on the switch
                // node): everything the classifier claimed must be drawn —
                // they hang off the out-net segment before the series
                // element.
                let mid_shunts: Vec<&str> = stage
                    .shunts
                    .iter()
                    .filter(|sh| {
                        sh.tap != stage.source_rail && sh.tap != stage.target_rail
                    })
                    .map(|sh| sh.inst.as_str())
                    .collect();
                for m in &mid_shunts {
                    x += SHUNT_PITCH;
                    draw_shunt(svg, netlist, decor, m, x, out_y);
                }
                if !mid_shunts.is_empty() {
                    x += 24.0; // label clearance before the series element
                    svg.wire(&[(ic_right, out_y), (x, out_y)]);
                    ic_right = x;
                }

                // ── Route aux nets ──
                // source-rail ties (EN → VIN): hook from the stub back onto
                // the input bus with a junction dot; anything not resolved
                // by an idiom gets a named NET FLAG — long-range notation,
                // honest and unambiguous.
                for (pname, _side, net) in &aux {
                    let Some((sx, sy, side)) = stubs.get(pname).copied() else { continue };
                    let Some(net) = net else { continue };
                    if *net == stage.source_rail {
                        match side {
                            PinSide::Left => {
                                let hook_x = bx - 16.0;
                                svg.route((sx, sy), (hook_x, spine));
                                svg.dot(hook_x, spine);
                            }
                            _ => {
                                svg.text(sx + 4.0, sy - 6.0, &net_label(netlist, *net), "rail");
                            }
                        }
                    } else if Some(pname.as_str()) == loop_pin
                        || strap_pins.contains(&pname.as_str())
                    {
                        // drawn by the loop/strap idioms below
                    } else {
                        // Named net flag.
                        let nname = netlist
                            .nets
                            .get(*net)
                            .and_then(|n| n.name.clone())
                            .unwrap_or_default();
                        if !nname.is_empty() {
                            svg.text(sx + 4.0, sy - 6.0, &nname, "rail");
                        }
                    }
                }

                // ── Straps: bridge from the DECLARED stub over the clear
                // airspace above the IC, tap onto the out-net segment. ──
                for strap in stage.straps.iter() {
                    let Some((sx, sy, side)) = stubs.get(&strap.ic_pin).copied() else {
                        continue;
                    };
                    let bridge_y = by - 26.0;
                    let tap_x = bx + IC_W + 20.0;
                    match side {
                        PinSide::Top => svg.wire(&[(sx, sy), (sx, bridge_y)]),
                        PinSide::Left => {
                            let fl = bx - 16.0;
                            svg.wire(&[(sx, sy), (fl, sy), (fl, bridge_y), (sx, bridge_y)]);
                        }
                        _ => svg.wire(&[(sx, sy), (sx, bridge_y)]),
                    }
                    let cap_x = tap_x - 40.0;
                    svg.wire(&[(sx.min(cap_x), bridge_y), (cap_x, bridge_y)]);
                    // Symbol by class — a strap can be an inductor or a
                    // resistor, not only the bootstrap cap.
                    let sym_w = match class_of_name(netlist, &strap.inst).as_str() {
                        "inductor" => {
                            svg.ind_h(cap_x, bridge_y);
                            56.0
                        }
                        "resistor" => {
                            svg.box_h(cap_x, bridge_y, 34.0);
                            34.0
                        }
                        _ => {
                            svg.cap_h(cap_x, bridge_y);
                            34.0
                        }
                    };
                    svg.wire(&[(cap_x + sym_w, bridge_y), (tap_x, bridge_y), (tap_x, out_y)]);
                    svg.dot(tap_x, out_y);
                    svg.place_label(cap_x - 8.0, bridge_y - 14.0, label_of(decor, &strap.inst), "ref");
                    let v = value_of(netlist, &strap.inst);
                    if !v.is_empty() {
                        svg.place_label(cap_x - 8.0, bridge_y + 14.0, &v, "val");
                    }
                }
            }
            BackboneElem::Series { inst } => {
                let class = class_of_name(netlist, inst);
                svg.wire(&[(x, spine), (x + 7.0, spine)]);
                match class.as_str() {
                    "inductor" => svg.ind_h(x + 7.0, spine),
                    _ => svg.box_h(x + 7.0, spine, 56.0),
                }
                svg.place_label(x + 14.0, spine - 16.0, label_of(decor, inst), "ref");
                let v = value_of(netlist, inst);
                if !v.is_empty() {
                    svg.place_label(x + 14.0, spine + 18.0, &v, "val");
                }
                if let Some(txt) = decor
                    .sim
                    .and_then(|s| s.instance_currents.get(inst))
                    .and_then(|i| fmt_sim_i(*i))
                {
                    svg.place_sim_label(x + 14.0, spine + 32.0, &txt, "sim");
                }
                let _ = &mid_shunt_zone;
                x += 7.0 + 56.0;
                svg.wire(&[(x, spine), (x + 7.0, spine)]);
                x += 7.0;
            }
        }
    }

    // Target bus + shunts (output bank).
    let tgt_bus_start = x;
    let tgt_shunts: Vec<&str> = stage
        .shunts
        .iter()
        .filter(|s| s.tap == stage.target_rail)
        .map(|s| s.inst.as_str())
        .collect();
    for inst in &tgt_shunts {
        x += SHUNT_PITCH;
        draw_shunt(svg, netlist, decor, inst, x, spine);
    }

    // Feedback divider off the target bus (extra clearance from the last
    // shunt column so labels don't crowd).
    if let Some(l) = stage.loops.first() {
        x += SHUNT_PITCH + 24.0;
        let dx = x;
        svg.dot(dx, spine);
        svg.wire(&[(dx, spine), (dx, spine + 10.0)]);
        // top leg
        svg.res_v(dx, spine + 10.0);
        svg.place_label(dx, spine + 24.0, label_of(decor, l.insts.first().map(String::as_str).unwrap_or("")), "ref");
        let mid = spine + 50.0;
        svg.dot(dx, mid);
        // Solved FB-node voltage (the reference the loop regulates to).
        if let Some(sim) = decor.sim {
            let fb_net_v = netlist
                .instances
                .iter()
                .find(|(_, i)| i.name == l.into_inst)
                .and_then(|(iid, _)| {
                    netlist.pin_instances.values().find(|pi| {
                        pi.instance == iid
                            && netlist
                                .pins
                                .get(pi.pin_def)
                                .map(|p| p.name == l.into_pin)
                                .unwrap_or(false)
                    })
                })
                .and_then(|pi| pi.net)
                .and_then(|nid| netlist.nets.get(nid).and_then(|n| n.name.clone()))
                .and_then(|name| sim.net_voltages.get(&name).copied());
            if let Some(v) = fb_net_v {
                svg.place_sim_label(dx + 12.0, mid, &format!("= {}", fmt_sim_v(v)), "sim");
            }
        }
        // bottom leg
        if l.insts.len() > 1 {
            svg.res_v(dx, mid);
            svg.place_label(dx, mid + 14.0, label_of(decor, &l.insts[1]), "ref");
            svg.wire(&[(dx, mid + 40.0), (dx, mid + 48.0)]);
            svg.ground(dx, mid + 48.0);
        }
        // Return path policy: AVOID crossings when possible; cross only
        // under congestion. The non-crossing route exists here: exit the
        // mid node on the divider's NEAR (left) flank, dive down in the
        // clear channel between the divider and the last shunt column, run
        // under the ground row, and rise into the FB stub — no crossings,
        // and no far-side detour (the earlier loop-under exited RIGHT,
        // adding travel away from the pin it was connecting).
        if let Some((fx, fy)) = fb_stub {
            // Routed by the micro-PnR: symbols and labels are walls, plain
            // wires crossable at cost, bends penalized — the near-flank
            // loop-under emerges from the costs instead of hand-picked
            // coordinates that break when anchors move.
            svg.route((dx, mid), (fx, fy));
        }
        x += SHUNT_PITCH;
    }

    // Close the target bus and flag it.
    svg.wire(&[(tgt_bus_start, spine), (x, spine)]);
    svg.rail_flag(x, spine, &net_label(netlist, stage.target_rail), false);
    if let Some(v) = solved_v(decor, netlist, stage.target_rail) {
        svg.place_sim_label(x + 4.0, spine + 20.0, &format!("= {}", fmt_sim_v(v)), "sim");
    }
    // Also draw the source bus under its shunts.
    svg.wire(&[(src_bus_start, spine), (tgt_bus_start.min(src_bus_start + 1.0).max(src_bus_start), spine)]);

    spine + 230.0 - y0
}


/// Render the whole sheet. Returns (svg, unidiomized_count).
pub fn render_sheet_svg(netlist: &Netlist, title: &str, decor: &SheetDecor) -> (String, usize) {
    let plan = classify_sheet(netlist);
    log::info!(
        "v4 plan: {} rails, {} grounds, {} stages, {} residue",
        plan.rails.len(), plan.grounds.len(), plan.stages.len(), plan.residue.len()
    );
    let mut svg = Svg::new();
    let mut y = 30.0;

    for i in 0..plan.stages.len() {
        let used = draw_stage(&mut svg, netlist, &plan, i, y, decor);
        y += used.max(STAGE_GAP);
    }

    // Residue: honest fallback row with net flags.
    if !plan.residue.is_empty() {
        svg.text(
            40.0,
            y + 10.0,
            &format!("unidiomized ({}):", plan.residue.len()),
            "absent",
        );
        let mut x = 40.0;
        for inst in &plan.residue {
            let ry = y + 30.0;
            svg.box_h(x, ry, 56.0);
            svg.text(x + 4.0, ry - 14.0, &format!("{} ({inst})", label_of(decor, inst)), "ref");
            // Net flags for each connected pin.
            let mut fy = ry + 22.0;
            if let Some((iid, _)) = netlist.instances.iter().find(|(_, i)| i.name == *inst) {
                for pi in netlist.pin_instances.values().filter(|p| p.instance == iid) {
                    let (Some(pin), Some(nid)) = (netlist.pins.get(pi.pin_def), pi.net) else {
                        continue;
                    };
                    let nname = netlist
                        .nets
                        .get(nid)
                        .and_then(|n| n.name.clone())
                        .unwrap_or_default();
                    svg.text(x + 4.0, fy, &format!("{}={}", pin.name, nname), "val");
                    fy += 13.0;
                }
            }
            x += 150.0;
        }
        y += 110.0;
    }

    let n_res = plan.residue.len();
    svg.grow(200.0, y);
    (svg.finish(title), n_res)
}
