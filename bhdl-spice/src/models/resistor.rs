//! Resistor SPICE models with temperature and noise effects

use std::collections::HashMap;
use super::{SpiceModel, ModelType, DEFAULT_TEMPERATURE};

/// Resistor model parameters
#[derive(Debug, Clone)]
pub struct ResistorParams {
    /// Nominal resistance (Ω)
    pub resistance: f64,
    /// Tolerance (%)
    pub tolerance: f64,
    /// Temperature coefficient 1 (ppm/°C)
    pub tc1: f64,
    /// Temperature coefficient 2 (ppm/°C²)
    pub tc2: f64,
    /// Nominal temperature (°C)
    pub tnom: f64,
    /// Power rating (W)
    pub power_rating: f64,
    /// Voltage rating (V)
    pub voltage_rating: f64,
    /// Noise coefficient
    pub noise_coeff: f64,
}

impl Default for ResistorParams {
    fn default() -> Self {
        Self {
            resistance: 1000.0,  // 1kΩ
            tolerance: 5.0,      // 5%
            tc1: 0.0,           // No temperature coefficient
            tc2: 0.0,
            tnom: DEFAULT_TEMPERATURE,
            power_rating: 0.25,  // 1/4W
            voltage_rating: 250.0,
            noise_coeff: 1.0,    // Standard thermal noise
        }
    }
}

/// Common resistor types
impl ResistorParams {
    /// Metal film resistor (low noise, good stability)
    pub fn metal_film(resistance: f64) -> Self {
        Self {
            resistance,
            tolerance: 1.0,
            tc1: 50.0,  // 50 ppm/°C
            tc2: 0.0,
            noise_coeff: 0.1,  // Low noise
            ..Default::default()
        }
    }
    
    /// Carbon film resistor (general purpose)
    pub fn carbon_film(resistance: f64) -> Self {
        Self {
            resistance,
            tolerance: 5.0,
            tc1: -200.0,  // -200 ppm/°C
            tc2: 0.0,
            noise_coeff: 1.0,
            ..Default::default()
        }
    }
    
    /// Wire wound resistor (high power, inductance)
    pub fn wire_wound(resistance: f64, power: f64) -> Self {
        Self {
            resistance,
            tolerance: 1.0,
            tc1: 20.0,  // 20 ppm/°C
            tc2: 0.0,
            power_rating: power,
            voltage_rating: (power * resistance).sqrt() * 2.0,
            noise_coeff: 0.05,  // Very low noise
            ..Default::default()
        }
    }
    
    /// SMD resistor by package
    pub fn smd(resistance: f64, package: &str) -> Self {
        let (power, tc1) = match package {
            "0402" => (0.063, 100.0),
            "0603" => (0.1, 100.0),
            "0805" => (0.125, 100.0),
            "1206" => (0.25, 100.0),
            "2010" => (0.75, 50.0),
            "2512" => (1.0, 50.0),
            _ => (0.125, 100.0),
        };
        
        Self {
            resistance,
            tolerance: 1.0,
            tc1,
            tc2: 0.0,
            power_rating: power,
            voltage_rating: (power * resistance).sqrt() * 2.0,
            noise_coeff: 0.5,
            ..Default::default()
        }
    }
}

/// Resistor SPICE model
pub struct ResistorModel {
    name: String,
    params: ResistorParams,
}

impl ResistorModel {
    /// Create new resistor model
    pub fn new(name: String, params: ResistorParams) -> Self {
        Self { name, params }
    }
    
    /// Create model from value and type
    pub fn from_value(name: &str, resistance: f64, resistor_type: &str) -> Self {
        let params = match resistor_type.to_lowercase().as_str() {
            "metal_film" => ResistorParams::metal_film(resistance),
            "carbon_film" => ResistorParams::carbon_film(resistance),
            "wire_wound" => ResistorParams::wire_wound(resistance, 5.0),
            "0402" | "0603" | "0805" | "1206" => ResistorParams::smd(resistance, resistor_type),
            _ => ResistorParams {
                resistance,
                ..Default::default()
            },
        };
        Self::new(name.to_string(), params)
    }
    
