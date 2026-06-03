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

use crate::glacier_physical_selection::{classify_component, compute_instance_max_voltages};
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
) -> Vec<SignoffRow> {
    let inst_v = compute_instance_max_voltages(netlist, net_voltages);
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
        let (axis, stress, derate, rating_key): (&str, Option<f64>, f64, &str) = match class.as_str()
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
    out.push_str("| Ref des | Class | Axis | Value | Stress | Derated | Rating | Margin | Verdict |\n");
    out.push_str("|---------|-------|------|-------|--------|---------|--------|--------|---------|\n");

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
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            r.refdes,
            r.class,
            r.axis,
            if r.value.is_empty() { "—" } else { &r.value },
            fmt_opt(r.stress, unit),
            fmt_opt(r.derated, unit),
            fmt_opt(r.rating, unit),
            margin,
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
