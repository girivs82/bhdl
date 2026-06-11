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

use bhdl_common::model::{EvaluatedModel, ModelRecipe, ModelRole};
use bhdl_netlist::Netlist;
use bhdl_netlist::types::NetClass;

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

/// Build the per-entity model overrides for a netlist: for every instantiated
/// entity that declares a `model { }` recipe, evaluate it and return
/// `entity_name → EvaluatedModel`. Inputs come from the entity's declared
/// attributes (`self.<param>`) and the output rail's declared current
/// (`i_out` = the `@ I` budget on the rail whose voltage matches `self.v_out`).
/// An entity whose recipe fails to evaluate (a referenced param the design
/// didn't supply) is omitted — the converter then uses its hardcoded
/// decomposition.
pub fn evaluate_model_overrides(
    netlist: &Netlist,
    model_recipes: &HashMap<String, ModelRecipe>,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> HashMap<String, EvaluatedModel> {
    let mut out: HashMap<String, EvaluatedModel> = HashMap::new();
    if model_recipes.is_empty() {
        return out;
    }
    let parse_si = |s: &str| bhdl_analyzer::value_snap::parse_value_string(s.trim());

    // Declared power rails as (voltage, current) for the i_out lookup.
    let rails: Vec<(f64, Option<f64>)> = netlist
        .nets
        .values()
        .filter_map(|net| match &net.net_class {
            NetClass::Power { voltage, current } if *voltage > 0.0 => Some((*voltage, *current)),
            _ => None,
        })
        .collect();

    for inst in netlist.instances.values() {
        let Some(module) = netlist.modules.get(inst.definition) else { continue };
        let entity = module.name.as_str();
        if out.contains_key(entity) {
            continue;
        }
        let Some(recipe) = model_recipes.get(entity) else { continue };

        let self_params: HashMap<String, f64> = entity_attrs
            .get(entity)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| parse_si(v).map(|n| (k.clone(), n)))
                    .collect()
            })
            .unwrap_or_default();

        // i_out = the declared load on the rail at the device's output voltage.
        let mut bare: HashMap<String, f64> = HashMap::new();
        if let Some(v_out) = self_params.get("v_out").copied() {
            if let Some((_, Some(i))) = rails
                .iter()
                .find(|(v, _)| (*v - v_out).abs() < 0.1 * v_out.max(1.0))
            {
                bare.insert("i_out".to_string(), *i);
            }
        }

        let inputs = ModelInputs { self_params, bare };
        if let Ok(m) = evaluate_model_recipe(recipe, &inputs) {
            out.insert(entity.to_string(), m);
        }
    }
    out
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
