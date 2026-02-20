//! Netlist → SchematicData extraction logic.
//!
//! This replaces SKALP's entire `provider.ts` parsing layer. BHDL already has
//! a fully synthesized `Netlist` with explicit nets, instances, and pins, so
//! we extract `SchematicData` directly from the netlist slotmaps.

use std::collections::HashMap;

use bhdl_netlist::{
    Netlist, ConnectionPoint, NetClass, PinDirection, PinType, PortDirection,
    InstanceId, PinId, PinInstanceId, NetId,
};

use crate::types::*;

/// Extract a `SchematicData` from a BHDL `Netlist` and optional analysis result.
///
/// This is the main public API for the Rust extraction layer.
pub fn extract_schematic_data(
    netlist: &Netlist,
    _analysis: Option<&bhdl_analyzer::AnalysisResult>,
) -> Result<SchematicData, String> {
    let top_module_id = netlist.top_level_module
        .ok_or_else(|| "No top-level module in netlist".to_string())?;

    let top_module = netlist.modules.get(top_module_id)
        .ok_or_else(|| "Top-level module not found".to_string())?;

    // --- 1. Collect board-level ports ---
    let mut ports = Vec::new();
    for &port_id in &top_module.ports {
        if let Some(port) = netlist.ports.get(port_id) {
            let direction = match port.direction {
                PortDirection::Input => "in",
                PortDirection::Output => "out",
                PortDirection::InOut => "inout",
                PortDirection::Internal => continue, // skip internal signals
            };
            ports.push(SchematicPort {
                name: port.name.clone(),
                direction: direction.to_string(),
                pin_type: "signal".to_string(),
                width: port.width.unwrap_or(1),
                line: None,
            });
        }
    }

    // --- 2. Build lookup tables for net connections ---
    // Map (InstanceId, PinId) → NetId  and  PinInstanceId → NetId
    let mut pin_inst_to_net: HashMap<PinInstanceId, NetId> = HashMap::new();
    let mut inst_pin_to_net: HashMap<(InstanceId, PinId), NetId> = HashMap::new();
    let mut port_to_net: HashMap<bhdl_netlist::PortId, NetId> = HashMap::new();

    // Also track net names for lookup
    let mut net_names: HashMap<NetId, String> = HashMap::new();
    let mut net_counter = 0;
    // Map (InstanceId, PortId) → NetId for InstancePort connections
    let mut inst_port_to_net: HashMap<(InstanceId, bhdl_netlist::PortId), NetId> = HashMap::new();

    for (net_id, net) in netlist.nets.iter() {
        let net_name = net.name.clone().unwrap_or_else(|| {
            net_counter += 1;
            format!("__net{}", net_counter)
        });
        net_names.insert(net_id, net_name);

        for conn in &net.connections {
            match *conn {
                ConnectionPoint::PinInstance(pi_id) => {
                    pin_inst_to_net.insert(pi_id, net_id);
                }
                ConnectionPoint::InstancePin(inst_id, pin_id) => {
                    inst_pin_to_net.insert((inst_id, pin_id), net_id);
                }
                ConnectionPoint::ModulePort(port_id) => {
                    port_to_net.insert(port_id, net_id);
                }
                ConnectionPoint::InstancePort(inst_id, port_id) => {
                    inst_port_to_net.insert((inst_id, port_id), net_id);
                }
            }
        }
    }

    // --- 3. Classify net types ---
    let mut net_class_map: HashMap<NetId, String> = HashMap::new();
    let mut net_voltage_map: HashMap<NetId, f64> = HashMap::new();
    for (net_id, net) in netlist.nets.iter() {
        let (class_str, voltage) = classify_net(&net.net_class);
        net_class_map.insert(net_id, class_str);
        if let Some(v) = voltage {
            net_voltage_map.insert(net_id, v);
        }
    }

    // --- 4. Collect instances ---
    let mut instances = Vec::new();

    // Determine which instances belong to the top-level module
    let top_instance_ids: Vec<InstanceId> = if top_module.internal_instances.is_empty() {
        // If top module doesn't track instances, use all instances
        netlist.instances.keys().collect()
    } else {
        top_module.internal_instances.clone()
    };

    for &instance_id in &top_instance_ids {
        let instance = match netlist.instances.get(instance_id) {
            Some(inst) => inst,
            None => continue,
        };

        let module_def = match netlist.modules.get(instance.definition) {
            Some(def) => def,
            None => continue,
        };

        let category = categorize_component(&module_def.name, &instance.attributes);

        // Build connections for this instance
        let mut connections = Vec::new();
        let mut found_pins: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Method 1: Use pin instances (preferred — most netlists use this)
        let pin_instances: Vec<_> = netlist.pin_instances.iter()
            .filter(|(_, pi)| pi.instance == instance_id)
            .collect();

        for (pi_id, pi) in &pin_instances {
            let pin_def = match netlist.pins.get(pi.pin_def) {
                Some(p) => p,
                None => continue,
            };

            // Find which net this pin instance is connected to.
            // Prefer the map (built from net.connections) over pi.net,
            // because pi.net can reference stale/merged net IDs.
            let net_id = pin_inst_to_net.get(pi_id).copied()
                .or(pi.net)
                .filter(|nid| net_names.contains_key(nid)); // skip stale IDs
            let signal = match net_id {
                Some(nid) => net_names.get(&nid).cloned().unwrap_or_default(),
                None => String::new(),
            };

            if signal.is_empty() {
                continue; // unconnected pin
            }

            let direction = determine_pin_direction(
                &pin_def.direction,
                &pin_def.pin_type,
                &pin_def.name,
                net_id.and_then(|nid| net_class_map.get(&nid).map(|s| s.as_str())),
            );

            let pin_type_str = pin_type_to_str(&pin_def.pin_type);

            found_pins.insert(pin_def.name.clone());
            connections.push(SchematicConnection {
                port: pin_def.name.clone(),
                signal,
                direction: direction.to_string(),
                pin_type: pin_type_str.to_string(),
            });
        }

        // Method 2: Also check InstancePin connections for any pins not found via PinInstance
        for &pin_id in &module_def.pins {
            let pin_def = match netlist.pins.get(pin_id) {
                Some(p) => p,
                None => continue,
            };

            // Skip pins already found via Method 1
            if found_pins.contains(&pin_def.name) {
                continue;
            }

            let net_id = inst_pin_to_net.get(&(instance_id, pin_id)).copied();
            let signal = match net_id {
                Some(nid) => net_names.get(&nid).cloned().unwrap_or_default(),
                None => String::new(),
            };

            if signal.is_empty() {
                continue; // unconnected pin
            }

            let direction = determine_pin_direction(
                &pin_def.direction,
                &pin_def.pin_type,
                &pin_def.name,
                net_id.and_then(|nid| net_class_map.get(&nid).map(|s| s.as_str())),
            );

            let pin_type_str = pin_type_to_str(&pin_def.pin_type);

            connections.push(SchematicConnection {
                port: pin_def.name.clone(),
                signal,
                direction: direction.to_string(),
                pin_type: pin_type_str.to_string(),
            });
        }

        // Method 3: Check InstancePort connections for ports not found via pins
        for &port_id in &module_def.ports {
            if let Some(port) = netlist.ports.get(port_id) {
                if found_pins.contains(&port.name) {
                    continue;
                }

                let net_id = inst_port_to_net.get(&(instance_id, port_id)).copied();
                let signal = match net_id {
                    Some(nid) => net_names.get(&nid).cloned().unwrap_or_default(),
                    None => String::new(),
                };

                if signal.is_empty() {
                    continue;
                }

                let direction = match port.direction {
                    PortDirection::Input => "in",
                    PortDirection::Output => "out",
                    PortDirection::InOut => "in",
                    PortDirection::Internal => continue,
                };

                // Determine pin type from port name heuristic
                let pin_type_str = if port.name.to_uppercase().contains("VCC")
                    || port.name.to_uppercase().contains("VDD")
                    || port.name == "VI" || port.name == "VO"
                    || port.name == "VIN" || port.name == "VOUT"
                {
                    "power"
                } else if port.name.to_uppercase() == "GND" || port.name.to_uppercase() == "VSS" {
                    "ground"
                } else {
                    "signal"
                };

                connections.push(SchematicConnection {
                    port: port.name.clone(),
                    signal,
                    direction: direction.to_string(),
                    pin_type: pin_type_str.to_string(),
                });
            }
        }

        // Skip instances with no connections (entity definitions, not real instances)
        if connections.is_empty() {
            continue;
        }

        // Extract meaningful parameters from instance attributes
        // Filter out simulation/stress metadata — only show user-visible values
        let parameters: Vec<(String, String)> = instance.attributes.iter()
            .filter(|(k, _)| {
                // Keep: value, voltage, named component params
                // Skip: sim_*, stress_*, calculation_method, simulation_enhanced, empty keys
                !k.starts_with("sim_") && !k.starts_with("stress_")
                    && k.as_str() != "calculation_method"
                    && k.as_str() != "simulation_enhanced"
                    && !k.is_empty()
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();

        instances.push(SchematicInstance {
            name: instance.name.clone(),
            entity_type: module_def.name.clone(),
            category,
            connections,
            parameters,
            line: None,
        });
    }

    // --- 5. Build nets ---
    let mut nets = Vec::new();

    for (net_id, net) in netlist.nets.iter() {
        let net_name = net_names.get(&net_id).cloned().unwrap_or_default();
        let (net_class_str, voltage) = classify_net(&net.net_class);

        // Determine driver and sinks from connection points
        let mut driver: Option<SchematicEndpoint> = None;
        let mut sinks: Vec<SchematicEndpoint> = Vec::new();

        for conn in &net.connections {
            match *conn {
                ConnectionPoint::ModulePort(port_id) => {
                    if let Some(port) = netlist.ports.get(port_id) {
                        let ep = SchematicEndpoint {
                            endpoint_type: "entity_port".to_string(),
                            name: String::new(),
                            port: port.name.clone(),
                        };
                        match port.direction {
                            PortDirection::Input => {
                                // Board input port drives into the design
                                if driver.is_none() { driver = Some(ep); } else { sinks.push(ep); }
                            }
                            PortDirection::Output => {
                                sinks.push(ep);
                            }
                            _ => { sinks.push(ep); }
                        }
                    }
                }
                ConnectionPoint::PinInstance(pi_id) => {
                    if let Some(pi) = netlist.pin_instances.get(pi_id) {
                        if let Some(inst) = netlist.instances.get(pi.instance) {
                            if let Some(pin) = netlist.pins.get(pi.pin_def) {
                                let ep = SchematicEndpoint {
                                    endpoint_type: "instance".to_string(),
                                    name: inst.name.clone(),
                                    port: pin.name.clone(),
                                };
                                let is_driver = matches!(pin.direction, PinDirection::Out)
                                    || matches!(pin.pin_type, PinType::Power);
                                if is_driver && driver.is_none() {
                                    driver = Some(ep);
                                } else {
                                    sinks.push(ep);
                                }
                            }
                        }
                    }
                }
                ConnectionPoint::InstancePin(inst_id, pin_id) => {
                    if let Some(inst) = netlist.instances.get(inst_id) {
                        if let Some(pin) = netlist.pins.get(pin_id) {
                            let ep = SchematicEndpoint {
                                endpoint_type: "instance".to_string(),
                                name: inst.name.clone(),
                                port: pin.name.clone(),
                            };
                            let is_driver = matches!(pin.direction, PinDirection::Out)
                                || matches!(pin.pin_type, PinType::Power);
                            if is_driver && driver.is_none() {
                                driver = Some(ep);
                            } else {
                                sinks.push(ep);
                            }
                        }
                    }
                }
                ConnectionPoint::InstancePort(inst_id, port_id) => {
                    if let Some(inst) = netlist.instances.get(inst_id) {
                        if let Some(port) = netlist.ports.get(port_id) {
                            let ep = SchematicEndpoint {
                                endpoint_type: "instance".to_string(),
                                name: inst.name.clone(),
                                port: port.name.clone(),
                            };
                            match port.direction {
                                PortDirection::Output => {
                                    if driver.is_none() { driver = Some(ep); } else { sinks.push(ep); }
                                }
                                _ => { sinks.push(ep); }
                            }
                        }
                    }
                }
            }
        }

        // If no explicit driver found, use first endpoint as driver
        if driver.is_none() && !sinks.is_empty() {
            driver = Some(sinks.remove(0));
        }

        // Skip nets with no connections
        if driver.is_none() || sinks.is_empty() {
            continue;
        }

        nets.push(SchematicNet {
            name: net_name,
            width: 1,
            net_class: net_class_str,
            voltage,
            driver: driver.unwrap(),
            sinks,
        });
    }

    // --- 5b. Post-filter: remove orphaned power symbols ---
    // A power symbol is orphaned if all its connections go to nets that don't
    // also connect to other non-power-symbol instances.
    let net_name_set: std::collections::HashSet<String> = nets.iter()
        .map(|n| n.name.clone())
        .collect();
    instances.retain(|inst| {
        // Keep all non-power-symbol instances
        if inst.connections.iter().any(|c| c.pin_type != "power" && c.pin_type != "ground") {
            return true;
        }
        // For power symbol instances, keep only if at least one of their signals
        // appears in a routable net (which requires both driver and sinks)
        inst.connections.iter().any(|c| net_name_set.contains(&c.signal))
    });

    // --- 6. Extract power rails ---
    // Collect names of instances that actually appear in the schematic (have connections)
    let instance_names: std::collections::HashSet<String> = instances.iter()
        .map(|i| i.name.clone())
        .collect();

    let mut power_rails = Vec::new();
    for (net_id, net) in netlist.nets.iter() {
        if let NetClass::Power(voltage) = net.net_class {
            let net_name = net_names.get(&net_id).cloned().unwrap_or_default();

            // Find connected instances that are actually in the schematic
            let mut connected = Vec::new();
            for conn in &net.connections {
                let inst_name = match *conn {
                    ConnectionPoint::PinInstance(pi_id) => {
                        netlist.pin_instances.get(pi_id)
                            .and_then(|pi| netlist.instances.get(pi.instance))
                            .map(|inst| inst.name.clone())
                    }
                    ConnectionPoint::InstancePin(inst_id, _) => {
                        netlist.instances.get(inst_id)
                            .map(|inst| inst.name.clone())
                    }
                    ConnectionPoint::InstancePort(inst_id, _) => {
                        netlist.instances.get(inst_id)
                            .map(|inst| inst.name.clone())
                    }
                    _ => None,
                };
                if let Some(name) = inst_name {
                    if !connected.contains(&name) && instance_names.contains(&name) {
                        connected.push(name);
                    }
                }
            }

            // Only include power rails that connect to actual schematic instances
            if !connected.is_empty() {
                power_rails.push(PowerRail {
                    name: net_name,
                    voltage,
                    max_current: 0.0, // TODO: extract from analysis if available
                    connected_instances: connected,
                });
            }
        }
    }

    Ok(SchematicData {
        entity_name: top_module.name.clone(),
        ports,
        instances,
        nets,
        power_rails,
        file_path: None,
        entity_line: None,
    })
}

/// Classify a `NetClass` into a string label and optional voltage.
fn classify_net(net_class: &NetClass) -> (String, Option<f64>) {
    match net_class {
        NetClass::Signal => ("signal".to_string(), None),
        NetClass::Power(v) => ("power".to_string(), Some(*v)),
        NetClass::Ground => ("ground".to_string(), None),
        NetClass::DifferentialPair { .. } => ("signal".to_string(), None),
        NetClass::Bus { .. } => ("signal".to_string(), None),
    }
}

/// Determine whether a pin should be placed on the WEST (in) or EAST (out) side.
fn determine_pin_direction(
    pin_dir: &PinDirection,
    pin_type: &PinType,
    pin_name: &str,
    net_class: Option<&str>,
) -> &'static str {
    match pin_dir {
        PinDirection::In => "in",
        PinDirection::Out => "out",
        PinDirection::InOut => "in", // default inout to input side
        PinDirection::Power => "in",
        PinDirection::Ground => "in",
        PinDirection::Passive => {
            // Heuristic for passive components:
            // - Pin connected to power → input side (power feeds in from left)
            // - Pin connected to ground → output side (ground sinks to right)
            // - Pin "1" → input, Pin "2" → output (left-to-right flow)
            if matches!(net_class, Some("power")) {
                "in"
            } else if matches!(net_class, Some("ground")) {
                "out" // ground is always a sink → right side
            } else if pin_name == "1" || pin_name == "A" || pin_name == "pos" || pin_name == "IN" {
                "in"
            } else if pin_name == "2" || pin_name == "K" || pin_name == "neg" || pin_name == "OUT" {
                "out"
            } else {
                // Default: use pin_type as a hint
                match pin_type {
                    PinType::Power | PinType::Ground => "in",
                    _ => "in",
                }
            }
        }
    }
}

/// Convert PinType to a string label for the renderer.
fn pin_type_to_str(pin_type: &PinType) -> &'static str {
    match pin_type {
        PinType::Signal => "signal",
        PinType::Power => "power",
        PinType::Ground => "ground",
        PinType::Clock => "clock",
        PinType::Reset => "reset",
        PinType::AnalogIn => "signal",
        PinType::AnalogOut => "signal",
        PinType::DifferentialPos => "signal",
        PinType::DifferentialNeg => "signal",
        PinType::Passive => "passive",
    }
}

