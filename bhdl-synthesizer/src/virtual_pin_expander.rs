//! Virtual Pin Expansion — post-synthesis pass
//!
//! After the main synthesis builds the netlist, this pass scans for instances
//! whose module has a virtual VOUT pin (identified by `component_class =
//! "switching_regulator"`) and expands them into concrete inductor, catch
//! diode, and output capacitor instances wired to the correct nets.
//!
//! The expansion creates an internal SW (switch-node) net and rewires the
//! buck IC's VOUT pin to it, inserting:
//!   - Inductor: SW → original VOUT net
//!   - Diode (optional): GND → SW (catch / freewheeling)
//!   - Output cap: VOUT → GND
//!
//! GLACIER DC simulation then processes the expanded netlist normally:
//! the buck IC becomes a VoltageRegulator decomposition with
//! VoltageSource(SW→GND) + dropout(VIN→SW), the inductor is a DC short,
//! and the diode/cap have no DC contribution.

use std::collections::HashMap;
use log::{debug, info};
use bhdl_netlist::{
    ConnectionPoint, InstanceId, ModuleId, ModuleKind, NetId, Netlist,
    PinDirection, PinInstanceId, PinType,
};
use bhdl_analyzer::spice_extraction::parse_unit_value;
use crate::ripple_calculator::compute_ripple_bank;

/// Summary of one virtual-pin expansion.
#[derive(Debug)]
pub struct ExpansionResult {
    pub regulator_name: String,
    pub inductor_name: String,
    pub diode_name: Option<String>,
    pub output_cap_name: String,
    /// Name of the switching node net, e.g. "buck_SW" or "buck_LX"
    pub sw_net_name: String,
    /// Switching node pin name from library (e.g. "SW", "LX", "PH")
    pub sw_pin_name: String,
    /// Additional output capacitor names (for multi-tier ripple banks)
    pub additional_output_caps: Vec<String>,
}

/// Expand all virtual pins in the netlist.
///
/// Call this **after** synthesis but **before** GLACIER simulation.
pub fn expand_virtual_pins(netlist: &mut Netlist) -> Vec<ExpansionResult> {
    // Phase 1 — identify candidates (immutable scan)
    let candidates = find_candidates(netlist);
    if candidates.is_empty() {
        return Vec::new();
    }

    info!("Virtual pin expansion: {} candidate(s) found", candidates.len());

    let mut results = Vec::new();

    for cand in candidates {
        match expand_one(netlist, &cand) {
            Ok(result) => {
                info!("Expanded virtual pin for {} → L={}, D={:?}, COUT={}",
                      result.regulator_name, result.inductor_name,
                      result.diode_name, result.output_cap_name);
                results.push(result);
            }
            Err(e) => {
                log::warn!("Virtual pin expansion failed for {}: {}", cand.instance_name, e);
            }
        }
    }

    results
}

// ── Candidate discovery ─────────────────────────────────────────────────

struct Candidate {
    instance_id: InstanceId,
    instance_name: String,
    vout_pin_inst: PinInstanceId,
    vout_net: NetId,
    gnd_net: NetId,
    // Expansion attributes (read from instance)
    sw_name: String,
    inductor_value: String,
    diode_vf: String,
    cout_value: String,
    has_diode: bool,
}

