//! Reliability engine (docs/spec/Functional_Safety.md §2.8): computes a
//! handbook part's base FIT from a *named prediction standard's*
//! equations, the board's declared mission profile, and the part's
//! **sim-derived** stress ratio (applied / rated, from the same GLACIER
//! DC solve the sign-off table uses).
//!
//! Real-Data rule: this module carries NO numbers. The per-class
//! coefficients live in a table file (`bhdl-stdlib/safety/<std>.toml`),
//! each class row named and sourced; a missing table, class, mission
//! profile or stress ratio ⇒ the FIT stays uncomputed and the model
//! gets a FIT_UNCOMPUTED gap — never a guessed number.
//!
//! Equation shape (IEC 62380 family, shared by IEC 61709 / SN 29500
//! conversion models):
//!
//!   λ = λ_base · π_T · π_S,   π_T = exp(Ea/k · (1/T_ref − 1/T_amb)),
//!                             π_S = (S / S_ref)^n   (S = applied/rated)
//!
//! with temperatures in kelvin. Each class row supplies λ_base (FIT at
//! reference conditions), Ea (eV), T_ref (°C), S_ref and n.

use std::collections::{BTreeMap, HashMap};

use bhdl_common::safety::{Gap, GapClass, Mission, PartData, SafetyModel};

/// Boltzmann constant in eV/K.
const K_EV: f64 = 8.617_333e-5;

/// One class row of a prediction-standard coefficient table.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ClassCoeffs {
    /// Base failure rate in FIT at reference conditions.
    pub lambda_base: f64,
    /// Activation energy in eV (temperature acceleration).
    pub ea_ev: f64,
    /// Reference ambient in °C at which lambda_base holds.
    pub t_ref_c: f64,
    /// Reference stress ratio at which lambda_base holds.
    pub s_ref: f64,
    /// Stress exponent.
    pub stress_exp: f64,
    /// Per-row provenance — printed verbatim in the report.
    pub source: String,
}

/// A parsed coefficient table for one prediction standard.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReliabilityTable {
    /// Standard name the table implements (must match `per=` on the part).
    pub standard: String,
    /// Table-wide provenance note (e.g. edition, or FIXTURE marker).
    pub source: String,
    /// Class name (e.g. "res_film") → coefficients.
    pub classes: BTreeMap<String, ClassCoeffs>,
}

impl ReliabilityTable {
    pub fn from_toml(text: &str) -> Result<ReliabilityTable, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// λ in FIT for a class at (stress ratio, ambient °C), with the
    /// human-readable basis string. `None` if the class is not in the table.
    pub fn fit_for(&self, class: &str, stress_ratio: f64, ambient_c: f64) -> Option<(f64, String)> {
        let c = self.classes.get(class)?;
        let t_amb = ambient_c + 273.15;
        let t_ref = c.t_ref_c + 273.15;
        let pi_t = (c.ea_ev / K_EV * (1.0 / t_ref - 1.0 / t_amb)).exp();
        // Clamp the stress ratio away from zero: an unloaded part is not
        // more reliable than the standard's floor, and S=0 would zero the
        // whole product for n>0.
        let s = stress_ratio.max(0.01);
        let pi_s = (s / c.s_ref).powf(c.stress_exp);
        let fit = c.lambda_base * pi_t * pi_s;
        let basis = format!(
            "λ={:.2} FIT = {:.2}·π_T({:.2})·π_S({:.2}) @ S={:.2}, Ta={:.0}°C per {} [{}]",
            fit, c.lambda_base, pi_t, pi_s, stress_ratio, ambient_c, self.standard, c.source
        );
        Some((fit, basis))
    }
}

/// Per-instance sim-derived stress: (applied, rated) in the class's
/// stress axis units (W for resistors, V for capacitors) — straight from
/// the sign-off rows, never estimated here.
pub type StressMap = HashMap<String, (f64, f64)>;

