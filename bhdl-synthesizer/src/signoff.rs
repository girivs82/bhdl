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
    /// Instance HANDLE (user-authored name) — the identity key; override
    /// maps and expansion-child lookups match on this.
    pub refdes: String,
    /// Table ink: `handle (refdes)` when a phase-12.7 refdes was stamped
    /// and differs from the handle, else just the handle.
    pub display: String,
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
    /// Rating provenance: `mpn:<MPN>` (catalog-bound), `declared`
    /// (author's claim), a stdlib `data_source` class point, or
    /// `unrated (…)` when no rating claim exists at all.
    pub source: Option<String>,
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

/// All netlist instances in NAME order. Every sign-off derivation iterates
/// through this instead of `instances.values()`: SlotMap iteration is
/// insertion order, and instance-creation order upstream is NOT stable
/// run-to-run (HashMap ordering during elaboration) — any "first match wins"
/// walk over it made the report nondeterministic on multi-switcher boards.
fn sorted_instances(netlist: &Netlist) -> Vec<(bhdl_netlist::InstanceId, &bhdl_netlist::Instance)> {
    let mut v: Vec<_> = netlist.instances.iter().collect();
    v.sort_by(|(_, a), (_, b)| a.name.cmp(&b.name));
    v
}

/// Recover the buck operating point of EVERY switching regulator on the
/// board, keyed by the regulator's instance name and returned in name order
/// (deterministic). Each stage's `v_out` is its OWN regulated rail —
/// resolved from the instance's `v_out`/`output_voltage` attribute first,
/// else structurally from the power rails its pins actually touch (VOUT →
/// rail), never from global rail ranking (which can only describe ONE
/// stage). `v_in` is the touched rail above `v_out`; `i_out` is the output
/// rail's declared `@ I` budget. A stage whose rails cannot be resolved is
/// dropped (its parts keep generic DC stress) — except a board's SOLE
/// switcher, which keeps the historical global-rail fallback (highest rail =
/// input, next = output).
fn recover_switcher_ops(
    netlist: &Netlist,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Vec<(String, SwitcherOp)> {
    // Declared power rails (voltage, load budget).
    let rails: Vec<(f64, Option<f64>)> = netlist
        .nets
        .values()
        .filter_map(|net| match &net.net_class {
            NetClass::Power { voltage, current } if *voltage > 0.0 => Some((*voltage, *current)),
            _ => None,
        })
        .collect();
    // Structural rail contact: instance name → power rails any of its pins
    // connect to. This is real connectivity (the regulator's VIN/VOUT pins),
    // available pre-solve.
    let mut touching: HashMap<String, Vec<(f64, Option<f64>)>> = HashMap::new();
    for net in netlist.nets.values() {
        let NetClass::Power { voltage, current } = &net.net_class else { continue };
        if !(*voltage > 0.0) {
            continue;
        }
        for conn in &net.connections {
            let inst_id = match conn {
                bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) => Some(*iid),
                bhdl_netlist::ConnectionPoint::PinInstance(pi) => {
                    netlist.pin_instances.get(*pi).map(|p| p.instance)
                }
                _ => None,
            };
            if let Some(inst) = inst_id.and_then(|iid| netlist.instances.get(iid)) {
                let rails = touching.entry(inst.name.clone()).or_default();
                if !rails.iter().any(|(v, _)| v == voltage) {
                    rails.push((*voltage, *current));
                }
            }
        }
    }
    let near = |a: f64, b: f64| (a - b).abs() < 0.1 * b.max(1.0);

    // The regulator's class/topology/f_sw are declared on the stdlib ENTITY.
    // For an entity WITHOUT an expansion/design block they are never stamped
    // onto the netlist instance or module (only `entity_attribute_index`
    // carries them), so look up each key in three places, instance first:
    //   instance attrs → module attrs → entity_attribute_index[entity name].
    let mut switchers: Vec<(String, f64, Option<f64>, Option<f64>, Option<f64>, Option<f64>, Option<f64>)> = Vec::new();
    for (_id, inst) in sorted_instances(netlist) {
        let module = netlist.modules.get(inst.definition);
        // Skip abstract stdlib module definitions surfacing as bare instances
        // (an instance literally named after its entity, e.g. `TPS54331 :
        // TPS54331`) — the same phantom class the sign-off loops drop. Only
        // real placed regulators are stages.
        if module.map(|m| m.name == inst.name).unwrap_or(true) {
            continue;
        }
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
            continue;
        }
        let Some(f_sw) = get("switching_frequency")
            .or_else(|| get("f_sw"))
            .and_then(|s| parse_si(s))
            .filter(|f| *f > 0.0)
        else {
            continue;
        };
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
        // Per-instance declared output voltage. Instance attrs only — an
        // entity-level value can't distinguish two instances regulated to
        // different voltages. `v_out` (the constructor parameter the S4
        // synthesizer passes, always this instance's own) BEFORE
        // `output_voltage`: when one entity is instantiated at two different
        // output voltages, the stamped `output_voltage` (an entity attribute
        // expression) can carry the OTHER instance's resolved value.
        let v_out_attr = inst
            .attributes
            .get("v_out")
            .or_else(|| inst.attributes.get("output_voltage"))
            .and_then(|s| parse_si(s))
            .filter(|v| *v > 0.0);
        switchers.push((inst.name.clone(), f_sw, ripple_ratio, loop_k, loop_ratio, v_ref, v_out_attr));
    }

    let sole = switchers.len() == 1;
    let mut ops = Vec::new();
    for (name, f_sw, ripple_ratio, loop_k, loop_ratio, v_ref, v_out_attr) in switchers {
        let touched = touching.get(&name).map(Vec::as_slice).unwrap_or(&[]);
        let t_max = touched.iter().map(|(v, _)| *v).fold(f64::MIN, f64::max);
        // v_out: declared attr → highest touched rail below the highest one
        // touched (VIN) → global two-rail ranking, sole switcher only.
        let v_out = v_out_attr
            .or_else(|| {
                touched
                    .iter()
                    .map(|(v, _)| *v)
                    .filter(|v| *v < t_max - 1e-9)
                    .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))))
            })
            .or_else(|| sole.then(|| rail_operating_point(netlist).map(|(_, v, _)| v)).flatten());
        let Some(v_out) = v_out.filter(|v| *v > 0.0) else { continue };
        // v_in: highest touched rail above v_out → highest declared rail
        // above v_out (an entity whose VIN pin reaches the rail through a
        // filter element touches no rail directly).
        let v_in = touched
            .iter()
            .map(|(v, _)| *v)
            .filter(|v| *v > v_out + 1e-9)
            .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))))
            .or_else(|| {
                rails
                    .iter()
                    .map(|(v, _)| *v)
                    .filter(|v| *v > v_out + 1e-9)
                    .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))))
            });
        let Some(v_in) = v_in.filter(|v| v_out < *v) else { continue };
        // The output rail's declared `@ I` load budget (None ⇒ UNCHECKED).
        let i_out = rails
            .iter()
            .filter(|(v, _)| near(*v, v_out))
            .filter_map(|(_, i)| *i)
            .fold(None::<f64>, |acc, i| Some(acc.map_or(i, |a| a.max(i))));
        ops.push((
            name,
            SwitcherOp {
                v_in,
                v_out,
                i_out,
                f_sw,
                duty: v_out / v_in,
                ripple_ratio,
                loop_k,
                loop_ratio,
                v_ref,
            },
        ));
    }
    ops
}

