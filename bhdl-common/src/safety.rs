//! Functional-safety semantic model (docs/spec/Functional_Safety.md §3).
//!
//! Built by the synthesizer from the `safety <Name> [of E] as ns { }`
//! blocks and library `safety_goal` definitions, resolved against the
//! synthesized netlist. Phase 1 carries goals, effects, mechanisms,
//! faults (declared, not run), waivers, assumptions, the per-instance
//! part table and the gap list — and NO metrics: nothing here is a
//! number that was not measured or sourced.
//!
//! ISO 26262 (ASIL) and IEC 61508 (SIL) share this model; only metric
//! definitions and targets (Phase 3/4) differ.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Safety integrity level. One enum for both standards so the model
/// stays shared; `standard()` tells them apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Level {
    QM,
    AsilA,
    AsilB,
    AsilC,
    AsilD,
    Sil1,
    Sil2,
    Sil3,
    Sil4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Standard {
    Iso26262,
    Iec61508,
}

impl Level {
    /// Parse the level token as written in source (`ASIL_B`, `QM`, `SIL3`, …).
    pub fn parse(s: &str) -> Option<Level> {
        match s.to_ascii_uppercase().as_str() {
            "QM" => Some(Level::QM),
            "ASIL_A" | "A" => Some(Level::AsilA),
            "ASIL_B" | "B" => Some(Level::AsilB),
            "ASIL_C" | "C" => Some(Level::AsilC),
            "ASIL_D" | "D" => Some(Level::AsilD),
            "SIL1" | "SIL_1" => Some(Level::Sil1),
            "SIL2" | "SIL_2" => Some(Level::Sil2),
            "SIL3" | "SIL_3" => Some(Level::Sil3),
            "SIL4" | "SIL_4" => Some(Level::Sil4),
            _ => None,
        }
    }

    pub fn standard(self) -> Standard {
        match self {
            Level::Sil1 | Level::Sil2 | Level::Sil3 | Level::Sil4 => Standard::Iec61508,
            _ => Standard::Iso26262,
        }
    }

    /// ISO 26262-5 requires latent-fault coverage (an LSM) from ASIL C
    /// up; IEC 61508 analogously from SIL 3. Phase-1 gap `PSM_WITHOUT_LSM`.
    pub fn requires_lsm(self) -> bool {
        matches!(self, Level::AsilC | Level::AsilD | Level::Sil3 | Level::Sil4)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Level::QM => "QM",
            Level::AsilA => "ASIL_A",
            Level::AsilB => "ASIL_B",
            Level::AsilC => "ASIL_C",
            Level::AsilD => "ASIL_D",
            Level::Sil1 => "SIL1",
            Level::Sil2 => "SIL2",
            Level::Sil3 => "SIL3",
            Level::Sil4 => "SIL4",
        }
    }
}

/// Severity class of a failure effect (ISO 26262-3 S0..S3).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Severity {
    S0,
    S1,
    S2,
    S3,
}

impl Severity {
    pub fn parse(s: &str) -> Option<Severity> {
        match s.to_ascii_uppercase().as_str() {
            "S0" => Some(Severity::S0),
            "S1" => Some(Severity::S1),
            "S2" => Some(Severity::S2),
            "S3" => Some(Severity::S3),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::S0 => "S0",
            Severity::S1 => "S1",
            Severity::S2 => "S2",
            Severity::S3 => "S3",
        }
    }
}

/// A failure effect: a predicate over the design's nets/pins, with the
/// source text of the expression (the Phase-3 campaign evaluates it on
/// the simulated operating point) and the handles it references.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Effect {
    pub name: String,
    pub expr: String,
    pub severity: Severity,
    /// Resolved design references in the expression (`rail_a_mon.nOUT`,
    /// net `V5_A`, …) — what the campaign must monitor.
    pub refs: Vec<String>,
}

/// A safety goal as instantiated on one design element (entity instance
/// or board). Library goals are expanded here with their formals bound.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Goal {
    /// Local goal name (`SG_OV`).
    pub name: String,
    /// Fully qualified: `<scope>.<name>` (`rail_a.SG_OV`, `SG_SUPPLY`).
    pub path: String,
    /// Library goal type, if instantiated from one.
    pub library_type: Option<String>,
    pub level: Level,
    pub title: String,
    pub id: Option<String>,
    pub ftti: Option<String>,
    pub safe_state: Option<String>,
    pub effects: Vec<Effect>,
    /// `refines <parent goal path>`.
    pub refines: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MechanismKind {
    Psm,
    Lsm,
}

