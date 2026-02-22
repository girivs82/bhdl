// GLACIER-Driven Component Physical Selection
//
// Uses GLACIER DC simulation results (voltage, current, power at every node)
// to automatically determine physical parameters for passive components:
// package size, voltage rating, power rating, dielectric type.
//
// Results are written as instance attributes on the netlist, flowing
// naturally through to the schematic viewer.

use std::collections::HashMap;
use log::{debug, info};
use bhdl_analyzer::spice_extraction::parse_unit_value;
use bhdl_netlist::Netlist;

use crate::passive_component_calculator::PassiveComponentCalculator;
use crate::package_selector::{PackageSelector, ApplicationRequirements};

/// Summary of a single component's physical selection result.
#[derive(Debug)]
pub struct PhysicalSelectionResult {
    pub instance_name: String,
    pub component_type: String,
    pub package: String,
    pub power_rating: Option<String>,
    pub voltage_rating: Option<String>,
    pub dielectric: Option<String>,
}

/// Apply GLACIER simulation results to select physical parameters for passive components.
///
/// Iterates over all netlist instances, identifies resistors and capacitors,
/// and uses the simulation-derived current/power/voltage to select appropriate
/// package sizes, voltage ratings, power ratings, and dielectric types.
///
/// Selected parameters are written directly as instance attributes.
pub fn apply_glacier_physical_selection(
    netlist: &mut Netlist,
    instance_currents: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
    net_voltages: &HashMap<String, f64>,
) -> Vec<PhysicalSelectionResult> {
    let calculator = PassiveComponentCalculator::new();
    let selector = PackageSelector::new();
    let requirements = ApplicationRequirements::default();
    let mut results = Vec::new();

    // Build a map from instance name to the net names it touches,
    // so we can look up max voltage across a capacitor.
    let instance_net_voltages = compute_instance_max_voltages(netlist, net_voltages);

    // Build per-net total load current (sum of absolute currents of all
    // non-source instances touching each net). Used to infer inductor current.
    let net_load_currents = compute_net_load_currents(netlist, instance_currents);

    // Collect instance IDs first to avoid borrow conflicts
    let instance_ids: Vec<_> = netlist.instances.keys().collect();

    for inst_id in instance_ids {
        let inst = &netlist.instances[inst_id];
        let inst_name = inst.name.clone();
        let def_id = inst.definition;
        let attrs = inst.attributes.clone();

        let component_class = classify_component(netlist, def_id, &attrs);

        match component_class.as_deref() {
            Some("resistor") => {
                if let Some(result) = select_resistor_physical(
                    &inst_name,
                    &attrs,
                    instance_currents,
                    instance_power,
                    &instance_net_voltages,
                    &calculator,
                    &selector,
                    &requirements,
                ) {
                    // Write attributes back to the instance
                    let inst_mut = &mut netlist.instances[inst_id];
                    inst_mut.attributes.insert("package".to_string(), result.package.clone());
                    if let Some(ref pr) = result.power_rating {
                        inst_mut.attributes.insert("power_rating".to_string(), pr.clone());
                    }
                    if let Some(ref vr) = result.voltage_rating {
                        inst_mut.attributes.insert("voltage_rating".to_string(), vr.clone());
                    }
                    results.push(result);
                }
            }
            Some("capacitor") => {
                if let Some(result) = select_capacitor_physical(
                    &inst_name,
                    &attrs,
                    &instance_net_voltages,
                    &calculator,
                    &selector,
                    &requirements,
                ) {
                    let inst_mut = &mut netlist.instances[inst_id];
                    inst_mut.attributes.insert("package".to_string(), result.package.clone());
                    if let Some(ref vr) = result.voltage_rating {
                        inst_mut.attributes.insert("voltage_rating".to_string(), vr.clone());
                    }
                    if let Some(ref di) = result.dielectric {
                        inst_mut.attributes.insert("dielectric".to_string(), di.clone());
                    }
                    results.push(result);
                }
            }
            Some("inductor") => {
                if let Some(result) = select_inductor_physical(
                    &inst_name,
                    inst_id,
                    &attrs,
                    instance_currents,
                    &net_load_currents,
                    netlist,
                ) {
                    let inst_mut = &mut netlist.instances[inst_id];
                    inst_mut.attributes.insert("package".to_string(), result.package.clone());
                    if let Some(ref pr) = result.power_rating {
                        inst_mut.attributes.insert("power_rating".to_string(), pr.clone());
                    }
                    if let Some(ref vr) = result.voltage_rating {
                        inst_mut.attributes.insert("current_rating".to_string(), vr.clone());
                    }
                    // Also store DCR and saturation current
                    if let Some(ref di) = result.dielectric {
                        inst_mut.attributes.insert("dcr".to_string(), di.clone());
                    }
                    results.push(result);
                }
            }
            _ => {
                // Not a passive component we handle — skip
            }
        }
    }

    if !results.is_empty() {
        info!("Physical selection applied to {} components", results.len());
        for r in &results {
            debug!(
                "  {} ({}): package={}, power={}, voltage={}, dielectric={}",
                r.instance_name,
                r.component_type,
                r.package,
                r.power_rating.as_deref().unwrap_or("-"),
                r.voltage_rating.as_deref().unwrap_or("-"),
                r.dielectric.as_deref().unwrap_or("-"),
            );
        }
    }

    results
}

