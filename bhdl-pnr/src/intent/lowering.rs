//! Lowering driver: walk a built `Board`'s component/net intents, run the
//! per-kind recipes, and populate `Board.constraints`.
//!
//! Runs after `semantic::build_board` (which assigns slotmap IDs and
//! reconstructs functional groups) and before placement. Idempotent-ish:
//! it appends to `Board.constraints`, so call once.

use crate::types::Board;

use super::recipes::lower_component_intents;
use super::resolve::LoweringContext;

/// Summary of a lowering pass, for logging / CLI `--emit-constraints`.
#[derive(Debug, Default)]
pub struct LoweringReport {
    pub constraints_emitted: usize,
    pub components_with_intent: usize,
    pub diagnostics: Vec<String>,
}

/// Lower all expansion/board intents on `board` into `board.constraints`.
///
/// (Interface constraints from `intf_const__*` module attributes are a
/// separate producer — `intent::interface_constraints`, TODO — and append
/// to the same vector.)
pub fn lower_board_intents(board: &mut Board) -> LoweringReport {
    let ctx = LoweringContext::build(board);
    let mut report = LoweringReport::default();

    // Snapshot component (id, intents) up front so we don't borrow `board`
    // mutably while reading. Intents are cheap clones (small vecs).
    let component_intents: Vec<_> = board
        .components
        .iter()
        .filter(|c| !c.layout_intents.is_empty())
        .map(|c| (c.id, c.layout_intents.clone()))
        .collect();

    for (id, intents) in &component_intents {
        report.components_with_intent += 1;
        let out = lower_component_intents(*id, intents, &ctx);
        report.constraints_emitted += out.constraints.len();
        report.diagnostics.extend(out.diagnostics);
        board.constraints.extend(out.constraints);
    }

    if !report.diagnostics.is_empty() {
        for d in &report.diagnostics {
            log::warn!("intent lowering: {d}");
        }
    }
    log::info!(
        "intent lowering: {} constraints from {} annotated components",
        report.constraints_emitted,
        report.components_with_intent
    );

    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::Constraint;
    use crate::types::*;
    use bhdl_common::intent::vocabulary::{LayoutIntent, PinRef};
    use slotmap::SlotMap;

    /// Build a board with an MCU (pins VCC, GND1) and a decoupling cap
    /// (pins 1, 2) grouped under the MCU, the cap carrying a
    /// `high_freq_bypass` intent referencing the MCU's host pins.
    fn atmega_like_board() -> Board {
        let mut comp_keys: SlotMap<ComponentId, ()> = SlotMap::with_key();
        let mut pin_keys: SlotMap<PinId, ()> = SlotMap::with_key();
        let mut group_keys: SlotMap<GroupId, ()> = SlotMap::with_key();

        let mcu_id = comp_keys.insert(());
        let vcc = pin_keys.insert(());
        let gnd1 = pin_keys.insert(());
        let mcu = Component {
            id: mcu_id,
            name: "mcu".into(),
            refdes: "U1".into(),
            width_mm: 10.0,
            height_mm: 10.0,
            pins: vec![
                PinPosition { pin_id: vcc, name: "VCC".into(), dx: 0.0, dy: 0.0, net: None, pad: None, unplaced: false },
                PinPosition { pin_id: gnd1, name: "GND1".into(), dx: 2.0, dy: 0.0, net: None, pad: None, unplaced: false },
            ],
            side: BoardSide::Top,
            group: None,
            thermal_power_w: 0.1,
            solved_current_a: None,
            package: "DIP-28".into(),
            placement: PlacementConstraint::Free,
            x: 20.0,
            y: 20.0,
            theta: 0.0,
            density_inflation: 1.0,
            layout_intents: vec![],
            bbox_dx: 0.0,
            bbox_dy: 0.0,
        };

        let cap_id = comp_keys.insert(());
        let c1 = pin_keys.insert(());
        let c2 = pin_keys.insert(());
        let cap = Component {
            id: cap_id,
            name: "C_vcc".into(),
            refdes: "C1".into(),
            width_mm: 1.0,
            height_mm: 0.5,
            pins: vec![
                PinPosition { pin_id: c1, name: "1".into(), dx: -0.5, dy: 0.0, net: None, pad: None, unplaced: false },
                PinPosition { pin_id: c2, name: "2".into(), dx: 0.5, dy: 0.0, net: None, pad: None, unplaced: false },
            ],
            side: BoardSide::Top,
            group: None,
            thermal_power_w: 0.0,
            solved_current_a: None,
            package: "0402".into(),
            placement: PlacementConstraint::Free,
            // Place the cap far from the MCU so the proximity constraint
            // is initially violated (lowering still emits it).
            x: 40.0,
            y: 40.0,
            theta: 0.0,
            density_inflation: 1.0,
            layout_intents: vec![LayoutIntent::HighFreqBypass {
                rail: PinRef::HostPin("VCC".into()),
                return_pin: PinRef::HostPin("GND1".into()),
                loop_area_max_mm2: 1.5,
                proximity_max_mm: 2.0,
            }],
            bbox_dx: 0.0,
            bbox_dy: 0.0,
        };

        let g = group_keys.insert(());
        let group = FunctionalGroup {
            id: g,
            name: "mcu_decoupling".into(),
            members: vec![mcu_id, cap_id],
            parent: Some(mcu_id),
        };

        Board {
            config: BoardConfig::default(),
            layer_stack: crate::stackup::stackup_preset(StackupPreset::TwoLayer),
            components: vec![mcu, cap],
            nets: vec![],
            groups: vec![group],
            placement_recipes: Default::default(),
            constraints: vec![],
            ddr_bin: None,
        }
    }

    #[test]
    fn lowers_high_freq_bypass_to_proximity_and_loop_area() {
        let mut board = atmega_like_board();
        let report = lower_board_intents(&mut board);

        assert_eq!(report.components_with_intent, 1);
        assert!(report.diagnostics.is_empty(), "diagnostics: {:?}", report.diagnostics);

        // Expect: Proximity (hard) + LoopArea (soft) + LayerHint (soft).
        let prox = board.constraints.iter().filter(|c| matches!(c, Constraint::Proximity { .. })).count();
        let loops = board.constraints.iter().filter(|c| matches!(c, Constraint::LoopArea { .. })).count();
        let layer = board.constraints.iter().filter(|c| matches!(c, Constraint::LayerHint { .. })).count();
        assert_eq!(prox, 1, "one proximity constraint");
        assert_eq!(loops, 1, "one loop-area constraint");
        assert_eq!(layer, 1, "one layer hint");

        // The proximity must reference the cap and the MCU's VCC pin.
        let p = board.constraints.iter().find(|c| matches!(c, Constraint::Proximity { .. })).unwrap();
        if let Constraint::Proximity { a, b, max_mm, hardness, .. } = p {
            use crate::constraint::EntitySel;
            assert!(matches!(a, EntitySel::Component(_)));
            assert!(matches!(b, EntitySel::Pin(_)));
            assert_eq!(*max_mm, 2.0);
            assert!(hardness.is_hard());
        }
    }

    #[test]
    fn missing_host_pin_degrades_gracefully() {
        let mut board = atmega_like_board();
        // Point the intent at a pin the MCU doesn't have.
        if let LayoutIntent::HighFreqBypass { rail, .. } =
            &mut board.components[1].layout_intents[0]
        {
            *rail = PinRef::HostPin("NONEXISTENT".into());
        }
        let report = lower_board_intents(&mut board);
        // No proximity emitted (rail couldn't resolve), one diagnostic.
        let prox = board.constraints.iter().filter(|c| matches!(c, Constraint::Proximity { .. })).count();
        assert_eq!(prox, 0);
        assert_eq!(report.diagnostics.len(), 1);
        // Build did not panic — warn-and-degrade honored.
    }
}
