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
use bhdl_netlist::{ConnectionPoint, NetId, Netlist};

use crate::passive_component_calculator::{DielectricType, PackageSize, PassiveComponentCalculator};
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

/// Describes a capacitor that must be split into a parallel bank.
#[derive(Debug)]
struct BankSplit {
    original_id: bhdl_netlist::InstanceId,
    original_name: String,
    count: usize,
    per_unit_value: String,
    package: String,
    voltage_rating: Option<String>,
    dielectric: Option<String>,
    /// Propagated from original instance so bank children stay grouped
    /// with their virtual-pin expansion parent in the schematic layout.
    vpin_parent: Option<String>,
    vpin_role: Option<String>,
    /// Propagated stage/intent metadata so bank children share their
    /// parent's stage coloring and intent in the schematic viewer.
    stage_name: Option<String>,
    stage_order: Option<String>,
    stage_rail: Option<String>,
}

/// Format a capacitance value in Farads as a human-readable string.
fn format_cap_value(farads: f64) -> String {
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

/// Find the two nets connected to an instance's pins (pin 1 and pin 2).
/// Returns (net_for_pin1, net_for_pin2) by scanning the netlist connections.
fn find_instance_nets(
    netlist: &Netlist,
    inst_id: bhdl_netlist::InstanceId,
) -> (Option<NetId>, Option<NetId>) {
    let instance = match netlist.instances.get(inst_id) {
        Some(i) => i,
        None => return (None, None),
    };
    let module_def = match netlist.modules.get(instance.definition) {
        Some(d) => d,
        None => return (None, None),
    };

    // Collect pin instances for this instance, ordered by pin definition
    let mut pin_nets: Vec<Option<NetId>> = Vec::new();
    for &pin_id in &module_def.pins {
        // Find the pin instance for (this instance, this pin_def)
        let pi_id = netlist.pin_instances.iter()
            .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin_id)
            .map(|(id, _)| id);

        let net_id = pi_id.and_then(|pi_id| {
            // Scan nets for one containing this pin instance (authoritative)
            let target = ConnectionPoint::PinInstance(pi_id);
            netlist.nets.iter()
                .find(|(_, net)| net.connections.contains(&target))
                .map(|(nid, _)| nid)
        });
        pin_nets.push(net_id);
    }

    let net1 = pin_nets.first().copied().flatten();
    let net2 = pin_nets.get(1).copied().flatten();
    (net1, net2)
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

    // Bank splits collected during the loop, applied afterwards to avoid
    // mutating the netlist while iterating over instances.
    let mut bank_splits: Vec<BankSplit> = Vec::new();

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
                    // Check if the capacitance exceeds what's realizable in one part
                    let capacitance = attrs.get("value")
                        .and_then(|v| parse_unit_value(v));
                    let max_per_unit = result.dielectric.as_ref()
                        .and_then(|d| DielectricType::from_display_str(d))
                        .and_then(|dt| PackageSize::from_str(&result.package).map(|ps| (dt, ps)))
                        .map(|(dt, ps)| PackageSelector::max_realizable_capacitance(dt, ps));

                    let needs_split = match (capacitance, max_per_unit) {
                        (Some(c), Some(max)) => c > max * 1.05,
                        _ => false,
                    };

                    if needs_split {
                        let c = capacitance.unwrap();
                        let max = max_per_unit.unwrap();
                        let count = (c / max).ceil() as usize;
                        let per_unit = c / count as f64;
                        let per_unit_str = format_cap_value(per_unit);
                        let total_str = format_cap_value(c);

                        info!(
                            "Capacitor bank split: {} ({}) → {}× {}",
                            inst_name, total_str, count, per_unit_str
                        );

                        // Update original instance to per-unit value
                        let inst_mut = &mut netlist.instances[inst_id];
                        inst_mut.attributes.insert("value".to_string(), per_unit_str.clone());
                        inst_mut.attributes.insert("bank_count".to_string(), count.to_string());
                        inst_mut.attributes.insert("bank_total".to_string(), total_str);
                        inst_mut.attributes.insert("package".to_string(), result.package.clone());
                        if let Some(ref vr) = result.voltage_rating {
                            inst_mut.attributes.insert("voltage_rating".to_string(), vr.clone());
                        }
                        if let Some(ref di) = result.dielectric {
                            inst_mut.attributes.insert("dielectric".to_string(), di.clone());
                        }

                        // Schedule creation of (count - 1) additional parallel instances.
                        // Propagate vpin_parent/vpin_role so bank children stay grouped
                        // with their expansion parent in the schematic layout.
                        bank_splits.push(BankSplit {
                            original_id: inst_id,
                            original_name: inst_name.clone(),
                            count,
                            per_unit_value: per_unit_str,
                            package: result.package.clone(),
                            voltage_rating: result.voltage_rating.clone(),
                            dielectric: result.dielectric.clone(),
                            vpin_parent: attrs.get("vpin_parent").cloned(),
                            vpin_role: attrs.get("vpin_role").cloned(),
                            stage_name: attrs.get("stage_name").cloned(),
                            stage_order: attrs.get("stage_order").cloned(),
                            stage_rail: attrs.get("stage_rail").cloned(),
                        });

                        results.push(PhysicalSelectionResult {
                            instance_name: inst_name,
                            component_type: "capacitor".to_string(),
                            package: result.package,
                            power_rating: None,
                            voltage_rating: result.voltage_rating,
                            dielectric: result.dielectric,
                        });
                    } else {
                        // Normal single-cap path
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

    // ── Phase 2: create additional parallel instances for bank splits ────
    for split in &bank_splits {
        let (net_pin1, net_pin2) = find_instance_nets(netlist, split.original_id);

        // Find or create a Cap module with pins "1" and "2"
        let cap_mod = crate::virtual_pin_expander::find_or_create_module(
            netlist, "Cap", &[("1", true), ("2", true)],
        );

        for i in 1..split.count {
            let name = format!("{}_{}", split.original_name, i + 1);
            let count_str = split.count.to_string();
            let mut attrs: Vec<(&str, &str)> = vec![
                ("component_class", "capacitor"),
                ("value", &split.per_unit_value),
                ("bank_count", &count_str),
                ("bank_parent", &split.original_name),
                ("package", &split.package),
            ];

            if let Some(ref vr) = split.voltage_rating {
                attrs.push(("voltage_rating", vr));
            }
            if let Some(ref di) = split.dielectric {
                attrs.push(("dielectric", di));
            }
            // Propagate expansion metadata so schematic groups bank children
            // with the virtual-pin parent (e.g. buck regulator)
            if let Some(ref vp) = split.vpin_parent {
                attrs.push(("vpin_parent", vp));
            }
            if let Some(ref vr) = split.vpin_role {
                attrs.push(("vpin_role", vr));
            }
            // Propagate stage/intent metadata so bank children share
            // parent's stage coloring in the schematic viewer
            if let Some(ref sn) = split.stage_name {
                attrs.push(("stage_name", sn));
            }
            if let Some(ref so) = split.stage_order {
                attrs.push(("stage_order", so));
            }
            if let Some(ref sr) = split.stage_rail {
                attrs.push(("stage_rail", sr));
            }

            let new_id = crate::virtual_pin_expander::create_instance(
                netlist, &name, cap_mod, &attrs,
            );
            let pins = netlist.create_pin_instances(new_id)
                .unwrap_or_default();

            // Connect pins to the same nets as the original capacitor
            if let Some(n1) = net_pin1 {
                let _ = crate::virtual_pin_expander::connect_pin_instance_by_name(
                    netlist, new_id, &pins, "1", n1,
                );
            }
            if let Some(n2) = net_pin2 {
                let _ = crate::virtual_pin_expander::connect_pin_instance_by_name(
                    netlist, new_id, &pins, "2", n2,
                );
            }

            debug!("  Created bank instance {} on same nets as {}", name, split.original_name);
        }
    }

    if !bank_splits.is_empty() {
        info!("Capacitor bank splitting: {} capacitor(s) split into parallel banks", bank_splits.len());
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
///
/// If the instance has a `dielectric_hint` attribute (e.g. from multi-tier
/// ripple bank generation), that dielectric is used instead of the default
/// selection. This ensures bulk caps get X5R, mid-freq caps get X7R, and
/// HF bypass caps get C0G.
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

    // If a dielectric_hint is set (from multi-tier ripple bank), override the default
    let dielectric = if let Some(hint) = attrs.get("dielectric_hint") {
        debug!("Capacitor {}: using dielectric_hint={} (from ripple tier)",
            inst_name, hint);
        hint.clone()
    } else {
        spec.dielectric.to_string()
    };

    // Re-select package if dielectric was overridden (different dielectrics
    // have different max capacitance per package)
    let package = if attrs.contains_key("dielectric_hint") {
        // Use the dielectric-specific package selection
        let dt = DielectricType::from_display_str(&dielectric);
        if let Some(dt) = dt {
            selector.select_capacitor_package_for_dielectric(capacitance, voltage_rating, dt, requirements)
                .unwrap_or_else(|| spec.package.to_string())
        } else {
            spec.package.to_string()
        }
    } else {
        spec.package.to_string()
    };

    debug!(
        "Capacitor {}: C={:.3e}F, Vmax={:.2}V → {} / {} / {}{}",
        inst_name,
        capacitance,
        max_voltage,
        package,
        spec.voltage_rating,
        dielectric,
        if attrs.contains_key("ripple_tier") {
            format!(" [tier: {}]", attrs.get("ripple_tier").unwrap())
        } else {
            String::new()
        },
    );

    Some(PhysicalSelectionResult {
        instance_name: inst_name.to_string(),
        component_type: "capacitor".to_string(),
        package,
        power_rating: None,
        voltage_rating: Some(spec.voltage_rating.to_string()),
        dielectric: Some(dielectric),
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
/// For expanded buck inductors (identified by `vpin_role = "series"`),
/// GLACIER reports 0A because both terminals sit at the same DC voltage.
/// In this case, we find the VOUT-side net and use the total load current
/// on that net — which is exactly what the inductor must carry. The cascade
/// fixup in `build_simulation_annotations()` ensures that power sources on
/// the VOUT net already include all downstream regulator loads.
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

    // For expanded buck inductors, the DC inductor current is 0 because both
    // sides sit at the same voltage. Infer the actual current from the
    // VOUT-side net's total load current.
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

    // Find the VOUT-side pin of the inductor ("OUT" for expansion inductors, "2" for legacy)
    let pin2_id = module_def.pins.iter()
        .find(|&&pid| netlist.pins.get(pid).map(|p| p.name == "OUT" || p.name == "2").unwrap_or(false))
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
            layout_intents: Vec::new(),
        });

        // Create capacitor instance with 100nF value
        let mut cap_attrs = HashMap::new();
        cap_attrs.insert("value".to_string(), "100nF".to_string());
        let cap_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "C1".to_string(),
            definition: cap_mod_id,
            attributes: cap_attrs,
            layout_intents: Vec::new(),
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
            layout_intents: Vec::new(),
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
            layout_intents: Vec::new(),
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
            layout_intents: Vec::new(),
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

    // ── Capacitor bank splitting tests ──────────────────────────────────

    /// Create a capacitor-only test netlist with the given capacitance value string.
    /// Returns (netlist, cap_id, net1_id, net2_id).
    fn make_cap_netlist(cap_value: &str) -> (Netlist, bhdl_netlist::InstanceId, bhdl_netlist::NetId, bhdl_netlist::NetId) {
        let mut netlist = Netlist::default();

        // Create Cap module with two passive pins using the proper API
        let cap_mod_id = netlist.add_module("Cap".to_string(), bhdl_netlist::ModuleKind::PhysicalComponent);
        netlist.add_pin(cap_mod_id, "1".to_string(), bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Passive);
        netlist.add_pin(cap_mod_id, "2".to_string(), bhdl_netlist::PinDirection::InOut, bhdl_netlist::PinType::Passive);

        let mut cap_attrs = HashMap::new();
        cap_attrs.insert("value".to_string(), cap_value.to_string());
        cap_attrs.insert("component_class".to_string(), "capacitor".to_string());
        let cap_id = netlist.instances.insert(bhdl_netlist::Instance {
            name: "C1".to_string(),
            definition: cap_mod_id,
            attributes: cap_attrs,
            layout_intents: Vec::new(),
        });

        // Create pin instances
        let pin_insts = netlist.create_pin_instances(cap_id).unwrap();

        // Create two nets and connect the cap
        let net1 = netlist.add_net(Some("VOUT".to_string()));
        let net2 = netlist.add_net(Some("GND".to_string()));
        netlist.connect(net1, bhdl_netlist::ConnectionPoint::PinInstance(pin_insts[0])).unwrap();
        netlist.connect(net2, bhdl_netlist::ConnectionPoint::PinInstance(pin_insts[1])).unwrap();

        (netlist, cap_id, net1, net2)
    }

    #[test]
    fn test_capacitor_bank_split_needed() {
        // 470µF X5R/1210 should split: max per unit = 47µF → 10× 47µF
        let (mut netlist, cap_id, _, _) = make_cap_netlist("470µF");

        // 5V across the cap
        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        let initial_instances = netlist.instances.len();

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        // Should have a result for C1
        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");

        // Original instance should have bank_count attribute
        let orig = &netlist.instances[cap_id];
        assert!(orig.attributes.contains_key("bank_count"),
            "Original cap should have bank_count attr");
        let count: usize = orig.attributes.get("bank_count").unwrap().parse().unwrap();
        assert!(count > 1, "bank_count should be > 1 for 470µF, got {}", count);

        // Should have created (count - 1) additional instances
        let total = netlist.instances.len();
        assert_eq!(total, initial_instances + count - 1,
            "Expected {} total instances ({} original + {} new), got {}",
            initial_instances + count - 1, initial_instances, count - 1, total);

        // Original should have updated value (per-unit, not total)
        let per_unit_value = orig.attributes.get("value").unwrap();
        assert_ne!(per_unit_value, "470µF",
            "Original value should be updated to per-unit value, got {}", per_unit_value);

        // bank_total should record the original total
        assert!(orig.attributes.contains_key("bank_total"),
            "Original cap should have bank_total attr");
    }

    #[test]
    fn test_capacitor_bank_no_split() {
        // 100nF X7R should NOT split (well under max for any package)
        let (mut netlist, cap_id, _, _) = make_cap_netlist("100nF");

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 3.3);
        net_voltages.insert("GND".to_string(), 0.0);

        let initial_instances = netlist.instances.len();

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");

        // No additional instances should be created
        assert_eq!(netlist.instances.len(), initial_instances,
            "100nF cap should not be split");

        // Should NOT have bank_count attribute
        let inst = &netlist.instances[cap_id];
        assert!(!inst.attributes.contains_key("bank_count"),
            "100nF cap should not have bank_count");
    }

    #[test]
    fn test_capacitor_bank_moderate() {
        // 100µF X5R/1206 should split: max per unit = 22µF → ceil(100/22) = 5× 20µF
        let (mut netlist, cap_id, _, _) = make_cap_netlist("100µF");

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 3.3);
        net_voltages.insert("GND".to_string(), 0.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");

        let orig = &netlist.instances[cap_id];
        assert!(orig.attributes.contains_key("bank_count"),
            "100µF cap should be split into a bank");
        let count: usize = orig.attributes.get("bank_count").unwrap().parse().unwrap();
        assert!(count >= 2 && count <= 10,
            "100µF should split into 2-10 units, got {}", count);
    }

    #[test]
    fn test_bank_instances_connected() {
        // Verify that new bank instances are connected to the same nets
        let (mut netlist, _cap_id, net1, net2) = make_cap_netlist("470µF");

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        let _results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        // Find all bank child instances (name starts with "C1_")
        let bank_children: Vec<_> = netlist.instances.iter()
            .filter(|(_, i)| i.name.starts_with("C1_"))
            .collect();

        assert!(!bank_children.is_empty(), "Should have bank child instances");

        // Each child should be connected to both nets
        for (child_id, child) in &bank_children {
            let (child_net1, child_net2) = find_instance_nets(&netlist, *child_id);
            assert!(child_net1.is_some() && child_net2.is_some(),
                "Bank child {} should be connected to two nets", child.name);
            assert_eq!(child_net1.unwrap(), net1,
                "Bank child {} pin 1 should be on VOUT net", child.name);
            assert_eq!(child_net2.unwrap(), net2,
                "Bank child {} pin 2 should be on GND net", child.name);
        }
    }

    #[test]
    fn test_format_cap_value() {
        assert_eq!(format_cap_value(470e-6), "470µF");
        assert_eq!(format_cap_value(47e-6), "47µF");
        assert_eq!(format_cap_value(100e-9), "100nF");
        assert_eq!(format_cap_value(10e-12), "10pF");
        assert_eq!(format_cap_value(2.2e-6), "2.2µF");
        assert_eq!(format_cap_value(1e-3), "1mF");
    }

    #[test]
    fn test_dielectric_hint_respected() {
        // A capacitor with dielectric_hint="C0G" should get C0G, not the default
        let (mut netlist, cap_id, _, _) = make_cap_netlist("100nF");

        // Set dielectric_hint as if placed by multi-tier ripple bank
        netlist.instances[cap_id].attributes.insert(
            "dielectric_hint".to_string(), "C0G".to_string(),
        );
        netlist.instances[cap_id].attributes.insert(
            "ripple_tier".to_string(), "hf_bypass".to_string(),
        );

        let mut net_voltages = HashMap::new();
        net_voltages.insert("VOUT".to_string(), 5.0);
        net_voltages.insert("GND".to_string(), 0.0);

        let results = apply_glacier_physical_selection(
            &mut netlist,
            &HashMap::new(),
            &HashMap::new(),
            &net_voltages,
        );

        let cap_result = results.iter().find(|r| r.instance_name == "C1");
        assert!(cap_result.is_some(), "C1 should have physical selection");
        let cap_result = cap_result.unwrap();

        // Dielectric should be C0G (from hint), not the default X7R
        assert_eq!(cap_result.dielectric.as_deref(), Some("C0G"),
            "dielectric_hint=C0G should be respected, got {:?}", cap_result.dielectric);

        // Verify the attribute was written to the instance
        let inst = &netlist.instances[cap_id];
        assert_eq!(inst.attributes.get("dielectric").map(|s| s.as_str()), Some("C0G"));
    }
}