fn find_candidates(netlist: &Netlist) -> Vec<Candidate> {
    let mut candidates = Vec::new();

    for (inst_id, inst) in &netlist.instances {
        // Only switching regulators participate
        let class = inst.attributes.get("component_class");
        if class.map(|c| c.as_str()) != Some("switching_regulator") {
            continue;
        }

        // Skip instances already expanded by the expansion interpreter
        if inst.attributes.contains_key("expansion_applied") {
            continue;
        }

        // Read expansion attributes (with defaults)
        let sw_name = inst.attributes.get("vpin_sw_name")
            .cloned().unwrap_or_else(|| "SW".to_string());
        let inductor_value = inst.attributes.get("vpin_inductor")
            .cloned().unwrap_or_else(|| "33µH".to_string());
        let diode_vf = inst.attributes.get("vpin_diode_vf")
            .cloned().unwrap_or_else(|| "0.5V".to_string());
        let cout_value = inst.attributes.get("vpin_cout")
            .cloned().unwrap_or_else(|| "470µF".to_string());
        let has_diode = inst.attributes.get("vpin_has_diode")
            .map(|v| v != "false" && v != "0")
            .unwrap_or(true);

        // Locate VOUT and GND pin instances
        let module_def = match netlist.modules.get(inst.definition) {
            Some(d) => d,
            None => continue,
        };

        let mut vout_pin_inst: Option<PinInstanceId> = None;
        let mut vout_net: Option<NetId> = None;
        let mut gnd_net: Option<NetId> = None;

        for &pin_id in &module_def.pins {
            let pin = match netlist.pins.get(pin_id) {
                Some(p) => p,
                None => continue,
            };

            // Find pin instance for this (instance, pin_def) pair
            let pi_id = netlist.pin_instances.iter()
                .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin_id)
                .map(|(id, _)| id);

            let pi_id = match pi_id {
                Some(id) => id,
                None => continue,
            };

            let pi_net = netlist.pin_instances.get(pi_id).and_then(|pi| pi.net);

            match (pin.pin_type, pin.direction) {
                (PinType::Power, PinDirection::Out) | (PinType::Signal, PinDirection::Out) => {
                    // VOUT — power out or signal out on a regulator
                    if pin.name.to_uppercase().contains("OUT") || pin.name == "VO" || pin.name == "VOUT" {
                        vout_pin_inst = Some(pi_id);
                        vout_net = pi_net;
                    }
                }
                (PinType::Ground, _) | (_, PinDirection::Ground) => {
                    gnd_net = pi_net;
                }
                _ => {}
            }
        }

        // Need both VOUT and GND nets to proceed
        let (vout_pi, vout_nid, gnd_nid) = match (vout_pin_inst, vout_net, gnd_net) {
            (Some(pi), Some(v), Some(g)) => (pi, v, g),
            _ => {
                debug!("Skipping {} — could not identify VOUT/GND nets", inst.name);
                continue;
            }
        };

        candidates.push(Candidate {
            instance_id: inst_id,
            instance_name: inst.name.clone(),
            vout_pin_inst: vout_pi,
            vout_net: vout_nid,
            gnd_net: gnd_nid,
            sw_name,
            inductor_value,
            diode_vf,
            cout_value,
            has_diode,
        });
    }

    candidates
}

// ── Single-instance expansion ───────────────────────────────────────────

