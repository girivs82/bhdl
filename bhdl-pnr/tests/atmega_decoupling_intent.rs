//! End-to-end-ish milestone test for intent-driven placement.
//!
//! Builds an ATmega-like board (MCU + 4 decoupling caps) with the same
//! `LayoutIntent` annotations the stdlib `atmega328p.bhdl` expansion now
//! carries, lowers them to constraints, and drives the intent-force
//! functions to convergence — asserting each cap ends up adjacent to the
//! pin its intent targets, with hard constraints satisfied.
//!
//! This is the placement-side proof for `intent_vocabulary_v0.md` §5 /
//! `constraint_model_v0.md` §8: intent → constraint → cost term → correct
//! geometry, with no board-level annotation and no `PlacementRecipe`.
//!
//! The test drives the intent forces in a plain gradient loop rather than
//! the full `place_and_route` pipeline, to isolate the intent mechanism
//! from orthogonal placer machinery (block-init, progressive freezer,
//! legalization). Full-pipeline integration is tracked separately — see
//! the note on the pre-existing Fixed-component overlap-resolution bug.

use bhdl_common::intent::vocabulary::{LayoutIntent, PinRef};
use bhdl_pnr::constraint::eval::LayoutSnapshot;
use bhdl_pnr::constraint::PinSel;
use bhdl_pnr::intent::lower_board_intents;
use bhdl_pnr::constraint::eval::eval_all;
use bhdl_pnr::placement::intent_forces::{compute_loop_area_forces, compute_proximity_forces};
use bhdl_pnr::types::*;
use slotmap::SlotMap;

struct Ids {
    comp: SlotMap<ComponentId, ()>,
    pin: SlotMap<PinId, ()>,
    group: SlotMap<GroupId, ()>,
}

fn mcu(ids: &mut Ids, x: f64, y: f64) -> (Component, ComponentId, Vec<PinId>) {
    let id = ids.comp.insert(());
    // VCC, GND1, AVCC, GND2, AREF — at spread-out offsets on the package.
    let names = ["VCC", "GND1", "AVCC", "GND2", "AREF"];
    let offsets = [(-4.0, 4.0), (-4.0, 2.0), (4.0, 4.0), (4.0, 2.0), (4.0, 0.0)];
    let pins: Vec<PinPosition> = names
        .iter()
        .zip(offsets.iter())
        .map(|(n, (dx, dy))| PinPosition {
            pin_id: ids.pin.insert(()),
            name: (*n).into(),
            dx: *dx,
            dy: *dy,
            net: None,
        })
        .collect();
    let pin_ids = pins.iter().map(|p| p.pin_id).collect();
    let c = Component {
        id,
        name: "mcu".into(),
        refdes: "U1".into(),
        width_mm: 10.0,
        height_mm: 10.0,
        pins,
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.2,
        package: "DIP-28".into(),
        // Anchor the MCU so the test is about where the caps go.
        placement: PlacementConstraint::Fixed { x, y, theta: 0.0 },
        x,
        y,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: vec![],
    };
    (c, id, pin_ids)
}

fn cap(ids: &mut Ids, name: &str, x: f64, y: f64, intent: LayoutIntent) -> (Component, ComponentId) {
    let id = ids.comp.insert(());
    let pins = vec![
        PinPosition { pin_id: ids.pin.insert(()), name: "1".into(), dx: -0.5, dy: 0.0, net: None },
        PinPosition { pin_id: ids.pin.insert(()), name: "2".into(), dx: 0.5, dy: 0.0, net: None },
    ];
    let c = Component {
        id,
        name: name.into(),
        refdes: name.into(),
        width_mm: 1.0,
        height_mm: 0.5,
        pins,
        side: BoardSide::Top,
        group: None,
        thermal_power_w: 0.0,
        package: "0402".into(),
        placement: PlacementConstraint::Free,
        x,
        y,
        theta: 0.0,
        density_inflation: 1.0,
        layout_intents: vec![intent],
    };
    (c, id)
}