/// Determine the component class from the module definition name and instance attributes.
fn classify_component(
    netlist: &Netlist,
    def_id: bhdl_netlist::ModuleId,
    attrs: &HashMap<String, String>,
) -> Option<String> {
    // Check explicit component_class attribute first
    if let Some(class) = attrs.get("component_class") {
        let lower = class.to_lowercase();
        if lower.contains("resistor") || lower == "res" || lower == "r" {
            return Some("resistor".to_string());
        }
        if lower.contains("capacitor") || lower == "cap" || lower == "c" {
            return Some("capacitor".to_string());
        }
        if lower.contains("inductor") || lower == "ind" || lower == "l" {
            return Some("inductor".to_string());
        }
    }

    // Fall back to the module definition name
    if let Some(def) = netlist.modules.get(def_id) {
        let name_lower = def.name.to_lowercase();
        if name_lower == "res" || name_lower == "resistor" || name_lower.starts_with("res_") || name_lower == "r" {
            return Some("resistor".to_string());
        }
        if name_lower == "cap" || name_lower == "capacitor" || name_lower.starts_with("cap_") || name_lower == "c" {
            return Some("capacitor".to_string());
        }
        if name_lower == "ind" || name_lower == "inductor" || name_lower.starts_with("ind_") || name_lower == "l" {
            return Some("inductor".to_string());
        }
    }

    // Check if instance name starts with a standard reference designator prefix
    // (This is a fallback; component_class or module name should be authoritative)
    None
}

/// For each instance, compute the maximum voltage seen across its connected nets.
/// This is used for capacitor voltage rating selection where V=P/I is 0/0 for DC caps.
fn compute_instance_max_voltages(
    netlist: &Netlist,
    net_voltages: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    // Build instance → set of connected net names
    let mut instance_nets: HashMap<String, Vec<String>> = HashMap::new();

    for (_net_id, net) in &netlist.nets {
        let net_name = match &net.name {
            Some(n) => n.clone(),
            None => continue,
        };
        for conn in &net.connections {
            let inst_id = match conn {
                bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) => Some(*iid),
                bhdl_netlist::ConnectionPoint::PinInstance(pi_id) => {
                    netlist.pin_instances.get(*pi_id).map(|pi| pi.instance)
                }
                _ => None,
            };
            if let Some(iid) = inst_id {
                if let Some(inst) = netlist.instances.get(iid) {
                    instance_nets
                        .entry(inst.name.clone())
                        .or_default()
                        .push(net_name.clone());
                }
            }
        }
    }

    // For each instance, find the max absolute voltage difference across its nets
    let mut max_voltages: HashMap<String, f64> = HashMap::new();
    for (inst_name, nets) in &instance_nets {
        let voltages: Vec<f64> = nets
            .iter()
            .filter_map(|n| net_voltages.get(n).copied())
            .collect();

        if voltages.len() >= 2 {
            // Max voltage across the component = max - min of connected net voltages
            let vmax = voltages.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
            let vmin = voltages.iter().cloned().fold(f64::INFINITY, f64::min);
            max_voltages.insert(inst_name.clone(), (vmax - vmin).abs());
        } else if voltages.len() == 1 {
            // Single net connected — voltage referenced to ground
            max_voltages.insert(inst_name.clone(), voltages[0].abs());
        }
    }

    max_voltages
}

