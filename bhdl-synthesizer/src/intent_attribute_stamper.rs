// Intent Attribute Stamper
//
// Bridges the gap between the analyzer's FlowTracker (which knows about
// design intents on flow paths) and the synthesizer's netlist (which knows
// about component instances). After synthesis produces the netlist, this
// pass scans FlowTracker results and stamps intent parameters onto the
// relevant netlist instances as attributes.
//
// Downstream passes (virtual_pin_expander, glacier_physical_selection)
// read these attributes to make intent-informed decisions — e.g. computing
// a multi-tier capacitor bank when `output_filtering(max_ripple: 5mV)` is
// present on a buck converter's output flow.

use std::collections::HashMap;
use log::{debug, info};
use bhdl_analyzer::flow_tracking::{FlowTracker, RailStageMap};
use bhdl_common::intent::IntentParam;
use bhdl_netlist::Netlist;

/// Scan FlowTracker results and stamp intent parameters onto netlist instances.
///
/// For each flow path with an intent:
/// 1. Find all component names in the flow
/// 2. Match them to netlist instances
/// 3. Stamp `intent_name`, `stage_name`, `stage_rail`, `stage_order` and all intent params
///
/// This must be called **after synthesis** (netlist exists) and **before**
/// virtual pin expansion (which reads these attributes).
pub fn stamp_intent_attributes(
    netlist: &mut Netlist,
    flow_tracker: &FlowTracker,
) {
    let flow_paths = flow_tracker.get_flow_paths();
    let rail_stage_map = flow_tracker.get_rail_stage_map();
    let mut stamped = 0usize;

    for flow in flow_paths {
        let intent = match &flow.intent {
            Some(i) => i,
            None => continue,
        };

        debug!("Processing flow #{} intent: {}({:?})",
            flow.id, intent.name, intent.params);

        // Build a map of intent parameter name → value string
        let param_map = extract_intent_params(&intent.params);

        // Resolve stage metadata: find which rail and what order this intent maps to
        let (stage_rail, stage_order) = resolve_stage_info(&intent.name, &flow.nets, rail_stage_map);

        // Find all netlist instances whose names appear in this flow's component list
        let matching_instances: Vec<_> = netlist.instances.iter()
            .filter(|(_, inst)| {
                flow.components.iter().any(|c| c == &inst.name)
            })
            .map(|(id, inst)| (id, inst.name.clone()))
            .collect();

        for (inst_id, inst_name) in matching_instances {
            let inst = &mut netlist.instances[inst_id];

            // Stamp intent name
            inst.attributes.insert("intent_name".to_string(), intent.name.clone());

            // Stamp stage metadata (from power domain stage chain)
            inst.attributes.insert("stage_name".to_string(), intent.name.clone());
            if let Some(ref rail) = stage_rail {
                inst.attributes.insert("stage_rail".to_string(), rail.clone());
            }
            if let Some(order) = stage_order {
                inst.attributes.insert("stage_order".to_string(), order.to_string());
            }

            // Stamp all intent parameters with "intent_" prefix
            for (key, value) in &param_map {
                inst.attributes.insert(
                    format!("intent_{}", key),
                    value.clone(),
                );
            }

            debug!("Stamped intent '{}' (stage_order={:?}) on '{}'",
                intent.name, stage_order, inst_name);
            stamped += 1;
        }

        // Also check for instances connected to nets in the flow path.
        // A buck converter might not be in the `components` list if the flow
        // was declared on the output net rather than the component itself.
        if intent.name == "output_filtering" {
            stamp_regulators_on_flow_nets(netlist, &flow.nets, &intent.name, &param_map, &mut stamped);
        }

        // For input_filtering: stamp capacitors connected to flow nets
        if intent.name == "input_filtering" {
            stamp_input_caps_on_flow_nets(netlist, &flow.nets, &intent.name, &param_map, &mut stamped);
        }
    }

    if stamped > 0 {
        info!("Intent attribute stamper: stamped {} instance(s)", stamped);
    }
}

