//! Fault injection capabilities for SPICE simulations
//! 
//! Enables injecting faults like short circuits, open circuits, and parameter drifts
//! into circuit simulations for reliability and safety testing.

use std::collections::HashMap;
use crate::{Circuit, ComponentModel, SpiceError, Result};
use petgraph::graph::{NodeIndex, EdgeIndex};

/// Types of faults that can be injected in SPICE simulations
#[derive(Debug, Clone)]
pub enum FaultType {
    /// Short circuit - replace component with low resistance
    ShortCircuit {
        resistance: f64, // Typically 0.001 to 0.1 ohms
    },
    
    /// Open circuit - replace component with high resistance
    OpenCircuit {
        resistance: f64, // Typically 1e9 to 1e12 ohms
    },
    
    /// Parameter drift - modify component value
    ParameterDrift {
        scale_factor: f64, // e.g., 1.5 = 50% increase
    },
    
    /// Component failure - replace with failure model
    ComponentFailure {
        failure_model: ComponentModel,
    },
}

/// Fault injection specification
#[derive(Debug, Clone)]
pub struct FaultSpec {
    /// Component name to inject fault into
    pub component_name: String,
    
    /// Type of fault
    pub fault_type: FaultType,
    
    /// Optional description
    pub description: Option<String>,
}

/// Fault injection engine for SPICE simulations
pub struct FaultInjector {
    /// Active faults
    active_faults: Vec<FaultSpec>,
    
    /// Original component models (for restoration)
    original_models: HashMap<String, ComponentModel>,
    
    /// Original branch values (for restoration)
    original_values: HashMap<EdgeIndex, f64>,
}

impl FaultInjector {
    pub fn new() -> Self {
        Self {
            active_faults: Vec::new(),
            original_models: HashMap::new(),
            original_values: HashMap::new(),
        }
    }
    
    /// Add a fault to be injected
    pub fn add_fault(&mut self, fault: FaultSpec) {
        self.active_faults.push(fault);
    }
    
    /// Apply faults to circuit and models
    pub fn apply_faults(
        &mut self,
        circuit: &mut Circuit,
        models: &mut HashMap<String, ComponentModel>
    ) -> Result<()> {
        // Clone to avoid borrow checker issues
        let faults = self.active_faults.clone();
        for fault in &faults {
            self.apply_single_fault(circuit, models, fault)?;
        }
        Ok(())
    }
    
    /// Apply a single fault
    fn apply_single_fault(
        &mut self,
        circuit: &mut Circuit,
        models: &mut HashMap<String, ComponentModel>,
        fault: &FaultSpec
    ) -> Result<()> {
        // Find the component in the circuit
        let edge_idx = circuit.branches()
            .find(|(_, branch)| branch.name == fault.component_name)
            .map(|(idx, _)| idx)
            .ok_or_else(|| SpiceError::ComponentNotFound(fault.component_name.clone()))?;
        
        // Store original value if not already stored
        if !self.original_values.contains_key(&edge_idx) {
            if let Some((_, branch)) = circuit.branches().find(|(idx, _)| *idx == edge_idx) {
                self.original_values.insert(edge_idx, branch.value);
            }
        }
        
        // Store original model if not already stored
        if let Some(model) = models.get(&fault.component_name) {
            self.original_models.entry(fault.component_name.clone())
                .or_insert_with(|| model.clone());
        }
        
        // Apply the fault
        match &fault.fault_type {
            FaultType::ShortCircuit { resistance } => {
                // Replace component with small resistor
                circuit.modify_branch(edge_idx, *resistance, "Resistor".to_string());
                
                // Update model to resistor
                models.insert(
                    fault.component_name.clone(),
                    ComponentModel::Resistor {
                        resistance: *resistance,
                        tolerance: 0.0,
                        limits: Default::default(),
                    }
                );
                
                println!("Injected short circuit fault in {}: R = {:.3}Ω", 
                         fault.component_name, resistance);
            }
            
            FaultType::OpenCircuit { resistance } => {
                // Replace component with large resistor
                circuit.modify_branch(edge_idx, *resistance, "Resistor".to_string());
                
                // Update model to high-resistance resistor
                models.insert(
                    fault.component_name.clone(),
                    ComponentModel::Resistor {
                        resistance: *resistance,
                        tolerance: 0.0,
                        limits: Default::default(),
                    }
                );
                
                println!("Injected open circuit fault in {}: R = {:.2e}Ω", 
                         fault.component_name, resistance);
            }
            
            FaultType::ParameterDrift { scale_factor } => {
                // Get current value
                let original_value = self.original_values.get(&edge_idx).copied().unwrap_or(1.0);
                let new_value = original_value * scale_factor;
                
                // Modify component value
                circuit.modify_branch(edge_idx, new_value, "".to_string()); // Keep existing type
                
                println!("Drifted {} value by {:.0}%: {:.3} -> {:.3}", 
                         fault.component_name, 
                         (scale_factor - 1.0) * 100.0,
                         original_value,
                         new_value);
                
                // Update model parameters
                if let Some(model) = models.get_mut(&fault.component_name) {
                    match model {
                        ComponentModel::Resistor { resistance, .. } => {
                            *resistance *= scale_factor;
                        }
                        ComponentModel::Capacitor { capacitance, .. } => {
                            *capacitance *= scale_factor;
                        }
                        _ => {
                            // Other component types would need specific handling
                        }
                    }
                }
            }
            
            FaultType::ComponentFailure { failure_model } => {
                // Replace with failure model
                models.insert(fault.component_name.clone(), failure_model.clone());
                println!("Injected component failure in {}", fault.component_name);
            }
        }
        
        Ok(())
    }
    