/// Power rails carry their source-declared per-rail load budget (`@ I`) on
/// the net class. V_in = the highest rail; V_out = the highest rail strictly
/// below it; `i_out` = the OUTPUT rail's declared current — the actual load,
/// or `None` when that rail omits `@ I` (→ i_out-dependent stresses UNCHECKED).
fn rail_operating_point(netlist: &Netlist) -> Option<(f64, f64, Option<f64>)> {
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
    Some((v_in, v_out, i_out))
}

/// LINEAR-regulator operating point for the §4 stress path: the same rail
/// recovery as the switcher, minus the switching requirements. Returned as a
/// `SwitcherOp` (f_sw = 0, no ripple/loop constants) purely so vendor stress
/// blocks get their `vin`/`vout`/`i_out` — it must NOT be used for the
/// analytic switching-ripple model (the caller keeps `op = None` there, so
/// the d_il / cap-bank paths stay off for linear boards).
fn recover_linear_op(
    netlist: &Netlist,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Option<SwitcherOp> {
    let has_linear_regulator = netlist.instances.values().any(|inst| {
        let module = netlist.modules.get(inst.definition);
        let ent = module.and_then(|m| entity_attrs.get(&m.name));
        let get = |k: &str| {
            inst.attributes
                .get(k)
                .or_else(|| module.and_then(|m| m.attributes.get(k)))
                .or_else(|| ent.and_then(|e| e.get(k)))
        };
        matches!(
            get("component_class").map(String::as_str),
            Some("voltage_regulator") | Some("ldo")
        )
    });
    if !has_linear_regulator {
        return None;
    }
    let (v_in, v_out, i_out) = rail_operating_point(netlist)?;
    Some(SwitcherOp {
        v_in,
        v_out,
        i_out,
        f_sw: 0.0,
        duty: v_out / v_in,
        ripple_ratio: None,
        loop_k: None,
        loop_ratio: None,
        v_ref: None,
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

/// The output inductor's ripple current `ΔI_L = (V_in−V_out)·D / (f_sw·L)`
/// for ONE regulator stage — from the stage's own output inductor (its
/// `expansion_parent` names the regulator instance). A board's sole switcher
/// falls back to the first inductor by name (hand-authored expansions carry
/// no parent tag). Shared by the inductor peak-current and the output-cap
/// ripple-voltage derivations.
fn inductor_ripple_current(
    netlist: &Netlist,
    stage: &str,
    op: &SwitcherOp,
    sole_stage: bool,
) -> Option<f64> {
    let mut fallback: Option<f64> = None;
    for (_id, inst) in sorted_instances(netlist) {
        if classify_component(netlist, inst.definition, &inst.attributes).as_deref()
            != Some("inductor")
        {
            continue;
        }
        let Some(l) = inst.attributes.get("value").and_then(|s| parse_si(s)).filter(|l| *l > 0.0)
        else {
            continue;
        };
        let d_il = (op.v_in - op.v_out) * op.duty / (op.f_sw * l);
        if inst.attributes.get("expansion_parent").map(String::as_str) == Some(stage) {
            return Some(d_il);
        }
        if sole_stage && fallback.is_none() {
            fallback = Some(d_il);
        }
    }
    fallback
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

/// Stability of ONE regulator stage — sign-off assesses each switching
/// stage against its OWN loop, not one blended board-level loop.
#[derive(Debug, Clone)]
pub struct StageStability {
    /// The regulator instance (e.g. `psu_vcc_5v`).
    pub stage: String,
    /// The stage's regulated output voltage.
    pub v_out: f64,
    pub outcome: StageOutcome,
}

#[derive(Debug, Clone)]
pub enum StageOutcome {
    Assessed(StabilityResult),
    /// The device declares no closed-form loop model (no `loop_crossover_k` /
    /// `loop_crossover_max_ratio` / `feedback_voltage`) — e.g. an externally
    /// compensated part. Real-Data Policy: not assessed, never guessed.
    NoLoopModel,
    /// Loop model declared but no output cap bank was identifiable near the
    /// stage's regulated rail in the solved voltages.
    NoOutputBank,
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

/// Evaluate loop stability per regulator stage, in stage-name order
/// (deterministic run-to-run). Returns one entry per switching stage: an
/// assessed loop, or an explicit not-assessed outcome — a device without a
/// declared loop model (`loop_crossover_k` / ratio / `feedback_voltage`) or
/// without an identifiable output bank is *unchecked*, never silently
/// "passed". Empty for non-switching boards.
pub fn compute_stability(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Vec<StageStability> {
    recover_switcher_ops(netlist, entity_attrs)
        .into_iter()
        .map(|(stage, op)| {
            let v_out = op.v_out;
            let outcome = compute_stage_stability(netlist, net_voltages, &op);
            StageStability { stage, v_out, outcome }
        })
        .collect()
}

/// One stage's stability from its own operating point.
fn compute_stage_stability(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
    op: &SwitcherOp,
) -> StageOutcome {
    use std::f64::consts::PI;
    // Real-Data Policy: the loop model exists only if the device declares all
    // of K, the crossover ratio, and V_ref. Any absent ⇒ no loop model ⇒ this
    // stage is not assessed (distinct from ESR-data-missing, → UNCHECKED).
    let (Some(k), Some(loop_ratio), Some(v_ref)) = (op.loop_k, op.loop_ratio, op.v_ref) else {
        return StageOutcome::NoLoopModel;
    };
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
    for (_id, inst) in sorted_instances(netlist) {
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
        return StageOutcome::NoOutputBank;
    }

    let f_co = k / (op.v_out * c_out); // needs only the real C_out
    let crossover_target = op.f_sw * loop_ratio;
    let crossover_ok = f_co < crossover_target;

    // Divider-top resistor: a resistor touching V_out and V_ref (the FB node).
    let r_top = sorted_instances(netlist).into_iter().find_map(|(_id, inst)| {
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
        // (any() is order-independent — no sorted walk needed here)
        role_is(inst, "capacitor")
            && inv.get(&inst.name).is_some_and(|vs| {
                vs.iter().any(|v| near(*v, op.v_out)) && vs.iter().any(|v| near(*v, v_ref))
            })
    });

    // An output cap whose ESR *and* type are both unknown ⇒ genuinely
    // UNCHECKED (can't apply the real ESR zero nor the ceramic argument).
    if !missing_esr.is_empty() {
        return StageOutcome::Assessed(StabilityResult {
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

    StageOutcome::Assessed(StabilityResult {
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

/// Render the per-stage stability assessments as ONE report section.
/// `None` when no stage was actually assessed (no switcher, or no stage
/// declares a loop model) — matching the historical "no section" behavior.
/// Single-stage boards keep the historical unlabelled block; multi-stage
/// boards get one labelled block per stage, INCLUDING an explicit
/// not-assessed line for stages without a loop model (an absent check is a
/// visible hole, not a silent pass).
pub fn format_stability(stages: &[StageStability]) -> Option<String> {
    if !stages.iter().any(|s| matches!(s.outcome, StageOutcome::Assessed(_))) {
        return None;
    }
    let mut out = String::from("\n## Control-loop stability (analytic, datasheet model)\n\n");
    let multi = stages.len() > 1;
    for s in stages {
        if multi {
            out.push_str(&format!("**{}** (V_out={}):\n", s.stage, fmt_si(s.v_out, "V")));
        }
        match &s.outcome {
            StageOutcome::Assessed(r) => out.push_str(&format_stability_block(r)),
            StageOutcome::NoLoopModel => out.push_str(
                "- no closed-form loop model declared (`loop_crossover_k` / \
                 `loop_crossover_max_ratio` / `feedback_voltage`) — e.g. an externally \
                 compensated device — **stability not assessed**\n",
            ),
            StageOutcome::NoOutputBank => out.push_str(
                "- no output capacitor bank identifiable on the regulated rail — \
                 **stability not assessed**\n",
            ),
        }
        if multi {
            out.push('\n');
        }
    }
    Some(out)
}

/// Render one assessed stage's bullets + verdict.
fn format_stability_block(s: &StabilityResult) -> String {
    let mut out = String::new();
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
    let ops = recover_switcher_ops(netlist, entity_attrs);
    if ops.is_empty() {
        return Vec::new();
    }
    let sole = ops.len() == 1;
    // Collect inductor targets (immutable borrow) before mutating, in name
    // order (deterministic), each paired with ITS OWN stage's operating point
    // (expansion parent; a board's sole switcher steps every inductor as
    // before). A multi-stage board's parentless inductor is skipped — no
    // basis to pick a stage for it.
    let targets: Vec<(bhdl_netlist::InstanceId, String, f64, usize)> = sorted_instances(netlist)
        .into_iter()
        .filter_map(|(id, inst)| {
            if classify_component(netlist, inst.definition, &inst.attributes).as_deref()
                != Some("inductor")
            {
                return None;
            }
            let vstr = inst.attributes.get("value")?.clone();
            let l = parse_si(&vstr).filter(|l| *l > 0.0)?;
            let op_idx = inst
                .attributes
                .get("expansion_parent")
                .and_then(|p| ops.iter().position(|(n, _)| n == p))
                .or(if sole { Some(0) } else { None })?;
            Some((id, vstr, l, op_idx))
        })
        .collect();

    let mut applied = Vec::new();
    for (id, vstr, l, op_idx) in targets {
        let op = &ops[op_idx].1;
        let d_il = (op.v_in - op.v_out) * op.duty / (op.f_sw * l);
        if let Some((l_step, ratio_from, ratio_new, target)) = inductor_value_step(op, l, d_il) {
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
    ops: &[(String, SwitcherOp)],
) -> HashMap<(String, String), f64> {
    let mut out: HashMap<(String, String), f64> = HashMap::new();
    if stress_recipes.is_empty() || ops.is_empty() {
        return out;
    }
    let sole = ops.len() == 1;
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

    // Evaluate each recipe once per INSTANCE of its entity (not once per
    // entity): a block's child references may name that instance's own
    // EXPANSION children, whose netlist refdes is prefixed with the parent
    // name (`U1_L_out` for the recipe's `L_out`). For each instance we build
    // a local view mapping the recipe-visible LOCAL child name to the snapped
    // value, layered over the board-level (unprefixed) names, and rewrite the
    // produced override keys back to the full refdes the sign-off table uses.
    for inst in netlist.instances.values() {
        let Some(module) = netlist.modules.get(inst.definition) else { continue };
        let entity = module.name.as_str();
        let Some(recipe) = stress_recipes.get(entity) else { continue };

        // Operating point exposed to this block: the declaring instance's OWN
        // stage — the recipe usually lives on the regulator entity itself
        // (matched by instance name), else the instance's expansion parent's
        // stage, else the board's sole recovered operating point. A recipe on
        // a multi-stage board with no resolvable stage is skipped (Real-Data
        // Policy: no blended board-level vin/vout guess).
        let stage_op = ops
            .iter()
            .find(|(n, _)| *n == inst.name)
            .or_else(|| {
                inst.attributes
                    .get("expansion_parent")
                    .and_then(|p| ops.iter().find(|(n, _)| n == p))
            })
            .or_else(|| if sole { ops.first() } else { None });
        let Some((_, op)) = stage_op else {
            log::debug!(
                "stress recipe for '{entity}' (instance {}) skipped: no per-stage \
                 operating point resolvable on a multi-stage board",
                inst.name,
            );
            continue;
        };
        let mut operating_point = HashMap::from([
            ("vin".to_string(), op.v_in),
            ("vout".to_string(), op.v_out),
        ]);
        if let Some(i) = op.i_out {
            operating_point.insert("i_out".to_string(), i);
        }

        // `self.<param>` ← the entity's declared attributes (numeric ones).
        let self_params: HashMap<String, f64> = entity_attrs
            .get(entity)
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| parse_si(v).map(|n| (k.clone(), n)))
                    .collect()
            })
            .unwrap_or_default();

        // This instance's expansion children: local name → full refdes.
        let prefix = format!("{}_", inst.name);
        let local_children: HashMap<String, String> = netlist
            .instances
            .values()
            .filter(|c| {
                c.attributes.get("expansion_parent").map(String::as_str)
                    == Some(inst.name.as_str())
            })
            .filter_map(|c| {
                c.name
                    .strip_prefix(&prefix)
                    .map(|local| (local.to_string(), c.name.clone()))
            })
            .collect();

        // Board-level names first, local expansion-child names layered on top
        // (a local `L_out` must win over an unrelated board-level `L_out`).
        let mut inst_child_values = child_values.clone();
        for (local, full) in &local_children {
            if let Some(v) = child_values.get(full) {
                inst_child_values.insert(local.clone(), *v);
            }
        }

        let inputs = StressInputs {
            operating_point,
            self_params,
            child_values: inst_child_values,
        };
        // A failed `require` or any eval error means the model does not apply —
        // skip it (those parts keep their generic/hardcoded stress). The debug
        // log is the discoverability hook: a recipe that never applies
        // otherwise looks identical to no recipe at all.
        match evaluate_stress_recipe(recipe, &inputs) {
            Ok(overrides) => {
                for ((child, axis), val) in overrides {
                    // `self.<axis> = …` targets the declaring instance itself —
                    // the linear-regulator pass-element dissipation form
                    // (`self.p_diss = (vin−vout)·i_out + vin·self.i_quiescent`).
                    let refdes = if child == "self" {
                        inst.name.clone()
                    } else {
                        local_children.get(&child).cloned().unwrap_or(child)
                    };
                    out.insert((refdes, axis), val);
                }
            }
            Err(e) => {
                log::debug!(
                    "stress recipe for '{entity}' (instance {}) skipped: {e:?} \
                     [local children: {:?}]",
                    inst.name,
                    local_children.keys().collect::<Vec<_>>(),
                );
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
    let near = |a: f64, b: f64| (a - b).abs() < 0.1 * b.max(1.0);
    // Analytic reference ripple model (switching topologies), PER regulator
    // stage: each stage's own operating point and its own output inductor's
    // ripple current ΔI_L, shared by that stage's inductor peak-current and
    // cap ripple-voltage derivations below.
    let ops = recover_switcher_ops(netlist, entity_attrs);
    let sole = ops.len() == 1;
    let d_il_by_stage: HashMap<&str, f64> = ops
        .iter()
        .filter_map(|(n, op)| {
            inductor_ripple_current(netlist, n, op, sole).map(|d| (n.as_str(), d))
        })
        .collect();
    // Per-instance overrides from vendor `stress { }` blocks (§4). Empty unless
    // an instantiated entity declares one — then these win over the hardcoded
    // reference ripple model below, per the graceful-degradation ladder.
    // Linear boards have no switcher op; recover the rail-only operating point
    // (vin/vout/i_out) for the vendor blocks alone — `ops` itself stays empty
    // so the analytic switching-ripple model below remains off.
    let overrides = if !ops.is_empty() {
        stress_overrides(netlist, entity_attrs, stress_recipes, &ops)
    } else {
        recover_linear_op(netlist, entity_attrs)
            .map(|lop| {
                stress_overrides(netlist, entity_attrs, stress_recipes, &[(String::new(), lop)])
            })
            .unwrap_or_default()
    };
    // Parallel caps on a rail SHARE the switching ripple current — the ripple
    // voltage is set by the TOTAL bank capacitance, not each cap alone (else a
    // 100nF HF bypass next to a 22µF bulk cap reads an absurd multi-volt
    // ripple). Per stage, sum the input-side and output-side bank capacitance,
    // classified by each cap's DC node voltage (the bank is physical: in a
    // cascade the mid rail is stage N's output bank AND stage N+1's input
    // bank — every cap on the rail belongs to both).
    let banks: HashMap<&str, (f64, f64)> = ops
        .iter()
        .map(|(n, op)| {
            let (mut cin, mut cout) = (0.0, 0.0);
            for (_id, inst) in sorted_instances(netlist) {
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
            (n.as_str(), (cin, cout))
        })
        .collect();
    // Associate a passive with its regulator stage: its expansion parent's
    // stage first (a stage's own C_in on a cascade's mid rail must read as
    // that stage's INPUT cap, not the upstream stage's output cap), else the
    // board's sole stage.
    let stage_by_parent = |inst: &bhdl_netlist::Instance| -> Option<&(String, SwitcherOp)> {
        inst.attributes
            .get("expansion_parent")
            .and_then(|p| ops.iter().find(|(n, _)| n == p))
            .or_else(|| if sole { ops.first() } else { None })
    };
    let mut rows = Vec::new();

    for (id, inst) in sorted_instances(netlist) {
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
        if !ops.is_empty() {
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
                    // The operating point is the inductor's OWN stage's.
                    if let (Some((_, op)), Some(l)) =
                        (stage_by_parent(inst), parse_si(&value).filter(|l| *l > 0.0))
                    {
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
                    // The bump applies only when this cap sits on ITS stage's
                    // OUTPUT rail — a cascade mid-rail cap owned by the
                    // downstream stage is that stage's INPUT cap and stays
                    // the rail.
                    let on_own_output = match stage_by_parent(inst) {
                        Some((_, op)) => near(v_dc, op.v_out),
                        None => ops.iter().any(|(_, op)| near(v_dc, op.v_out)),
                    };
                    if on_own_output {
                        stress = Some(v_dc + dv / 2.0);
                    }
                    ripple = Some(format!("ΔV={:.1}mV (stress block)", dv * 1000.0));
                }
                "capacitor" => {
                    // Ripple voltage is a per-RAIL quantity set by the whole
                    // parallel bank, shared by every cap on the rail — not a
                    // function of this cap's own value. The stage is the cap's
                    // own (expansion parent) so a cascade mid-rail cap reads
                    // as its downstream stage's INPUT cap; a board-level cap
                    // matches whichever stage regulates (else feeds) its rail.
                    let v_dc = stress.unwrap_or(0.0);
                    let assoc = stage_by_parent(inst)
                        .or_else(|| ops.iter().find(|(_, op)| near(v_dc, op.v_out)))
                        .or_else(|| ops.iter().find(|(_, op)| near(v_dc, op.v_in)));
                    if let Some((sname, op)) = assoc {
                        let (c_in_total, c_out_total) =
                            banks.get(sname.as_str()).copied().unwrap_or((0.0, 0.0));
                        let d_il = d_il_by_stage.get(sname.as_str()).copied();
                        if near(v_dc, op.v_out) && c_out_total > 0.0 && d_il.is_some() {
                            // Output cap: total voltage = V_out + ΔV_out/2.
                            let dv = d_il.unwrap() / (8.0 * op.f_sw * c_out_total);
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
                }
                _ => {}
            }
        }

        // Rating source: the catalog-selected part's rating when supply-chain
        // selection resolved a real part (`mpn` present); otherwise the
        // author's DECLARED rating, preserved by the catalog pass as
        // `declared_<axis>_rating` — for an unresolved/hand-pinned part the
        // declaration IS the datasheet claim (Real-Data), not the catalog
        // family's blind fallback stamp. Parse-aware: a declaration that
        // never evaluated (an unbound param leaves the raw expression, e.g.
        // `"voltage"`) must not eclipse a parseable stamped rating.
        // 0-valued ratings are the UNDECLARED sentinel (an inductor's
        // `Ind(10µH)` with no rating claim), not a zero-ampere part —
        // they read as ABSENT so the verdict is honestly NO DATA
        // instead of a division against a phantom.
        let nz = |v: f64| v > 0.0;
        let rating = if inst.attributes.contains_key("mpn") {
            inst.attributes
                .get(rating_key)
                .and_then(|s| parse_si(s))
                .filter(|r| nz(*r))
        } else {
            inst.attributes
                .get(&format!("declared_{rating_key}"))
                .and_then(|s| parse_si(s))
                .filter(|r| nz(*r))
                .or_else(|| {
                    inst.attributes
                        .get(rating_key)
                        .and_then(|s| parse_si(s))
                        .filter(|r| nz(*r))
                })
        };
        // Rating PROVENANCE for the report: which claim is this margin
        // graded against? An MPN-bound catalog rating, the author's
        // declaration, a stdlib class point (data_source attribute),
        // or nothing.
        let source = if rating.is_none() {
            inst.attributes
                .get("data_source")
                .map(|s| format!("unrated ({s})"))
                .or_else(|| Some("unrated".to_string()))
        } else if let Some(mpn) = inst.attributes.get("mpn") {
            Some(format!("mpn:{mpn}"))
        } else if inst
            .attributes
            .get(&format!("declared_{rating_key}"))
            .and_then(|s| parse_si(s))
            .filter(|r| nz(*r))
            .is_some()
        {
            Some("declared".to_string())
        } else {
            inst.attributes
                .get("data_source")
                .cloned()
                .or_else(|| Some("declared".to_string()))
        };
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
            display: display_label(inst),
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
            source,
        });
    }

    // §4 self-stress rows: a vendor stress block that assigned an axis to the
    // DEVICE ITSELF (`self.p_diss = …`, mapped to the instance refdes by
    // stress_overrides) gets its own sign-off row — the linear-regulator
    // pass-element dissipation gate, checked against the entity's declared
    // package `power_rating` with the same derate discipline as resistors.
    // Only instances outside the passive classes above (those already rowed).
    for inst in netlist.instances.values() {
        let key = (inst.name.clone(), "p_diss".to_string());
        let Some(&p_diss) = overrides.get(&key) else { continue };
        // Skip abstract stdlib module definitions surfacing as bare instances
        // (an instance literally named after its entity, e.g. `LM7805 :
        // LM7805`) — the same class of phantom the passive loop's empty-value
        // guard drops. Only real placed regulators get a dissipation row.
        if netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name == inst.name)
            .unwrap_or(false)
        {
            continue;
        }
        let get = |k: &str| inst.attributes.get(k);
        let class = get("component_class").cloned().unwrap_or_default();
        if matches!(class.as_str(), "resistor" | "capacitor" | "inductor") {
            continue; // passives already have their own axis rows
        }
        let value = get("part_number")
            .or_else(|| get("output_voltage"))
            .cloned()
            .unwrap_or_default();
        let dnp = get("dnp").map(|v| v == "true").unwrap_or(false);
        let rating = get("power_rating").and_then(|s| parse_si(s));
        let derated = Some(p_diss * RES_POWER_DERATE);
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
            display: display_label(inst),
            class,
            axis: "P",
            value,
            stress: Some(p_diss),
            derated,
            rating,
            margin,
            verdict,
            dnp,
            ripple: Some(format!("P_pass={:.3}W (stress block)", p_diss)),
            step: None,
            source: None,
        });

        // ── junction temperature row: the measured dissipation through the
        // part's θ_JA against its junction rating at the board's ambient.
        // T_A: the stage requirement's `temp_max` (on this silicon's
        // block, via composed_parent, or on the instance itself), else
        // 25 °C ASSUMED and said so. Axis = thermal RISE: stress = P·θ_JA,
        // rating = T_J,max − T_A, so the margin is the rise budget ratio.
        let theta_ja = get("theta_ja").and_then(|s| crate::stage_acceptance::parse_si(s));
        let tj_max = get("tj_max").and_then(|s| crate::stage_acceptance::parse_temp_c(s));
        if let (Some(theta), Some(tjm)) = (theta_ja, tj_max) {
            let req_text = get("stage_requirement").cloned().or_else(|| {
                get("composed_parent")
                    .and_then(|pn| netlist.instances.values().find(|i| &i.name == pn))
                    .and_then(|parent| parent.attributes.get("stage_requirement").cloned())
            });
            let ta_req = req_text.as_deref().and_then(|t| {
                t.split(',').filter_map(|kv| kv.split_once('=')).find(|(k, _)| k.trim() == "temp_max").and_then(|(_, v)| crate::stage_acceptance::parse_temp_c(v))
            });
            let (ta, ta_src) = match ta_req {
                Some(t) => (t, format!("T_A={t:.0}°C (requirement temp_max)")),
                None => (25.0, "T_A=25°C ASSUMED (no temp_max requirement on this stage)".to_string()),
            };
            let rise = p_diss * theta;
            let allowed = tjm - ta;
            let tj = ta + rise;
            let margin = if rise > 0.0 { Some(allowed / rise) } else { None };
            let verdict = match margin {
                Some(m) if m >= SIGNOFF_MARGIN => Verdict::SignedOff,
                Some(m) if m >= 1.0 => Verdict::UnderMargin,
                Some(_) => Verdict::OverStress,
                None => Verdict::NoData,
            };
            rows.push(SignoffRow {
                refdes: inst.name.clone(),
                display: display_label(inst),
                class: "junction temperature".into(),
                axis: "T",
                value: format!("T_J,max={tjm:.0}°C"),
                stress: Some(rise),
                derated: Some(rise),
                rating: Some(allowed),
                margin,
                verdict,
                dnp,
                ripple: Some(format!(
                    "T_J = {ta_src} + {:.3}W × {theta:.1}°C/W = {tj:.1}°C (θ_JA: JEDEC board metric, board-dependent)",
                    p_diss
                )),
                step: None,
                source: None,
            });
        }
    }

    // Requirement rows (Power_Supply_Synthesis.md §5): a `supply` statement's
    // spec axes ride the synthesized instance as `supply_*` attributes; each
    // becomes a gated row checked against the AS-BUILT value — the
    // requirement is verified, not satisfied by construction. An axis whose
    // achieved value cannot be computed reports NoData (UNCHECKED), never a
    // silent pass.
    for inst in netlist.instances.values() {
        let get = |k: &str| inst.attributes.get(k);
        let class_of = |v: &str| format!("requirement ({v})");

        // ripple_max vs the achieved output ripple — the §4 stress block's
        // v_ripple on this instance's own C_out expansion child.
        if let Some(spec) = get("supply_ripple_max").and_then(|s| parse_si(s)) {
            let achieved = overrides
                .iter()
                .filter(|((child, axis), _)| {
                    // Case-insensitive: hand-authored expansions name the
                    // child C_out, the S4 emitter uses the TI designator
                    // c_out1 — both are this instance's output bank.
                    axis == "v_ripple"
                        && child
                            .to_lowercase()
                            .starts_with(&format!("{}_c_out", inst.name.to_lowercase()))
                })
                .map(|(_, v)| *v)
                .fold(None::<f64>, |acc, v| Some(acc.map_or(v, |a| a.max(v))));
            let margin = achieved.filter(|a| *a > 0.0).map(|a| spec / a);
            let verdict = match margin {
                Some(m) if m >= SIGNOFF_MARGIN => Verdict::SignedOff,
                Some(m) if m >= 1.0 => Verdict::UnderMargin,
                Some(_) => Verdict::OverStress,
                None => Verdict::NoData,
            };
            rows.push(SignoffRow {
                refdes: inst.name.clone(),
                display: display_label(inst),
                class: class_of("ripple"),
                axis: "V",
                value: fmt_si(spec, "V"),
                stress: achieved,
                derated: achieved,
                rating: Some(spec),
                margin,
                verdict,
                dnp: false,
                ripple: Some(match achieved {
                    Some(a) => format!(
                        "spec ≤ {}; achieved ΔV={:.1}mV (stress block)",
                        fmt_si(spec, "V"),
                        a * 1000.0
                    ),
                    None => format!(
                        "spec ≤ {}; achieved UNCHECKED (no stress-block ripple)",
                        fmt_si(spec, "V")
                    ),
                }),
                step: None,
            source: None,
        });
        }

        // i_q_max vs the part's declared quiescent current.
        if let Some(spec) = get("supply_i_q_max").and_then(|s| parse_si(s)) {
            let achieved = get("i_quiescent").and_then(|s| parse_si(s));
            let margin = achieved.filter(|a| *a > 0.0).map(|a| spec / a);
            let verdict = match margin {
                Some(m) if m >= SIGNOFF_MARGIN => Verdict::SignedOff,
                Some(m) if m >= 1.0 => Verdict::UnderMargin,
                Some(_) => Verdict::OverStress,
                None => Verdict::NoData,
            };
            rows.push(SignoffRow {
                refdes: inst.name.clone(),
                display: display_label(inst),
                class: class_of("i_q"),
                axis: "I",
                value: fmt_si(spec, "A"),
                stress: achieved,
                derated: achieved,
                rating: Some(spec),
                margin,
                verdict,
                dnp: false,
                ripple: Some(match achieved {
                    Some(a) => format!(
                        "spec ≤ {}; datasheet i_q={} (attr)",
                        fmt_si(spec, "A"),
                        fmt_si(a, "A")
                    ),
                    None => format!(
                        "spec ≤ {}; achieved UNCHECKED (no i_quiescent attr)",
                        fmt_si(spec, "A")
                    ),
                }),
                step: None,
            source: None,
        });
        }

        // efficiency_min: no loss model surfaced to sign-off yet — always an
        // explicit UNCHECKED row rather than a silent pass.
        if let Some(spec_txt) = get("supply_efficiency_min") {
            rows.push(SignoffRow {
                refdes: inst.name.clone(),
                display: display_label(inst),
                class: class_of("efficiency"),
                axis: "P",
                value: spec_txt.clone(),
                stress: None,
                derated: None,
                rating: parse_si(spec_txt),
                margin: None,
                verdict: Verdict::NoData,
                dnp: false,
                ripple: Some(format!(
                    "spec ≥ {spec_txt}; achieved UNCHECKED (loss model not yet surfaced)"
                )),
                step: None,
            source: None,
        });
        }
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

/// `handle (refdes)` table label — refdes from the phase-12.7 stamped
/// attribute; falls back to the bare handle when absent or identical.
fn display_label(inst: &bhdl_netlist::Instance) -> String {
    match inst.attributes.get("refdes") {
        Some(rd) if rd != &inst.name => format!("{} ({})", inst.name, rd),
        _ => inst.name.clone(),
    }
}

/// Render the sign-off rows as a Markdown table plus a one-line summary.
/// Returns `None` if there are no stress-bearing passives to report.
/// The derived-values section of the sign-off report: every part whose
/// value the toolchain DERIVED by simulation (`derive_rule` markers),
/// with the author's seed, the landed value, and the rule's
/// spec-vs-MEASURED basis — including derivations that were SKIPPED
/// (seed kept), which the engineer must see for the same reason ERC024
/// shows unchecked axes: a skipped derivation is a hole, not a pass.
/// One switching FET's gate-drive demand: I_gate_avg = Qg · f_sw —
/// exact charge-per-cycle math, deliberately NOT a power claim (power
/// would need the driver's V_drive, which is unmodeled; absence over
/// assumption). A row exists only where BOTH facts are real: a FET
/// whose instance declares its datasheet qg_nc (0 = undeclared
/// sentinel) inside a recovered switching stage.
pub struct GateDriveRow {
    pub display: String,
    pub part: String,
    pub stage: String,
    pub qg_nc: f64,
    pub f_sw_hz: f64,
    pub i_avg_a: f64,
}

/// One MLCC's effective capacitance at its SOLVED DC bias — the
/// consumer for MPN-bound dc_bias_c_* curve points (SKU-specific
/// data; the generic Cap class deliberately carries none).
pub struct MlccBiasRow {
    pub instance: String,
    pub c_nom_uf: f64,
    pub v_rated: f64,
    pub v_solved: f64,
    pub factor: f64,
    pub c_eff_uf: f64,
    pub verdict: String,
}

/// Compute effective capacitance under DC bias for every capacitor
/// instance that carries dc_bias_c_* points (linear interpolation in
/// volts between the curve points, end-clamped). Solved bias = the
/// instance's max pin-to-pin DC voltage, same source the voltage-
/// rating margin uses.
pub fn compute_mlcc_bias(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Vec<MlccBiasRow> {
    let inst_v = compute_instance_max_voltages(netlist, net_voltages);
    let mut rows = Vec::new();
    for (_, inst) in sorted_instances(netlist) {
        let entity = netlist
            .modules
            .get(inst.definition)
            .map(|m| m.name.clone())
            .unwrap_or_default();
        let attrs = entity_attrs.get(&entity);
        let get = |name: &str| -> Option<f64> {
            inst.attributes
                .get(name)
                .or_else(|| attrs.and_then(|a| a.get(name)))
                .and_then(|v| parse_si(v))
        };
        // Curve points at integer volts 0..=10 — collect whatever the
        // part declares.
        let mut pts: Vec<(f64, f64)> = (0..=10)
            .filter_map(|v| get(&format!("dc_bias_c_{v}v")).map(|f| (v as f64, f)))
            .collect();
        if pts.len() < 2 {
            continue;
        }
        pts.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        let Some(&v_solved) = inst_v.get(inst.name.as_str()) else { continue };
        let Some(c_nom) = get("capacitance") else { continue };
        let v_rated = get("voltage_rating").unwrap_or(0.0);
        let factor = {
            let v = v_solved.abs();
            if v <= pts[0].0 {
                pts[0].1
            } else if v >= pts[pts.len() - 1].0 {
                pts[pts.len() - 1].1
            } else {
                let mut f = pts[pts.len() - 1].1;
                for w in pts.windows(2) {
                    if v <= w[1].0 {
                        let t = (v - w[0].0) / (w[1].0 - w[0].0);
                        f = w[0].1 + t * (w[1].1 - w[0].1);
                        break;
                    }
                }
                f
            }
        };
        let verdict = if factor < 0.5 {
            "SIZE-UP (>50% lost to bias)".to_string()
        } else if factor < 0.8 {
            "DERATED".to_string()
        } else {
            "OK".to_string()
        };
        rows.push(MlccBiasRow {
            instance: inst.name.clone(),
            c_nom_uf: c_nom * 1e6,
            v_rated,
            v_solved,
            factor,
            c_eff_uf: c_nom * factor * 1e6,
            verdict,
        });
    }
    rows
}

pub fn format_mlcc_bias(rows: &[MlccBiasRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("\n### MLCC DC-bias derating (solved bias vs MPN curve)\n\n");
    out.push_str("| Instance | C nominal | Rated | Solved bias | C effective | Verdict |\n");
    out.push_str("|----------|-----------|-------|-------------|-------------|--------|\n");
    for r in rows {
        out.push_str(&format!(
            "| {} | {:.1}µF | {:.1}V | {:.2}V | {:.1}µF ({:.0}%) | {} |\n",
            r.instance,
            r.c_nom_uf,
            r.v_rated,
            r.v_solved,
            r.c_eff_uf,
            r.factor * 100.0,
            r.verdict
        ));
    }
    out.push_str(
        "\n_Class-II MLCCs lose capacitance under DC bias; the factor is \
         interpolated from the part's OWN datasheet curve at the solved \
         operating voltage. Only MPN-bound parts carry curves — a generic \
         Cap prints nothing here (absence over invention)._\n",
    );
    Some(out)
}

/// One optocoupler's solved transfer point vs its datasheet envelope
/// — the consumer for the CTR derating chain the entity carries
/// (rank envelope × Fig.6 curve × temperature × IRED aging).
pub struct OptoTransferRow {
    pub instance: String,
    pub if_ma: f64,
    pub ctr_min_pct: f64,
    pub curve_factor: f64,
    pub ic_avail_ma: f64,
    pub ic_solved_ma: f64,
    pub ic_worst_ma: f64,
    pub derations: String,
    pub verdict: String,
}

pub fn format_opto_transfer(rows: &[OptoTransferRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("\n### Optocoupler transfer (solved point vs derating chain)\n\n");
    out.push_str("| Instance | IF | CTR min × curve | IC avail | IC solved | IC worst-case | Verdict |\n");
    out.push_str("|----------|----|-----------------|----------|-----------|---------------|--------|\n");
    for r in rows {
        out.push_str(&format!(
            "| {} | {:.2}mA | {:.0}% × {:.2} | {:.2}mA | {:.2}mA | {:.2}mA ({}) | {} |\n",
            r.instance,
            r.if_ma,
            r.ctr_min_pct,
            r.curve_factor,
            r.ic_avail_ma,
            r.ic_solved_ma,
            r.ic_worst_ma,
            r.derations,
            r.verdict
        ));
    }
    out.push_str(
        "\n_IC avail = min-rank CTR × the Fig.6 curve factor at the SOLVED IF. \
         Worst-case additionally applies the entity's hot (100degC) and IRED-aging \
         factors — a solved IC above it is the classic opto field failure: works \
         fresh and cool, dies hot and aged. Order a tighter rank or raise IF._\n",
    );
    Some(out)
}

pub fn compute_gate_drive(
    netlist: &Netlist,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Vec<GateDriveRow> {
    let ops = recover_switcher_ops(netlist, entity_attrs);
    if ops.is_empty() {
        return Vec::new();
    }
    let sole = ops.len() == 1;
    let mut rows = Vec::new();
    for (_, inst) in sorted_instances(netlist) {
        let is_fet = inst
            .attributes
            .get("component_class")
            .map(|c| c == "mosfet")
            .unwrap_or(false);
        if !is_fet {
            continue;
        }
        let Some(qg) = inst
            .attributes
            .get("qg_nc")
            .and_then(|s| parse_si(s))
            .filter(|q| *q > 0.0)
        else {
            continue;
        };
        let stage = inst
            .attributes
            .get("expansion_parent")
            .and_then(|p| ops.iter().find(|(n, _)| n == p))
            .or_else(|| if sole { ops.first() } else { None });
        let Some((stage_name, op)) = stage else { continue };
        if op.f_sw <= 0.0 {
            continue;
        }
        rows.push(GateDriveRow {
            display: display_label(inst),
            part: inst
                .attributes
                .get("part_number")
                .cloned()
                .unwrap_or_default(),
            stage: stage_name.clone(),
            qg_nc: qg,
            f_sw_hz: op.f_sw,
            i_avg_a: qg * 1e-9 * op.f_sw,
        });
    }
    rows
}

pub fn format_gate_drive(rows: &[GateDriveRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("\n## Gate drive (declared Qg × recovered f_sw)\n\n");
    out.push_str("| FET | Part | Stage | Qg | f_sw | I_gate avg |\n");
    out.push_str("|-----|------|-------|----|------|------------|\n");
    for r in rows {
        out.push_str(&format!(
            "| {} | {} | {} | {:.1}nC | {} | {} |\n",
            r.display,
            if r.part.is_empty() { "—" } else { &r.part },
            r.stage,
            r.qg_nc,
            fmt_si(r.f_sw_hz, "Hz"),
            fmt_si(r.i_avg_a, "A"),
        ));
    }
    out.push_str(
        "\nAverage current the controller's driver must source per FET; \
         power needs V_drive (unmodeled) — deliberately not claimed.\n",
    );
    Some(out)
}

pub fn format_derived_values(netlist: &bhdl_netlist::Netlist) -> Option<String> {
    let mut rows: Vec<(String, String, String, String, String)> = netlist
        .instances
        .iter()
        .filter_map(|(_, inst)| {
            let rule = inst.attributes.get("derive_rule")?;
            let detail = inst
                .attributes
                .get("derive_detail")
                .cloned()
                .unwrap_or_else(|| "no derivation ran (pre-solve path?)".to_string());
            let seed = inst
                .attributes
                .get("derive_seed")
                .cloned()
                .unwrap_or_else(|| "—".to_string());
            let value = inst
                .attributes
                .get("value")
                .cloned()
                .unwrap_or_else(|| "—".to_string());
            let display = match inst.attributes.get("refdes") {
                Some(r) => format!("{} ({})", inst.name, r.trim_matches('"')),
                None => inst.name.clone(),
            };
            Some((
                display,
                rule.trim_matches('"').to_string(),
                seed,
                value,
                detail,
            ))
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    rows.sort();
    let mut out = String::new();
    out.push_str("
### Derived values (simulation-driven)

");
    out.push_str("| Part | Rule | Seed | Derived | Basis (spec vs MEASURED) |
");
    out.push_str("|------|------|------|---------|---------------------------|
");
    let mut skipped = 0;
    for (display, rule, seed, value, detail) in &rows {
        if detail.contains("SKIPPED") {
            skipped += 1;
        }
        out.push_str(&format!(
            "| {display} | {rule} | {seed} | {value} | {detail} |
"
        ));
    }
    if skipped > 0 {
        out.push_str(&format!(
            "
_{skipped} derivation(s) SKIPPED (seed kept) — see the basis column; a skipped derivation is an unverified value, not a pass._
"
        ));
    }
    Some(out)
}

pub fn format_signoff_report(rows: &[SignoffRow]) -> Option<String> {
    if rows.is_empty() {
        return None;
    }
    let mut out = String::new();
    out.push_str("\n## Sign-off report (DC operating point, post-snap)\n\n");
    out.push_str("| Ref des | Class | Axis | Value | Stress | Derated | Rating | Margin | Ripple | Source | Verdict |\n");
    out.push_str("|---------|-------|------|-------|--------|---------|--------|--------|--------|--------|---------|\n");

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
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.display,
            r.class,
            r.axis,
            if r.value.is_empty() { "—" } else { &r.value },
            fmt_opt(r.stress, unit),
            fmt_opt(r.derated, unit),
            fmt_opt(r.rating, unit),
            margin,
            r.ripple.as_deref().unwrap_or("—"),
            r.source.as_deref().unwrap_or("—"),
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

    // ── ERC024 — the absence ledger (docs/spec/ERC.md batch 3) ──
    //
    // Real-Data Policy makes axes SKIP rather than guess; this section makes
    // every skip VISIBLE as an Info finding. An unchecked axis is not a
    // pass — it is a hole in the verification, and the engineer signs the
    // report knowing exactly which holes remain. Deliberately not waivable:
    // a waived absence is still an absence.
    let unchecked: Vec<&SignoffRow> = rows
        .iter()
        .filter(|r| {
            matches!(r.verdict, Verdict::NoData)
                || r.ripple.as_deref().is_some_and(|t| t.contains("UNCHECKED"))
        })
        .collect();
    if !unchecked.is_empty() {
        out.push_str(&format!(
            "\n### Unchecked axes (ERC024 — absence ledger, {} Info)\n\n",
            unchecked.len()
        ));
        out.push_str("| Ref des | Class | Axis | What is missing |\n");
        out.push_str("|---------|-------|------|------------------|\n");
        for r in &unchecked {
            let why = match r.ripple.as_deref() {
                Some(t) if t.contains("UNCHECKED") => t.to_string(),
                _ => "no DC stress or no selected rating to compare \
                      (transient/AC stress is not modelled in this pass)"
                    .to_string(),
            };
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                r.display, r.class, r.axis, why
            ));
            log::info!(
                "DRC ERC024 [Unchecked axis] Info: {} {} axis {} — {}",
                r.refdes, r.class, r.axis, why
            );
        }
        out.push_str(
            "\n_Every row here is an axis the Real-Data Policy refused to guess. \
             Supply the missing datum (declared `@ I` load, stress block, \
             datasheet attribute) to convert it into a gated verdict; the \
             phase-margin section reports its own UNCHECKED state separately._\n",
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