/// A design element declared as a safety mechanism for a goal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mechanism {
    /// Resolved netlist instance (`rail_a_mon`).
    pub instance: String,
    /// As written (`dut.mon`), for the report.
    pub handle: String,
    pub kind: MechanismKind,
    /// Goal path this mechanism serves.
    pub goal: String,
    pub detects: Vec<String>,
    /// LSM: the PSM instance it protects.
    pub protects: Option<String>,
    pub claimed_dc: Option<f64>,
    pub dc_source: Option<String>,
    pub interval: Option<String>,
    pub latency: Option<String>,
    /// Detection predicate (voltage expr over design handles): TRUE on a
    /// faulted operating point ⇔ this mechanism has detected the fault.
    /// Without it a measured DC cannot exist.
    #[serde(default)]
    pub detected_when: Option<String>,
    /// MEASURED diagnostic coverage from the fault-universe campaign
    /// (Phase 3): detected dangerous weight / total dangerous weight.
    #[serde(default)]
    pub measured_dc: Option<f64>,
    /// Basis of the measurement (weighting, counts) — printed verbatim.
    #[serde(default)]
    pub measured_note: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fault {
    /// `short` | `open` | `drift` | `state`
    pub kind: String,
    /// Resolved targets.
    pub targets: Vec<String>,
    /// `<goal path>.<effect>`
    pub expect: String,
    pub detected_by: Option<String>,
    pub within: Option<String>,
    /// Phase 3 fills this in; Phase 1 = false (`FAULT_UNRUN`).
    pub run: bool,
    /// Effect paths whose predicate evaluated TRUE on the faulted
    /// operating point (campaign result).
    #[serde(default)]
    pub fired: Vec<String>,
    /// Did the expected effect fire? None until run.
    #[serde(default)]
    pub expectation_met: Option<bool>,
    /// Campaign note (solve diverged, kind unsupported, …).
    #[serde(default)]
    pub note: Option<String>,
    /// `within <FTTI>` verdict: Some(true) = the detecting mechanism's
    /// declared interval+latency budget fits inside the FTTI and the
    /// fault IS detected at steady state; Some(false) = never detected
    /// or budget exceeds FTTI; None = no `within`, or unverifiable
    /// (mechanism declares no timing and there is no transient sim).
    #[serde(default)]
    pub timing_met: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Waiver {
    pub instance: String,
    pub handle: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AssumptionStatus {
    Open,
    SatisfiedBy(String),
    Waived(String),
}

/// An assumption of use declared by a scope, discharged (or not) by a
/// parent scope.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Assumption {
    pub id: String,
    /// Fully qualified `<scope>.<id>`.
    pub path: String,
    pub text: String,
    pub status: AssumptionStatus,
}

/// One vendor-declared failure state of a behavioral part.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FailureState {
    pub name: String,
    /// Vendor FIT share of this state (`fit=8 of 40`) — the REAL mode
    /// fraction, used as the λ weight in the fault universe.
    pub fit: Option<f64>,
    /// What the state DOES, as a board-observable mutation:
    /// `open(PIN)` | `short(PIN_A,PIN_B)` | `force(PIN, <voltage>)` |
    /// `pulse(PIN, <voltage>, <duration>)` (transient pin symptom;
    /// several ';'-separated ops = ONE fault's correlated multi-pin
    /// symptom vector, one λ — never a multi-point fault).
    /// Absent ⇒ the state cannot be simulated (honest gap, never a guess).
    pub behavior: Option<String>,
    /// Vendor declaration that the CHIP detects this state internally:
    /// a duration ⇒ detection with that reaction latency; "yes"/"true"
    /// ⇒ detected but with no timing data (FTTI unverifiable, stated).
    /// Absent ⇒ no internal detection claimed.
    #[serde(default)]
    pub internal_detection: Option<String>,
}

