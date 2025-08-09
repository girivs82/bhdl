//! Component models and electrical characteristics

use serde::{Serialize, Deserialize};

/// Electrical limits for a component
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ElectricalLimits {
    /// Maximum voltage rating (V)
    pub max_voltage: Option<f64>,
    /// Maximum current rating (A)
    pub max_current: Option<f64>,
    /// Maximum power dissipation (W)
    pub max_power: Option<f64>,
    /// Minimum voltage (for active components)
    pub min_voltage: Option<f64>,
    /// Operating temperature range (°C)
    pub temp_range: Option<(f64, f64)>,
}

impl Default for ElectricalLimits {
    fn default() -> Self {
        Self {
            max_voltage: None,
            max_current: None,
            max_power: None,
            min_voltage: None,
            temp_range: None,
        }
    }
}

/// Component type enumeration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentType {
    Resistor,
    Capacitor,
    Inductor,
    Diode,
    LED,
    BJT,
    MOSFET,
    VoltageSource,
    CurrentSource,
    OpAmp,
    VoltageRegulator,
    Other(String),
}

/// Component model for electrical analysis
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ComponentModel {
    /// Linear resistor: V = I * R
    Resistor {
        resistance: f64,  // Ohms
        tolerance: f64,   // Percentage
        limits: ElectricalLimits,
    },
    
    /// Capacitor (DC analysis: open circuit)
    Capacitor {
        capacitance: f64,  // Farads
        esr: Option<f64>,  // Equivalent series resistance
        limits: ElectricalLimits,
    },
    
    /// Inductor (DC analysis: short circuit)
    Inductor {
        inductance: f64,   // Henries
        dcr: Option<f64>,  // DC resistance
        limits: ElectricalLimits,
    },
    
    /// Diode with simplified model
    Diode {
        forward_voltage: f64,     // Vf at nominal current
        forward_resistance: f64,  // Dynamic resistance
        reverse_current: f64,     // Leakage current
        saturation_current: Option<f64>, // Is for Shockley equation
        emission_coefficient: Option<f64>, // n for ideality factor
        limits: ElectricalLimits,
    },
    
    /// LED model
    LED {
        color: String,
        forward_voltage: f64,     // Typical Vf
        forward_current: f64,     // Nominal If
        dynamic_resistance: f64,  // Small signal resistance
        // SPICE model parameters for accurate exponential modeling
        saturation_current: Option<f64>,    // Is - reverse saturation current (A)
        emission_coefficient: Option<f64>,  // n - ideality factor (dimensionless)
        thermal_voltage: Option<f64>,       // Vt - thermal voltage (V)
        limits: ElectricalLimits,
    },
    
    /// Ideal voltage source
    VoltageSource {
        voltage: f64,
        internal_resistance: Option<f64>,
    },
    
    /// Ideal current source
    CurrentSource {
        current: f64,
        internal_resistance: Option<f64>,
    },
    
    /// Linear voltage regulator
    VoltageRegulator {
        output_voltage: f64,
        dropout_voltage: f64,
        quiescent_current: f64,
        limits: ElectricalLimits,
    },
}

impl ComponentModel {
    /// Get component limits
    pub fn limits(&self) -> &ElectricalLimits {
        match self {
            ComponentModel::Resistor { limits, .. } |
            ComponentModel::Capacitor { limits, .. } |
            ComponentModel::Inductor { limits, .. } |
            ComponentModel::Diode { limits, .. } |
            ComponentModel::LED { limits, .. } |
            ComponentModel::VoltageRegulator { limits, .. } => limits,
            _ => &DEFAULT_LIMITS,
        }
    }
    
    /// Calculate equivalent resistance for DC analysis
    pub fn dc_resistance(&self) -> f64 {
        match self {
            ComponentModel::Resistor { resistance, .. } => *resistance,
            ComponentModel::Capacitor { .. } => f64::INFINITY,  // Open circuit
            ComponentModel::Inductor { dcr, .. } => dcr.unwrap_or(0.0),  // Short or DCR
            ComponentModel::Diode { forward_resistance, .. } => *forward_resistance,
            ComponentModel::LED { dynamic_resistance, .. } => *dynamic_resistance,
            ComponentModel::VoltageSource { internal_resistance, .. } => internal_resistance.unwrap_or(0.0),
            ComponentModel::CurrentSource { internal_resistance, .. } => internal_resistance.unwrap_or(f64::INFINITY),
            _ => 0.0,
        }
    }
    
