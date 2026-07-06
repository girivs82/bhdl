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

use super::classify::{classify_sheet, BackboneElem, ChainElem, SheetPlan};
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
    /// SIM decorations queued for post-route placement.
    pending_sims: Vec<(f64, f64, String, String)>,
    /// Placement failures: labels that found NO clear slot and fell back
    /// to an overlapping position. The sweep gates on this — sheet
    /// quality as a NUMBER, like the unidiomized count.
    collisions: usize,
    /// Reserved WIRING CHANNELS: the stage declares its loop-under lane
    /// and riser lane up front. Labels must stay out (a label parked in a
    /// channel blocks the route that needs it — the chicken-and-egg that
    /// killed tidy routing); wires route through them freely.
    channels: Vec<Rect>,
}

impl Svg {
    fn new() -> Self {
        Svg { body: String::new(), w: 0.0, h: 0.0, solids: Vec::new(), wire_segs: Vec::new(), channels: Vec::new(), pending_sims: Vec::new(), collisions: 0 }
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
                && !self.wire_segs.iter().any(|w| w.overlaps(&r))
                && !self.channels.iter().any(|c| c.overlaps(&r));
            if clear {
                self.text(x, y, t, cls);
                return;
            }
        }
        self.text(ax + 8.0, ay + 4.0, t, cls);
    }
    /// Place a refdes+value PAIR as one block: the two lines search for a
    /// combined slot beside the symbol and move TOGETHER — independent
    /// placement let the sim annotation steal the prime slot and scatter a
    /// resistor's value away from its body.
    fn place_label_pair(&mut self, ax: f64, ay: f64, l1: &str, c1: &str, l2: &str, c2: &str) {
        const CAND: [(f64, f64); 9] = [
            (10.0, 0.0),
            (10.0, -14.0),
            (-10.0, 0.0),
            (10.0, 14.0),
            (-10.0, -14.0),
            (10.0, 28.0),
            // Escape slots beside the ground symbol (whose solid spans
            // ±13px) — a shunt column crossed by a reserved wiring lane
            // has no clear side slot at symbol height.
            (18.0, 44.0),
            (-18.0, 44.0),
            (18.0, 58.0),
        ];
        let w = 6.8 * l1.len().max(l2.len()) as f64;
        for (dx, dy) in CAND {
            let x = if dx < 0.0 { ax + dx - w } else { ax + dx };
            let y = ay + dy;
            let r = Rect { x0: x - 1.0, y0: y - 10.0, x1: x + w, y1: y + 16.0 };
            let clear = !self.solids.iter().any(|s| s.overlaps(&r.pad(1.0)))
                && !self.wire_segs.iter().any(|wr| wr.overlaps(&r))
                && !self.channels.iter().any(|c| c.overlaps(&r));
            if clear {
                self.text(x, y, l1, c1);
                if !l2.is_empty() {
                    self.text(x, y + 13.0, l2, c2);
                }
                return;
            }
        }
        self.collisions += 1;
        if std::env::var("BHDL_V4_DEBUG").is_ok() {
            eprintln!("[v4] label pair fallback (collision): '{l1}' / '{l2}' at ({ax:.0},{ay:.0})");
        }
        self.text(ax + 10.0, ay, l1, c1);
        if !l2.is_empty() {
            self.text(ax + 10.0, ay + 13.0, l2, c2);
        }
    }
    /// QUEUE a SIM decoration for placement AFTER all routes are drawn
    /// (the ordering refinement): during drawing, channels are reserved
    /// and wires don't exist yet — placing then either blocks lanes or
    /// judges against reservations instead of real geometry. Queued
    /// decorations place last, against the FINISHED sheet, channels
    /// released.
    fn queue_sim(&mut self, ax: f64, ay: f64, t: &str, cls: &str) {
        self.pending_sims.push((ax, ay, t.to_string(), cls.to_string()));
    }
    /// Place all queued SIM decorations near their subjects or not at all.
    /// Candidates never slide LEFT of the anchor — a current label that
    /// travels left of its element lands beside the PREVIOUS part and
    /// reads as annotating it (the 79µA-next-to-D1 review finding).
    fn flush_sims(&mut self) {
        self.channels.clear();
        let pending = std::mem::take(&mut self.pending_sims);
        for (ax, ay, t, cls) in pending {
            // A wider ring than the label placer: decorations must survive
            // dense neighbourhoods (an amp output crowds a feedback riser,
            // the next part's label and the spine wire) — but still never
            // LEFT of the anchor.
            const NEAR: [(f64, f64); 10] = [
                (8.0, 4.0),
                (8.0, -8.0),
                (8.0, 18.0),
                (14.0, 4.0),
                (8.0, -22.0),
                (22.0, -8.0),
                (22.0, 4.0),
                (8.0, 32.0),
                (30.0, -22.0),
                (30.0, 18.0),
            ];
            for (dx, dy) in NEAR {
                let (x, y) = (ax + dx, ay + dy);
                let r = Self::text_rect(&t, x, y);
                let clear = !self.solids.iter().any(|s| s.overlaps(&r.pad(1.0)))
                    && !self.wire_segs.iter().any(|w| w.overlaps(&r));
                if clear {
                    self.text(x, y, &t, &cls);
                    break;
                }
            }
            // No clear near slot — decoration dropped.
        }
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
                let bend = if nd as u8 != d { 12 } else { 0 };
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
        // ── POST-ROUTE STRAIGHTENING — the part local search can't do. ──
        // A shortest path is happy with stair-steps; a designer slides the
        // whole segment. Repeatedly collapse Z-jogs (H-V-H / V-H-V with
        // parallel outer segments) by moving the middle onto the far
        // segment's axis when the swept corridor is clear of SOLIDS
        // (crossing wires stays legal). Also drop zero-length and merge
        // collinear runs. Runs to fixpoint.
        let seg_clear = |a: (f64, f64), b: (f64, f64), solids: &[Rect]| -> bool {
            let r = Rect {
                x0: a.0.min(b.0) - 2.0,
                y0: a.1.min(b.1) - 2.0,
                x1: a.0.max(b.0) + 2.0,
                y1: a.1.max(b.1) + 2.0,
            };
            if let Some(hit) = solids.iter().find(|s2| s2.overlaps(&r)) {
                if std::env::var("BHDL_V4_DEBUG").is_ok() {
                    eprintln!("[v4-route]   corridor {a:?}->{b:?} blocked by {hit:?}");
                }
                return false;
            }
            true
        };
        let solids_for_route: Vec<Rect> = self
            .solids
            .iter()
            .filter(|r| !r.pad(2.0).hit(from.0, from.1) && !r.pad(2.0).hit(to.0, to.1))
            .cloned()
            .collect();
        for _pass in 0..8 {
            let mut changed = false;
            // Merge collinear + drop duplicates first.
            let mut merged: Vec<(f64, f64)> = vec![ortho[0]];
            for &p in &ortho[1..] {
                let l = *merged.last().unwrap();
                if (l.0 - p.0).abs() < 0.01 && (l.1 - p.1).abs() < 0.01 {
                    continue;
                }
                if merged.len() >= 2 {
                    let ll = merged[merged.len() - 2];
                    let col = (ll.0 - l.0).abs() < 0.01 && (l.0 - p.0).abs() < 0.01
                        || (ll.1 - l.1).abs() < 0.01 && (l.1 - p.1).abs() < 0.01;
                    if col {
                        *merged.last_mut().unwrap() = p;
                        continue;
                    }
                }
                merged.push(p);
            }
            ortho = merged;
            // Z-collapse: for interior corner pairs (i, i+1), if the
            // segments before and after are PARALLEL, slide the middle
            // segment onto the LATER one's axis (biasing jogs toward the
            // destination) when both replacement corridors are clear.
            let n = ortho.len();
            if n >= 4 {
                'scan: for i in 1..n - 2 {
                    let (a, b, c, d) = (ortho[i - 1], ortho[i], ortho[i + 1], ortho[i + 2]);
                    let ab_h = (a.1 - b.1).abs() < 0.01;
                    let cd_h = (c.1 - d.1).abs() < 0.01;
                    if ab_h != cd_h {
                        continue;
                    }
                    // Dogleg removal: slide the MIDDLE segment onto an END.
                    // (The first cut of this pass computed (b.x, d.y) —
                    // which keeps the middle where it was: a no-op. The
                    // correct corners are the two L-paths between a and d.)
                    for nb in [(d.0, a.1), (a.0, d.1)] {
                        if std::env::var("BHDL_V4_DEBUG").is_ok() {
                            eprintln!(
                                "[v4-route] try collapse i={i} a={a:?} d={d:?} nb={nb:?}: leg1={} leg2={}",
                                seg_clear(a, nb, &solids_for_route),
                                seg_clear(nb, d, &solids_for_route)
                            );
                        }
                        if seg_clear(a, nb, &solids_for_route)
                            && seg_clear(nb, d, &solids_for_route)
                        {
                            ortho.splice(i..i + 2, [nb]);
                            changed = true;
                            break 'scan;
                        }
                    }
                }
            }
            if !changed {
                break;
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
    /// Resistor drawn horizontally (IEC box): in left (x, y), out (x+40, y).
    fn res_h(&mut self, x: f64, y: f64) {
        self.solid(Rect { x0: x + 7.0, y0: y - 9.0, x1: x + 33.0, y1: y + 9.0 });
        self.wire(&[(x, y), (x + 8.0, y)]);
        let _ = writeln!(
            self.body,
            r##"<rect x="{:.1}" y="{:.1}" width="24" height="16" fill="none" stroke="#222" stroke-width="1.6"/>"##,
            x + 8.0,
            y - 8.0
        );
        self.wire(&[(x + 32.0, y), (x + 40.0, y)]);
    }
    /// Signal-net flag: the rail-flag arrow shape with signal styling.
    fn sig_flag(&mut self, x: f64, y: f64, label: &str, leftward: bool) {
        let d = if leftward { -1.0 } else { 1.0 };
        self.wire(&[(x, y), (x + d * 14.0, y - 10.0)]);
        self.wire(&[(x, y), (x + d * 14.0, y + 10.0)]);
        self.text(x + 4.0, y - 14.0, label, "part");
    }
    /// Test point: a small open circle riding a stub above the net.
    fn testpoint(&mut self, x: f64, y_net: f64, label: &str) {
        self.wire(&[(x, y_net), (x, y_net - 16.0)]);
        let _ = writeln!(
            self.body,
            r##"<circle cx="{x:.1}" cy="{:.1}" r="4.5" fill="none" stroke="#222" stroke-width="1.6"/>"##,
            y_net - 21.0
        );
        self.solid(Rect { x0: x - 5.0, y0: y_net - 26.0, x1: x + 5.0, y1: y_net - 16.0 });
        self.place_label(x, y_net - 34.0, label, "ref");
    }
    /// Waveform glyph: two cycles of a sine, 36×16, drawn in sim blue.
    /// Pure decoration — registers as a solid so labels avoid it.
    fn sine_glyph(&mut self, x: f64, y: f64) {
        let mut pts = String::new();
        for i in 0..=24 {
            let t = i as f64 / 24.0;
            let px = x + t * 36.0;
            let py = y - (t * 2.0 * std::f64::consts::PI * 2.0).sin() * 8.0;
            let _ = write!(pts, "{px:.1},{py:.1} ");
        }
        let _ = writeln!(
            self.body,
            r##"<polyline points="{}" fill="none" stroke="#06c" stroke-width="1.4"/>"##,
            pts.trim_end()
        );
        self.grow(x + 40.0, y + 12.0);
        self.solid(Rect { x0: x - 2.0, y0: y - 10.0, x1: x + 38.0, y1: y + 10.0 });
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
             .absent{{fill:#a60;font-style:italic}}.sim{{fill:#06c;font-style:italic}}\
             .intent{{fill:#862d86;font-style:italic}}</style>\n\
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

fn fmt_freq(f: f64) -> String {
    if f >= 1e6 {
        format!("{:.0}MHz", f / 1e6)
    } else if f >= 1e3 {
        format!("{:.0}kHz", f / 1e3)
    } else {
        format!("{f:.0}Hz")
    }
}

fn fmt_sim_v(v: f64) -> String {
    // Clamp numeric dust to a true zero — "-0mV" is a formatting artifact,
    // not a measurement.
    let v = if v.abs() < 5e-4 { 0.0 } else { v };
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

/// Parse "31.6k" / "10kΩ" / "0.8" → f64 for the divider equation.
fn parse_val(txt: &str) -> Option<f64> {
    let t = txt.trim().trim_end_matches('Ω').trim_end_matches("ohm").trim();
    let (num, mult) = match t.chars().last()? {
        'k' | 'K' => (&t[..t.len() - 1], 1e3),
        'M' => (&t[..t.len() - 1], 1e6),
        'm' => (&t[..t.len() - 1], 1e-3),
        _ => (t, 1.0),
    };
    num.trim().parse::<f64>().ok().map(|v| v * mult)
}

fn value_of(netlist: &Netlist, inst: &str) -> String {
    let raw = netlist
        .instances
        .values()
        .find(|i| i.name == inst)
        .and_then(|i| i.attributes.get("value").cloned())
        .unwrap_or_default();
    // Synthesized parts carry RAW f64 strings ("0.0000149999…") — print
    // them in engineering notation with the class's unit (15µF): the same
    // number, readable ink. Values that already carry a unit ("100nF",
    // "10kΩ") don't parse as bare floats and pass through untouched.
    if let Ok(v) = raw.trim().parse::<f64>() {
        let unit = match class_of_name(netlist, inst).as_str() {
            "capacitor" => "F",
            "resistor" => "Ω",
            "inductor" => "H",
            _ => "",
        };
        if !unit.is_empty() && v != 0.0 {
            return fmt_eng_value(v, unit);
        }
    }
    raw
}

/// Engineering-notation formatter: 1.4999999e-5, "F" → "15µF".
fn fmt_eng_value(v: f64, unit: &str) -> String {
    const PREFIXES: [(f64, &str); 8] = [
        (1e9, "G"),
        (1e6, "M"),
        (1e3, "k"),
        (1.0, ""),
        (1e-3, "m"),
        (1e-6, "µ"),
        (1e-9, "n"),
        (1e-12, "p"),
    ];
    let a = v.abs();
    let (scale, prefix) = PREFIXES
        .iter()
        .find(|(s, _)| a >= *s * 0.9995)
        .copied()
        .unwrap_or((1e-12, "p"));
    let scaled = v / scale;
    // Three significant-ish digits, trailing zeros trimmed.
    let s = format!("{scaled:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}{prefix}{unit}")
}

/// The designer's DECLARED intent on an instance (`for intent(...)` in
/// bhdl), formatted "name (k: v, …)". None when no intent was declared.
fn intent_of(netlist: &Netlist, inst: &str) -> Option<String> {
    let attrs = &netlist.instances.values().find(|i| i.name == inst)?.attributes;
    let name = attrs.get("intent_name")?;
    let mut params: Vec<String> = attrs
        .iter()
        .filter(|(k, _)| k.starts_with("intent_") && *k != "intent_name")
        .map(|(k, v)| format!("{}: {}", k.trim_start_matches("intent_"), v))
        .collect();
    params.sort();
    Some(if params.is_empty() {
        name.clone()
    } else {
        format!("{} ({})", name, params.join(", "))
    })
}

/// The stage's DERIVED role, from netlist structure and placed values —
/// never guessed: unity wire → buffer; resistive feedback with a ground
/// leg → the non-inverting gain equation evaluated from the placed R's;
/// resistive feedback with NO ground leg → still a follower (G = 1).
/// Reactive/exotic feedback → None (an equation we did not derive is an
/// equation we do not print).
fn amp_role(
    netlist: &Netlist,
    decor: &SheetDecor,
    fb_parts: &[String],
    gnd_leg: &[String],
    unity: bool,
) -> Option<String> {
    if unity {
        return Some("buffer".to_string());
    }
    let rf = fb_parts.iter().find(|p| class_of_name(netlist, p) == "resistor");
    let rg = gnd_leg.iter().find(|p| class_of_name(netlist, p) == "resistor");
    match (rf, rg) {
        (Some(rf), Some(rg)) => {
            let vf = parse_val(&value_of(netlist, rf))?;
            let vg = parse_val(&value_of(netlist, rg))?;
            if vg <= 0.0 {
                return None;
            }
            Some(format!(
                "G = 1+{}/{} = ×{:.2}",
                label_of(decor, rf),
                label_of(decor, rg),
                1.0 + vf / vg
            ))
        }
        // Feedback resistor, no divider to ground: a follower.
        (Some(_), None) if gnd_leg.is_empty() => Some("buffer".to_string()),
        _ => None,
    }
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
    svg.place_label_pair(x, sym_top + 8.0, label_of(decor, inst), "ref", &value_of(netlist, inst), "val");
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

    // Channel reservation (micro-PnR): the FB return's natural lane is
    // the MID-NODE ROW — it threads the gap between the divider's top and
    // bottom legs by construction (that is where the mid node lives), and
    // it is the straight shot back to the FB stub. Reserve it across the
    // stage BEFORE any label is placed, so no label squats in the lane and
    // forces the route into stair-steps between the shunt columns.
    if !stage.loops.is_empty() {
        let lane_y = spine + 50.0; // the divider mid row
        svg.channels.push(Rect {
            x0: 40.0,
            y0: lane_y - 6.0,
            x1: 40.0 + 1400.0,
            y1: lane_y + 6.0,
        });
    }

    // Source flag + bus (+ solved operating point when GLACIER ran).
    svg.rail_flag(x, spine, &net_label(netlist, stage.source_rail), true);
    if let Some(v) = solved_v(decor, netlist, stage.source_rail) {
        svg.queue_sim(x + 4.0, spine + 20.0, &format!("= {}", fmt_sim_v(v)), "sim");
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

    // Backbone. `wire_from` tracks how far the spine conductor is drawn —
    // leading series elements (fuse → regulator) draw BEFORE the IC, so
    // the bus can no longer be one src→IC stroke.
    let mut fb_stub: Option<(f64, f64)> = None;
    let mut ic_right = x;
    let mut wire_from = src_bus_start;
    let mut mid_shunt_zone: Vec<(String, f64)> = Vec::new();
    for elem in &stage.backbone {
        match elem {
            BackboneElem::Ic { inst, in_pin, out_pin } => {
                // Bus from wherever the spine currently ends into the IC.
                svg.wire(&[(wire_from, spine), (x, spine)]);
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
                        svg.queue_sim(bx + 8.0, by + 30.0, &format!("{:.2}W", p), "sim");
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
                wire_from = x;

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
                if x > wire_from {
                    svg.wire(&[(wire_from, spine), (x, spine)]);
                }
                svg.wire(&[(x, spine), (x + 7.0, spine)]);
                match class.as_str() {
                    "inductor" => svg.ind_h(x + 7.0, spine),
                    _ => svg.box_h(x + 7.0, spine, 56.0),
                }
                svg.place_label_pair(
                    x + 28.0,
                    spine - 20.0,
                    label_of(decor, inst),
                    "ref",
                    &value_of(netlist, inst),
                    "val",
                );
                if let Some(txt) = decor
                    .sim
                    .and_then(|s| s.instance_currents.get(inst))
                    .and_then(|i| fmt_sim_i(*i))
                {
                    // Currents ride ON the net: anchored just ABOVE the
                    // outgoing wire segment, immediately after the element
                    // — touching the conductor it measures, unambiguous.
                    svg.queue_sim(x + 72.0, spine - 16.0, &txt, "sim");
                }
                let _ = &mid_shunt_zone;
                x += 7.0 + 56.0;
                svg.wire(&[(x, spine), (x + 7.0, spine)]);
                x += 7.0;
                wire_from = x;
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
        svg.place_label_pair(
            dx + 9.0,
            spine + 24.0,
            label_of(decor, l.insts.first().map(String::as_str).unwrap_or("")),
            "ref",
            &value_of(netlist, l.insts.first().map(String::as_str).unwrap_or("")),
            "val",
        );
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
                svg.queue_sim(dx + 12.0, mid, &format!("= {}", fmt_sim_v(v)), "sim");
            }
        }
        let top_val = value_of(netlist, l.insts.first().map(String::as_str).unwrap_or(""));
        // bottom leg
        if l.insts.len() > 1 {
            svg.res_v(dx, mid);
            svg.place_label_pair(
                dx + 9.0,
                mid + 14.0,
                label_of(decor, &l.insts[1]),
                "ref",
                &value_of(netlist, &l.insts[1]),
                "val",
            );
            svg.wire(&[(dx, mid + 40.0), (dx, mid + 48.0)]);
            svg.ground(dx, mid + 48.0);

            // The designer's equation — WHY these values (review ask): the
            // divider sets VOUT = VREF·(1 + Rtop/Rbot). VREF from the IC's
            // declared feedback_voltage, else the SOLVED FB voltage (in
            // regulation they are the same node). Rendered only when every
            // input is real (Real-Data).
            let vref = netlist
                .instances
                .values()
                .find(|i| i.name == l.into_inst)
                .and_then(|i| i.attributes.get("feedback_voltage").cloned())
                .and_then(|v| {
                    let t = v.trim_end_matches('V');
                    t.parse::<f64>().ok()
                })
                .or_else(|| {
                    decor.sim.and_then(|sim| {
                        netlist
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
                            .and_then(|nid| {
                                netlist.nets.get(nid).and_then(|n| n.name.clone())
                            })
                            .and_then(|name| sim.net_voltages.get(&name).copied())
                    })
                });
            // The equation comes from the PART (stdlib-declared
            // `fb_equation` template) — divider relations differ across
            // parts (an LM317 is 1.25·(1+R2/R1)+IADJ·R2), so a hardcoded
            // form would be wrong ink under the wrong IC. No declaration →
            // no equation (Real-Data). Line 1 substitutes refdes, line 2
            // substitutes values + the reference, and cites the RAIL's own
            // voltage as the result rather than evaluating anything.
            // CONSISTENCY RULE (review finding): the numeric line must be
            // one solution set, never a mix. The declared 800mV reference
            // times the snapped ratio gives 3.33V — pasting the declared
            // 3.30V rail after an "=" was false arithmetic. With a solve,
            // use the SOLVED pair (FB voltage → rail voltage: GLACIER
            // holds the rail and derives FB, so they satisfy the equation
            // by construction). Without one, show substituted declared
            // values but NO "= result" tail — an equality we did not
            // compute is an equality we do not print.
            let solved_fb = decor.sim.and_then(|sim| {
                netlist
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
                    .and_then(|name| sim.net_voltages.get(&name).copied())
            });
            let solved_rail = decor.sim.and_then(|sim| {
                netlist
                    .nets
                    .get(stage.target_rail)
                    .and_then(|n| n.name.clone())
                    .and_then(|name| sim.net_voltages.get(&name).copied())
            });
            let template = netlist
                .instances
                .values()
                .find(|i| i.name == l.into_inst)
                .and_then(|i| i.attributes.get("fb_equation").cloned())
                .map(|t| t.trim_matches('"').to_string());
            if let (Some(tpl), Some(vref)) = (template, vref) {
                let bot_val = value_of(netlist, &l.insts[1]);
                let r1 = label_of(decor, l.insts.first().map(String::as_str).unwrap_or(""));
                let r2 = label_of(decor, &l.insts[1]);
                let line1 = tpl
                    .replace("{R1}", r1)
                    .replace("{R2}", r2)
                    .replace("{VREF}", "VREF")
                    .replace('*', "·");
                // One solution set per line: solved(FB, rail) together,
                // or declared VREF with no claimed result.
                let (vref_shown, result_tail) = match (solved_fb, solved_rail) {
                    (Some(fb), Some(rail)) => (fb, format!(" = {:.2}V", rail)),
                    _ => (vref, String::new()),
                };
                let line2 = format!(
                    "{}{}",
                    tpl.split('=')
                        .nth(1)
                        .unwrap_or("")
                        .trim()
                        .replace("{R1}", &top_val)
                        .replace("{R2}", &bot_val)
                        .replace("{VREF}", &fmt_sim_v(vref_shown))
                        .replace('*', "·"),
                    result_tail
                );
                let eq_y = mid + 48.0 + 40.0;
                svg.text(dx - 60.0, eq_y, &line1, "val");
                svg.text(dx - 60.0, eq_y + 14.0, &format!("= {line2}"), "val");
            }
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
        svg.queue_sim(x + 4.0, spine + 20.0, &format!("= {}", fmt_sim_v(v)), "sim");
    }
    // Also draw the source bus under its shunts.
    svg.wire(&[(src_bus_start, spine), (tgt_bus_start.min(src_bus_start + 1.0).max(src_bus_start), spine)]);

    spine + 230.0 - y0
}


/// Render the whole sheet. Returns (svg, unidiomized_count).
fn pin_net_name(netlist: &Netlist, inst: &str, pin: &str) -> Option<String> {
    let (iid, _) = netlist.instances.iter().find(|(_, i)| i.name == inst)?;
    let pi = netlist.pin_instances.values().find(|p| {
        p.instance == iid
            && netlist
                .pins
                .get(p.pin_def)
                .map(|d| d.name == pin)
                .unwrap_or(false)
    })?;
    netlist.nets.get(pi.net?)?.name.clone()
}

fn pin_on_net(netlist: &Netlist, inst: &str, pin: &str, net: NetId) -> bool {
    let Some((iid, _)) = netlist.instances.iter().find(|(_, i)| i.name == inst) else {
        return false;
    };
    netlist.pin_instances.values().any(|p| {
        p.instance == iid
            && p.net == Some(net)
            && netlist
                .pins
                .get(p.pin_def)
                .map(|d| d.name == pin)
                .unwrap_or(false)
    })
}

/// Node decorations for one chain net at the running x cursor: test-point
/// stubs, ground-shunt columns, rail-clamp columns. Returns the advanced x.
fn chain_node(
    svg: &mut Svg,
    netlist: &Netlist,
    decor: &SheetDecor,
    chain: &super::classify::ChainPlan,
    net: NetId,
    mut x: f64,
    spine: f64,
    depth: &mut f64,
) -> f64 {
    for s in chain.shunts.iter().filter(|s| s.tap == net) {
        draw_shunt(svg, netlist, decor, &s.inst, x, spine);
        *depth = depth.max(spine + 130.0);
        svg.wire(&[(x, spine), (x + 44.0, spine)]);
        x += 44.0;
    }
    for c in chain.clamps.iter().filter(|c| c.tap == net) {
        let v = match netlist.nets.get(c.rail).map(|n| &n.net_class) {
            Some(NetClass::Power { voltage, .. }) => *voltage,
            _ => 1.0,
        };
        let up = v >= 0.0; // positive rail clamps point up, negative down
        let k_on_tap = pin_on_net(netlist, &c.inst, "K", c.tap);
        svg.dot(x, spine);
        let rail_name = netlist
            .nets
            .get(c.rail)
            .and_then(|n| n.name.clone())
            .unwrap_or_default();
        if up {
            // Column bottom = net, top = rail. Cathode at rail unless the
            // netlist says otherwise (Real-Data: draw what is wired).
            svg.wire(&[(x, spine), (x, spine - 16.0)]);
            if k_on_tap {
                svg.diode_v(x, spine - 50.0);
            } else {
                svg.diode_v_up(x, spine - 50.0);
            }
            svg.wire(&[(x, spine - 50.0), (x, spine - 60.0)]);
            svg.text(x - 12.0, spine - 66.0, &rail_name, "rail");
        } else {
            // Column top = net, bottom = rail.
            svg.wire(&[(x, spine), (x, spine + 16.0)]);
            if k_on_tap {
                svg.diode_v_up(x, spine + 16.0);
            } else {
                svg.diode_v(x, spine + 16.0);
            }
            svg.wire(&[(x, spine + 50.0), (x, spine + 60.0)]);
            svg.text(x - 12.0, spine + 74.0, &rail_name, "rail");
            *depth = depth.max(spine + 90.0);
        }
        svg.place_label_pair(x + 4.0, if up { spine - 40.0 } else { spine + 30.0 },
            label_of(decor, &c.inst), "ref", &value_of(netlist, &c.inst), "val");
        svg.wire(&[(x, spine), (x + 40.0, spine)]);
        x += 40.0;
    }
    // Taps LAST — their label search must see the clamp/shunt glyphs
    // already drawn, or it parks the label under a diode (placement-order
    // doctrine: judge against final geometry).
    for (tnet, tname) in &chain.taps {
        if *tnet != net {
            continue;
        }
        // Lead in first — a tap right at the node plants its stub through
        // the net flag's label.
        svg.wire(&[(x, spine), (x + 28.0, spine)]);
        x += 28.0;
        svg.dot(x, spine);
        svg.testpoint(x, spine, label_of(decor, tname));
        svg.wire(&[(x, spine), (x + 18.0, spine)]);
        x += 18.0;
    }
    x
}

/// The op-amp glyph with its feedback: triangle entered at INP (upper-left,
/// via a small jog), INN on the lower-left, OUT at the apex. Feedback draws
/// BELOW the amp — the unity wire as a single loop, a network as horizontal
/// lanes between the OUT tap and the INN node column, with the ground leg
/// (r_g) continuing down from the node to ground. Supply pins render as
/// short labeled stubs. Returns the new x cursor (end of the OUT lead).
#[allow(clippy::too_many_arguments)]
fn draw_amp(
    svg: &mut Svg,
    netlist: &Netlist,
    decor: &SheetDecor,
    inst: &str,
    fb_parts: &[String],
    gnd_leg: &[String],
    unity: bool,
    x: f64,
    spine: f64,
    depth: &mut f64,
) -> f64 {
    // Jog up into INP.
    svg.wire(&[(x, spine), (x + 12.0, spine), (x + 12.0, spine - 12.0), (x + 28.0, spine - 12.0)]);
    let tx = x + 28.0;
    let _ = writeln!(
        svg.body,
        r##"<polygon points="{tx:.1},{:.1} {tx:.1},{:.1} {:.1},{spine:.1}" fill="#f7f7f2" stroke="#222" stroke-width="1.8"/>"##,
        spine - 28.0,
        spine + 28.0,
        tx + 60.0
    );
    svg.grow(tx + 60.0, spine + 28.0);
    svg.solid(Rect { x0: tx, y0: spine - 28.0, x1: tx + 60.0, y1: spine + 28.0 });
    svg.text(tx + 5.0, spine - 8.0, "+", "part");
    svg.text(tx + 5.0, spine + 17.0, "−", "part");
    svg.place_label(tx + 18.0, spine - 40.0, label_of(decor, inst), "ref");

    // Supply stubs off the triangle's sloped edges.
    for (pin, dy) in [("VCC", -1.0), ("VEE", 1.0)] {
        if let Some(nname) = pin_net_name(netlist, inst, pin) {
            let ex = tx + 18.0;
            svg.wire(&[(ex, spine + dy * 19.0), (ex, spine + dy * 44.0)]);
            svg.place_label(ex - 4.0, spine + dy * 52.0, &nname, "rail");
        }
    }

    let ox = tx + 60.0;
    svg.wire(&[(ox, spine), (ox + 24.0, spine)]);
    let fb_x = tx - 16.0; // INN / feedback node column
    let tap_x = ox + 12.0; // feedback tap on the OUT lead

    // Stage caption: the DERIVED role (structure + placed values), and the
    // designer's DECLARED intent when a stage part carries one — two
    // different truths, two different inks. Drawn under the stage's own
    // feedback zone, where the geometry is this amp's to spend.
    let role = amp_role(netlist, decor, fb_parts, gnd_leg, unity);
    let intent = std::iter::once(inst)
        .chain(fb_parts.iter().map(String::as_str))
        .chain(gnd_leg.iter().map(String::as_str))
        .find_map(|p| intent_of(netlist, p));
    let mut caption_at = |svg: &mut Svg, y: f64| {
        let mut cy = y;
        if let Some(r) = &role {
            svg.place_label(fb_x + 26.0, cy, r, "val");
            cy += 15.0;
        }
        if let Some(i) = &intent {
            svg.place_label(fb_x + 26.0, cy, i, "intent");
            cy += 15.0;
        }
        cy
    };

    if unity {
        svg.dot(tap_x, spine);
        svg.wire(&[
            (tap_x, spine),
            (tap_x, spine + 58.0),
            (fb_x, spine + 58.0),
            (fb_x, spine + 12.0),
            (tx, spine + 12.0),
        ]);
        let end = caption_at(svg, spine + 76.0);
        *depth = depth.max(end.max(spine + 74.0));
    } else {
        svg.wire(&[(tx, spine + 12.0), (fb_x, spine + 12.0)]);
        let n = fb_parts.len().max(1);
        let lane0 = spine + 66.0;
        let lane_h = 40.0;
        let last_lane = lane0 + (n as f64 - 1.0) * lane_h;
        svg.wire(&[(fb_x, spine + 12.0), (fb_x, last_lane)]);
        svg.dot(tap_x, spine);
        svg.wire(&[(tap_x, spine), (tap_x, last_lane)]);
        for (k, p) in fb_parts.iter().enumerate() {
            let lane = lane0 + k as f64 * lane_h;
            if k + 1 < n {
                svg.dot(fb_x, lane);
                svg.dot(tap_x, lane);
            }
            let class = class_of_name(netlist, p);
            let w = if class == "capacitor" { 34.0 } else { 40.0 };
            let sx = fb_x + ((tap_x - fb_x) - w) / 2.0;
            svg.wire(&[(fb_x, lane), (sx, lane)]);
            match class.as_str() {
                "capacitor" => svg.cap_h(sx, lane),
                _ => svg.res_h(sx, lane),
            }
            svg.wire(&[(sx + w, lane), (tap_x, lane)]);
            // Label OUTSIDE the right riser — between the risers every
            // candidate slot crosses a wire and the pair falls back.
            svg.place_label_pair(tap_x + 2.0, lane - 2.0,
                label_of(decor, p), "ref", &value_of(netlist, p), "val");
        }
        *depth = depth.max(last_lane + 30.0);
        for (k, g) in gnd_leg.iter().enumerate() {
            let gx = fb_x - k as f64 * 46.0;
            if k > 0 {
                svg.wire(&[(fb_x, last_lane), (gx, last_lane)]);
            }
            svg.dot(gx, last_lane);
            svg.wire(&[(gx, last_lane), (gx, last_lane + 8.0)]);
            let class = class_of_name(netlist, g);
            let sym_h = match class.as_str() {
                "capacitor" => {
                    svg.cap_v(gx, last_lane + 8.0);
                    34.0
                }
                _ => {
                    svg.res_v(gx, last_lane + 8.0);
                    40.0
                }
            };
            svg.wire(&[(gx, last_lane + 8.0 + sym_h), (gx, last_lane + 16.0 + sym_h)]);
            svg.ground(gx, last_lane + 16.0 + sym_h);
            svg.place_label_pair(gx + 8.0, last_lane + 20.0,
                label_of(decor, g), "ref", &value_of(netlist, g), "val");
            *depth = depth.max(last_lane + sym_h + 60.0);
        }
        let caption_y = if gnd_leg.is_empty() {
            last_lane + 28.0
        } else {
            last_lane + 96.0 // below the ground-leg column
        };
        let end = caption_at(svg, caption_y);
        *depth = depth.max(end);
    }

    ox + 24.0
}

fn net_kind(netlist: &Netlist, id: NetId) -> (String, bool, bool) {
    // (name, is_power, is_ground)
    match netlist.nets.get(id) {
        Some(n) => (
            n.name.clone().unwrap_or_default(),
            matches!(n.net_class, NetClass::Power { .. }),
            matches!(n.net_class, NetClass::Ground),
        ),
        None => (String::new(), false, false),
    }
}

/// Signal-interconnect boxes: multi-pin actives as IC boxes with per-pin
/// net flags (long-range notation — how every digital section is drawn).
/// Returns the height consumed.
fn draw_signal_row(
    svg: &mut Svg,
    netlist: &Netlist,
    plan: &SheetPlan,
    y0: f64,
    decor: &SheetDecor,
) -> f64 {
    if plan.signal_row.is_empty() {
        return 0.0;
    }
    let per_row = 5usize;
    let x0 = 60.0;
    let mut y = y0 + 30.0;
    let mut row_h: f64 = 0.0;
    let mut bx = x0;
    for (k, inst) in plan.signal_row.iter().enumerate() {
        if k > 0 && k % per_row == 0 {
            y += row_h + 60.0;
            row_h = 0.0;
            bx = x0;
        }
        let Some((iid, _)) = netlist.instances.iter().find(|(_, i)| i.name == *inst) else {
            continue;
        };
        // Connected pins: ground pins draw as a bottom ground stub; the
        // rest get right-side stubs with pin name + net flag.
        let mut flagged: Vec<(String, NetId)> = Vec::new();
        let mut has_gnd = false;
        for pi in netlist.pin_instances.values().filter(|p| p.instance == iid) {
            let (Some(pin), Some(nid)) = (netlist.pins.get(pi.pin_def), pi.net) else {
                continue;
            };
            if pin.is_virtual {
                continue;
            }
            let (_, _, is_g) = net_kind(netlist, nid);
            if is_g || matches!(pin.direction, bhdl_netlist::types::PinDirection::Ground) {
                has_gnd = true;
            } else {
                flagged.push((pin.name.clone(), nid));
            }
        }
        flagged.sort();
        // Adaptive geometry: the box holds its longest pin name; the cell
        // advances past its longest net flag — long names never overprint
        // the neighbour.
        let max_pin = flagged.iter().map(|(p, _)| p.len()).max().unwrap_or(0);
        let max_net = flagged
            .iter()
            .map(|(_, n)| net_kind(netlist, *n).0.len())
            .max()
            .unwrap_or(0);
        let box_w = (30.0 + 6.8 * max_pin as f64).max(104.0);
        let box_h = (26.0 + flagged.len() as f64 * 15.0).max(48.0);
        row_h = row_h.max(box_h + if has_gnd { 40.0 } else { 0.0 });
        let _ = writeln!(
            svg.body,
            r##"<rect x="{bx:.1}" y="{y:.1}" width="{box_w:.1}" height="{box_h:.1}" fill="#f7f7f2" stroke="#222" stroke-width="1.8"/>"##
        );
        svg.grow(bx + box_w + 90.0, y + box_h);
        svg.solid(Rect { x0: bx, y0: y, x1: bx + box_w, y1: y + box_h });
        svg.text(bx + 2.0, y - 6.0, label_of(decor, inst), "ref");
        let part = netlist
            .instances
            .get(iid)
            .and_then(|i| netlist.modules.get(i.definition).map(|m| m.name.clone()))
            .unwrap_or_default();
        svg.text(bx + 4.0, y + 14.0, &part, "part");
        for (slot, (pin, nid)) in flagged.iter().enumerate() {
            let sy = y + 26.0 + slot as f64 * 15.0;
            svg.wire(&[(bx + box_w, sy), (bx + box_w + 10.0, sy)]);
            svg.text(bx + box_w - 6.8 * pin.len() as f64 - 4.0, sy + 3.0, pin, "val");
            let (nname, is_p, _) = net_kind(netlist, *nid);
            if !nname.is_empty() {
                svg.text(bx + box_w + 13.0, sy + 3.0, &nname, if is_p { "rail" } else { "part" });
            }
        }
        if has_gnd {
            let gx = bx + box_w / 2.0;
            svg.wire(&[(gx, y + box_h), (gx, y + box_h + 8.0)]);
            svg.ground(gx, y + box_h + 8.0);
        }
        bx += box_w + 26.0 + 6.8 * max_net as f64 + 40.0;
    }
    y + row_h + 70.0 - y0
}

/// Flagged passive strip: support passives with no structural idiom drawn
/// as vertical columns — net flag on top, symbol, ground/second flag below.
/// Returns the height consumed.
fn draw_passive_strip(
    svg: &mut Svg,
    netlist: &Netlist,
    plan: &SheetPlan,
    y0: f64,
    decor: &SheetDecor,
) -> f64 {
    if plan.passive_strip.is_empty() {
        return 0.0;
    }
    let per_row = 9usize;
    let x0 = 70.0;
    let mut top = y0 + 44.0;
    let mut x = x0;
    for (k, inst) in plan.passive_strip.iter().enumerate() {
        if k > 0 && k % per_row == 0 {
            top += 190.0;
            x = x0;
        }
        let Some((iid, _)) = netlist.instances.iter().find(|(_, i)| i.name == *inst) else {
            continue;
        };
        let mut nets: Vec<NetId> = netlist
            .pin_instances
            .values()
            .filter(|p| p.instance == iid)
            .filter_map(|p| p.net)
            .collect();
        nets.dedup();
        // Top = rail if present else the first signal net (pull-ups read
        // rail-on-top naturally); bottom = ground if present else the
        // remaining signal net.
        let gnd_net = nets.iter().copied().find(|&n| net_kind(netlist, n).2);
        let rail_net = nets.iter().copied().find(|&n| net_kind(netlist, n).1);
        let others: Vec<NetId> = nets
            .iter()
            .copied()
            .filter(|n| Some(*n) != gnd_net && Some(*n) != rail_net)
            .collect();
        let top_net = rail_net.or_else(|| others.first().copied());
        let bot_net = gnd_net.or_else(|| {
            others
                .get(if rail_net.is_some() { 0 } else { 1 })
                .copied()
        });

        // Top flag.
        if let Some(tn) = top_net {
            let (nname, is_p, _) = net_kind(netlist, tn);
            svg.text(x - 12.0, top - 10.0, &nname, if is_p { "rail" } else { "part" });
        }
        svg.wire(&[(x, top), (x, top + 12.0)]);
        // Symbol (single-pin parts draw as a test point instead).
        let class = class_of_name(netlist, inst);
        let single_pin = netlist
            .pin_instances
            .values()
            .filter(|p| p.instance == iid)
            .count()
            == 1;
        let sym_bot = if single_pin {
            svg.dot(x, top + 12.0);
            svg.testpoint(x, top + 30.0, label_of(decor, inst));
            svg.wire(&[(x, top + 12.0), (x, top + 30.0)]);
            top + 30.0
        } else {
            let sb = match class.as_str() {
                "resistor" => {
                    svg.res_v(x, top + 12.0);
                    top + 52.0
                }
                "diode" | "led" | "protection" => {
                    svg.diode_v(x, top + 12.0);
                    top + 46.0
                }
                _ => {
                    svg.cap_v(x, top + 12.0);
                    top + 46.0
                }
            };
            svg.place_label_pair(x + 4.0, top + 24.0, label_of(decor, inst), "ref", &value_of(netlist, inst), "val");
            sb
        };
        // Bottom end.
        if !single_pin {
            if let Some(bn) = bot_net {
                let (nname, is_p, is_g) = net_kind(netlist, bn);
                if is_g {
                    svg.wire(&[(x, sym_bot), (x, sym_bot + 8.0)]);
                    svg.ground(x, sym_bot + 8.0);
                } else {
                    svg.wire(&[(x, sym_bot), (x, sym_bot + 14.0)]);
                    svg.text(x - 12.0, sym_bot + 28.0, &nname, if is_p { "rail" } else { "part" });
                }
            }
        }
        svg.grow(x + 60.0, top + 130.0);
        // Advance past this column's widest flag text.
        let widest = nets
            .iter()
            .map(|&n| net_kind(netlist, n).0.len())
            .max()
            .unwrap_or(0);
        x += (6.8 * widest as f64 + 44.0).max(120.0);
    }
    let rows = plan.passive_strip.len().div_ceil(per_row);
    44.0 + rows as f64 * 190.0
}

/// Render one op-amp signal chain; returns the height consumed.
fn draw_chain(
    svg: &mut Svg,
    netlist: &Netlist,
    plan: &SheetPlan,
    ci: usize,
    y0: f64,
    decor: &SheetDecor,
) -> f64 {
    let chain = &plan.chains[ci];
    let spine = y0 + 90.0;
    let mut depth = spine + 90.0;
    let mut x = 60.0;

    // Solved-DC on a SIGNAL flag is information only when NONZERO (a real
    // bias point / offset). Zero is the solver's default for an externally
    // driven path — the 0A-on-FB rule: an annotation that conveys nothing
    // is not printed. (Rails differ: a 0V rail would be alarming, so rail
    // annotations always print.)
    let chain_dc = |v: f64| v.abs() >= 1e-3;

    let in_net = chain.spine_nets[0];
    svg.sig_flag(x, spine, &net_label(netlist, in_net), true);
    if let Some(v) = solved_v(decor, netlist, in_net) {
        if chain_dc(v) {
            svg.queue_sim(x + 4.0, spine + 8.0, &format!("= {}", fmt_sim_v(v)), "sim");
        }
    }
    // Stimulus-response decoration: sine glyph + STIMULUS label at the
    // input, drawn only when a transient actually ran on this chain.
    let stim = decor
        .sim
        .and_then(|s| s.stimulus.as_ref())
        .filter(|s| {
            Some(s.input_net.as_str())
                == netlist.nets.get(in_net).and_then(|n| n.name.as_deref())
        });
    if let Some(s) = stim {
        svg.sine_glyph(x + 6.0, spine - 44.0);
        svg.queue_sim(
            x + 44.0,
            spine - 48.0,
            &format!("{} · {}", fmt_sim_v(s.vin_amplitude), fmt_freq(s.frequency_hz)),
            "sim",
        );
    }
    svg.wire(&[(x, spine), (x + 24.0, spine)]);
    x += 24.0;

    for (i, elem) in chain.elems.iter().enumerate() {
        x = chain_node(svg, netlist, decor, chain, chain.spine_nets[i], x, spine, &mut depth);
        match elem {
            ChainElem::Series { inst } => {
                let class = class_of_name(netlist, inst);
                let w = match class.as_str() {
                    "capacitor" => {
                        svg.cap_h(x, spine);
                        34.0
                    }
                    "inductor" => {
                        svg.ind_h(x, spine);
                        56.0
                    }
                    "resistor" => {
                        svg.res_h(x, spine);
                        40.0
                    }
                    _ => {
                        svg.wire(&[(x, spine), (x + 40.0, spine)]);
                        svg.box_h(x, spine, 40.0);
                        40.0
                    }
                };
                svg.place_label_pair(x + w / 2.0 - 6.0, spine - 26.0,
                    label_of(decor, inst), "ref", &value_of(netlist, inst), "val");
                x += w;
                svg.wire(&[(x, spine), (x + 16.0, spine)]);
                x += 16.0;
            }
            ChainElem::Amp { inst, fb_parts, gnd_leg, unity } => {
                x = draw_amp(svg, netlist, decor, inst, fb_parts, gnd_leg, *unity, x, spine, &mut depth);
                // Per-stage measured amplitude at the pin this part
                // DECLARED as its probe point (stdlib sim_probe policy) —
                // the stage's transformation, visible at its output.
                // Skipped when the probe net IS the chain output: the
                // output flag already carries that measurement.
                let stage = decor.sim.and_then(|s| s.stimulus.as_ref()).and_then(|s| {
                    s.stages
                        .iter()
                        .find(|st| st.instance == *inst && st.net != s.output_net)
                });
                if let Some(st) = stage {
                    svg.queue_sim(
                        x - 20.0,
                        spine - 18.0,
                        &format!(
                            "= {}{}",
                            fmt_sim_v(st.amplitude),
                            if st.clipped { " CLIPPED" } else { "" }
                        ),
                        "sim",
                    );
                }
            }
        }
    }

    let out_net = *chain.spine_nets.last().expect("chain has nets");
    x = chain_node(svg, netlist, decor, chain, out_net, x, spine, &mut depth);
    svg.wire(&[(x, spine), (x + 18.0, spine)]);
    x += 18.0;
    svg.sig_flag(x, spine, &net_label(netlist, out_net), false);
    if let Some(v) = solved_v(decor, netlist, out_net) {
        if chain_dc(v) {
            svg.queue_sim(x + 4.0, spine + 8.0, &format!("= {}", fmt_sim_v(v)), "sim");
        }
    }
    // MEASURED response at the output — the transient's amplitude over the
    // final stimulus cycle, never nominal-gain arithmetic.
    let stim_out = decor
        .sim
        .and_then(|s| s.stimulus.as_ref())
        .filter(|s| {
            Some(s.output_net.as_str())
                == netlist.nets.get(out_net).and_then(|n| n.name.as_deref())
        });
    if let Some(s) = stim_out {
        svg.sine_glyph(x - 44.0, spine - 44.0);
        svg.queue_sim(
            x - 2.0,
            spine - 48.0,
            &format!(
                "= {}{}",
                fmt_sim_v(s.vout_amplitude),
                if s.clipped { " CLIPPED" } else { "" }
            ),
            "sim",
        );
    }

    depth - y0 + 50.0
}

pub fn render_sheet_svg(
    netlist: &Netlist,
    title: &str,
    decor: &SheetDecor,
) -> (String, usize, usize) {
    render_sheet_svg_with_blocks(netlist, title, decor, &[])
}

/// One rendered sheet of a hierarchical board.
pub struct SheetOut {
    /// File-name slug ("" = the top sheet).
    pub slug: String,
    pub title: String,
    pub svg: String,
    pub unidiomized: usize,
    pub collisions: usize,
}

/// Render a hierarchical board as a sheet tree: a top sheet with each
/// expanded entity drawn as a LINKED block (native SVG hyperlinks — the
/// interactive binding needs no scripting), plus one sheet per entity
/// holding the parent IC and its expansion children. Flat boards return
/// the single sheet unchanged. `href_for` maps a parent instance name to
/// the (relative) href/slug its block links to.
pub fn render_sheet_tree(
    netlist: &Netlist,
    title: &str,
    decor: &SheetDecor,
    href_for: &dyn Fn(&str) -> String,
) -> Vec<SheetOut> {
    let Some(groups) = super::sheets::partition_sheets(netlist) else {
        let (svg, unidiomized, collisions) = render_sheet_svg(netlist, title, decor);
        return vec![SheetOut { slug: String::new(), title: title.to_string(), svg, unidiomized, collisions }];
    };
    let blocks = super::sheets::block_specs(netlist, &groups, href_for);
    let mut out = Vec::new();
    for g in &groups {
        let sub = super::sheets::subset_netlist(netlist, &g.members);
        match &g.parent {
            None => {
                let (svg, unidiomized, collisions) =
                    render_sheet_svg_with_blocks(&sub, title, decor, &blocks);
                out.push(SheetOut { slug: String::new(), title: title.to_string(), svg, unidiomized, collisions });
            }
            Some(parent) => {
                let sheet_title = format!("{title} · {parent}");
                let (svg, unidiomized, collisions) =
                    render_sheet_svg_with_blocks(&sub, &sheet_title, decor, &[]);
                out.push(SheetOut { slug: parent.clone(), title: sheet_title, svg, unidiomized, collisions });
            }
        }
    }
    out
}

fn render_sheet_svg_with_blocks(
    netlist: &Netlist,
    title: &str,
    decor: &SheetDecor,
    blocks: &[super::sheets::BlockSpec],
) -> (String, usize, usize) {
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

    // ── Signal chains: op-amp paths on a horizontal spine ──
    for i in 0..plan.chains.len() {
        let used = draw_chain(&mut svg, netlist, &plan, i, y, decor);
        y += used.max(240.0);
    }

    // ── Entity blocks: expanded subcircuits as LINKED boxes with their
    // port pins and net flags — clicking one opens its sheet. ──
    if !blocks.is_empty() {
        let mut bx = 60.0;
        let by = y + 30.0;
        let mut row_h: f64 = 0.0;
        for b in blocks {
            let max_pin = b.ports.iter().map(|(p, ..)| p.len()).max().unwrap_or(0);
            // MEASURED port current: the GLACIER-solved current of the
            // group's DOMINANT carrier on the net (>=10x every other
            // member, or the sole significant one) — an ambiguous split is
            // not a port current and is not printed. Computed up front so
            // the cell reserves width for it: this ink sits ON its row,
            // deterministically (a slot-searched decoration that slides to
            // another row reads as the wrong port's current).
            // EXACT port current: the solved net injection of this block's
            // branches into the port net (computed in the CLI against the
            // final circuit) — physical boundary flow, no heuristics.
            let port_current = |nname: &str| -> Option<String> {
                let sim = decor.sim?;
                let i = sim.port_currents.get(&format!("{}::{}", b.inst, nname))?;
                Some(format!("= {}", fmt_sim_i(*i)?))
            };
            let max_net = b
                .ports
                .iter()
                .map(|(_, n, ..)| {
                    n.len() + port_current(n).map(|c| c.len() + 2).unwrap_or(0)
                })
                .max()
                .unwrap_or(0);
            let box_w = (34.0 + 6.8 * max_pin as f64).max(120.0);
            let visible: Vec<_> = b.ports.iter().filter(|(_, _, _, g, _)| !g).collect();
            let box_h = (30.0 + visible.len() as f64 * 15.0).max(56.0);
            row_h = row_h.max(box_h + 46.0);
            let _ = writeln!(svg.body, r##"<a href="{}">"##, b.href);
            let _ = writeln!(
                svg.body,
                r##"<rect x="{bx:.1}" y="{by:.1}" width="{box_w:.1}" height="{box_h:.1}" fill="#eef2fa" stroke="#225" stroke-width="2.2" rx="4"/>"##
            );
            svg.text(bx + 2.0, by - 6.0, label_of(decor, &b.inst), "ref");
            svg.text(bx + 6.0, by + 16.0, &b.part, "part");
            svg.text(bx + 6.0, by + 30.0, "▸ sheet", "sim");
            let _ = writeln!(svg.body, "</a>");
            svg.solid(Rect { x0: bx, y0: by, x1: bx + box_w, y1: by + box_h });
            svg.grow(bx + box_w + 90.0, by + box_h + 40.0);
            let mut slot = 0usize;
            let mut drew_gnd = false;
            for (pin, nname, is_p, is_g, _insts) in &b.ports {
                if *is_g {
                    if !drew_gnd {
                        let gx = bx + box_w / 2.0;
                        svg.wire(&[(gx, by + box_h), (gx, by + box_h + 8.0)]);
                        svg.ground(gx, by + box_h + 8.0);
                        drew_gnd = true;
                    }
                    continue;
                }
                let sy = by + 44.0 + slot as f64 * 15.0 - 14.0;
                svg.wire(&[(bx + box_w, sy), (bx + box_w + 10.0, sy)]);
                svg.text(bx + box_w - 6.8 * pin.len() as f64 - 4.0, sy + 3.0, pin, "val");
                if !nname.is_empty() {
                    svg.text(
                        bx + box_w + 13.0,
                        sy + 3.0,
                        nname,
                        if *is_p { "rail" } else { "part" },
                    );
                }
                if let Some(label) = port_current(nname) {
                    svg.text(
                        bx + box_w + 13.0 + 6.8 * nname.len() as f64 + 8.0,
                        sy + 3.0,
                        &label,
                        "sim",
                    );
                }
                slot += 1;
            }
            bx += box_w + 26.0 + 6.8 * max_net as f64 + 46.0;
        }
        y = by + row_h + 30.0;
    }

    // ── Load rows: rail consumers fanned out under a shared bus ──
    for load in &plan.loads {
        let bus_y = y + 40.0;
        let box_w = 96.0;
        let box_h = 60.0;
        let pitch = 130.0;
        let x0 = 60.0;
        svg.rail_flag(x0 - 20.0, bus_y, &net_label(netlist, load.rail), true);
        if let Some(v) = solved_v(decor, netlist, load.rail) {
            svg.queue_sim(x0 - 16.0, bus_y + 18.0, &format!("= {}", fmt_sim_v(v)), "sim");
        }
        let boxes_end = x0 + load.insts.len() as f64 * pitch;
        let bus_end = boxes_end + load.shunts.len() as f64 * SHUNT_PITCH;
        svg.wire(&[(x0 - 20.0, bus_y), (bus_end, bus_y)]);
        // Decoupling bank: shunt columns continue along the consumer bus.
        {
            let mut sx = boxes_end;
            for inst in &load.shunts {
                draw_shunt(&mut svg, netlist, decor, inst, sx, bus_y);
                sx += SHUNT_PITCH;
            }
        }
        for (k, inst) in load.insts.iter().enumerate() {
            let bx = x0 + k as f64 * pitch;
            let by = bus_y + 26.0;
            let _ = writeln!(
                svg.body,
                r##"<rect x="{bx:.1}" y="{by:.1}" width="{box_w:.1}" height="{box_h:.1}" fill="#f7f7f2" stroke="#222" stroke-width="1.8"/>"##
            );
            svg.grow(bx + box_w, by + box_h);
            svg.solid(Rect { x0: bx, y0: by, x1: bx + box_w, y1: by + box_h });
            svg.text(bx + 4.0, by - 6.0, label_of(decor, inst), "ref");
            let part = netlist
                .instances
                .values()
                .find(|i| i.name == *inst)
                .and_then(|i| netlist.modules.get(i.definition).map(|m| m.name.clone()))
                .unwrap_or_default();
            svg.text(bx + 4.0, by + 14.0, &part, "part");
            // Power stub up to the bus with a junction dot; GND stub down.
            let px = bx + box_w / 2.0;
            svg.wire(&[(px, by), (px, bus_y)]);
            svg.dot(px, bus_y);
            // Pins: gnd → ground below; other connected pins → right-side
            // stubs with net flags (long-range notation — the fan-out row
            // doesn't wire signals point-to-point).
            let mut slot = 0usize;
            if let Some((iid, _)) = netlist.instances.iter().find(|(_, i)| i.name == *inst) {
                for pin_i in netlist.pin_instances.values().filter(|p| p.instance == iid) {
                    let (Some(pin), Some(nid)) = (netlist.pins.get(pin_i.pin_def), pin_i.net)
                    else {
                        continue;
                    };
                    if pin.is_virtual {
                        continue;
                    }
                    if matches!(pin.direction, bhdl_netlist::types::PinDirection::Ground) {
                        let gx = bx + box_w / 2.0;
                        svg.wire(&[(gx, by + box_h), (gx, by + box_h + 8.0)]);
                        svg.ground(gx, by + box_h + 8.0);
                        continue;
                    }
                    if nid == load.rail {
                        continue; // the power stub above
                    }
                    let nname = netlist
                        .nets
                        .get(nid)
                        .and_then(|n| n.name.clone())
                        .unwrap_or_default();
                    let sy = by + 14.0 + slot as f64 * 14.0;
                    if sy > by + box_h - 6.0 {
                        continue; // box full — remaining pins live in the report
                    }
                    svg.wire(&[(bx + box_w, sy), (bx + box_w + 10.0, sy)]);
                    svg.text(bx + box_w - 8.0 * pin.name.len() as f64, sy - 3.0, &pin.name, "val");
                    if !nname.is_empty() {
                        svg.text(bx + box_w + 12.0, sy + 3.0, &nname, "rail");
                    }
                    slot += 1;
                }
            }
        }
        y = bus_y + 26.0 + box_h + 70.0;
    }

    // ── Signal-interconnect boxes + flagged passive strip ──
    y += draw_signal_row(&mut svg, netlist, &plan, y, decor);
    y += draw_passive_strip(&mut svg, netlist, &plan, y, decor);

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
            let ry = y + 44.0;
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
        y += 124.0;
    }

    svg.flush_sims();

    let n_res = plan.residue.len();
    let n_coll = svg.collisions;
    svg.grow(200.0, y);
    (svg.finish(title), n_res, n_coll)
}