/// Fill in computed FITs on every handbook part that names a prediction
/// standard. Adds a FIT_UNCOMPUTED gap for each such part whose FIT
/// could not be computed, saying exactly which ingredient is missing.
pub fn apply_reliability(
    model: &mut SafetyModel,
    stress: &StressMap,
    tables: &HashMap<String, ReliabilityTable>,
) {
    let mission: Option<Mission> = model.mission.clone();
    let mut gaps: Vec<Gap> = Vec::new();
    for part in &mut model.parts {
        let PartData::Handbook { class, per, fit, fit_basis, .. } = &mut part.data else { continue };
        let Some(std_name) = per.clone() else { continue };
        let mut missing: Vec<String> = Vec::new();
        let table = tables.get(&std_name);
        if table.is_none() {
            missing.push(format!("no coefficient table for '{}'", std_name));
        }
        let m = mission.as_ref();
        if m.is_none() {
            missing.push("no mission { ambient = … } in the board safety block".to_string());
        }
        let st = stress.get(&part.instance);
        if st.is_none() {
            missing.push("no sim-derived stress (DC solve did not cover this instance)".to_string());
        }
        if let (Some(table), Some(m), Some((applied, rated))) = (table, m, st) {
            if *rated > 0.0 {
                match table.fit_for(class, applied / rated, m.ambient_c) {
                    Some((f, basis)) => {
                        *fit = Some(f);
                        *fit_basis = Some(basis);
                        continue;
                    }
                    None => missing.push(format!("class '{}' not in the {} table", class, std_name)),
                }
            } else {
                missing.push("part has no rating (stress ratio undefined)".to_string());
            }
        }
        gaps.push(Gap {
            class: GapClass::FitUncomputed,
            goal: String::new(),
            subject: part.instance.clone(),
            fix: missing.join("; "),
        });
    }
    model.gaps.extend(gaps);
    model.gaps.sort_by(|a, b| (a.class as u8, &a.subject).cmp(&(b.class as u8, &b.subject)));
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = r#"
standard = "IEC62380"
source = "unit-test fixture"

[classes.res_film]
lambda_base = 0.5
ea_ev = 0.15
t_ref_c = 40.0
s_ref = 0.5
stress_exp = 1.0
source = "test row"
"#;

    #[test]
    fn apply_reliability_fills_fits_and_gaps_honestly() {
        use bhdl_common::safety::{Mission, Part, SafetyModel};
        let t = ReliabilityTable::from_toml(TABLE).unwrap();
        let tables: HashMap<String, ReliabilityTable> = [("IEC62380".to_string(), t)].into();
        let hb = |per: Option<&str>| PartData::Handbook {
            class: "res_film".into(), source: "test".into(),
            per: per.map(|p| p.to_string()), fit: None, fit_basis: None,
        };
        let mut model = SafetyModel {
            board: "B".into(),
            mission: Some(Mission { ambient_c: 40.0, on_hours: None, cycles: None }),
            scopes: vec![], gaps: vec![], errors: vec![],
            parts: vec![
                Part { instance: "r1".into(), type_name: "Res".into(), parent: None, data: hb(Some("IEC62380")) },
                Part { instance: "r2".into(), type_name: "Res".into(), parent: None, data: hb(Some("IEC62380")) }, // no stress
                Part { instance: "r3".into(), type_name: "Res".into(), parent: None, data: hb(None) },             // no standard: untouched
            ],
        };
        let stress: StressMap = [("r1".to_string(), (0.25, 0.5))].into();
        apply_reliability(&mut model, &stress, &tables);
        match &model.parts[0].data {
            PartData::Handbook { fit: Some(f), fit_basis: Some(_), .. } => assert!((f - 0.5).abs() < 1e-9, "S=0.5 at Tref → lambda_base, got {f}"),
            other => panic!("r1 should have a computed FIT: {other:?}"),
        }
        assert!(matches!(&model.parts[1].data, PartData::Handbook { fit: None, .. }));
        let fit_gaps: Vec<_> = model.gaps.iter().filter(|g| g.class == GapClass::FitUncomputed).collect();
        assert_eq!(fit_gaps.len(), 1, "only the standard-naming part without stress gaps");
        assert_eq!(fit_gaps[0].subject, "r2");
        assert!(fit_gaps[0].fix.contains("no sim-derived stress"));
    }

    #[test]
    fn fit_scales_with_temperature_and_stress() {
        let t = ReliabilityTable::from_toml(TABLE).unwrap();
        let (base, _) = t.fit_for("res_film", 0.5, 40.0).unwrap();
        assert!((base - 0.5).abs() < 1e-9, "reference conditions return lambda_base, got {base}");
        let (hot, _) = t.fit_for("res_film", 0.5, 85.0).unwrap();
        assert!(hot > base, "hotter must not be more reliable");
        let (stressed, _) = t.fit_for("res_film", 1.0, 40.0).unwrap();
        assert!((stressed - 1.0).abs() < 1e-9, "linear stress exponent doubles at S=1.0, got {stressed}");
        assert!(t.fit_for("cap_ceramic", 0.5, 40.0).is_none(), "unknown class must not invent a number");
    }
}