/// What kind of safety data a physical part carries (Phase 2 fills the
/// variants; Phase 1 can only see `None` and waivers).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PartData {
    /// Behavioral model with declared failure states.
    Behavioral { failure_states: usize, source: String, states: Vec<FailureState> },
    /// Black box with SEooC data.
    Seooc { lambda_fit: Option<f64>, source: String },
    /// Handbook class data (passives). `per` names the prediction
    /// standard whose equations compute the FIT (e.g. "IEC62380");
    /// `fit`/`fit_basis` are filled by the reliability engine when the
    /// mission profile, the sim-derived stress and the coefficient
    /// table are all present — never guessed.
    Handbook {
        class: String,
        source: String,
        per: Option<String>,
        fit: Option<f64>,
        fit_basis: Option<String>,
    },
    /// Waived out of the argument with a reason.
    Waived { reason: String },
    /// Nothing — gap `PART_NO_SAFETY_DATA`.
    None,
}

/// One physical instance in the analysed scope.
/// A declared SoC/regulator power domain — the PDN contract the board
/// must meet. All numbers are VENDOR data (typically NDA'd — they live
/// in the customer's own files, never this repo); absence of a field
/// simply skips that check, stated.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PowerDomain {
    pub name: String,
    /// Entity pins belonging to this domain (share one rail net).
    pub pins: Vec<String>,
    /// Nominal rail voltage (V).
    pub v_nom: f64,
    /// Static tolerance (± percent of v_nom).
    pub tol_pct: Option<f64>,
    /// Nominal / maximum current draw (A).
    pub i_nom_a: Option<f64>,
    pub i_max_a: Option<f64>,
    /// Target-impedance mask breakpoints (Hz, Ω), log-log interpolated
    /// between points; checked only inside the declared span.
    pub zmask: Vec<(f64, f64)>,
    /// Load-step stimulus: magnitude (A), rise time (s), hold (s).
    pub step_a: Option<f64>,
    pub step_rise_s: Option<f64>,
    pub step_dur_s: Option<f64>,
    /// Allowed droop under the step (± percent of v_nom).
    pub droop_max_pct: Option<f64>,
    /// Declared layout-PDN budget: series R (Ω) and L (H) between the
    /// board network and the die — added as labelled series terms.
    pub pdn_r_ohm: Option<f64>,
    pub pdn_l_h: Option<f64>,
    /// Rail noise target (µVrms), from the part's datasheet supply
    /// requirements (analog/PLL rails state these). Drives the power
    /// tree's buck-vs-LDO and post-regulation decisions.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_uvrms: Option<f64>,
    /// Always-on load (standby/RTC/management): its rail must live
    /// independent of the main protected front end — the power tree
    /// lets it hang DIRECT off the input, stated, when a pre-regulator
    /// policy is in force.
    #[serde(default)]
    pub always_on: bool,
    /// Power-UP sequencing (the part's own sequencing table). Any
    /// combination is valid: explicit edges (`after=` + optional hard
    /// `t_min=`), slot numbers (`slot=` + optional `slot_t_min=` —
    /// slot-N rails come up after ALL slot-N−1 rails), and
    /// software-enabled rails (`sw_enabled=true` — firmware raises the
    /// rail after boot; hardware must expose a signal-driven enable and
    /// the ordering itself becomes a stated software assumption).
    /// This domain must come up AFTER these sibling domains (names in
    /// the same entity).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seq_after: Vec<String>,
    /// Hard minimum delay (s) from the `after` rails good to this
    /// rail's enable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq_t_min_s: Option<f64>,
    /// Hard MAXIMUM delay (s) — the rail must follow within this window
    /// (SoC latch-up windows). Pairwise checks cannot verify it
    /// (delays COMPOSE along the chain); the power-up timeline does.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq_t_max_s: Option<f64>,
    /// Slot number in the part's power-up sequence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq_slot: Option<u32>,
    /// Minimum inter-slot delay (s) before this rail's slot.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq_slot_t_min_s: Option<f64>,
    /// Firmware raises this rail after boot (see above).
    #[serde(default)]
    pub sw_enabled: bool,
    /// Draw in the SLEEP state (A). Discharge physics: a dropped
    /// rail's fall time is C·V/I_load in the TARGET state — a
    /// nearly-unloaded rail bleeds slowly, which is why discharge
    /// paths exist.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub i_sleep_a: Option<f64>,
    /// This rail is DROPPED in sleep (firmware lowers its enable —
    /// requires a signal-driven EN, checked).
    #[serde(default)]
    pub sleep_off: bool,
    /// Power-DOWN ordering: this domain must be down BEFORE these
    /// sibling domains.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seq_down_before: Vec<String>,
    /// Maximum time (s) from the power-down trigger to this rail
    /// fully down.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seq_down_t_max_s: Option<f64>,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Part {
    /// Netlist instance name (`rail_a_mon`).
    pub instance: String,
    /// Entity/component type (`VoltageSupervisor`).
    pub type_name: String,
    /// The safety part (entity instance) it belongs to (`rail_a`), or
    /// `None` for top-level parts.
    pub parent: Option<String>,
    pub data: PartData,
    /// Declared power domains (SoC PDN contract), if any.
    #[serde(default)]
    pub domains: Vec<PowerDomain>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum GapClass {
    EffectUndetected,
    PsmWithoutLsm,
    DcUnsourced,
    AssumptionOpen,
    PartNoSafetyData,
    FaultUnrun,
    /// A handbook part names a prediction standard but its FIT could not
    /// be computed (missing mission profile, unsolved stress, or no
    /// coefficient table).
    FitUncomputed,
    /// The entity's vendor safety data declares the configuration it was
    /// computed for (`config k=v … source=…`) and this instance's actual
    /// configuration differs — the FIT/failure split does not apply here.
    ConfigMismatch,
    /// A vendor assumption of use (PDN mask, droop window, supply
    /// capability) is VIOLATED by the measured board — blocks the
    /// verdict like any other gap.
    AouViolated,
    /// A measured architectural metric (SPFM/LFM/PMHF) misses its ISO
    /// target, or the measurement is incomplete at a level that has one.
    MetricMissed,
}