    /// Get voltage drop across component at given current (for nonlinear components)
    pub fn voltage_at_current(&self, current: f64) -> f64 {
        match self {
            ComponentModel::Resistor { resistance, .. } => current * resistance,
            ComponentModel::Diode { forward_voltage, forward_resistance, .. } => {
                if current > 0.0 {
                    forward_voltage + current * forward_resistance
                } else {
                    current * 1e9  // Very high reverse resistance
                }
            }
            ComponentModel::LED { forward_voltage, dynamic_resistance, .. } => {
                if current > 0.0 {
                    forward_voltage + current * dynamic_resistance
                } else {
                    current * 1e9  // Very high reverse resistance
                }
            }
            _ => current * self.dc_resistance(),
        }
    }
}

/// Default limits for components without specified limits
static DEFAULT_LIMITS: ElectricalLimits = ElectricalLimits {
    max_voltage: None,
    max_current: None,
    max_power: None,
    min_voltage: None,
    temp_range: None,
};

/// Component specification combining model and metadata
#[derive(Debug, Clone)]
pub struct Component {
    pub name: String,
    pub component_type: ComponentType,
    pub model: ComponentModel,
    pub part_number: Option<String>,
    pub manufacturer: Option<String>,
}

impl Component {
    /// Create a resistor component
    pub fn resistor(name: String, resistance: f64, power_rating: f64) -> Self {
        Self {
            name,
            component_type: ComponentType::Resistor,
            model: ComponentModel::Resistor {
                resistance,
                tolerance: 5.0,
                limits: ElectricalLimits {
                    max_power: Some(power_rating),
                    ..Default::default()
                },
            },
            part_number: None,
            manufacturer: None,
        }
    }
    
    /// Create an LED component
    pub fn led(name: String, color: String) -> Self {
        let (vf, if_nom) = match color.to_lowercase().as_str() {
            "red" => (1.8, 0.020),
            "green" => (2.2, 0.020),
            "blue" => (3.2, 0.020),
            "white" => (3.3, 0.020),
            "yellow" => (2.0, 0.020),
            "orange" => (2.0, 0.020),
            _ => (2.0, 0.020),
        };
        
        Self {
            name,
            component_type: ComponentType::LED,
            model: ComponentModel::LED {
                color: color.clone(),
                forward_voltage: vf,
                forward_current: if_nom,
                dynamic_resistance: 10.0,  // Typical value
                // Use realistic SPICE parameters based on color (calculated from datasheet)
                saturation_current: Some(match color.as_str() {
                    "red" | "yellow" => 3.96e-19,    // Calculated from Vf=2V @ 20mA
                    "green" => 2.5e-19,              // Slightly lower for higher Vf  
                    "blue" | "white" => 1.5e-19,     // Lower Is for higher Vf
                    "ir" => 5e-19,                   // Higher Is for lower Vf
                    _ => 3.96e-19,                   // Default realistic value
                }),
                emission_coefficient: Some(match color.as_str() {
                    "red" => 2.0,
                    "yellow" => 1.9,
                    "green" => 1.8,
                    "blue" | "white" => 1.6,
                    "ir" => 1.5,
                    _ => 2.0,  // Default
                }),
                thermal_voltage: Some(0.026),  // 26mV at room temperature
                limits: ElectricalLimits {
                    max_current: Some(0.030),  // 30mA typical absolute max
                    max_power: Some(0.1),      // 100mW typical
                    ..Default::default()
                },
            },
            part_number: None,
            manufacturer: None,
        }
    }
    
    /// Create a voltage source
    pub fn voltage_source(name: String, voltage: f64) -> Self {
        Self {
            name,
            component_type: ComponentType::VoltageSource,
            model: ComponentModel::VoltageSource {
                voltage,
                internal_resistance: Some(0.001),  // 1mΩ typical
            },
            part_number: None,
            manufacturer: None,
        }
    }
    
    /// Create a linear voltage regulator
    pub fn voltage_regulator(name: String, output_voltage: f64) -> Self {
        Self {
            name,
            component_type: ComponentType::VoltageRegulator,
            model: ComponentModel::VoltageRegulator {
                output_voltage,
                dropout_voltage: 2.0,  // Typical for 7805
                quiescent_current: 0.005,  // 5mA typical
                limits: ElectricalLimits {
                    max_voltage: Some(35.0),  // Typical for 78xx series
                    max_current: Some(1.5),   // With heatsink
                    max_power: Some(15.0),    // With heatsink
                    min_voltage: Some(output_voltage + 2.0),
                    ..Default::default()
                },
            },
            part_number: None,
            manufacturer: None,
        }
    }
}