/// Select physical parameters for a resistor instance.
fn select_resistor_physical(
    inst_name: &str,
    attrs: &HashMap<String, String>,
    instance_currents: &HashMap<String, f64>,
    instance_power: &HashMap<String, f64>,
    instance_max_voltages: &HashMap<String, f64>,
    calculator: &PassiveComponentCalculator,
    selector: &PackageSelector,
    requirements: &ApplicationRequirements,
) -> Option<PhysicalSelectionResult> {
    // Get resistance value
    let value_str = attrs.get("value")?;
    let resistance = parse_unit_value(value_str)?;

    // Get current from GLACIER results (use absolute value)
    let current = instance_currents
        .get(inst_name)
        .copied()
        .unwrap_or(0.0)
        .abs();

    // Calculate power rating: use GLACIER power if available, else I²R
    let power = instance_power
        .get(inst_name)
        .copied()
        .unwrap_or_else(|| current * current * resistance)
        .abs();

    let power_rating = calculator.calculate_resistor_power_rating(resistance, current);

    // Get voltage across resistor for voltage rating
    let voltage_across = instance_max_voltages
        .get(inst_name)
        .copied()
        .unwrap_or_else(|| current * resistance);
    let voltage_rating = calculator.calculate_resistor_voltage_rating(voltage_across);

    let spec = selector.select_resistor_spec(resistance, power_rating, voltage_rating, requirements);

    debug!(
        "Resistor {}: R={}Ω, I={:.3}mA, P={:.3}mW → {} / {} / {}",
        inst_name,
        resistance,
        current * 1e3,
        power * 1e3,
        spec.package,
        spec.power_rating,
        spec.voltage_rating
    );

    Some(PhysicalSelectionResult {
        instance_name: inst_name.to_string(),
        component_type: "resistor".to_string(),
        package: spec.package.to_string(),
        power_rating: Some(spec.power_rating.to_string()),
        voltage_rating: Some(spec.voltage_rating.to_string()),
        dielectric: None,
    })
}

/// Select physical parameters for a capacitor instance.
fn select_capacitor_physical(
    inst_name: &str,
    attrs: &HashMap<String, String>,
    instance_max_voltages: &HashMap<String, f64>,
    calculator: &PassiveComponentCalculator,
    selector: &PackageSelector,
    requirements: &ApplicationRequirements,
) -> Option<PhysicalSelectionResult> {
    // Get capacitance value
    let value_str = attrs.get("value")?;
    let capacitance = parse_unit_value(value_str)?;

    // For capacitors, use max voltage across connected nets (not P/I which is 0/0 for DC).
    let max_voltage = instance_max_voltages
        .get(inst_name)
        .copied()
        .unwrap_or(0.0)
        .abs();

    let voltage_rating = calculator.calculate_capacitor_voltage_rating(max_voltage);

    let spec = selector.select_capacitor_spec(capacitance, voltage_rating, requirements);

    debug!(
        "Capacitor {}: C={:.3e}F, Vmax={:.2}V → {} / {} / {}",
        inst_name,
        capacitance,
        max_voltage,
        spec.package,
        spec.voltage_rating,
        spec.dielectric
    );

    Some(PhysicalSelectionResult {
        instance_name: inst_name.to_string(),
        component_type: "capacitor".to_string(),
        package: spec.package.to_string(),
        power_rating: None,
        voltage_rating: Some(spec.voltage_rating.to_string()),
        dielectric: Some(spec.dielectric.to_string()),
    })
}