/// Resolve the rail name and stage order for an intent from the RailStageMap.
/// Returns (Some(rail_name), Some(order)) if the intent name matches a declared stage.
fn resolve_stage_info(
    intent_name: &str,
    flow_nets: &[String],
    rail_stage_map: &RailStageMap,
) -> (Option<String>, Option<usize>) {
    // Check each rail's stage list for a match with the intent name
    for (rail_name, stages) in rail_stage_map {
        // The flow must be connected to this rail (rail name appears in flow nets)
        let rail_match = flow_nets.iter().any(|n| n == rail_name);
        if !rail_match {
            continue;
        }

        for (order, (stage_name, _)) in stages.iter().enumerate() {
            if stage_name == intent_name {
                return (Some(rail_name.clone()), Some(order));
            }
        }
    }

    // No match in any rail's stage chain
    (None, None)
}

/// For `output_filtering` intents: find switching regulators whose output nets
/// overlap with the flow path's net list, and stamp them.
fn stamp_regulators_on_flow_nets(
    netlist: &mut Netlist,
    flow_nets: &[String],
    intent_name: &str,
    param_map: &HashMap<String, String>,
    stamped: &mut usize,
) {
    // Build a set of net names in this flow
    let flow_net_set: std::collections::HashSet<&str> = flow_nets.iter().map(|s| s.as_str()).collect();

    // For each switching regulator, check if any of its connected nets overlap
    let reg_instances: Vec<_> = netlist.instances.iter()
        .filter(|(_, inst)| {
            inst.attributes.get("component_class")
                .map(|c| c == "switching_regulator")
                .unwrap_or(false)
                && !inst.attributes.contains_key("intent_name") // not already stamped
        })
        .map(|(id, inst)| (id, inst.name.clone()))
        .collect();

    for (inst_id, inst_name) in reg_instances {
        // Find nets connected to this instance's output pin
        let instance = &netlist.instances[inst_id];
        let module_def = match netlist.modules.get(instance.definition) {
            Some(d) => d,
            None => continue,
        };

        let mut output_net_name: Option<String> = None;
        for &pin_id in &module_def.pins {
            let pin = match netlist.pins.get(pin_id) {
                Some(p) => p,
                None => continue,
            };
            // Look for output power pin (VOUT)
            if !pin.name.to_uppercase().contains("OUT") {
                continue;
            }
            // Find pin instance
            let pi_id = netlist.pin_instances.iter()
                .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin_id)
                .map(|(id, _)| id);
            if let Some(pi_id) = pi_id {
                // Find net
                let net_name = netlist.nets.iter()
                    .find(|(_, net)| {
                        net.connections.contains(&bhdl_netlist::ConnectionPoint::PinInstance(pi_id))
                    })
                    .and_then(|(_, net)| net.name.clone());
                output_net_name = net_name;
            }
        }

        if let Some(ref net_name) = output_net_name {
            if flow_net_set.contains(net_name.as_str()) {
                let inst = &mut netlist.instances[inst_id];
                inst.attributes.insert("intent_name".to_string(), intent_name.to_string());
                for (key, value) in param_map {
                    inst.attributes.insert(
                        format!("intent_{}", key),
                        value.clone(),
                    );
                }
                debug!("Stamped intent '{}' on regulator '{}' via output net '{}'",
                    intent_name, inst_name, net_name);
                *stamped += 1;
            }
        }
    }
}

