//! Pin metadata for component functional identification
//! 
//! This module provides enhanced pin metadata that enables component role detection
//! without relying on naming conventions. Pin functions are explicitly declared,
//! allowing accurate identification of switch nodes, bootstrap pins, feedback pins, etc.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

/// Pin function types for components
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PinFunction {
    /// Power input pin
    PowerIn,
    /// Power output pin
    PowerOut,
    /// High dV/dt switching node
    SwitchNode,
    /// Bootstrap capacitor connection
    Bootstrap,
    /// Feedback voltage sensing
    Feedback,
    /// Compensation network connection
    Compensation,
    /// Soft-start capacitor connection
    SoftStart,
    /// Enable/shutdown control
    Enable,
    /// Current sense input
    CurrentSense,
    /// Error amplifier output
    ErrorAmplifierOut,
    /// Internal voltage reference
    VoltageReference,
    /// Ground/reference
    Ground,
    /// General signal pin
    Signal,
    /// Passive component terminal
    Passive,
    /// Unknown/unspecified function
    Unknown,
}

/// Extended metadata for a pin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinMetadata {
    /// Functional role of the pin
    pub function: PinFunction,
    /// Electrical characteristics
    pub electrical: PinElectricalData,
    /// Additional descriptive text
    pub description: Option<String>,
}

/// Electrical characteristics of a pin
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinElectricalData {
    /// Voltage range (min, max) in volts
    pub voltage_range: Option<(f64, f64)>,
    /// Current rating in amperes
    pub max_current: Option<f64>,
    /// Impedance in ohms (for inputs)
    pub impedance: Option<f64>,
    /// dV/dt rating in V/µs (for switch nodes)
    pub dv_dt_rating: Option<f64>,
    /// Frequency characteristics in Hz
    pub frequency_range: Option<(f64, f64)>,
}

impl Default for PinElectricalData {
    fn default() -> Self {
        Self {
            voltage_range: None,
            max_current: None,
            impedance: None,
            dv_dt_rating: None,
            frequency_range: None,
        }
    }
}

/// Component pin metadata database
#[derive(Debug, Clone, Default)]
pub struct ComponentPinDatabase {
    /// Map from component type to pin metadata
    /// Key format: "ComponentType:PinName"
    metadata: HashMap<String, PinMetadata>,
}

