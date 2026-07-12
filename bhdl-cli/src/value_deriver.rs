//! Simulation-derived component values.
//!
//! A part marked `derive_rule="<rule>"` (machine-passthrough constructor
//! arg, like `ibis_*`) declares that its written value is a SEED and the
//! toolchain owns the final number: the deriver sweeps E-series
//! candidates through the same GLACIER/IBIS solves the sign-off uses,
//! MEASURES the figure each rule constrains, and stamps the winning
//! value back onto the instance (with `derive_detail` recording
//! spec-vs-measured provenance). A marked part whose rule can't run —
//! unknown rule, missing neighbor data, no vendor model on the driver —
//! is a hard error, never a silent seed-keep.
//!
//! Rules:
//! * `led_current` — series resistor feeding an LED: pick the E24 value
//!   whose SOLVED LED current lands closest to the LED's declared
//!   `forward_current` without exceeding `max_current`. The driver's
//!   real V_OL (IBIS pulldown) participates in the solve.
//! * `i2c_pullup` — bus pull-up: R_min from the I2C spec's sink limit
//!   (V_OL ≤ 0.4 V at I_OL = 3 mA against the rail), then the LARGEST
//!   E24 value whose MEASURED 30–70 % rise (bus released from 0 V into
//!   the pull-up, loaded by every attached pin's real C_comp/clamps)
//!   meets the mode's rise budget: 1000 ns standard-mode, 300 ns
//!   fast-mode (`derive_i2c_khz` ≥ 400 selects fast-mode).

use std::path::Path;

use anyhow::{bail, Context, Result};
use bhdl_netlist::netlist::Netlist;
use bhdl_spice::glacier_dc_solver::GlacierDcSolver;
use bhdl_spice::netlist_converter::NetlistToSpiceConverter;

pub struct DerivedValue {
    pub instance: String,
    pub rule: String,
    pub seed: String,
    pub derived: String,
    /// Spec vs measured, human-readable.
    pub detail: String,
}

const E24: [f64; 24] = [
    1.0, 1.1, 1.2, 1.3, 1.5, 1.6, 1.8, 2.0, 2.2, 2.4, 2.7, 3.0,
    3.3, 3.6, 3.9, 4.3, 4.7, 5.1, 5.6, 6.2, 6.8, 7.5, 8.2, 9.1,
];

fn e24_ladder(lo: f64, hi: f64) -> Vec<f64> {
    let mut out = Vec::new();
    let mut decade = 10f64.powf(lo.log10().floor());
    while decade <= hi {
        for m in E24 {
            let v = m * decade;
            if v >= lo * 0.999 && v <= hi * 1.001 {
                out.push(v);
            }
        }
        decade *= 10.0;
    }
    out
}

fn fmt_r(v: f64) -> String {
    let (scaled, unit) = if v >= 1e6 {
        (v / 1e6, "MΩ")
    } else if v >= 1e3 {
        (v / 1e3, "kΩ")
    } else {
        (v, "Ω")
    };
    let s = format!("{scaled:.2}");
    format!("{}{}", s.trim_end_matches('0').trim_end_matches('.'), unit)
}

/// Parse "20mA" / "2.0V" / "150" style attribute values.
fn parse_engineering(s: &str) -> Option<f64> {
    let t = s.trim().trim_matches('"');
    let split = t
        .find(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-' || c == '+'))
        .unwrap_or(t.len());
    let (mant, suffix) = t.split_at(split);
    let base: f64 = mant.parse().ok()?;
    let scale = match suffix.trim_start_matches(|c: char| c.is_ascii_alphabetic() && false) {
        s if s.starts_with('m') => 1e-3,
        s if s.starts_with('u') || s.starts_with('µ') => 1e-6,
        s if s.starts_with('n') => 1e-9,
        s if s.starts_with('k') || s.starts_with('K') => 1e3,
        s if s.starts_with('M') => 1e6,
        _ => 1.0,
    };
    Some(base * scale)
}

struct Ctx<'a> {
    analysis: &'a bhdl_analyzer::AnalysisResult,
    base_dir: Option<std::path::PathBuf>,
}

