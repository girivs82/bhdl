// Input Capacitor Sizer — post-GLACIER orchestration pass
//
// Uses actual cascade-corrected currents from SimulationAnnotations to size
// input filter capacitor banks. Runs AFTER GLACIER DC (cap values don't
// affect DC operating point) and BEFORE glacier_physical_selection.
//
// Algorithm:
// 1. Find cap instances with `intent_name = "input_filtering"` and `intent_max_ripple`
// 2. For each, find the power rail net (pin "1") and discover downstream regulators
// 3. Look up actual cascade-corrected currents from SimulationAnnotations
// 4. Compute multi-tier input bank via compute_input_bank()
// 5. Update user's cap value if computed bulk > user-specified; create sibling caps

use log::{debug, info};
use bhdl_analyzer::AnalysisResult;
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_analyzer::spice_extraction::parse_unit_value;
use bhdl_analyzer::symbol_table::SymbolKind;
use bhdl_netlist::{ConnectionPoint, InstanceId, NetId, Netlist, PinInstanceId};
use bhdl_schematic::SimulationAnnotations;

use crate::input_cap_calculator::{compute_input_bank, DownstreamRegulator, RegulatorType};
use crate::virtual_pin_expander::{
    find_or_create_module, create_instance, connect_pin_instance_by_name,
    find_net_for_pin_instance, format_cap_value_for_attr,
};

// ── Auto-creation of input filter caps from stage chains ────────────────

/// Specification for a rail that needs an auto-created input filter cap.
#[derive(Debug)]
pub struct RailFilteringSpec {
    pub rail_name: String,
    pub rail_voltage: f64,
    /// From FlowTracker flow paths or default (1% of rail voltage)
    pub max_ripple_v: f64,
    pub stage_order: usize,
}

/// Scan the FlowTracker rail stage map for rails with `input_filtering` stage
/// and collect their voltage + ripple specs from the analysis result.
///
/// The voltage comes from the net attribute on the power domain symbol.
/// The ripple target comes from any `input_filtering` intent on flow paths
/// for this rail, falling back to 1% of rail voltage (standard practice).
pub fn collect_rails_needing_input_filter(
    flow_tracker: &FlowTracker,
    analysis: &AnalysisResult,
) -> Vec<RailFilteringSpec> {
    let rail_stage_map = flow_tracker.get_rail_stage_map();
    let mut specs = Vec::new();

    for (rail_name, stages) in rail_stage_map {
        // Check if this rail has an "input_filtering" stage
        let stage_idx = stages.iter().position(|(name, _)| name == "input_filtering");
        let stage_order = match stage_idx {
            Some(idx) => idx,
            None => continue,
        };

        // Get rail voltage from net attributes in symbol tables
        let rail_voltage = find_rail_voltage(rail_name, analysis);
        if rail_voltage <= 0.0 {
            debug!("Skipping rail '{}': no positive voltage found", rail_name);
            continue;
        }

        // Get max_ripple: stage chain params > flow intent params > 1% default
        let max_ripple_v = flow_tracker
            .get_stage_params(rail_name, "input_filtering")
            .and_then(|p| p.get("max_ripple"))
            .and_then(|v| parse_unit_value(v))
            .or_else(|| find_max_ripple_for_rail(rail_name, flow_tracker))
            .unwrap_or(rail_voltage * 0.01); // default 1%

        specs.push(RailFilteringSpec {
            rail_name: rail_name.clone(),
            rail_voltage,
            max_ripple_v,
            stage_order,
        });
    }

    specs
}

