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
    classify_component, compute_instance_max_voltages, declared_net_voltages,
};
use bhdl_netlist::Netlist;
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
    i_out: f64,
    f_sw: f64,
    duty: f64,
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
    let (f_sw, i_out) = netlist.instances.values().find_map(|inst| {
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
        // I_out: the regulator's rated output current. (The per-rail `@ <I>`
        // budget on a `power` decl is not yet carried in NetClass, so the
        // rated current is the available proxy — slightly conservative.)
        let f_sw = get("switching_frequency")
            .or_else(|| get("f_sw"))
            .and_then(|s| parse_si(s))
            .filter(|f| *f > 0.0)?;
        let i_out = get("output_current")
            .or_else(|| get("i_out_max"))
            .or_else(|| get("output_current_max"))
            .and_then(|s| parse_si(s))
            .filter(|i| *i > 0.0)?;
        Some((f_sw, i_out))
    })?;
    let rails: Vec<f64> = declared_net_voltages(netlist)
        .into_values()
        .filter(|v| *v > 0.0)
        .collect();
    if rails.len() < 2 {
        return None;
    }
    let v_in = rails.iter().cloned().fold(f64::MIN, f64::max);
    let v_out = rails
        .iter()
        .cloned()
        .filter(|v| *v < v_in - 1e-9)
        .fold(f64::MIN, f64::max);
    if !(v_out > 0.0 && v_out < v_in) {
        return None;
    }
    Some(SwitcherOp {
        v_in,
        v_out,
        i_out,
        f_sw,
        duty: v_out / v_in,
    })
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

/// Compute the per-passive sign-off rows for a (snapped) netlist given a
/// GLACIER operating point: `net_voltages` (node name → V), `instance_power`
/// (refdes → W), `instance_currents` (refdes → A).
pub fn compute_signoff(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
    instance_currents: &HashMap<String, f64>,
    entity_attrs: &HashMap<String, HashMap<String, String>>,
) -> Vec<SignoffRow> {
    let inst_v = compute_instance_max_voltages(netlist, net_voltages);
    // Analytic reference ripple model (switching topologies). `d_il` is the
    // output inductor's ripple current, shared by the inductor peak-current
    // and the output/input cap ripple-voltage derivations below.
    let op = recover_switcher_op(netlist, entity_attrs);
    let d_il = op.as_ref().and_then(|op| inductor_ripple_current(netlist, op));
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
        if let (Some(op), Some(d_il)) = (op.as_ref(), d_il) {
            match class.as_str() {
                "inductor" => {
                    // Peak current is the saturation-critical stress, not DC avg.
                    if let Some(l) = parse_si(&value).filter(|l| *l > 0.0) {
                        let dil = (op.v_in - op.v_out) * op.duty / (op.f_sw * l);
                        let i_pk = op.i_out + dil / 2.0;
                        stress = Some(i_pk);
                        ripple = Some(format!("ΔI_L={dil:.3}A → I_pk={i_pk:.3}A"));
                    }
                }
                "capacitor" => {
                    let v_dc = stress.unwrap_or(0.0);
                    let near = |a: f64, b: f64| (a - b).abs() < 0.1 * b.max(1.0);
                    if let Some(c) = parse_si(&value).filter(|c| *c > 0.0) {
                        if near(v_dc, op.v_out) {
                            // Output cap: total voltage = V_out + ΔV_out/2.
                            let dv = d_il / (8.0 * op.f_sw * c);
                            stress = Some(op.v_out + dv / 2.0);
                            ripple = Some(format!("ΔV_out={:.1}mV", dv * 1000.0));
                        } else if near(v_dc, op.v_in) {
                            // Input cap: ripple voltage (stress stays the rail).
                            let dv = op.i_out * op.duty * (1.0 - op.duty) / (op.f_sw * c);
                            ripple = Some(format!("ΔV_in={:.1}mV", dv * 1000.0));
                        }
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
        });
    }

    rows.sort_by(|a, b| a.refdes.cmp(&b.refdes));
    rows
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
    Some(out)
}
