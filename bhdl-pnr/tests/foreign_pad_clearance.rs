//! Clearance-by-construction regression: a routed track must NEVER pass
//! through a foreign pad or its clearance halo.
//!
//! Topology: source pad (net SIG) at the left, sink pad (net SIG) at the
//! right, and a fat foreign pad (net OTHER) sitting exactly on the
//! straight line between them. The correct route detours; the P0 oracle
//! caught routes going straight through (shorting_items on the LED
//! fixture board). Routing is exercised directly through
//! `pathfinder_route` on a hand-built board — no placement, fully
//! deterministic.

use bhdl_pnr::routing::grid::RoutingGrid;
use bhdl_pnr::routing::pathfinder;
use bhdl_pnr::types::*;
use slotmap::SlotMap;

fn pad(w: f64, h: f64) -> Option<PadGeom> {
    Some(PadGeom {
        width_mm: w,
        height_mm: h,
        shape: PadShapeKind::Rect,
        drill_mm: None,
    })
}

fn board_with_blocker() -> (Board, NetId, NetId) {
    let mut ck: SlotMap<ComponentId, ()> = SlotMap::with_key();
    let mut pk: SlotMap<PinId, ()> = SlotMap::with_key();
    let mut nk: SlotMap<NetId, ()> = SlotMap::with_key();

    let a = ck.insert(());
    let b = ck.insert(());
    let m = ck.insert(());
    let a1 = pk.insert(());
    let b1 = pk.insert(());
    let m1 = pk.insert(());
    let sig = nk.insert(());
    let other = nk.insert(());

    let mk = |id, name: &str, x, y, pins: Vec<PinPosition>| Component {
        id,
        name: name.into(),
        refdes: name.into(),
        width_mm: 2.0,
        height_mm: 2.0,
        pins,
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
        package: "TEST".into(),
        placement: PlacementConstraint::Fixed { x, y, theta: 0.0 },
        x,
        y,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: vec![],
            bbox_dx: 0.0,
            bbox_dy: 0.0,
        };

    let comp_a = mk(a, "A", 5.0, 10.0, vec![PinPosition {
        pin_id: a1, name: "1".into(), dx: 0.0, dy: 0.0, net: Some(sig), pad: pad(1.0, 1.0), unplaced: false }]);
    let comp_b = mk(b, "B", 25.0, 10.0, vec![PinPosition {
        pin_id: b1, name: "1".into(), dx: 0.0, dy: 0.0, net: Some(sig), pad: pad(1.0, 1.0), unplaced: false }]);
    // The blocker: a 3×3 mm foreign pad dead-center on the A→B line.
    let comp_m = mk(m, "M", 15.0, 10.0, vec![PinPosition {
        pin_id: m1, name: "1".into(), dx: 0.0, dy: 0.0, net: Some(other), pad: pad(3.0, 3.0), unplaced: false }]);

    let mk_net = |id, name: &str, pins: Vec<(ComponentId, PinId)>| PnrNet {
        allowed_layers: None,
        id,
        name: name.into(),
        pins,
        net_class: PnrNetClass::Signal,
        weight: 1.0,
        required_trace_width_mm: 0.15,
        layer_constraint: LayerConstraint::Any,
        intent: None,
        layout_intents: vec![],
            plane_layer: None,
            plane_region: None,
        };

    let board = Board {
        config: BoardConfig {
            outline: BoardOutline::Rectangle { width_mm: 30.0, height_mm: 20.0 },
            ..BoardConfig::default()
        },
        layer_stack: bhdl_pnr::stackup::stackup_preset(StackupPreset::TwoLayer),
        components: vec![comp_a, comp_b, comp_m],
        nets: vec![
            mk_net(sig, "SIG", vec![(a, a1), (b, b1)]),
            mk_net(other, "OTHER", vec![(m, m1)]),
        ],
        groups: vec![],
        placement_recipes: Default::default(),
        constraints: vec![],
    };
    (board, sig, other)
}

/// Minimum distance from segment PQ to point C.
fn seg_point_dist(p: (f64, f64), q: (f64, f64), c: (f64, f64)) -> f64 {
    let (px, py) = p;
    let (qx, qy) = q;
    let (cx, cy) = c;
    let (dx, dy) = (qx - px, qy - py);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((cx - px) * dx + (cy - py) * dy) / len2).clamp(0.0, 1.0)
    };
    let (nx, ny) = (px + t * dx, py + t * dy);
    ((cx - nx).powi(2) + (cy - ny).powi(2)).sqrt()
}

