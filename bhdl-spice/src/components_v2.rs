//! Improved component models with physics-based parameters only

use serde::{Deserialize, Serialize};

/// Improved LED model using only physics-based parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LEDModelV2 {
    /// Saturation current (A) - typically 1e-12 to 1e-15
    pub saturation_current: f64,
    
    /// Emission coefficient (dimensionless) - typically 1.5-2.0
    pub emission_coefficient: f64,
    
    /// Thermal voltage (V) - kT/q, typically 0.026V at room temperature
    pub thermal_voltage: f64,
    
    /// Series resistance (Ω) - bulk and contact resistance
    pub series_resistance: Option<f64>,
    
    /// Operating point hints for solver guidance (optional)
    /// Vec<(current_A, voltage_V)>
    pub operating_hints: Option<Vec<(f64, f64)>>,
    
    /// Maximum ratings (for safety checks only)
    pub max_current: Option<f64>,
    pub max_reverse_voltage: Option<f64>,
    
    /// Color/wavelength info (for documentation only, not for solving)
    pub color: Option<String>,
    pub wavelength_nm: Option<f64>,
}

impl LEDModelV2 {
    /// Create a red LED model with typical parameters
    pub fn red() -> Self {
        Self {
            saturation_current: 1e-12,
            emission_coefficient: 1.5,
            thermal_voltage: 0.026,
            series_resistance: Some(1.0), // 1Ω typical
            operating_hints: Some(vec![
                (0.001, 0.80),  // 1mA
                (0.010, 0.90),  // 10mA
                (0.020, 0.95),  // 20mA (typical test current)
            ]),
            max_current: Some(0.030),
            max_reverse_voltage: Some(5.0),
            color: Some("red".to_string()),
            wavelength_nm: Some(660.0),
        }
    }
    
    /// Calculate voltage for given current using Shockley equation
    pub fn voltage_at_current(&self, current: f64) -> f64 {
        if current <= 0.0 {
            0.0
        } else {
            let vd = self.emission_coefficient * self.thermal_voltage * 
                     ((current / self.saturation_current) + 1.0).ln();
            
            // Add series resistance drop if specified
            if let Some(rs) = self.series_resistance {
                vd + current * rs
            } else {
                vd
            }
        }
    }
    
    /// Calculate current for given voltage (inverse Shockley)
    pub fn current_at_voltage(&self, voltage: f64) -> f64 {
        if voltage <= 0.0 {
            0.0
        } else {
            // For now, ignore series resistance in inverse calculation
            // (would require iterative solution)
            self.saturation_current * 
                ((voltage / (self.emission_coefficient * self.thermal_voltage)).exp() - 1.0)
        }
    }
    
    /// Dynamic resistance at operating point
    pub fn dynamic_resistance(&self, current: f64) -> f64 {
        if current <= 0.0 {
            1e12 // Very high resistance when off
        } else {
            let rd = (self.emission_coefficient * self.thermal_voltage) / 
                     (current + self.saturation_current);
            
            // Add series resistance if specified
            if let Some(rs) = self.series_resistance {
                rd + rs
            } else {
                rd
            }
        }
    }
    
    /// Extract Is from a (Vf, If) operating point
    pub fn from_operating_point(vf: f64, if_current: f64, n: f64, vt: f64) -> f64 {
        // Is = If / (exp(Vf/nVt) - 1)
        if_current / ((vf / (n * vt)).exp() - 1.0)
    }
}

/// Improved component model enum
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ComponentModelV2 {
    Resistor {
        resistance: f64,
        tolerance_percent: Option<f64>,
        max_power: Option<f64>,
    },
    
    Capacitor {
        capacitance: f64,
        tolerance_percent: Option<f64>,
        max_voltage: Option<f64>,
        esr: Option<f64>, // Equivalent series resistance
    },
    
    Inductor {
        inductance: f64,
        tolerance_percent: Option<f64>,
        max_current: Option<f64>,
        dcr: Option<f64>, // DC resistance
    },
    
    VoltageSource {
        voltage: f64,
        internal_resistance: Option<f64>,
        max_current: Option<f64>,
    },
    
    CurrentSource {
        current: f64,
        compliance_voltage: Option<f64>,
    },
    
    LED(LEDModelV2),
    
    Diode {
        saturation_current: f64,
        emission_coefficient: f64,
        thermal_voltage: f64,
        breakdown_voltage: Option<f64>,
        max_current: Option<f64>,
    },
    
    // Add more as needed...
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_led_voltage_current_relationship() {
        let led = LEDModelV2::red();
        
        // Test various operating points
        let test_points = vec![
            (0.0001, "0.1mA"),
            (0.001, "1mA"),
            (0.01, "10mA"),
            (0.02, "20mA"),
        ];
        
        println!("LED V-I Characteristics:");
        for (current, label) in test_points {
            let voltage = led.voltage_at_current(current);
            let r_dyn = led.dynamic_resistance(current);
            println!("{}: V={:.3}V, Rd={:.1}Ω", label, voltage, r_dyn);
        }
    }
    
    #[test]
    fn test_no_fixed_voltage() {
        let led = LEDModelV2::red();
        
        // Verify voltage varies with current (not fixed at 2V!)
        let v1 = led.voltage_at_current(0.001);
        let v2 = led.voltage_at_current(0.020);
        
        assert!(v1 < v2, "Voltage should increase with current");
        assert!((v1 - 0.80).abs() < 0.1, "1mA voltage should be around 0.80V");
        assert!((v2 - 0.95).abs() < 0.1, "20mA voltage should be around 0.95V");
    }
}