fn expand_one(netlist: &mut Netlist, cand: &Candidate) -> Result<ExpansionResult, String> {
    let base = &cand.instance_name; // e.g. "buck"

    // Re-resolve nets by scanning all nets for ones that contain our pin instances.
    // The pin_instance.net field can be stale after net merges, but the net's
    // connection list is authoritative.
    let vout_net = find_net_for_pin_instance(netlist, cand.vout_pin_inst)
        .ok_or_else(|| "VOUT pin instance not connected to any net".to_string())?;

    let gnd_net = {
        let inst = netlist.instances.get(cand.instance_id)
            .ok_or("instance not found")?;
        let module_def = netlist.modules.get(inst.definition)
            .ok_or("module def not found")?;
        let mut found_gnd = None;
        for &pin_id in &module_def.pins {
            if let Some(pin) = netlist.pins.get(pin_id) {
                if pin.pin_type == PinType::Ground || pin.direction == PinDirection::Ground {
                    let pi_id = netlist.pin_instances.iter()
                        .find(|(_, pi)| pi.instance == cand.instance_id && pi.pin_def == pin_id)
                        .map(|(id, _)| id);
                    if let Some(pi_id) = pi_id {
                        found_gnd = find_net_for_pin_instance(netlist, pi_id);
                    }
                }
            }
        }
        found_gnd.ok_or_else(|| "GND pin has no net".to_string())?
    };

    debug!("Expanding {} — VOUT net: {:?}, GND net: {:?}", base, vout_net, gnd_net);

    // 1. Create internal switching-node net (name from library, e.g. "SW", "LX", "PH")
    let sw_net_name = format!("{}_{}", base, cand.sw_name);
    let sw_net = netlist.add_net(Some(sw_net_name.clone()));

    // 2. Rewire: disconnect buck's VOUT pin instance from user VOUT net,
    //    reconnect it to the SW net.
    disconnect_pin_from_net(netlist, cand.vout_pin_inst, vout_net);
    netlist.connect(sw_net, ConnectionPoint::PinInstance(cand.vout_pin_inst))
        .map_err(|e| format!("connect VOUT→SW: {}", e))?;

    // Record display name override: the viewer should show the switching node name
    // (e.g. "SW", "LX", "PH") instead of "VOUT"
    if let Some(inst) = netlist.instances.get_mut(cand.instance_id) {
        inst.attributes.insert("vpin_display_VOUT".to_string(), cand.sw_name.clone());
    }

    // 3. Create module defs (reuse if already present) and instances
    // Inductor pins: "IN" (from SW) and "OUT" (to VOUT) with directional types,
    // so the schematic extractor assigns correct left/right port placement.
    let ind_mod = find_or_create_module(netlist, "Ind", &[("IN", false), ("OUT", false)]);
    let cap_mod = find_or_create_module(netlist, "Cap", &[("1", true), ("2", true)]);

    // Read parent instance attributes early (also used later for ripple/intent)
    let inst_attrs = netlist.instances.get(cand.instance_id)
        .map(|i| i.attributes.clone())
        .unwrap_or_default();

    // Stamp intent and stage attributes on expansion children.
    // Inductor and diode are part of regulation; output caps are output_filtering.
    let parent_stage_name = inst_attrs.get("stage_name").cloned().unwrap_or_default();
    let parent_stage_order = inst_attrs.get("stage_order").cloned().unwrap_or_default();
    let parent_stage_rail = inst_attrs.get("stage_rail").cloned().unwrap_or_default();
    let parent_intent = inst_attrs.get("intent").cloned()
        .unwrap_or_else(|| "regulation".to_string());
    let mut stage_attrs: Vec<(&str, &str)> = vec![
        ("intent", &parent_intent),
    ];
    if !parent_stage_name.is_empty() {
        stage_attrs.push(("stage_name", &parent_stage_name));
        stage_attrs.push(("stage_order", &parent_stage_order));
        stage_attrs.push(("stage_rail", &parent_stage_rail));
    }

    // Output caps belong to the VOUT rail's output_filtering stage.
    let vout_rail_name = netlist.nets.get(vout_net)
        .and_then(|n| n.name.clone())
        .unwrap_or_default();
    let cap_stage_name = "output_filtering".to_string();
    let cap_stage_order = "0".to_string();
    let cap_intent = "output_filtering".to_string();
    let mut cap_stage_attrs: Vec<(&str, &str)> = vec![
        ("intent", &cap_intent),
    ];
    if !parent_stage_name.is_empty() {
        cap_stage_attrs.push(("stage_name", &cap_stage_name));
        cap_stage_attrs.push(("stage_order", &cap_stage_order));
        cap_stage_attrs.push(("stage_rail", &vout_rail_name));
    }

    // --- Inductor: pin 1 → SW, pin 2 → VOUT ---
    let ind_name = format!("{}_L", base);
    let mut ind_attrs: Vec<(&str, &str)> = vec![
        ("component_class", "inductor"),
        ("value", &cand.inductor_value),
        ("vpin_parent", base),
        ("vpin_role", "series"),
    ];
    ind_attrs.extend_from_slice(&stage_attrs);
    let ind_id = create_instance(netlist, &ind_name, ind_mod, &ind_attrs);
    let ind_pins = netlist.create_pin_instances(ind_id)
        .map_err(|e| format!("create inductor pins: {}", e))?;
    // IN → SW net
    connect_pin_instance_by_name(netlist, ind_id, &ind_pins, "IN", sw_net)?;
    // OUT → original VOUT net
    connect_pin_instance_by_name(netlist, ind_id, &ind_pins, "OUT", vout_net)?;

    // --- Diode (optional): A → GND, K → SW ---
    let diode_name = if cand.has_diode {
        let diode_mod = find_or_create_module(netlist, "Diode", &[("A", false), ("K", false)]);
        let d_name = format!("{}_D", base);
        let mut d_attrs: Vec<(&str, &str)> = vec![
            ("component_class", "diode"),
            ("forward_voltage", &cand.diode_vf),
            ("vpin_parent", base),
            ("vpin_role", "shunt"),
        ];
        d_attrs.extend_from_slice(&stage_attrs);
        let d_id = create_instance(netlist, &d_name, diode_mod, &d_attrs);
        let d_pins = netlist.create_pin_instances(d_id)
            .map_err(|e| format!("create diode pins: {}", e))?;
        connect_pin_instance_by_name(netlist, d_id, &d_pins, "A", gnd_net)?;
        connect_pin_instance_by_name(netlist, d_id, &d_pins, "K", sw_net)?;
        Some(d_name)
    } else {
        None
    };

    // --- Output capacitor(s): VOUT → GND ---
    // Check for intent-driven ripple target on the regulator instance
    let max_ripple = inst_attrs.get("intent_max_ripple")
        .and_then(|v| parse_unit_value(v));
    let f_sw = inst_attrs.get("f_sw")
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(500e3); // default 500kHz

    let mut additional_caps = Vec::new();

    if let Some(ripple_target) = max_ripple {
        // Intent-driven multi-tier capacitor bank
        let v_out = inst_attrs.get("output_voltage")
            .and_then(|v| parse_unit_value(v))
            .unwrap_or(5.0);
        // Estimate v_in from the VIN net voltage or use a safe default
        // We don't have GLACIER results yet, so use 2× v_out as a conservative estimate
        let v_in = v_out * 2.0; // Will be refined post-GLACIER
        let inductance = parse_unit_value(&cand.inductor_value).unwrap_or(33e-6);
        // Conservative load current estimate (will be refined by GLACIER physical selection)
        let i_load = 1.0;

        let bank = compute_ripple_bank(v_in, v_out, i_load, f_sw, inductance, ripple_target);

        info!("Ripple-aware bank for {}: {} tiers, est. ripple {:.2}mV (target {:.2}mV)",
            base, bank.tiers.len(), bank.estimated_ripple_v * 1e3, ripple_target * 1e3);

        let mut first_cap_name = String::new();
        for tier in &bank.tiers {
            for i in 0..tier.count {
                let cap_name = format!("{}_{}_{}", base, tier.role, i + 1);
                let cap_value = format_cap_value_for_attr(tier.capacitance);
                let vpin_role_str = format!("output_{}", tier.role);
                let mut cap_attrs: Vec<(&str, &str)> = vec![
                    ("component_class", "capacitor"),
                    ("value", &cap_value),
                    ("vpin_parent", base),
                    ("vpin_role", &vpin_role_str),
                    ("dielectric_hint", tier.dielectric_hint),
                    ("ripple_tier", tier.role),
                ];
                cap_attrs.extend_from_slice(&cap_stage_attrs);
                let cap_id = create_instance(netlist, &cap_name, cap_mod, &cap_attrs);
                let cap_pins = netlist.create_pin_instances(cap_id)
                    .map_err(|e| format!("create {} cap pins: {}", cap_name, e))?;
                connect_pin_instance_by_name(netlist, cap_id, &cap_pins, "1", vout_net)?;
                connect_pin_instance_by_name(netlist, cap_id, &cap_pins, "2", gnd_net)?;

                if first_cap_name.is_empty() {
                    first_cap_name = cap_name;
                } else {
                    additional_caps.push(cap_name);
                }
            }
        }

        Ok(ExpansionResult {
            regulator_name: base.clone(),
            inductor_name: ind_name,
            diode_name,
            output_cap_name: first_cap_name,
            sw_net_name,
            sw_pin_name: cand.sw_name.clone(),
            additional_output_caps: additional_caps,
        })
    } else {
        // Existing single-cap path (no intent)
        let cout_name = format!("{}_Cout", base);
        let mut cout_attrs: Vec<(&str, &str)> = vec![
            ("component_class", "capacitor"),
            ("value", &cand.cout_value),
            ("vpin_parent", base),
            ("vpin_role", "shunt"),
        ];
        cout_attrs.extend_from_slice(&cap_stage_attrs);
        let cout_id = create_instance(netlist, &cout_name, cap_mod, &cout_attrs);
        let cout_pins = netlist.create_pin_instances(cout_id)
            .map_err(|e| format!("create cout pins: {}", e))?;
        connect_pin_instance_by_name(netlist, cout_id, &cout_pins, "1", vout_net)?;
        connect_pin_instance_by_name(netlist, cout_id, &cout_pins, "2", gnd_net)?;

        Ok(ExpansionResult {
            regulator_name: base.clone(),
            inductor_name: ind_name,
            diode_name,
            output_cap_name: cout_name,
            sw_net_name,
            sw_pin_name: cand.sw_name.clone(),
            additional_output_caps: Vec::new(),
        })
    }
}