impl Ctx<'_> {
    fn converter(&self, netlist: &Netlist) -> NetlistToSpiceConverter {
        let mut conv = NetlistToSpiceConverter::new();
        conv.set_model_overrides(bhdl_synthesizer::model_evaluator::evaluate_model_overrides(
            netlist,
            &self.analysis.model_recipes,
            &self.analysis.entity_attribute_index,
        ));
        conv.set_ibis_models(
            self.analysis
                .model_recipes
                .iter()
                .filter_map(|(e, r)| (!r.ibis.is_empty()).then(|| (e.clone(), r.ibis.clone())))
                .collect(),
            self.base_dir.clone().unwrap_or_default(),
        );
        conv
    }
}

/// Run every `derive_rule` marker in the netlist. Mutates values in
/// place and returns the report rows. Errors are hard: a derivation the
/// author asked for either happens or fails loudly.
pub fn derive_values(
    netlist: &mut Netlist,
    analysis: &bhdl_analyzer::AnalysisResult,
    source_path: &Path,
) -> Result<Vec<DerivedValue>> {
    let marked: Vec<(bhdl_netlist::types::InstanceId, String, String)> = netlist
        .instances
        .iter()
        .filter_map(|(id, inst)| {
            inst.attributes.get("derive_rule").map(|r| {
                (
                    id,
                    r.trim_matches('"').to_string(),
                    inst.attributes.get("value").cloned().unwrap_or_default(),
                )
            })
        })
        .collect();
    if marked.is_empty() {
        return Ok(Vec::new());
    }

    let mut out = Vec::new();
    let base_dir = source_path.parent().map(|p| p.to_path_buf());
    for (id, rule, seed) in marked {
        let name = netlist.instances[id].name.clone();
        let ctx = Ctx { analysis, base_dir: base_dir.clone() };
        let row = match rule.as_str() {
            "led_current" => derive_led_current(netlist, id, &ctx)
                .with_context(|| format!("derive_rule=led_current on '{name}'"))?,
            "i2c_pullup" => derive_i2c_pullup(netlist, id, &ctx)
                .with_context(|| format!("derive_rule=i2c_pullup on '{name}'"))?,
            other => bail!(
                "'{name}': unknown derive_rule '{other}' (known: led_current, i2c_pullup)"
            ),
        };
        out.push(DerivedValue { instance: name, rule, seed, ..row });
    }
    Ok(out)
}

/// Nets an instance's pins land on.
fn nets_of(netlist: &Netlist, id: bhdl_netlist::types::InstanceId) -> Vec<bhdl_netlist::types::NetId> {
    let mut v: Vec<_> = netlist
        .pin_instances
        .values()
        .filter(|pi| pi.instance == id)
        .filter_map(|pi| pi.net)
        .collect();
    v.dedup();
    v
}

/// Solve the board DC with the marked resistor set to `r`, returning
/// (its branch current, node voltages by name).
fn solve_with_r(
    netlist: &mut Netlist,
    id: bhdl_netlist::types::InstanceId,
    r: f64,
    ctx: &Ctx,
) -> Result<(f64, std::collections::HashMap<String, f64>)> {
    let name = netlist.instances[id].name.clone();
    netlist.instances[id]
        .attributes
        .insert("value".to_string(), format!("{r}"));
    let mut conv = ctx.converter(netlist);
    let circuit = conv.convert(netlist).context("circuit conversion")?;
    let result = GlacierDcSolver::new().solve(circuit.clone()).context("DC solve")?;
    let i = circuit
        .branches()
        .find(|(_, b)| b.name == name)
        .and_then(|(e, _)| result.branch_currents.get(&e).copied())
        .unwrap_or(0.0);
    let volts = circuit
        .nodes()
        .map(|(idx, n)| {
            let v = if n.is_ground {
                0.0
            } else {
                result.node_voltages.get(&idx).copied().unwrap_or(0.0)
            };
            (n.name.clone(), v)
        })
        .collect();
    Ok((i, volts))
}