impl GapClass {
    pub fn as_str(self) -> &'static str {
        match self {
            GapClass::EffectUndetected => "EFFECT_UNDETECTED",
            GapClass::PsmWithoutLsm => "PSM_WITHOUT_LSM",
            GapClass::DcUnsourced => "DC_UNSOURCED",
            GapClass::AssumptionOpen => "ASSUMPTION_OPEN",
            GapClass::PartNoSafetyData => "PART_NO_SAFETY_DATA",
            GapClass::FaultUnrun => "FAULT_UNRUN",
            GapClass::FitUncomputed => "FIT_UNCOMPUTED",
            GapClass::ConfigMismatch => "CONFIG_MISMATCH",
            GapClass::AouViolated => "AOU_VIOLATED",
            GapClass::MetricMissed => "METRIC_MISSED",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Gap {
    pub class: GapClass,
    /// Goal path the gap counts against (or the scope, for part gaps).
    pub goal: String,
    /// Where: effect name / instance / assumption id.
    pub subject: String,
    /// One-line fix.
    pub fix: String,
}

/// One analysed scope: an entity instance or a board that has a
/// `safety` block (directly or inherited from its entity).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Scope {
    /// Instance path (`rail_a`) or the board name for the root.
    pub path: String,
    /// The entity whose `safety` block applies.
    pub entity: String,
    /// Namespace name used in the block (`dut`, `brd`), for the report.
    pub ns: String,
    pub goals: Vec<Goal>,
    pub mechanisms: Vec<Mechanism>,
    pub faults: Vec<Fault>,
    pub waivers: Vec<Waiver>,
    pub assumptions: Vec<Assumption>,
    /// Measured FMEDA metrics (Phase 3), filled after the universe runs.
    #[serde(default)]
    pub metrics: Option<Metrics>,
}

/// One phase of a mission profile: a fraction of life at one ambient.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MissionPhase {
    pub name: String,
    /// Fraction of calendar life (0..1). Phases must sum to ~1.
    pub frac: f64,
    /// Ambient temperature in °C during this phase.
    pub ambient_c: f64,
    /// Powered? Unpowered phases contribute no operating failure rate
    /// (the shipped models carry no dormant term — stated, not hidden).
    pub powered: bool,
}