// ── Helpers ─────────────────────────────────────────────────────────────

/// Find the net that contains a given pin instance by scanning all nets' connection lists.
/// This is more reliable than `pin_instance.net` which can be stale after net merges.
pub(crate) fn find_net_for_pin_instance(netlist: &Netlist, pi_id: PinInstanceId) -> Option<NetId> {
    let target = ConnectionPoint::PinInstance(pi_id);
    for (net_id, net) in &netlist.nets {
        if net.connections.contains(&target) {
            return Some(net_id);
        }
    }
    // Fall back to pin_instance.net if the connection list doesn't have it
    // (this can happen if the pin was connected via the PinInstance path in connect())
    netlist.pin_instances.get(pi_id)
        .and_then(|pi| pi.net)
        .filter(|nid| netlist.nets.contains_key(*nid))
}

/// Find an existing module definition by name, or create one with the given pins.
/// `pins` is a slice of (name, is_passive): passive pins get InOut/Passive,
/// non-passive get In/Signal or Out/Signal based on name convention (A=In, K=Out).
pub(crate) fn find_or_create_module(
    netlist: &mut Netlist,
    name: &str,
    pins: &[(&str, bool)],
) -> ModuleId {
    // Check if module already exists with matching pin names
    let required_pin_names: Vec<&str> = pins.iter().map(|(n, _)| *n).collect();
    'outer: for (mod_id, mod_def) in &netlist.modules {
        if mod_def.name == name {
            // Verify pin names match — a stdlib "Ind" with "1"/"2" must not
            // be reused when the caller needs "IN"/"OUT".
            if mod_def.pins.len() == pins.len() {
                for (i, &pin_id) in mod_def.pins.iter().enumerate() {
                    if let Some(pin) = netlist.pins.get(pin_id) {
                        if pin.name != required_pin_names[i] {
                            continue 'outer;
                        }
                    }
                }
                return mod_id;
            }
        }
    }

    // Create new module
    let mod_id = netlist.add_module(name.to_string(), ModuleKind::PhysicalComponent);

    for &(pin_name, is_passive) in pins {
        let (dir, ptype) = if is_passive {
            (PinDirection::InOut, PinType::Passive)
        } else {
            match pin_name {
                "A" | "IN" | "VIN" => (PinDirection::In, PinType::Signal),
                "K" | "OUT" | "VOUT" => (PinDirection::Out, PinType::Signal),
                _ => (PinDirection::InOut, PinType::Signal),
            }
        };
        netlist.add_pin(mod_id, pin_name.to_string(), dir, ptype);
    }

    mod_id
}