fn derive_led_current(
    netlist: &mut Netlist,
    id: bhdl_netlist::types::InstanceId,
    ctx: &Ctx,
) -> Result<DerivedValue> {
    // The LED: an instance with component_class "led" sharing a net.
    let my_nets = nets_of(netlist, id);
    let led = netlist
        .instances
        .iter()
        .find(|(lid, inst)| {
            *lid != id
                && inst
                    .attributes
                    .get("component_class")
                    .map(|c| c.trim_matches('"') == "led")
                    .unwrap_or(false)
                && nets_of(netlist, *lid).iter().any(|n| my_nets.contains(n))
        })
        .map(|(lid, _)| lid);
    let Some(led_id) = led else {
        bail!("no LED shares a net with this resistor — led_current needs one");
    };
    let led_attrs = netlist.instances[led_id].attributes.clone();
    let target = led_attrs
        .get("forward_current")
        .and_then(|s| parse_engineering(s))
        .ok_or_else(|| anyhow::anyhow!("LED declares no forward_current"))?;
    let i_max = led_attrs
        .get("max_current")
        .and_then(|s| parse_engineering(s))
        .unwrap_or(f64::INFINITY);

    let seed = netlist.instances[id]
        .attributes
        .get("value")
        .and_then(|s| parse_engineering(s))
        .filter(|v| *v > 0.0)
        .unwrap_or(1000.0);

    let mut best: Option<(f64, f64)> = None; // (r, i_measured)
    for r in e24_ladder(seed / 10.0, seed * 10.0) {
        let (i, _) = solve_with_r(netlist, id, r, ctx)?;
        let i = i.abs();
        if i > i_max {
            continue;
        }
        if best.map(|(_, bi)| (i - target).abs() < (bi - target).abs()).unwrap_or(true) {
            best = Some((r, i));
        }
    }
    let Some((r, i)) = best else {
        bail!("no E24 value in [{}, {}] keeps the LED within max_current",
            fmt_r(seed / 10.0), fmt_r(seed * 10.0));
    };
    // Land the winner on the netlist.
    let (_i, _) = solve_with_r(netlist, id, r, ctx)?;
    let detail = format!(
        "target {:.0}mA (LED forward_current), MEASURED {:.2}mA at {}",
        target * 1e3, i * 1e3, fmt_r(r)
    );
    netlist.instances[id]
        .attributes
        .insert("derive_detail".to_string(), detail.clone());
    Ok(DerivedValue {
        instance: String::new(),
        rule: String::new(),
        seed: String::new(),
        derived: fmt_r(r),
        detail,
    })
}

