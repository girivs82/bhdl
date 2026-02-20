//! Circuit loader for initializing simulation state from analysis results

use crate::circuit::state::{CircuitState, CircuitTopology, ConnectionPoint};
use crate::error::SimulationResult;
use bhdl_analyzer::{AnalysisResult, expression_evaluator::RuntimeValue};
use bhdl_netlist::Netlist;
use std::collections::HashMap;

/// Loads circuits from analysis results into simulation state
pub struct CircuitLoader {
    netlist: Netlist,
    analysis_result: AnalysisResult,
}

impl CircuitLoader {
    /// Create a new circuit loader
    pub fn new(netlist: Netlist, analysis_result: AnalysisResult) -> Self {
        Self {
            netlist,
            analysis_result,
        }
    }
    
    /// Load from netlist only (for testing)
    pub fn load_from_netlist(netlist: &Netlist) -> SimulationResult<CircuitState> {
        let topology = Self::create_topology_from_netlist(netlist)?;
        Ok(CircuitState::new(topology))
    }
    
    /// Load the circuit into simulation state
    pub fn load_circuit(&self) -> SimulationResult<CircuitState> {
        // Create topology
        let topology = self.create_topology()?;
        
        // Create initial state
        let mut state = CircuitState::new(topology);
        
        // Initialize attribute values
        self.initialize_attributes(&mut state)?;
        
        // Initialize pin models
        self.initialize_pins(&mut state)?;
        
        // Validate initial state
        self.validate_state(&state)?;
        
        Ok(state)
    }
    
    /// Create circuit topology from netlist
    fn create_topology(&self) -> SimulationResult<CircuitTopology> {
        let mut instance_modules = HashMap::new();
        let mut net_connections = HashMap::new();
        
        // Build instance to module mapping
        for (instance_id, instance) in &self.netlist.instances {
            if let Some(module) = self.netlist.modules.get(instance.definition) {
                instance_modules.insert(instance_id, module.name.clone());
            }
        }
        
        // Build net connectivity
        for (net_id, net) in &self.netlist.nets {
            let connections = net.connections.iter().filter_map(|conn| {
                match conn {
                    bhdl_netlist::ConnectionPoint::InstancePort(inst_id, _) => {
                        Some(ConnectionPoint {
                            instance: *inst_id,
                            pin: String::new(), // TODO: Get actual pin name
                        })
                    }
                    _ => None
                }
            }).collect();
            
            net_connections.insert(net_id, connections);
        }
        
        Ok(CircuitTopology {
            instance_modules,
            net_connections,
        })
    }
    
    /// Initialize attribute values from analysis
    fn initialize_attributes(&self, state: &mut CircuitState) -> SimulationResult<()> {
        // Extract initial attribute values from analysis result
        for (_ptr, value) in &self.analysis_result.resolved_constants {
            // TODO: Map pointer to attribute name
            // For now, just convert the value
            let _runtime_value = match value.as_i64() {
                Some(i) => RuntimeValue::Integer(i),
                None => match value.as_f64() {
                    Some(f) => RuntimeValue::Real(f),
                    None => continue,
                },
            };
            // TODO: state.update_attribute(name, runtime_value);
        }
        
        // Initialize behavioral attributes from attribute analysis
        for (name, info) in &self.analysis_result.attribute_analysis.attributes {
            // Skip if already initialized from constants
            if state.get_attribute(name).is_none() {
                // For expression attributes, we'll evaluate them during simulation
                // For now, initialize with a default value
                match &info.attribute_type {
                    bhdl_ast::attributes::AttributeType::Static(val) => {
                        // Parse the static value
                        if let Ok(parsed) = val.parse::<f64>() {
                            state.update_attribute(name, RuntimeValue::Real(parsed));
                        } else {
                            state.update_attribute(name, RuntimeValue::String(val.clone()));
                        }
                    }
                    _ => {
                        // Expression or mutable attributes start at 0
                        state.update_attribute(name, RuntimeValue::Real(0.0));
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Initialize pin models
    fn initialize_pins(&self, state: &mut CircuitState) -> SimulationResult<()> {
        // Initialize all pins to default values
        for (instance_id, _instance) in &self.netlist.instances {
            // TODO: Get pin information from module definition
            // For now, just initialize common pins
            for pin_name in ["1", "2", "IN", "OUT", "VCC", "GND"].iter() {
                let pin_path = format!("{:?}.{}", instance_id, pin_name);
                state.update_pin(&pin_path, Default::default());
            }
        }
        
        Ok(())
    }
    
    /// Validate the initial state
    fn validate_state(&self, _state: &CircuitState) -> SimulationResult<()> {
        // Check that all required attributes are initialized
        // This is a placeholder - real validation would check more thoroughly
        
        Ok(())
    }
    
    /// Create topology from netlist only
    fn create_topology_from_netlist(netlist: &Netlist) -> SimulationResult<CircuitTopology> {
        let mut instance_modules = HashMap::new();
        let mut net_connections = HashMap::new();
        
        // Build instance to module mapping
        for (instance_id, instance) in &netlist.instances {
            if let Some(module) = netlist.modules.get(instance.definition) {
                instance_modules.insert(instance_id, module.name.clone());
            }
        }
        
        // Build net connectivity
        for (net_id, net) in &netlist.nets {
            let connections = net.connections.iter().filter_map(|conn| {
                match conn {
                    bhdl_netlist::ConnectionPoint::InstancePort(inst_id, _) => {
                        Some(ConnectionPoint {
                            instance: *inst_id,
                            pin: String::new(), // TODO: Get actual pin name
                        })
                    }
                    _ => None
                }
            }).collect();
            
            net_connections.insert(net_id, connections);
        }
        
        Ok(CircuitTopology {
            instance_modules,
            net_connections,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bhdl_netlist::ModuleKind;
    
    #[test]
    fn test_basic_loading() {
        // Create a simple netlist
        let mut netlist = Netlist::new();
        let _module_id = netlist.add_module(
            "test".to_string(),
            ModuleKind::Module
        );
        
        // Create empty analysis result
        let analysis_result = AnalysisResult::default();
        
        // Load circuit
        let loader = CircuitLoader::new(netlist, analysis_result);
        let state = loader.load_circuit().unwrap();
        
        // Basic checks
        assert_eq!(state.changed_attributes().len(), 0);
        assert_eq!(state.changed_pins().len(), 0);
    }
}