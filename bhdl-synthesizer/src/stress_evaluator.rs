//! Evaluator for entity `simulation { stress { } }` blocks
//! (docs/spec/Vendor_Simulation_Blocks.md §4), Stage 3 of the migration.
//!
//! Reuses the shared expression engine from [`crate::design_evaluator`]
//! (`evaluate_text` over the BHDL `Expr` grammar) via the [`EvalLookup`] trait
//! — only identifier resolution differs. A stress block resolves:
//!
//! - **operating point** — bare `vin`, `vout` (and any solved net voltage),
//! - **load** — bare `i_out` (the output rail's declared `@ I` budget),
//! - **self params** — `self.<param>` (the entity's constructor values),
//! - **children** — `<child>.value` (the snapped expansion-child value),
//! - **locals** — `const` bindings declared earlier in the block.
//!
//! It produces a map `(child_refdes, axis) → value` — the per-child stress
//! overrides the sign-off loop folds into the margin computation (Stage 4).

use std::collections::HashMap;

use bhdl_common::stress::{StressRecipe, StressStatement};

use crate::design_evaluator::{evaluate_text, DesignEvalError, EvalLookup};

/// Read-only inputs a stress block evaluates against. All maps are keyed by the
/// bare identifier the block uses (`vin`, `i_out`, `f_sw`, `L_out`, …).
pub struct StressInputs {
    /// Operating point — bare net names (`vin`, `vout`, …) and the load
    /// (`i_out`) → solved/declared value.
    pub operating_point: HashMap<String, f64>,
    /// Entity constructor values, reached as `self.<param>`.
    pub self_params: HashMap<String, f64>,
    /// Expansion children, reached as `<child>.value` → snapped value.
    pub child_values: HashMap<String, f64>,
}

/// Live evaluation context: the read-only inputs plus the `const` locals
/// accumulated as the block runs top-to-bottom.
struct StressContext<'a> {
    inputs: &'a StressInputs,
    locals: HashMap<String, f64>,
}

impl EvalLookup for StressContext<'_> {
    fn lookup(&self, name: &str) -> Result<f64, DesignEvalError> {
        let name = name.trim();
        if let Some((ns, field)) = name.split_once('.') {
            // `self.<param>` — the entity's own constructor values.
            if ns == "self" {
                return self.inputs.self_params.get(field).copied().ok_or_else(|| {
                    DesignEvalError::EvalError(format!(
                        "self.{field} is not a constructor parameter of this entity"
                    ))
                });
            }
            // `<child>.value` — the snapped value of an expansion child. Only
            // `.value` is exposed today (the input the ripple forms read).
            if field == "value" {
                return self.inputs.child_values.get(ns).copied().ok_or_else(|| {
                    DesignEvalError::EvalError(format!(
                        "{ns}.value: no expansion child named '{ns}' with a snapped value"
                    ))
                });
            }
            return Err(DesignEvalError::EvalError(format!(
                "unknown reference '{name}' in stress block \
                 (recognised: self.<param>, <child>.value, and bare \
                 operating-point names like vin/vout/i_out)"
            )));
        }
        // Bare name: a local `const` first, then the operating point / load.
        if let Some(v) = self.locals.get(name).copied() {
            return Ok(v);
        }
        if let Some(v) = self.inputs.operating_point.get(name).copied() {
            return Ok(v);
        }
        Err(DesignEvalError::EvalError(format!(
            "identifier '{name}' is not in scope in the stress block \
             (no local const, operating-point net, or load by that name)"
        )))
    }
}