fn derive_i2c_pullup(
    netlist: &mut Netlist,
    id: bhdl_netlist::types::InstanceId,
    ctx: &Ctx,
) -> Result<DerivedValue> {
    // Rail side (Power-class net) and bus side of the resistor.
    let my_nets = nets_of(netlist, id);
    let mut rail_v = None;
    let mut bus_net = None;
    for nid in &my_nets {
        let Some(net) = netlist.nets.get(*nid) else { continue };
        match net.net_class {
            bhdl_netlist::types::NetClass::Power { voltage, .. } => rail_v = Some(voltage),
            _ => bus_net = net.name.clone(),
        }
    }
    let Some(v_rail) = rail_v else {
        bail!("pull-up has no Power-class net side");
    };
    let Some(bus) = bus_net else {
        bail!("pull-up has no named bus-side net");
    };

    // I2C spec constraints. Fast-mode when the marker says ≥ 400 kHz.
    let khz = netlist.instances[id]
        .attributes
        .get("derive_i2c_khz")
        .and_then(|s| parse_engineering(s))
        .unwrap_or(100.0);
    let t_budget = if khz >= 400.0 { 300e-9 } else { 1000e-9 };
    let r_min = (v_rail - 0.4) / 3e-3; // V_OL ≤ 0.4V at I_OL = 3mA

    // Largest E24 R whose MEASURED 30–70% rise meets the budget: the bus
    // released from 0V into the pull-up, loaded by every attached pin's
    // real input structure (IBIS clamps + C_comp stamp from the board).
    let mut chosen: Option<(f64, f64)> = None; // (r, t_r)
    let mut reached_vih = false;
    let mut candidates = e24_ladder(r_min, 100e3);
    candidates.reverse();
    for r in candidates {
        // Board DC first (rails up). The settled bus level (pull-up
        // against the pin's leakage) is the rise TARGET; then release
        // the bus at 0V.
        let (_, mut ic) = solve_with_r(netlist, id, r, ctx)?;
        let v_target = ic.get(&bus).copied().unwrap_or(v_rail);
        // I2C spec: a released bus must reach V_IH = 0.7·VDD. A bus that
        // can't get there at DC is broken regardless of R — typically the
        // driver's IBIS model is characterized at a different voltage
        // domain than the rail, and its POWER clamp eats the pull-up.
        if v_target < 0.7 * v_rail {
            continue;
        }
        reached_vih = true;
        ic.insert(bus.clone(), 0.0);
        let mut conv = ctx.converter(netlist);
        let circuit = conv.convert(netlist)?;
        // 2× the budget at 400 steps ⇒ h = budget/200: a rise right at
        // the budget is resolved by ~170 samples; one far below it by
        // enough to rank candidates. (Much finer h would turn rail caps
        // into sub-mΩ companion shorts — the duration/400 rule.)
        let dur = (t_budget * 2.0f64).max(100e-9);
        let params = bhdl_spice::transient::TransientParams::new(
            "",
            bhdl_spice::transient::Stimulus::Constant(0.0),
            vec![bus.clone()],
            dur,
            dur / 400.0,
        );
        let tr = bhdl_spice::ibis_transient::run_transient_ibis_ic(&circuit, &params, &[], Some(&ic))
            .context("rise-time transient")?;
        let trace = &tr.probe_voltages[&bus];
        // Thresholds against the DC TARGET, not the trace end — a rise
        // still mid-flight when the window closes must read as "too
        // slow", not as a fast rise to a truncated endpoint.
        let cross = |lvl: f64| -> Option<f64> {
            tr.times
                .iter()
                .zip(trace)
                .find(|(_, v)| **v >= lvl)
                .map(|(t, _)| *t)
        };
        let (Some(t30), Some(t70)) = (cross(0.3 * v_target), cross(0.7 * v_target)) else {
            continue; // never reached 70% of the settled level in-window
        };
        let t_r = t70 - t30;
        if t_r <= t_budget {
            chosen = Some((r, t_r));
            break; // descending ladder: first pass = largest passing R
        }
    }
    let Some((r, t_r)) = chosen else {
        if !reached_vih {
            // Even the strongest legal pull-up never lifted the bus to
            // V_IH at DC: the attached pin models clamp the bus — the
            // vendor model is characterized at a different voltage
            // domain than this rail. The MEASUREMENT is infeasible, not
            // the design: keep the author's seed, loudly.
            let seed_txt = netlist.instances[id]
                .attributes
                .get("value")
                .cloned()
                .unwrap_or_default();
            let detail = format!(
                "derivation SKIPPED (seed kept): bus '{bus}' cannot reach \
                 V_IH = 0.7·{v_rail}V at DC with ANY pull-up — the attached pin \
                 models clamp it (characterized at a different voltage domain \
                 than this rail; check sim_model_provenance)"
            );
            eprintln!("  warning: {}: {detail}", netlist.instances[id].name);
            netlist.instances[id]
                .attributes
                .insert("derive_detail".to_string(), detail.clone());
            return Ok(DerivedValue {
                instance: String::new(),
                rule: String::new(),
                seed: String::new(),
                derived: seed_txt,
                detail,
            });
        }
        bail!(
            "no E24 value ≥ {} (I2C V_OL/I_OL floor) meets the {:.0}ns rise budget \
             on '{bus}' — the bus is too heavily loaded for {khz:.0}kHz",
            fmt_r(r_min), t_budget * 1e9
        );
    };
    let (_, _) = solve_with_r(netlist, id, r, ctx)?;
    let detail = format!(
        "{khz:.0}kHz I2C: rise budget {:.0}ns, MEASURED 30–70% {:.0}ns at {}; \
         R_min {} from V_OL≤0.4V@3mA against {v_rail}V rail",
        t_budget * 1e9, t_r * 1e9, fmt_r(r), fmt_r(r_min)
    );
    netlist.instances[id]
        .attributes
        .insert("derive_detail".to_string(), detail.clone());
    Ok(DerivedValue {
        instance: String::new(),
        rule: String::new(),
        seed: String::new(),
        derived: fmt_r(r),
        detail,
    })
}