/// Board-level mission profile (spec §2.8): the environment every
/// per-standard FIT equation evaluates against. Declared once in the
/// board's safety block — either a single `ambient` (one implicit
/// phase) or a named `profile` / inline `phase { }` histogram; the
/// engine computes the time-weighted λ over powered phases.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mission {
    /// Ambient temperature in °C (single-phase shorthand; ignored when
    /// `phases` is non-empty).
    pub ambient_c: f64,
    /// Powered hours per year (8760 = always on).
    pub on_hours: Option<f64>,
    /// Power on/off cycles per year (thermal-cycling term).
    pub cycles: Option<f64>,
    /// Environment symbol for π_E lookups (MIL-HDBK-217F Table 3-2
    /// vocabulary: GB, GF, GM, NS, …). Absent ⇒ engine default "GB",
    /// printed in the basis.
    pub environment: Option<String>,
    /// Quality level for π_Q lookups (S/R/P/M/mil_spec/lower). Absent ⇒
    /// engine default "lower" (COTS), printed in the basis.
    pub quality: Option<String>,
    /// Named profile to resolve from mission_profiles.toml (project
    /// tunable — "passenger_compartment", "motor_control", …).
    /// Explicit mission items override the profile's fields.
    pub profile: Option<String>,
    /// Temperature/time histogram. Empty ⇒ one implicit phase at
    /// `ambient_c`.
    pub phases: Vec<MissionPhase>,
    /// λ averaging basis: "operating" (default — λ per operating hour,
    /// the FMEDA/PMHF convention) or "calendar" (unpowered time counts
    /// as zero-rate time).
    pub time_basis: Option<String>,
    /// Service lifetime in operating hours (`lifetime = 15000h`) — the
    /// exposure window of the dual-point PMHF term. Absent ⇒ PMHF stays
    /// the single-point approximation, stated.
    #[serde(default)]
    pub lifetime_h: Option<f64>,
}

/// One automatically-generated fault in the whole-universe campaign
/// (Phase 3): a part × standard failure mode, classified on the faulted
/// operating point.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UniverseFault {
    /// Owning scope path ("" = board).
    pub scope: String,
    pub part: String,
    /// "short" | "open" | "open_pin" | "state".
    pub mode: String,
    pub targets: Vec<String>,
    pub ran: bool,
    /// Effect paths that fired on the faulted point.
    pub fired: Vec<String>,
    /// Mechanism handles whose detected_when was TRUE.
    pub detected: Vec<String>,
    /// Detection asserted with NO dangerous effect (mechanism self-fault
    /// or spurious trip).
    pub false_alarm: bool,
    /// LATENT: this fault alone is neither dangerous nor annunciated,
    /// but the double-fault probe showed it defeats the detection of an
    /// otherwise-detected dangerous fault (ISO 26262 multi-point latent).
    #[serde(default)]
    pub latent: bool,
    /// Σ λ (FIT) of the detected-dangerous faults whose detection this
    /// latent fault defeats — the exposure of the dual-point PMHF term.
    #[serde(default)]
    pub latent_exposed_fit: f64,
    /// λ share of this mode in FIT (part FIT split over its modes), when
    /// the part's FIT was computed.
    pub weight_fit: Option<f64>,
    pub note: Option<String>,
}

/// FMEDA metrics for one scope (ISO 26262-5; the identical residual
/// arithmetic yields IEC 61508's SFF for SIL-level goals). All λ in
/// FIT, all from the MEASURED fault universe — nothing assumed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    /// Σ weight of universe faults that ran with a computed λ share.
    pub lambda_total_fit: f64,
    /// Dangerous and undetected (single-point/residual — the measured
    /// campaign cannot distinguish ISO's SPF from RF; both are counted).
    pub lambda_residual_fit: f64,
    /// Latent multi-point λ (double-fault probe).
    pub lambda_latent_fit: f64,
    /// Universe faults that could NOT enter the measurement (no λ share
    /// or not run) — metrics are incomplete unless this is 0.
    pub unmeasured_faults: usize,
    /// SPFM = 1 − λ_residual/λ_total (ISO 26262-5 §8.4.5).
    pub spfm: f64,
    /// LFM = 1 − λ_latent/(λ_total − λ_residual) (ISO 26262-5 §8.4.6).
    pub lfm: f64,
    /// PMHF in FIT: λ_residual plus, when the mission declares a
    /// service lifetime, the dual-point term
    /// Σ_L λ_L·λ_exposed·T_life/2 over the latent faults (second-order
    /// approximation, ISO 26262-10 §8.3.3 shape). Without a lifetime it
    /// is the single-point approximation — stated, not hidden.
    pub pmhf_fit: f64,
    /// The dual-point contribution inside pmhf_fit; None ⇒ no mission
    /// lifetime declared (single-point approximation).
    #[serde(default)]
    pub pmhf_dual_fit: Option<f64>,
    /// The strictest goal level in the scope that carries targets.
    pub target_level: Option<Level>,
    /// (spfm_min, lfm_min, pmhf_max_fit) for that level, if any.
    pub targets: Option<(f64, f64, f64)>,
    /// Pass verdict against the targets; None when no targets apply or
    /// the measurement is incomplete.
    pub pass: Option<bool>,
}

