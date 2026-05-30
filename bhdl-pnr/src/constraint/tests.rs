//! Unit tests for the constraint catalog + evaluation.
//!
//! These build tiny synthetic boards directly (no synthesizer) and check
//! that `eval` behaves as the constraint-model doc claims — in particular
//! that the shoelace loop-area approximation collapses when a bypass cap
//! is placed between its rail and return pins.

use crate::constraint::eval::{eval_all, shoelace_area, LayoutSnapshot};
use crate::constraint::{
    Constraint, ConstraintSource, CostShape, Eval, Hardness, PinSel,
};
use crate::types::*;

use slotmap::SlotMap;

/// Build a minimal 2-pin component at the origin with two pins offset on x.
fn make_component(
    comps: &mut SlotMap<ComponentId, ()>,
    pins: &mut SlotMap<PinId, ()>,
    name: &str,
    x: f64,
    y: f64,
    pin_offsets: &[(f64, f64)],
) -> (ComponentId, Vec<PinId>, Component) {
    let id = comps.insert(());
    let pin_positions: Vec<PinPosition> = pin_offsets
        .iter()
        .enumerate()
        .map(|(i, (dx, dy))| PinPosition {
            pin_id: pins.insert(()),
            name: format!("{}", i + 1),
            dx: *dx,
            dy: *dy,
            net: None,
        })
        .collect();
    let pin_ids = pin_positions.iter().map(|p| p.pin_id).collect();
    let comp = Component {
        id,
        name: name.into(),
        refdes: name.into(),
        width_mm: 1.0,
        height_mm: 0.5,
        pins: pin_positions,
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
        package: "0402".into(),
        placement: PlacementConstraint::Free,
        x,
        y,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: Vec::new(),
    };
    (id, pin_ids, comp)
}

fn empty_board(components: Vec<Component>, constraints: Vec<Constraint>) -> Board {
    Board {
        config: BoardConfig::default(),
        layer_stack: crate::stackup::stackup_preset(StackupPreset::TwoLayer),
        components,
        nets: Vec::new(),
        groups: Vec::new(),
        placement_recipes: Default::default(),
        constraints,
    }
}

fn soft_quadratic(weight: f64) -> Hardness {
    Hardness::Soft { shape: CostShape::Quadratic, weight }
}

#[test]
fn shoelace_collapses_for_straddling_cap() {
    // Rail pin at (0,0), return pin at (4,0). A cap whose two pins sit on
    // the segment between them encloses (near) zero area; a cap placed off
    // to the side encloses a real quadrilateral.
    let straddling = [(0.0, 0.0), (1.0, 0.0), (3.0, 0.0), (4.0, 0.0)];
    let area_straddle = shoelace_area(&straddling);
    assert!(area_straddle < 1e-9, "collinear loop should be ~0, got {area_straddle}");

    let offset = [(0.0, 0.0), (1.0, 2.0), (3.0, 2.0), (4.0, 0.0)];
    let area_offset = shoelace_area(&offset);
    assert!(area_offset > 1.0, "offset loop should enclose area, got {area_offset}");
}

#[test]
fn proximity_satisfied_and_violated() {
    let mut comps = SlotMap::with_key();
    let mut pins = SlotMap::with_key();

    // MCU at origin, cap 1.5mm away.
    let (mcu_id, _mcu_pins, mcu) =
        make_component(&mut comps, &mut pins, "mcu", 0.0, 0.0, &[(0.0, 0.0)]);
    let (cap_id, _cap_pins, cap) =
        make_component(&mut comps, &mut pins, "C_vcc", 1.5, 0.0, &[(0.0, 0.0), (0.5, 0.0)]);

    use crate::constraint::EntitySel;
    // Within 2mm → satisfied.
    let ok = Constraint::Proximity {
        a: EntitySel::Component(cap_id),
        b: EntitySel::Component(mcu_id),
        max_mm: 2.0,
        hardness: Hardness::Hard,
        source: ConstraintSource::intent("high_freq_bypass"),
    };
    // Within 1mm → violated by 0.5mm.
    let bad = Constraint::Proximity {
        a: EntitySel::Component(cap_id),
        b: EntitySel::Component(mcu_id),
        max_mm: 1.0,
        hardness: Hardness::Hard,
        source: ConstraintSource::intent("high_freq_bypass"),
    };

    let board = empty_board(vec![mcu, cap], vec![]);
    assert_eq!(ok.eval(&board), Eval::Satisfied);
    match bad.eval(&board) {
        Eval::Violated { slack, .. } => assert!((slack - 0.5).abs() < 1e-5),
        other => panic!("expected violation, got {other:?}"),
    }
}

#[test]
fn loop_area_constraint_rewards_straddle() {
    let mut comps = SlotMap::with_key();
    let mut pins = SlotMap::with_key();

    // Rail pin on the MCU at (0,0); return pin on the MCU at (4,0).
    let (_mcu_id, mcu_pins, mcu) = make_component(
        &mut comps,
        &mut pins,
        "mcu",
        0.0,
        0.0,
        &[(0.0, 0.0), (4.0, 0.0)],
    );
    let rail = PinSel { component: mcu.id, pin: mcu_pins[0] };
    let ret = PinSel { component: mcu.id, pin: mcu_pins[1] };

    // Cap straddling between them (pins at x=1 and x=3 on the segment).
    let (_cap_id, cap_pins, cap) = make_component(
        &mut comps,
        &mut pins,
        "C_vcc",
        2.0,
        0.0,
        &[(-1.0, 0.0), (1.0, 0.0)],
    );
    let c1 = PinSel { component: cap.id, pin: cap_pins[0] };
    let c2 = PinSel { component: cap.id, pin: cap_pins[1] };

    let loop_c = Constraint::LoopArea {
        loop_pins: vec![rail, c1, c2, ret],
        max_mm2: 1.5,
        hardness: soft_quadratic(4.0),
        source: ConstraintSource::intent("high_freq_bypass"),
    };

    let board = empty_board(vec![mcu, cap], vec![loop_c]);
    // Straddling, collinear → ~0 area → satisfied (no soft cost).
    assert_eq!(board.constraints[0].eval(&board), Eval::Satisfied);

    // Now lift the cap off-axis and confirm a soft cost appears.
    let mut board2 = board.clone();
    board2.components[1].y = 3.0; // move cap up
    let summary = eval_all(&board2);
    assert!(
        summary.soft_cost > 0.0,
        "off-axis cap should incur loop-area soft cost, got {}",
        summary.soft_cost
    );
}

#[test]
fn routing_constraints_are_unknown_pre_routing() {
    let mut comps = SlotMap::with_key();
    let mut pins = SlotMap::with_key();
    let (_id, p, c) =
        make_component(&mut comps, &mut pins, "U1", 0.0, 0.0, &[(0.0, 0.0), (1.0, 0.0)]);

    let tl = Constraint::TraceLength {
        from: PinSel { component: c.id, pin: p[0] },
        to: PinSel { component: c.id, pin: p[1] },
        max_mm: 5.0,
        hardness: Hardness::Hard,
        source: ConstraintSource::intent("series_termination"),
    };
    let board = empty_board(vec![c], vec![tl]);
    assert_eq!(board.constraints[0].eval(&board), Eval::Unknown);

    // eval_all should count it as unknown, not a violation.
    let s = eval_all(&board);
    assert_eq!(s.unknown, 1);
    assert_eq!(s.hard_cost, 0.0);
}