impl ComponentPinDatabase {
    /// Create a new pin database with common components
    pub fn new_with_defaults() -> Self {
        let mut db = Self::default();
        
        // Buck controller pins
        db.add_pin_metadata("BuckController", "SW", PinMetadata {
            function: PinFunction::SwitchNode,
            electrical: PinElectricalData {
                voltage_range: Some((-0.3, 40.0)),
                max_current: Some(5.0),
                dv_dt_rating: Some(100.0), // 100V/µs typical
                ..Default::default()
            },
            description: Some("Switch node - high dV/dt output".to_string()),
        });
        
        db.add_pin_metadata("BuckController", "BOOT", PinMetadata {
            function: PinFunction::Bootstrap,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 50.0)),
                ..Default::default()
            },
            description: Some("Bootstrap capacitor for high-side gate drive".to_string()),
        });
        
        db.add_pin_metadata("BuckController", "FB", PinMetadata {
            function: PinFunction::Feedback,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 5.0)),
                impedance: Some(1e6), // High impedance input
                ..Default::default()
            },
            description: Some("Feedback voltage input".to_string()),
        });
        
        db.add_pin_metadata("BuckController", "COMP", PinMetadata {
            function: PinFunction::Compensation,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 3.3)),
                ..Default::default()
            },
            description: Some("Compensation network connection".to_string()),
        });
        
        db.add_pin_metadata("BuckController", "SS", PinMetadata {
            function: PinFunction::SoftStart,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 5.0)),
                ..Default::default()
            },
            description: Some("Soft-start timing capacitor".to_string()),
        });
        
        db.add_pin_metadata("BuckController", "EN", PinMetadata {
            function: PinFunction::Enable,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 40.0)),
                impedance: Some(1e6),
                ..Default::default()
            },
            description: Some("Enable/shutdown control input".to_string()),
        });
        
        db.add_pin_metadata("BuckController", "VIN", PinMetadata {
            function: PinFunction::PowerIn,
            electrical: PinElectricalData {
                voltage_range: Some((3.0, 40.0)),
                max_current: Some(0.1), // IC supply current
                ..Default::default()
            },
            description: Some("Input power supply".to_string()),
        });
        
        db.add_pin_metadata("BuckController", "GND", PinMetadata {
            function: PinFunction::Ground,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 0.0)),
                ..Default::default()
            },
            description: Some("Ground reference".to_string()),
        });
        
        // Boost controller pins
        db.add_pin_metadata("BoostController", "SW", PinMetadata {
            function: PinFunction::SwitchNode,
            electrical: PinElectricalData {
                voltage_range: Some((-0.3, 60.0)),
                max_current: Some(3.0),
                dv_dt_rating: Some(50.0),
                ..Default::default()
            },
            description: Some("Switch node for boost converter".to_string()),
        });
        
        // Linear regulator pins
        db.add_pin_metadata("VoltageRegulator", "IN", PinMetadata {
            function: PinFunction::PowerIn,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 40.0)),
                max_current: Some(1.5),
                ..Default::default()
            },
            description: Some("Input voltage".to_string()),
        });
        
        db.add_pin_metadata("VoltageRegulator", "OUT", PinMetadata {
            function: PinFunction::PowerOut,
            electrical: PinElectricalData {
                voltage_range: Some((0.0, 37.0)),
                max_current: Some(1.0),
                ..Default::default()
            },
            description: Some("Regulated output voltage".to_string()),
        });
        
        db
    }
    
    /// Add pin metadata for a component type and pin
    pub fn add_pin_metadata(&mut self, component_type: &str, pin_name: &str, metadata: PinMetadata) {
        let key = format!("{}:{}", component_type, pin_name);
        self.metadata.insert(key, metadata);
    }
    
    /// Get pin metadata for a component type and pin
    pub fn get_pin_metadata(&self, component_type: &str, pin_name: &str) -> Option<&PinMetadata> {
        let key = format!("{}:{}", component_type, pin_name);
        self.metadata.get(&key)
    }
    
    /// Check if a pin has a specific function
    pub fn pin_has_function(&self, component_type: &str, pin_name: &str, function: &PinFunction) -> bool {
        self.get_pin_metadata(component_type, pin_name)
            .map(|meta| &meta.function == function)
            .unwrap_or(false)
    }
}

/// Integration with Circuit for pin metadata
impl crate::circuit::Circuit {
    /// Get pin metadata for a component
    pub fn get_component_pin_metadata(&self, component_id: crate::circuit::ComponentId, pin_database: &ComponentPinDatabase) -> HashMap<String, PinMetadata> {
        let mut result = HashMap::new();
        
        if let Some(component) = self.get_component(component_id) {
            // For now, we'll use a simple pin naming scheme
            // In a real implementation, this would come from the netlist
            let pins = match component.component_type() {
                "BuckController" => vec!["VIN", "SW", "BOOT", "FB", "COMP", "EN", "SS", "GND"],
                "BoostController" => vec!["VIN", "SW", "FB", "COMP", "EN", "GND"],
                "VoltageRegulator" => vec!["IN", "OUT", "GND"],
                _ => vec!["1", "2"], // Default two-pin component
            };
            
            for pin in pins {
                if let Some(metadata) = pin_database.get_pin_metadata(component.component_type(), pin) {
                    result.insert(pin.to_string(), metadata.clone());
                }
            }
        }
        
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_pin_database() {
        let db = ComponentPinDatabase::new_with_defaults();
        
        // Test buck controller switch node
        assert!(db.pin_has_function("BuckController", "SW", &PinFunction::SwitchNode));
        assert!(db.pin_has_function("BuckController", "BOOT", &PinFunction::Bootstrap));
        assert!(db.pin_has_function("BuckController", "FB", &PinFunction::Feedback));
        
        // Test that wrong functions return false
        assert!(!db.pin_has_function("BuckController", "SW", &PinFunction::PowerIn));
        
        // Test metadata retrieval
        let sw_meta = db.get_pin_metadata("BuckController", "SW").unwrap();
        assert_eq!(sw_meta.function, PinFunction::SwitchNode);
        assert_eq!(sw_meta.electrical.dv_dt_rating, Some(100.0));
    }
}