/// Compute total load current per net.
///
/// For each net, sum the absolute currents of all non-source, non-regulator
/// instances connected to it. This represents the total current sunk by
/// loads on that net — exactly what an inductor feeding that net must carry.
fn compute_net_load_currents(
    netlist: &Netlist,
    instance_currents: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    // Build net_name → list of (instance_name, current)
    let mut net_instances: HashMap<String, Vec<(String, f64)>> = HashMap::new();

    for (_net_id, net) in &netlist.nets {
        let net_name = match &net.name {
            Some(n) => n.clone(),
            None => continue,
        };
        for conn in &net.connections {
            let inst_name = match conn {
                bhdl_netlist::ConnectionPoint::PinInstance(pi_id) => {
                    netlist.pin_instances.get(*pi_id)
                        .and_then(|pi| netlist.instances.get(pi.instance))
                        .map(|i| i.name.clone())
                }
                bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) => {
                    netlist.instances.get(*iid).map(|i| i.name.clone())
                }
                _ => None,
            };
            if let Some(name) = inst_name {
                if let Some(&current) = instance_currents.get(&name) {
                    net_instances.entry(net_name.clone())
                        .or_default()
                        .push((name, current.abs()));
                }
            }
        }
    }

    // For each net, sum all load currents (use max as a conservative estimate
    // since individual branch currents may overlap on a power rail)
    let mut result = HashMap::new();
    for (net_name, instances) in &net_instances {
        // Take the maximum current seen — on a power rail this is typically
        // the power source's current, which equals total load current.
        let max_current = instances.iter()
            .map(|(_, c)| *c)
            .fold(0.0f64, f64::max);
        result.insert(net_name.clone(), max_current);
    }

    result
}

