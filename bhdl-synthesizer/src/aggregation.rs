//! PMIC AGGREGATION — the resolver's post-step (docs/spec/
//! Requirements_And_Resolution.md §8).
//!
//! Per-rail resolution runs FIRST, exactly as before: one stage per
//! rail, each independently surveyed, priced and bound. THEN this pass
//! asks whether one multi-output part could cover a SET of those
//! requirements — the question a per-rail greedy survey structurally
//! cannot see, because a PMIC loses to a $0.40 buck on every single
//! rail and wins on the set.
//!
//! A multi-output block declares its capability table as data:
//!   attribute pmic_outputs = "DCDC1:buck:1.8V:1.2A,LDO2:ldo:3.3V:0.1A,…";
//!   attribute pmic_seq     = "LDO1,DCDC1,…";   // built-in power-up order
//! (fixed voltages — the honest model of an OTP part). The evaluation:
//! each resolved requirement matches an unused output when the fixed
//! voltage equals the requirement's vout (within 2 %, the parts' own
//! accuracy class), the derated requirement current fits the output's
//! rating, and the requirement's input rail lies inside the PMIC's
//! input range. Greedy assignment, outputs never reused.
//!
//! This increment REPORTS the option — coverage, leftover rails, the
//! price of the PMIC silicon vs the Σ of the bound discrete stages,
//! and the built-in sequencing order (informational; the strict
//! sequencing gate binds when an aggregation COMMIT lands — a future
//! increment, stated). Nothing is auto-committed: the comparison is
//! the designer's lever.

use std::collections::HashMap;
use std::path::Path;

use crate::stage_resolution::StageResolution;
use crate::supply_synthesis::{collect_bhdl, entity_attrs_txt, parse_si_txt};

#[derive(Debug, Clone)]
struct PmicOutput {
    name: String,
    topology: String,
    vout: f64,
    i_max: f64,
}

#[derive(Debug, Clone)]
struct PmicBlock {
    block: String,
    part_number: Option<String>,
    vin_min: Option<f64>,
    vin_max: Option<f64>,
    outputs: Vec<PmicOutput>,
    seq: Option<String>,
    seq_dly: Option<String>,
}

fn scan_pmics(stdlib_root: &Path) -> Vec<PmicBlock> {
    let mut files = Vec::new();
    collect_bhdl(stdlib_root, &mut files);
    files.sort();
    let mut out = Vec::new();
    for f in files {
        let Ok(text) = std::fs::read_to_string(&f) else { continue };
        if !text.contains("pmic_outputs") {
            continue;
        }
        // every `entity <X>` in the file that declares pmic_outputs
        let mut off = 0usize;
        while let Some(p) = text[off..].find("entity ") {
            let at = off + p;
            off = at + 7;
            let name: String = text[at + 7..]
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            if name.is_empty() {
                continue;
            }
            let attrs = entity_attrs_txt(&text, &name);
            let Some(tbl) = attrs.get("pmic_outputs") else { continue };
            let outputs: Vec<PmicOutput> = tbl
                .trim_matches('"')
                .split(',')
                .filter_map(|e| {
                    let p: Vec<&str> = e.split(':').collect();
                    if p.len() != 4 {
                        return None;
                    }
                    Some(PmicOutput {
                        name: p[0].to_string(),
                        topology: p[1].to_string(),
                        vout: parse_si_txt(p[2])?,
                        i_max: parse_si_txt(p[3])?,
                    })
                })
                .collect();
            if outputs.is_empty() {
                continue;
            }
            // the silicon's part_number: the part entity this block
            // instantiates, or its own attr
            let part_number = attrs.get("part_number").cloned().or_else(|| {
                crate::stage_resolution::block_part_number(&text, &name)
            });
            out.push(PmicBlock {
                block: name,
                part_number,
                vin_min: attrs.get("vin_min").and_then(|v| parse_si_txt(v)),
                vin_max: attrs.get("vin_max").and_then(|v| parse_si_txt(v)),
                outputs,
                seq: attrs.get("pmic_seq").map(|s| s.trim_matches('"').to_string()),
                seq_dly: attrs.get("pmic_seq_dly").map(|s| s.trim_matches('"').to_string()),
            });
        }
    }
    out
}

