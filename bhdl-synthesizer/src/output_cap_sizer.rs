// Output Capacitor Sizer — post-GLACIER orchestration pass
//
// Mirrors input_cap_sizer.rs for output rails. Auto-creates output filter
// caps from `|> output_filtering(max_ripple: ...)` stage chains and sizes
// them using actual GLACIER currents.
//
// Skip logic:
//   - Rails where virtual pin expansion already created output caps (vpin_parent attr)
//   - Rails where user explicitly placed a cap with `for output_filtering(...)` intent
//
// Sizing:
//   - Switching regulators: multi-tier bank via compute_ripple_bank()
//   - Linear regulators: C = I_load × Δt_response / ΔV_ripple (Δt ≈ 25µs typical LDO)
//   - Minimum 1µF for any output cap

use log::{debug, info};
use bhdl_analyzer::AnalysisResult;
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_analyzer::spice_extraction::parse_unit_value;
use bhdl_netlist::{ConnectionPoint, InstanceId, NetId, Netlist, PinInstanceId};
use bhdl_schematic::SimulationAnnotations;

use crate::input_cap_sizer::{find_rail_voltage, find_net_by_name, find_gnd_net, RailFilteringSpec};
use crate::ripple_calculator::{compute_ripple_bank, standardize_bulk_cap};
use crate::virtual_pin_expander::{
    find_or_create_module, create_instance, connect_pin_instance_by_name,
    find_net_for_pin_instance, format_cap_value_for_attr,
};

// ── Auto-creation of output filter caps from stage chains ───────────────

/// Scan the FlowTracker rail stage map for rails with `output_filtering` stage
/// and collect their voltage + ripple specs from the analysis result.
pub fn collect_rails_needing_output_filter(
    flow_tracker: &FlowTracker,
    analysis: &AnalysisResult,
) -> Vec<RailFilteringSpec> {
    let rail_stage_map = flow_tracker.get_rail_stage_map();
    let mut specs = Vec::new();

    for (rail_name, stages) in rail_stage_map {
        let stage_idx = stages.iter().position(|(name, _)| name == "output_filtering");
        let stage_order = match stage_idx {
            Some(idx) => idx,
            None => continue,
        };

        let rail_voltage = find_rail_voltage(rail_name, analysis);
        if rail_voltage <= 0.0 {
            debug!("output_cap: skipping rail '{}': no positive voltage found", rail_name);
            continue;
        }

        // Priority: stage chain params > flow intent params > 1% default
        let max_ripple_v = flow_tracker
            .get_stage_params(rail_name, "output_filtering")
            .and_then(|p| p.get("max_ripple"))
            .and_then(|v| parse_unit_value(v))
            .or_else(|| find_output_filtering_ripple_for_rail(rail_name, flow_tracker))
            .unwrap_or(rail_voltage * 0.01);

        specs.push(RailFilteringSpec {
            rail_name: rail_name.clone(),
            rail_voltage,
            max_ripple_v,
            stage_order,
        });
    }

    specs
}

/// Auto-create seed output filter cap instances for rails that need them.
///
/// For each spec, checks if:
///   1. A cap with `intent_name = "output_filtering"` already exists on the rail net
///   2. A cap with `vpin_parent` attr exists on the rail net (virtual pin expansion)
/// If either check passes, skip (cap already present). Otherwise create a seed Cap.
///
/// Returns the names of auto-created instances.
pub fn auto_create_output_filter_caps(
    netlist: &mut Netlist,
    specs: &[RailFilteringSpec],
) -> Vec<String> {
    let mut created = Vec::new();

    for spec in specs {
        let rail_net = match find_net_by_name(netlist, &spec.rail_name) {
            Some(id) => id,
            None => {
                debug!("output_cap auto_create: rail net '{}' not found", spec.rail_name);
                continue;
            }
        };

        // Skip check 1: existing output_filtering cap (user-placed or previous auto-create)
        if has_output_filtering_cap(netlist, rail_net) {
            debug!("output_cap auto_create: rail '{}' already has output_filtering cap", spec.rail_name);
            continue;
        }

        // Skip check 2: virtual pin expansion already created a cap on this rail
        if has_vpin_expansion_cap(netlist, rail_net) {
            debug!("output_cap auto_create: rail '{}' already has vpin expansion cap", spec.rail_name);
            continue;
        }

        let gnd_net = match find_gnd_net(netlist) {
            Some(id) => id,
            None => {
                debug!("output_cap auto_create: GND net not found");
                continue;
            }
        };

        // Create seed Cap instance
        let cap_mod = find_or_create_module(netlist, "Cap", &[("1", true), ("2", true)]);
        let inst_name = format!("auto_c_out_{}", spec.rail_name);
        let ripple_str = format!("{}V", spec.max_ripple_v);
        let attrs: Vec<(&str, &str)> = vec![
            ("component_class", "capacitor"),
            ("value", "10µF"), // placeholder — sized by size_output_filter_caps()
            ("intent_name", "output_filtering"),
            ("intent_max_ripple", &ripple_str),
            ("stage_name", "output_filtering"),
            ("stage_rail", &spec.rail_name),
            ("vpin_role", "shunt"),
            ("auto_created", "true"),
        ];
        let inst_id = create_instance(netlist, &inst_name, cap_mod, &attrs);
        let pins = match netlist.create_pin_instances(inst_id) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("output_cap auto_create: failed to create pins for '{}': {}", inst_name, e);
                continue;
            }
        };

        // Pin 1 → rail, Pin 2 → GND
        if let Err(e) = connect_pin_instance_by_name(netlist, inst_id, &pins, "1", rail_net) {
            log::warn!("output_cap auto_create: connect pin 1 failed for '{}': {}", inst_name, e);
            continue;
        }
        if let Err(e) = connect_pin_instance_by_name(netlist, inst_id, &pins, "2", gnd_net) {
            log::warn!("output_cap auto_create: connect pin 2 failed for '{}': {}", inst_name, e);
            continue;
        }

        info!("Auto-created output filter cap '{}' on rail '{}' (ripple target: {:.1}mV)",
            inst_name, spec.rail_name, spec.max_ripple_v * 1e3);
        created.push(inst_name);
    }

    created
}