/// Architectural-metric targets: (SPFM/SFF min, LFM min, PMHF/PFH max
/// in FIT).
///
/// ASIL levels: ISO 26262-5:2018 Tables 4, 5 and 6. ASIL A and QM
/// carry no normative targets.
///
/// SIL levels: SFF = 1 − λ_DU/λ_total is the SAME residual arithmetic
/// as SPFM; thresholds per IEC 61508-2:2010 Table 3 assuming a
/// **Type A subsystem with HFT = 0** (single-channel, simple
/// components — the assumption is printed in the report; a Type B or
/// redundant architecture needs its own row). PFH limits per IEC
/// 61508-1:2010 Table 3 (high demand / continuous mode), expressed in
/// FIT (1 FIT = 1e-9/h). IEC has no LFM equivalent — the LFM floor is
/// 0 for SIL rows.
pub fn metric_targets(level: Level) -> Option<(f64, f64, f64)> {
    match level {
        Level::AsilB => Some((0.90, 0.60, 100.0)), // ISO 26262-5:2018 T4/T5/T6
        Level::AsilC => Some((0.97, 0.80, 100.0)),
        Level::AsilD => Some((0.99, 0.90, 10.0)),
        // IEC 61508: SFF (61508-2 T3, Type A HFT=0) + PFH (61508-1 T3)
        Level::Sil1 => Some((0.0, 0.0, 10_000.0)), // <1e-5/h; no SFF floor at HFT=0
        Level::Sil2 => Some((0.60, 0.0, 1_000.0)), // <1e-6/h
        Level::Sil3 => Some((0.90, 0.0, 100.0)),   // <1e-7/h
        Level::Sil4 => Some((0.99, 0.0, 10.0)),    // <1e-8/h
        Level::QM | Level::AsilA => None,
    }
}

/// The whole model for one top-level board.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SafetyModel {
    pub board: String,
    /// Board-level mission profile, if declared.
    pub mission: Option<Mission>,
    pub scopes: Vec<Scope>,
    pub parts: Vec<Part>,
    /// Whole-universe campaign results (Phase 3), empty until it runs.
    #[serde(default)]
    pub universe: Vec<UniverseFault>,
    pub gaps: Vec<Gap>,
    /// Hard errors found while resolving (unknown handle, unbound formal…).
    pub errors: Vec<String>,
}

impl SafetyModel {
    pub fn verdict_pass(&self) -> bool {
        self.errors.is_empty() && self.gaps.is_empty()
    }

    /// Stable, sorted view for the baseline/delta (§5): everything a
    /// later build can be diffed against, keyed by path.
    pub fn baseline(&self) -> Baseline {
        let mut b = Baseline::default();
        for p in &self.parts {
            b.parts.insert(p.instance.clone(), format!("{} {:?}", p.type_name, p.data));
        }
        for s in &self.scopes {
            for g in &s.goals {
                b.goals.insert(g.path.clone(), format!("{} {}", g.level.as_str(), g.title));
                for e in &g.effects {
                    b.effects.insert(format!("{}.{}", g.path, e.name), e.expr.clone());
                }
            }
            for m in &s.mechanisms {
                b.mechanisms.insert(
                    format!("{}:{}", m.goal, m.instance),
                    format!("{:?} detects={:?} dc={:?}", m.kind, m.detects, m.claimed_dc),
                );
            }
            for a in &s.assumptions {
                b.assumptions.insert(a.path.clone(), format!("{:?}", a.status));
            }
            for f in &s.faults {
                b.faults.insert(
                    format!("{}:{}({})", s.path, f.kind, f.targets.join(",")),
                    format!("expect {} run={}", f.expect, f.run),
                );
            }
        }
        for g in &self.gaps {
            b.gaps.insert(format!("{}:{}:{}", g.class.as_str(), g.goal, g.subject), g.fix.clone());
        }
        b.verdict_pass = self.verdict_pass();
        b
    }
}

