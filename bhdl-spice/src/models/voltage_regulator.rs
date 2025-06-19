//! Voltage Regulator SPICE models (7805, LM317, etc.)

use std::collections::HashMap;
use super::{SpiceModel, ModelType};

/// Voltage regulator types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RegulatorType {
    Fixed,      // Fixed output (78xx series)
    Adjustable, // Adjustable output (LM317)
}

/// Voltage regulator model parameters
#[derive(Debug, Clone)]
pub struct VoltageRegulatorParams {
    /// Regulator type
    pub reg_type: RegulatorType,
    /// Nominal output voltage (V) - for fixed regulators
    pub vout_nom: f64,
    /// Minimum output voltage (V) - for adjustable regulators
    pub vout_min: f64,
    /// Maximum output voltage (V) - for adjustable regulators
    pub vout_max: f64,
    /// Reference voltage (V) - for adjustable regulators
    pub vref: f64,
    /// Dropout voltage (V)
    pub dropout: f64,
    /// Maximum output current (A)
    pub iout_max: f64,
    /// Quiescent current (A)
    pub iq: f64,
    /// Ground pin current (A) - typically iq + iout/ratio
    pub ignd_ratio: f64,
    /// Load regulation (%/A)
    pub load_reg: f64,
    /// Line regulation (%/V)
    pub line_reg: f64,
    /// Output resistance (Ω)
    pub rout: f64,
    /// Thermal resistance (°C/W)
    pub rth: f64,
    /// Maximum junction temperature (°C)
    pub tj_max: f64,
    /// Temperature coefficient (V/°C)
    pub tc: f64,
    /// Adjustment pin current (A) - for LM317
    pub iadj: f64,
    /// Minimum load current (A) - some regulators need minimum load
    pub iload_min: f64,
    /// Power supply rejection ratio (dB)
    pub psrr: f64,
    /// Output noise voltage (µV RMS)
    pub vnoise: f64,
    /// Nominal temperature (°C)
    pub tnom: f64,
}

impl Default for VoltageRegulatorParams {
    fn default() -> Self {
        Self {
            reg_type: RegulatorType::Fixed,
            vout_nom: 5.0,
            vout_min: 1.25,
            vout_max: 37.0,
            vref: 1.25,
            dropout: 2.0,
            iout_max: 1.0,
            iq: 5e-3,
            ignd_ratio: 0.01,  // Ignd ≈ Iq + Iout/100
            load_reg: 0.01,    // 1%/A
            line_reg: 0.001,   // 0.1%/V
            rout: 0.01,
            rth: 65.0,         // TO-220 typical
            tj_max: 125.0,
            tc: -1e-3,         // -1mV/°C
            iadj: 50e-6,       // 50µA for LM317
            iload_min: 5e-3,   // 5mA minimum load
            psrr: 80.0,        // 80dB
            vnoise: 40.0,      // 40µV RMS
            tnom: 27.0,
        }
    }
}

/// Common voltage regulator models
impl VoltageRegulatorParams {
    /// 7805 5V fixed regulator
    pub fn lm7805() -> Self {
        Self {
            reg_type: RegulatorType::Fixed,
            vout_nom: 5.0,
            dropout: 2.0,
            iout_max: 1.0,
            iq: 5e-3,
            load_reg: 0.005,   // 0.5%/A
            line_reg: 0.0001,  // 0.01%/V
            rout: 0.017,       // 17mΩ typical
            psrr: 73.0,        // 73dB typical
            ..Default::default()
        }
    }
    
    /// 7812 12V fixed regulator
    pub fn lm7812() -> Self {
        Self {
            reg_type: RegulatorType::Fixed,
            vout_nom: 12.0,
            dropout: 2.0,
            iout_max: 1.0,
            iq: 5e-3,
            load_reg: 0.005,
            line_reg: 0.0001,
            rout: 0.018,
            psrr: 71.0,
            ..Default::default()
        }
    }
    
