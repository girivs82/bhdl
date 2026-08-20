//! Reliability engine (docs/spec/Functional_Safety.md §2.8): computes a
//! handbook part's FIT from a *named prediction standard's* equations,
//! the board's declared mission profile, and the part's **sim-derived**
//! stress ratio (applied / rated, from the same GLACIER DC solve the
//! sign-off table uses).
//!
//! Real-Data rule: this module carries NO numbers. The per-class
//! coefficients live in a table file (resolved via `$BHDL_SAFETY_TABLES`,
//! `<std>.local.toml`, then the in-repo `<std>.toml`), each class row
//! named and sourced; a missing table, class, mission profile or stress
//! ratio ⇒ the FIT stays uncomputed and the model gets a FIT_UNCOMPUTED
//! gap — never a guessed number.
//!
//! Two model forms are implemented:
//!
//! `model = "arrhenius_stress"` — the generic λ_base·π_T·π_S shape
//! shared by the IEC 62380 / IEC 61709 / SN 29500 family:
//!   λ = λ_base · exp(Ea/k · (1/T_ref − 1/T_amb)) · (S/S_ref)^n   [FIT]
//!
//! `model = "mil217f_resistor"` — MIL-HDBK-217F §9 resistor part-stress
//! (public domain, US government work — the one standard whose real
//! constants ship in-repo):
//!   λ_p  = λ_b · π_R · π_Q · π_E   failures/10⁶ h;   FIT = 1000·λ_p
//!   λ_b  = a · exp(b·((T+273)/nt)^p) · exp((S/g)·((T+273)/ns))
//! π_R from the resistance range, π_Q from the quality level, π_E from
//! the mission environment. The generic λ_b form covers §9.1
//! (composition: a=4.5e-9, b=12, nt=343, p=1, g=0.6, ns=273), §9.2
//! RL/RLR (a=3.25e-4, b=1, nt=343, p=3, g=1, ns=273) and §9.2 RN
//! (a=5e-5, b=3.5, nt=398, p=1, g=1, ns=273) — each transcription is
//! unit-tested against the handbook's own printed λ_b tables.

use std::collections::{BTreeMap, HashMap};

use bhdl_common::safety::{Gap, GapClass, Mission, PartData, SafetyModel};

/// Boltzmann constant in eV/K.
const K_EV: f64 = 8.617_333e-5;

/// One class row of a prediction-standard coefficient table.
#[derive(Debug, Clone, serde::Deserialize)]
#[serde(tag = "model")]
pub enum ClassModel {
    /// Generic Arrhenius + power-law stress (IEC 62380 family shape).
    #[serde(rename = "arrhenius_stress")]
    ArrheniusStress {
        /// Base failure rate in FIT at reference conditions.
        lambda_base: f64,
        /// Activation energy in eV.
        ea_ev: f64,
        /// Reference ambient in °C at which lambda_base holds.
        t_ref_c: f64,
        /// Reference stress ratio at which lambda_base holds.
        s_ref: f64,
        /// Stress exponent.
        stress_exp: f64,
        /// Per-row provenance — printed verbatim in the report.
        source: String,
    },
    /// MIL-HDBK-217F resistor part-stress model (λ_p in failures/10⁶ h).
    #[serde(rename = "mil217f_resistor")]
    Mil217fResistor {
        a: f64,
        b: f64,
        nt: f64,
        p: f64,
        g: f64,
        ns: f64,
        /// Resistance-range multiplier: sorted [max_ohms, pi_r] rows;
        /// the first row whose max_ohms exceeds the part's resistance
        /// applies. A final huge max_ohms catches ">10M".
        pi_r: Vec<(f64, f64)>,
        /// Quality level → π_Q.
        pi_q: BTreeMap<String, f64>,
        /// Environment symbol (GB, GF, GM, …) → π_E.
        pi_e: BTreeMap<String, f64>,
        source: String,
    },
}

impl ClassModel {
    pub fn source(&self) -> &str {
        match self {
            ClassModel::ArrheniusStress { source, .. } => source,
            ClassModel::Mil217fResistor { source, .. } => source,
        }
    }
}

/// A parsed coefficient table for one prediction standard.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReliabilityTable {
    /// Standard name the table implements (must match `per=` on the part).
    pub standard: String,
    /// Table-wide provenance note.
    pub source: String,
    /// Class name (e.g. "res_fixed_film", MIL-HDBK-217F §9.2) → model.
    pub classes: BTreeMap<String, ClassModel>,
}