#[test]
fn route_never_enters_foreign_pad_halo() {
    let (board, sig, _other) = board_with_blocker();
    let mut grid = RoutingGrid::build(&board);
    let routes = pathfinder::pathfinder_route(&mut grid, &board.nets, &board, 100, 1.0, 1.0, false);

    let sig_route = routes.iter().find(|r| r.net_id == sig).expect("SIG route");
    assert!(!sig_route.is_empty(), "SIG must route (detour exists above/below the blocker)");

    // Foreign pad copper rect: center (15,10), 3×3 → x 13.5..16.5, y 8.5..11.5.
    // A legal track CENTER stays ≥ pad_half + spacing + trace_half away.
    let spacing = board.config.min_spacing_mm;
    let trace_half = 0.15 / 2.0;
    // Distance from the pad's rect: approximate with distance to the pad
    // center minus its half-extent along the closest axis — use the
    // conservative circumscribed check per corner sample instead: sample
    // each segment against the pad RECT expanded by (spacing + trace_half).
    let (x0, y0, x1, y1) = (
        13.5 - spacing - trace_half,
        8.5 - spacing - trace_half,
        16.5 + spacing + trace_half,
        11.5 + spacing + trace_half,
    );
    let mut worst: Option<((f64, f64), (f64, f64))> = None;
    for seg in &sig_route.segments {
        // Sample along the segment; inside-expanded-rect = violation.
        for i in 0..=20 {
            let t = i as f64 / 20.0;
            let x = seg.start.0 + t * (seg.end.0 - seg.start.0);
            let y = seg.start.1 + t * (seg.end.1 - seg.start.1);
            if x > x0 && x < x1 && y > y0 && y < y1 {
                worst = Some((seg.start, seg.end));
            }
        }
    }
    if let Some((s, e)) = worst {
        // Diagnostic: how close did the center-line get to the pad center?
        let d = seg_point_dist(s, e, (15.0, 10.0));
        panic!(
            "SIG track {s:?}→{e:?} enters the foreign pad halo \
             (center-line to pad center: {d:.3}mm; legal ≥ {:.3}mm)",
            1.5 + spacing + trace_half
        );
    }
}


/// The LED-fixture reproduction: the sink pad and a NO-NET (NC) pad sit
/// on the SAME component, close enough that their clearance halos
/// overlap (SOT-23 K vs pad 3, component rotated 90°). The route must
/// reach K without copper over the NC pad. This is the exact geometry
/// the P0 oracle flagged (shorting "net ''" against pad 3).
#[test]
fn route_avoids_nc_pad_on_own_component() {
    let mut ck: SlotMap<ComponentId, ()> = SlotMap::with_key();
    let mut pk: SlotMap<PinId, ()> = SlotMap::with_key();
    let mut nk: SlotMap<NetId, ()> = SlotMap::with_key();

    let a = ck.insert(());
    let m = ck.insert(());
    let a1 = pk.insert(());
    let mk_pin = pk.insert(());
    let m3 = pk.insert(());
    let sig = nk.insert(());

    let mk = |id, name: &str, x, y, theta: f64, pins: Vec<PinPosition>| Component {
        id,
        name: name.into(),
        refdes: name.into(),
        width_mm: 3.0,
        height_mm: 3.0,
        pins,
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
        package: "SOT-23".into(),
        placement: PlacementConstraint::Fixed { x, y, theta },
        x,
        y,
        theta,
        density_inflation: 1.0,
        layout_intents: vec![],
            bbox_dx: 0.0,
            bbox_dy: 0.0,
        };

    // Source far below; sink K and NC pad 3 mimic the SOT-23 layout:
    // K at dx −0.925, pad 3 at dx +0.925 — 1.85 mm apart with 1.35×0.5
    // pads, so their clearance halos overlap between them. Rotated 90°.
    let comp_a = mk(a, "A", 15.0, 3.0, 0.0, vec![PinPosition {
        pin_id: a1, name: "1".into(), dx: 0.0, dy: 0.0, net: Some(sig), pad: pad(1.0, 1.0), unplaced: false }]);
    let comp_m = mk(m, "M", 15.0, 12.0, std::f64::consts::FRAC_PI_2, vec![
        PinPosition {
            pin_id: mk_pin, name: "K".into(), dx: -0.925, dy: 0.0,
            net: Some(sig), pad: pad(1.35, 0.5), unplaced: false },
        PinPosition {
            pin_id: m3, name: "3".into(), dx: 0.925, dy: 0.0,
            net: None, pad: pad(1.35, 0.5), unplaced: false },
    ]);

    let board = Board {
        config: BoardConfig {
            outline: BoardOutline::Rectangle { width_mm: 30.0, height_mm: 20.0 },
            ..BoardConfig::default()
        },
        layer_stack: bhdl_pnr::stackup::stackup_preset(StackupPreset::TwoLayer),
        components: vec![comp_a, comp_m],
        nets: vec![PnrNet {
        allowed_layers: None,
            id: sig,
            name: "SIG".into(),
            pins: vec![(a, a1), (m, mk_pin)],
            net_class: PnrNetClass::Signal,
            weight: 1.0,
            required_trace_width_mm: 0.15,
            layer_constraint: LayerConstraint::Any,
            intent: None,
            layout_intents: vec![],
            plane_layer: None,
            plane_region: None,
        }],
        groups: vec![],
        placement_recipes: Default::default(),
        constraints: vec![],
    };

    let mut grid = RoutingGrid::build(&board);
    let routes = pathfinder::pathfinder_route(&mut grid, &board.nets, &board, 100, 1.0, 1.0, false);
    let r = &routes[0];
    assert!(!r.is_empty(), "SIG must route to K");

    // NC pad 3 (rotated 90°): center (15.0, 12.0 + 0.925) = (15, 12.925),
    // extents swap to 0.5×1.35 → x 14.75..15.25, y 12.25..13.6, expanded
    // by spacing + trace/2.
    let sp = board.config.min_spacing_mm + 0.15 / 2.0;
    let (x0, y0, x1, y1) = (14.75 - sp, 12.25 - sp, 15.25 + sp, 13.6 + sp);
    for seg in &r.segments {
        for i in 0..=20 {
            let t = i as f64 / 20.0;
            let x = seg.start.0 + t * (seg.end.0 - seg.start.0);
            let y = seg.start.1 + t * (seg.end.1 - seg.start.1);
            assert!(
                !(x > x0 && x < x1 && y > y0 && y < y1),
                "SIG track {:?}→{:?} runs over the NC pad 3 halo at ({x:.2},{y:.2})",
                seg.start, seg.end
            );
        }
    }
}


