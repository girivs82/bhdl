//! Simulation-refined margin & sign-off report.
//!
//! Spec: `docs/spec/Simulation_Margin_Signoff.md`.
//!
//! Stage 4 of the sizing pipeline: after the E-series snap, re-solve the
//! *snapped* netlist with GLACIER and report each passive's margin
//! (`rating / derated_stress`) against the catalogue rating the BOM
//! actually selected. This module only MEASURES and REPORTS — it changes
//! no values. The stepping loop (#5) builds on the same margin computation.
//!
//! The derate factors mirror `bhdl_analyzer::value_snap` (the convention
//! the catalogue selection already enforces); margin is measured *on top
//! of* the derate, so `margin >= 1.0` means the part clears its derated
//! gate and `margin >= SIGNOFF_MARGIN` means it does so with target
//! head-room.

use crate::glacier_physical_selection::{
    classify_component, compute_instance_max_voltages,
};
use bhdl_netlist::Netlist;
use bhdl_netlist::types::NetClass;
use bhdl_common::stress::StressRecipe;
use crate::stress_evaluator::{evaluate_stress_recipe, StressInputs};
use std::collections::HashMap;

/// Derate factors — kept in sync with `value_snap.rs`.
const CAP_VOLTAGE_DERATE: f64 = 2.0;
const RES_POWER_DERATE: f64 = 2.0;
const IND_CURRENT_DERATE: f64 = 1.25;

/// Target head-room ABOVE the derated gate for a clean sign-off (a further
/// 20 % beyond the derate). A part between `1.0` and this is populated but
/// flagged tight.
const SIGNOFF_MARGIN: f64 = 1.2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// `margin >= SIGNOFF_MARGIN`.
    SignedOff,
    /// `1.0 <= margin < SIGNOFF_MARGIN` — passes the derate gate, tight.
    UnderMargin,
    /// `margin < 1.0` — fails the derate gate (over-stressed).
    OverStress,
    /// No simulated stress and/or no selected rating to compare.
    NoData,
}