    /// Calculate temperature-adjusted resistance
    fn resistance_temp(&self, temp: f64) -> f64 {
        let temp_delta = temp - self.params.tnom;
        let tc1_factor = 1.0 + self.params.tc1 * 1e-6 * temp_delta;
        let tc2_factor = 1.0 + self.params.tc2 * 1e-6 * temp_delta * temp_delta;
        
        self.params.resistance * tc1_factor * tc2_factor
    }
    
    /// Calculate thermal noise voltage (RMS)
    fn thermal_noise_voltage(&self, temp: f64, bandwidth: f64) -> f64 {
        // Thermal noise: Vn = sqrt(4*k*T*R*B)
        let k = 1.38064852e-23; // Boltzmann constant
        let temp_k = temp + 273.15;
        let r = self.resistance_temp(temp);
        
        (4.0 * k * temp_k * r * bandwidth).sqrt() * self.params.noise_coeff
    }
    
    /// Check power dissipation limits
    fn check_power_limit(&self, voltage: f64, current: f64) -> bool {
        let power = voltage.abs() * current.abs();
        power <= self.params.power_rating
    }
    
    /// Check voltage rating
    fn check_voltage_limit(&self, voltage: f64) -> bool {
        voltage.abs() <= self.params.voltage_rating
    }
}

impl Default for ResistorModel {
    fn default() -> Self {
        Self::new("generic_resistor".to_string(), ResistorParams::default())
    }
}

impl SpiceModel for ResistorModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::Resistor
    }
    
    fn current(&self, voltages: &[f64], temp: f64) -> f64 {
        if voltages.len() != 2 {
            return 0.0;
        }
        
        let v = voltages[1] - voltages[0];
        let r = self.resistance_temp(temp);
        
        // Ohm's law: I = V/R
        v / r
    }
    
    fn conductance(&self, _voltages: &[f64], temp: f64) -> Vec<f64> {
        let r = self.resistance_temp(temp);
        vec![1.0 / r]
    }
    
    fn num_terminals(&self) -> usize {
        2
    }
    
    fn is_nonlinear(&self) -> bool {
        false  // Resistors are linear (ignoring temperature during analysis)
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("r".to_string(), self.params.resistance);
        params.insert("tolerance".to_string(), self.params.tolerance);
        params.insert("tc1".to_string(), self.params.tc1);
        params.insert("tc2".to_string(), self.params.tc2);
        params.insert("tnom".to_string(), self.params.tnom);
        params.insert("power_rating".to_string(), self.params.power_rating);
        params.insert("voltage_rating".to_string(), self.params.voltage_rating);
        params.insert("noise_coeff".to_string(), self.params.noise_coeff);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "r" | "resistance" => self.params.resistance = value,
            "tolerance" => self.params.tolerance = value,
            "tc1" => self.params.tc1 = value,
            "tc2" => self.params.tc2 = value,
            "tnom" => self.params.tnom = value,
            "power_rating" => self.params.power_rating = value,
            "voltage_rating" => self.params.voltage_rating = value,
            "noise_coeff" => self.params.noise_coeff = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_temperature_coefficient() {
        let mut resistor = ResistorModel::from_value("test", 1000.0, "metal_film");
        
        // At nominal temperature
        let r_nom = resistor.resistance_temp(27.0);
        assert_eq!(r_nom, 1000.0);
        
        // At elevated temperature (50°C above nominal)
        let r_hot = resistor.resistance_temp(77.0);
        assert!(r_hot > r_nom); // Metal film has positive TC
        assert!((r_hot - 1002.5).abs() < 0.1); // Should be ~1002.5Ω
    }
    
    #[test]
    fn test_ohms_law() {
        let resistor = ResistorModel::from_value("test", 100.0, "carbon_film");
        let current = resistor.current(&[0.0, 10.0], 27.0);
        assert_eq!(current, 0.1); // 10V / 100Ω = 0.1A
    }
}