/// Diffable snapshot (docs/spec/Functional_Safety.md §5). BTreeMaps so
/// serialisation is deterministic.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Baseline {
    pub parts: BTreeMap<String, String>,
    pub goals: BTreeMap<String, String>,
    pub effects: BTreeMap<String, String>,
    pub mechanisms: BTreeMap<String, String>,
    pub assumptions: BTreeMap<String, String>,
    pub faults: BTreeMap<String, String>,
    pub gaps: BTreeMap<String, String>,
    pub verdict_pass: bool,
}

/// One section of a baseline delta.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct DeltaSection {
    pub added: Vec<(String, String)>,
    pub removed: Vec<(String, String)>,
    pub changed: Vec<(String, String, String)>,
}

impl DeltaSection {
    fn of(old: &BTreeMap<String, String>, new: &BTreeMap<String, String>) -> DeltaSection {
        let mut d = DeltaSection::default();
        for (k, v) in new {
            match old.get(k) {
                None => d.added.push((k.clone(), v.clone())),
                Some(ov) if ov != v => d.changed.push((k.clone(), ov.clone(), v.clone())),
                _ => {}
            }
        }
        for (k, v) in old {
            if !new.contains_key(k) {
                d.removed.push((k.clone(), v.clone()));
            }
        }
        d
    }
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.removed.is_empty() && self.changed.is_empty()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Delta {
    pub parts: DeltaSection,
    pub goals: DeltaSection,
    pub effects: DeltaSection,
    pub mechanisms: DeltaSection,
    pub assumptions: DeltaSection,
    pub faults: DeltaSection,
    pub gaps: DeltaSection,
    pub verdict_before: bool,
    pub verdict_after: bool,
}

impl Delta {
    pub fn between(old: &Baseline, new: &Baseline) -> Delta {
        Delta {
            parts: DeltaSection::of(&old.parts, &new.parts),
            goals: DeltaSection::of(&old.goals, &new.goals),
            effects: DeltaSection::of(&old.effects, &new.effects),
            mechanisms: DeltaSection::of(&old.mechanisms, &new.mechanisms),
            assumptions: DeltaSection::of(&old.assumptions, &new.assumptions),
            faults: DeltaSection::of(&old.faults, &new.faults),
            gaps: DeltaSection::of(&old.gaps, &new.gaps),
            verdict_before: old.verdict_pass,
            verdict_after: new.verdict_pass,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.parts.is_empty()
            && self.goals.is_empty()
            && self.effects.is_empty()
            && self.mechanisms.is_empty()
            && self.assumptions.is_empty()
            && self.faults.is_empty()
            && self.gaps.is_empty()
            && self.verdict_before == self.verdict_after
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn levels_parse_and_classify() {
        assert_eq!(Level::parse("ASIL_B"), Some(Level::AsilB));
        assert_eq!(Level::parse("sil3"), Some(Level::Sil3));
        assert_eq!(Level::parse("QM"), Some(Level::QM));
        assert_eq!(Level::parse("ASIL_E"), None);
        assert_eq!(Level::AsilB.standard(), Standard::Iso26262);
        assert_eq!(Level::Sil2.standard(), Standard::Iec61508);
        assert!(!Level::AsilB.requires_lsm());
        assert!(Level::AsilC.requires_lsm());
        assert!(Level::Sil3.requires_lsm());
    }

    #[test]
    fn delta_reports_added_removed_changed() {
        let mut a = Baseline::default();
        a.parts.insert("r1".into(), "Res".into());
        a.parts.insert("r2".into(), "Res".into());
        let mut b = a.clone();
        b.parts.remove("r2");
        b.parts.insert("r3".into(), "Res".into());
        b.parts.insert("r1".into(), "Res waived".into());
        let d = Delta::between(&a, &b);
        assert_eq!(d.parts.added.len(), 1);
        assert_eq!(d.parts.removed.len(), 1);
        assert_eq!(d.parts.changed.len(), 1);
        assert!(!d.is_empty());
        assert!(Delta::between(&a, &a).is_empty());
    }
}