impl Verdict {
    pub fn label(self) -> &'static str {
        match self {
            Verdict::SignedOff => "SIGNED-OFF",
            Verdict::UnderMargin => "UNDER-MARGIN",
            Verdict::OverStress => "OVER-STRESS",
            Verdict::NoData => "—",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SignoffRow {
    pub refdes: String,
    pub class: String,
    /// Stress axis for this class: `"V"`, `"P"`, or `"I"`.
    pub axis: &'static str,
    pub value: String,
    /// Raw measured stress in SI base units (V / W / A).
    pub stress: Option<f64>,
    /// `stress * derate`.
    pub derated: Option<f64>,
    /// Selected part rating in SI base units.
    pub rating: Option<f64>,
    /// `rating / derated`.
    pub margin: Option<f64>,
    pub verdict: Verdict,
    /// True if the part was left do-not-populate (over-stress with no part).
    pub dnp: bool,
    /// Ripple-model annotation when an analytic switcher model applied
    /// (e.g. `"ΔI_L=1.35A, I_pk=3.68A"`, `"ΔV_out=9mV"`), else `None`.
    pub ripple: Option<String>,
    /// Stage-C value-stepping recommendation when the part is over its ripple
    /// target (e.g. `"4.7µH → 6.8µH (ratio 0.41→0.28)"`), else `None`.
    pub step: Option<String>,
}

/// Operating point of a switching (buck) converter, recovered from the
/// netlist for the analytic **reference ripple model**. This is form-3
/// ("our own analytic composition") of the `simulation {}` provenance
/// ladder (`Vendor_Simulation_Blocks.md` §5.1.1): the in-tree fallback a
/// vendor simulation model can later supersede. The closed forms are the
/// same TI buck equations the `design {}` block uses to *seed* the values,
/// here run forward on the snapped values to *check* them.
struct SwitcherOp {
    v_in: f64,
    v_out: f64,
    /// The actual per-rail load on the regulated output, taken from the output
    /// rail's source-declared budget (`power VOUT = V @ I`). `None` ⇒ the rail
    /// declares no load ⇒ the i_out-dependent stresses (inductor peak current,
    /// ripple-ratio stepping, input-cap ripple) are reported UNCHECKED.
    /// Real-Data Policy: never the regulator's *rated* output current (a
    /// capability, not the load) as a proxy.
    i_out: Option<f64>,
    f_sw: f64,
    duty: f64,
    /// Target inductor ripple ratio ΔI_L/I_out from the regulator's
    /// `ripple_ratio` datasheet attribute. `None` ⇒ not declared ⇒ inductor
    /// value-stepping is not attempted (Real-Data Policy: no default).
    ripple_ratio: Option<f64>,
    /// Control-loop crossover constant K in `f_co = K / (V_out·C_out)` (the
    /// device's `loop_crossover_k` datasheet constant). `None` ⇒ no loop model
    /// declared ⇒ stability reported unchecked.
    loop_k: Option<f64>,
    /// Crossover must stay below `f_sw · loop_ratio` (datasheet ≈ f_sw/10).
    /// `None` ⇒ not declared ⇒ stability unchecked.
    loop_ratio: Option<f64>,
    /// Feedback reference voltage (regulation point), for divider-top
    /// detection. `None` ⇒ not declared ⇒ stability unchecked.
    v_ref: Option<f64>,
}

/// Recover the buck operating point from the netlist: `f_sw` and `i_out`
/// from the switching regulator's attributes, `v_in`/`v_out` from the
/// declared power rails (highest rail = input, next = regulated output).
/// Returns `None` for non-switching topologies or when an input is missing
/// — the parts then keep their generic DC stress (ripple is additive).
fn recover_switcher_op(
    netlist: &Netlist,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Option<SwitcherOp> {
    // The regulator's class/topology/f_sw are declared on the stdlib ENTITY.
    // For an entity WITHOUT an expansion/design block they are never stamped
    // onto the netlist instance or module (only `entity_attribute_index`
    // carries them), so look up each key in three places, instance first:
    //   instance attrs → module attrs → entity_attribute_index[entity name].
    let (f_sw, ripple_ratio, loop_k, loop_ratio, v_ref) =
        netlist.instances.values().find_map(|inst| {
        let module = netlist.modules.get(inst.definition);
        let ent = module.and_then(|m| entity_attrs.get(&m.name));
        let get = |k: &str| {
            inst.attributes
                .get(k)
                .or_else(|| module.and_then(|m| m.attributes.get(k)))
                .or_else(|| ent.and_then(|e| e.get(k)))
        };
        let class = get("component_class").map(String::as_str).unwrap_or("");
        let topo = get("topology").map(String::as_str).unwrap_or("");
        let rtype = get("regulator_type").map(String::as_str).unwrap_or("");
        let is_switcher = class == "switching_regulator"
            || topo == "buck"
            || rtype.contains("buck")
            || (class == "voltage_regulator"
                && (get("switching_frequency").is_some() || get("f_sw").is_some()));
        if !is_switcher {
            return None;
        }
        let f_sw = get("switching_frequency")
            .or_else(|| get("f_sw"))
            .and_then(|s| parse_si(s))
            .filter(|f| *f > 0.0)?;
        // NOTE: I_out is deliberately NOT taken from the regulator's rated
        // `output_current` — that is the device's *capability*, not the actual
        // load. The real per-rail load is the OUTPUT rail's declared `@ I`
        // budget, read below from the netlist (Real-Data Policy: a different
        // real value must not be substituted as a proxy).
        // Real-Data Policy: each of these is the device's own datasheet
        // attribute or `None` — never a fabricated default. A `None` makes the
        // dependent check (stepping / stability) report unchecked, not run on a
        // guessed value.
        let ripple_ratio = get("ripple_ratio")
            .and_then(|s| parse_si(s))
            .filter(|r| *r > 0.0);
        let loop_k = get("loop_crossover_k")
            .and_then(|s| parse_si(s))
            .filter(|k| *k > 0.0);
        let loop_ratio = get("loop_crossover_max_ratio")
            .and_then(|s| parse_si(s))
            .filter(|r| *r > 0.0);
        let v_ref = get("feedback_voltage")
            .or_else(|| get("v_ref"))
            .and_then(|s| parse_si(s))
            .filter(|v| *v > 0.0);
        Some((f_sw, ripple_ratio, loop_k, loop_ratio, v_ref))
    })?;
    // Power rails carry their source-declared per-rail load budget (`@ I`) on
    // the net class. V_in = the highest rail; V_out = the highest rail strictly
    // below it; `i_out` = the OUTPUT rail's declared current — the actual load,
    // or `None` when that rail omits `@ I` (→ i_out-dependent stresses UNCHECKED).
    let rails: Vec<(f64, Option<f64>)> = netlist
        .nets
        .values()
        .filter_map(|net| match &net.net_class {
            NetClass::Power { voltage, current } if *voltage > 0.0 => Some((*voltage, *current)),
            _ => None,
        })
        .collect();
    if rails.len() < 2 {
        return None;
    }
    let v_in = rails.iter().map(|(v, _)| *v).fold(f64::MIN, f64::max);
    let (v_out, i_out) = match rails
        .iter()
        .filter(|(v, _)| *v < v_in - 1e-9)
        .max_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    {
        Some(&(v, i)) => (v, i),
        None => return None,
    };
    if !(v_out > 0.0 && v_out < v_in) {
        return None;
    }
    Some(SwitcherOp {
        v_in,
        v_out,
        i_out,
        f_sw,
        duty: v_out / v_in,
        ripple_ratio,
        loop_k,
        loop_ratio,
        v_ref,
    })
}

/// Smallest standard E12 value ≥ `target` (a ceil onto the E12 ladder),
/// preserving decade. Used to step a reactive value UP to the next standard
/// part — Stage C steps the inductor up until its ripple ratio meets target.
fn e12_ceil(target: f64) -> f64 {
    const E12: [f64; 12] = [
        1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2,
    ];
    if !(target > 0.0) {
        return target;
    }
    let decade = 10f64.powf(target.log10().floor());
    for b in E12 {
        let v = b * decade;
        if v >= target * (1.0 - 1e-9) {
            return v;
        }
    }
    10.0 * decade // top of decade → next decade's 1.0
}

/// Stage-C inductor value-stepping. If the output inductor's ripple ratio
/// ΔI_L/I_out exceeds the target, return the E12 value it should step UP to so
/// the ratio meets target, with the resulting ratio. `L_target =
/// (V_in−V_out)·D / (f_sw · ripple_ratio · I_out)`, ceiled onto E12. Larger L ⇒
/// less ripple, strictly monotone and stability-benign, so no search is needed.
/// Returns `(l_step, from_ratio, new_ratio, target)`, or `None` when the value
/// already meets the target, OR no `ripple_ratio` is declared, OR the output
/// rail declares no load (`op.i_out` is `None`) — all Real-Data Policy: no
/// default target and no proxy load.
fn inductor_value_step(op: &SwitcherOp, l_current: f64, d_il: f64) -> Option<(f64, f64, f64, f64)> {
    let target = op.ripple_ratio?;
    let i_out = op.i_out?; // no declared load ⇒ ratio is UNCHECKED ⇒ no step
    let ratio = d_il / i_out;
    if ratio <= target + 1e-9 {
        return None; // already within target
    }
    let l_target = (op.v_in - op.v_out) * op.duty / (op.f_sw * target * i_out);
    let l_step = e12_ceil(l_target.max(l_current));
    let d_il_new = (op.v_in - op.v_out) * op.duty / (op.f_sw * l_step);
    Some((l_step, ratio, d_il_new / i_out, target))
}

/// The output inductor's ripple current `ΔI_L = (V_in−V_out)·D / (f_sw·L)`,
/// found from the (first) inductor's snapped value. Shared by the inductor
/// peak-current and the output-cap ripple-voltage derivations.
fn inductor_ripple_current(netlist: &Netlist, op: &SwitcherOp) -> Option<f64> {
    netlist.instances.values().find_map(|inst| {
        if classify_component(netlist, inst.definition, &inst.attributes).as_deref()
            == Some("inductor")
        {
            let l = parse_si(inst.attributes.get("value")?).filter(|l| *l > 0.0)?;
            Some((op.v_in - op.v_out) * op.duty / (op.f_sw * l))
        } else {
            None
        }
    })
}

/// Parse a rating/stress attribute like `"50V"`, `"250mW"`, `"2A"`, `"0.1"`
/// into SI base units. Reuses the general SI-suffix value parser, which
/// ignores the trailing unit letter.
fn parse_si(s: &str) -> Option<f64> {
    bhdl_analyzer::value_snap::parse_value_string(s.trim())
}

/// Per-instance list of its connected nets' DC voltages — used to identify a
/// part's role from the operating point (an output cap touches V_out, the
/// divider-top resistor touches V_out and V_ref, etc.).
fn instance_net_voltages(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
) -> HashMap<String, Vec<f64>> {
    let mut out: HashMap<String, Vec<f64>> = HashMap::new();
    for (_id, net) in netlist.nets.iter() {
        let Some(name) = &net.name else { continue };
        let Some(v) = net_voltages.get(name).copied() else {
            continue;
        };
        for conn in &net.connections {
            let inst_id = match conn {
                bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) => Some(*iid),
                bhdl_netlist::ConnectionPoint::PinInstance(pi) => {
                    netlist.pin_instances.get(*pi).map(|p| p.instance)
                }
                _ => None,
            };
            if let Some(iid) = inst_id {
                if let Some(inst) = netlist.instances.get(iid) {
                    out.entry(inst.name.clone()).or_default().push(v);
                }
            }
        }
    }
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StabilityVerdict {
    /// Loop has adequate phase margin per the datasheet criterion.
    Stable,
    /// ESR zero well above crossover with no feedforward cap — low phase margin.
    LowMargin,
    /// Crossover above the datasheet target (f_sw·loop_ratio) — too fast.
    FastCrossover,
    /// A required real-data value (output-cap ESR or even the cap *type*) is not
    /// available — per the Real-Data Policy we never substitute a fabricated
    /// value, so the phase margin cannot be verified.
    Unchecked,
}

/// On what basis the output-cap ESR zero (the loop's phase-boost term) was
/// established — the heart of the ceramic-vs-bulk split.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EsrBasis {
    /// Computed numerically from each output cap's REAL published ESR
    /// (electrolytic / tantalum / polymer — DigiKey carries these). The ESR
    /// zero `f_z(esr)` is a real number.
    Real,
    /// The output bank contains a ceramic (MLCC). Ceramic ESR is structurally
    /// single-digit mΩ, so `f_z(esr)/f_co = V_out/(2π·ESR·K)` is provably ≫ 1
    /// (and is INDEPENDENT of C_out) — the ESR zero sits far above crossover
    /// and provides no phase boost there, REGARDLESS of the exact mΩ. This is a
    /// structural inequality, not a fabricated number: phase margin must come
    /// from a feedforward cap, not the ESR zero. No numeric `f_z(esr)`.
    CeramicStructural,
    /// At least one output cap's ESR is unknown AND its type is not identifiable
    /// as ceramic — neither a real ESR nor the structural ceramic argument
    /// applies, so the phase margin is genuinely UNCHECKED.
    Unchecked,
}

/// Control-loop stability assessment (analytic, datasheet model). For an
/// internally-compensated current-mode buck (TPS54302 class) whose internal
/// comp values TI does not publish, the loop is characterised by TI's own
/// closed-form equations: crossover `f_co = K/(V_out·C_out)` (Eq 14), the
/// `f_co < f_sw·loop_ratio` target, and the low-ESR-ceramic phase-margin
/// criterion that calls for a feedforward cap `C_ff = 1/(2π·f_co·R_top)`
/// (Eq 16). The constant `K` and ratio are declared by the device.
#[derive(Debug, Clone)]
pub struct StabilityResult {
    pub f_co: f64,
    pub f_sw: f64,
    /// Datasheet crossover target f_sw·loop_ratio (real, not 0.1·f_sw).
    pub crossover_target: f64,
    pub crossover_ok: bool,
    /// ESR zero `1/(2π·ESR·C_out)` of the output bank — `Some` only under
    /// `EsrBasis::Real`. `None` for the ceramic-structural case (the zero is
    /// known-high but not computed to a number) and for `Unchecked`.
    pub f_z_esr: Option<f64>,
    /// How the ESR-zero phase-boost term was established (real / ceramic-
    /// structural / unchecked).
    pub esr_basis: EsrBasis,
    pub ff_present: bool,
    pub r_top: Option<f64>,
    pub c_ff_required: Option<f64>,
    /// Output-cap refdes whose ESR *and* type are both unknown (Real-Data
    /// Policy → UNCHECKED). A ceramic with no numeric ESR is NOT listed here —
    /// it's handled structurally.
    pub missing_esr: Vec<String>,
    pub verdict: StabilityVerdict,
}

/// Ceramic (Class-I/II) dielectric codes. A cap tagged with one of these has
/// structurally low ESR (single-digit mΩ) — enough to place its ESR zero far
/// above any practical crossover (see `EsrBasis::CeramicStructural`).
const CERAMIC_DIELECTRICS: &[&str] = &[
    "C0G", "NP0", "U2J", "X5R", "X6S", "X6T", "X7R", "X7S", "X7T", "X8R", "X8L",
    "Y5V", "Z5U", "Y5U",
];

/// Is this capacitor a ceramic (MLCC)? Decided ONLY from the real `dielectric`
/// attribute — the dielectric of the part actually selected from the catalogue
/// (glacier stamps it from the chosen MPN). Real-Data Policy: the sizer's
/// `dielectric_hint` is a *sourcing recommendation* (which dielectric to look
/// for), NOT a measured property of a chosen part, so it must never feed the
/// ESR-zero stability verdict — a cap carrying only a hint is treated as
/// unknown (→ UNCHECKED), never as a structurally-negligible-ESR ceramic.
/// `None`-dielectric ⇒ not identifiable as ceramic (caller treats as unknown).
fn cap_is_ceramic(inst: &bhdl_netlist::Instance) -> bool {
    let d = inst.attributes.get("dielectric");
    match d {
        Some(s) => {
            let up = s.to_ascii_uppercase();
            CERAMIC_DIELECTRICS.iter().any(|c| up.contains(c))
        }
        None => false,
    }
}

/// Evaluate loop stability from the netlist + operating point. Returns `None`
/// when there is no switcher, no declared loop constant (`loop_crossover_k`),
/// or no identifiable output cap bank — i.e. stability is then *unchecked*,
/// never silently "passed".
pub fn compute_stability(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Option<StabilityResult> {
    use std::f64::consts::PI;
    let op = recover_switcher_op(netlist, entity_attrs)?;
    // Real-Data Policy: the loop model exists only if the device declares all
    // of K, the crossover ratio, and V_ref. Any absent ⇒ no loop model ⇒ no
    // stability section (distinct from ESR-data-missing, which is UNCHECKED).
    let k = op.loop_k?;
    let loop_ratio = op.loop_ratio?;
    let v_ref = op.v_ref?;
    let inv = instance_net_voltages(netlist, net_voltages);
    let near = |a: f64, b: f64| (a - b).abs() < 0.1 * b.max(1.0);
    let role_is = |inst: &bhdl_netlist::Instance, class: &str| {
        classify_component(netlist, inst.definition, &inst.attributes).as_deref() == Some(class)
    };

    // Output capacitor bank. Sum C, and classify each cap's ESR-zero basis
    // (Real-Data Policy, docs/spec/Real_Data_Policy.md):
    //   • REAL ESR (electrolytic/tantalum/polymer): combine 1/ESR in parallel
    //     for a numeric bank ESR zero. NEVER a dielectric/package estimate, and
    //     NOT `sim_max_esr` (that is the max-ESR a part must beat).
    //   • CERAMIC (MLCC, identified by dielectric): no numeric ESR needed — its
    //     ESR zero is structurally ≫ crossover (f_z/f_co = V_out/(2π·ESR·K) is
    //     ≫1 and C_out-independent for single-digit-mΩ ESR), so it provides no
    //     phase boost. A real inequality, not a fabricated number.
    //   • UNKNOWN (no real ESR, not identifiable as ceramic): genuinely
    //     UNCHECKED — we won't guess the type or the ESR.
    let mut c_out = 0.0f64;
    let mut g_esr = 0.0f64; // Σ 1/ESR_i over caps with REAL ESR
    let mut real_esr_count = 0usize;
    let mut ceramic_present = false;
    let mut missing_esr: Vec<String> = Vec::new();
    for inst in netlist.instances.values() {
        if !role_is(inst, "capacitor") {
            continue;
        }
        let Some(vs) = inv.get(&inst.name) else { continue };
        if vs.iter().any(|v| near(*v, op.v_out)) && !vs.iter().any(|v| near(*v, op.v_in)) {
            let Some(c) = inst.attributes.get("value").and_then(|s| parse_si(s)) else {
                continue;
            };
            c_out += c;
            match inst.attributes.get("esr").and_then(|s| parse_si(s)).filter(|e| *e > 0.0) {
                Some(esr) => { g_esr += 1.0 / esr; real_esr_count += 1; }
                None if cap_is_ceramic(inst) => ceramic_present = true,
                None => missing_esr.push(inst.name.clone()),
            }
        }
    }
    if !(c_out > 0.0) {
        return None;
    }

    let f_co = k / (op.v_out * c_out); // needs only the real C_out
    let crossover_target = op.f_sw * loop_ratio;
    let crossover_ok = f_co < crossover_target;

    // Divider-top resistor: a resistor touching V_out and V_ref (the FB node).
    let r_top = netlist.instances.values().find_map(|inst| {
        if !role_is(inst, "resistor") {
            return None;
        }
        let vs = inv.get(&inst.name)?;
        if vs.iter().any(|v| near(*v, op.v_out)) && vs.iter().any(|v| near(*v, v_ref)) {
            inst.attributes.get("value").and_then(|s| parse_si(s))
        } else {
            None
        }
    });
    // Feedforward cap: a cap across the divider top (touches V_out and V_ref).
    let ff_present = netlist.instances.values().any(|inst| {
        role_is(inst, "capacitor")
            && inv.get(&inst.name).is_some_and(|vs| {
                vs.iter().any(|v| near(*v, op.v_out)) && vs.iter().any(|v| near(*v, v_ref))
            })
    });

    // An output cap whose ESR *and* type are both unknown ⇒ genuinely
    // UNCHECKED (can't apply the real ESR zero nor the ceramic argument).
    if !missing_esr.is_empty() {
        return Some(StabilityResult {
            f_co,
            f_sw: op.f_sw,
            crossover_target,
            crossover_ok,
            f_z_esr: None,
            esr_basis: EsrBasis::Unchecked,
            ff_present,
            r_top,
            c_ff_required: None,
            missing_esr,
            verdict: StabilityVerdict::Unchecked,
        });
    }

    // Establish the ESR-zero phase-boost term.
    //   • Ceramic in the bank ⇒ its low ESR dominates the bank's HF impedance,
    //     so the bank ESR zero is structurally ≫ crossover (no numeric ESR
    //     needed, and it holds even mixed with bulk caps).
    //   • Otherwise all output caps have REAL ESR ⇒ compute the bank ESR zero.
    let (f_z_esr, esr_basis, esr_zero_high) = if ceramic_present {
        (None, EsrBasis::CeramicStructural, true)
    } else {
        // real_esr_count > 0 here (c_out > 0 and missing/ceramic both empty)
        let fz = 1.0 / (2.0 * PI * (1.0 / g_esr) * c_out);
        (Some(fz), EsrBasis::Real, fz > 10.0 * f_co)
    };
    // The ESR zero boosts phase at crossover only if it sits at/below it; ≫
    // crossover (low/ceramic ESR) ⇒ no boost ⇒ low margin without a feedforward
    // cap. This verdict is now real for ceramics, not UNCHECKED.
    let c_ff_required = if esr_zero_high && !ff_present {
        r_top.map(|rt| 1.0 / (2.0 * PI * f_co * rt))
    } else {
        None
    };
    let verdict = if !crossover_ok {
        StabilityVerdict::FastCrossover
    } else if esr_zero_high && !ff_present {
        StabilityVerdict::LowMargin
    } else {
        StabilityVerdict::Stable
    };

    Some(StabilityResult {
        f_co,
        f_sw: op.f_sw,
        crossover_target,
        crossover_ok,
        f_z_esr,
        esr_basis,
        ff_present,
        r_top,
        c_ff_required,
        missing_esr: Vec::new(),
        verdict,
    })
}

/// Render the stability assessment as a report section.
pub fn format_stability(s: &StabilityResult) -> String {
    let mut out = String::from("\n## Control-loop stability (analytic, datasheet model)\n\n");
    let verdict = match s.verdict {
        StabilityVerdict::Stable => "STABLE",
        StabilityVerdict::LowMargin => "LOW PHASE MARGIN",
        StabilityVerdict::FastCrossover => "CROSSOVER TOO FAST",
        StabilityVerdict::Unchecked => "UNCHECKED",
    };
    out.push_str(&format!(
        "- crossover f_co = {} (target < {} = f_sw·loop_ratio): {}\n",
        fmt_si(s.f_co, "Hz"),
        fmt_si(s.crossover_target, "Hz"),
        if s.crossover_ok { "OK" } else { "OVER" },
    ));
    match s.esr_basis {
        EsrBasis::Real => {
            let fz = s.f_z_esr.unwrap_or(0.0);
            let high = fz > 10.0 * s.f_co;
            out.push_str(&format!(
                "- ESR zero f_z(esr) = {} (real datasheet ESR; {} crossover ⇒ {}); feedforward cap {}\n",
                fmt_si(fz, "Hz"),
                if high { "≫" } else { "≤" },
                if high { "no phase boost there" } else { "adds phase boost" },
                if s.ff_present { "present" } else { "absent" },
            ));
        }
        EsrBasis::CeramicStructural => out.push_str(&format!(
            "- ESR zero: ceramic output bank ⇒ structurally ≫ crossover \
             (f_z/f_co = V_out/(2π·ESR·K) ≫ 1, C_out-independent) ⇒ no phase boost; \
             phase margin must come from a feedforward cap ({})\n",
            if s.ff_present { "present" } else { "absent" },
        )),
        EsrBasis::Unchecked => out.push_str(&format!(
            "- ESR zero: **not computable — output cap(s) {} have neither a real ESR \
             nor an identifiable ceramic dielectric**\n",
            s.missing_esr.join(", "),
        )),
    }
    out.push_str(&format!("- **verdict: {verdict}**\n"));
    if let Some(cff) = s.c_ff_required {
        out.push_str(&format!(
            "_Add a feedforward cap C_ff ≈ {} across the FB divider top{} to boost \
             phase margin (datasheet Eq 16). Until then loop stability is marginal._\n",
            fmt_si(cff, "F"),
            s.r_top
                .map(|rt| format!(" (R_top={})", fmt_si(rt, "Ω")))
                .unwrap_or_default(),
        ));
    }
    if s.verdict == StabilityVerdict::Unchecked {
        out.push_str(
            "_Phase margin is UNCHECKED: an output cap has neither a real published ESR \
             (electrolytic/tantalum/polymer — sourced via the DigiKey provider) nor an \
             identifiable ceramic dielectric (which would make the ESR zero structurally \
             negligible). Per the Real-Data Policy no value is fabricated; tag the cap's \
             dielectric or source a real ESR._\n",
        );
    }
    out
}

/// A value-step that was APPLIED to the netlist (Stage C → BOM closure).
#[derive(Debug, Clone)]
pub struct AppliedStep {
    pub refdes: String,
    pub from: String,
    pub to: String,
    pub note: String,
}

/// Apply Stage-C inductor ripple-ratio stepping directly to the netlist:
/// for each output inductor over its ripple-ratio target, mutate its `value`
/// to the E12 step-up that meets the target, returning the changes. This is
/// the analytic, no-solve case (the operating point is recovered from rails +
/// regulator attributes), so it can run right after the snap and before part
/// re-selection. Caller is responsible for re-packaging / re-resolving the
/// MPN of the stepped parts. Returns empty for non-switching designs.
pub fn apply_inductor_stepping(
    netlist: &mut Netlist,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Vec<AppliedStep> {
    let Some(op) = recover_switcher_op(netlist, entity_attrs) else {
        return Vec::new();
    };
    // Collect inductor targets (immutable borrow) before mutating.
    let targets: Vec<(bhdl_netlist::InstanceId, String, f64)> = netlist
        .instances
        .iter()
        .filter_map(|(id, inst)| {
            if classify_component(netlist, inst.definition, &inst.attributes).as_deref()
                != Some("inductor")
            {
                return None;
            }
            let vstr = inst.attributes.get("value")?.clone();
            let l = parse_si(&vstr).filter(|l| *l > 0.0)?;
            Some((id, vstr, l))
        })
        .collect();

    let mut applied = Vec::new();
    for (id, vstr, l) in targets {
        let d_il = (op.v_in - op.v_out) * op.duty / (op.f_sw * l);
        if let Some((l_step, ratio_from, ratio_new, target)) = inductor_value_step(&op, l, d_il) {
            let new_str = fmt_si(l_step, "H");
            if let Some(inst) = netlist.instances.get_mut(id) {
                inst.attributes.insert("value".to_string(), new_str.clone());
                applied.push(AppliedStep {
                    refdes: inst.name.clone(),
                    from: vstr,
                    to: new_str,
                    note: format!(
                        "ripple ratio {ratio_from:.2}→{ratio_new:.2}, target {target:.2}",
                    ),
                });
            }
        }
    }
    applied
}

/// Compute the per-passive sign-off rows for a (snapped) netlist given a
/// GLACIER operating point: `net_voltages` (node name → V), `instance_power`
/// (refdes → W), `instance_currents` (refdes → A).
/// Evaluate the per-instance stress overrides contributed by entities that
/// declare a `simulation { stress { } }` block (Vendor_Simulation_Blocks.md §4,
/// Stage 4). Returns a map keyed by `(instance_name, axis)` (e.g.
/// `("L_out","i_peak")`) → value. A stress recipe's child references resolve to
/// the netlist instance of the SAME name. The result is empty when no
/// instantiated entity declares a recipe — in which case sign-off uses the
/// hardcoded reference ripple model unchanged (the graceful-degradation
/// fallback; this is what keeps every existing circuit byte-identical).
fn stress_overrides(
    netlist: &Netlist,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
    stress_recipes: &HashMap<String, StressRecipe>,
    op: &SwitcherOp,
) -> HashMap<(String, String), f64> {
    let mut out: HashMap<(String, String), f64> = HashMap::new();
    if stress_recipes.is_empty() {
        return out;
    }
    // Operating point exposed to every block: the recovered switcher point.
    let mut operating_point = HashMap::from([
        ("vin".to_string(), op.v_in),
        ("vout".to_string(), op.v_out),
    ]);
    if let Some(i) = op.i_out {
        operating_point.insert("i_out".to_string(), i);
    }
    // `<child>.value` resolves to the snapped value of the like-named instance.
    let child_values: HashMap<String, f64> = netlist
        .instances
        .values()
        .filter_map(|inst| {
            inst.attributes
                .get("value")
                .and_then(|s| parse_si(s))
                .map(|v| (inst.name.clone(), v))
        })
        .collect();

    // Evaluate the recipe of every instantiated entity that declares one.
    let mut seen_entities: Vec<&str> = Vec::new();
    for inst in netlist.instances.values() {
        let Some(module) = netlist.modules.get(inst.definition) else { continue };
        let entity = module.name.as_str();
        if seen_entities.contains(&entity) {
            continue;
        }
        let Some(recipe) = stress_recipes.get(entity) else { continue };
        seen_entities.push(entity);

        // `self.<param>` ← the entity's declared attributes (numeric ones).
        let self_params: HashMap<String, f64> = entity_attrs
            .get(entity)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| parse_si(v).map(|n| (k.clone(), n)))
                    .collect()
            })
            .unwrap_or_default();

        let inputs = StressInputs {
            operating_point: operating_point.clone(),
            self_params,
            child_values: child_values.clone(),
        };
        // A failed `require` or any eval error means the model does not apply —
        // skip it (those parts keep their generic/hardcoded stress).
        if let Ok(overrides) = evaluate_stress_recipe(recipe, &inputs) {
            for (key, val) in overrides {
                out.insert(key, val);
            }
        }
    }
    out
}

pub fn compute_signoff(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
    instance_currents: &HashMap<String, f64>,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
    stress_recipes: &HashMap<String, StressRecipe>,
) -> Vec<SignoffRow> {
    let inst_v = compute_instance_max_voltages(netlist, net_voltages);
    // Analytic reference ripple model (switching topologies). `d_il` is the
    // output inductor's ripple current, shared by the inductor peak-current
    // and the output/input cap ripple-voltage derivations below.
    let op = recover_switcher_op(netlist, entity_attrs);
    let d_il = op.as_ref().and_then(|op| inductor_ripple_current(netlist, op));
    // Per-instance overrides from vendor `stress { }` blocks (§4). Empty unless
    // an instantiated entity declares one — then these win over the hardcoded
    // reference ripple model below, per the graceful-degradation ladder.
    let overrides = op
        .as_ref()
        .map(|op| stress_overrides(netlist, entity_attrs, stress_recipes, op))
        .unwrap_or_default();
    // Parallel caps on a rail SHARE the switching ripple current — the ripple
    // voltage is set by the TOTAL bank capacitance, not each cap alone (else a
    // 100nF HF bypass next to a 22µF bulk cap reads an absurd multi-volt
    // ripple). Sum the input-side and output-side bank capacitance, classified
    // by each cap's DC node voltage.
    let (c_in_total, c_out_total) = match op.as_ref() {
        Some(op) => {
            let near = |a: f64, b: f64| (a - b).abs() < 0.1 * b.max(1.0);
            let (mut cin, mut cout) = (0.0, 0.0);
            for inst in netlist.instances.values() {
                if classify_component(netlist, inst.definition, &inst.attributes).as_deref()
                    != Some("capacitor")
                {
                    continue;
                }
                let Some(c) = inst.attributes.get("value").and_then(|s| parse_si(s)) else {
                    continue;
                };
                let v = inst_v.get(&inst.name).copied().unwrap_or(0.0);
                if near(v, op.v_out) {
                    cout += c;
                } else if near(v, op.v_in) {
                    cin += c;
                }
            }
            (cin, cout)
        }
        None => (0.0, 0.0),
    };
    let mut rows = Vec::new();

    for (id, inst) in netlist.instances.iter() {
        let _ = id;
        let Some(class) = classify_component(netlist, inst.definition, &inst.attributes) else {
            continue;
        };
        if !matches!(class.as_str(), "resistor" | "capacitor" | "inductor") {
            continue;
        }
        let value = inst
            .attributes
            .get("value")
            .cloned()
            .unwrap_or_default();
        // Skip abstract stdlib module definitions that surface as bare
        // instances (`Cap`/`Res`/`Ind` with no value) — only real placed
        // passives carry a value and are sign-off candidates.
        if value.trim().is_empty() {
            continue;
        }
        let dnp = inst
            .attributes
            .get("dnp")
            .map(|v| v == "true")
            .unwrap_or(false);

        // (axis, raw stress, derate, selected-rating attribute key)
        let (axis, mut stress, derate, rating_key): (&str, Option<f64>, f64, &str) = match class
            .as_str()
        {
            "capacitor" => (
                "V",
                inst_v.get(&inst.name).copied().filter(|v| *v > 1e-9),
                CAP_VOLTAGE_DERATE,
                "voltage_rating",
            ),
            "resistor" => (
                "P",
                instance_power
                    .get(&inst.name)
                    .copied()
                    .map(f64::abs)
                    .filter(|p| *p > 1e-12),
                RES_POWER_DERATE,
                "power_rating",
            ),
            "inductor" => (
                "I",
                instance_currents
                    .get(&inst.name)
                    .copied()
                    .map(f64::abs)
                    .filter(|i| *i > 1e-12),
                IND_CURRENT_DERATE,
                "current_rating",
            ),
            _ => unreachable!(),
        };

        // Ripple override (analytic reference switcher model). A reactive
        // part's *value* sets its ripple stress — the place value-stepping
        // actually bites (`Simulation_Margin_Signoff.md` §11). DC parts and
        // non-switching designs keep the generic stress above.
        let mut ripple: Option<String> = None;
        let mut step: Option<String> = None;
        if let (Some(op), Some(d_il)) = (op.as_ref(), d_il) {
            match class.as_str() {
                "inductor" if overrides.contains_key(&(inst.name.clone(), "i_peak".to_string())) => {
                    // A vendor stress block supplied this inductor's peak current
                    // directly (§4) — it wins over the hardcoded reference model.
                    let i_pk = overrides[&(inst.name.clone(), "i_peak".to_string())];
                    stress = Some(i_pk);
                    ripple = Some(format!("I_pk={i_pk:.3}A (stress block)"));
                }
                "inductor" => {
                    // Peak current is the saturation-critical stress, not DC avg.
                    if let Some(l) = parse_si(&value).filter(|l| *l > 0.0) {
                        // ΔI_L is independent of the load; the peak current
                        // (the saturation-critical stress) needs the real load.
                        let dil = (op.v_in - op.v_out) * op.duty / (op.f_sw * l);
                        ripple = Some(match op.i_out {
                            Some(i_out) => {
                                let i_pk = i_out + dil / 2.0;
                                stress = Some(i_pk);
                                format!("ΔI_L={dil:.3}A → I_pk={i_pk:.3}A")
                            }
                            // Real-Data Policy: no declared output-rail load ⇒
                            // peak current is UNCHECKED (no rated-current proxy).
                            None => format!(
                                "ΔI_L={dil:.3}A → I_pk UNCHECKED (output rail declares no `@ I` load)"
                            ),
                        });
                        // Stage C: if the ripple ratio is over target, recommend
                        // the E12 step-up that meets it (larger L ⇒ less ripple,
                        // monotone — the value-fixable axis, distinct from the
                        // current *rating* margin which is the supply gate's job).
                        // Returns None when the load is undeclared (ratio UNCHECKED).
                        if let Some((l_step, ratio_from, ratio_new, target)) =
                            inductor_value_step(op, l, dil)
                        {
                            step = Some(format!(
                                "{} → {} (ratio {:.2}→{:.2}, target {:.2})",
                                fmt_si(l, "H"),
                                fmt_si(l_step, "H"),
                                ratio_from,
                                ratio_new,
                                target,
                            ));
                        }
                    }
                }
                "capacitor" if overrides.contains_key(&(inst.name.clone(), "v_ripple".to_string())) => {
                    // A vendor stress block supplied this cap's ripple voltage
                    // directly (§4). Per §4.1 the voltage stress is bumped to
                    // V_dc + ΔV/2 only on the OUTPUT rail; an input cap's stress
                    // "stays the rail" (its ripple is a separate target check),
                    // matching the hardcoded model's convention.
                    let dv = overrides[&(inst.name.clone(), "v_ripple".to_string())];
                    let v_dc = stress.unwrap_or(0.0);
                    let near = |a: f64, b: f64| (a - b).abs() < 0.1 * b.max(1.0);
                    if near(v_dc, op.v_out) {
                        stress = Some(v_dc + dv / 2.0);
                    }
                    ripple = Some(format!("ΔV={:.1}mV (stress block)", dv * 1000.0));
                }
                "capacitor" => {
                    // Ripple voltage is a per-RAIL quantity set by the whole
                    // parallel bank (c_*_total), shared by every cap on the
                    // rail — not a function of this cap's own value.
                    let v_dc = stress.unwrap_or(0.0);
                    let near = |a: f64, b: f64| (a - b).abs() < 0.1 * b.max(1.0);
                    if near(v_dc, op.v_out) && c_out_total > 0.0 {
                        // Output cap: total voltage = V_out + ΔV_out/2.
                        let dv = d_il / (8.0 * op.f_sw * c_out_total);
                        stress = Some(op.v_out + dv / 2.0);
                        ripple = Some(format!(
                            "ΔV_out={:.1}mV (bank {})",
                            dv * 1000.0,
                            fmt_si(c_out_total, "F")
                        ));
                    } else if near(v_dc, op.v_in) && c_in_total > 0.0 {
                        // Input cap: ripple voltage (stress stays the rail).
                        // The input ripple current is load-driven, so it needs
                        // the real output-rail load — UNCHECKED without it.
                        ripple = Some(match op.i_out {
                            Some(i_out) => {
                                let dv = i_out * op.duty * (1.0 - op.duty)
                                    / (op.f_sw * c_in_total);
                                format!(
                                    "ΔV_in={:.1}mV (bank {})",
                                    dv * 1000.0,
                                    fmt_si(c_in_total, "F")
                                )
                            }
                            None => format!(
                                "ΔV_in UNCHECKED — output rail declares no `@ I` load (bank {})",
                                fmt_si(c_in_total, "F")
                            ),
                        });
                    }
                }
                _ => {}
            }
        }

        let rating = inst.attributes.get(rating_key).and_then(|s| parse_si(s));
        let derated = stress.map(|s| s * derate);
        let margin = match (rating, derated) {
            (Some(r), Some(d)) if d > 0.0 => Some(r / d),
            _ => None,
        };
        let verdict = match margin {
            Some(m) if m >= SIGNOFF_MARGIN => Verdict::SignedOff,
            Some(m) if m >= 1.0 => Verdict::UnderMargin,
            Some(_) => Verdict::OverStress,
            None => Verdict::NoData,
        };

        rows.push(SignoffRow {
            refdes: inst.name.clone(),
            class,
            axis,
            value,
            stress,
            derated,
            rating,
            margin,
            verdict,
            dnp,
            ripple,
            step,
        });
    }

    rows.sort_by(|a, b| a.refdes.cmp(&b.refdes));
    rows
}

/// Format an SI base-unit value with an engineering prefix, e.g.
/// `6.8e-6, "H"` → `"6.8µH"`, `47e-6, "F"` → `"47µF"`.
fn fmt_si(v: f64, unit: &str) -> String {
    if !(v.abs() > 0.0) {
        return format!("0{unit}");
    }
    const PREFIXES: &[(&str, f64)] = &[
        ("p", 1e-12),
        ("n", 1e-9),
        ("µ", 1e-6),
        ("m", 1e-3),
        ("", 1e0),
        ("k", 1e3),
        ("M", 1e6),
    ];
    let mut best = ("", 1e0);
    for &(p, scale) in PREFIXES {
        let scaled = v / scale;
        if scaled.abs() >= 1.0 && scaled.abs() < 1000.0 {
            best = (p, scale);
            break;
        }
    }
    let scaled = v / best.1;
    // Trim trailing zeros: 6.80 → 6.8, 47.0 → 47.
    let s = format!("{scaled:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    format!("{s}{}{unit}", best.0)
}

fn fmt_opt(v: Option<f64>, unit: &str) -> String {
    match v {
        Some(x) => format!("{x:.4}{unit}"),
        None => "—".to_string(),
    }
}

/// Render the sign-off rows as a Markdown table plus a one-line summary.
/// Returns `None` if there are no stress-bearing passives to report.
pub fn format_signoff_report(rows: &[SignoffRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("\n## Sign-off report (DC operating point, post-snap)\n\n");
    out.push_str("| Ref des | Class | Axis | Value | Stress | Derated | Rating | Margin | Ripple | Verdict |\n");
    out.push_str("|---------|-------|------|-------|--------|---------|--------|--------|--------|---------|\n");

    let (mut signed, mut under, mut over, mut nodata) = (0, 0, 0, 0);
    for r in rows {
        match r.verdict {
            Verdict::SignedOff => signed += 1,
            Verdict::UnderMargin => under += 1,
            Verdict::OverStress => over += 1,
            Verdict::NoData => nodata += 1,
        }
        let unit = match r.axis {
            "V" => "V",
            "P" => "W",
            "I" => "A",
            _ => "",
        };
        let margin = r
            .margin
            .map(|m| format!("{m:.2}×"))
            .unwrap_or_else(|| "—".to_string());
        let verdict = if r.dnp {
            format!("{} (DNP)", r.verdict.label())
        } else {
            r.verdict.label().to_string()
        };
        out.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.refdes,
            r.class,
            r.axis,
            if r.value.is_empty() { "—" } else { &r.value },
            fmt_opt(r.stress, unit),
            fmt_opt(r.derated, unit),
            fmt_opt(r.rating, unit),
            margin,
            r.ripple.as_deref().unwrap_or("—"),
            verdict,
        ));
    }

    out.push_str(&format!(
        "\n**Summary:** {signed} signed-off, {under} under-margin, {over} over-stress, {nodata} not-simulated.\n"
    ));
    if nodata > 0 {
        out.push_str(
            "_Not-simulated rows had no DC stress (e.g. a cap with no DC voltage across it) \
             or no selected rating to compare; transient/AC stress is not modelled in this DC pass._\n",
        );
    }

    // Stage C — value-stepping recommendations (reactive parts over their
    // ripple target). Currently the inductor ripple-ratio case; the value is
    // recommended, not yet applied to the BOM (re-selecting the stepped part's
    // MPN is a follow-up), so it reads as a recommendation.
    let steps: Vec<&SignoffRow> = rows.iter().filter(|r| r.step.is_some()).collect();
    if !steps.is_empty() {
        out.push_str("\n**Stepping recommendations (to meet ripple target):**\n");
        for r in steps {
            out.push_str(&format!(
                "- {} ({}): {}\n",
                r.refdes,
                r.class,
                r.step.as_deref().unwrap_or("")
            ));
        }
        out.push_str(
            "_Recommended values, not yet applied to the BOM (stepped-part MPN re-selection \
             is a follow-up). Output-cap stepping additionally needs the control-loop stability \
             check (a larger C_out can reduce phase margin) and is deferred._\n",
        );
    }
    Some(out)
}