/// Select physical parameters for an inductor instance.
///
/// Uses the GLACIER-derived current to determine:
/// - Saturation current rating (derated by 0.8)
/// - DCR estimate: ~0.01Ω × √(L_µH) for SMD inductors
/// - Power dissipation: I² × DCR
/// - Package size by current rating
///
/// For expanded buck inductors (name matching `*_L1`), GLACIER reports 0A
/// because both terminals sit at the same DC voltage. In this case, we find
/// the VOUT-side net and use the total load current on that net — which is
/// exactly what the inductor must carry. The cascade fixup in
/// `build_simulation_annotations()` ensures that power sources on the VOUT net
/// already include all downstream regulator loads.
fn select_inductor_physical(
    inst_name: &str,
    inst_id: bhdl_netlist::InstanceId,
    attrs: &HashMap<String, String>,
    instance_currents: &HashMap<String, f64>,
    net_load_currents: &HashMap<String, f64>,
    netlist: &Netlist,
) -> Option<PhysicalSelectionResult> {
    // Get inductance value
    let value_str = attrs.get("value")?;
    let inductance = parse_unit_value(value_str)?;

    // Get current from GLACIER (absolute value)
    let mut current = instance_currents
        .get(inst_name)
        .copied()
        .unwrap_or(0.0)
        .abs();

    // For expanded buck inductors (e.g. "buck_L1"), the DC inductor current is 0
    // because both sides sit at the same voltage. Infer the actual current from
    // the VOUT-side net's total load current.
    //
    // The inductor connects SW (pin 1) → VOUT (pin 2). Pin 2's net carries
    // the load current we need.
    if current < 1e-6 {
        // Find pin 2's net (the VOUT side)
        let vout_side_current = find_inductor_vout_net_current(
            netlist, inst_id, net_load_currents,
        );
        if let Some(load_current) = vout_side_current {
            current = load_current;
            debug!("Inductor {} inferred current from VOUT-side net: {:.3}A",
                   inst_name, current);
        }
    }

    // Estimate DCR: ~0.01Ω × √(L in µH)
    // This is a rough heuristic; actual DCR depends on construction.
    let l_uh = inductance * 1e6; // convert H → µH
    let dcr = 0.01 * l_uh.sqrt().max(1.0);

    // Power dissipation = I² × DCR
    let power = current * current * dcr;

    // Current rating: derate by 0.8 (select for 80% of saturation)
    let required_sat_current = if current > 0.0 { current / 0.8 } else { 0.1 };

    // Package selection by current rating
    let package = if required_sat_current <= 0.5 {
        "0805"
    } else if required_sat_current <= 1.5 {
        "1210"
    } else if required_sat_current <= 3.0 {
        "1812"
    } else if required_sat_current <= 5.0 {
        "2220"
    } else {
        "THT"
    };

    // Format saturation current for display
    let sat_current_str = if required_sat_current >= 1.0 {
        format!("{:.1}A", required_sat_current)
    } else {
        format!("{:.0}mA", required_sat_current * 1e3)
    };

    debug!(
        "Inductor {}: L={:.1}µH, I={:.3}A, DCR={:.3}Ω, P={:.1}mW → {} / I_sat={}",
        inst_name, l_uh, current, dcr, power * 1e3, package, sat_current_str
    );

    Some(PhysicalSelectionResult {
        instance_name: inst_name.to_string(),
        component_type: "inductor".to_string(),
        package: package.to_string(),
        power_rating: Some(format!("{:.1}mW", power * 1e3)),
        // Reuse voltage_rating field for current_rating (written as "current_rating" in attrs)
        voltage_rating: Some(sat_current_str),
        // Reuse dielectric field for DCR
        dielectric: Some(format!("{:.3}Ω", dcr)),
    })
}

