//! Library of standard component models

use crate::behavioral::component_model::{BehavioralModel, ModelPort, PortDirection, PortType};
use crate::behavioral::analog_model::{
    AnalogModel, ResistorBehavior, CapacitorBehavior, InductorBehavior, VoltageSourceBehavior
};
use crate::behavioral::digital_model::{
    DigitalModel, NotGateBehavior, AndGateBehavior, DFlipFlopBehavior
};
use crate::behavioral::mixed_signal::{AdcModel, DacModel};
use crate::error::{SimulationResult, SimulationError};
use std::collections::HashMap;

/// Factory for creating behavioral models
pub struct ModelFactory {
    creators: HashMap<String, Box<dyn Fn(&HashMap<String, f64>) -> SimulationResult<Box<dyn BehavioralModel>> + Send + Sync>>,
}

impl ModelFactory {
    /// Create a new model factory with standard models
    pub fn new() -> Self {
        let mut factory = Self {
            creators: HashMap::new(),
        };
        
        // Register standard models
        factory.register_standard_models();
        
        factory
    }
    
    /// Register a custom model creator
    pub fn register<F>(&mut self, model_type: &str, creator: F)
    where
        F: Fn(&HashMap<String, f64>) -> SimulationResult<Box<dyn BehavioralModel>> + Send + Sync + 'static,
    {
        self.creators.insert(model_type.to_string(), Box::new(creator));
    }
    
    /// Create a model instance
    pub fn create(&self, model_type: &str, parameters: &HashMap<String, f64>) -> SimulationResult<Box<dyn BehavioralModel>> {
        if let Some(creator) = self.creators.get(model_type) {
            creator(parameters)
        } else {
            Err(SimulationError::ConfigError(format!("Unknown model type: {}", model_type)))
        }
    }
    