    /// LM317 adjustable regulator
    pub fn lm317() -> Self {
        Self {
            reg_type: RegulatorType::Adjustable,
            vout_nom: 1.25,    // Vref
            vout_min: 1.25,
            vout_max: 37.0,
            vref: 1.25,
            dropout: 3.0,      // Higher dropout than 78xx
            iout_max: 1.5,
            iq: 5e-3,
            iadj: 50e-6,
            load_reg: 0.001,   // 0.1%/A - better than 78xx
            line_reg: 0.00002, // 0.002%/V - excellent
            rout: 0.028,
            iload_min: 10e-3,  // Needs 10mA minimum
            psrr: 80.0,
            ..Default::default()
        }
    }
    
    /// LM1117 3.3V low-dropout regulator
    pub fn lm1117_3v3() -> Self {
        Self {
            reg_type: RegulatorType::Fixed,
            vout_nom: 3.3,
            dropout: 1.2,      // Low dropout
            iout_max: 0.8,
            iq: 5e-3,
            load_reg: 0.002,
            line_reg: 0.00005,
            rout: 0.2,         // Higher than 78xx
            psrr: 75.0,
            ..Default::default()
        }
    }
}

/// Voltage regulator SPICE model
pub struct VoltageRegulatorModel {
    name: String,
    params: VoltageRegulatorParams,
    // State for adjustable regulators
    r1: f64,  // Upper feedback resistor
    r2: f64,  // Lower feedback resistor
}

impl VoltageRegulatorModel {
    /// Create new voltage regulator model
    pub fn new(name: String, params: VoltageRegulatorParams) -> Self {
        Self { 
            name, 
            params,
            r1: 240.0,   // Default R1 for LM317
            r2: 1000.0,  // Default R2 for ~6.25V output
        }
    }
    
    /// Create model from preset
    pub fn from_preset(name: &str, preset: &str) -> Self {
        let params = match preset.to_lowercase().as_str() {
            "7805" | "lm7805" => VoltageRegulatorParams::lm7805(),
            "7812" | "lm7812" => VoltageRegulatorParams::lm7812(),
            "lm317" => VoltageRegulatorParams::lm317(),
            "lm1117-3.3" | "lm1117_3v3" => VoltageRegulatorParams::lm1117_3v3(),
            _ => VoltageRegulatorParams::default(),
        };
        Self::new(name.to_string(), params)
    }
    
    /// Set feedback resistors for adjustable regulator
    pub fn set_feedback(&mut self, r1: f64, r2: f64) {
        self.r1 = r1;
        self.r2 = r2;
    }
    
    /// Calculate output voltage for adjustable regulator
    fn calculate_vout_adjustable(&self) -> f64 {
        // Vout = Vref * (1 + R2/R1) + Iadj * R2
        self.params.vref * (1.0 + self.r2 / self.r1) + self.params.iadj * self.r2
    }
    
    /// Calculate regulated output voltage
    fn regulated_voltage(&self, vin: f64, iout: f64, temp: f64) -> f64 {
        // Base output voltage
        let vout_base = match self.params.reg_type {
            RegulatorType::Fixed => self.params.vout_nom,
            RegulatorType::Adjustable => self.calculate_vout_adjustable(),
        };
        
        // Apply temperature coefficient
        let temp_delta = temp - self.params.tnom;
        let vout_temp = vout_base + self.params.tc * temp_delta;
        
        // Apply load regulation
        let vout_load = vout_temp * (1.0 - self.params.load_reg * iout);
        
        // Apply line regulation
        let vin_nom = vout_base + self.params.dropout + 3.0; // Nominal input
        let line_delta = (vin - vin_nom).max(0.0);
        let vout_line = vout_load * (1.0 + self.params.line_reg * line_delta);
        
        // Check dropout condition
        let vout_max_dropout = vin - self.params.dropout;
        vout_line.min(vout_max_dropout).max(0.0)
    }
    
    /// Calculate ground pin current
    fn ground_current(&self, iout: f64) -> f64 {
        // Ground current = quiescent current + portion of output current
        self.params.iq + iout * self.params.ignd_ratio
    }
    