// ── Skip checks ─────────────────────────────────────────────────────────

/// Check if any capacitor on the given net already has `intent_name = "output_filtering"`.
fn has_output_filtering_cap(netlist: &Netlist, rail_net: NetId) -> bool {
    let net = match netlist.nets.get(rail_net) {
        Some(n) => n,
        None => return false,
    };

    for conn in &net.connections {
        let pi_id = match conn {
            ConnectionPoint::PinInstance(pi) => *pi,
            _ => continue,
        };
        let pi = match netlist.pin_instances.get(pi_id) {
            Some(pi) => pi,
            None => continue,
        };
        let inst = match netlist.instances.get(pi.instance) {
            Some(i) => i,
            None => continue,
        };
        if inst.attributes.get("intent_name").map(|s| s.as_str()) == Some("output_filtering") {
            let is_cap = inst.attributes.get("component_class").map(|s| s.as_str()) == Some("capacitor")
                || netlist.modules.get(inst.definition)
                    .map(|m| m.name.starts_with("Cap"))
                    .unwrap_or(false);
            if is_cap {
                return true;
            }
        }
    }

    false
}

/// Check if any capacitor on the given net has a `vpin_parent` attribute,
/// indicating it was created by virtual pin expansion (e.g. buck output cap).
fn has_vpin_expansion_cap(netlist: &Netlist, rail_net: NetId) -> bool {
    let net = match netlist.nets.get(rail_net) {
        Some(n) => n,
        None => return false,
    };

    for conn in &net.connections {
        let pi_id = match conn {
            ConnectionPoint::PinInstance(pi) => *pi,
            _ => continue,
        };
        let pi = match netlist.pin_instances.get(pi_id) {
            Some(pi) => pi,
            None => continue,
        };
        let inst = match netlist.instances.get(pi.instance) {
            Some(i) => i,
            None => continue,
        };
        if inst.attributes.contains_key("vpin_parent") {
            let is_cap = inst.attributes.get("component_class").map(|s| s.as_str()) == Some("capacitor")
                || netlist.modules.get(inst.definition)
                    .map(|m| m.name.starts_with("Cap"))
                    .unwrap_or(false);
            if is_cap {
                return true;
            }
        }
    }

    false
}

// ── Ripple lookup from flow intents ─────────────────────────────────────

