//! Component Registry for SPICE Model Mapping
//! 
//! This module provides a data-driven approach to mapping BHDL component types
//! to SPICE models, avoiding hardcoded logic in the core.

use std::collections::HashMap;
use anyhow::Result;
use serde::{Serialize, Deserialize};
use crate::components::ComponentType;

/// Component metadata from BHDL stdlib
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentMetadata {
    /// Component class (e.g., "resistor", "capacitor", "voltage_regulator")
    pub component_class: String,
    /// SPICE model type (e.g., "resistor", "capacitor", "subcircuit")
    pub spice_model: String,
    /// Default parameters for the component
    pub default_parameters: HashMap<String, f64>,
    /// Pin count and types
    pub pin_info: PinInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinInfo {
    pub count: usize,
    pub pin_types: Vec<String>,
}

/// Registry of component types and their SPICE mappings
pub struct ComponentRegistry {
    /// Mapping from component class to metadata
    class_map: HashMap<String, ComponentMetadata>,
    /// Mapping from module names to component classes
    module_map: HashMap<String, String>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            class_map: HashMap::new(),
            module_map: HashMap::new(),
        };
        
        // Initialize with default mappings
        // In production, these would be loaded from stdlib
        registry.init_default_mappings();
        
        registry
    }
    
    /// Load component metadata from BHDL stdlib
    pub fn load_from_stdlib(&mut self, stdlib_path: &str) -> Result<()> {
        // TODO: Parse stdlib modules and extract component metadata
        // For now, use hardcoded defaults
        Ok(())
    }
    
    /// Register a component class
    pub fn register_class(&mut self, class: &str, metadata: ComponentMetadata) {
        self.class_map.insert(class.to_string(), metadata);
    }
    
    /// Register a module name mapping
    pub fn register_module(&mut self, module_name: &str, component_class: &str) {
        self.module_map.insert(module_name.to_string(), component_class.to_string());
    }
    
    /// Get component type from module name or attributes
    pub fn get_component_type(&self, module_name: &str, attributes: &HashMap<String, String>) -> Option<ComponentType> {
        // First check if component_class attribute is present
        if let Some(class) = attributes.get("component_class") {
            return self.class_to_component_type(class);
        }
        
        // Then check module name mapping
        if let Some(class) = self.module_map.get(module_name) {
            return self.class_to_component_type(class);
        }
        
        // Finally, try fuzzy matching on module name
        self.fuzzy_match_component_type(module_name)
    }
    
    /// Get SPICE model type for a component class
    pub fn get_spice_model(&self, component_class: &str) -> Option<&str> {
        self.class_map.get(component_class)
            .map(|meta| meta.spice_model.as_str())
    }
    
    /// Get component class for a module name
    pub fn get_component_class(&self, module_name: &str, _attributes: &HashMap<String, String>) -> Option<String> {
        self.module_map.get(module_name).cloned()
    }
    
    /// Convert component class to ComponentType enum
    fn class_to_component_type(&self, class: &str) -> Option<ComponentType> {
        match class {
            "resistor" => Some(ComponentType::Resistor),
            "capacitor" => Some(ComponentType::Capacitor),
            "capacitor_polarized" => Some(ComponentType::Capacitor),
            "inductor" => Some(ComponentType::Inductor),
            "diode" => Some(ComponentType::Diode),
            "led" => Some(ComponentType::LED),
            "tvs_diode" => Some(ComponentType::Diode), // TVS is a special diode
            "bjt" | "transistor" => Some(ComponentType::BJT),
            "mosfet" | "fet" => Some(ComponentType::MOSFET),
            "opamp" | "op_amp" => Some(ComponentType::OpAmp),
            "voltage_regulator" => Some(ComponentType::VoltageRegulator),
            "power_source" => Some(ComponentType::VoltageSource),
            "ground" => Some(ComponentType::Other("ground".to_string())),
            "test_point" => Some(ComponentType::Other("test_point".to_string())),
            "fuse" => Some(ComponentType::Resistor), // Fuse is modeled as low resistance
            _ => None,
        }
    }
    
    /// Fuzzy matching for component types
    fn fuzzy_match_component_type(&self, module_name: &str) -> Option<ComponentType> {
        let lower = module_name.to_lowercase();
        
        // Check registered modules first
        for (name, class) in &self.module_map {
            if lower.contains(&name.to_lowercase()) {
                return self.class_to_component_type(class);
            }
        }
        
        None
    }
    
    /// Initialize default mappings
    fn init_default_mappings(&mut self) {
        // Resistor
        self.register_class("resistor", ComponentMetadata {
            component_class: "resistor".to_string(),
            spice_model: "resistor".to_string(),
            default_parameters: HashMap::from([
                ("resistance".to_string(), 1000.0),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["passive".to_string(); 2],
            },
        });
        self.register_module("Res", "resistor");
        self.register_module("Resistor", "resistor");
        self.register_module("R", "resistor");
        
        // Capacitor
        self.register_class("capacitor", ComponentMetadata {
            component_class: "capacitor".to_string(),
            spice_model: "capacitor".to_string(),
            default_parameters: HashMap::from([
                ("capacitance".to_string(), 1e-9),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["passive".to_string(); 2],
            },
        });
        self.register_module("Cap", "capacitor");
        self.register_module("Capacitor", "capacitor");
        self.register_module("C", "capacitor");
        
        // Polarized Capacitor
        self.register_class("capacitor_polarized", ComponentMetadata {
            component_class: "capacitor_polarized".to_string(),
            spice_model: "capacitor".to_string(),
            default_parameters: HashMap::from([
                ("capacitance".to_string(), 10e-6),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["positive".to_string(), "negative".to_string()],
            },
        });
        self.register_module("ElectrolyticCap", "capacitor_polarized");
        self.register_module("CP", "capacitor_polarized");
        self.register_module("ElCap", "capacitor_polarized");
        
        // Inductor
        self.register_class("inductor", ComponentMetadata {
            component_class: "inductor".to_string(),
            spice_model: "inductor".to_string(),
            default_parameters: HashMap::from([
                ("inductance".to_string(), 1e-6),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["passive".to_string(); 2],
            },
        });
        self.register_module("Inductor", "inductor");
        self.register_module("L", "inductor");
        self.register_module("Ind", "inductor");
        
        // LED
        self.register_class("led", ComponentMetadata {
            component_class: "led".to_string(),
            spice_model: "diode".to_string(),
            default_parameters: HashMap::from([
                ("is".to_string(), 1e-14),
                ("n".to_string(), 2.0),
                ("rs".to_string(), 10.0),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["anode".to_string(), "cathode".to_string()],
            },
        });
        self.register_module("LED", "led");
        
        // Diode
        self.register_class("diode", ComponentMetadata {
            component_class: "diode".to_string(),
            spice_model: "diode".to_string(),
            default_parameters: HashMap::from([
                ("is".to_string(), 1e-14),
                ("n".to_string(), 1.0),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["anode".to_string(), "cathode".to_string()],
            },
        });
        self.register_module("Diode", "diode");
        self.register_module("SchottkyDiode", "diode");
        self.register_module("TVSDiode", "diode");
        self.register_module("D", "diode");
        
        // Voltage Regulator
        self.register_class("voltage_regulator", ComponentMetadata {
            component_class: "voltage_regulator".to_string(),
            spice_model: "subcircuit".to_string(),
            default_parameters: HashMap::from([
                ("vout".to_string(), 5.0),
                ("dropout".to_string(), 2.0),
            ]),
            pin_info: PinInfo {
                count: 3,
                pin_types: vec!["power_in".to_string(), "ground".to_string(), "power_out".to_string()],
            },
        });
        self.register_module("LM7805", "voltage_regulator");
        self.register_module("LM317", "voltage_regulator");
        
        // Test Point / Connector
        self.register_class("test_point", ComponentMetadata {
            component_class: "test_point".to_string(),
            spice_model: "resistor".to_string(), // High-Z probe
            default_parameters: HashMap::from([
                ("resistance".to_string(), 1e9), // 1GΩ
            ]),
            pin_info: PinInfo {
                count: 1,
                pin_types: vec!["passive".to_string()],
            },
        });
        self.register_module("TestPoint", "test_point");
        self.register_module("TP", "test_point");
        
        // TVS Diode
        self.register_class("tvs_diode", ComponentMetadata {
            component_class: "tvs_diode".to_string(),
            spice_model: "diode".to_string(),
            default_parameters: HashMap::from([
                ("breakdown_voltage".to_string(), 15.0),
                ("capacitance".to_string(), 1e-9), // 1nF
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["anode".to_string(), "cathode".to_string()],
            },
        });
        self.register_module("TVSDiode", "tvs_diode");
        self.register_module("TVS", "tvs_diode");
        
        // Fuse
        self.register_class("fuse", ComponentMetadata {
            component_class: "fuse".to_string(),
            spice_model: "resistor".to_string(), // Low resistance
            default_parameters: HashMap::from([
                ("resistance".to_string(), 0.01), // 10mΩ
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["passive".to_string(); 2],
            },
        });
        self.register_module("Fuse", "fuse");
        self.register_module("F", "fuse");
        self.register_module("FUSE", "fuse");
        
        // Power source
        self.register_class("power_source", ComponentMetadata {
            component_class: "power_source".to_string(),
            spice_model: "voltage_source".to_string(),
            default_parameters: HashMap::from([
                ("voltage".to_string(), 5.0),
            ]),
            pin_info: PinInfo {
                count: 1,
                pin_types: vec!["power_out".to_string()],
            },
        });
        self.register_module("Power", "power_source");
        self.register_module("VCC", "power_source");
        self.register_module("VDD", "power_source");
        
        // Ground
        self.register_class("ground", ComponentMetadata {
            component_class: "ground".to_string(),
            spice_model: "ground".to_string(),
            default_parameters: HashMap::new(),
            pin_info: PinInfo {
                count: 1,
                pin_types: vec!["ground".to_string()],
            },
        });
        self.register_module("Ground", "ground");
        self.register_module("GND", "ground");
        self.register_module("VSS", "ground");
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}