    /// Check thermal limits
    fn check_thermal(&self, vin: f64, vout: f64, iout: f64, tamb: f64) -> bool {
        let power_dissipation = (vin - vout) * iout;
        let tj = tamb + power_dissipation * self.params.rth;
        tj <= self.params.tj_max
    }
}

impl Default for VoltageRegulatorModel {
    fn default() -> Self {
        Self::new("generic_vreg".to_string(), VoltageRegulatorParams::default())
    }
}

impl SpiceModel for VoltageRegulatorModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::VoltageRegulator
    }
    
    fn current(&self, voltages: &[f64], temp: f64) -> f64 {
        if voltages.len() < 3 {
            return 0.0;
        }
        
        // For DC analysis, return ground pin current
        // Terminals: 0=IN, 1=OUT, 2=GND, (3=ADJ for adjustable)
        let vin = voltages[0] - voltages[2];  // Input voltage
        let vout = voltages[1] - voltages[2]; // Output voltage
        
        // Estimate output current from voltage drop across output resistance
        let iout = if self.params.rout > 0.0 {
            (self.regulated_voltage(vin, 0.0, temp) - vout) / self.params.rout
        } else {
            0.0
        };
        
        // Return ground pin current
        self.ground_current(iout.max(0.0))
    }
    
    fn conductance(&self, voltages: &[f64], temp: f64) -> Vec<f64> {
        // For voltage regulators, we return the output conductance
        // This helps convergence in iterative solvers
        vec![1.0 / self.params.rout]
    }
    
    fn num_terminals(&self) -> usize {
        match self.params.reg_type {
            RegulatorType::Fixed => 3,      // IN, OUT, GND
            RegulatorType::Adjustable => 4,  // IN, OUT, GND, ADJ
        }
    }
    
    fn is_nonlinear(&self) -> bool {
        true  // Voltage regulators are highly nonlinear
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("type".to_string(), match self.params.reg_type {
            RegulatorType::Fixed => 0.0,
            RegulatorType::Adjustable => 1.0,
        });
        params.insert("vout_nom".to_string(), self.params.vout_nom);
        params.insert("dropout".to_string(), self.params.dropout);
        params.insert("iout_max".to_string(), self.params.iout_max);
        params.insert("iq".to_string(), self.params.iq);
        params.insert("load_reg".to_string(), self.params.load_reg);
        params.insert("line_reg".to_string(), self.params.line_reg);
        params.insert("rout".to_string(), self.params.rout);
        params.insert("psrr".to_string(), self.params.psrr);
        params.insert("vref".to_string(), self.params.vref);
        params.insert("iadj".to_string(), self.params.iadj);
        params.insert("r1".to_string(), self.r1);
        params.insert("r2".to_string(), self.r2);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "vout_nom" => self.params.vout_nom = value,
            "dropout" => self.params.dropout = value,
            "iout_max" => self.params.iout_max = value,
            "iq" => self.params.iq = value,
            "load_reg" => self.params.load_reg = value,
            "line_reg" => self.params.line_reg = value,
            "rout" => self.params.rout = value,
            "r1" => self.r1 = value,
            "r2" => self.r2 = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_7805_regulation() {
        let vreg = VoltageRegulatorModel::from_preset("test", "7805");
        
        // Test with good input voltage
        let vout = vreg.regulated_voltage(8.0, 0.1, 27.0);
        assert!((vout - 5.0).abs() < 0.05); // Should be close to 5V
        
        // Test dropout
        let vout_dropout = vreg.regulated_voltage(6.0, 0.1, 27.0);
        assert!(vout_dropout < 4.5); // Should be in dropout
    }
    
    #[test]
    fn test_lm317_adjustable() {
        let mut vreg = VoltageRegulatorModel::from_preset("test", "lm317");
        vreg.set_feedback(240.0, 1500.0); // Should give ~9V
        
        let vout = vreg.calculate_vout_adjustable();
        assert!((vout - 9.0).abs() < 0.5); // Approximately 9V
    }
}