/// Create a component instance with the given attributes.
pub(crate) fn create_instance(
    netlist: &mut Netlist,
    name: &str,
    module_id: ModuleId,
    attrs: &[(&str, &str)],
) -> InstanceId {
    let inst_id = netlist.instances.insert(bhdl_netlist::Instance {
        name: name.to_string(),
        definition: module_id,
        attributes: attrs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
    });
    inst_id
}

/// Disconnect a pin instance from a specific net (remove from net's connection list,
/// clear the pin instance's net reference).
pub(crate) fn disconnect_pin_from_net(netlist: &mut Netlist, pi_id: PinInstanceId, net_id: NetId) {
    // Remove from net's connections
    if let Some(net) = netlist.nets.get_mut(net_id) {
        net.connections.retain(|c| *c != ConnectionPoint::PinInstance(pi_id));
    }
    // Clear pin instance's net ref
    if let Some(pi) = netlist.pin_instances.get_mut(pi_id) {
        if pi.net == Some(net_id) {
            pi.net = None;
        }
    }
}

/// Connect a named pin of an instance to a net.
pub(crate) fn connect_pin_instance_by_name(
    netlist: &mut Netlist,
    inst_id: InstanceId,
    pin_instances: &[PinInstanceId],
    pin_name: &str,
    net_id: NetId,
) -> Result<(), String> {
    // Find the pin instance whose pin_def has the matching name
    for &pi_id in pin_instances {
        let pi = netlist.pin_instances.get(pi_id)
            .ok_or_else(|| format!("pin instance {:?} not found", pi_id))?;
        let pin = netlist.pins.get(pi.pin_def)
            .ok_or_else(|| format!("pin def {:?} not found", pi.pin_def))?;
        if pin.name == pin_name {
            return netlist.connect(net_id, ConnectionPoint::PinInstance(pi_id));
        }
    }
    Err(format!("pin '{}' not found on instance {:?}", pin_name, inst_id))
}

