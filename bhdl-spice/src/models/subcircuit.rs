//! SPICE Subcircuit Model
//! 
//! Subcircuits allow hierarchical circuit definitions where complex components
//! are represented as a collection of simpler SPICE elements.

use std::collections::HashMap;
use crate::{
    SpiceModel, ModelType, SpiceError,
    Circuit, NodeId,
};

/// Pin mapping for subcircuit connections
#[derive(Debug, Clone)]
pub struct SubcircuitPin {
    /// External pin name (as seen from outside)
    pub external_name: String,
    /// Internal node name (inside the subcircuit)
    pub internal_node: String,
    /// Pin type (input, output, power, ground, etc.)
    pub pin_type: String,
}

/// SPICE subcircuit definition
#[derive(Clone)]
pub struct SubcircuitDefinition {
    /// Subcircuit name (e.g., "LM741", "555_TIMER")
    pub name: String,
    /// Pin definitions
    pub pins: Vec<SubcircuitPin>,
    /// Internal circuit representation
    pub internal_circuit: Circuit,
    /// Parameters that can be overridden
    pub parameters: HashMap<String, f64>,
    /// Default parameter values
    pub defaults: HashMap<String, f64>,
}

/// Subcircuit instance model
#[derive(Clone)]
pub struct SubcircuitModel {
    /// Instance name (e.g., "U1")
    pub name: String,
    /// Reference to subcircuit definition
    pub definition: SubcircuitDefinition,
    /// Instance-specific parameter overrides
    pub overrides: HashMap<String, f64>,
    /// Pin connections (external pin -> connected node)
    pub connections: HashMap<String, NodeId>,
}

impl SubcircuitModel {
    /// Create new subcircuit instance
    pub fn new(name: String, definition: SubcircuitDefinition) -> Self {
        Self {
            name,
            definition,
            overrides: HashMap::new(),
            connections: HashMap::new(),
        }
    }
    
    /// Set parameter override
    pub fn set_parameter(&mut self, param: &str, value: f64) {
        self.overrides.insert(param.to_string(), value);
    }
    
    /// Get effective parameter value (override or default)
    pub fn get_parameter(&self, param: &str) -> Option<f64> {
        self.overrides.get(param)
            .or_else(|| self.definition.parameters.get(param))
            .or_else(|| self.definition.defaults.get(param))
            .copied()
    }
    
    /// Connect external pin to circuit node
    pub fn connect_pin(&mut self, pin_name: &str, node_id: NodeId) -> crate::Result<()> {
        // Verify pin exists
        if !self.definition.pins.iter().any(|p| p.external_name == pin_name) {
            return Err(SpiceError::Other(anyhow::anyhow!(
                "Pin '{}' not found in subcircuit '{}'", pin_name, self.definition.name
            )));
        }
        
        self.connections.insert(pin_name.to_string(), node_id);
        Ok(())
    }
    
    /// Get external pins
    pub fn pins(&self) -> Vec<(String, String)> {
        self.definition.pins.iter()
            .map(|p| (p.external_name.clone(), p.pin_type.clone()))
            .collect()
    }
    
    /// Expand subcircuit into the parent circuit
    pub fn expand_into_circuit(&self, parent_circuit: &mut Circuit) -> crate::Result<()> {
        // Map from internal node names to parent circuit node indices
        let mut node_mapping: HashMap<String, NodeId> = HashMap::new();
        
        // First, map external pins to their connected nodes
        for pin in &self.definition.pins {
            if let Some(&parent_node) = self.connections.get(&pin.external_name) {
                node_mapping.insert(pin.internal_node.clone(), parent_node);
            } else {
                return Err(SpiceError::Other(anyhow::anyhow!(
                    "Pin '{}' of subcircuit '{}' is not connected", 
                    pin.external_name, self.name
                )));
            }
        }
        
        // Create internal nodes in parent circuit
        for (node_idx, node) in self.definition.internal_circuit.nodes() {
            // Skip nodes that are mapped to external pins
            if !node_mapping.values().any(|&n| n == node_idx) {
                let internal_name = format!("{}:{}", self.name, node.name);
                let new_idx = parent_circuit.add_node(internal_name, None);
                node_mapping.insert(node.name.clone(), new_idx);
            }
        }
        
        // Copy components with parameter substitution
        for (comp_idx, component) in self.definition.internal_circuit.branches() {
            let (n1, n2) = self.definition.internal_circuit.branch_nodes(comp_idx)
                .ok_or_else(|| SpiceError::Other(anyhow::anyhow!("Invalid branch nodes")))?;
            
            let node1 = self.definition.internal_circuit.get_node_by_id(n1)
                .ok_or_else(|| SpiceError::Other(anyhow::anyhow!("Node not found")))?;
            let node2 = self.definition.internal_circuit.get_node_by_id(n2)
                .ok_or_else(|| SpiceError::Other(anyhow::anyhow!("Node not found")))?;
            
            // Map nodes
            let mapped_n1 = node_mapping.get(&node1.name)
                .ok_or_else(|| SpiceError::Other(anyhow::anyhow!(
                    "Node mapping not found for '{}'", node1.name
                )))?;
            let mapped_n2 = node_mapping.get(&node2.name)
                .ok_or_else(|| SpiceError::Other(anyhow::anyhow!(
                    "Node mapping not found for '{}'", node2.name
                )))?;
            
            // Apply parameter substitution to component value
            let value = component.value;
            
            // Check if value references a parameter
            // (In a full implementation, this would parse expressions)
            // For now, just use the raw value
            
            // Add component to parent circuit
            let comp_name = format!("{}:{}", self.name, component.name);
            
            // Get node names before calling add_branch to avoid borrow checker issues
            let node1_name = parent_circuit.get_node_by_id(*mapped_n1).unwrap().name.clone();
            let node2_name = parent_circuit.get_node_by_id(*mapped_n2).unwrap().name.clone();
            
            parent_circuit.add_branch(
                comp_name,
                &node1_name,
                &node2_name,
                component.component_type.clone(),
                value,
                None,
            );
        }
        
        Ok(())
    }
}