/// Categorize a component by its entity type name and attributes.
fn categorize_component(entity_type: &str, attrs: &HashMap<String, String>) -> String {
    let lower = entity_type.to_lowercase();
    if lower.starts_with("res") || lower == "r" {
        "resistor".to_string()
    } else if lower.starts_with("cap") || lower == "c" {
        "capacitor".to_string()
    } else if lower.starts_with("ind") || lower == "l" {
        "inductor".to_string()
    } else if lower.starts_with("led") || lower.starts_with("diode") || lower == "d" {
        "diode".to_string()
    } else if lower.contains("regulator") || lower.contains("7805") || lower.contains("lm78")
        || lower.contains("ldo") || lower.contains("vreg")
    {
        "regulator".to_string()
    } else if lower.starts_with("tvs") {
        "protection".to_string()
    } else if lower.starts_with("buf") || lower.starts_with("buffer") {
        "buffer".to_string()
    } else if lower.starts_with("osc") || lower.starts_with("xtal") || lower.starts_with("crystal") {
        "oscillator".to_string()
    } else if lower.starts_with("conn") || lower.starts_with("header") || lower.starts_with("usb") {
        "connector".to_string()
    } else if attrs.contains_key("value") {
        // Has a value → likely a passive
        "passive".to_string()
    } else {
        "ic".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::*;

    /// Build a minimal test netlist: a board with two resistors in series.
    fn make_test_netlist() -> Netlist {
        let mut nl = Netlist::new();

        // Top-level board
        let board_id = nl.add_module("TestBoard".into(), ModuleKind::Board);
        nl.top_level_module = Some(board_id);

        // Add a board input port
        let in_port = nl.add_port(board_id, "VIN".into(), PortDirection::Input, None).unwrap();

        // Add a board output port
        let out_port = nl.add_port(board_id, "VOUT".into(), PortDirection::Output, None).unwrap();

        // Resistor component definition
        let res_def = nl.add_module("Res".into(), ModuleKind::PhysicalComponent);
        let res_pin1 = nl.add_pin(res_def, "1".into(), PinDirection::Passive, PinType::Passive).unwrap();
        let res_pin2 = nl.add_pin(res_def, "2".into(), PinDirection::Passive, PinType::Passive).unwrap();

        // Instance R1
        let r1_id = nl.add_instance("R1".into(), res_def).unwrap();
        {
            let r1 = nl.instances.get_mut(r1_id).unwrap();
            r1.attributes.insert("value".into(), "10k".into());
        }
        let r1_pins = nl.create_pin_instances(r1_id).unwrap();

        // Instance R2
        let r2_id = nl.add_instance("R2".into(), res_def).unwrap();
        {
            let r2 = nl.instances.get_mut(r2_id).unwrap();
            r2.attributes.insert("value".into(), "4.7k".into());
        }
        let r2_pins = nl.create_pin_instances(r2_id).unwrap();

        // Register instances with board
        let board = nl.modules.get_mut(board_id).unwrap();
        board.internal_instances.push(r1_id);
        board.internal_instances.push(r2_id);

        // Net: VIN -> R1.1
        let net_vin = nl.add_net(Some("VIN".into()));
        nl.connect(net_vin, ConnectionPoint::ModulePort(in_port)).unwrap();
        nl.connect(net_vin, ConnectionPoint::PinInstance(r1_pins[0])).unwrap();

        // Net: R1.2 -> R2.1
        let net_mid = nl.add_net(Some("mid".into()));
        nl.connect(net_mid, ConnectionPoint::PinInstance(r1_pins[1])).unwrap();
        nl.connect(net_mid, ConnectionPoint::PinInstance(r2_pins[0])).unwrap();

        // Net: R2.2 -> VOUT
        let net_vout = nl.add_net(Some("VOUT".into()));
        nl.connect(net_vout, ConnectionPoint::PinInstance(r2_pins[1])).unwrap();
        nl.connect(net_vout, ConnectionPoint::ModulePort(out_port)).unwrap();

        nl
    }

    #[test]
    fn test_extract_basic() {
        let nl = make_test_netlist();
        let data = extract_schematic_data(&nl, None).unwrap();

        assert_eq!(data.entity_name, "TestBoard");
        assert_eq!(data.ports.len(), 2);
        assert_eq!(data.instances.len(), 2);

        // Check ports
        let in_port = data.ports.iter().find(|p| p.name == "VIN").unwrap();
        assert_eq!(in_port.direction, "in");
        let out_port = data.ports.iter().find(|p| p.name == "VOUT").unwrap();
        assert_eq!(out_port.direction, "out");

        // Check instances
        let r1 = data.instances.iter().find(|i| i.name == "R1").unwrap();
        assert_eq!(r1.entity_type, "Res");
        assert_eq!(r1.category, "resistor");
        assert_eq!(r1.connections.len(), 2);

        // Check nets — should have 3 nets connecting everything
        assert!(data.nets.len() >= 2, "Expected at least 2 routable nets, got {}", data.nets.len());
    }

    #[test]
    fn test_extract_power_rails() {
        let mut nl = Netlist::new();
        let board_id = nl.add_module("PowerBoard".into(), ModuleKind::Board);
        nl.top_level_module = Some(board_id);

        // Create a power net
        let vcc_net = nl.add_net_with_class(Some("VCC".into()), NetClass::Power(5.0));

        // Resistor connected to VCC
        let res_def = nl.add_module("Res".into(), ModuleKind::PhysicalComponent);
        let _res_pin1 = nl.add_pin(res_def, "1".into(), PinDirection::Passive, PinType::Passive).unwrap();
        let _res_pin2 = nl.add_pin(res_def, "2".into(), PinDirection::Passive, PinType::Passive).unwrap();

        let r1_id = nl.add_instance("R1".into(), res_def).unwrap();
        let r1_pins = nl.create_pin_instances(r1_id).unwrap();

        let board = nl.modules.get_mut(board_id).unwrap();
        board.internal_instances.push(r1_id);

        nl.connect(vcc_net, ConnectionPoint::PinInstance(r1_pins[0])).unwrap();

        let data = extract_schematic_data(&nl, None).unwrap();
        assert_eq!(data.power_rails.len(), 1);
        assert_eq!(data.power_rails[0].name, "VCC");
        assert_eq!(data.power_rails[0].voltage, 5.0);
        assert!(data.power_rails[0].connected_instances.contains(&"R1".to_string()));
    }

    #[test]
    fn test_categorize_component() {
        let empty = HashMap::new();
        assert_eq!(categorize_component("Res", &empty), "resistor");
        assert_eq!(categorize_component("Cap", &empty), "capacitor");
        assert_eq!(categorize_component("LED", &empty), "diode");
        assert_eq!(categorize_component("LM7805", &empty), "regulator");
        assert_eq!(categorize_component("TVSDiode", &empty), "protection");
        assert_eq!(categorize_component("ATmega328", &empty), "ic");

        let mut with_value = HashMap::new();
        with_value.insert("value".into(), "100".into());
        assert_eq!(categorize_component("Unknown", &with_value), "passive");
    }
}
