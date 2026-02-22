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
            if let bhdl_netlist::ConnectionPoint::InstancePort(iid, _)
                | bhdl_netlist::ConnectionPoint::InstancePin(iid, _) = conn
            {
                if let Some(inst) = netlist.instances.get(*iid) {
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
}
