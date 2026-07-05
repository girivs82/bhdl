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

const SHUNT_PITCH: f64 = 70.0;
const IC_W: f64 = 130.0;
const IC_H: f64 = 100.0;
const SERIES_W: f64 = 70.0;
const STAGE_GAP: f64 = 300.0;

struct Svg {
    body: String,
    w: f64,
    h: f64,
}

impl Svg {
    fn new() -> Self {
        Svg { body: String::new(), w: 0.0, h: 0.0 }
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
    }
    fn dot(&mut self, x: f64, y: f64) {
        let _ = writeln!(self.body, r##"<circle cx="{x:.1}" cy="{y:.1}" r="3" fill="#222"/>"##);
    }
    fn text(&mut self, x: f64, y: f64, t: &str, cls: &str) {
        let esc = t.replace('&', "&amp;").replace('<', "&lt;");
        let _ = writeln!(self.body, r#"<text x="{x:.1}" y="{y:.1}" class="{cls}">{esc}</text>"#);
        self.grow(x + 8.0 * t.len() as f64, y + 14.0);
    }
    /// Ground symbol, stem entering at (x, y) from above.
    fn ground(&mut self, x: f64, y: f64) {
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
        self.wire(&[(x, y), (x, y + 13.0)]);
        self.wire(&[(x - 11.0, y + 13.0), (x + 11.0, y + 13.0)]);
        self.wire(&[(x - 11.0, y + 21.0), (x + 11.0, y + 21.0)]);
        self.wire(&[(x, y + 21.0), (x, y + 34.0)]);
    }
    /// Resistor drawn vertically (IEC box): in top (x,y), out (x, y+40).
    fn res_v(&mut self, x: f64, y: f64) {
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
    /// Generic 2-terminal fallback (horizontal box).
    fn box_h(&mut self, x: f64, y: f64, w: f64) {
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
             .absent{{fill:#a60;font-style:italic}}</style>\n\
             <rect width=\"{w:.0}\" height=\"{h:.0}\" fill=\"white\"/>\n\
             <text x=\"16\" y=\"22\" class=\"part\">{title}</text>\n{body}</svg>\n",
            w = self.w + 30.0,
            h = self.h + 30.0,
            title = title,
            body = self.body
        )
    }
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
fn draw_shunt(svg: &mut Svg, netlist: &Netlist, inst: &str, x: f64, y_bus: f64) {
    svg.dot(x, y_bus);
    let class = class_of_name(netlist, inst);
    let sym_top = y_bus + 16.0;
    svg.wire(&[(x, y_bus), (x, sym_top)]);
    let sym_bot = match class.as_str() {
        "resistor" => {
            svg.res_v(x, sym_top);
            sym_top + 40.0
        }
        _ => {
            svg.cap_v(x, sym_top);
            sym_top + 34.0
        }
    };
    svg.wire(&[(x, sym_bot), (x, sym_bot + 10.0)]);
    svg.ground(x, sym_bot + 10.0);
    svg.text(x + 8.0, sym_top + 12.0, inst, "ref");
    let v = value_of(netlist, inst);
    if !v.is_empty() {
        svg.text(x + 8.0, sym_top + 26.0, &v, "val");
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
) -> f64 {
    let stage = &plan.stages[stage_idx];
    let spine = y0 + 120.0;
    let mut x = 40.0;

    // Source flag + bus.
    svg.rail_flag(x, spine, &net_label(netlist, stage.source_rail), true);
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
        draw_shunt(svg, netlist, inst, x, spine);
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
                svg.text(bx + 8.0, by - 6.0, inst, "ref");
                // Part name inside.
                let part = netlist
                    .instances
                    .values()
                    .find(|i| i.name == *inst)
                    .and_then(|i| netlist.modules.get(i.definition).map(|m| m.name.clone()))
                    .unwrap_or_default();
                svg.text(bx + 8.0, by + 16.0, &part, "part");
                // in pin stub (left mid).
                svg.text(bx + 6.0, spine + 4.0, in_pin, "val");
                // out pin stub (right, upper-mid).
                let out_y = spine;
                svg.wire(&[(bx + IC_W, out_y), (bx + IC_W + 14.0, out_y)]);
                svg.text(bx + IC_W - 8.0 * out_pin.len() as f64, out_y - 5.0, out_pin, "val");
                // GND stub (bottom center) + ground symbol.
                let gx = bx + IC_W / 2.0;
                svg.wire(&[(gx, by + IC_H), (gx, by + IC_H + 10.0)]);
                svg.ground(gx, by + IC_H + 10.0);
                svg.text(gx + 6.0, by + IC_H + 12.0, "GND", "val");
                // FB stub (right side, lower) if a loop returns here.
                if let Some(l) = stage.loops.iter().find(|l| l.into_inst == *inst) {
                    let fy = spine + IC_H * 0.30;
                    svg.wire(&[(bx + IC_W, fy), (bx + IC_W + 14.0, fy)]);
                    svg.text(
                        bx + IC_W - 8.0 * l.into_pin.len() as f64,
                        fy - 5.0,
                        &l.into_pin,
                        "val",
                    );
                    fb_stub = Some((bx + IC_W + 14.0, fy));
                }
                x = bx + IC_W + 14.0;
                ic_right = x;
            }
            BackboneElem::Series { inst } => {
                let class = class_of_name(netlist, inst);
                svg.wire(&[(x, spine), (x + 7.0, spine)]);
                match class.as_str() {
                    "inductor" => svg.ind_h(x + 7.0, spine),
                    _ => svg.box_h(x + 7.0, spine, 56.0),
                }
                svg.text(x + 14.0, spine - 12.0, inst, "ref");
                let v = value_of(netlist, inst);
                if !v.is_empty() {
                    svg.text(x + 14.0, spine + 22.0, &v, "val");
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
        draw_shunt(svg, netlist, inst, x, spine);
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
        svg.text(dx + 10.0, spine + 28.0, l.insts.first().map(String::as_str).unwrap_or(""), "ref");
        let mid = spine + 50.0;
        svg.dot(dx, mid);
        // bottom leg
        if l.insts.len() > 1 {
            svg.res_v(dx, mid);
            svg.text(dx + 10.0, mid + 18.0, &l.insts[1], "ref");
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
            let flank_x = dx - 26.0; // left of the divider, right of the last shunt
            let clear_y = spine + 160.0; // below every ground symbol
            let up_x = ic_right + 10.0; // clear channel right of the IC
            svg.wire(&[
                (dx, mid),
                (flank_x, mid),
                (flank_x, clear_y),
                (up_x, clear_y),
                (up_x, fy),
                (fx, fy),
            ]);
        }
        x += SHUNT_PITCH;
    }

    // Close the target bus and flag it.
    svg.wire(&[(tgt_bus_start, spine), (x, spine)]);
    svg.rail_flag(x, spine, &net_label(netlist, stage.target_rail), false);
    // Also draw the source bus under its shunts.
    svg.wire(&[(src_bus_start, spine), (tgt_bus_start.min(src_bus_start + 1.0).max(src_bus_start), spine)]);

    spine + 230.0 - y0
}


/// Render the whole sheet. Returns (svg, unidiomized_count).
pub fn render_sheet_svg(netlist: &Netlist, title: &str) -> (String, usize) {
    let plan = classify_sheet(netlist);
    log::info!(
        "v4 plan: {} rails, {} grounds, {} stages, {} residue",
        plan.rails.len(), plan.grounds.len(), plan.stages.len(), plan.residue.len()
    );
    let mut svg = Svg::new();
    let mut y = 30.0;

    for i in 0..plan.stages.len() {
        let used = draw_stage(&mut svg, netlist, &plan, i, y);
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
            svg.text(x + 4.0, ry - 14.0, inst, "ref");
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