/// For an inductor instance, find the VOUT-side net (pin "2") and return the
/// total load current on that net.
///
/// In a buck converter topology:
///   SW ──[L pin1]──[L pin2]── VOUT_net ──[loads]── GND
///
/// The inductor must carry the total current consumed by everything on VOUT_net.
/// The `net_load_currents` map has the max current per net (typically the power
/// source's current, which equals total load including cascaded regulators).
fn find_inductor_vout_net_current(
    netlist: &Netlist,
    inst_id: bhdl_netlist::InstanceId,
    net_load_currents: &HashMap<String, f64>,
) -> Option<f64> {
    let instance = netlist.instances.get(inst_id)?;
    let module_def = netlist.modules.get(instance.definition)?;

    // Find pin "2" (VOUT side) of the inductor
    let pin2_id = module_def.pins.iter()
        .find(|&&pid| netlist.pins.get(pid).map(|p| p.name == "2").unwrap_or(false))
        .copied()?;

    // Find the pin instance for (this instance, pin 2)
    let pi_entry = netlist.pin_instances.iter()
        .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin2_id);

    let (pi_id, _) = pi_entry?;

    // Find which net this pin instance is on (scan connection lists for reliability)
    let net_name = netlist.nets.iter()
        .find(|(_, net)| {
            net.connections.contains(&bhdl_netlist::ConnectionPoint::PinInstance(pi_id))
        })
        .and_then(|(_, net)| net.name.clone())?;

    let load = net_load_currents.get(&net_name).copied();
    if let Some(c) = load {
        debug!("Inductor on net '{}': load current = {:.3}A", net_name, c);
    }
    load
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::{Netlist, ModuleId, InstanceId};

    fn make_test_netlist() -> (Netlist, InstanceId, InstanceId) {
        let mut netlist = Netlist::default();

        // Create a resistor module definition
        let res_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Res".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        // Create a capacitor module definition
        let cap_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Cap".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        // Create resistor instance with 10k value
        let mut res_attrs = HashMap::new();
        res_attrs.insert("value".to_string(), "10k".to_string());
        let res_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "R1".to_string(),
            definition: res_mod_id,
            attributes: res_attrs,
        });

        // Create capacitor instance with 100nF value
        let mut cap_attrs = HashMap::new();
        cap_attrs.insert("value".to_string(), "100nF".to_string());
        let cap_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "C1".to_string(),
            definition: cap_mod_id,
            attributes: cap_attrs,
        });

        (netlist, res_id, cap_id)
    }

    #[test]
    fn test_resistor_physical_selection() {
        let (mut netlist, res_id, _cap_id) = make_test_netlist();

        let mut instance_currents = HashMap::new();
        instance_currents.insert("R1".to_string(), 0.5e-3); // 0.5mA

        let mut instance_power = HashMap::new();
        instance_power.insert("R1".to_string(), 2.5e-3); // 2.5mW

        let net_voltages = HashMap::new();

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &instance_power,
            &net_voltages,
        );

        // Should have at least the resistor result
        let res_result = results.iter().find(|r| r.instance_name == "R1");
        assert!(res_result.is_some(), "R1 should have physical selection");
        let res_result = res_result.unwrap();
        assert_eq!(res_result.component_type, "resistor");
        assert!(!res_result.package.is_empty());
        assert!(res_result.power_rating.is_some());
        assert!(res_result.voltage_rating.is_some());

        // Verify attributes were written to the instance
        let inst = &netlist.instances[res_id];
        assert!(inst.attributes.contains_key("package"));
        assert!(inst.attributes.contains_key("power_rating"));
        assert!(inst.attributes.contains_key("voltage_rating"));
    }

    #[test]
    fn test_capacitor_physical_selection() {
        let (mut netlist, _res_id, cap_id) = make_test_netlist();

        let instance_currents = HashMap::new();
        let instance_power = HashMap::new();

        // Simulate 5V across the capacitor via net voltages
        let mut net_voltages = HashMap::new();
        net_voltages.insert("VCC".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        // We need to connect the capacitor to these nets for voltage computation
        // Create nets and connect them to C1
        let port1 = netlist.ports.insert(bhdl_netlist::Port {
            name: "1".to_string(),
            direction: bhdl_netlist::PortDirection::InOut,
            net: None,
            width: None,
            module: netlist.instances[cap_id].definition,
        });
        let port2 = netlist.ports.insert(bhdl_netlist::Port {
            name: "2".to_string(),
            direction: bhdl_netlist::PortDirection::InOut,
            net: None,
            width: None,
            module: netlist.instances[cap_id].definition,
        });

        netlist.nets.insert(bhdl_netlist::Net {
            name: Some("VCC".to_string()),
            connections: vec![bhdl_netlist::ConnectionPoint::InstancePort(cap_id, port1)],
            net_class: bhdl_netlist::NetClass::Signal,
        });
        netlist.nets.insert(bhdl_netlist::Net {
            name: Some("GND".to_string()),
            connections: vec![bhdl_netlist::ConnectionPoint::InstancePort(cap_id, port2)],
            net_class: bhdl_netlist::NetClass::Signal,
        });

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &instance_power,
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");
        let cap_result = cap_result.unwrap();
        assert_eq!(cap_result.component_type, "capacitor");
        assert!(cap_result.dielectric.is_some());
        assert!(cap_result.voltage_rating.is_some());

        // Verify attributes were written
        let inst = &netlist.instances[cap_id];
        assert!(inst.attributes.contains_key("package"));
        assert!(inst.attributes.contains_key("voltage_rating"));
        assert!(inst.attributes.contains_key("dielectric"));

        // 5V cap should get at least 10V rating (2x derating)
        let vr = inst.attributes.get("voltage_rating").unwrap();
        assert!(vr == "10V" || vr == "16V" || vr == "25V",
            "Expected voltage rating >= 10V for 5V cap, got {}", vr);
    }

    #[test]
    fn test_no_value_attribute_skips() {
        let mut netlist = Netlist::default();

        let mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Res".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        // Instance without a "value" attribute
        netlist.instances.insert(bhdl_netlist::Instance {
            name: "R_novalue".to_string(),
            definition: mod_id,
            attributes: HashMap::new(),
        });

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &HashMap::new(),
        );

        assert!(results.is_empty(), "Instance without value should be skipped");
    }

    #[test]
    fn test_high_power_resistor_gets_large_package() {
        let (mut netlist, res_id, _) = make_test_netlist();

        // 10k resistor with 10mA → P = I²R = 1W
        let mut instance_currents = HashMap::new();
        instance_currents.insert("R1".to_string(), 10e-3);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &HashMap::new(),
            &HashMap::new(),
        );

        let res_result = results.iter().find(|r| r.instance_name == "R1").unwrap();
        // 1W derated by 0.7 → 1.43W → needs P2W (2512 package)
        assert!(
            res_result.package == "2512" || res_result.package == "THT",
            "High power resistor should get large package, got {}",
            res_result.package
        );
    }

    #[test]
    fn test_inductor_physical_selection() {
        let mut netlist = Netlist::default();

        let ind_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Ind".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        let mut ind_attrs = HashMap::new();
        ind_attrs.insert("value".to_string(), "33µH".to_string());
        ind_attrs.insert("component_class".to_string(), "inductor".to_string());
        let ind_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "L1".to_string(),
            definition: ind_mod_id,
            attributes: ind_attrs,
        });

        // 2A through the inductor
        let mut instance_currents = HashMap::new();
        instance_currents.insert("L1".to_string(), 2.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &HashMap::new(),
            &HashMap::new(),
        );

        let ind_result = results.iter().find(|r| r.instance_name == "L1");
        assert!(ind_result.is_some(), "L1 should have physical selection");
        let ind_result = ind_result.unwrap();
        assert_eq!(ind_result.component_type, "inductor");

        // 2A / 0.8 = 2.5A required sat current → 1812 package (≤3A)
        assert_eq!(ind_result.package, "1812",
            "2A inductor should get 1812 package, got {}", ind_result.package);

        // Check attributes written to instance
        let inst = &netlist.instances[ind_id];
        assert!(inst.attributes.contains_key("package"));
        assert!(inst.attributes.contains_key("current_rating"));
        assert!(inst.attributes.contains_key("dcr"));
    }

    #[test]
    fn test_high_current_inductor_gets_large_package() {
        let mut netlist = Netlist::default();

        let ind_mod_id = netlist.modules.insert(bhdl_netlist::ModuleDefinition {
            name: "Ind".to_string(),
            kind: bhdl_netlist::ModuleKind::PhysicalComponent,
            ports: vec![],
            pins: vec![],
            internal_instances: vec![],
            internal_nets: vec![],
            attributes: HashMap::new(),
        });

        let mut ind_attrs = HashMap::new();
        ind_attrs.insert("value".to_string(), "10µH".to_string());
        ind_attrs.insert("component_class".to_string(), "inductor".to_string());
        netlist.instances.insert(bhdl_netlist::Instance {
            name: "L_big".to_string(),
            definition: ind_mod_id,
            attributes: ind_attrs,
        });

        // 8A — needs THT
        let mut instance_currents = HashMap::new();
        instance_currents.insert("L_big".to_string(), 8.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &instance_currents,
            &HashMap::new(),
            &HashMap::new(),
        );

        let ind_result = results.iter().find(|r| r.instance_name == "L_big").unwrap();
        assert_eq!(ind_result.package, "THT",
            "8A inductor should get THT package, got {}", ind_result.package);
    }
}