/// Find max_ripple parameter from flow paths with output_filtering intent on a rail.
fn find_output_filtering_ripple_for_rail(rail_name: &str, flow_tracker: &FlowTracker) -> Option<f64> {
    for flow in flow_tracker.get_flow_paths() {
        let intent_matches = flow.intent.as_ref()
            .map(|i| i.name == "output_filtering")
            .unwrap_or(false);
        if !intent_matches {
            continue;
        }

        let rail_matches = flow.nets.iter().any(|n| n == rail_name);
        if !rail_matches {
            continue;
        }

        if let Some(ref intent) = flow.intent {
            for param in &intent.params {
                match param {
                    bhdl_common::IntentParam::Named(name, value) if name == "max_ripple" => {
                        if let bhdl_common::IntentValue::Number(v, _) = value {
                            return Some(*v);
                        }
                        if let bhdl_common::IntentValue::String(s) = value {
                            if let Some(v) = parse_unit_value(s) {
                                return Some(v);
                            }
                        }
                    }
                    bhdl_common::IntentParam::Positional(value) => {
                        if let bhdl_common::IntentValue::Number(v, _) = value {
                            return Some(*v);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    None
}

// ── Post-GLACIER sizing ─────────────────────────────────────────────────

/// Summary of one output cap sizing result.
#[derive(Debug)]
pub struct OutputCapSizingResult {
    pub cap_name: String,
    pub computed_cap_uf: f64,
    pub load_current_ma: f64,
    pub ripple_target_mv: f64,
    pub regulator_type: &'static str,
    pub siblings_created: usize,
}

/// Size output filter caps using actual GLACIER simulation data.
///
/// Must be called **after** `build_simulation_annotations()` and **before**
/// `apply_glacier_physical_selection()`.
pub fn size_output_filter_caps(
    netlist: &mut Netlist,
    annotations: &SimulationAnnotations,
) -> Vec<OutputCapSizingResult> {
    let candidates = find_output_cap_candidates(netlist);
    if candidates.is_empty() {
        return Vec::new();
    }

    info!("Output cap sizer: {} candidate(s) found", candidates.len());

    let mut results = Vec::new();

    for cand in candidates {
        match size_one_output_cap(netlist, annotations, &cand) {
            Ok(result) => {
                info!("Sized output cap '{}': {:.0}µF, {:.1}mA load, type={}",
                    result.cap_name, result.computed_cap_uf, result.load_current_ma, result.regulator_type);
                results.push(result);
            }
            Err(e) => {
                log::warn!("Output cap sizing failed for '{}': {}", cand.instance_name, e);
            }
        }
    }

    results
}

// ── Candidate discovery ─────────────────────────────────────────────────

struct OutputCapCandidate {
    instance_id: InstanceId,
    instance_name: String,
    #[allow(dead_code)]
    pin1_pi: PinInstanceId,
    pin1_net: NetId,
    #[allow(dead_code)]
    pin2_pi: PinInstanceId,
    pin2_net: NetId,
    max_ripple_v: f64,
    user_cap_value: Option<f64>,
    stage_name: Option<String>,
    stage_rail: Option<String>,
}

fn find_output_cap_candidates(netlist: &Netlist) -> Vec<OutputCapCandidate> {
    let mut candidates = Vec::new();

    for (inst_id, inst) in &netlist.instances {
        // Must have output_filtering intent
        match inst.attributes.get("intent_name") {
            Some(name) if name == "output_filtering" => {}
            _ => continue,
        }

        // Must have max_ripple (may be absent → will use default in sizing)
        let max_ripple_v = inst.attributes.get("intent_max_ripple")
            .and_then(|v| parse_unit_value(v))
            .unwrap_or(0.0);

        // Must be a capacitor
        let is_cap = inst.attributes.get("component_class")
            .map(|c| c == "capacitor")
            .unwrap_or(false)
            || netlist.modules.get(inst.definition)
                .map(|m| m.name.starts_with("Cap"))
                .unwrap_or(false);
        if !is_cap {
            continue;
        }

        // Find pin instances
        let pin_instances: Vec<_> = netlist.pin_instances.iter()
            .filter(|(_, pi)| pi.instance == inst_id)
            .collect();

        let mut pin1_pi = None;
        let mut pin2_pi = None;

        for (pi_id, pi) in &pin_instances {
            if let Some(pin) = netlist.pins.get(pi.pin_def) {
                match pin.name.as_str() {
                    "1" => pin1_pi = Some(*pi_id),
                    "2" => pin2_pi = Some(*pi_id),
                    _ => {}
                }
            }
        }

        let (pin1_pi, pin2_pi) = match (pin1_pi, pin2_pi) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };

        let pin1_net = match find_net_for_pin_instance(netlist, pin1_pi) {
            Some(n) => n,
            None => continue,
        };
        let pin2_net = match find_net_for_pin_instance(netlist, pin2_pi) {
            Some(n) => n,
            None => continue,
        };

        let user_cap_value = inst.attributes.get("value")
            .and_then(|v| parse_unit_value(v));

        candidates.push(OutputCapCandidate {
            instance_id: inst_id,
            instance_name: inst.name.clone(),
            pin1_pi,
            pin1_net,
            pin2_pi,
            pin2_net,
            max_ripple_v,
            user_cap_value,
            stage_name: inst.attributes.get("stage_name").cloned(),
            stage_rail: inst.attributes.get("stage_rail").cloned(),
        });
    }

    candidates
}

// ── Per-candidate sizing ────────────────────────────────────────────────

/// Typical LDO transient response time (seconds).
/// Used for sizing output caps on linear regulator rails.
const LDO_RESPONSE_TIME_S: f64 = 25e-6; // 25µs

fn size_one_output_cap(
    netlist: &mut Netlist,
    annotations: &SimulationAnnotations,
    cand: &OutputCapCandidate,
) -> Result<OutputCapSizingResult, String> {
    let rail_net = cand.pin1_net;
    let gnd_net = cand.pin2_net;

    // Get rail voltage and name
    let rail_name = netlist.nets.get(rail_net)
        .and_then(|n| n.name.clone())
        .unwrap_or_default();
    let v_out = annotations.net_voltages.get(&rail_name).copied().unwrap_or(0.0);

    if v_out <= 0.0 {
        return Err(format!("Rail '{}' has no voltage in simulation", rail_name));
    }

    // Determine ripple target: from intent attrs, or default 1% of rail voltage
    let max_ripple_v = if cand.max_ripple_v > 0.0 {
        cand.max_ripple_v
    } else {
        v_out * 0.01
    };

    // Find the upstream regulator driving this rail to determine type
    let (reg_type, reg_info) = find_upstream_regulator(netlist, annotations, rail_net);

    // Get load current: sum of all non-cap instance currents on this rail
    let load_current = compute_rail_load_current(netlist, annotations, rail_net);
    let effective_load = if load_current > 1e-6 { load_current } else { 0.01 }; // fallback 10mA

    let (computed_cap_f, reg_type_str, siblings_created) = match reg_type {
        UpstreamRegType::Switching(info) => {
            // Use multi-tier ripple bank for switching regulators
            let f_sw = info.f_sw;
            let v_in = info.v_in;
            let inductance = info.inductance;

            let bank = compute_ripple_bank(v_in, v_out, effective_load, f_sw, inductance, max_ripple_v);

            let bulk_tier = bank.tiers.iter().find(|t| t.role == "bulk");
            let bulk_f = bulk_tier.map(|t| t.capacitance * t.count as f64).unwrap_or(10e-6);

            // Update original cap with bulk tier
            if let Some(bulk_t) = bulk_tier {
                let inst = &mut netlist.instances[cand.instance_id];
                let value_str = format_cap_value_for_attr(bulk_t.capacitance);
                inst.attributes.insert("value".to_string(), value_str);
                if bulk_t.count > 1 {
                    inst.attributes.insert("bank_count".to_string(), bulk_t.count.to_string());
                    inst.attributes.insert("bank_total".to_string(),
                        format_cap_value_for_attr(bulk_t.capacitance * bulk_t.count as f64));
                }
                inst.attributes.insert("dielectric_hint".to_string(), bulk_t.dielectric_hint.to_string());
                inst.attributes.insert("output_bank_computed".to_string(), "true".to_string());
            }

            // Create sibling caps for non-bulk tiers
            let cap_mod = find_or_create_module(netlist, "Cap", &[("1", true), ("2", true)]);
            let mut siblings = 0;
            let base_name = &cand.instance_name;

            for tier in &bank.tiers {
                if tier.role == "bulk" {
                    // Create additional bulk instances if count > 1
                    if let Some(bulk_t) = bulk_tier {
                        for i in 1..bulk_t.count {
                            let name = format!("{}_bulk_{}", base_name, i + 1);
                            if create_output_sibling_cap(
                                netlist, &name, cap_mod, rail_net, gnd_net,
                                bulk_t.capacitance, bulk_t.dielectric_hint, "bulk", base_name, cand,
                            ).is_ok() {
                                siblings += 1;
                            }
                        }
                    }
                    continue;
                }

                for i in 0..tier.count {
                    let name = if tier.count == 1 {
                        format!("{}_{}", base_name, tier.role)
                    } else {
                        format!("{}_{}_{}", base_name, tier.role, i + 1)
                    };
                    if create_output_sibling_cap(
                        netlist, &name, cap_mod, rail_net, gnd_net,
                        tier.capacitance, tier.dielectric_hint, tier.role, base_name, cand,
                    ).is_ok() {
                        siblings += 1;
                    }
                }
            }

            (bulk_f, "switching", siblings)
        }
        UpstreamRegType::Linear | UpstreamRegType::Unknown => {
            // Simple bulk cap for LDO: C = I_load × Δt / ΔV
            let c_raw = effective_load * LDO_RESPONSE_TIME_S / max_ripple_v;
            let c_min = 1e-6; // minimum 1µF
            let c_needed = c_raw.max(c_min);

            let (c_per_unit, count) = standardize_bulk_cap(c_needed);
            let effective_f = c_per_unit * count as f64;

            // Only update if computed > user-specified
            let user_f = cand.user_cap_value.unwrap_or(0.0);
            if effective_f > user_f * 1.05 {
                let inst = &mut netlist.instances[cand.instance_id];
                inst.attributes.insert("value".to_string(), format_cap_value_for_attr(c_per_unit));
                if count > 1 {
                    inst.attributes.insert("bank_count".to_string(), count.to_string());
                    inst.attributes.insert("bank_total".to_string(), format_cap_value_for_attr(effective_f));
                }
                inst.attributes.insert("output_bank_computed".to_string(), "true".to_string());
            }

            // Create additional bulk instances if count > 1
            let mut siblings = 0;
            if count > 1 {
                let cap_mod = find_or_create_module(netlist, "Cap", &[("1", true), ("2", true)]);
                let base_name = &cand.instance_name;
                for i in 1..count {
                    let name = format!("{}_bulk_{}", base_name, i + 1);
                    if create_output_sibling_cap(
                        netlist, &name, cap_mod, rail_net, gnd_net,
                        c_per_unit, "X5R", "bulk", base_name, cand,
                    ).is_ok() {
                        siblings += 1;
                    }
                }
            }

            let type_str = if matches!(reg_type, UpstreamRegType::Linear) { "linear" } else { "unknown" };
            (effective_f, type_str, siblings)
        }
    };

    Ok(OutputCapSizingResult {
        cap_name: cand.instance_name.clone(),
        computed_cap_uf: computed_cap_f * 1e6,
        load_current_ma: effective_load * 1e3,
        ripple_target_mv: max_ripple_v * 1e3,
        regulator_type: reg_type_str,
        siblings_created,
    })
}

// ── Upstream regulator discovery ────────────────────────────────────────

#[derive(Debug)]
enum UpstreamRegType {
    Switching(SwitchingRegInfo),
    Linear,
    Unknown,
}

#[derive(Debug)]
struct SwitchingRegInfo {
    v_in: f64,
    f_sw: f64,
    inductance: f64,
}

/// Find the upstream regulator that drives a given output rail net.
/// Returns the regulator type and relevant parameters for sizing.
fn find_upstream_regulator(
    netlist: &Netlist,
    annotations: &SimulationAnnotations,
    rail_net_id: NetId,
) -> (UpstreamRegType, Option<InstanceId>) {
    let rail_net = match netlist.nets.get(rail_net_id) {
        Some(n) => n,
        None => return (UpstreamRegType::Unknown, None),
    };

    for conn in &rail_net.connections {
        let pi_id = match conn {
            ConnectionPoint::PinInstance(pi) => *pi,
            _ => continue,
        };

        let pi = match netlist.pin_instances.get(pi_id) {
            Some(pi) => pi,
            None => continue,
        };

        let inst_id = pi.instance;
        let inst = match netlist.instances.get(inst_id) {
            Some(i) => i,
            None => continue,
        };

        let component_class = inst.attributes.get("component_class").map(|s| s.as_str());
        let is_regulator = matches!(component_class, Some("switching_regulator") | Some("voltage_regulator"));
        if !is_regulator {
            continue;
        }

        // Check this is an output pin (VOUT/VO/OUT)
        let pin = match netlist.pins.get(pi.pin_def) {
            Some(p) => p,
            None => continue,
        };
        let pin_upper = pin.name.to_uppercase();
        let is_output_pin = pin_upper == "VOUT" || pin_upper == "VO" || pin_upper == "OUT";
        if !is_output_pin {
            continue;
        }

        if component_class == Some("switching_regulator") {
            let f_sw = inst.attributes.get("f_sw")
                .and_then(|v| parse_unit_value(v))
                .unwrap_or(500e3);

            // Find VIN voltage by looking at the regulator's VIN pin
            let v_in = find_regulator_vin_voltage(netlist, annotations, inst_id);

            // Find inductance from expansion children
            let inductance = find_expansion_inductance(netlist, inst_id)
                .unwrap_or(10e-6); // default 10µH

            return (UpstreamRegType::Switching(SwitchingRegInfo {
                v_in,
                f_sw,
                inductance,
            }), Some(inst_id));
        } else {
            return (UpstreamRegType::Linear, Some(inst_id));
        }
    }

    (UpstreamRegType::Unknown, None)
}

/// Find the input voltage of a regulator by looking at its VIN pin's net.
fn find_regulator_vin_voltage(
    netlist: &Netlist,
    annotations: &SimulationAnnotations,
    inst_id: InstanceId,
) -> f64 {
    for (pi_id, pi) in &netlist.pin_instances {
        if pi.instance != inst_id {
            continue;
        }
        let pin = match netlist.pins.get(pi.pin_def) {
            Some(p) => p,
            None => continue,
        };
        let pin_upper = pin.name.to_uppercase();
        if pin_upper != "VIN" && pin_upper != "VI" && pin_upper != "IN" {
            continue;
        }

        if let Some(net_id) = find_net_for_pin_instance(netlist, pi_id) {
            if let Some(net) = netlist.nets.get(net_id) {
                if let Some(ref name) = net.name {
                    if let Some(&voltage) = annotations.net_voltages.get(name) {
                        return voltage;
                    }
                }
            }
        }
    }

    // Fallback: try input_voltage attr
    netlist.instances.get(inst_id)
        .and_then(|i| i.attributes.get("input_voltage"))
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(0.0)
}

/// Find the inductance of the expansion inductor associated with a switching regulator.
fn find_expansion_inductance(netlist: &Netlist, reg_inst_id: InstanceId) -> Option<f64> {
    let reg_name = netlist.instances.get(reg_inst_id)?.name.clone();

    // Look for inductor with vpin_parent matching the regulator name
    for (_, inst) in &netlist.instances {
        if inst.attributes.get("vpin_parent").map(|s| s.as_str()) != Some(&reg_name) {
            continue;
        }
        // Check if it's an inductor
        let is_inductor = inst.attributes.get("component_class").map(|s| s.as_str()) == Some("inductor")
            || netlist.modules.get(inst.definition)
                .map(|m| m.name.starts_with("Ind"))
                .unwrap_or(false);
        if is_inductor {
            return inst.attributes.get("value").and_then(|v| parse_unit_value(v));
        }
    }

    None
}

/// Compute the total load current on a rail by summing non-capacitor instance currents.
fn compute_rail_load_current(
    netlist: &Netlist,
    annotations: &SimulationAnnotations,
    rail_net_id: NetId,
) -> f64 {
    let rail_net = match netlist.nets.get(rail_net_id) {
        Some(n) => n,
        None => return 0.0,
    };

    let mut total = 0.0;

    for conn in &rail_net.connections {
        let pi_id = match conn {
            ConnectionPoint::PinInstance(pi) => *pi,
            _ => continue,
        };
        let pi = match netlist.pin_instances.get(pi_id) {
            Some(pi) => pi,
            None => continue,
        };
        let inst = match netlist.instances.get(pi.instance) {
            Some(i) => i,
            None => continue,
        };

        // Skip capacitors (they don't draw DC load current)
        let is_cap = inst.attributes.get("component_class").map(|s| s.as_str()) == Some("capacitor")
            || netlist.modules.get(inst.definition)
                .map(|m| m.name.starts_with("Cap"))
                .unwrap_or(false);
        if is_cap {
            continue;
        }

        // Skip the upstream regulator's output pin (it sources, not loads)
        let component_class = inst.attributes.get("component_class").map(|s| s.as_str());
        let is_regulator = matches!(component_class, Some("switching_regulator") | Some("voltage_regulator"));
        if is_regulator {
            let pin = netlist.pins.get(pi.pin_def);
            let is_output = pin.map(|p| {
                let u = p.name.to_uppercase();
                u == "VOUT" || u == "VO" || u == "OUT"
            }).unwrap_or(false);
            if is_output {
                continue;
            }
        }

        if let Some(&current) = annotations.instance_currents.get(&inst.name) {
            total += current.abs();
        }
    }

    total
}

// ── Sibling cap creation ────────────────────────────────────────────────

fn create_output_sibling_cap(
    netlist: &mut Netlist,
    name: &str,
    cap_mod_id: bhdl_netlist::ModuleId,
    rail_net: NetId,
    gnd_net: NetId,
    capacitance: f64,
    dielectric: &str,
    role: &str,
    parent_name: &str,
    cand: &OutputCapCandidate,
) -> Result<bool, String> {
    let value_str = format_cap_value_for_attr(capacitance);
    let mut attrs: Vec<(&str, String)> = vec![
        ("component_class", "capacitor".to_string()),
        ("value", value_str),
        ("dielectric_hint", dielectric.to_string()),
        ("output_bank_role", role.to_string()),
        ("output_bank_parent", parent_name.to_string()),
        ("vpin_role", "shunt".to_string()),
        ("intent_name", "output_filtering".to_string()),
    ];

    if let Some(ref stage_name) = cand.stage_name {
        attrs.push(("stage_name", stage_name.clone()));
    }
    if let Some(ref stage_rail) = cand.stage_rail {
        attrs.push(("stage_rail", stage_rail.clone()));
    }

    let attr_refs: Vec<(&str, &str)> = attrs.iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    let inst_id = create_instance(netlist, name, cap_mod_id, &attr_refs);
    let pins = netlist.create_pin_instances(inst_id)
        .map_err(|e| format!("create pins for {}: {}", name, e))?;

    connect_pin_instance_by_name(netlist, inst_id, &pins, "1", rail_net)?;
    connect_pin_instance_by_name(netlist, inst_id, &pins, "2", gnd_net)?;

    debug!("Created output bank sibling '{}': {} {} on rail", name, format_cap_value_for_attr(capacitance), dielectric);

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use bhdl_netlist::{ModuleKind, PinDirection, PinType};

    /// Build a minimal netlist with a linear regulator driving V3_3 rail.
    fn make_linear_reg_netlist() -> (Netlist, NetId, NetId) {
        let mut nl = Netlist::default();

        // Cap module
        let cap_mod = nl.add_module("Cap".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(cap_mod, "1".to_string(), PinDirection::InOut, PinType::Passive);
        nl.add_pin(cap_mod, "2".to_string(), PinDirection::InOut, PinType::Passive);

        // Regulator module
        let reg_mod = nl.add_module("LM1117".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(reg_mod, "VI".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(reg_mod, "VO".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(reg_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        // Load resistor module
        let res_mod = nl.add_module("Res".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(res_mod, "1".to_string(), PinDirection::InOut, PinType::Passive);
        nl.add_pin(res_mod, "2".to_string(), PinDirection::InOut, PinType::Passive);

        // Nets
        let vin_net = nl.add_net(Some("V5_BUCK".to_string()));
        let v33_net = nl.add_net(Some("V3_3".to_string()));
        let gnd_net = nl.add_net(Some("GND".to_string()));

        // Regulator: V5_BUCK → V3_3
        let mut reg_attrs = HashMap::new();
        reg_attrs.insert("component_class".to_string(), "voltage_regulator".to_string());

        let reg_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "reg33".to_string(),
            definition: reg_mod,
            attributes: reg_attrs,
            layout_intents: Vec::new(),
        });
        let reg_pins = nl.create_pin_instances(reg_id).unwrap();
        nl.connect(vin_net, ConnectionPoint::PinInstance(reg_pins[0])).unwrap(); // VI
        nl.connect(v33_net, ConnectionPoint::PinInstance(reg_pins[1])).unwrap(); // VO
        nl.connect(gnd_net, ConnectionPoint::PinInstance(reg_pins[2])).unwrap(); // GND

        // Load resistor on V3_3
        let mut res_attrs = HashMap::new();
        res_attrs.insert("component_class".to_string(), "resistor".to_string());
        res_attrs.insert("value".to_string(), "330".to_string());

        let res_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "r_load33".to_string(),
            definition: res_mod,
            attributes: res_attrs,
            layout_intents: Vec::new(),
        });
        let res_pins = nl.create_pin_instances(res_id).unwrap();
        nl.connect(v33_net, ConnectionPoint::PinInstance(res_pins[0])).unwrap();
        nl.connect(gnd_net, ConnectionPoint::PinInstance(res_pins[1])).unwrap();

        (nl, v33_net, gnd_net)
    }

    fn make_annotations_for_v33() -> SimulationAnnotations {
        let mut ann = SimulationAnnotations {
            net_voltages: HashMap::new(),
            instance_currents: HashMap::new(),
            instance_power: HashMap::new(),
            port_currents: HashMap::new(),
            power_nets: HashSet::new(),
            internal_nets: HashSet::new(),
            stimulus: None,
        };
        ann.net_voltages.insert("V5_BUCK".to_string(), 5.0);
        ann.net_voltages.insert("V3_3".to_string(), 3.3);
        ann.net_voltages.insert("GND".to_string(), 0.0);
        ann.instance_currents.insert("reg33".to_string(), 0.01);
        ann.instance_currents.insert("r_load33".to_string(), 0.01);
        ann
    }

    #[test]
    fn test_auto_create_output_filter_cap() {
        let (mut nl, _v33, _gnd) = make_linear_reg_netlist();
        let initial_count = nl.instances.len();

        let specs = vec![RailFilteringSpec {
            rail_name: "V3_3".to_string(),
            rail_voltage: 3.3,
            max_ripple_v: 0.033, // 1%
            stage_order: 0,
        }];

        let created = auto_create_output_filter_caps(&mut nl, &specs);

        assert_eq!(created.len(), 1, "should create 1 output cap");
        assert_eq!(created[0], "auto_c_out_V3_3");
        assert_eq!(nl.instances.len(), initial_count + 1);

        let auto_inst = nl.instances.values()
            .find(|i| i.name == "auto_c_out_V3_3")
            .expect("auto_c_out_V3_3 should exist");
        assert_eq!(auto_inst.attributes.get("intent_name").map(|s| s.as_str()), Some("output_filtering"));
        assert_eq!(auto_inst.attributes.get("auto_created").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_auto_create_skips_existing_output_cap() {
        let (mut nl, v33, gnd) = make_linear_reg_netlist();

        // Manually add an output_filtering cap on V3_3
        let cap_mod = find_or_create_module(&mut nl, "Cap", &[("1", true), ("2", true)]);
        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), "capacitor".to_string());
        attrs.insert("intent_name".to_string(), "output_filtering".to_string());
        attrs.insert("value".to_string(), "10µF".to_string());

        let cap_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "c33".to_string(),
            definition: cap_mod,
            attributes: attrs,
            layout_intents: Vec::new(),
        });
        let cap_pins = nl.create_pin_instances(cap_id).unwrap();
        nl.connect(v33, ConnectionPoint::PinInstance(cap_pins[0])).unwrap();
        nl.connect(gnd, ConnectionPoint::PinInstance(cap_pins[1])).unwrap();

        let initial_count = nl.instances.len();

        let specs = vec![RailFilteringSpec {
            rail_name: "V3_3".to_string(),
            rail_voltage: 3.3,
            max_ripple_v: 0.033,
            stage_order: 0,
        }];

        let created = auto_create_output_filter_caps(&mut nl, &specs);
        assert!(created.is_empty(), "should skip — cap already exists");
        assert_eq!(nl.instances.len(), initial_count);
    }

    #[test]
    fn test_auto_create_skips_vpin_expansion_cap() {
        let (mut nl, v33, gnd) = make_linear_reg_netlist();

        // Add a cap with vpin_parent (simulating virtual pin expansion)
        let cap_mod = find_or_create_module(&mut nl, "Cap", &[("1", true), ("2", true)]);
        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), "capacitor".to_string());
        attrs.insert("vpin_parent".to_string(), "buck".to_string());
        attrs.insert("value".to_string(), "22µF".to_string());

        let cap_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "buck_C".to_string(),
            definition: cap_mod,
            attributes: attrs,
            layout_intents: Vec::new(),
        });
        let cap_pins = nl.create_pin_instances(cap_id).unwrap();
        nl.connect(v33, ConnectionPoint::PinInstance(cap_pins[0])).unwrap();
        nl.connect(gnd, ConnectionPoint::PinInstance(cap_pins[1])).unwrap();

        let initial_count = nl.instances.len();

        let specs = vec![RailFilteringSpec {
            rail_name: "V3_3".to_string(),
            rail_voltage: 3.3,
            max_ripple_v: 0.033,
            stage_order: 0,
        }];

        let created = auto_create_output_filter_caps(&mut nl, &specs);
        assert!(created.is_empty(), "should skip — vpin expansion cap exists");
        assert_eq!(nl.instances.len(), initial_count);
    }

    #[test]
    fn test_size_output_cap_linear_regulator() {
        let (mut nl, v33, gnd) = make_linear_reg_netlist();

        // Add output cap with output_filtering intent
        let cap_mod = find_or_create_module(&mut nl, "Cap", &[("1", true), ("2", true)]);
        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), "capacitor".to_string());
        attrs.insert("intent_name".to_string(), "output_filtering".to_string());
        attrs.insert("intent_max_ripple".to_string(), "0.033V".to_string());
        attrs.insert("value".to_string(), "10µF".to_string());

        let cap_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "auto_c_out_V3_3".to_string(),
            definition: cap_mod,
            attributes: attrs,
            layout_intents: Vec::new(),
        });
        let cap_pins = nl.create_pin_instances(cap_id).unwrap();
        nl.connect(v33, ConnectionPoint::PinInstance(cap_pins[0])).unwrap();
        nl.connect(gnd, ConnectionPoint::PinInstance(cap_pins[1])).unwrap();

        let ann = make_annotations_for_v33();
        let results = size_output_filter_caps(&mut nl, &ann);

        assert_eq!(results.len(), 1, "should size 1 output cap");
        let r = &results[0];
        assert_eq!(r.cap_name, "auto_c_out_V3_3");
        assert!(r.computed_cap_uf >= 1.0, "should compute at least 1µF for LDO");
        assert_eq!(r.regulator_type, "linear");
    }

    #[test]
    fn test_no_output_filtering_no_candidates() {
        let mut nl = Netlist::default();
        let ann = SimulationAnnotations {
            net_voltages: HashMap::new(),
            instance_currents: HashMap::new(),
            instance_power: HashMap::new(),
            port_currents: HashMap::new(),
            power_nets: HashSet::new(),
            internal_nets: HashSet::new(),
            stimulus: None,
        };
        let results = size_output_filter_caps(&mut nl, &ann);
        assert!(results.is_empty(), "no candidates = no results");
    }

    #[test]
    fn test_find_upstream_regulator_linear() {
        let (nl, v33, _gnd) = make_linear_reg_netlist();
        let ann = make_annotations_for_v33();
        let (reg_type, _) = find_upstream_regulator(&nl, &ann, v33);
        assert!(matches!(reg_type, UpstreamRegType::Linear));
    }
}
