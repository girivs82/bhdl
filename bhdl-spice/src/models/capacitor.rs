//! Capacitor SPICE models with ESR, ESL, and voltage coefficients

use std::collections::HashMap;
use super::{SpiceModel, ModelType, DEFAULT_TEMPERATURE};

/// Capacitor model parameters
#[derive(Debug, Clone)]
pub struct CapacitorParams {
    /// Nominal capacitance (F)
    pub capacitance: f64,
    /// Tolerance (%)
    pub tolerance: f64,
    /// Equivalent series resistance (Ω)
    pub esr: f64,
    /// Equivalent series inductance (H)
    pub esl: f64,
    /// Linear voltage coefficient (ppm/V)
    pub vc1: f64,
    /// Quadratic voltage coefficient (ppm/V²)
    pub vc2: f64,
    /// Temperature coefficient (ppm/°C)
    pub tc1: f64,
    /// Nominal temperature (°C)
    pub tnom: f64,
    /// Voltage rating (V)
    pub voltage_rating: f64,
    /// Leakage resistance (Ω)
    pub rleak: f64,
    /// Dielectric absorption (%)
    pub da: f64,
}

impl Default for CapacitorParams {
    fn default() -> Self {
        Self {
            capacitance: 1e-6,      // 1µF
            tolerance: 10.0,        // 10%
            esr: 0.1,              // 100mΩ
            esl: 1e-9,             // 1nH
            vc1: 0.0,              // No voltage coefficient
            vc2: 0.0,
            tc1: 0.0,              // No temperature coefficient
            tnom: DEFAULT_TEMPERATURE,
            voltage_rating: 50.0,   // 50V
            rleak: 1e9,            // 1GΩ leakage
            da: 0.0,               // No dielectric absorption
        }
    }
}

/// Common capacitor types
impl CapacitorParams {
    /// Ceramic capacitor (X7R)
    pub fn ceramic_x7r(capacitance: f64, voltage: f64) -> Self {
        Self {
            capacitance,
            tolerance: 10.0,
            esr: 0.01 / capacitance.sqrt(),  // ESR decreases with C
            esl: 0.5e-9,                      // 0.5nH typical
            vc1: -1000.0,                     // -1000 ppm/V
            vc2: 0.0,
            tc1: -750.0,                      // ±15% over temperature
            voltage_rating: voltage,
            rleak: 1e10,                      // 10GΩ
            da: 0.5,
            ..Default::default()
        }
    }
    
    /// Ceramic capacitor (C0G/NP0)
    pub fn ceramic_c0g(capacitance: f64, voltage: f64) -> Self {
        Self {
            capacitance,
            tolerance: 5.0,
            esr: 0.005 / capacitance.sqrt(),
            esl: 0.5e-9,
            vc1: 0.0,      // Very stable
            vc2: 0.0,
            tc1: 30.0,     // ±30 ppm/°C
            voltage_rating: voltage,
            rleak: 1e11,   // 100GΩ
            da: 0.1,
            ..Default::default()
        }
    }
    
    /// Electrolytic capacitor (aluminum)
    pub fn electrolytic(capacitance: f64, voltage: f64) -> Self {
        Self {
            capacitance,
            tolerance: 20.0,
            esr: 0.5 / capacitance.sqrt(),   // Higher ESR
            esl: 10e-9,                       // Higher ESL
            vc1: 0.0,
            vc2: 0.0,
            tc1: -2000.0,                     // Large temperature variation
            voltage_rating: voltage,
            // Electrolytic leakage spec is I_leak ≈ 0.01·C·V, so
            // R_leak ≈ 1/(0.01·C): bigger caps leak more (lower R).
            rleak: 100.0 / capacitance,
            da: 2.0,                          // Higher dielectric absorption
            ..Default::default()
        }
    }
    
    /// Tantalum capacitor
    pub fn tantalum(capacitance: f64, voltage: f64) -> Self {
        Self {
            capacitance,
            tolerance: 10.0,
            esr: 0.2 / capacitance.sqrt(),
            esl: 2e-9,
            vc1: -200.0,
            vc2: 0.0,
            tc1: -500.0,
            voltage_rating: voltage,
            rleak: 500.0 / capacitance,  // ~5x lower leakage than aluminum
            da: 1.0,
            ..Default::default()
        }
    }
    
    /// Film capacitor (polypropylene)
    pub fn film_pp(capacitance: f64, voltage: f64) -> Self {
        Self {
            capacitance,
            tolerance: 5.0,
            esr: 0.001 / capacitance.sqrt(),
            esl: 5e-9,
            vc1: -200.0,
            vc2: 0.0,
            tc1: -200.0,
            voltage_rating: voltage,
            rleak: 1e12,  // Very high
            da: 0.05,     // Very low
            ..Default::default()
        }
    }
}

/// Capacitor SPICE model
pub struct CapacitorModel {
    name: String,
    params: CapacitorParams,
}

impl CapacitorModel {
    /// Create new capacitor model
    pub fn new(name: String, params: CapacitorParams) -> Self {
        Self { name, params }
    }
    
