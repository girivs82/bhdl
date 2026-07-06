//! Regulator input-draw fixpoint — make the DC solve conserve power
//! through regulators.
//!
//! The DC model supplies a regulator's output either through a decomposed
//! source branch or through the BOARD RAIL's own ideal source (when the
//! output net is a declared rail, double-drive prevention leaves exactly
//! one of them). Either way the INPUT rail never sees the load, so every
//! upstream current is fiction. This wrapper closes the loop: after each
//! solve, each regulator's input draw is computed from its SOLVED output
//! current
//!
//!     i_in = v_out · i_out / (η · v_in)
//!
//! (η from the part's datasheet efficiency; absent → 1.0, the linear-
//! regulator law i_in ≈ i_out) and upserted as a CurrentSource on the
//! input net, attributed to the regulator instance; the system re-solves
//! until the draws are stable (1%). Cascades converge in depth+1 passes.
//!
//! `i_out` is taken from the output source branch's solved current when
//! reported, else by KCL over every other branch on the output node — a
//! clamped source hides its own current but not its neighbours'.

use std::collections::HashMap;

use log::{debug, warn};

use crate::circuit::Circuit;
use crate::errors::Result;
use crate::glacier_dc_solver::{DcAnalysisResult, GlacierDcSolver};

/// Netlist-exact facts about one regulator, supplied by the caller (which
/// has the netlist): where its input and output pins actually sit, which
/// instance the synthesized draw belongs to, and its datasheet efficiency.
#[derive(Debug, Clone)]
pub struct RegulatorHint {
    pub vin_net: String,
    pub vout_net: String,
    pub instance: bhdl_netlist::InstanceId,
    /// Fraction 0..1; None → 1.0 (linear-regulator law).
    pub efficiency: Option<f64>,
}

/// Solve DC with regulator input draws closed to a fixpoint. Returns the
/// final result AND the final circuit (callers annotate against the
/// circuit that was actually solved — including the `_in_draw` branches).
pub fn solve_dc_with_input_draws(
    base: Circuit,
    hints: &HashMap<String, RegulatorHint>,
) -> Result<(DcAnalysisResult, Circuit)> {
    let solver = GlacierDcSolver::new();
    let mut circuit = base;
    let mut result = solver.solve(circuit.clone())?;
    if hints.is_empty() {
        return Ok((result, circuit));
    }

    let gnd_name = circuit
        .nodes()
        .find(|(_, n)| n.is_ground)
        .map(|(_, n)| n.name.clone());
    let Some(gnd_name) = gnd_name else {
        return Ok((result, circuit));
    };

    for pass in 0..5 {
        let updates = regulator_input_draws(&circuit, &result, hints);
        let mut changed = false;
        for (parent, i_in) in &updates {
            let hint = &hints[parent];
            let bname = format!("{parent}_in_draw");
            let existing = circuit
                .branches()
                .find(|(_, b)| b.name == bname)
                .map(|(_, b)| b.value);
            match existing {
                Some(v) if (v - i_in).abs() <= 0.01 * i_in.abs().max(1e-9) => {}
                Some(_) => {
                    circuit.set_branch_value(&bname, *i_in);
                    changed = true;
                }
                None if *i_in > 1e-6 => {
                    circuit.add_branch(
                        bname,
                        &hint.vin_net,
                        &gnd_name,
                        "CurrentSource".to_string(),
                        *i_in,
                        Some(hint.instance),
                    );
                    changed = true;
                }
                None => {}
            }
        }
        if !changed {
            debug!("input-draw fixpoint stable after {pass} pass(es)");
            break;
        }
        result = solver.solve(circuit.clone())?;
        if pass == 4 {
            warn!("input-draw fixpoint hit the pass cap — draws may lag the loads by one pass");
        }
    }
    Ok((result, circuit))
}

/// Per-regulator input draw from the solved operating point: (parent, i_in).
fn regulator_input_draws(
    circuit: &Circuit,
    result: &DcAnalysisResult,
    hints: &HashMap<String, RegulatorHint>,
) -> Vec<(String, f64)> {
    let node_by_name = |name: &str| {
        circuit
            .nodes()
            .find(|(_, n)| n.name == name)
            .map(|(idx, _)| idx)
    };
    let node_v = |idx: petgraph::graph::NodeIndex| -> f64 {
        result.node_voltages.get(&idx).copied().unwrap_or(0.0)
    };
    let mut out = Vec::new();
    for (parent, hint) in hints {
        let (Some(vin_node), Some(vout_node)) =
            (node_by_name(&hint.vin_net), node_by_name(&hint.vout_net))
        else {
            continue;
        };
        // Output current: the source branch's solved current when the
        // solver reports one, else KCL over every NON-source branch on the
        // output node. The source = any VoltageSource incident to vout
        // (decomposed regulator source or the board rail's own).
        let source_edge = circuit.branches().find_map(|(e, b)| {
            (b.component_type == "VoltageSource" && b.nodes.contains(&vout_node)).then_some(e)
        });
        let i_out = source_edge
            .and_then(|e| result.branch_currents.get(&e).copied())
            .map(f64::abs)
            .unwrap_or_else(|| {
                let mut sum = 0.0;
                for (e2, b2) in circuit.branches() {
                    if Some(e2) == source_edge || b2.nodes.len() != 2 {
                        continue;
                    }
                    let i2 = result.branch_currents.get(&e2).copied().unwrap_or_else(|| {
                        if b2.component_type == "CurrentSource" { b2.value } else { 0.0 }
                    });
                    if b2.nodes[0] == vout_node {
                        sum -= i2;
                    }
                    if b2.nodes[1] == vout_node {
                        sum += i2;
                    }
                }
                sum.abs()
            });
        if i_out < 1e-6 {
            continue;
        }
        let (v_in, v_out) = (node_v(vin_node), node_v(vout_node));
        if v_in.abs() < 0.5 {
            continue;
        }
        let eff = hint.efficiency.filter(|e| *e > 0.0 && *e <= 1.0).unwrap_or(1.0);
        let i_in = (v_out * i_out / (eff * v_in)).abs();
        debug!(
            "regulator {parent}: i_out={i_out:.4}A v_out={v_out:.2}V v_in={v_in:.2}V η={eff:.2} → i_in={i_in:.4}A on {}",
            hint.vin_net
        );
        out.push((parent.clone(), i_in));
    }
    out
}
