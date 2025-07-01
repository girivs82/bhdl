//! BHDL Stdlib Model Loader
//! 
//! This module loads component models from BHDL stdlib electrical parameters
//! conforming to the BHDL architecture where models come from stdlib.

use std::collections::HashMap;
use crate::{ComponentModel, ElectricalLimits};
use anyhow::{Result, Context};

/// LED color mapping to stdlib parameter names
#[derive(Debug, Clone)]
pub enum LedColor {
    Red,
    Green,
    Blue,
    White,
    Yellow,
    IR,
}

impl LedColor {
    /// Parse color from string
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "red" => Some(Self::Red),
            "green" => Some(Self::Green),
            "blue" => Some(Self::Blue),
            "white" => Some(Self::White),
            "yellow" => Some(Self::Yellow),
            "ir" | "infrared" => Some(Self::IR),
            _ => None,
        }
    }
    
    /// Get stdlib parameters for this LED color
    /// These values are calculated to match Vf at If using Shockley equation
    pub fn get_params(&self) -> LedStdlibParams {
        match self {
            Self::Red => LedStdlibParams {
                forward_voltage: 2.0,
                forward_current: 0.020,  // 20mA
                max_current: 0.030,      // 30mA
                dynamic_resistance: 10.0,
                // Correct Is value calculated for Vf=2.0V at If=20mA with n=1.8
                saturation_current: 5.51e-21,  // 5.51 zeptoamps
                emission_coefficient: 1.8,     // Typical for red LED
                thermal_voltage: 0.026,        // 26mV
            },
            Self::Green => LedStdlibParams {
                forward_voltage: 2.2,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 12.0,
                // Correct Is value calculated for Vf=2.2V at If=20mA with n=1.9
                saturation_current: 9.12e-22,   // 0.912 zeptoamps
                emission_coefficient: 1.9,
                thermal_voltage: 0.026,
            },
            Self::Blue => LedStdlibParams {
                forward_voltage: 3.2,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 15.0,
                // Correct Is value calculated for Vf=3.2V at If=20mA with n=2.2
                saturation_current: 1.01e-26,   // 10.1 yoctoamps
                emission_coefficient: 2.2,
                thermal_voltage: 0.026,
            },
            Self::White => LedStdlibParams {
                forward_voltage: 3.3,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 16.0,
                // Correct Is value calculated for Vf=3.3V at If=20mA with n=2.3
                saturation_current: 2.16e-26,   // 21.6 yoctoamps
                emission_coefficient: 2.3,
                thermal_voltage: 0.026,
            },
            Self::Yellow => LedStdlibParams {
                forward_voltage: 2.1,
                forward_current: 0.020,
                max_current: 0.030,
                dynamic_resistance: 11.0,
                // Correct Is value calculated for Vf=2.1V at If=20mA with n=1.85
                saturation_current: 2.19e-21,   // 2.19 zeptoamps
                emission_coefficient: 1.85,
                thermal_voltage: 0.026,
            },
            Self::IR => LedStdlibParams {
                forward_voltage: 1.4,
                forward_current: 0.050,      // 50mA
                max_current: 0.100,          // 100mA
                dynamic_resistance: 5.0,
                // Correct Is value calculated for Vf=1.4V at If=50mA with n=1.5
                saturation_current: 1.29e-17,   // 12.9 attoamps
                emission_coefficient: 1.5,
                thermal_voltage: 0.026,
            },
        }
    }
}

/// LED parameters from stdlib
#[derive(Debug, Clone)]
pub struct LedStdlibParams {
    pub forward_voltage: f64,
    pub forward_current: f64,
    pub max_current: f64,
    pub dynamic_resistance: f64,
    pub saturation_current: f64,
    pub emission_coefficient: f64,
    pub thermal_voltage: f64,
}

/// Stdlib model loader
pub struct StdlibModelLoader;