    /// Create model from value and type
    pub fn from_value(name: &str, capacitance: f64, cap_type: &str, voltage: f64) -> Self {
        let params = match cap_type.to_lowercase().as_str() {
            "x7r" | "ceramic" => CapacitorParams::ceramic_x7r(capacitance, voltage),
            "c0g" | "np0" => CapacitorParams::ceramic_c0g(capacitance, voltage),
            "electrolytic" | "elco" => CapacitorParams::electrolytic(capacitance, voltage),
            "tantalum" | "tant" => CapacitorParams::tantalum(capacitance, voltage),
            "film" | "pp" => CapacitorParams::film_pp(capacitance, voltage),
            _ => CapacitorParams {
                capacitance,
                voltage_rating: voltage,
                ..Default::default()
            },
        };
        Self::new(name.to_string(), params)
    }
    
    /// Calculate voltage and temperature adjusted capacitance
    fn capacitance_adjusted(&self, voltage: f64, temp: f64) -> f64 {
        let temp_delta = temp - self.params.tnom;
        let tc_factor = 1.0 + self.params.tc1 * 1e-6 * temp_delta;
        
        let vc_factor = 1.0 + self.params.vc1 * 1e-6 * voltage + 
                              self.params.vc2 * 1e-6 * voltage * voltage;
        
        self.params.capacitance * tc_factor * vc_factor
    }
    
    /// For DC analysis, capacitor is open circuit (with leakage)
    fn dc_resistance(&self) -> f64 {
        self.params.rleak
    }
    
    /// Check voltage rating
    fn check_voltage_limit(&self, voltage: f64) -> bool {
        voltage.abs() <= self.params.voltage_rating
    }
    
    /// Calculate quality factor Q at given frequency
    fn quality_factor(&self, frequency: f64) -> f64 {
        let omega = 2.0 * std::f64::consts::PI * frequency;
        let xc = 1.0 / (omega * self.params.capacitance);
        xc / self.params.esr
    }
}

impl Default for CapacitorModel {
    fn default() -> Self {
        Self::new("generic_capacitor".to_string(), CapacitorParams::default())
    }
}

impl SpiceModel for CapacitorModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::Capacitor
    }
    
    fn current(&self, voltages: &[f64], _temp: f64) -> f64 {
        if voltages.len() != 2 {
            return 0.0;
        }
        
        // For DC analysis, only leakage current
        let v = voltages[1] - voltages[0];
        v / self.dc_resistance()
    }
    
    fn conductance(&self, _voltages: &[f64], _temp: f64) -> Vec<f64> {
        // DC conductance is just 1/Rleak
        vec![1.0 / self.dc_resistance()]
    }
    
    fn num_terminals(&self) -> usize {
        2
    }
    
    fn is_nonlinear(&self) -> bool {
        // Nonlinear if significant voltage coefficients
        self.params.vc1.abs() > 100.0 || self.params.vc2.abs() > 10.0
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("c".to_string(), self.params.capacitance);
        params.insert("tolerance".to_string(), self.params.tolerance);
        params.insert("esr".to_string(), self.params.esr);
        params.insert("esl".to_string(), self.params.esl);
        params.insert("vc1".to_string(), self.params.vc1);
        params.insert("vc2".to_string(), self.params.vc2);
        params.insert("tc1".to_string(), self.params.tc1);
        params.insert("tnom".to_string(), self.params.tnom);
        params.insert("voltage_rating".to_string(), self.params.voltage_rating);
        params.insert("rleak".to_string(), self.params.rleak);
        params.insert("da".to_string(), self.params.da);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "c" | "capacitance" => self.params.capacitance = value,
            "tolerance" => self.params.tolerance = value,
            "esr" => self.params.esr = value,
            "esl" => self.params.esl = value,
            "vc1" => self.params.vc1 = value,
            "vc2" => self.params.vc2 = value,
            "tc1" => self.params.tc1 = value,
            "tnom" => self.params.tnom = value,
            "voltage_rating" => self.params.voltage_rating = value,
            "rleak" => self.params.rleak = value,
            "da" => self.params.da = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_voltage_coefficient() {
        let cap = CapacitorModel::from_value("test", 10e-6, "x7r", 25.0);
        
        // Nominal capacitance
        let c_nom = cap.capacitance_adjusted(0.0, 27.0);
        assert_eq!(c_nom, 10e-6);
        
        // With applied voltage (should decrease for X7R)
        let c_bias = cap.capacitance_adjusted(10.0, 27.0);
        assert!(c_bias < c_nom);
    }
    
    #[test]
    fn test_dc_behavior() {
        let cap = CapacitorModel::from_value("test", 100e-6, "electrolytic", 25.0);
        let current = cap.current(&[0.0, 10.0], 27.0);
        
        // Should have small leakage current. A 100µF electrolytic at 10V
        // leaks ~10µA (I_leak ≈ 0.01·C·V), so bound at 100µA.
        assert!(current > 0.0);
        assert!(current < 100e-6);
    }
}