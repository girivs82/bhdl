//! Constraint-driven routing criticality.
//!
//! Net/signal constraints (from the interface-constraint reader) make
//! some nets more sensitive to routing quality than others — a
//! diff pair, an impedance-controlled line, or a length-matched bus needs
//! a clean path *before* congestion builds. This module scores each net
//! by the constraints touching it; the router adds the score to the net's
//! base weight so constrained nets route first (negotiated-congestion
//! routers are order-sensitive: earlier nets get the least-congested
//! paths).
//!
//! This is the v0 router-side consumption of the net/signal half of the
//! constraint catalog — *ordering* only. Full diff-pair coupled routing,
//! length matching, and impedance-controlled width are later work; this
//! is the cheapest high-value step (`constraint_model_v0.md` §7).

use std::collections::HashMap;

use crate::constraint::Constraint;
use crate::types::{Board, NetId};

/// Criticality bonuses by constraint kind (added to a net's base weight).
/// Higher = routed earlier. Tuned so a diff pair outranks a plain
/// impedance line outranks a length-matched bus member outranks a
/// classified signal, all above unconstrained nets.
mod bonus {
    pub const DIFF_PAIR: f64 = 100.0;
    pub const IMPEDANCE: f64 = 60.0;
    pub const TOPOLOGY: f64 = 50.0;
    pub const LENGTH_MATCH: f64 = 40.0;
    pub const CLOCK: f64 = 80.0;
    pub const DATA: f64 = 30.0;
    pub const OTHER_CLASS: f64 = 20.0;
}

/// Per-net routing criticality from the board's constraint set. Nets not
/// present in the map have zero bonus.
pub fn net_criticality(board: &Board) -> HashMap<NetId, f64> {
    let mut score: HashMap<NetId, f64> = HashMap::new();
    let mut add = |net: NetId, b: f64| {
        let e = score.entry(net).or_insert(0.0);
        // Take the max bonus per kind rather than summing duplicates, but
        // accumulate across distinct kinds — use simple addition here and
        // rely on the kind set being small per net.
        *e += b;
    };

    for c in &board.constraints {
        match c {
            Constraint::DiffPair { p_net, n_net, .. } => {
                add(*p_net, bonus::DIFF_PAIR);
                add(*n_net, bonus::DIFF_PAIR);
            }
            Constraint::Impedance { net, .. } => add(*net, bonus::IMPEDANCE),
            Constraint::Topology { net, .. } => add(*net, bonus::TOPOLOGY),
            Constraint::LengthMatchGroup { nets, .. } => {
                for n in nets {
                    add(*n, bonus::LENGTH_MATCH);
                }
            }
            Constraint::SignalClass { net, class, .. } => {
                let b = match class.as_str() {
                    "CLOCK" => bonus::CLOCK,
                    "DATA" | "DM" => bonus::DATA,
                    _ => bonus::OTHER_CLASS,
                };
                add(*net, b);
            }
            _ => {}
        }
    }

    score
}

/// Effective routing weight: base net weight plus constraint criticality.
/// The router sorts descending by this to pick net order.
pub fn effective_weight(net: &crate::types::PnrNet, crit: &HashMap<NetId, f64>) -> f64 {
    // FAT TRUNKS FIRST: a power/ground rail that did NOT get a plane
    // (the stackup is the user's budget — PnR never adds layers) must
    // route as wide copper, and wide copper wants an empty board: a
    // human routes the power trunks before any signal. The bonus puts
    // every plane-less power-class net ahead of all signals while
    // preserving relative order among rails (fatter = earlier via the
    // width term) and among signals (constraint criticality).
    let power_first = match net.net_class {
        crate::types::PnrNetClass::Power { .. } | crate::types::PnrNetClass::Ground
            if net.plane_layer.is_none() =>
        {
            5.0 + net.required_trace_width_mm
        }
        _ => 0.0,
    };
    net.weight + power_first + crit.get(&net.id).copied().unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::constraint::{Constraint, ConstraintSource};
    use crate::types::*;
    use slotmap::SlotMap;

    fn mk_net(id: NetId, name: &str, weight: f64) -> PnrNet {
        PnrNet {
            id,
            name: name.into(),
            pins: vec![],
            net_class: PnrNetClass::Signal,
            weight,
            required_trace_width_mm: 0.15,
            layer_constraint: LayerConstraint::Any,
            intent: None,
            layout_intents: vec![],
            plane_layer: None,
            plane_region: None,
        }
    }

    #[test]
    fn diff_pair_and_clock_outrank_plain_nets() {
        let mut k: SlotMap<NetId, ()> = SlotMap::with_key();
        let dp_p = k.insert(());
        let dp_n = k.insert(());
        let clk = k.insert(());
        let data = k.insert(());
        let plain = k.insert(());
        let src = ConstraintSource::intent("interface:x");

        let board = Board {
            config: BoardConfig::default(),
            layer_stack: crate::stackup::stackup_preset(StackupPreset::TwoLayer),
            components: vec![],
            nets: vec![
                mk_net(dp_p, "DQS.P", 1.0),
                mk_net(dp_n, "DQS.N", 1.0),
                mk_net(clk, "CK", 1.0),
                mk_net(data, "DQ0", 1.0),
                mk_net(plain, "GPIO", 1.0),
            ],
            groups: vec![],
            placement_recipes: Default::default(),
            constraints: vec![
                Constraint::DiffPair { p_net: dp_p, n_net: dp_n, spacing_mm: 0.15, length_match_mm: 0.1, source: src.clone() },
                Constraint::SignalClass { net: clk, class: "CLOCK".into(), max_freq_hz: None, source: src.clone() },
                Constraint::SignalClass { net: data, class: "DATA".into(), max_freq_hz: None, source: src.clone() },
            ],
        };

        let crit = net_criticality(&board);
        let w = |n: NetId| effective_weight(board.nets.iter().find(|x| x.id == n).unwrap(), &crit);

        // Diff pair (100) > clock (80) > data (30) > plain (0), all above base 1.0.
        assert!(w(dp_p) > w(clk), "{} vs {}", w(dp_p), w(clk));
        assert!(w(clk) > w(data));
        assert!(w(data) > w(plain));
        assert!((w(plain) - 1.0).abs() < 1e-9, "plain net keeps base weight");
        // Both legs of the pair score equally.
        assert!((w(dp_p) - w(dp_n)).abs() < 1e-9);
    }

    #[test]
    fn no_constraints_no_bonus() {
        let board = Board {
            config: BoardConfig::default(),
            layer_stack: crate::stackup::stackup_preset(StackupPreset::TwoLayer),
            components: vec![],
            nets: vec![],
            groups: vec![],
            placement_recipes: Default::default(),
            constraints: vec![],
        };
        assert!(net_criticality(&board).is_empty());
    }
}