/// Auto-create seed input filter cap instances for rails that need them.
///
/// For each spec, checks if a cap with `intent_name = "input_filtering"` already
/// exists on the rail net. If not, creates a seed Cap instance connected to
/// rail and GND. The subsequent `size_input_filter_caps()` will size it properly.
///
/// Returns the names of auto-created instances.
pub fn auto_create_input_filter_caps(
    netlist: &mut Netlist,
    specs: &[RailFilteringSpec],
) -> Vec<String> {
    let mut created = Vec::new();

    for spec in specs {
        // Find rail net by name
        let rail_net = match find_net_by_name(netlist, &spec.rail_name) {
            Some(id) => id,
            None => {
                debug!("auto_create: rail net '{}' not found in netlist", spec.rail_name);
                continue;
            }
        };

        // Check if any cap on this net already has input_filtering intent → skip
        if has_input_filtering_cap(netlist, rail_net) {
            debug!("auto_create: rail '{}' already has input_filtering cap, skipping", spec.rail_name);
            continue;
        }

        // Find GND net
        let gnd_net = match find_gnd_net(netlist) {
            Some(id) => id,
            None => {
                debug!("auto_create: GND net not found");
                continue;
            }
        };

        // Create seed Cap instance
        let cap_mod = find_or_create_module(netlist, "Cap", &[("1", true), ("2", true)]);
        let inst_name = format!("auto_c_in_{}", spec.rail_name);
        let ripple_str = format!("{}V", spec.max_ripple_v);
        let attrs: Vec<(&str, &str)> = vec![
            ("component_class", "capacitor"),
            ("value", "10µF"), // placeholder — sized by size_input_filter_caps()
            ("intent_name", "input_filtering"),
            ("intent_max_ripple", &ripple_str),
            ("stage_name", "input_filtering"),
            ("stage_rail", &spec.rail_name),
            ("vpin_role", "shunt"),
            ("auto_created", "true"),
        ];
        let inst_id = create_instance(netlist, &inst_name, cap_mod, &attrs);
        let pins = match netlist.create_pin_instances(inst_id) {
            Ok(p) => p,
            Err(e) => {
                log::warn!("auto_create: failed to create pins for '{}': {}", inst_name, e);
                continue;
            }
        };

        // Pin 1 → rail, Pin 2 → GND
        if let Err(e) = connect_pin_instance_by_name(netlist, inst_id, &pins, "1", rail_net) {
            log::warn!("auto_create: connect pin 1 failed for '{}': {}", inst_name, e);
            continue;
        }
        if let Err(e) = connect_pin_instance_by_name(netlist, inst_id, &pins, "2", gnd_net) {
            log::warn!("auto_create: connect pin 2 failed for '{}': {}", inst_name, e);
            continue;
        }

        info!("Auto-created input filter cap '{}' on rail '{}' (ripple target: {:.1}mV)",
            inst_name, spec.rail_name, spec.max_ripple_v * 1e3);
        created.push(inst_name);
    }

    created
}

/// Look up rail voltage from net attributes in analysis symbol tables.
pub(crate) fn find_rail_voltage(rail_name: &str, analysis: &AnalysisResult) -> f64 {
    // Check global scope nets
    if let Some(sym) = analysis.global_scope.get_nets().get(rail_name) {
        if sym.kind == SymbolKind::Net {
            if let Some(ref attr) = sym.net_attributes {
                if let Some(v) = attr.voltage() {
                    return v;
                }
            }
        }
    }

    // Check definition scopes (power domains are typically in board scope)
    for (_, scope) in &analysis.definition_scopes {
        if let Some(sym) = scope.get_nets().get(rail_name) {
            if sym.kind == SymbolKind::Net {
                if let Some(ref attr) = sym.net_attributes {
                    if let Some(v) = attr.voltage() {
                        return v;
                    }
                }
            }
        }
    }

    0.0
}

