//! Exercises the whole-net bootstrap directly: greedy recovery on the
//! corpus always lands SOME copper first, so the bootstrap never fires
//! there — this is its behavioral contract. Two cases: an open board
//! (direct pad-to-pad seed on the pad layer) and a full-height crossing
//! fence (no same-layer path and no shovable squeeze — must cross
//! under with two vias, exactly sited).

use bhdl_pnr::bootstrap_empty_route;
use bhdl_pnr::types::*;
use slotmap::SlotMap;

fn board_with_fence(fenced: bool) -> (Board, Vec<Route>) {
    let mut ck: SlotMap<ComponentId, ()> = SlotMap::with_key();
    let mut pk: SlotMap<PinId, ()> = SlotMap::with_key();
    let mut nk: SlotMap<NetId, ()> = SlotMap::with_key();

    let a = ck.insert(());
    let b = ck.insert(());
    let a0 = pk.insert(());
    let b0 = pk.insert(());
    let sig = nk.insert(());
    let wall = nk.insert(());

    let mk = |id, name: &str, x, y, pins: Vec<PinPosition>| Component {
        id,
        name: name.into(),
        refdes: name.into(),
        width_mm: 3.0,
        height_mm: 3.0,
        pins,
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
        solved_current_a: None,
        package: "R0402".into(),
        placement: PlacementConstraint::Free,
        x,
        y,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: vec![],
        bbox_dx: 0.0,
        bbox_dy: 0.0,
    };
    let comp_a = mk(a, "A", 10.0, 25.0, vec![PinPosition {
        pin_id: a0, name: "1".into(), dx: 0.0, dy: 0.0, net: Some(sig), pad: None, unplaced: false,
    }]);
    let comp_b = mk(b, "B", 40.0, 25.0, vec![PinPosition {
        pin_id: b0, name: "1".into(), dx: 0.0, dy: 0.0, net: Some(sig), pad: None, unplaced: false,
    }]);

    let mk_net = |id, name: &str, pins: Vec<(ComponentId, PinId)>| PnrNet {
        allowed_layers: None,
        solved_voltage_v: None,
        edge_swing_v: None,
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
        ddr_bin: None,
        config: BoardConfig {
            outline: BoardOutline::Rectangle { width_mm: 50.0, height_mm: 50.0 },
            ..BoardConfig::default()
        },
        layer_stack: bhdl_pnr::stackup::stackup_preset(StackupPreset::TwoLayer),
        components: vec![comp_a, comp_b],
        nets: vec![
            mk_net(sig, "SIG", vec![(a, a0), (b, b0)]),
            mk_net(wall, "WALL", vec![]),
        ],
        groups: vec![],
        placement_recipes: Default::default(),
        constraints: vec![],
    };

    let mut wall_route = Route::empty(wall);
    if fenced {
        // Full-height vertical wall between the pads on the top layer:
        // no same-layer path (direct/L/Z/U all cross it) and no
        // shovable squeeze (it CROSSES every escape chord).
        let ec = board.config.edge_clearance_mm;
        wall_route.segments.push(RouteSegment {
            layer: 0,
            start: (25.0, ec + 0.2),
            end: (25.0, 50.0 - ec - 0.2),
            width_mm: 0.3,
        });
        wall_route.path_spans.push((0, 1));
        wall_route.path_parents.push(None);
        wall_route.via_spans.push((0, 0));
    }
    let routes = vec![Route::empty(sig), wall_route];
    (board, routes)
}

#[test]
fn bootstrap_seeds_open_board_on_pad_layer() {
    let (board, mut routes) = board_with_fence(false);
    assert!(bootstrap_empty_route(&board, &mut routes, 0));
    let r = &routes[0];
    assert!(!r.segments.is_empty(), "seed span committed");
    assert!(r.vias.is_empty(), "open board needs no vias");
    assert!(r.segments.iter().all(|sg| sg.layer == 0), "stays on the pad layer");
    // Span endpoints reach both pads.
    let touches = |p: (f64, f64)| {
        r.segments.iter().any(|sg| {
            (sg.start.0 - p.0).hypot(sg.start.1 - p.1) < 1e-6
                || (sg.end.0 - p.0).hypot(sg.end.1 - p.1) < 1e-6
        })
    };
    assert!(touches((10.0, 25.0)) && touches((40.0, 25.0)));
}

#[test]
fn bootstrap_crosses_under_a_full_height_fence() {
    let (board, mut routes) = board_with_fence(true);
    assert!(bootstrap_empty_route(&board, &mut routes, 0));
    let r = &routes[0];
    assert_eq!(r.vias.len(), 2, "cross-under uses exactly two vias");
    assert!(
        r.segments.iter().any(|sg| sg.layer == 1),
        "tunnel runs on the far layer"
    );
    // The wall itself must be untouched (crossing fences are not
    // shovable — the bootstrap must not deform it).
    assert_eq!(routes[1].segments.len(), 1);
    assert!((routes[1].segments[0].start.0 - 25.0).abs() < 1e-9);
}