/// Evaluate every declared multi-output block against the board's
/// resolved requirements; returns report lines (empty when fewer than
/// two requirements, or nothing covers more than one rail).
pub fn evaluate(resolutions: &[StageResolution], stdlib_root: &Path) -> Vec<String> {
    if resolutions.len() < 2 {
        return Vec::new();
    }
    let req_val = |r: &StageResolution, key: &str| -> Option<f64> {
        r.requirement
            .split(',')
            .filter_map(|kv| kv.trim().split_once('='))
            .find(|(k, _)| k.trim() == key)
            .and_then(|(_, v)| parse_si_txt(v.trim()))
    };
    let mut lines = Vec::new();
    for pmic in scan_pmics(stdlib_root) {
        let mut used: Vec<usize> = Vec::new();
        let mut cover: Vec<(String, String)> = Vec::new(); // (instance, output)
        let mut covered_price = 0.0f64;
        let mut covered_priced = true;
        let mut leftovers: Vec<String> = Vec::new();
        for r in resolutions {
            let (Some(vout), Some(imax)) = (req_val(r, "vout"), req_val(r, "i_max")) else {
                leftovers.push(format!("{} (no vout/i_max)", r.instance));
                continue;
            };
            let vin = req_val(r, "vin");
            let vin_ok = match (vin, pmic.vin_min, pmic.vin_max) {
                (Some(v), Some(lo), Some(hi)) => v >= lo - 1e-9 && v <= hi + 1e-9,
                _ => true, // unstated = not disqualifying at report level
            };
            let derated = imax / 0.8;
            let slot = pmic.outputs.iter().enumerate().find(|(i, o)| {
                !used.contains(i)
                    && vin_ok
                    && (o.vout - vout).abs() <= 0.02 * vout + 1e-9
                    && o.i_max + 1e-9 >= derated
            });
            match slot {
                Some((i, o)) => {
                    used.push(i);
                    cover.push((r.instance.clone(), format!("{} ({}, {:.2}V, {:.1}A rated)", o.name, o.topology, o.vout, o.i_max)));
                    // the discrete price this output would displace
                    match r
                        .candidates
                        .iter()
                        .find(|c| Some(&c.block) == r.bound.as_ref())
                        .and_then(|c| c.ic_price)
                    {
                        Some(p) => covered_price += p,
                        None => covered_priced = false,
                    }
                }
                None => leftovers.push(format!(
                    "{} ({}V @ {}A{})",
                    r.instance,
                    vout,
                    imax,
                    if vin_ok { "" } else { " — input rail outside the PMIC range" }
                )),
            }
        }
        if cover.len() < 2 {
            continue; // a PMIC that covers one rail is not aggregation
        }
        let pmic_price: Option<(f64, String)> = pmic
            .part_number
            .as_deref()
            .and_then(|pn| crate::supply_synthesis::price_via_provider(pn, ""))
            .and_then(|(p, m, _)| p.map(|p| (p, m.unwrap_or_default())));
        lines.push(format!(
            "AGGREGATION option — {} covers {} of {} rails:",
            pmic.block,
            cover.len(),
            resolutions.len()
        ));
        for (inst, out) in &cover {
            lines.push(format!("    {inst} → {out}"));
        }
        for l in &leftovers {
            lines.push(format!("    not covered: {l}"));
        }
        match (pmic_price, covered_priced, covered_price) {
            (Some((p, mpn)), true, d) if d > 0.0 => lines.push(format!(
                "    price: {} ${:.4} vs Σ displaced discrete silicon ${:.4} (support parts BOM-time both ways) — {}",
                mpn, p, d,
                if p < d { "the PMIC wins on silicon" } else { "the discretes win on silicon; the PMIC may still win on area/BOM-lines/sequencing" }
            )),
            (Some((p, mpn)), _, _) => lines.push(format!(
                "    price: {} ${:.4}; displaced discrete Σ not fully priced (a covered stage is unresolved/unpriced) — stated",
                mpn, p
            )),
            (None, _, _) => lines.push(
                "    price: PMIC silicon not priced (provider/DB absent or no match) — stated".into(),
            ),
        }
        if let Some(seq) = &pmic.seq {
            lines.push(format!(
                "    built-in power-up order: {} (inter-strobe delay {}) — the sequencing these rails inherit for free; the strict gate against declared domain ordering binds at aggregation COMMIT (future increment, stated)",
                seq,
                pmic.seq_dly.as_deref().unwrap_or("unstated")
            ));
        }
        lines.push("    commit is the designer's lever — aggregation is REPORTED, never auto-bound (this increment)".into());
    }
    lines
}