impl SpiceModel for SubcircuitModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::Subcircuit
    }
    
    fn current(&self, _voltages: &[f64], _temp: f64) -> f64 {
        // Subcircuits are expanded, not evaluated directly
        0.0
    }
    
    fn conductance(&self, _voltages: &[f64], _temp: f64) -> Vec<f64> {
        // Subcircuits are expanded, not evaluated directly
        vec![]
    }
    
    fn num_terminals(&self) -> usize {
        self.definition.pins.len()
    }
    
    fn is_nonlinear(&self) -> bool {
        // Depends on internal components
        true
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = self.definition.defaults.clone();
        // Apply overrides
        for (key, value) in &self.overrides {
            params.insert(key.clone(), *value);
        }
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        self.overrides.insert(name.to_string(), value);
        Ok(())
    }
}

/// Subcircuit library for storing definitions
#[derive(Default)]
pub struct SubcircuitLibrary {
    /// Stored subcircuit definitions
    definitions: HashMap<String, SubcircuitDefinition>,
}

impl SubcircuitLibrary {
    /// Create new library
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Add a subcircuit definition
    pub fn add_definition(&mut self, definition: SubcircuitDefinition) {
        self.definitions.insert(definition.name.clone(), definition);
    }
    
    /// Get a subcircuit definition
    pub fn get_definition(&self, name: &str) -> Option<&SubcircuitDefinition> {
        self.definitions.get(name)
    }
    
    /// Create an instance of a subcircuit
    pub fn instantiate(&self, instance_name: &str, definition_name: &str) -> Result<SubcircuitModel, SpiceError> {
        let definition = self.get_definition(definition_name)
            .ok_or_else(|| SpiceError::Other(anyhow::anyhow!(
                "Subcircuit definition '{}' not found", definition_name
            )))?;
        
        Ok(SubcircuitModel::new(instance_name.to_string(), definition.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_subcircuit_creation() {
        // Create a simple voltage divider subcircuit
        let mut internal = Circuit::new();
        let in_node = internal.add_node("IN".to_string(), None);
        let out_node = internal.add_node("OUT".to_string(), None);
        let gnd_node = internal.add_node("GND".to_string(), None);
        
        internal.add_branch(
            "R1".to_string(),
            "IN",
            "OUT",
            "Resistor".to_string(),
            10e3,
            None,
        );
        internal.add_branch(
            "R2".to_string(),
            "OUT",
            "GND",
            "Resistor".to_string(),
            10e3,
            None,
        );
        
        let pins = vec![
            SubcircuitPin {
                external_name: "VIN".to_string(),
                internal_node: "IN".to_string(),
                pin_type: "input".to_string(),
            },
            SubcircuitPin {
                external_name: "VOUT".to_string(),
                internal_node: "OUT".to_string(),
                pin_type: "output".to_string(),
            },
            SubcircuitPin {
                external_name: "GND".to_string(),
                internal_node: "GND".to_string(),
                pin_type: "ground".to_string(),
            },
        ];
        
        let def = SubcircuitDefinition {
            name: "VDIV".to_string(),
            pins,
            internal_circuit: internal,
            parameters: HashMap::new(),
            defaults: HashMap::new(),
        };
        
        let mut library = SubcircuitLibrary::new();
        library.add_definition(def);
        
        let instance = library.instantiate("U1", "VDIV").unwrap();
        assert_eq!(instance.name, "U1");
        assert_eq!(instance.definition.name, "VDIV");
    }
}