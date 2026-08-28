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

fn one_f64() -> f64 { 1.0 }
fn mode_one() -> String { "one".to_string() }

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
    /// MIL-HDBK-217F discrete-semiconductor part-stress model (§6):
    /// λ_p = λ_b·π_T·π_A·π_R·π_S·π_C·π_Q·π_E failures/10⁶ h with
    /// π_T = exp(−t_const·(1/(T_J+273) − 1/298)), T_J = T_A + θ_JA·P
    /// (sim-derived power; θ_JA from the netlist attribute).
    #[serde(rename = "mil217f_semiconductor")]
    Mil217fSemiconductor {
        lambda_b: f64,
        /// π_T exponential constant (3091 diodes, 1925 vreg/FET, 2114 BJT).
        t_const: f64,
        /// "one" | "diode" (.054 / Vs^2.43) | "bjt" (.045·e^{3.1·Vs}).
        pi_s_mode: String,
        /// Application factor with its handbook label (e.g. 0.70 switching).
        pi_a: f64,
        /// Contact construction (diodes; 1.0 metallurgically bonded).
        #[serde(default = "one_f64")]
        pi_c: f64,
        /// "one" | "bjt" (.43 for ≤0.1W else Pr^0.37; needs rated power).
        #[serde(default = "mode_one")]
        pi_r_mode: String,
        pi_q: BTreeMap<String, f64>,
        /// π_Q key applied when the mission declares none ("plastic" for
        /// COTS semiconductors).
        default_quality: String,
        pi_e: BTreeMap<String, f64>,
        source: String,
    },
    /// MIL-HDBK-217F inductive-device model (§11): λ_p = λ_b·π_C·π_Q·π_E
    /// with λ_b = a·exp(((T_HS+273)/nt)^p) and hot spot
    /// T_HS = T_A + 1.1·ΔT (§11.3; ΔT from the temp_rise attribute).
    #[serde(rename = "mil217f_inductive")]
    Mil217fInductive {
        a: f64,
        nt: f64,
        p: f64,
        /// Construction factor (coils: 1.0 fixed / 2.0 variable).
        #[serde(default = "one_f64")]
        pi_c: f64,
        pi_q: BTreeMap<String, f64>,
        default_quality: String,
        pi_e: BTreeMap<String, f64>,
        source: String,
    },
    /// MIL-HDBK-217F capacitor part-stress model (§10, λ_p in
    /// failures/10⁶ h): λ_p = λ_b·π_CV·[π_SR]·π_Q·π_E with
    /// λ_b = a·[(S/g)³+1]·exp(b·((T+273)/nt)^p) and π_CV = cv_a·C^cv_exp
    /// (C in `cv_unit`, "pF" or "uF"). S = applied/rated VOLTAGE.
    #[serde(rename = "mil217f_capacitor")]
    Mil217fCapacitor {
        a: f64,
        g: f64,
        b: f64,
        nt: f64,
        p: f64,
        cv_a: f64,
        cv_exp: f64,
        /// Unit the π_CV formula expects C in: "pF" or "uF".
        cv_unit: String,
        /// Optional series-resistance factor (§10.12 tantalum):
        /// [min_cr_ohms_per_volt, pi_sr] rows, tried in order — the
        /// first row whose min is below the circuit CR applies.
        /// Present ⇒ the instance must supply CR or the FIT is a gap.
        pi_sr: Option<Vec<(f64, f64)>>,
        pi_q: BTreeMap<String, f64>,
        pi_e: BTreeMap<String, f64>,
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
            ClassModel::Mil217fCapacitor { source, .. } => source,
            ClassModel::Mil217fSemiconductor { source, .. } => source,
            ClassModel::Mil217fInductive { source, .. } => source,
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
    /// S = applied / rated (sim-derived, never estimated). None when no
    /// sign-off row rated this instance — models that need S error out.
    pub stress_ratio: Option<f64>,
    /// Mission ambient, °C.
    pub ambient_c: f64,
    /// The part's resistance in ohms (netlist attribute), for π_R.
    pub resistance_ohm: Option<f64>,
    /// Quality level key into π_Q. `None` ⇒ "lower" (COTS — the
    /// handbook's own category for non-established-reliability parts).
    pub quality: Option<String>,
    /// Environment symbol key into π_E. `None` ⇒ "GB" (ground benign).
    pub environment: Option<String>,
    /// Capacitance in farads (netlist attribute), for π_CV.
    pub capacitance_f: Option<f64>,
    /// Circuit resistance in Ω/V between capacitor and supply, for the
    /// tantalum π_SR factor. Not derivable yet ⇒ usually None.
    pub cr_ohms_per_volt: Option<f64>,
    /// Dissipated power in W (GLACIER DC solve), for T_J / hot-spot.
    pub power_w: Option<f64>,
    /// θ_JA in °C/W (netlist attribute), for T_J = T_A + θ·P.
    pub theta_ja_c_per_w: Option<f64>,
    /// Rated power in W (power_rating attribute), for the BJT π_R.
    pub rated_power_w: Option<f64>,
    /// Average winding temperature rise ΔT in °C (temp_rise attribute),
    /// for the §11.3 hot spot T_HS = T_A + 1.1·ΔT.
    pub temp_rise_c: Option<f64>,
}