/// Find max_ripple parameter from flow paths with input_filtering intent on a rail.
fn find_max_ripple_for_rail(rail_name: &str, flow_tracker: &FlowTracker) -> Option<f64> {
    for flow in flow_tracker.get_flow_paths() {
        let intent_matches = flow.intent.as_ref()
            .map(|i| i.name == "input_filtering")
            .unwrap_or(false);
        if !intent_matches {
            continue;
        }

        let rail_matches = flow.nets.iter().any(|n| n == rail_name);
        if !rail_matches {
            continue;
        }

        // Extract max_ripple from intent params
        if let Some(ref intent) = flow.intent {
            for param in &intent.params {
                match param {
                    bhdl_common::IntentParam::Named(name, value) if name == "max_ripple" => {
                        if let bhdl_common::IntentValue::Number(v, _) = value {
                            return Some(*v);
                        }
                        // Try parsing string representation
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

/// Find a net by name in the netlist.
pub(crate) fn find_net_by_name(netlist: &Netlist, name: &str) -> Option<NetId> {
    for (net_id, net) in &netlist.nets {
        if net.name.as_deref() == Some(name) {
            return Some(net_id);
        }
    }
    None
}

/// Find the GND net in the netlist (name "GND" or "0").
pub(crate) fn find_gnd_net(netlist: &Netlist) -> Option<NetId> {
    for (net_id, net) in &netlist.nets {
        match net.name.as_deref() {
            Some("GND") | Some("0") => return Some(net_id),
            _ => {}
        }
    }
    None
}

/// Check if any capacitor on the given net already has `intent_name = "input_filtering"`.
fn has_input_filtering_cap(netlist: &Netlist, rail_net: NetId) -> bool {
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
        if inst.attributes.get("intent_name").map(|s| s.as_str()) == Some("input_filtering") {
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

/// Summary of one input cap sizing result.
#[derive(Debug)]
pub struct InputCapSizingResult {
    pub cap_name: String,
    pub computed_bulk_uf: f64,
    pub total_load_ma: f64,
    pub regulator_count: usize,
    pub ripple_target_mv: f64,
    pub rms_current_ma: f64,
    pub siblings_created: usize,
}

/// Size input filter caps using actual GLACIER simulation data.
///
/// Must be called **after** `build_simulation_annotations()` (which includes
/// cascade current fixup) and **before** `glacier_physical_selection()`.
pub fn size_input_filter_caps(
    netlist: &mut Netlist,
    annotations: &SimulationAnnotations,
) -> Vec<InputCapSizingResult> {
    // Phase 1: Find candidate input caps (immutable scan)
    let candidates = find_input_cap_candidates(netlist);
    if candidates.is_empty() {
        return Vec::new();
    }

    info!("Input cap sizer: {} candidate(s) found", candidates.len());

    let mut results = Vec::new();

    for cand in candidates {
        match size_one_input_cap(netlist, annotations, &cand) {
            Ok(result) => {
                info!("Sized input cap '{}': {:.0}µF bulk, {:.1}mA load, {} regulators",
                    result.cap_name, result.computed_bulk_uf, result.total_load_ma, result.regulator_count);
                results.push(result);
            }
            Err(e) => {
                log::warn!("Input cap sizing failed for '{}': {}", cand.instance_name, e);
            }
        }
    }

    results
}

// ── Candidate discovery ─────────────────────────────────────────────────

struct InputCapCandidate {
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
    /// Stage/intent metadata to propagate to sibling caps
    stage_name: Option<String>,
    stage_order: Option<String>,
    stage_rail: Option<String>,
}

fn find_input_cap_candidates(netlist: &Netlist) -> Vec<InputCapCandidate> {
    let mut candidates = Vec::new();

    for (inst_id, inst) in &netlist.instances {
        // Must have input_filtering intent
        match inst.attributes.get("intent_name") {
            Some(name) if name == "input_filtering" => {}
            _ => continue,
        }

        // Must have max_ripple
        let max_ripple_str = match inst.attributes.get("intent_max_ripple") {
            Some(v) => v.clone(),
            None => continue,
        };

        let max_ripple_v = match parse_unit_value(&max_ripple_str) {
            Some(v) if v > 0.0 => v,
            _ => {
                debug!("Skipping '{}': could not parse max_ripple '{}'", inst.name, max_ripple_str);
                continue;
            }
        };

        // Must be a capacitor
        let is_cap = inst.attributes.get("component_class")
            .map(|c| c == "capacitor")
            .unwrap_or(false)
            || netlist.modules.get(inst.definition)
                .map(|m| m.name.starts_with("Cap"))
                .unwrap_or(false);
        if !is_cap {
            debug!("Skipping '{}': not a capacitor", inst.name);
            continue;
        }

        // Find pin instances for this cap (pin "1" and pin "2")
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
            _ => {
                debug!("Skipping '{}': could not find pin 1 and pin 2", inst.name);
                continue;
            }
        };

        // Find nets
        let pin1_net = match find_net_for_pin_instance(netlist, pin1_pi) {
            Some(n) => n,
            None => continue,
        };
        let pin2_net = match find_net_for_pin_instance(netlist, pin2_pi) {
            Some(n) => n,
            None => continue,
        };

        // Parse user's cap value
        let user_cap_value = inst.attributes.get("value")
            .and_then(|v| parse_unit_value(v));

        candidates.push(InputCapCandidate {
            instance_id: inst_id,
            instance_name: inst.name.clone(),
            pin1_pi,
            pin1_net,
            pin2_pi,
            pin2_net,
            max_ripple_v,
            user_cap_value,
            stage_name: inst.attributes.get("stage_name").cloned(),
            stage_order: inst.attributes.get("stage_order").cloned(),
            stage_rail: inst.attributes.get("stage_rail").cloned(),
        });
    }

    candidates
}

// ── Per-candidate sizing ────────────────────────────────────────────────

fn size_one_input_cap(
    netlist: &mut Netlist,
    annotations: &SimulationAnnotations,
    cand: &InputCapCandidate,
) -> Result<InputCapSizingResult, String> {
    // pin "1" connects to the power rail; find regulators on that rail
    let rail_net = cand.pin1_net;
    let gnd_net = cand.pin2_net;

    // Get rail voltage from annotations
    let rail_name = netlist.nets.get(rail_net)
        .and_then(|n| n.name.clone())
        .unwrap_or_default();
    let v_in = annotations.net_voltages.get(&rail_name).copied().unwrap_or(0.0);

    if v_in <= 0.0 {
        return Err(format!("Rail '{}' has no voltage in simulation", rail_name));
    }

    // Find downstream regulators on this rail
    let regulators = find_regulators_on_rail(netlist, annotations, rail_net);

    if regulators.is_empty() {
        return Err(format!("No regulators found on rail '{}'", rail_name));
    }

    let total_load: f64 = regulators.iter().map(|r| r.i_load).sum();

    // Compute input bank
    let bank = compute_input_bank(v_in, &regulators, cand.max_ripple_v);

    // Find the bulk tier capacitance
    let bulk_tier = bank.tiers.iter().find(|t| t.role == "bulk");
    let computed_bulk_f = bulk_tier
        .map(|t| t.capacitance * t.count as f64)
        .unwrap_or(100e-6);

    // Determine effective bulk value: max(computed, user-specified)
    let user_bulk_f = cand.user_cap_value.unwrap_or(0.0);
    let effective_bulk_f = computed_bulk_f.max(user_bulk_f);

    // Update the original cap's value if computed > user
    if computed_bulk_f > user_bulk_f * 1.05 {
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
            inst.attributes.insert("input_bank_computed".to_string(), "true".to_string());
            debug!("Updated '{}' value to {} (was {})",
                cand.instance_name,
                format_cap_value_for_attr(bulk_t.capacitance),
                cand.user_cap_value.map(|v| format_cap_value_for_attr(v)).unwrap_or_default());
        }
    }

    // Create sibling caps for non-bulk tiers (HF bypass, mid-freq)
    let mut siblings_created = 0;
    let base_name = &cand.instance_name;

    // Build stage attrs for siblings
    let cap_mod = find_or_create_module(netlist, "Cap", &[("1", true), ("2", true)]);

    for tier in &bank.tiers {
        if tier.role == "bulk" {
            // Bulk tier updates the original cap (already handled above)
            // But if count > 1, create additional bulk instances
            if let Some(bulk_t) = bulk_tier {
                if bulk_t.count > 1 {
                    for i in 1..bulk_t.count {
                        let sibling_name = format!("{}_bulk_{}", base_name, i + 1);
                        let created = create_sibling_cap(
                            netlist, &sibling_name, cap_mod,
                            rail_net, gnd_net,
                            bulk_t.capacitance, bulk_t.dielectric_hint,
                            "bulk", base_name, cand,
                        )?;
                        if created {
                            siblings_created += 1;
                        }
                    }
                }
            }
            continue;
        }

        // For HF bypass and mid-freq, create the required number of sibling instances
        for i in 0..tier.count {
            let suffix = if tier.count == 1 {
                format!("{}_{}", base_name, tier.role)
            } else {
                format!("{}_{}_{}", base_name, tier.role, i + 1)
            };
            let created = create_sibling_cap(
                netlist, &suffix, cap_mod,
                rail_net, gnd_net,
                tier.capacitance, tier.dielectric_hint,
                tier.role, base_name, cand,
            )?;
            if created {
                siblings_created += 1;
            }
        }
    }

    Ok(InputCapSizingResult {
        cap_name: cand.instance_name.clone(),
        computed_bulk_uf: effective_bulk_f * 1e6,
        total_load_ma: total_load * 1e3,
        regulator_count: regulators.len(),
        ripple_target_mv: cand.max_ripple_v * 1e3,
        rms_current_ma: bank.rms_current * 1e3,
        siblings_created,
    })
}

// ── Regulator discovery on a rail ───────────────────────────────────────

fn find_regulators_on_rail(
    netlist: &Netlist,
    annotations: &SimulationAnnotations,
    rail_net_id: NetId,
) -> Vec<DownstreamRegulator> {
    let mut regs = Vec::new();

    let rail_net = match netlist.nets.get(rail_net_id) {
        Some(n) => n,
        None => return regs,
    };

    // Scan all connections on the rail net for regulator input pins
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

        // Is this a regulator?
        let component_class = inst.attributes.get("component_class").map(|s| s.as_str());
        let is_regulator = matches!(component_class, Some("switching_regulator") | Some("voltage_regulator"));
        if !is_regulator {
            continue;
        }

        // Is this the input power pin?
        let pin = match netlist.pins.get(pi.pin_def) {
            Some(p) => p,
            None => continue,
        };
        let pin_name_upper = pin.name.to_uppercase();
        let is_input_pin = pin_name_upper == "VIN" || pin_name_upper == "VI" || pin_name_upper == "IN";
        if !is_input_pin {
            continue;
        }

        // Get actual cascade-corrected current from GLACIER annotations
        let i_load = annotations.instance_currents.get(&inst.name)
            .copied()
            .unwrap_or(0.0)
            .abs();

        if i_load < 1e-6 {
            debug!("Skipping regulator '{}': negligible current ({:.6}A)", inst.name, i_load);
            continue;
        }

        // Find VOUT net and voltage
        let v_out = find_regulator_vout_voltage(netlist, annotations, inst_id);

        // Determine regulator type and switching frequency
        let reg_type = if component_class == Some("switching_regulator") {
            RegulatorType::Switching
        } else {
            RegulatorType::Linear
        };

        let f_sw = inst.attributes.get("f_sw")
            .and_then(|v| parse_unit_value(v))
            .unwrap_or(500e3); // default 500kHz

        debug!("Found regulator '{}' on rail: type={:?}, I={:.3}A, Vout={:.2}V, f_sw={:.0}kHz",
            inst.name, reg_type, i_load, v_out, f_sw / 1e3);

        regs.push(DownstreamRegulator {
            name: inst.name.clone(),
            reg_type,
            v_out,
            i_load,
            f_sw,
        });
    }

    regs
}

/// Find the output voltage of a regulator by looking at its VOUT/VO pin's net
/// and reading the voltage from annotations.
fn find_regulator_vout_voltage(
    netlist: &Netlist,
    annotations: &SimulationAnnotations,
    inst_id: InstanceId,
) -> f64 {
    let inst = match netlist.instances.get(inst_id) {
        Some(i) => i,
        None => return 0.0,
    };

    // Find VOUT pin instance
    for (pi_id, pi) in &netlist.pin_instances {
        if pi.instance != inst_id {
            continue;
        }
        let pin = match netlist.pins.get(pi.pin_def) {
            Some(p) => p,
            None => continue,
        };
        let pin_upper = pin.name.to_uppercase();
        if pin_upper != "VOUT" && pin_upper != "VO" && pin_upper != "OUT" {
            continue;
        }

        // Find the net this pin connects to
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

    // Fallback: try to read from instance attributes (e.g. "output_voltage")
    inst.attributes.get("output_voltage")
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(0.0)
}

// ── Sibling cap creation ────────────────────────────────────────────────

fn create_sibling_cap(
    netlist: &mut Netlist,
    name: &str,
    cap_mod_id: bhdl_netlist::ModuleId,
    rail_net: NetId,
    gnd_net: NetId,
    capacitance: f64,
    dielectric: &str,
    role: &str,
    parent_name: &str,
    cand: &InputCapCandidate,
) -> Result<bool, String> {
    // Build attributes
    let value_str = format_cap_value_for_attr(capacitance);
    let mut attrs: Vec<(&str, String)> = vec![
        ("component_class", "capacitor".to_string()),
        ("value", value_str),
        ("dielectric_hint", dielectric.to_string()),
        ("input_bank_role", role.to_string()),
        ("input_bank_parent", parent_name.to_string()),
        ("vpin_role", "shunt".to_string()),
        ("intent_name", "input_filtering".to_string()),
    ];

    if let Some(ref stage_name) = cand.stage_name {
        attrs.push(("stage_name", stage_name.clone()));
    }
    if let Some(ref stage_order) = cand.stage_order {
        attrs.push(("stage_order", stage_order.clone()));
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

    // Pin 1 → rail net, Pin 2 → GND net
    connect_pin_instance_by_name(netlist, inst_id, &pins, "1", rail_net)?;
    connect_pin_instance_by_name(netlist, inst_id, &pins, "2", gnd_net)?;

    debug!("Created input bank sibling '{}': {} {} on rail", name, format_cap_value_for_attr(capacitance), dielectric);

    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, HashSet};
    use bhdl_netlist::{ModuleKind, PinDirection, PinType};

    /// Build a minimal netlist with a cap on a power rail with a regulator.
    fn make_test_netlist() -> (Netlist, InstanceId, NetId, NetId) {
        let mut nl = Netlist::default();

        // Cap module
        let cap_mod = nl.add_module("Cap".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(cap_mod, "1".to_string(), PinDirection::InOut, PinType::Passive);
        nl.add_pin(cap_mod, "2".to_string(), PinDirection::InOut, PinType::Passive);

        // Regulator module
        let reg_mod = nl.add_module("BuckRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(reg_mod, "VIN".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(reg_mod, "VOUT".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(reg_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        // Nets
        let vin_net = nl.add_net(Some("VIN".to_string()));
        let gnd_net = nl.add_net(Some("GND".to_string()));
        let vout_net = nl.add_net(Some("V5_BUCK".to_string()));

        // Cap instance: c_in on VIN with input_filtering intent
        let mut cap_attrs = HashMap::new();
        cap_attrs.insert("component_class".to_string(), "capacitor".to_string());
        cap_attrs.insert("value".to_string(), "100µF".to_string());
        cap_attrs.insert("intent_name".to_string(), "input_filtering".to_string());
        cap_attrs.insert("intent_max_ripple".to_string(), "0.05V".to_string());

        let cap_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "c_in".to_string(),
            definition: cap_mod,
            attributes: cap_attrs,
            layout_intents: Vec::new(),
        });
        let cap_pins = nl.create_pin_instances(cap_id).unwrap();
        nl.connect(vin_net, ConnectionPoint::PinInstance(cap_pins[0])).unwrap(); // pin 1 → VIN
        nl.connect(gnd_net, ConnectionPoint::PinInstance(cap_pins[1])).unwrap(); // pin 2 → GND

        // Regulator instance: buck on VIN→V5_BUCK
        let mut reg_attrs = HashMap::new();
        reg_attrs.insert("component_class".to_string(), "switching_regulator".to_string());
        reg_attrs.insert("f_sw".to_string(), "500kHz".to_string());

        let reg_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "buck".to_string(),
            definition: reg_mod,
            attributes: reg_attrs,
            layout_intents: Vec::new(),
        });
        let reg_pins = nl.create_pin_instances(reg_id).unwrap();
        nl.connect(vin_net, ConnectionPoint::PinInstance(reg_pins[0])).unwrap(); // VIN
        nl.connect(vout_net, ConnectionPoint::PinInstance(reg_pins[1])).unwrap(); // VOUT
        nl.connect(gnd_net, ConnectionPoint::PinInstance(reg_pins[2])).unwrap(); // GND

        (nl, cap_id, vin_net, gnd_net)
    }

    fn make_test_annotations() -> SimulationAnnotations {
        let mut ann = SimulationAnnotations {
            net_voltages: HashMap::new(),
            instance_currents: HashMap::new(),
            instance_power: HashMap::new(),
            port_currents: HashMap::new(),
            power_nets: HashSet::new(),
            internal_nets: HashSet::new(),
            stimulus: None,
            transients: Vec::new(),
        };
        ann.net_voltages.insert("VIN".to_string(), 24.0);
        ann.net_voltages.insert("V5_BUCK".to_string(), 5.0);
        ann.net_voltages.insert("GND".to_string(), 0.0);
        ann.instance_currents.insert("buck".to_string(), 0.5085);
        ann.instance_power.insert("buck".to_string(), 9.66);
        ann
    }

    #[test]
    fn test_find_input_cap_candidates() {
        let (nl, _cap_id, _vin, _gnd) = make_test_netlist();
        let candidates = find_input_cap_candidates(&nl);
        assert_eq!(candidates.len(), 1, "should find 1 input cap candidate");
        assert_eq!(candidates[0].instance_name, "c_in");
        assert!((candidates[0].max_ripple_v - 0.05).abs() < 1e-6);
    }

    #[test]
    fn test_find_regulators_on_rail() {
        let (nl, _cap_id, vin_net, _gnd) = make_test_netlist();
        let ann = make_test_annotations();
        let regs = find_regulators_on_rail(&nl, &ann, vin_net);
        assert_eq!(regs.len(), 1, "should find 1 regulator on VIN");
        assert_eq!(regs[0].name, "buck");
        assert_eq!(regs[0].reg_type, RegulatorType::Switching);
        assert!((regs[0].i_load - 0.5085).abs() < 1e-4);
    }

    #[test]
    fn test_size_input_filter_caps_creates_siblings() {
        let (mut nl, _cap_id, _vin, _gnd) = make_test_netlist();
        let ann = make_test_annotations();

        let initial_count = nl.instances.len();
        let results = size_input_filter_caps(&mut nl, &ann);

        assert_eq!(results.len(), 1, "should size 1 input cap");
        let r = &results[0];
        assert_eq!(r.cap_name, "c_in");
        assert!(r.computed_bulk_uf > 0.0, "should compute nonzero bulk");
        assert_eq!(r.regulator_count, 1);
        assert!(r.total_load_ma > 500.0, "should see ~508mA load");
        assert!(r.siblings_created > 0, "should create HF bypass + mid-freq siblings");

        // Should have created new instances
        let final_count = nl.instances.len();
        assert!(final_count > initial_count,
            "should have more instances: {} → {}", initial_count, final_count);
    }

    #[test]
    fn test_no_intent_no_sizing() {
        let mut nl = Netlist::default();
        // Empty netlist — no candidates
        let ann = SimulationAnnotations {
            net_voltages: HashMap::new(),
            instance_currents: HashMap::new(),
            instance_power: HashMap::new(),
            port_currents: HashMap::new(),
            power_nets: HashSet::new(),
            internal_nets: HashSet::new(),
            stimulus: None,
            transients: Vec::new(),
        };
        let results = size_input_filter_caps(&mut nl, &ann);
        assert!(results.is_empty(), "no candidates = no results");
    }

    // ── Auto-creation tests ─────────────────────────────────────────────

    /// Build a netlist with a regulator on VIN but NO input filter cap.
    fn make_netlist_without_input_cap() -> (Netlist, NetId, NetId) {
        let mut nl = Netlist::default();

        // Cap module (needed for auto-creation to find_or_create)
        let _cap_mod = nl.add_module("Cap".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(_cap_mod, "1".to_string(), PinDirection::InOut, PinType::Passive);
        nl.add_pin(_cap_mod, "2".to_string(), PinDirection::InOut, PinType::Passive);

        // Regulator module
        let reg_mod = nl.add_module("BuckRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(reg_mod, "VIN".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(reg_mod, "VOUT".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(reg_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        // Nets
        let vin_net = nl.add_net(Some("VIN".to_string()));
        let gnd_net = nl.add_net(Some("GND".to_string()));
        let vout_net = nl.add_net(Some("V5_BUCK".to_string()));

        // Regulator instance only — no input cap
        let mut reg_attrs = HashMap::new();
        reg_attrs.insert("component_class".to_string(), "switching_regulator".to_string());

        let reg_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "buck".to_string(),
            definition: reg_mod,
            attributes: reg_attrs,
            layout_intents: Vec::new(),
        });
        let reg_pins = nl.create_pin_instances(reg_id).unwrap();
        nl.connect(vin_net, ConnectionPoint::PinInstance(reg_pins[0])).unwrap();
        nl.connect(vout_net, ConnectionPoint::PinInstance(reg_pins[1])).unwrap();
        nl.connect(gnd_net, ConnectionPoint::PinInstance(reg_pins[2])).unwrap();

        (nl, vin_net, gnd_net)
    }

    #[test]
    fn test_auto_create_input_filter_cap() {
        let (mut nl, _vin, _gnd) = make_netlist_without_input_cap();
        let initial_count = nl.instances.len();

        let specs = vec![RailFilteringSpec {
            rail_name: "VIN".to_string(),
            rail_voltage: 24.0,
            max_ripple_v: 0.24, // 1% of 24V
            stage_order: 1,
        }];

        let created = auto_create_input_filter_caps(&mut nl, &specs);

        assert_eq!(created.len(), 1, "should create 1 auto cap");
        assert_eq!(created[0], "auto_c_in_VIN");

        // Check instance was created
        assert_eq!(nl.instances.len(), initial_count + 1);

        // Find the auto-created instance and verify attributes
        let auto_inst = nl.instances.values()
            .find(|i| i.name == "auto_c_in_VIN")
            .expect("auto_c_in_VIN should exist");
        assert_eq!(auto_inst.attributes.get("intent_name").map(|s| s.as_str()), Some("input_filtering"));
        assert_eq!(auto_inst.attributes.get("component_class").map(|s| s.as_str()), Some("capacitor"));
        assert_eq!(auto_inst.attributes.get("auto_created").map(|s| s.as_str()), Some("true"));
    }

    #[test]
    fn test_auto_create_skips_existing_cap() {
        // Use the standard netlist that already has c_in with input_filtering
        let (mut nl, _cap_id, _vin, _gnd) = make_test_netlist();
        let initial_count = nl.instances.len();

        let specs = vec![RailFilteringSpec {
            rail_name: "VIN".to_string(),
            rail_voltage: 24.0,
            max_ripple_v: 0.24,
            stage_order: 1,
        }];

        let created = auto_create_input_filter_caps(&mut nl, &specs);

        assert!(created.is_empty(), "should skip — cap already exists");
        assert_eq!(nl.instances.len(), initial_count, "no new instances");
    }

    #[test]
    fn test_auto_create_no_gnd_net() {
        let mut nl = Netlist::default();
        // Only a VIN net, no GND
        let _vin = nl.add_net(Some("VIN".to_string()));

        let specs = vec![RailFilteringSpec {
            rail_name: "VIN".to_string(),
            rail_voltage: 12.0,
            max_ripple_v: 0.12,
            stage_order: 0,
        }];

        let created = auto_create_input_filter_caps(&mut nl, &specs);
        assert!(created.is_empty(), "should skip — no GND net");
    }
}