/// Format a capacitance value (in farads) as a human-readable string for attributes.
pub(crate) fn format_cap_value_for_attr(farads: f64) -> String {
    if farads >= 1e-3 {
        format!("{:.0}mF", farads * 1e3)
    } else if farads >= 1e-6 {
        let uf = farads * 1e6;
        if (uf - uf.round()).abs() < 0.05 {
            format!("{:.0}µF", uf)
        } else {
            format!("{:.1}µF", uf)
        }
    } else if farads >= 1e-9 {
        format!("{:.0}nF", farads * 1e9)
    } else {
        format!("{:.0}pF", farads * 1e12)
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{ModuleKind, NetClass};

    /// Build a minimal netlist with a switching regulator instance (VIN, VOUT, GND)
    /// connected to three nets, then run expand_virtual_pins and verify the result.
    fn make_buck_netlist() -> Netlist {
        let mut nl = Netlist::default();

        // Module def for BuckRegulator with 3 pins
        let buck_mod = nl.add_module("BuckRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(buck_mod, "VIN".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(buck_mod, "VOUT".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(buck_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        // Instance
        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), "switching_regulator".to_string());
        attrs.insert("output_voltage".to_string(), "5".to_string());
        attrs.insert("vpin_inductor".to_string(), "33µH".to_string());
        attrs.insert("vpin_cout".to_string(), "470µF".to_string());
        attrs.insert("vpin_has_diode".to_string(), "true".to_string());
        let buck_id = nl.instances.insert(bhdl_netlist::Instance {
            name: "buck".to_string(),
            definition: buck_mod,
            attributes: attrs,
        });

        // Create pin instances
        let pin_insts = nl.create_pin_instances(buck_id).unwrap();

        // Three nets
        let vin_net = nl.add_net_with_class(Some("VIN_12V".to_string()), NetClass::Power(12.0));
        let vout_net = nl.add_net_with_class(Some("V5".to_string()), NetClass::Power(5.0));
        let gnd_net = nl.add_net_with_class(Some("GND".to_string()), NetClass::Ground);

        // Connect pin instances to nets (VIN=0, VOUT=1, GND=2)
        nl.connect(vin_net, ConnectionPoint::PinInstance(pin_insts[0])).unwrap();
        nl.connect(vout_net, ConnectionPoint::PinInstance(pin_insts[1])).unwrap();
        nl.connect(gnd_net, ConnectionPoint::PinInstance(pin_insts[2])).unwrap();

        nl
    }

    #[test]
    fn test_expand_creates_inductor_diode_cap() {
        let mut nl = make_buck_netlist();

        let initial_instances = nl.instances.len();
        let initial_nets = nl.nets.len();

        let results = expand_virtual_pins(&mut nl);

        assert_eq!(results.len(), 1, "should expand exactly one regulator");
        let r = &results[0];
        assert_eq!(r.regulator_name, "buck");
        assert!(r.inductor_name.contains("_L"), "inductor should have _L suffix, got {}", r.inductor_name);
        assert!(r.diode_name.as_ref().unwrap().contains("_D"), "diode should have _D suffix");
        assert!(r.output_cap_name.contains("_Cout"), "cap should have _Cout suffix, got {}", r.output_cap_name);
        assert!(r.sw_net_name.contains("SW"));

        // 3 new instances (L, D, C)
        assert_eq!(nl.instances.len(), initial_instances + 3);
        // 1 new net (SW)
        assert_eq!(nl.nets.len(), initial_nets + 1);

        // Verify SW net exists
        let sw_net = nl.nets.iter()
            .find(|(_, n)| n.name.as_deref() == Some("buck_SW"));
        assert!(sw_net.is_some(), "SW net should exist");

        // Verify buck's VOUT is now on SW net, not the original V5 net
        let buck_vout_pi = nl.pin_instances.iter()
            .find(|(_, pi)| {
                let pin = nl.pins.get(pi.pin_def);
                pin.map(|p| p.name == "VOUT").unwrap_or(false)
                    && nl.instances.get(pi.instance).map(|i| i.name == "buck").unwrap_or(false)
            });
        assert!(buck_vout_pi.is_some());
        let (_, pi) = buck_vout_pi.unwrap();
        let sw_net_id = sw_net.unwrap().0;
        assert_eq!(pi.net, Some(sw_net_id), "VOUT pin should now be on SW net");

        // Verify display name override on parent
        let buck_inst = nl.instances.iter()
            .find(|(_, i)| i.name == "buck").unwrap().1;
        assert_eq!(buck_inst.attributes.get("vpin_display_VOUT").map(|s| s.as_str()),
                   Some("SW"), "buck should have display name override for VOUT");

        // Verify expansion metadata on children (find by vpin_parent, not by name)
        let children: Vec<_> = nl.instances.iter()
            .filter(|(_, i)| i.attributes.get("vpin_parent").map(|s| s.as_str()) == Some("buck"))
            .collect();
        assert_eq!(children.len(), 3, "should have 3 expansion children");

        let inductor = children.iter().find(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("series")).unwrap().1;
        assert_eq!(inductor.attributes.get("component_class").map(|s| s.as_str()), Some("inductor"));
        assert!(inductor.name.contains("_L"), "inductor name should have _L suffix, got {}", inductor.name);

        let shunts: Vec<_> = children.iter()
            .filter(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("shunt"))
            .collect();
        assert_eq!(shunts.len(), 2, "should have 2 shunt children (diode + cap)");

        // Verify sw_pin_name in result
        assert_eq!(r.sw_pin_name, "SW");
    }

    #[test]
    fn test_no_diode_when_disabled() {
        let mut nl = make_buck_netlist();

        // Set has_diode = false
        for (_, inst) in &mut nl.instances {
            if inst.name == "buck" {
                inst.attributes.insert("vpin_has_diode".to_string(), "false".to_string());
            }
        }

        let results = expand_virtual_pins(&mut nl);
        assert_eq!(results.len(), 1);
        assert!(results[0].diode_name.is_none(), "diode should be skipped for synchronous buck");

        // Only 2 new instances (L, C — no D)
        // original: 1 (buck) → total should be 3
        assert_eq!(nl.instances.len(), 3);
    }

    #[test]
    fn test_no_expansion_for_linear_regulator() {
        let mut nl = Netlist::default();
        let lin_mod = nl.add_module("LinearRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(lin_mod, "VI".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(lin_mod, "VO".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(lin_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        let mut attrs = HashMap::new();
        attrs.insert("component_class".to_string(), "voltage_regulator".to_string());
        nl.instances.insert(bhdl_netlist::Instance {
            name: "reg".to_string(),
            definition: lin_mod,
            attributes: attrs,
        });

        let results = expand_virtual_pins(&mut nl);
        assert!(results.is_empty(), "linear regulators should not be expanded");
    }

    #[test]
    fn test_module_reuse() {
        let mut nl = Netlist::default();

        // Create two buck regulators
        let buck_mod = nl.add_module("BuckRegulator".to_string(), ModuleKind::PhysicalComponent);
        nl.add_pin(buck_mod, "VIN".to_string(), PinDirection::In, PinType::Power);
        nl.add_pin(buck_mod, "VOUT".to_string(), PinDirection::Out, PinType::Power);
        nl.add_pin(buck_mod, "GND".to_string(), PinDirection::Ground, PinType::Ground);

        for name in &["buck1", "buck2"] {
            let mut attrs = HashMap::new();
            attrs.insert("component_class".to_string(), "switching_regulator".to_string());
            attrs.insert("output_voltage".to_string(), "5".to_string());
            let inst_id = nl.instances.insert(bhdl_netlist::Instance {
                name: name.to_string(),
                definition: buck_mod,
                attributes: attrs,
            });
            let pins = nl.create_pin_instances(inst_id).unwrap();
            let vin = nl.add_net(Some(format!("{}_VIN", name)));
            let vout = nl.add_net(Some(format!("{}_VOUT", name)));
            let gnd = nl.add_net(Some("GND".to_string()));
            nl.connect(vin, ConnectionPoint::PinInstance(pins[0])).unwrap();
            nl.connect(vout, ConnectionPoint::PinInstance(pins[1])).unwrap();
            nl.connect(gnd, ConnectionPoint::PinInstance(pins[2])).unwrap();
        }

        let results = expand_virtual_pins(&mut nl);
        assert_eq!(results.len(), 2, "both regulators should expand");

        // Ind and Cap module defs should be created once and reused
        let ind_mods: Vec<_> = nl.modules.iter()
            .filter(|(_, m)| m.name == "Ind")
            .collect();
        assert_eq!(ind_mods.len(), 1, "Ind module should exist exactly once");

        // Verify expansion metadata on both sets of children (find by vpin_parent)
        for prefix in &["buck1", "buck2"] {
            let children: Vec<_> = nl.instances.iter()
                .filter(|(_, i)| i.attributes.get("vpin_parent").map(|s| s.as_str()) == Some(*prefix))
                .collect();
            // 3 children each: L, D, C
            assert_eq!(children.len(), 3, "{} should have 3 expansion children", prefix);

            let inductor = children.iter()
                .find(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("series"))
                .unwrap().1;
            assert!(inductor.name.contains("_L"), "inductor name should have _L suffix, got {}", inductor.name);

            let shunts: Vec<_> = children.iter()
                .filter(|(_, i)| i.attributes.get("vpin_role").map(|s| s.as_str()) == Some("shunt"))
                .collect();
            assert_eq!(shunts.len(), 2, "{} should have 2 shunt children", prefix);
        }

        // Verify unique names: all inductor instances should have distinct names
        let l_names: Vec<_> = nl.instances.iter()
            .filter(|(_, i)| i.name.contains("_L"))
            .map(|(_, i)| i.name.clone())
            .collect();
        assert_eq!(l_names.len(), 2);
        assert_ne!(l_names[0], l_names[1], "inductors should have distinct names");
    }

    #[test]
    fn test_intent_driven_multi_tier_expansion() {
        let mut nl = make_buck_netlist();

        // Add intent attributes to the buck instance (as if stamped by intent_attribute_stamper)
        for (_, inst) in &mut nl.instances {
            if inst.name == "buck" {
                inst.attributes.insert("intent_name".to_string(), "output_filtering".to_string());
                inst.attributes.insert("intent_max_ripple".to_string(), "5mV".to_string());
                inst.attributes.insert("f_sw".to_string(), "500kHz".to_string());
            }
        }

        let initial_instances = nl.instances.len();
        let results = expand_virtual_pins(&mut nl);

        assert_eq!(results.len(), 1, "should expand exactly one regulator");
        let r = &results[0];

        // With intent, should create multi-tier caps (not single _Cout)
        // The first cap becomes output_cap_name, rest go into additional_output_caps
        let total_output_caps = 1 + r.additional_output_caps.len();
        assert!(total_output_caps >= 3,
            "multi-tier bank should have >= 3 output caps (hf + mid + bulk), got {}",
            total_output_caps);

        // Verify all output cap instances have ripple_tier attribute
        let cap_children: Vec<_> = nl.instances.iter()
            .filter(|(_, i)| {
                i.attributes.get("vpin_parent").map(|s| s.as_str()) == Some("buck")
                    && i.attributes.get("component_class").map(|s| s.as_str()) == Some("capacitor")
            })
            .collect();

        assert!(cap_children.len() >= 3,
            "should have >= 3 cap children, got {}", cap_children.len());

        // Check that we have all three tiers
        let tiers: Vec<&str> = cap_children.iter()
            .filter_map(|(_, i)| i.attributes.get("ripple_tier").map(|s| s.as_str()))
            .collect();
        assert!(tiers.contains(&"hf_bypass"), "should have hf_bypass tier");
        assert!(tiers.contains(&"mid_freq"), "should have mid_freq tier");
        assert!(tiers.contains(&"bulk"), "should have bulk tier");

        // Check dielectric hints are set correctly
        for (_, inst) in &cap_children {
            if let Some(tier) = inst.attributes.get("ripple_tier") {
                let hint = inst.attributes.get("dielectric_hint").map(|s| s.as_str());
                match tier.as_str() {
                    "hf_bypass" => assert_eq!(hint, Some("C0G")),
                    "mid_freq" => assert_eq!(hint, Some("X7R")),
                    "bulk" => assert_eq!(hint, Some("X5R")),
                    _ => panic!("unexpected tier: {}", tier),
                }
            }
        }

        // Total instances: buck + L + D + N caps
        assert!(nl.instances.len() > initial_instances + 3,
            "multi-tier should create more instances than single-cap path");
    }

    #[test]
    fn test_no_intent_falls_back_to_single_cap() {
        // Without intent attrs, should use existing single-cap path
        let mut nl = make_buck_netlist();

        let results = expand_virtual_pins(&mut nl);
        assert_eq!(results.len(), 1);
        let r = &results[0];

        // Should use single _Cout, no additional caps
        assert!(r.output_cap_name.contains("_Cout"),
            "without intent should use single _Cout, got {}", r.output_cap_name);
        assert!(r.additional_output_caps.is_empty(),
            "without intent should have no additional caps");
    }
}