/// Everything the engine may evaluate against for one instance. Stress
/// and resistance are sim-/netlist-derived; quality and environment come
/// from the mission profile (with explicit, printed defaults).
#[derive(Debug, Clone)]
pub struct FitInputs {
    /// S = applied / rated (sim-derived, never estimated).
    pub stress_ratio: f64,
    /// Mission ambient, °C.
    pub ambient_c: f64,
    /// The part's resistance in ohms (netlist attribute), for π_R.
    pub resistance_ohm: Option<f64>,
    /// Quality level key into π_Q. `None` ⇒ "lower" (COTS — the
    /// handbook's own category for non-established-reliability parts).
    pub quality: Option<String>,
    /// Environment symbol key into π_E. `None` ⇒ "GB" (ground benign).
    pub environment: Option<String>,
}

impl ReliabilityTable {
    pub fn from_toml(text: &str) -> Result<ReliabilityTable, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }

    /// λ in FIT for a class under `inp`, with the human-readable basis
    /// string. `Err` explains exactly which ingredient is missing or
    /// unmapped; an unknown class is an `Err` too.
    pub fn fit_for(&self, class: &str, inp: &FitInputs) -> Result<(f64, String), String> {
        let Some(c) = self.classes.get(class) else {
            return Err(format!("class '{}' not in the {} table", class, self.standard));
        };
        match c {
            ClassModel::ArrheniusStress { lambda_base, ea_ev, t_ref_c, s_ref, stress_exp, source } => {
                let t_amb = inp.ambient_c + 273.15;
                let t_ref = t_ref_c + 273.15;
                let pi_t = (ea_ev / K_EV * (1.0 / t_ref - 1.0 / t_amb)).exp();
                // Clamp the stress ratio away from zero: an unloaded part
                // is not more reliable than the standard's floor, and S=0
                // would zero the whole product for n>0.
                let s = inp.stress_ratio.max(0.01);
                let pi_s = (s / s_ref).powf(*stress_exp);
                let fit = lambda_base * pi_t * pi_s;
                Ok((fit, format!(
                    "λ={:.2} FIT = {:.2}·π_T({:.2})·π_S({:.2}) @ S={:.2}, Ta={:.0}°C per {} [{}]",
                    fit, lambda_base, pi_t, pi_s, inp.stress_ratio, inp.ambient_c, self.standard, source
                )))
            }
            ClassModel::Mil217fResistor { a, b, nt, p, g, ns, pi_r, pi_q, pi_e, source } => {
                let t = inp.ambient_c;
                let s = inp.stress_ratio.max(0.01);
                let lambda_b = a
                    * (b * ((t + 273.0) / nt).powf(*p)).exp()
                    * ((s / g) * ((t + 273.0) / ns)).exp();
                let (r_ohm, pr) = match inp.resistance_ohm {
                    Some(r) => match pi_r.iter().find(|(max, _)| r < *max) {
                        Some((_, f)) => (r, *f),
                        None => return Err(format!("resistance {r}Ω above every π_R range")),
                    },
                    None => return Err("no resistance attribute on the instance (π_R needs it)".to_string()),
                };
                let q_key = inp.quality.clone().unwrap_or_else(|| "lower".to_string());
                let Some(pq) = pi_q.get(&q_key) else {
                    return Err(format!("quality '{}' not in π_Q ({})", q_key, pi_q.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                let e_key = inp.environment.clone().unwrap_or_else(|| "GB".to_string());
                let Some(pe) = pi_e.get(&e_key) else {
                    return Err(format!("environment '{}' not in π_E ({})", e_key, pi_e.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                // λ_p is failures/10⁶ h; FIT is failures/10⁹ h.
                let fit = 1000.0 * lambda_b * pr * pq * pe;
                Ok((fit, format!(
                    "λ={:.1} FIT = 1000·λb({:.5})·π_R({:.1}@{:.0}Ω)·π_Q({} {})·π_E({} {}) @ S={:.2}, Ta={:.0}°C per {} [{}]",
                    fit, lambda_b, pr, r_ohm, pq, q_key, pe, e_key, inp.stress_ratio, inp.ambient_c, self.standard, source
                )))
            }
        }
    }
}

/// Per-instance sim-derived inputs: applied and rated stress in the
/// class's axis units (W for resistors), plus the resistance from the
/// netlist attribute — straight from sign-off rows and the netlist,
/// never estimated here.
#[derive(Debug, Clone, Copy, Default)]
pub struct InstanceStress {
    pub applied: f64,
    pub rated: f64,
    pub resistance_ohm: Option<f64>,
}

pub type StressMap = HashMap<String, InstanceStress>;

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
        if let (Some(table), Some(m), Some(st)) = (table, m, st) {
            if st.rated > 0.0 {
                let inp = FitInputs {
                    stress_ratio: st.applied / st.rated,
                    ambient_c: m.ambient_c,
                    resistance_ohm: st.resistance_ohm,
                    quality: m.quality.clone(),
                    environment: m.environment.clone(),
                };
                match table.fit_for(class, &inp) {
                    Ok((f, basis)) => {
                        *fit = Some(f);
                        *fit_basis = Some(basis);
                        continue;
                    }
                    Err(e) => missing.push(e),
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

    const ARR_TABLE: &str = r#"
standard = "IEC62380"
source = "unit-test fixture"

[classes.res_film_low_dissipation]
model = "arrhenius_stress"
lambda_base = 0.5
ea_ev = 0.15
t_ref_c = 40.0
s_ref = 0.5
stress_exp = 1.0
source = "test row"
"#;

    /// The real §9.1/§9.2 rows, same data as shipped in milhdbk217f.toml.
    const M217_TABLE: &str = r#"
standard = "MILHDBK217F"
source = "MIL-HDBK-217F Notice 2 (28 Feb 1995), Section 9"

[classes.res_fixed_film]
model = "mil217f_resistor"
a = 3.25e-4
b = 1.0
nt = 343.0
p = 3.0
g = 1.0
ns = 273.0
pi_r = [[1e5, 1.0], [1e6, 1.1], [1e7, 1.6], [1e30, 2.5]]
pi_q = { S = 0.03, R = 0.1, P = 0.3, M = 1.0, mil_spec = 5.0, lower = 15.0 }
pi_e = { GB = 1.0, GF = 2.0, GM = 8.0, NS = 4.0, NU = 14.0, AIC = 4.0, AIF = 8.0, AUC = 10.0, AUF = 18.0, ARW = 19.0, SF = 0.2, MF = 10.0, ML = 28.0, CL = 510.0 }
source = "MIL-HDBK-217F §9.2 (RL/RLR), p.9-3/9-4"

[classes.res_fixed_film_rn]
model = "mil217f_resistor"
a = 5.0e-5
b = 3.5
nt = 398.0
p = 1.0
g = 1.0
ns = 273.0
pi_r = [[1e5, 1.0], [1e6, 1.1], [1e7, 1.6], [1e30, 2.5]]
pi_q = { S = 0.03, R = 0.1, P = 0.3, M = 1.0, mil_spec = 5.0, lower = 15.0 }
pi_e = { GB = 1.0, GF = 2.0, GM = 8.0 }
source = "MIL-HDBK-217F §9.2 (RN), p.9-3/9-4"

[classes.res_fixed_composition]
model = "mil217f_resistor"
a = 4.5e-9
b = 12.0
nt = 343.0
p = 1.0
g = 0.6
ns = 273.0
pi_r = [[1e5, 1.0], [1e6, 1.1], [1e7, 1.6], [1e30, 2.5]]
pi_q = { S = 0.03, R = 0.1, P = 0.3, M = 1.0, mil_spec = 5.0, lower = 15.0 }
pi_e = { GB = 1.0, GF = 3.0, GM = 8.0 }
source = "MIL-HDBK-217F §9.1 (RC/RCR), p.9-2"
"#;

    fn inp(s: f64, t: f64) -> FitInputs {
        FitInputs { stress_ratio: s, ambient_c: t, resistance_ohm: Some(1000.0), quality: Some("M".into()), environment: Some("GB".into()) }
    }

    /// λ_b must reproduce the handbook's own printed base-failure-rate
    /// tables (§9.1/§9.2) — the transcription validates itself.
    #[test]
    fn mil217f_lambda_b_matches_the_handbooks_printed_tables() {
        let t = ReliabilityTable::from_toml(M217_TABLE).unwrap();
        // fit = 1000·λ_b·π_R(1.0)·π_Q(M=1.0)·π_E(GB=1.0) ⇒ λ_b = fit/1000
        let lb = |class: &str, s: f64, temp: f64| t.fit_for(class, &inp(s, temp)).unwrap().0 / 1000.0;
        let cases: &[(&str, f64, f64, f64)] = &[
            // (class, S, T, handbook λ_b)
            ("res_fixed_film", 0.1, 0.0, 0.00059),
            ("res_fixed_film", 0.1, 20.0, 0.00067),
            ("res_fixed_film", 0.5, 80.0, 0.0018),
            ("res_fixed_film", 0.1, 140.0, 0.0022),
            ("res_fixed_film_rn", 0.1, 20.0, 0.00073),
            ("res_fixed_film_rn", 0.9, 80.0, 0.0036),
            ("res_fixed_film_rn", 0.1, 170.0, 0.0029),
            ("res_fixed_composition", 0.1, 20.0, 0.00015),
            ("res_fixed_composition", 0.5, 40.0, 0.00067),
            ("res_fixed_composition", 0.1, 120.0, 0.0054),
        ];
        for (class, s, temp, expect) in cases {
            let got = lb(class, *s, *temp);
            let rel = (got - expect).abs() / expect;
            assert!(rel < 0.05, "{class} S={s} T={temp}: computed λ_b {got:.5} vs handbook {expect} (rel {rel:.3})");
        }
    }

    #[test]
    fn mil217f_multipliers_and_defaults() {
        let t = ReliabilityTable::from_toml(M217_TABLE).unwrap();
        // π_R kicks in above 0.1MΩ
        let mut i = inp(0.1, 20.0);
        let base = t.fit_for("res_fixed_film", &i).unwrap().0;
        i.resistance_ohm = Some(5e6);
        let hi_r = t.fit_for("res_fixed_film", &i).unwrap().0;
        assert!((hi_r / base - 1.6).abs() < 1e-9, "5MΩ ⇒ π_R=1.6");
        // defaults: no quality ⇒ lower (15×), no environment ⇒ GB (1×)
        let mut d = inp(0.1, 20.0);
        d.quality = None;
        d.environment = None;
        let cots = t.fit_for("res_fixed_film", &d).unwrap();
        assert!((cots.0 / base - 15.0).abs() < 1e-6, "COTS default = π_Q lower = 15");
        assert!(cots.1.contains("lower") && cots.1.contains("GB"));
        // honest errors, not guesses
        let mut e = inp(0.1, 20.0);
        e.resistance_ohm = None;
        assert!(t.fit_for("res_fixed_film", &e).unwrap_err().contains("resistance"));
        let mut q = inp(0.1, 20.0);
        q.quality = Some("bogus".into());
        assert!(t.fit_for("res_fixed_film", &q).unwrap_err().contains("bogus"));
    }

    #[test]
    fn apply_reliability_fills_fits_and_gaps_honestly() {
        use bhdl_common::safety::{Mission, Part, SafetyModel};
        let t = ReliabilityTable::from_toml(ARR_TABLE).unwrap();
        let tables: HashMap<String, ReliabilityTable> = [("IEC62380".to_string(), t)].into();
        let hb = |per: Option<&str>| PartData::Handbook {
            class: "res_film_low_dissipation".into(), source: "test".into(),
            per: per.map(|p| p.to_string()), fit: None, fit_basis: None,
        };
        let mut model = SafetyModel {
            board: "B".into(),
            mission: Some(Mission { ambient_c: 40.0, on_hours: None, cycles: None, environment: None, quality: None }),
            scopes: vec![], gaps: vec![], errors: vec![],
            parts: vec![
                Part { instance: "r1".into(), type_name: "Res".into(), parent: None, data: hb(Some("IEC62380")) },
                Part { instance: "r2".into(), type_name: "Res".into(), parent: None, data: hb(Some("IEC62380")) }, // no stress
                Part { instance: "r3".into(), type_name: "Res".into(), parent: None, data: hb(None) },             // no standard: untouched
            ],
        };
        let stress: StressMap = [("r1".to_string(), InstanceStress { applied: 0.25, rated: 0.5, resistance_ohm: Some(1e3) })].into();
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
        let t = ReliabilityTable::from_toml(ARR_TABLE).unwrap();
        let base_inp = |s: f64, temp: f64| FitInputs { stress_ratio: s, ambient_c: temp, resistance_ohm: None, quality: None, environment: None };
        let (base, _) = t.fit_for("res_film_low_dissipation", &base_inp(0.5, 40.0)).unwrap();
        assert!((base - 0.5).abs() < 1e-9, "reference conditions return lambda_base, got {base}");
        let (hot, _) = t.fit_for("res_film_low_dissipation", &base_inp(0.5, 85.0)).unwrap();
        assert!(hot > base, "hotter must not be more reliable");
        let (stressed, _) = t.fit_for("res_film_low_dissipation", &base_inp(1.0, 40.0)).unwrap();
        assert!((stressed - 1.0).abs() < 1e-9, "linear stress exponent doubles at S=1.0, got {stressed}");
        assert!(t.fit_for("cap_ceramic", &base_inp(0.5, 40.0)).is_err(), "unknown class must not invent a number");
    }
}
