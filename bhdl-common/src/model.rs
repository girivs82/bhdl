//! Device **model** recipe — vendor-authored `simulation { model { } }` blocks
//! (docs/spec/Vendor_Simulation_Blocks.md §5).
//!
//! Where a [`crate::stress::StressRecipe`] says how a device *stresses its
//! support parts*, a model recipe says how the device itself *stamps into the
//! DC solve* — the thing currently hardcoded in `bhdl-spice`'s
//! `netlist_converter` (a regulator as a controlled `VOUT` source plus an
//! efficiency-scaled `VIN` current draw). This module captures the
//! **primitive-composition** form (§5.1 form 3): per-node `source`/`draws`
//! expressions. The richer `builtin <model>` (form 2) and `vendor spice …`
//! (form 1) surfaces are not represented here yet.
//!
//! Expressions are kept as raw source text and parsed by the evaluator, exactly
//! like design/stress recipes.

/// Which kind of branch a `node` statement contributes to the solve.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelRole {
    /// `node N source = <expr>;` — a controlled voltage source N → GND.
    Source,
    /// `node N draws = <expr>;` — a current the node sinks (current source to GND).
    Draws,
}

/// One `node <net> <role> = <expr>;` statement.
#[derive(Debug, Clone)]
pub struct ModelNode {
    /// The entity pin / net the branch attaches to (e.g. `"VOUT"`, `"VIN"`).
    pub net: String,
    pub role: ModelRole,
    /// Raw expression source, evaluated against the device's params + operating
    /// inputs (e.g. `self.v_out`, `i_out * self.v_out / (self.v_in * efficiency)`).
    pub expr: String,
}

/// The device-model recipe for one entity: the set of branches its
/// `model { }` block contributes (primitive-composition form).
#[derive(Debug, Clone)]
pub struct ModelRecipe {
    pub entity_name: String,
    pub nodes: Vec<ModelNode>,
}

/// The evaluated branches a model recipe contributes at circuit-build time.
/// Plain data (lives here, not in the evaluator's crate) so the SPICE
/// converter can consume it without depending on the synthesizer.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct EvaluatedModel {
    /// net → controlled-source voltage (`node N source = …`).
    pub sources: std::collections::HashMap<String, f64>,
    /// net → current draw (`node N draws = …`).
    pub draws: std::collections::HashMap<String, f64>,
}

impl ModelRecipe {
    pub fn new(entity_name: String) -> Self {
        Self { entity_name, nodes: Vec::new() }
    }

    pub fn has_nodes(&self) -> bool {
        !self.nodes.is_empty()
    }

    /// The `source` expression for a net, if declared.
    pub fn source_for(&self, net: &str) -> Option<&str> {
        self.nodes.iter()
            .find(|n| n.role == ModelRole::Source && n.net == net)
            .map(|n| n.expr.as_str())
    }

    /// The `draws` expression for a net, if declared.
    pub fn draws_for(&self, net: &str) -> Option<&str> {
        self.nodes.iter()
            .find(|n| n.role == ModelRole::Draws && n.net == net)
            .map(|n| n.expr.as_str())
    }
}