    /// Register standard analog models
    fn register_standard_models(&mut self) {
        // Resistor
        self.register("Resistor", |params| {
            let resistance = params.get("resistance").copied().unwrap_or(1000.0);
            let mut model = AnalogModel::new("Resistor".to_string(), ResistorBehavior::new(resistance));
            
            model.base.add_port(ModelPort {
                name: "1".to_string(),
                direction: PortDirection::Bidirectional,
                port_type: PortType::Analog,
            });
            model.base.add_port(ModelPort {
                name: "2".to_string(),
                direction: PortDirection::Bidirectional,
                port_type: PortType::Analog,
            });
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // Capacitor
        self.register("Capacitor", |params| {
            let capacitance = params.get("capacitance").copied().unwrap_or(1e-6);
            let mut model = AnalogModel::new("Capacitor".to_string(), CapacitorBehavior::new(capacitance));
            
            model.base.add_port(ModelPort {
                name: "1".to_string(),
                direction: PortDirection::Bidirectional,
                port_type: PortType::Analog,
            });
            model.base.add_port(ModelPort {
                name: "2".to_string(),
                direction: PortDirection::Bidirectional,
                port_type: PortType::Analog,
            });
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // Inductor
        self.register("Inductor", |params| {
            let inductance = params.get("inductance").copied().unwrap_or(1e-3);
            let mut model = AnalogModel::new("Inductor".to_string(), InductorBehavior::new(inductance));
            
            model.base.add_port(ModelPort {
                name: "1".to_string(),
                direction: PortDirection::Bidirectional,
                port_type: PortType::Analog,
            });
            model.base.add_port(ModelPort {
                name: "2".to_string(),
                direction: PortDirection::Bidirectional,
                port_type: PortType::Analog,
            });
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // Voltage Source
        self.register("VoltageSource", |params| {
            let voltage = params.get("voltage").copied().unwrap_or(5.0);
            let resistance = params.get("internal_resistance").copied().unwrap_or(0.1);
            let mut model = AnalogModel::new(
                "VoltageSource".to_string(),
                VoltageSourceBehavior::new(voltage, resistance)
            );
            
            model.base.add_port(ModelPort {
                name: "+".to_string(),
                direction: PortDirection::Output,
                port_type: PortType::Power,
            });
            model.base.add_port(ModelPort {
                name: "-".to_string(),
                direction: PortDirection::Output,
                port_type: PortType::Ground,
            });
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // NOT Gate
        self.register("NOT", |params| {
            let prop_delay = params.get("propagation_delay").copied().unwrap_or(1e-9);
            let mut model = DigitalModel::new("NOT".to_string(), NotGateBehavior::new(prop_delay));
            
            model.base.add_port(ModelPort {
                name: "A".to_string(),
                direction: PortDirection::Input,
                port_type: PortType::Digital,
            });
            model.base.add_port(ModelPort {
                name: "Y".to_string(),
                direction: PortDirection::Output,
                port_type: PortType::Digital,
            });
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // AND Gate
        self.register("AND", |params| {
            let num_inputs = params.get("num_inputs").copied().unwrap_or(2.0) as usize;
            let prop_delay = params.get("propagation_delay").copied().unwrap_or(1e-9);
            let mut model = DigitalModel::new("AND".to_string(), AndGateBehavior::new(num_inputs, prop_delay));
            
            for i in 0..num_inputs {
                model.base.add_port(ModelPort {
                    name: format!("A{}", i),
                    direction: PortDirection::Input,
                    port_type: PortType::Digital,
                });
            }
            
            model.base.add_port(ModelPort {
                name: "Y".to_string(),
                direction: PortDirection::Output,
                port_type: PortType::Digital,
            });
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // D Flip-Flop
        self.register("DFF", |params| {
            let setup_time = params.get("setup_time").copied().unwrap_or(1e-9);
            let hold_time = params.get("hold_time").copied().unwrap_or(0.5e-9);
            let clk_to_q = params.get("clk_to_q_delay").copied().unwrap_or(2e-9);
            let mut model = DigitalModel::new(
                "DFF".to_string(),
                DFlipFlopBehavior::new(setup_time, hold_time, clk_to_q)
            );
            
            model.base.add_port(ModelPort {
                name: "D".to_string(),
                direction: PortDirection::Input,
                port_type: PortType::Digital,
            });
            model.base.add_port(ModelPort {
                name: "CLK".to_string(),
                direction: PortDirection::Input,
                port_type: PortType::Digital,
            });
            model.base.add_port(ModelPort {
                name: "Q".to_string(),
                direction: PortDirection::Output,
                port_type: PortType::Digital,
            });
            model.base.add_port(ModelPort {
                name: "Q_BAR".to_string(),
                direction: PortDirection::Output,
                port_type: PortType::Digital,
            });
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // ADC
        self.register("ADC", |params| {
            let resolution = params.get("resolution").copied().unwrap_or(8.0) as u32;
            let vref_high = params.get("vref_high").copied().unwrap_or(5.0);
            let vref_low = params.get("vref_low").copied().unwrap_or(0.0);
            let conversion_time = params.get("conversion_time").copied().unwrap_or(1e-6);
            
            let model = AdcModel::new(
                "ADC".to_string(),
                resolution,
                vref_high,
                vref_low,
                conversion_time
            );
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
        
        // DAC
        self.register("DAC", |params| {
            let resolution = params.get("resolution").copied().unwrap_or(8.0) as u32;
            let vref_high = params.get("vref_high").copied().unwrap_or(5.0);
            let vref_low = params.get("vref_low").copied().unwrap_or(0.0);
            let settling_time = params.get("settling_time").copied().unwrap_or(1e-6);
            
            let model = DacModel::new(
                "DAC".to_string(),
                resolution,
                vref_high,
                vref_low,
                settling_time
            );
            
            Ok(Box::new(model) as Box<dyn BehavioralModel>)
        });
    }
}

impl Default for ModelFactory {
    fn default() -> Self {
        Self::new()
    }
}

/// Library of behavioral models
pub struct ModelLibrary {
    factory: ModelFactory,
    model_parameters: HashMap<String, HashMap<String, f64>>,
}

impl ModelLibrary {
    /// Create a new model library
    pub fn new() -> Self {
        Self {
            factory: ModelFactory::new(),
            model_parameters: HashMap::new(),
        }
    }
    
    /// Load standard component parameters
    pub fn load_standard_parameters(&mut self) {
        // Standard resistor values
        self.add_model_parameters("R_1k", {
            let mut params = HashMap::new();
            params.insert("resistance".to_string(), 1000.0);
            params
        });
        
        self.add_model_parameters("R_10k", {
            let mut params = HashMap::new();
            params.insert("resistance".to_string(), 10000.0);
            params
        });
        
        // Standard capacitor values
        self.add_model_parameters("C_100n", {
            let mut params = HashMap::new();
            params.insert("capacitance".to_string(), 100e-9);
            params
        });
        
        self.add_model_parameters("C_10u", {
            let mut params = HashMap::new();
            params.insert("capacitance".to_string(), 10e-6);
            params
        });
        
        // Standard logic gates
        self.add_model_parameters("74HC04", {
            let mut params = HashMap::new();
            params.insert("propagation_delay".to_string(), 10e-9);
            params
        });
        
        self.add_model_parameters("74HC08", {
            let mut params = HashMap::new();
            params.insert("num_inputs".to_string(), 2.0);
            params.insert("propagation_delay".to_string(), 10e-9);
            params
        });
    }
    
    /// Add model parameters
    pub fn add_model_parameters(&mut self, name: &str, parameters: HashMap<String, f64>) {
        self.model_parameters.insert(name.to_string(), parameters);
    }
    
    /// Create a model by name
    pub fn create_model(&self, model_name: &str, model_type: &str) -> SimulationResult<Box<dyn BehavioralModel>> {
        let params = self.model_parameters.get(model_name)
            .ok_or_else(|| SimulationError::ConfigError(format!("Unknown model: {}", model_name)))?;
        
        self.factory.create(model_type, params)
    }
    
    /// Get the factory for custom model registration
    pub fn factory_mut(&mut self) -> &mut ModelFactory {
        &mut self.factory
    }
}

impl Default for ModelLibrary {
    fn default() -> Self {
        let mut library = Self::new();
        library.load_standard_parameters();
        library
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_model_factory() {
        let factory = ModelFactory::new();
        
        // Test resistor creation
        let mut params = HashMap::new();
        params.insert("resistance".to_string(), 1000.0);
        
        let model = factory.create("Resistor", &params).unwrap();
        assert_eq!(model.name(), "Resistor");
        assert_eq!(model.ports().len(), 2);
    }
    
    #[test]
    fn test_model_library() {
        let library = ModelLibrary::default();
        
        // Test standard resistor
        let model = library.create_model("R_1k", "Resistor").unwrap();
        assert_eq!(model.name(), "Resistor");
        
        // Test standard capacitor
        let model = library.create_model("C_100n", "Capacitor").unwrap();
        assert_eq!(model.name(), "Capacitor");
    }
}