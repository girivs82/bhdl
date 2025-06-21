//! Base behavioral model interface

use crate::circuit::state::{PinValue, LogicLevel};
use crate::error::{SimulationResult, SimulationError};
use bhdl_netlist::InstanceId;
use std::collections::HashMap;

/// Behavioral model for a component
pub trait BehavioralModel: Send + Sync {
    /// Get model name
    fn name(&self) -> &str;
    
    /// Get model type
    fn model_type(&self) -> ModelType;
    
    /// Get ports
    fn ports(&self) -> &[ModelPort];
    
    /// Initialize the model
    fn initialize(&mut self, parameters: &HashMap<String, f64>) -> SimulationResult<()>;
    
    /// Update model state based on input pin values
    fn update(
        &mut self,
        inputs: &HashMap<String, PinValue>,
        time: f64,
        dt: f64,
    ) -> SimulationResult<HashMap<String, PinValue>>;
    
    /// Get internal state for debugging
    fn get_state(&self) -> HashMap<String, f64>;
    
    /// Reset model to initial state
    fn reset(&mut self);
}

/// Type of behavioral model
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelType {
    Analog,
    Digital,
    MixedSignal,
}

/// Port definition for a model
#[derive(Debug, Clone)]
pub struct ModelPort {
    pub name: String,
    pub direction: PortDirection,
    pub port_type: PortType,
}

/// Port direction
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortDirection {
    Input,
    Output,
    Bidirectional,
}

/// Port type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortType {
    Analog,
    Digital,
    Power,
    Ground,
}

/// Base implementation helper for behavioral models
pub struct ModelBase {
    pub name: String,
    pub model_type: ModelType,
    pub ports: Vec<ModelPort>,
    pub parameters: HashMap<String, f64>,
}

impl ModelBase {
    /// Create a new model base
    pub fn new(name: String, model_type: ModelType) -> Self {
        Self {
            name,
            model_type,
            ports: Vec::new(),
            parameters: HashMap::new(),
        }
    }
    
    /// Add a port
    pub fn add_port(&mut self, port: ModelPort) {
        self.ports.push(port);
    }
    
    /// Set parameter
    pub fn set_parameter(&mut self, name: String, value: f64) {
        self.parameters.insert(name, value);
    }
    
    /// Get parameter
    pub fn get_parameter(&self, name: &str) -> Option<f64> {
        self.parameters.get(name).copied()
    }
}

/// Registry of behavioral models
pub struct ModelRegistry {
    models: HashMap<InstanceId, Box<dyn BehavioralModel>>,
}

impl ModelRegistry {
    /// Create a new model registry
    pub fn new() -> Self {
        Self {
            models: HashMap::new(),
        }
    }
    
    /// Register a model for an instance
    pub fn register(&mut self, instance: InstanceId, model: Box<dyn BehavioralModel>) {
        self.models.insert(instance, model);
    }
    
    /// Get model for an instance
    pub fn get(&self, instance: &InstanceId) -> Option<&dyn BehavioralModel> {
        self.models.get(instance).map(|b| b.as_ref())
    }
    
    /// Get mutable model for an instance
    pub fn get_mut(&mut self, instance: &InstanceId) -> Option<&mut (dyn BehavioralModel + 'static)> {
        self.models.get_mut(instance).map(|b| b.as_mut())
    }
    
    /// Update all models
    pub fn update_all(
        &mut self,
        instance_inputs: &HashMap<InstanceId, HashMap<String, PinValue>>,
        time: f64,
        dt: f64,
    ) -> SimulationResult<HashMap<InstanceId, HashMap<String, PinValue>>> {
        let mut all_outputs = HashMap::new();
        
        for (instance_id, model) in &mut self.models {
            if let Some(inputs) = instance_inputs.get(instance_id) {
                let outputs = model.update(inputs, time, dt)?;
                all_outputs.insert(*instance_id, outputs);
            }
        }
        
        Ok(all_outputs)
    }
    
    /// Reset all models
    pub fn reset_all(&mut self) {
        for model in self.models.values_mut() {
            model.reset();
        }
    }
}

impl Default for ModelRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Test model implementation
    struct TestModel {
        base: ModelBase,
        state: f64,
    }
    
    impl TestModel {
        fn new() -> Self {
            let mut base = ModelBase::new("TestModel".to_string(), ModelType::Analog);
            base.add_port(ModelPort {
                name: "IN".to_string(),
                direction: PortDirection::Input,
                port_type: PortType::Analog,
            });
            base.add_port(ModelPort {
                name: "OUT".to_string(),
                direction: PortDirection::Output,
                port_type: PortType::Analog,
            });
            
            Self {
                base,
                state: 0.0,
            }
        }
    }
    
    impl BehavioralModel for TestModel {
        fn name(&self) -> &str {
            &self.base.name
        }
        
        fn model_type(&self) -> ModelType {
            self.base.model_type
        }
        
        fn ports(&self) -> &[ModelPort] {
            &self.base.ports
        }
        
        fn initialize(&mut self, parameters: &HashMap<String, f64>) -> SimulationResult<()> {
            self.base.parameters = parameters.clone();
            Ok(())
        }
        
        fn update(
            &mut self,
            inputs: &HashMap<String, PinValue>,
            _time: f64,
            dt: f64,
        ) -> SimulationResult<HashMap<String, PinValue>> {
            let mut outputs = HashMap::new();
            
            if let Some(input) = inputs.get("IN") {
                // Simple integration
                self.state += input.voltage * dt;
                
                outputs.insert("OUT".to_string(), PinValue {
                    voltage: self.state,
                    current: 0.0,
                    impedance: 1e6,
                    drive_strength: crate::circuit::state::DriveStrength::None,
                    logic_level: None,
                });
            }
            
            Ok(outputs)
        }
        
        fn get_state(&self) -> HashMap<String, f64> {
            let mut state = HashMap::new();
            state.insert("state".to_string(), self.state);
            state
        }
        
        fn reset(&mut self) {
            self.state = 0.0;
        }
    }
    
    #[test]
    fn test_model_registry() {
        use bhdl_netlist::{Netlist, ModuleKind};
        
        let mut registry = ModelRegistry::new();
        
        // Create instances using netlist
        let mut netlist = Netlist::new();
        let module_id = netlist.add_module("TestModule".to_string(), ModuleKind::Module);
        let instance1 = netlist.add_instance("inst1".to_string(), module_id).unwrap();
        let instance2 = netlist.add_instance("inst2".to_string(), module_id).unwrap();
        
        // Register models
        registry.register(instance1, Box::new(TestModel::new()));
        registry.register(instance2, Box::new(TestModel::new()));
        
        // Test retrieval
        assert!(registry.get(&instance1).is_some());
        assert!(registry.get(&instance2).is_some());
        
        // Test update
        let mut inputs = HashMap::new();
        let mut instance1_inputs = HashMap::new();
        instance1_inputs.insert("IN".to_string(), PinValue {
            voltage: 1.0,
            current: 0.0,
            impedance: 50.0,
            drive_strength: crate::circuit::state::DriveStrength::None,
            logic_level: None,
        });
        inputs.insert(instance1, instance1_inputs);
        
        let outputs = registry.update_all(&inputs, 0.0, 0.001).unwrap();
        assert!(outputs.contains_key(&instance1));
        
        let instance1_outputs = &outputs[&instance1];
        assert!(instance1_outputs.contains_key("OUT"));
        assert!((instance1_outputs["OUT"].voltage - 0.001).abs() < 1e-6);
    }
}