impl StdlibModelLoader {
    /// Create LED model from stdlib parameters
    pub fn create_led_model(name: &str, color: &str) -> Result<ComponentModel> {
        let led_color = LedColor::from_str(color)
            .ok_or_else(|| anyhow::anyhow!("Unknown LED color: {}", color))?;
        
        let params = led_color.get_params();
        
        Ok(ComponentModel::LED {
            color: color.to_string(),
            forward_voltage: params.forward_voltage,
            forward_current: params.forward_current,
            dynamic_resistance: params.dynamic_resistance,
            saturation_current: Some(params.saturation_current),
            emission_coefficient: Some(params.emission_coefficient),
            thermal_voltage: Some(params.thermal_voltage),
            limits: ElectricalLimits {
                max_voltage: Some(5.0),  // Common reverse voltage limit
                max_current: Some(params.max_current),
                max_power: Some(params.forward_voltage * params.max_current),
                min_voltage: None,
                temp_range: Some((-40.0, 85.0)),
            },
        })
    }
    
    /// Create resistor model
    pub fn create_resistor_model(name: &str, resistance: f64, power_rating: Option<f64>) -> ComponentModel {
        let power = power_rating.unwrap_or(0.125);  // Default 1/8W
        
        ComponentModel::Resistor {
            resistance,
            tolerance: 5.0,  // Default 5%
            limits: ElectricalLimits {
                max_voltage: Some((power * resistance).sqrt()),
                max_current: Some((power / resistance).sqrt()),
                max_power: Some(power),
                min_voltage: None,
                temp_range: Some((-55.0, 125.0)),
            },
        }
    }
    
    /// Create voltage source model
    pub fn create_voltage_source_model(name: &str, voltage: f64) -> ComponentModel {
        ComponentModel::VoltageSource {
            voltage,
            internal_resistance: Some(0.0),  // Ideal source
        }
    }
    
    /// Create a collection of LED models with varying Is values for testing
    pub fn create_test_led_models(is_values: &[f64]) -> HashMap<String, ComponentModel> {
        let mut models = HashMap::new();
        
        for (i, &is_value) in is_values.iter().enumerate() {
            let name = format!("D{}", i + 1);
            models.insert(name.clone(), ComponentModel::LED {
                color: "red".to_string(),
                forward_voltage: 2.0,
                forward_current: 0.020,
                dynamic_resistance: 10.0,
                saturation_current: Some(is_value),
                emission_coefficient: Some(1.5),
                thermal_voltage: Some(0.026),
                limits: ElectricalLimits {
                    max_voltage: Some(5.0),
                    max_current: Some(0.030),
                    max_power: Some(0.060),  // 60mW
                    min_voltage: None,
                    temp_range: Some((-40.0, 85.0)),
                },
            });
        }
        
        models
    }
    
    /// Load models from BHDL circuit with stdlib defaults
    pub fn load_models_from_circuit(circuit: &crate::Circuit) -> Result<HashMap<String, ComponentModel>> {
        let mut models = HashMap::new();
        
        for (_idx, branch) in circuit.branches() {
            let model = match branch.component_type.as_str() {
                "VoltageSource" => {
                    Self::create_voltage_source_model(&branch.name, branch.value)
                }
                "Resistor" => {
                    Self::create_resistor_model(&branch.name, branch.value, None)
                }
                "LED" => {
                    // Default to red LED if no color specified
                    Self::create_led_model(&branch.name, "red")?
                }
                _ => continue,  // Skip unknown types
            };
            
            models.insert(branch.name.clone(), model);
        }
        
        Ok(models)
    }
}

/// Create IBIS table model (simplified)
pub fn create_ibis_model(name: &str, voltages: Vec<f64>, currents: Vec<f64>) -> ComponentModel {
    // For now, approximate as resistor
    // In production, would create proper IBIS model
    let resistance = if currents.len() > 1 && voltages.len() > 1 {
        (voltages[1] - voltages[0]) / (currents[1] - currents[0])
    } else {
        50.0  // Default 50 ohm
    };
    
    ComponentModel::Resistor {
        resistance,
        tolerance: 10.0,
        limits: ElectricalLimits::default(),
    }
}