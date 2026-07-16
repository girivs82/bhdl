//! Exercises the FULL `place_and_route` pipeline **including routing** on
//! a small netted board carrying a net/signal constraint. Every other
//! test runs on net-less boards (routing is a no-op there); this is the
//! first to actually run the PathFinder router and the constraint-driven
//! criticality ordering.

use bhdl_pnr::constraint::{Constraint, ConstraintSource};
use bhdl_pnr::{place_and_route, types::*};
use slotmap::SlotMap;

/// Two 2-pin components wired by two signal nets. Net "CLK" carries a
/// CLOCK signal-class constraint (exercises criticality ordering); net
/// "D0" is plain.
fn netted_board() -> Board {
    let mut ck: SlotMap<ComponentId, ()> = SlotMap::with_key();
    let mut pk: SlotMap<PinId, ()> = SlotMap::with_key();
    let mut nk: SlotMap<NetId, ()> = SlotMap::with_key();

    let a = ck.insert(());
    let b = ck.insert(());
    let a0 = pk.insert(());
    let a1 = pk.insert(());
    let b0 = pk.insert(());
    let b1 = pk.insert(());
    let clk = nk.insert(());
    let d0 = nk.insert(());

    let mk = |id, name: &str, x, y, pins: Vec<PinPosition>| Component {
        id,
        name: name.into(),
        refdes: name.into(),
        width_mm: 4.0,
        height_mm: 4.0,
        pins,
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
            solved_current_a: None,
        package: "SOIC-8".into(),
        placement: PlacementConstraint::Free,
        x,
        y,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: vec![],
            bbox_dx: 0.0,
            bbox_dy: 0.0,
        };

    let comp_a = mk(a, "A", 10.0, 25.0, vec![
        PinPosition { pin_id: a0, name: "1".into(), dx: -2.0, dy: 1.0, net: Some(clk), pad: None, unplaced: false },
        PinPosition { pin_id: a1, name: "2".into(), dx: -2.0, dy: -1.0, net: Some(d0), pad: None, unplaced: false },
    ]);
    let comp_b = mk(b, "B", 40.0, 25.0, vec![
        PinPosition { pin_id: b0, name: "1".into(), dx: 2.0, dy: 1.0, net: Some(clk), pad: None, unplaced: false },
        PinPosition { pin_id: b1, name: "2".into(), dx: 2.0, dy: -1.0, net: Some(d0), pad: None, unplaced: false },
    ]);

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

    Board {
        config: BoardConfig {
            outline: BoardOutline::Rectangle { width_mm: 50.0, height_mm: 50.0 },
            ..BoardConfig::default()
        },
        layer_stack: bhdl_pnr::stackup::stackup_preset(StackupPreset::TwoLayer),
        components: vec![comp_a, comp_b],
        nets: vec![
            mk_net(clk, "CLK", vec![(a, a0), (b, b0)]),
            mk_net(d0, "D0", vec![(a, a1), (b, b1)]),
        ],
        groups: vec![],
        placement_recipes: Default::default(),
        // A CLOCK signal-class constraint on CLK → higher routing
        // criticality than the plain D0 net.
        constraints: vec![Constraint::SignalClass {
            net: clk,
            class: "CLOCK".into(),
            max_freq_hz: Some(100e6),
            source: ConstraintSource::intent("interface:signal_class"),
        }],
    }
}

#[test]
fn full_pipeline_routes_a_constrained_board() {
    let board = netted_board();
    let config = PnrConfig { max_iterations: 300, ..PnrConfig::default() };

    let result = place_and_route(board, config, 0).expect("place_and_route should succeed");

    // The full pipeline executed the router (constraint-driven criticality
    // ordering included): one Route slot per net, metrics computed.
    assert_eq!(result.routes.len(), 2, "expected a route entry per net");
    let routed = result.routes.iter().filter(|r| !r.is_empty()).count();
    eprintln!("routed {}/2 nets, HPWL={:.1}mm", routed, result.metrics.hpwl_mm);
    // Both nets must actually route. This guards a former router bug: a pin
    // whose pad center happened to land on a grid-cell center (a sub-cell
    // alignment accident vs. the 1mm routing grid) got its own cell marked
    // `blocked` by the pad keepaway, and `dijkstra_to_any` never expanded
    // into a blocked sink cell — so the net was spuriously declared
    // unroutable. The post-placement coordinates here land exactly that way,
    // so this is a direct regression for that fix (was 0/2, now 2/2).
    assert_eq!(routed, 2, "both nets should route (had {}/2)", routed);

    // Placement produced a finite wirelength and in-bounds components.
    assert!(result.metrics.hpwl_mm.is_finite() && result.metrics.hpwl_mm > 0.0);
    for c in &result.board.components {
        assert!(c.x >= -1.0 && c.x <= 51.0 && c.y >= -1.0 && c.y <= 51.0,
            "{} placed out of bounds at ({}, {})", c.name, c.x, c.y);
    }

    // The criticality ordering ranks the CLOCK-constrained net above the
    // plain net (the reason it routes first).
    let crit = bhdl_pnr::routing::criticality::net_criticality(&result.board);
    let clk = result.board.nets.iter().find(|n| n.name == "CLK").unwrap();
    let d0 = result.board.nets.iter().find(|n| n.name == "D0").unwrap();
    let ew = |n| bhdl_pnr::routing::criticality::effective_weight(n, &crit);
    assert!(ew(clk) > ew(d0), "CLK ({}) should outrank D0 ({})", ew(clk), ew(d0));
}
