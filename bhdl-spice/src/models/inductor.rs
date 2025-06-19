//! Inductor SPICE models with saturation and core losses

use std::collections::HashMap;
use super::{SpiceModel, ModelType, DEFAULT_TEMPERATURE};

/// Inductor model parameters
#[derive(Debug, Clone)]
pub struct InductorParams {
    /// Nominal inductance (H)
    pub inductance: f64,
    /// Tolerance (%)
    pub tolerance: f64,
    /// DC resistance (Ω)
    pub dcr: f64,
    /// Saturation current (A)
    pub isat: f64,
    /// Core loss resistance (Ω) - frequency dependent
    pub rcore: f64,
    /// Parasitic capacitance (F)
    pub cpar: f64,
    /// Temperature coefficient (ppm/°C)
    pub tc1: f64,
    /// Nominal temperature (°C)
    pub tnom: f64,
    /// Current rating (A)
    pub current_rating: f64,
    /// Quality factor at 1kHz
    pub q_factor: f64,
    /// Core material type
    pub core_type: CoreType,
}

/// Core material types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CoreType {
    Air,
    Ferrite,
    IronPowder,
    Laminated,
    MPP,     // Molypermalloy
    Sendust,
}

impl Default for InductorParams {
    fn default() -> Self {
        Self {
            inductance: 1e-3,        // 1mH
            tolerance: 10.0,         // 10%
            dcr: 0.1,               // 100mΩ
            isat: 1.0,              // 1A saturation
            rcore: 1000.0,          // 1kΩ core loss
            cpar: 1e-12,            // 1pF parasitic
            tc1: 100.0,             // 100 ppm/°C
            tnom: DEFAULT_TEMPERATURE,
            current_rating: 1.0,     // 1A
            q_factor: 50.0,         // Q=50 at 1kHz
            core_type: CoreType::Ferrite,
        }
    }
}

/// Common inductor types
impl InductorParams {
    /// SMD power inductor
    pub fn smd_power(inductance: f64, current: f64) -> Self {
        Self {
            inductance,
            tolerance: 20.0,
            dcr: 0.01 * (inductance * 1e6).sqrt(),  // Rough approximation
            isat: current,
            rcore: 50.0 / (inductance * 1e6).sqrt(),
            cpar: 0.5e-12,
            tc1: 200.0,
            current_rating: current,
            q_factor: 30.0,
            core_type: CoreType::Ferrite,
            ..Default::default()
        }
    }
    
    /// RF inductor (high Q)
    pub fn rf_inductor(inductance: f64) -> Self {
        Self {
            inductance,
            tolerance: 5.0,
            dcr: 0.001 * (inductance * 1e9).sqrt(),  // Very low DCR
            isat: 0.1,  // Small signal
            rcore: 10000.0,  // Very low loss
            cpar: 0.1e-12,   // Minimal parasitic
            tc1: 50.0,
            current_rating: 0.1,
            q_factor: 150.0,  // High Q
            core_type: CoreType::Air,
            ..Default::default()
        }
    }
    
    /// Common mode choke
    pub fn common_mode(inductance: f64, current: f64) -> Self {
        Self {
            inductance,
            tolerance: 30.0,
            dcr: 0.05 * (inductance * 1e3).sqrt(),
            isat: current * 2.0,  // Higher saturation
            rcore: 100.0 / (inductance * 1e3).sqrt(),
            cpar: 2e-12,
            tc1: 300.0,
            current_rating: current,
            q_factor: 20.0,
            core_type: CoreType::Ferrite,
            ..Default::default()
        }
    }
    
    /// Iron powder toroid
    pub fn iron_powder(inductance: f64, current: f64) -> Self {
        Self {
            inductance,
            tolerance: 10.0,
            dcr: 0.02 * (inductance * 1e6).sqrt(),
            isat: current,
            rcore: 20.0 / (inductance * 1e6).sqrt(),
            cpar: 1e-12,
            tc1: 150.0,
            current_rating: current,
            q_factor: 40.0,
            core_type: CoreType::IronPowder,
            ..Default::default()
        }
    }
}

/// Inductor SPICE model
pub struct InductorModel {
    name: String,
    params: InductorParams,
}

impl InductorModel {
    /// Create new inductor model
    pub fn new(name: String, params: InductorParams) -> Self {
        Self { name, params }
    }
    
