//! Regression test for the Fixed-component overlap-resolution bug.
//!
//! The placement loop's "Direct overlap resolution every iteration" block in
//! `lib.rs` used to mutate component positions checking only the progressive
//! freezer, not `placement.is_fixed()`. A `Fixed` component overlapped by a
//! free component could therefore be shoved out of its constrained position,
//! tripping the `debug_assert!` in `legalization::mod.rs` ("Fixed component
//! moved from ...") and panicking in debug builds.
//!
//! To force the overlap deterministically — block-init spreads components
//! apart and density holds them at the resolver's separation threshold — the
//! board is sized so two 5×5 mm components *cannot* avoid overlapping: a
//! 6×6 mm outline with 0.5 mm edge clearance confines both centres to a 5 mm
//! span, below the resolver's 5.5 mm minimum separation. Every iteration the
//! resolver then wants to push the pair apart, and (with the bug) shoves the
//! Fixed component. The test asserts the Fixed component's x/y/theta are
//! byte-identical before and after `place_and_route`.

use bhdl_pnr::place_and_route;
use bhdl_pnr::types::*;
use slotmap::SlotMap;

fn comp(
    comps: &mut SlotMap<ComponentId, ()>,
    pins: &mut SlotMap<PinId, ()>,
    name: &str,
    placement: PlacementConstraint,
    x: f64,
    y: f64,
) -> Component {
    let id = comps.insert(());
    let pin = PinPosition {
        pin_id: pins.insert(()),
        name: "1".into(),
        dx: 0.0,
        dy: 0.0,
        net: None,
            pad: None,
    };
    Component {
        id,
        name: name.into(),
        refdes: name.into(),
        width_mm: 5.0,
        height_mm: 5.0,
        pins: vec![pin],
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
        package: "0402".into(),
        placement,
        x,
        y,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: vec![],
    }
}

#[test]
fn fixed_component_not_moved_by_overlap_resolution() {
    let mut comps = SlotMap::with_key();
    let mut pins = SlotMap::with_key();

    // A Fixed component anchored at the centre of a deliberately tiny board.
    let fixed = comp(
        &mut comps,
        &mut pins,
        "U_fixed",
        PlacementConstraint::Fixed { x: 3.0, y: 3.0, theta: 0.0 },
        3.0,
        3.0,
    );
    let fixed_id = fixed.id;
    let (fx, fy, ftheta) = (fixed.x, fixed.y, fixed.theta);

    // A Free component. On a 6×6 mm board it is forced to overlap the fixed
    // one no matter where it lands.
    let free = comp(
        &mut comps,
        &mut pins,
        "R_free",
        PlacementConstraint::Free,
        3.0,
        3.0,
    );

    let board = Board {
        config: BoardConfig {
            outline: BoardOutline::Rectangle { width_mm: 6.0, height_mm: 6.0 },
            ..BoardConfig::default()
        },
        layer_stack: bhdl_pnr::stackup::stackup_preset(StackupPreset::TwoLayer),
        components: vec![fixed, free],
        nets: vec![],
        groups: vec![],
        placement_recipes: Default::default(),
        constraints: vec![],
    };

    // Before the fix this panicked via the legalization debug_assert (or, in
    // a release build, silently relocated the fixed component); the
    // pure-overlap shove on the Fixed component must no longer happen.
    let result = place_and_route(board, PnrConfig::default(), 1)
        .expect("place_and_route should succeed");

    let after = result
        .board
        .components
        .iter()
        .find(|c| c.id == fixed_id)
        .expect("fixed component survives P&R");

    assert_eq!(after.x, fx, "fixed component x must not move");
    assert_eq!(after.y, fy, "fixed component y must not move");
    assert_eq!(after.theta, ftheta, "fixed component theta must not move");
}