/// For `input_filtering` intents: find capacitors whose connected nets overlap
/// with the flow path's net list, and stamp them with intent attributes.
fn stamp_input_caps_on_flow_nets(
    netlist: &mut Netlist,
    flow_nets: &[String],
    intent_name: &str,
    param_map: &HashMap<String, String>,
    stamped: &mut usize,
) {
    let flow_net_set: std::collections::HashSet<&str> = flow_nets.iter().map(|s| s.as_str()).collect();

    // Find capacitor instances not already stamped
    let cap_instances: Vec<_> = netlist.instances.iter()
        .filter(|(_, inst)| {
            let is_cap = inst.attributes.get("component_class")
                .map(|c| c == "capacitor")
                .unwrap_or(false)
                || netlist.modules.get(inst.definition)
                    .map(|m| m.name.starts_with("Cap"))
                    .unwrap_or(false);
            is_cap && !inst.attributes.contains_key("intent_name")
        })
        .map(|(id, inst)| (id, inst.name.clone()))
        .collect();

    for (inst_id, inst_name) in cap_instances {
        // Find nets connected to this capacitor's pins
        let instance = &netlist.instances[inst_id];
        let module_def = match netlist.modules.get(instance.definition) {
            Some(d) => d,
            None => continue,
        };

        let mut connected_net_name: Option<String> = None;
        for &pin_id in &module_def.pins {
            let pi_id = netlist.pin_instances.iter()
                .find(|(_, pi)| pi.instance == inst_id && pi.pin_def == pin_id)
                .map(|(id, _)| id);
            if let Some(pi_id) = pi_id {
                let net_name = netlist.nets.iter()
                    .find(|(_, net)| {
                        net.connections.contains(&bhdl_netlist::ConnectionPoint::PinInstance(pi_id))
                    })
                    .and_then(|(_, net)| net.name.clone());
                if let Some(ref name) = net_name {
                    if flow_net_set.contains(name.as_str()) {
                        connected_net_name = Some(name.clone());
                        break;
                    }
                }
            }
        }

        if let Some(ref net_name) = connected_net_name {
            let inst = &mut netlist.instances[inst_id];
            inst.attributes.insert("intent_name".to_string(), intent_name.to_string());
            for (key, value) in param_map {
                inst.attributes.insert(
                    format!("intent_{}", key),
                    value.clone(),
                );
            }
            debug!("Stamped intent '{}' on capacitor '{}' via flow net '{}'",
                intent_name, inst_name, net_name);
            *stamped += 1;
        }
    }
}

/// Extract named intent parameters into a flat string map.
/// Positional parameters are stored as "param_0", "param_1", etc.
fn extract_intent_params(params: &[IntentParam]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let mut pos_idx = 0;

    for param in params {
        match param {
            IntentParam::Named(name, value) => {
                map.insert(name.clone(), format_intent_value(value));
            }
            IntentParam::Positional(value) => {
                map.insert(format!("param_{}", pos_idx), format_intent_value(value));
                pos_idx += 1;
            }
        }
    }

    map
}

/// Format an IntentValue as a string suitable for netlist attributes.
fn format_intent_value(value: &bhdl_common::intent::IntentValue) -> String {
    match value {
        bhdl_common::intent::IntentValue::Number(n, Some(unit)) => format!("{}{}", n, unit),
        bhdl_common::intent::IntentValue::Number(n, None) => format!("{}", n),
        bhdl_common::intent::IntentValue::String(s) => s.clone(),
        bhdl_common::intent::IntentValue::Boolean(b) => format!("{}", b),
        bhdl_common::intent::IntentValue::Identifier(id) => id.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_common::intent::{IntentParam, IntentValue};

    #[test]
    fn test_extract_intent_params_named() {
        let params = vec![
            IntentParam::Named("max_ripple".to_string(), IntentValue::Number(0.005, Some("V".to_string()))),
            IntentParam::Named("bandwidth".to_string(), IntentValue::Number(100.0, Some("kHz".to_string()))),
        ];

        let map = extract_intent_params(&params);
        assert_eq!(map.get("max_ripple"), Some(&"0.005V".to_string()));
        assert_eq!(map.get("bandwidth"), Some(&"100kHz".to_string()));
    }

    #[test]
    fn test_extract_intent_params_positional() {
        let params = vec![
            IntentParam::Positional(IntentValue::Number(5.0, Some("mV".to_string()))),
        ];

        let map = extract_intent_params(&params);
        assert_eq!(map.get("param_0"), Some(&"5mV".to_string()));
    }

    #[test]
    fn test_format_intent_value() {
        assert_eq!(format_intent_value(&IntentValue::Number(3.3, Some("V".to_string()))), "3.3V");
        assert_eq!(format_intent_value(&IntentValue::Number(42.0, None)), "42");
        assert_eq!(format_intent_value(&IntentValue::Boolean(true)), "true");
        assert_eq!(format_intent_value(&IntentValue::String("hello".to_string())), "hello");
    }

    #[test]
    fn test_stamp_on_empty_netlist() {
        // Verify it doesn't panic with no instances
        use bhdl_analyzer::flow_tracking::FlowTracker;
        use bhdl_common::intent::IntentRegistry;
        let mut netlist = Netlist::default();
        let tracker = FlowTracker::new(IntentRegistry::new());
        stamp_intent_attributes(&mut netlist, &tracker);
        // No assertions needed — just shouldn't panic
    }
}
