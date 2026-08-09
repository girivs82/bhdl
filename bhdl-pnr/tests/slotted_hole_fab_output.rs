//! A slotted pad must reach FABRICATION as a milled slot.
//!
//! Mounting lugs and wide power terminals need oblong holes; a round
//! approximation either refuses the lug or eats clearance. The board
//! model can now carry the slot, and this pins the two emissions that
//! actually reach a fab house: KiCad's oval drill form (so the oracle
//! judges the real opening) and an Excellon G85 route.

use bhdl_pnr::types::*;
use slotmap::SlotMap;

fn slotted_board() -> Board {
    let mut comps: SlotMap<ComponentId, ()> = SlotMap::with_key();
    let mut pins: SlotMap<PinId, ()> = SlotMap::with_key();
    let id = comps.insert(());
    let pin_id = pins.insert(());
    let comp = Component {
        id,
        name: "J1".into(),
        refdes: "J1".into(),
        width_mm: 6.0,
        height_mm: 6.0,
        pins: vec![PinPosition {
            pin_id,
            name: "1".into(),
            dx: 0.0,
            dy: 0.0,
            net: None,
            // 4.3 x 1.7 copper over a 3.6 x 1.0 slot — the reference
            // board's own DC-jack terminal. The pad must exceed its
            // hole in every direction.
            pad: Some(PadGeom {
                width_mm: 4.3,
                height_mm: 1.7,
                shape: PadShapeKind::RoundRect,
                drill_mm: Some(1.0),
                drill_slot_mm: Some((3.6, 1.0)),
            }),
            unplaced: false,
        }],
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
        solved_current_a: None,
        package: "DC-10A".into(),
        placement: PlacementConstraint::Fixed { x: 10.0, y: 10.0, theta: 0.0 },
        x: 10.0,
        y: 10.0,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: vec![],
        bbox_dx: 0.0,
        bbox_dy: 0.0,
    };
    Board {
        ddr_bin: None,
        config: BoardConfig {
            outline: BoardOutline::Rectangle { width_mm: 20.0, height_mm: 20.0 },
            ..BoardConfig::default()
        },
        layer_stack: bhdl_pnr::stackup::stackup_preset(StackupPreset::TwoLayer),
        components: vec![comp],
        nets: vec![],
        groups: vec![],
        placement_recipes: Default::default(),
        constraints: vec![],
    }
}

#[test]
fn kicad_writer_emits_an_oval_drill() {
    let board = slotted_board();
    let pcb = bhdl_pnr::output::kicad::export_kicad_pcb(&board, &[]);
    assert!(
        pcb.contains("(drill oval 3.6 1)"),
        "slotted pad must emit KiCad's oval drill so the oracle sees the real \
         opening; got:\n{}",
        pcb.lines()
            .filter(|l| l.contains("(pad "))
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn excellon_routes_the_slot_between_end_centres() {
    let board = slotted_board();
    let pkg = bhdl_pnr::output::gerber::export_fab(
        &board,
        &[],
        &bhdl_pnr::output::kicad::BoardFills::default(),
    );
    let drl = &pkg.drill.contents;
    assert!(drl.contains("G85"), "slot must be a routed hole:\n{drl}");
    assert!(
        drl.contains("C1.000"),
        "tool must be the slot's MINOR axis (an oversized tool would \
         widen the slot):\n{drl}"
    );
    let line = drl.lines().find(|l| l.contains("G85")).unwrap();
    // Parse the two X ordinates specifically: a naive numeric split
    // also swallows the 85 out of "G85".
    let xs: Vec<f64> = line
        .split('X')
        .skip(1)
        .filter_map(|t| {
            t.split(|c: char| c != '.' && c != '-' && !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse().ok())
        })
        .collect();
    assert_eq!(xs.len(), 2, "a G85 route has a start and an end: `{line}`");
    // Travel is END-CENTRE to END-CENTRE: 3.6 long minus the 1.0 tool.
    let dx = (xs[1] - xs[0]).abs();
    assert!(
        (dx - 2.6).abs() < 1e-6,
        "milled length must come out at 3.6mm: travel should be 2.6, got {dx} \
         from `{line}`"
    );
}