fn need_s(inp: &FitInputs) -> Result<f64, String> {
    inp.stress_ratio.ok_or_else(|| "no sim-derived stress ratio (no sign-off rating for this instance)".to_string())
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
                let s0 = need_s(inp)?;
                let s = s0.max(0.01);
                let pi_s = (s / s_ref).powf(*stress_exp);
                let fit = lambda_base * pi_t * pi_s;
                Ok((fit, format!(
                    "λ={:.2} FIT = {:.2}·π_T({:.2})·π_S({:.2}) @ S={:.2}, Ta={:.0}°C per {} [{}]",
                    fit, lambda_base, pi_t, pi_s, s0, inp.ambient_c, self.standard, source
                )))
            }
            ClassModel::Mil217fSemiconductor { lambda_b, t_const, pi_s_mode, pi_a, pi_c, pi_r_mode, pi_q, default_quality, pi_e, source } => {
                let Some(power) = inp.power_w else {
                    return Err("no sim-derived power (DC solve did not cover this instance; T_J needs it)".to_string());
                };
                let Some(theta) = inp.theta_ja_c_per_w else {
                    return Err("no theta_ja attribute on the instance (T_J = T_A + θ_JA·P needs it)".to_string());
                };
                let tj = inp.ambient_c + theta * power;
                let pi_t = (-t_const * (1.0 / (tj + 273.0) - 1.0 / 298.0)).exp();
                let (ps_txt, ps) = match pi_s_mode.as_str() {
                    "one" => (String::new(), 1.0),
                    "diode" => {
                        let vs = need_s(inp)?.clamp(0.0, 1.0);
                        let f = if vs <= 0.3 { 0.054 } else { vs.powf(2.43) };
                        (format!("·π_S({f:.3}@Vs={vs:.2})"), f)
                    }
                    "bjt" => {
                        let vs = need_s(inp)?.clamp(0.0, 1.0);
                        let f = 0.045 * (3.1 * vs).exp();
                        (format!("·π_S({f:.3}@Vs={vs:.2})"), f)
                    }
                    other => return Err(format!("pi_s_mode '{}' unknown (table bug)", other)),
                };
                let (pr_txt, pr) = match pi_r_mode.as_str() {
                    "one" => (String::new(), 1.0),
                    "bjt" => {
                        let Some(rp) = inp.rated_power_w else {
                            return Err("no power_rating attribute (BJT π_R needs rated power)".to_string());
                        };
                        let f = if rp <= 0.1 { 0.43 } else { rp.powf(0.37) };
                        (format!("·π_R({f:.2}@{rp}W)"), f)
                    }
                    other => return Err(format!("pi_r_mode '{}' unknown (table bug)", other)),
                };
                let q_key = inp.quality.clone().unwrap_or_else(|| default_quality.clone());
                let Some(pq) = pi_q.get(&q_key) else {
                    return Err(format!("quality '{}' not in π_Q ({})", q_key, pi_q.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                let e_key = inp.environment.clone().unwrap_or_else(|| "GB".to_string());
                let Some(pe) = pi_e.get(&e_key) else {
                    return Err(format!("environment '{}' not in π_E ({})", e_key, pi_e.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                let fit = 1000.0 * lambda_b * pi_t * pi_a * pr * ps * pi_c * pq * pe;
                Ok((fit, format!(
                    "λ={:.1} FIT = 1000·λb({})·π_T({:.2}@Tj={:.0}°C){}{}·π_A({}){}·π_Q({} {})·π_E({} {}) per {} [{}]",
                    fit, lambda_b, pi_t, tj, ps_txt, pr_txt, pi_a,
                    if *pi_c != 1.0 { format!("·π_C({pi_c})") } else { String::new() },
                    pq, q_key, pe, e_key, self.standard, source
                )))
            }
            ClassModel::Mil217fInductive { a, nt, p, pi_c, pi_q, default_quality, pi_e, source } => {
                let Some(dt) = inp.temp_rise_c else {
                    return Err("no temp_rise attribute (§11.3 hot spot T_HS = T_A + 1.1·ΔT needs the winding rise)".to_string());
                };
                let ths = inp.ambient_c + 1.1 * dt;
                let lambda_b = a * (((ths + 273.0) / nt).powf(*p)).exp();
                let q_key = inp.quality.clone().unwrap_or_else(|| default_quality.clone());
                let Some(pq) = pi_q.get(&q_key) else {
                    return Err(format!("quality '{}' not in π_Q ({})", q_key, pi_q.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                let e_key = inp.environment.clone().unwrap_or_else(|| "GB".to_string());
                let Some(pe) = pi_e.get(&e_key) else {
                    return Err(format!("environment '{}' not in π_E ({})", e_key, pi_e.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                let fit = 1000.0 * lambda_b * pi_c * pq * pe;
                Ok((fit, format!(
                    "λ={:.1} FIT = 1000·λb({:.5}@Ths={:.0}°C){}·π_Q({} {})·π_E({} {}) per {} [{}]",
                    fit, lambda_b, ths,
                    if *pi_c != 1.0 { format!("·π_C({pi_c})") } else { String::new() },
                    pq, q_key, pe, e_key, self.standard, source
                )))
            }
            ClassModel::Mil217fCapacitor { a, g, b, nt, p, cv_a, cv_exp, cv_unit, pi_sr, pi_q, pi_e, source } => {
                let t = inp.ambient_c;
                let s0 = need_s(inp)?;
                let s = s0.max(0.01);
                let lambda_b = a * ((s / g).powi(3) + 1.0) * (b * ((t + 273.0) / nt).powf(*p)).exp();
                let Some(c_f) = inp.capacitance_f else {
                    return Err("no capacitance attribute on the instance (π_CV needs it)".to_string());
                };
                let c_in_unit = match cv_unit.as_str() {
                    "pF" => c_f * 1e12,
                    "uF" => c_f * 1e6,
                    other => return Err(format!("cv_unit '{}' not pF/uF (table bug)", other)),
                };
                let pi_cv = cv_a * c_in_unit.powf(*cv_exp);
                let (sr_txt, psr) = match pi_sr {
                    None => (String::new(), 1.0),
                    Some(rows) => match inp.cr_ohms_per_volt {
                        None => return Err("class uses π_SR (series resistance) but the instance has no cr_ohms_per_volt".to_string()),
                        Some(cr) => match rows.iter().find(|(min, _)| cr > *min) {
                            Some((_, f)) => (format!("·π_SR({:.2}@{:.2}Ω/V)", f, cr), *f),
                            None => return Err(format!("CR {cr}Ω/V below every π_SR range")),
                        },
                    },
                };
                let q_key = inp.quality.clone().unwrap_or_else(|| "lower".to_string());
                let Some(pq) = pi_q.get(&q_key) else {
                    return Err(format!("quality '{}' not in π_Q ({})", q_key, pi_q.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                let e_key = inp.environment.clone().unwrap_or_else(|| "GB".to_string());
                let Some(pe) = pi_e.get(&e_key) else {
                    return Err(format!("environment '{}' not in π_E ({})", e_key, pi_e.keys().cloned().collect::<Vec<_>>().join(", ")));
                };
                let fit = 1000.0 * lambda_b * pi_cv * psr * pq * pe;
                Ok((fit, format!(
                    "λ={:.1} FIT = 1000·λb({:.5})·π_CV({:.2}@{:.0}{}){}·π_Q({} {})·π_E({} {}) @ S={:.2}, Ta={:.0}°C per {} [{}]",
                    fit, lambda_b, pi_cv, c_in_unit, cv_unit, sr_txt, pq, q_key, pe, e_key, s0, inp.ambient_c, self.standard, source
                )))
            }
            ClassModel::Mil217fResistor { a, b, nt, p, g, ns, pi_r, pi_q, pi_e, source } => {
                let t = inp.ambient_c;
                let s0 = need_s(inp)?;
                let s = s0.max(0.01);
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
                    fit, lambda_b, pr, r_ohm, pq, q_key, pe, e_key, s0, inp.ambient_c, self.standard, source
                )))
            }
        }
    }
}

/// A named mission profile from mission_profiles.toml — the project
/// tunable ("passenger_compartment", "motor_control", …). Explicit
/// mission items in the safety block override the profile's fields.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct MissionProfileFile {
    pub source: String,
    pub profiles: BTreeMap<String, MissionProfileDef>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct MissionProfileDef {
    pub environment: Option<String>,
    pub quality: Option<String>,
    pub time_basis: Option<String>,
    pub source: String,
    pub phases: Vec<ProfilePhaseDef>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct ProfilePhaseDef {
    pub name: String,
    /// Fraction of calendar life (0..1).
    pub time: f64,
    /// Ambient °C.
    pub ambient: f64,
    #[serde(default = "default_true")]
    pub powered: bool,
}

fn default_true() -> bool { true }

impl MissionProfileFile {
    pub fn from_toml(text: &str) -> Result<MissionProfileFile, String> {
        toml::from_str(text).map_err(|e| e.to_string())
    }
}

/// Resolve `mission.profile` against a profile file: fill phases and
/// (only where the board did not say otherwise) environment / quality /
/// time_basis. Returns an error string for an unknown profile or a
/// histogram that does not cover the life.
pub fn resolve_mission_profile(mission: &mut Mission, file: &MissionProfileFile) -> Result<String, String> {
    let Some(name) = mission.profile.clone() else { return Ok(String::new()) };
    let Some(def) = file.profiles.get(&name) else {
        return Err(format!("profile '{}' not in mission_profiles ({})", name, file.profiles.keys().cloned().collect::<Vec<_>>().join(", ")));
    };
    if mission.phases.is_empty() {
        mission.phases = def.phases.iter().map(|p| bhdl_common::safety::MissionPhase {
            name: p.name.clone(), frac: p.time, ambient_c: p.ambient, powered: p.powered,
        }).collect();
    }
    let sum: f64 = mission.phases.iter().map(|p| p.frac).sum();
    if (sum - 1.0).abs() > 0.02 {
        return Err(format!("profile '{}' phases sum to {:.3}, not 1.0", name, sum));
    }
    if mission.environment.is_none() { mission.environment = def.environment.clone(); }
    if mission.quality.is_none() { mission.quality = def.quality.clone(); }
    if mission.time_basis.is_none() { mission.time_basis = def.time_basis.clone(); }
    if mission.ambient_c.is_nan() {
        mission.ambient_c = mission.phases.iter().map(|p| p.frac * p.ambient_c).sum::<f64>() / sum;
    }
    Ok(def.source.clone())
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
    pub capacitance_f: Option<f64>,
    /// Dissipated power from the DC solve (all instances).
    pub power_w: Option<f64>,
    /// θ_JA °C/W from the theta_ja attribute.
    pub theta_ja_c_per_w: Option<f64>,
    /// Rated power in W from the power_rating attribute.
    pub rated_power_w: Option<f64>,
    /// Winding temperature rise from the temp_rise attribute.
    pub temp_rise_c: Option<f64>,
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
            {
                let mk_inp = |ambient_c: f64| FitInputs {
                    stress_ratio: (st.rated > 0.0).then(|| st.applied / st.rated),
                    ambient_c,
                    resistance_ohm: st.resistance_ohm,
                    quality: m.quality.clone(),
                    environment: m.environment.clone(),
                    capacitance_f: st.capacitance_f,
                    cr_ohms_per_volt: None,
                    power_w: st.power_w,
                    theta_ja_c_per_w: st.theta_ja_c_per_w,
                    rated_power_w: st.rated_power_w,
                    temp_rise_c: st.temp_rise_c,
                };
                if m.phases.is_empty() && m.ambient_c.is_nan() {
                    missing.push("mission profile named but unresolved (no phases, no ambient)".to_string());
                } else if m.phases.is_empty() {
                    match table.fit_for(class, &mk_inp(m.ambient_c)) {
                        Ok((f, basis)) => {
                            *fit = Some(f);
                            *fit_basis = Some(basis);
                            continue;
                        }
                        Err(e) => missing.push(e),
                    }
                } else {
                    // Time-weighted λ over the histogram. Unpowered
                    // phases contribute zero (the shipped models have no
                    // dormant term — stated, not hidden). Basis:
                    // "operating" (default) divides by powered time —
                    // λ per operating hour, the FMEDA/PMHF convention;
                    // "calendar" divides by total time.
                    let mut weighted = 0.0f64;
                    let mut err: Option<String> = None;
                    let mut parts_txt: Vec<String> = Vec::new();
                    let powered_frac: f64 = m.phases.iter().filter(|p| p.powered).map(|p| p.frac).sum();
                    for ph in &m.phases {
                        if !ph.powered {
                            parts_txt.push(format!("{} {:.0}%@{:.0}°C off", ph.name, ph.frac * 100.0, ph.ambient_c));
                            continue;
                        }
                        match table.fit_for(class, &mk_inp(ph.ambient_c)) {
                            Ok((f, _)) => {
                                weighted += ph.frac * f;
                                parts_txt.push(format!("{} {:.0}%@{:.0}°C→{:.1}", ph.name, ph.frac * 100.0, ph.ambient_c, f));
                            }
                            Err(e) => { err = Some(e); break; }
                        }
                    }
                    if let Some(e) = err {
                        missing.push(e);
                    } else {
                        let basis_kind = m.time_basis.clone().unwrap_or_else(|| "operating".to_string());
                        let f = match basis_kind.as_str() {
                            "calendar" => weighted,
                            _ => if powered_frac > 0.0 { weighted / powered_frac } else { 0.0 },
                        };
                        let profile_txt = m.profile.as_deref().map(|p| format!(" profile={p}")).unwrap_or_default();
                        *fit = Some(f);
                        *fit_basis = Some(format!(
                            "λ={:.1} FIT ({} basis{}) = Σ[{}] / powered {:.0}%{} per {}",
                            f, basis_kind, profile_txt, parts_txt.join(", "), powered_frac * 100.0,
                            if st.rated > 0.0 { format!(" @ S={:.2}", st.applied / st.rated) } else { String::new() }, table.standard
                        ));
                        continue;
                    }
                }
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
        FitInputs { stress_ratio: Some(s), ambient_c: t, resistance_ohm: Some(1000.0), quality: Some("M".into()), environment: Some("GB".into()), capacitance_f: None, cr_ohms_per_volt: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None }
    }

    const M217_CAP_TABLE: &str = r#"
standard = "MILHDBK217F"
source = "MIL-HDBK-217F Notice 2, Section 10"

[classes.cap_ceramic_chip]
model = "mil217f_capacitor"
a = 2.6e-9
g = 0.3
b = 14.3
nt = 398.0
p = 1.0
cv_a = 0.59
cv_exp = 0.12
cv_unit = "pF"
pi_q = { S = 0.03, R = 0.1, P = 0.3, M = 1.0, mil_spec = 3.0, lower = 10.0 }
pi_e = { GB = 1.0, GF = 2.0, GM = 10.0 }
source = "§10.11 p.10-20"

[classes.cap_ceramic_gp]
model = "mil217f_capacitor"
a = 3.0e-4
g = 0.3
b = 1.0
nt = 398.0
p = 1.0
cv_a = 0.41
cv_exp = 0.11
cv_unit = "pF"
pi_q = { M = 1.0, lower = 10.0 }
pi_e = { GB = 1.0 }
source = "§10.10 p.10-18/19"

[classes.cap_tantalum_solid]
model = "mil217f_capacitor"
a = 3.75e-3
g = 0.4
b = 2.6
nt = 398.0
p = 9.0
cv_a = 1.0
cv_exp = 0.12
cv_unit = "uF"
pi_sr = [[0.8, 0.066], [0.6, 0.10], [0.4, 0.13], [0.2, 0.20], [0.1, 0.27], [0.0, 0.33]]
pi_q = { M = 1.0, lower = 10.0 }
pi_e = { GB = 1.0 }
source = "§10.12 p.10-21"
"#;

    /// Capacitor λ_b and π_CV must reproduce the handbook's own printed
    /// tables (§10.10/§10.11/§10.12).
    #[test]
    fn mil217f_capacitor_matches_the_handbooks_printed_tables() {
        let t = ReliabilityTable::from_toml(M217_CAP_TABLE).unwrap();
        // π_CV = 1 requires C s.t. cv_a·C^exp = 1; instead divide out:
        // use the handbook's π_CV spot values to pick C, then check the
        // combined number. Simpler: λ_b via fit/(1000·π_CV) with known C.
        let lb = |class: &str, s: f64, temp: f64, c_f: f64, cv: f64| {
            let inp = FitInputs { stress_ratio: Some(s), ambient_c: temp, resistance_ohm: None, quality: Some("M".into()), environment: Some("GB".into()), capacitance_f: Some(c_f), cr_ohms_per_volt: Some(1.0), power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None };
            t.fit_for(class, &inp).unwrap().0 / 1000.0 / cv / if class == "cap_tantalum_solid" { 0.066 } else { 1.0 }
        };
        // (class, S, T, C_farads, handbook π_CV at that C, handbook λ_b)
        let cases: &[(&str, f64, f64, f64, f64, f64)] = &[
            ("cap_ceramic_chip", 0.1, 20.0, 4.1e-9, 1.6, 0.00010),   // 4100 pF → π_CV 1.6
            ("cap_ceramic_chip", 0.5, 80.0, 4.1e-9, 1.6, 0.0047),
            ("cap_ceramic_gp", 0.1, 20.0, 3.6e-8, 1.3, 0.00065),     // 36000 pF → π_CV 1.3
            ("cap_ceramic_gp", 0.9, 120.0, 3.6e-8, 1.3, 0.023),
            ("cap_tantalum_solid", 0.1, 20.0, 8.9e-6, 1.3, 0.0045),  // 8.9 µF → π_CV 1.3, CR>0.8 → π_SR .066
            ("cap_tantalum_solid", 0.5, 120.0, 8.9e-6, 1.3, 0.11),
        ];
        for (class, s, temp, c_f, cv, expect) in cases {
            let got = lb(class, *s, *temp, *c_f, *cv);
            let rel = (got - expect).abs() / expect;
            assert!(rel < 0.06, "{class} S={s} T={temp}: computed λ_b {got:.5} vs handbook {expect} (rel {rel:.3})");
        }
        // honest errors: no capacitance / tantalum without CR
        let no_c = FitInputs { stress_ratio: Some(0.1), ambient_c: 20.0, resistance_ohm: None, quality: None, environment: None, capacitance_f: None, cr_ohms_per_volt: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None };
        assert!(t.fit_for("cap_ceramic_chip", &no_c).unwrap_err().contains("capacitance"));
        let no_cr = FitInputs { stress_ratio: Some(0.1), ambient_c: 20.0, resistance_ohm: None, quality: None, environment: None, capacitance_f: Some(1e-6), cr_ohms_per_volt: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None };
        assert!(t.fit_for("cap_tantalum_solid", &no_cr).unwrap_err().contains("cr_ohms_per_volt"));
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
            mission: Some(Mission { ambient_c: 40.0, on_hours: None, cycles: None, environment: None, quality: None, profile: None, phases: vec![], time_basis: None, lifetime_h: None }),
            scopes: vec![], universe: vec![], gaps: vec![], errors: vec![],
            parts: vec![
                Part { instance: "r1".into(), type_name: "Res".into(), parent: None, data: hb(Some("IEC62380")), domains: vec![], clocks: vec![] },
                Part { instance: "r2".into(), type_name: "Res".into(), parent: None, data: hb(Some("IEC62380")), domains: vec![], clocks: vec![] }, // no stress
                Part { instance: "r3".into(), type_name: "Res".into(), parent: None, data: hb(None), domains: vec![], clocks: vec![] },             // no standard: untouched
            ],
        };
        let stress: StressMap = [("r1".to_string(), InstanceStress { applied: 0.25, rated: 0.5, resistance_ohm: Some(1e3), capacitance_f: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None })].into();
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
    fn phase_weighted_mission_and_profile_resolution() {
        use bhdl_common::safety::{Mission, MissionPhase, Part, SafetyModel};
        let t = ReliabilityTable::from_toml(M217_TABLE).unwrap();
        let tables: HashMap<String, ReliabilityTable> = [("MILHDBK217F".to_string(), t.clone())].into();
        let phases = vec![
            MissionPhase { name: "parked".into(), frac: 0.9, ambient_c: 23.0, powered: false },
            MissionPhase { name: "drive".into(), frac: 0.1, ambient_c: 60.0, powered: true },
        ];
        let mut model = SafetyModel {
            board: "B".into(),
            mission: Some(Mission { ambient_c: 26.7, on_hours: None, cycles: None, environment: Some("GB".into()), quality: Some("M".into()), profile: Some("p".into()), phases: phases.clone(), time_basis: None, lifetime_h: None }),
            scopes: vec![], universe: vec![], gaps: vec![], errors: vec![],
            parts: vec![Part { instance: "r1".into(), type_name: "Res".into(), parent: None, data: PartData::Handbook {
                class: "res_fixed_film".into(), source: "t".into(), per: Some("MILHDBK217F".into()), fit: None, fit_basis: None }, domains: vec![], clocks: vec![] }],
        };
        let stress: StressMap = [("r1".to_string(), InstanceStress { applied: 0.1, rated: 1.0, resistance_ohm: Some(1e3), capacitance_f: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None })].into();
        apply_reliability(&mut model, &stress, &tables);
        let PartData::Handbook { fit: Some(f), fit_basis: Some(basis), .. } = &model.parts[0].data else { panic!("no fit") };
        // operating basis: unpowered 90% contributes 0, division by the
        // powered 10% ⇒ λ equals the 60°C point-value exactly.
        let inp = FitInputs { stress_ratio: Some(0.1), ambient_c: 60.0, resistance_ohm: Some(1e3), quality: Some("M".into()), environment: Some("GB".into()), capacitance_f: None, cr_ohms_per_volt: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None };
        let (point, _) = t.fit_for("res_fixed_film", &inp).unwrap();
        assert!((f - point).abs() < 1e-9, "operating-basis λ {f} must equal the powered phase point value {point}");
        assert!(basis.contains("operating") && basis.contains("profile=p") && basis.contains("off"));
        // calendar basis scales by the powered fraction
        model.parts[0].data = PartData::Handbook { class: "res_fixed_film".into(), source: "t".into(), per: Some("MILHDBK217F".into()), fit: None, fit_basis: None };
        model.gaps.clear();
        model.mission.as_mut().unwrap().time_basis = Some("calendar".into());
        apply_reliability(&mut model, &stress, &tables);
        let PartData::Handbook { fit: Some(fc), .. } = &model.parts[0].data else { panic!("no fit") };
        assert!((fc - point * 0.1).abs() < 1e-9, "calendar basis = powered_frac × point value");

        // profile resolution: fills phases + env, explicit items win, bad sums error
        let file = MissionProfileFile::from_toml(r#"
source = "test"
[profiles.cabin]
environment = "GM"
source = "test profile"
phases = [ { name = "parked", time = 0.9, ambient = 23.0, powered = false },
           { name = "drive", time = 0.1, ambient = 60.0 } ]
[profiles.broken]
source = "broken"
phases = [ { name = "half", time = 0.5, ambient = 23.0 } ]
"#).unwrap();
        let mut m = Mission { ambient_c: f64::NAN, on_hours: None, cycles: None, environment: Some("GB".into()), quality: None, profile: Some("cabin".into()), phases: vec![], time_basis: None, lifetime_h: None };
        resolve_mission_profile(&mut m, &file).unwrap();
        assert_eq!(m.phases.len(), 2);
        assert_eq!(m.environment.as_deref(), Some("GB"), "explicit board environment beats the profile's");
        assert!(!m.ambient_c.is_nan());
        let mut bad = Mission { ambient_c: f64::NAN, on_hours: None, cycles: None, environment: None, quality: None, profile: Some("broken".into()), phases: vec![], time_basis: None, lifetime_h: None };
        assert!(resolve_mission_profile(&mut bad, &file).unwrap_err().contains("sum"));
        let mut unk = Mission { ambient_c: f64::NAN, on_hours: None, cycles: None, environment: None, quality: None, profile: Some("nope".into()), phases: vec![], time_basis: None, lifetime_h: None };
        assert!(resolve_mission_profile(&mut unk, &file).unwrap_err().contains("nope"));
    }

    const M217_SEMI_TABLE: &str = r#"
standard = "MILHDBK217F"
source = "MIL-HDBK-217F Notice 2, Sections 6 + 11"

[classes.diode_gp]
model = "mil217f_semiconductor"
lambda_b = 0.0038
t_const = 3091.0
pi_s_mode = "diode"
pi_a = 1.0
pi_q = { JANTX = 1.0, plastic = 8.0 }
default_quality = "plastic"
pi_e = { GB = 1.0, GM = 9.0 }
source = "§6.1"

[classes.bjt_npn_pnp]
model = "mil217f_semiconductor"
lambda_b = 0.00074
t_const = 2114.0
pi_s_mode = "bjt"
pi_a = 0.70
pi_r_mode = "bjt"
pi_q = { JANTX = 1.0, plastic = 8.0 }
default_quality = "plastic"
pi_e = { GB = 1.0 }
source = "§6.3"

[classes.coil_fixed]
model = "mil217f_inductive"
a = 0.000319
nt = 364.0
p = 8.7
pi_q = { M = 1.0, lower = 20.0 }
default_quality = "lower"
pi_e = { GB = 1.0 }
source = "§11.2 class B"
"#;

    /// π_T, π_S, π_R and the inductive λ_b must reproduce the
    /// handbook's own printed tables (§6.1/§6.3/§11.2).
    #[test]
    fn mil217f_semiconductor_and_inductive_match_the_handbook() {
        let t = ReliabilityTable::from_toml(M217_SEMI_TABLE).unwrap();
        let semi = |class: &str, s: Option<f64>, tj_via: (f64, f64, f64), q: &str, rp: Option<f64>| {
            let (amb, theta, pw) = tj_via;
            let inp = FitInputs { stress_ratio: s, ambient_c: amb, resistance_ohm: None, quality: Some(q.into()), environment: Some("GB".into()), capacitance_f: None, cr_ohms_per_volt: None, power_w: Some(pw), theta_ja_c_per_w: Some(theta), rated_power_w: rp, temp_rise_c: None };
            t.fit_for(class, &inp).unwrap().0
        };
        // diode πT table: Tj=100 → 8.0 (handbook §6.1). Tj = 25 + 75·1.
        let base = semi("diode_gp", Some(0.9999), (25.0, 0.0, 0.0), "JANTX", None); // Tj=25 → πT=1, πS≈1
        let hot = semi("diode_gp", Some(0.9999), (25.0, 75.0, 1.0), "JANTX", None);
        let ratio = hot / base;
        assert!((ratio - 8.0).abs() / 8.0 < 0.02, "diode πT@Tj=100 should be ~8.0, got {ratio:.2}");
        // diode πS: Vs=.5 → .19 (handbook §6.1)
        let s19 = semi("diode_gp", Some(0.5), (25.0, 0.0, 0.0), "JANTX", None) / base;
        assert!((s19 - 0.19).abs() / 0.19 < 0.05, "πS@.5 ~ .19, got {s19:.3}");
        // BJT πR: Pr=10W → 2.3 (handbook §6.3)
        let b1 = semi("bjt_npn_pnp", Some(0.5), (25.0, 0.0, 0.0), "JANTX", Some(1.0));
        let b10 = semi("bjt_npn_pnp", Some(0.5), (25.0, 0.0, 0.0), "JANTX", Some(10.0));
        assert!((b10 / b1 - 2.3).abs() / 2.3 < 0.02, "BJT πR@10W ~ 2.3×πR@1W, got {:.2}", b10 / b1);
        // missing θ_JA = honest gap
        let no_theta = FitInputs { stress_ratio: Some(0.5), ambient_c: 25.0, resistance_ohm: None, quality: None, environment: None, capacitance_f: None, cr_ohms_per_volt: None, power_w: Some(0.1), theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None };
        assert!(t.fit_for("diode_gp", &no_theta).unwrap_err().contains("theta_ja"));
        // inductive: λ_b@T_HS=85 = .00076 (§11.2 class-125 column @85).
        // ambient 60 + 1.1·22.727 = 85.
        let ind = FitInputs { stress_ratio: None, ambient_c: 60.0, resistance_ohm: None, quality: Some("M".into()), environment: Some("GB".into()), capacitance_f: None, cr_ohms_per_volt: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: Some(22.727) };
        let (f, basis) = t.fit_for("coil_fixed", &ind).unwrap();
        assert!((f / 1000.0 - 0.00076).abs() / 0.00076 < 0.02, "coil λ_b@85 ~ .00076, got {}", f / 1000.0);
        assert!(basis.contains("Ths=85"));
        // missing temp_rise = honest gap
        let no_rise = FitInputs { temp_rise_c: None, ..ind.clone() };
        assert!(t.fit_for("coil_fixed", &no_rise).unwrap_err().contains("temp_rise"));
    }

    #[test]
    fn fit_scales_with_temperature_and_stress() {
        let t = ReliabilityTable::from_toml(ARR_TABLE).unwrap();
        let base_inp = |s: f64, temp: f64| FitInputs { stress_ratio: Some(s), ambient_c: temp, resistance_ohm: None, quality: None, environment: None, capacitance_f: None, cr_ohms_per_volt: None, power_w: None, theta_ja_c_per_w: None, rated_power_w: None, temp_rise_c: None };
        let (base, _) = t.fit_for("res_film_low_dissipation", &base_inp(0.5, 40.0)).unwrap();
        assert!((base - 0.5).abs() < 1e-9, "reference conditions return lambda_base, got {base}");
        let (hot, _) = t.fit_for("res_film_low_dissipation", &base_inp(0.5, 85.0)).unwrap();
        assert!(hot > base, "hotter must not be more reliable");
        let (stressed, _) = t.fit_for("res_film_low_dissipation", &base_inp(1.0, 40.0)).unwrap();
        assert!((stressed - 1.0).abs() < 1e-9, "linear stress exponent doubles at S=1.0, got {stressed}");
        assert!(t.fit_for("cap_ceramic", &base_inp(0.5, 40.0)).is_err(), "unknown class must not invent a number");
    }
}
