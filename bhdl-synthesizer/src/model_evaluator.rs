//! Evaluator for entity `simulation { model { } }` blocks
//! (docs/spec/Vendor_Simulation_Blocks.md §5), Stage 2 of the model migration.
//!
//! Like [`crate::stress_evaluator`] it reuses the shared expression engine via
//! [`EvalLookup`], but a model block is evaluated at *circuit-build* time
//! (before the DC solve), so its inputs are the device's own description, not a
//! solved operating point:
//!
//! - **self params** — `self.<param>` (the entity's constructor / attribute
//!   values, e.g. `self.v_out`, `self.v_in`),
//! - **load** — `i_out` (the output rail's declared `@ I` budget),
//! - **efficiency** and any other bare datasheet name the block references.
//!
//! It produces, per `node` statement, a number: a `source` net's voltage and a
//! `draws` net's current — the branches `netlist_converter` stamps in place of
//! its hardcoded regulator decomposition.

use std::collections::HashMap;

use bhdl_common::model::{ModelRecipe, ModelRole};

use crate::design_evaluator::{evaluate_text, DesignEvalError, EvalLookup};

/// Read-only inputs a model block evaluates against.
pub struct ModelInputs {
    /// `self.<param>` ← the entity's declared constructor values / attributes.
    pub self_params: HashMap<String, f64>,
    /// Bare datasheet names the block reads directly (`i_out`, `efficiency`, …).
    pub bare: HashMap<String, f64>,
}

struct ModelContext<'a> {
    inputs: &'a ModelInputs,
}

impl EvalLookup for ModelContext<'_> {
    fn lookup(&self, name: &str) -> Result<f64, DesignEvalError> {
        let name = name.trim();
        if let Some((ns, field)) = name.split_once('.') {
            if ns == "self" {
                return self.inputs.self_params.get(field).copied().ok_or_else(|| {
                    DesignEvalError::EvalError(format!(
                        "self.{field} is not a parameter of this entity"
                    ))
                });
            }
            return Err(DesignEvalError::EvalError(format!(
                "unknown reference '{name}' in model block (recognised: self.<param> \
                 and bare datasheet names like i_out / efficiency)"
            )));
        }
        // Bare name: an explicit input first, then fall back to a self param of
        // the same name (so `efficiency` resolves whether read bare or declared).
        if let Some(v) = self.inputs.bare.get(name).copied() {
            return Ok(v);
        }
        if let Some(v) = self.inputs.self_params.get(name).copied() {
            return Ok(v);
        }
        Err(DesignEvalError::EvalError(format!(
            "identifier '{name}' is not in scope in the model block"
        )))
    }
}

/// The evaluated branches a model recipe contributes.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct EvaluatedModel {
    /// net → controlled-source voltage (`node N source = …`).
    pub sources: HashMap<String, f64>,
    /// net → current draw (`node N draws = …`).
    pub draws: HashMap<String, f64>,
}

/// Evaluate a model recipe's `node source/draws` expressions. Any eval error
/// (e.g. a referenced param the design didn't supply) propagates — the caller
/// then falls back to the hardcoded device decomposition rather than stamping a
/// partially-evaluated model.
pub fn evaluate_model_recipe(
    recipe: &ModelRecipe,
    inputs: &ModelInputs,
) -> Result<EvaluatedModel, DesignEvalError> {
    let ctx = ModelContext { inputs };
    let mut out = EvaluatedModel::default();
    for node in &recipe.nodes {
        let v = evaluate_text(&node.expr, &ctx)?;
        match node.role {
            ModelRole::Source => { out.sources.insert(node.net.clone(), v); }
            ModelRole::Draws => { out.draws.insert(node.net.clone(), v); }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_common::model::{ModelNode, ModelRole};

    fn buck_recipe() -> ModelRecipe {
        let mut r = ModelRecipe::new("DemoBuck".to_string());
        r.nodes = vec![
            ModelNode { net: "VOUT".into(), role: ModelRole::Source, expr: "self.v_out".into() },
            ModelNode {
                net: "VIN".into(),
                role: ModelRole::Draws,
                expr: "i_out * self.v_out / (self.v_in * efficiency)".into(),
            },
        ];
        r
    }

    fn buck_inputs() -> ModelInputs {
        ModelInputs {
            self_params: HashMap::from([
                ("v_out".to_string(), 5.0),
                ("v_in".to_string(), 12.0),
            ]),
            bare: HashMap::from([
                ("i_out".to_string(), 2.0),
                ("efficiency".to_string(), 0.9),
            ]),
        }
    }

    #[test]
    fn evaluates_source_and_draw() {
        let m = evaluate_model_recipe(&buck_recipe(), &buck_inputs()).unwrap();
        assert_eq!(m.sources["VOUT"], 5.0);
        // I_in = i_out·V_out / (V_in·η) = 2·5 / (12·0.9) = 10/10.8 = 0.9259A.
        let i_in = m.draws["VIN"];
        assert!((i_in - 10.0 / 10.8).abs() < 1e-9, "i_in={i_in}");
    }

    #[test]
    fn missing_param_errors() {
        let inputs = ModelInputs {
            self_params: HashMap::from([("v_out".to_string(), 5.0)]), // no v_in
            bare: HashMap::from([("i_out".to_string(), 2.0), ("efficiency".to_string(), 0.9)]),
        };
        assert!(evaluate_model_recipe(&buck_recipe(), &inputs).is_err());
    }
}