fn build_board() -> (Board, Vec<(ComponentId, PinSel)>) {
    let mut ids = Ids {
        comp: SlotMap::with_key(),
        pin: SlotMap::with_key(),
        group: SlotMap::with_key(),
    };

    let (mcu_c, mcu_id, _mcu_pins) = mcu(&mut ids, 25.0, 25.0);

    // Four caps, started deliberately FAR from the MCU (board corners), so
    // the test proves the placer pulls them in. Each references the host
    // entity's pins by name (HostPin), resolved via the functional group.
    let (c_vcc, c_vcc_id) = cap(
        &mut ids, "C_vcc", 2.0, 2.0,
        LayoutIntent::HighFreqBypass {
            rail: PinRef::HostPin("VCC".into()),
            return_pin: PinRef::HostPin("GND1".into()),
            loop_area_max_mm2: 1.5,
            proximity_max_mm: 2.0,
        },
    );
    let (c_bulk, c_bulk_id) = cap(
        &mut ids, "C_bulk", 48.0, 2.0,
        LayoutIntent::BulkReservoir {
            rail: PinRef::HostPin("VCC".into()),
            return_pin: PinRef::HostPin("GND1".into()),
            proximity_max_mm: 10.0,
        },
    );
    let (c_avcc, c_avcc_id) = cap(
        &mut ids, "C_avcc", 2.0, 48.0,
        LayoutIntent::HighFreqBypass {
            rail: PinRef::HostPin("AVCC".into()),
            return_pin: PinRef::HostPin("GND2".into()),
            loop_area_max_mm2: 1.5,
            proximity_max_mm: 2.0,
        },
    );
    let (c_aref, c_aref_id) = cap(
        &mut ids, "C_aref", 48.0, 48.0,
        LayoutIntent::AnalogRefFilter {
            ref_pin: PinRef::HostPin("AREF".into()),
            return_pin: PinRef::HostPin("GND2".into()),
            proximity_max_mm: 3.0,
        },
    );

    // Group all caps under the MCU (this is what reconstructs the host
    // relationship the lowering resolves HostPin against — mirrors
    // semantic.rs's expansion-group extraction).
    let g = ids.group.insert(());
    let group = FunctionalGroup {
        id: g,
        name: "mcu_decoupling".into(),
        members: vec![mcu_id, c_vcc_id, c_bulk_id, c_avcc_id, c_aref_id],
        parent: Some(mcu_id),
    };

    let board = Board {
        config: BoardConfig {
            outline: BoardOutline::Rectangle { width_mm: 50.0, height_mm: 50.0 },
            ..BoardConfig::default()
        },
        layer_stack: bhdl_pnr::stackup::stackup_preset(StackupPreset::TwoLayer),
        components: vec![mcu_c, c_vcc, c_bulk, c_avcc, c_aref],
        nets: vec![],
        groups: vec![group],
        placement_recipes: Default::default(),
        constraints: vec![],
    };

    // Targets: (cap id, the host pin it should end up adjacent to).
    let find_pin = |b: &Board, comp: ComponentId, name: &str| -> PinSel {
        let c = b.components.iter().find(|c| c.id == comp).unwrap();
        let p = c.pins.iter().find(|p| p.name == name).unwrap();
        PinSel { component: comp, pin: p.pin_id }
    };
    let targets = vec![
        (c_vcc_id, find_pin(&board, mcu_id, "VCC")),
        (c_avcc_id, find_pin(&board, mcu_id, "AVCC")),
        (c_aref_id, find_pin(&board, mcu_id, "AREF")),
    ];

    (board, targets)
}

fn dist(b: &Board, cap: ComponentId, pin: PinSel) -> f64 {
    let c = b.components.iter().find(|c| c.id == cap).unwrap();
    let (px, py) = b.pin_abs(pin).unwrap();
    ((c.x - px).powi(2) + (c.y - py).powi(2)).sqrt()
}

/// Drive ONLY the intent forces in a plain gradient loop — no block-init,
/// no progressive freezer, no legalization. This isolates the milestone
/// question ("do intent constraints produce correct cap geometry?") from
/// the surrounding placer machinery. The MCU is held fixed by simply not
/// updating it; free caps step along the accumulated descent direction.
fn drive_intent_forces(board: &mut Board, iters: usize, lr: f64) {
    for _ in 0..iters {
        let mut f = compute_proximity_forces(board);
        let la = compute_loop_area_forces(board);
        // Same weights as the lib.rs cost loop (proximity dominant).
        for i in 0..f.dx.len() {
            f.dx[i] += la.dx[i]; // lambda_loop_area = 1.0
            f.dy[i] += la.dy[i];
        }
        for (i, c) in board.components.iter_mut().enumerate() {
            if !c.placement.is_free() {
                continue;
            }
            // Normalize per-component step so far-away caps don't overshoot.
            let (dx, dy) = (f.dx[i], f.dy[i]);
            let mag = (dx * dx + dy * dy).sqrt().max(1e-9);
            let step = lr.min(mag); // cap the step length at lr
            c.x += step * dx / mag;
            c.y += step * dy / mag;
        }
    }
}

/// Milestone: intent constraints place each decoupling cap adjacent to
/// the specific host pin its intent targets, and minimize its return
/// loop. Drives the real intent-force functions to convergence.
#[test]
fn intent_forces_place_caps_at_their_pins() {
    let (mut board, targets) = build_board();
    let report = lower_board_intents(&mut board);
    assert!(
        report.constraints_emitted >= 7,
        "expected proximity+loop+layer constraints from 4 caps, got {}",
        report.constraints_emitted
    );
    assert!(report.diagnostics.is_empty(), "lowering diagnostics: {:?}", report.diagnostics);

    let before: Vec<f64> = targets.iter().map(|(c, p)| dist(&board, *c, *p)).collect();
    drive_intent_forces(&mut board, 2000, 0.2);

    for ((cap_id, pin), d0) in targets.iter().zip(before.iter()) {
        let d1 = dist(&board, *cap_id, *pin);
        let name = &board.components.iter().find(|c| c.id == *cap_id).unwrap().name;
        eprintln!("{name}: {d0:.1}mm -> {d1:.2}mm");
        // Each hard-proximity cap (high_freq_bypass / analog_ref_filter)
        // should land within its proximity target + slack of its pin.
        assert!(
            d1 < 4.0,
            "{name}: expected within ~4mm of its target pin, got {d1:.2}mm (was {d0:.1}mm)"
        );
        assert!(d1 < *d0, "{name}: expected to move closer ({d0:.1} -> {d1:.2})");
    }

    // The lowered constraint set should be (near-)satisfied at convergence:
    // hard proximity met, loop areas small.
    let summary = eval_all(&board);
    eprintln!(
        "post-convergence: soft_cost={:.3}, hard_cost={:.3}, hard_violations={}",
        summary.soft_cost,
        summary.hard_cost,
        summary.hard_violations.len()
    );
    assert!(
        summary.hard_violations.is_empty(),
        "all hard proximity constraints should be satisfied at convergence, {} remain: {:?}",
        summary.hard_violations.len(),
        summary.hard_violations
    );
}
