//! Component Registry for SPICE Model Mapping
//!
//! This module provides a data-driven approach to mapping BHDL component types
//! to SPICE models. Component classification is driven by the `component_class`
//! attribute on entity definitions (from stdlib or user code), not by hardcoded
//! name→class mappings.

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

/// Registry of component classes and their SPICE model metadata.
///
/// Component classification is driven by the `component_class` attribute that
/// flows from entity definitions through the synthesis pipeline to instances.
/// This registry only maps class→SPICE-model-parameters; it does NOT contain
/// name→class mappings (those come from entity attributes).
pub struct ComponentRegistry {
    /// Mapping from component class to metadata
    class_map: HashMap<String, ComponentMetadata>,
}

impl ComponentRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            class_map: HashMap::new(),
        };

        registry.init_default_mappings();

        registry
    }

    /// Load component metadata from BHDL stdlib
    pub fn load_from_stdlib(&mut self, stdlib_path: &str) -> Result<()> {
        // TODO: Parse stdlib modules and extract component metadata
        Ok(())
    }

    /// Register a component class with its SPICE model metadata
    pub fn register_class(&mut self, class: &str, metadata: ComponentMetadata) {
        self.class_map.insert(class.to_string(), metadata);
    }

    /// Get component type from instance attributes.
    ///
    /// The primary (and only) lookup path is through the `component_class`
    /// attribute, which flows from entity definitions through the pipeline.
    pub fn get_component_type(&self, _module_name: &str, attributes: &HashMap<String, String>) -> Option<ComponentType> {
        if let Some(class) = attributes.get("component_class") {
            return self.class_to_component_type(class);
        }
        None
    }

    /// Get SPICE model type for a component class
    pub fn get_spice_model(&self, component_class: &str) -> Option<&str> {
        self.class_map.get(component_class)
            .map(|meta| meta.spice_model.as_str())
    }

    /// Get metadata for a component class
    pub fn get_class_metadata(&self, component_class: &str) -> Option<&ComponentMetadata> {
        self.class_map.get(component_class)
    }

    /// Convert component class string to ComponentType enum
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
            "opamp" | "op_amp" | "operational_amplifier" => Some(ComponentType::OpAmp),
            "voltage_regulator" | "linear_regulator" => Some(ComponentType::VoltageRegulator),
            "switching_regulator" => Some(ComponentType::VoltageRegulator),
            "triode" | "vacuum_triode" => Some(ComponentType::Triode),
            "power_source" => Some(ComponentType::VoltageSource),
            "ground" | "ground_reference" => Some(ComponentType::Other("ground".to_string())),
            "test_point" | "connector" => Some(ComponentType::Other("test_point".to_string())),
            "fuse" => Some(ComponentType::Resistor), // Fuse is modeled as low resistance
            _ => None,
        }
    }

    /// Initialize default class→metadata mappings (SPICE model parameters).
    /// These provide default SPICE parameters for each component class.
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

        // Linear Regulator (alias for voltage_regulator)
        self.register_class("linear_regulator", ComponentMetadata {
            component_class: "linear_regulator".to_string(),
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

        // Switching regulators
        self.register_class("switching_regulator", ComponentMetadata {
            component_class: "switching_regulator".to_string(),
            spice_model: "subcircuit".to_string(),
            default_parameters: HashMap::from([
                ("switching_frequency".to_string(), 150e3),
                ("efficiency".to_string(), 0.85),
            ]),
            pin_info: PinInfo {
                count: 5,
                pin_types: vec!["power_in".to_string(), "ground".to_string(),
                               "power_out".to_string(), "control".to_string(), "control".to_string()],
            },
        });

        // Vacuum triode — 3-terminal nonlinear device, Koren model. The
        // default parameters are the nominal 6SN7 set; a real tube refines
        // them by firmware calibration. `spice_model: "triode"` flags the
        // converter to emit a multi-terminal device rather than a branch.
        self.register_class("triode", ComponentMetadata {
            component_class: "triode".to_string(),
            spice_model: "triode".to_string(),
            default_parameters: HashMap::from([
                ("mu".to_string(), 20.0),
                ("ex".to_string(), 1.4),
                ("kg1".to_string(), 1180.0),
                ("kp".to_string(), 470.0),
                ("kvb".to_string(), 300.0),
            ]),
            pin_info: PinInfo {
                count: 3,
                pin_types: vec![
                    "plate".to_string(), "grid".to_string(), "cathode".to_string(),
                ],
            },
        });

        // Test Point / Connector
        self.register_class("test_point", ComponentMetadata {
            component_class: "test_point".to_string(),
            spice_model: "resistor".to_string(),
            default_parameters: HashMap::from([
                ("resistance".to_string(), 1e9),
            ]),
            pin_info: PinInfo {
                count: 1,
                pin_types: vec!["passive".to_string()],
            },
        });

        // TVS Diode
        self.register_class("tvs_diode", ComponentMetadata {
            component_class: "tvs_diode".to_string(),
            spice_model: "diode".to_string(),
            default_parameters: HashMap::from([
                ("breakdown_voltage".to_string(), 15.0),
                ("capacitance".to_string(), 1e-9),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["anode".to_string(), "cathode".to_string()],
            },
        });

        // Fuse
        self.register_class("fuse", ComponentMetadata {
            component_class: "fuse".to_string(),
            spice_model: "resistor".to_string(),
            default_parameters: HashMap::from([
                ("resistance".to_string(), 0.01),
            ]),
            pin_info: PinInfo {
                count: 2,
                pin_types: vec!["passive".to_string(); 2],
            },
        });

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

        // Op-Amp
        self.register_class("opamp", ComponentMetadata {
            component_class: "opamp".to_string(),
            spice_model: "subcircuit".to_string(),
            default_parameters: HashMap::from([
                ("gain".to_string(), 100000.0),
                ("gbw".to_string(), 1e6),
            ]),
            pin_info: PinInfo {
                count: 5,
                pin_types: vec!["input_p".to_string(), "input_n".to_string(),
                               "output".to_string(), "power".to_string(), "ground".to_string()],
            },
        });
    }
}

impl Default for ComponentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
