//! Integration between BHDL analyzer and SPICE electrical analysis

use std::collections::HashMap;
use bhdl_ast::{SourceFile, ComponentInst};
use bhdl_netlist::{Netlist, NetId, InstanceId};
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, NonlinearDcAnalysis, ComponentInference as SpiceInference};
use crate::component_inference::{ComponentSuggestion, InferredParameter, ParameterValue};
use crate::types::AnalysisResult;
use crate::component_library::ModuleResolver;

/// Convert BHDL netlist to SPICE circuit
pub fn netlist_to_spice_circuit(netlist: &Netlist) -> Result<Circuit, String> {
    let mut circuit = Circuit::new();
    
    // Add all nets as nodes
    for (net_id, net) in &netlist.nets {
        let name = net.name.clone().unwrap_or_else(|| format!("net_{:?}", net_id));
        circuit.add_node(name, Some(net_id));
    }
    
    // Add components as branches
    for (instance_id, instance) in &netlist.instances {
        if let Some(module) = netlist.modules.get(instance.definition) {
            // Find nets connected to this instance
            let mut connected_nets = Vec::new();
            for (net_id, net) in &netlist.nets {
                for conn_point in &net.connections {
                    use bhdl_netlist::ConnectionPoint;
                    match conn_point {
                        ConnectionPoint::InstancePort(inst_id, _) |
                        ConnectionPoint::InstancePin(inst_id, _) => {
                            if *inst_id == instance_id {
                                connected_nets.push(net_id);
                                break;
                            }
                        }
                        _ => {}
                    }
                }
            }
            
            // For 2-pin components, create branch
            if connected_nets.len() >= 2 {
                let node1 = netlist.nets.get(connected_nets[0])
                    .and_then(|n| n.name.clone())
                    .unwrap_or_else(|| format!("net_{:?}", connected_nets[0]));
                let node2 = netlist.nets.get(connected_nets[1])
                    .and_then(|n| n.name.clone())
                    .unwrap_or_else(|| format!("net_{:?}", connected_nets[1]));
                
                circuit.add_branch(
                    instance.name.clone(),
                    &node1,
                    &node2,
                    module.name.clone(),
                    1.0, // Default value - will be overridden by model
                    Some(instance_id),
                );
            }
        }
    }
    
    Ok(circuit)
}

/// Extract SPICE models from component library
pub fn extract_spice_models(
    netlist: &Netlist,
    module_resolver: &mut ModuleResolver,
) -> HashMap<String, ComponentModel> {
    let mut models = HashMap::new();
    
    for (_instance_id, instance) in &netlist.instances {
        if let Some(module_def) = netlist.modules.get(instance.definition) {
            // Try to resolve the module from the library
            if let Ok(resolved_module) = module_resolver.resolve(&module_def.name) {
                // Extract SPICE model from resolved module attributes
                if let Some(model) = extract_spice_model_from_resolved_module(&resolved_module, &module_def.name) {
                    models.insert(instance.name.clone(), model);
                } else if let Some(model) = create_default_model(&module_def.name) {
                    // Fallback to default if no SPICE attributes found
                    models.insert(instance.name.clone(), model);
                }
            } else {
                // Fallback to basic models based on component type
                if let Some(model) = create_default_model(&module_def.name) {
                    models.insert(instance.name.clone(), model);
                }
            }
        }
    }
    
    models
}

/// Extract SPICE model from resolved module's electrical specs
fn extract_spice_model_from_resolved_module(
    module: &crate::component_library::ComponentModule,
    component_type: &str,
) -> Option<ComponentModel> {
    use crate::spice_extraction::extract_spice_model_from_params;
    
    // Convert electrical specs to params format expected by spice_extraction
    let mut params = HashMap::new();
    for (key, value) in &module.metadata.electrical_specs {
        params.insert(key.clone(), value.clone());
    }
    
    extract_spice_model_from_params(component_type, &params)
}


/// Create default SPICE model based on component type
fn create_default_model(component_type: &str) -> Option<ComponentModel> {
    match component_type {
        "Res" | "Resistor" => Some(ComponentModel::Resistor {
            resistance: 1000.0, // 1kΩ default
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        }),
        "Cap" | "Capacitor" => Some(ComponentModel::Capacitor {
            capacitance: 1e-6, // 1µF default
            esr: Some(0.1),
            limits: ElectricalLimits::default(),
        }),
        "LED" => Some(ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.020,
            dynamic_resistance: 10.0,
            limits: ElectricalLimits {
                max_current: Some(0.030),
                max_power: Some(0.100),
                ..Default::default()
            },
        }),
        _ => None,
    }
}

/// Run SPICE-based component inference
pub fn run_spice_inference(
    netlist: &Netlist,
    module_resolver: &mut ModuleResolver,
) -> Result<Vec<ComponentSuggestion>, String> {
    // Convert netlist to SPICE circuit
    let circuit = netlist_to_spice_circuit(netlist)?;
    
    // Extract SPICE models
    let models = extract_spice_models(netlist, module_resolver);
    
    // Create inference engine
    let mut inference = SpiceInference::new(circuit);
    
    // Add models
    for (name, model) in models {
        inference.add_model(name, model);
    }
    
    // Run inference
    let inferred = inference.infer()
        .map_err(|e| format!("SPICE inference failed: {}", e))?;
    
    // Convert to ComponentSuggestion format
    let suggestions: Vec<ComponentSuggestion> = inferred.into_iter()
        .map(|comp| {
            let mut parameters = Vec::new();
            
            // Extract value parameter based on component type
            let param_value = match comp.component_type.as_str() {
                "Resistor" => ParameterValue::Resistance(comp.value),
                "Capacitor" => ParameterValue::Capacitance(comp.value),
                "Inductor" => ParameterValue::Inductance(comp.value),
                _ => ParameterValue::Real(comp.value),
            };
            
            parameters.push(InferredParameter {
                name: "value".to_string(),
                value: param_value,
                confidence: comp.confidence,
                reasoning: comp.reason.clone(),
            });
            
            ComponentSuggestion {
                component_type: comp.component_type,
                instance_name: Some(comp.name),
                part_number: None, // TODO: Map to specific part numbers
                parameters,
                reasoning: comp.reason,
                confidence: comp.confidence,
                alternatives: vec![],
            }
        })
        .collect();
    
    Ok(suggestions)
}

/// Check if SPICE inference should be used for a component
pub fn should_use_spice_inference(component_type: &str) -> bool {
    match component_type {
        "Res" | "Resistor" | "Cap" | "Capacitor" | "LED" | "Diode" => true,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_netlist_to_spice_conversion() {
        // TODO: Add tests
    }
}