    /// Restore original circuit state
    pub fn restore(
        &mut self,
        circuit: &mut Circuit,
        models: &mut HashMap<String, ComponentModel>
    ) -> Result<()> {
        // Restore branch values
        for (edge_idx, original_value) in &self.original_values {
            // We need to get component type first, then modify
            let original_type = circuit.branches()
                .find(|(idx, _)| *idx == *edge_idx)
                .map(|(_, branch)| branch.component_type.clone())
                .unwrap_or_else(|| "Resistor".to_string());
            
            circuit.modify_branch(*edge_idx, *original_value, original_type);
        }
        
        // Restore models
        for (name, original_model) in &self.original_models {
            models.insert(name.clone(), original_model.clone());
        }
        
        // Clear stored values
        self.original_values.clear();
        self.original_models.clear();
        self.active_faults.clear();
        
        Ok(())
    }
    
    /// Get list of active faults
    pub fn active_faults(&self) -> &[FaultSpec] {
        &self.active_faults
    }
}

/// Helper to detect overcurrent conditions after fault injection
pub fn detect_overcurrent(
    circuit: &Circuit,
    models: &HashMap<String, ComponentModel>,
    threshold_factor: f64 // e.g., 1.5 = 150% of rated current
) -> Vec<(String, f64, f64)> { // (component, current, limit)
    let mut overcurrents = Vec::new();
    
    for (_edge_idx, branch) in circuit.branches() {
        if let Some(current) = branch.current {
            let current_abs = current.abs();
            
            // Get component current limit
            if let Some(model) = models.get(&branch.name) {
                let limit = match model {
                    ComponentModel::Resistor { limits, .. } => limits.max_current,
                    ComponentModel::LED { limits, .. } => limits.max_current,
                    ComponentModel::Capacitor { limits, .. } => limits.max_current,
                    _ => None,
                };
                
                if let Some(max_current) = limit {
                    if current_abs > max_current * threshold_factor {
                        overcurrents.push((
                            branch.name.clone(),
                            current_abs,
                            max_current
                        ));
                    }
                }
            }
        }
    }
    
    overcurrents
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ElectricalLimits;
    
    #[test]
    fn test_short_circuit_fault() {
        let mut circuit = Circuit::new();
        circuit.add_node("in".to_string(), None);
        circuit.add_node("out".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 1000.0, None);
        
        let mut models = HashMap::new();
        models.insert("R1".to_string(), ComponentModel::Resistor {
            resistance: 1000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
        
        let mut injector = FaultInjector::new();
        injector.add_fault(FaultSpec {
            component_name: "R1".to_string(),
            fault_type: FaultType::ShortCircuit { resistance: 0.01 },
            description: Some("R1 short circuit test".to_string()),
        });
        
        // Apply fault
        injector.apply_faults(&mut circuit, &mut models).unwrap();
        
        // Check that R1 is now a 0.01 ohm resistor
        let branch = circuit.branches()
            .find(|(_, b)| b.name == "R1")
            .unwrap().1;
        assert_eq!(branch.value, 0.01);
        
        // Restore
        injector.restore(&mut circuit, &mut models).unwrap();
        
        // Check restoration
        let branch = circuit.branches()
            .find(|(_, b)| b.name == "R1")
            .unwrap().1;
        assert_eq!(branch.value, 1000.0);
    }
}