/// Evaluate a stress recipe at the given operating point. Returns the per-child
/// stress overrides keyed by `(child_refdes, axis)` (e.g. `("L_out","i_peak")`).
///
/// A `require` that fails returns [`DesignEvalError::RequireFailed`] — the
/// caller treats that (and any eval error) as "the stress model does not apply
/// here", falling back to the generic DC stress (Real-Data/​graceful
/// degradation: a partial or unsatisfiable model never fabricates a number).
pub fn evaluate_stress_recipe(
    recipe: &StressRecipe,
    inputs: &StressInputs,
) -> Result<HashMap<(String, String), f64>, DesignEvalError> {
    let mut ctx = StressContext { inputs, locals: HashMap::new() };
    let mut out: HashMap<(String, String), f64> = HashMap::new();

    for stmt in &recipe.statements {
        match stmt {
            StressStatement::Let { name, expr } => {
                let v = evaluate_text(expr, &ctx)?;
                ctx.locals.insert(name.clone(), v);
            }
            StressStatement::Require { condition, message } => {
                let v = evaluate_text(condition, &ctx)?;
                if v == 0.0 {
                    return Err(DesignEvalError::RequireFailed(message.clone()));
                }
            }
            StressStatement::Assign { child_name, axis, expr } => {
                let v = evaluate_text(expr, &ctx)?;
                out.insert((child_name.clone(), axis.clone()), v);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buck_inputs() -> StressInputs {
        // V_in=12, V_out=5, I_out=2A, f_sw=500kHz, L=10µH, C_out=69µF.
        StressInputs {
            operating_point: HashMap::from([
                ("vin".to_string(), 12.0),
                ("vout".to_string(), 5.0),
                ("i_out".to_string(), 2.0),
            ]),
            self_params: HashMap::from([("f_sw".to_string(), 500_000.0)]),
            child_values: HashMap::from([
                ("L_out".to_string(), 10e-6),
                ("C_out".to_string(), 69e-6),
            ]),
        }
    }

    #[test]
    fn evaluates_buck_ripple_forms() {
        let mut recipe = StressRecipe::new("TPS54302".to_string());
        recipe.statements = vec![
            StressStatement::Let {
                name: "duty".to_string(),
                expr: "vout / vin".to_string(),
            },
            StressStatement::Let {
                name: "d_il".to_string(),
                expr: "(vin - vout) * duty / (self.f_sw * L_out.value)".to_string(),
            },
            StressStatement::Assign {
                child_name: "L_out".to_string(),
                axis: "i_peak".to_string(),
                expr: "i_out + d_il / 2".to_string(),
            },
            StressStatement::Assign {
                child_name: "C_out".to_string(),
                axis: "v_ripple".to_string(),
                expr: "d_il / (8 * self.f_sw * C_out.value)".to_string(),
            },
        ];

        let out = evaluate_stress_recipe(&recipe, &buck_inputs()).unwrap();

        // duty = 5/12 = 0.41667; d_il = 7*0.41667/(5e5*1e-5) = 0.58333A.
        let i_peak = out[&("L_out".to_string(), "i_peak".to_string())];
        assert!((i_peak - (2.0 + 0.58333 / 2.0)).abs() < 1e-3, "i_peak={i_peak}");

        // v_ripple = d_il / (8 * f_sw * C_out) = 0.58333 / (8*5e5*69e-6).
        let v_ripple = out[&("C_out".to_string(), "v_ripple".to_string())];
        assert!((v_ripple - 0.58333 / (8.0 * 5e5 * 69e-6)).abs() < 1e-6, "v_ripple={v_ripple}");
    }

    #[test]
    fn require_failure_is_reported() {
        let mut recipe = StressRecipe::new("Buck".to_string());
        recipe.statements = vec![
            StressStatement::Require {
                condition: "self.f_sw > 1000000".to_string(),
                message: "f_sw below model range".to_string(),
            },
            StressStatement::Assign {
                child_name: "L".to_string(),
                axis: "i_peak".to_string(),
                expr: "i_out".to_string(),
            },
        ];
        let err = evaluate_stress_recipe(&recipe, &buck_inputs()).unwrap_err();
        assert!(matches!(err, DesignEvalError::RequireFailed(m) if m.contains("f_sw")));
    }

    #[test]
    fn unknown_child_errors() {
        let mut recipe = StressRecipe::new("Buck".to_string());
        recipe.statements = vec![StressStatement::Assign {
            child_name: "X".to_string(),
            axis: "i_peak".to_string(),
            expr: "Nonexistent.value".to_string(),
        }];
        assert!(evaluate_stress_recipe(&recipe, &buck_inputs()).is_err());
    }
}