    /// Create model from value and type
    pub fn from_value(name: &str, inductance: f64, inductor_type: &str, current: f64) -> Self {
        let params = match inductor_type.to_lowercase().as_str() {
            "smd" | "power" => InductorParams::smd_power(inductance, current),
            "rf" | "air" => InductorParams::rf_inductor(inductance),
            "common_mode" | "cm" => InductorParams::common_mode(inductance, current),
            "toroid" | "iron" => InductorParams::iron_powder(inductance, current),
            _ => InductorParams {
                inductance,
                current_rating: current,
                ..Default::default()
            },
        };
        Self::new(name.to_string(), params)
    }
    
    /// Calculate inductance with saturation effects
    fn inductance_saturated(&self, current: f64) -> f64 {
        if self.params.isat <= 0.0 {
            return self.params.inductance;
        }
        
        // Simple saturation model: L(I) = L0 / (1 + (I/Isat)²)
        let sat_factor = 1.0 + (current / self.params.isat).powi(2);
        self.params.inductance / sat_factor
    }
    
    /// Temperature adjusted inductance
    fn inductance_temp(&self, current: f64, temp: f64) -> f64 {
        let temp_delta = temp - self.params.tnom;
        let tc_factor = 1.0 + self.params.tc1 * 1e-6 * temp_delta;
        
        self.inductance_saturated(current) * tc_factor
    }
    
    /// For DC analysis, inductor is short circuit with DCR
    fn dc_resistance(&self) -> f64 {
        self.params.dcr
    }
    
    /// Calculate core losses at given frequency
    fn core_loss_resistance(&self, frequency: f64) -> f64 {
        // Core loss increases with frequency
        // Simple model: Rcore(f) = Rcore_nom * sqrt(f/1kHz)
        self.params.rcore * (frequency / 1000.0).sqrt()
    }
    
    /// Check current rating
    fn check_current_limit(&self, current: f64) -> bool {
        current.abs() <= self.params.current_rating
    }
}

impl Default for InductorModel {
    fn default() -> Self {
        Self::new("generic_inductor".to_string(), InductorParams::default())
    }
}

impl SpiceModel for InductorModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::Inductor
    }
    
    fn current(&self, voltages: &[f64], _temp: f64) -> f64 {
        if voltages.len() != 2 {
            return 0.0;
        }
        
        // For DC analysis, V = I * DCR
        let v = voltages[1] - voltages[0];
        v / self.dc_resistance()
    }
    
    fn conductance(&self, _voltages: &[f64], _temp: f64) -> Vec<f64> {
        // DC conductance is 1/DCR
        vec![1.0 / self.dc_resistance()]
    }
    
    fn num_terminals(&self) -> usize {
        2
    }
    
    fn is_nonlinear(&self) -> bool {
        // Nonlinear if saturation current is specified
        self.params.isat > 0.0 && self.params.isat < f64::INFINITY
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("l".to_string(), self.params.inductance);
        params.insert("tolerance".to_string(), self.params.tolerance);
        params.insert("dcr".to_string(), self.params.dcr);
        params.insert("isat".to_string(), self.params.isat);
        params.insert("rcore".to_string(), self.params.rcore);
        params.insert("cpar".to_string(), self.params.cpar);
        params.insert("tc1".to_string(), self.params.tc1);
        params.insert("tnom".to_string(), self.params.tnom);
        params.insert("current_rating".to_string(), self.params.current_rating);
        params.insert("q_factor".to_string(), self.params.q_factor);
        params.insert("core_type".to_string(), self.params.core_type as u8 as f64);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "l" | "inductance" => self.params.inductance = value,
            "tolerance" => self.params.tolerance = value,
            "dcr" => self.params.dcr = value,
            "isat" => self.params.isat = value,
            "rcore" => self.params.rcore = value,
            "cpar" => self.params.cpar = value,
            "tc1" => self.params.tc1 = value,
            "tnom" => self.params.tnom = value,
            "current_rating" => self.params.current_rating = value,
            "q_factor" => self.params.q_factor = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_saturation() {
        let inductor = InductorModel::from_value("test", 10e-6, "smd", 1.0);
        
        // Nominal inductance at zero current
        let l_nom = inductor.inductance_saturated(0.0);
        assert_eq!(l_nom, 10e-6);
        
        // Reduced inductance at saturation current
        let l_sat = inductor.inductance_saturated(1.0);
        assert!(l_sat < l_nom);
        assert!((l_sat - 5e-6).abs() < 1e-6); // Should be L/2 at Isat
    }
    
    #[test]
    fn test_dc_behavior() {
        let inductor = InductorModel::from_value("test", 1e-3, "power", 5.0);
        let current = inductor.current(&[0.0, 0.1], 27.0);
        
        // I = V/DCR
        let expected = 0.1 / inductor.params.dcr;
        assert!((current - expected).abs() < 1e-6);
    }
}