/// Exporter coordinate-convention regression: KiCad's canvas is Y-DOWN,
/// so the emitted footprint angle must be the NEGATION of the engine's
/// y-up theta. We parse the emitted footprint and recompute each pad's
/// global position exactly as KiCad does (x' = x + dx·cos a + dy·sin a,
/// y' = y − dx·sin a + dy·cos a) and require it to match the engine's
/// own y-up placement math. The DRC oracle originally caught this:
/// rotated SOT-23 pads landed mirrored about the component center and
/// every track "shorted" the wrong pad.
#[test]
fn kicad_export_places_rotated_pads_where_the_engine_thinks()  {
    let (mut board, _sig, _other) = board_with_blocker();
    // Rotate the middle component 90° like the LED fixture's SOT-23,
    // and give it an asymmetric pad offset so a mirror is detectable.
    let m = &mut board.components[2];
    m.theta = std::f64::consts::FRAC_PI_2;
    m.pins[0].dx = 0.925;
    m.pins[0].dy = 0.3;

    // Engine truth (y-up).
    let comp = &board.components[2];
    let (dx, dy) = (comp.pins[0].dx, comp.pins[0].dy);
    let gx = comp.x + dx * comp.theta.cos() - dy * comp.theta.sin();
    let gy = comp.y + dx * comp.theta.sin() + dy * comp.theta.cos();

    let pcb = bhdl_pnr::output::kicad::export_kicad_pcb(&board, &[]);

    // Parse the emitted (at x y rot) of footprint "TEST" named M and its
    // first pad's local (at dx dy).
    let fp_start = pcb
        .split("(footprint")
        .find(|s| s.contains("\"M\""))
        .expect("footprint M in export");
    let at = fp_start
        .lines()
        .next()
        .unwrap();
    let nums: Vec<f64> = at
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter_map(|t| t.parse().ok())
        .collect();
    let (fx, fy, rot) = (nums[0], nums[1], nums[2]);
    let pad_line = fp_start.lines().find(|l| l.contains("(pad ")).unwrap();
    let pn: Vec<f64> = pad_line
        .split("(at ")
        .nth(1)
        .unwrap()
        .split(')')
        .next()
        .unwrap()
        .split_whitespace()
        .filter_map(|t| t.parse().ok())
        .collect();
    let (pdx, pdy) = (pn[0], pn[1]);

    // KiCad's y-down pad transform with the EMITTED angle.
    let a = rot.to_radians();
    let kx = fx + pdx * a.cos() + pdy * a.sin();
    let ky = fy - pdx * a.sin() + pdy * a.cos();

    assert!(
        (kx - gx).abs() < 1e-6 && (ky - gy).abs() < 1e-6,
        "KiCad places the pad at ({kx:.3},{ky:.3}) but the engine routed to ({gx:.3},{gy:.3}) \
         — exported rotation convention broken (theta must be negated for KiCad's y-down canvas)"
    );
}
