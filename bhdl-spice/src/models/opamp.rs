//! Operational Amplifier SPICE models

use std::collections::HashMap;
use super::{SpiceModel, ModelType};

/// Op-amp model parameters
#[derive(Debug, Clone)]
pub struct OpAmpParams {
    /// Open-loop gain (V/V)
    pub aol: f64,
    /// Gain-bandwidth product (Hz)
    pub gbw: f64,
    /// Slew rate (V/µs)
    pub slew_rate: f64,
    /// Input offset voltage (V)
    pub vos: f64,
    /// Input bias current (A)
    pub ib: f64,
    /// Input offset current (A)
    pub ios: f64,
    /// Common-mode rejection ratio (dB)
    pub cmrr: f64,
    /// Power supply rejection ratio (dB)
    pub psrr: f64,
    /// Input resistance (Ω)
    pub rin: f64,
    /// Output resistance (Ω)
    pub rout: f64,
    /// Positive supply voltage (V)
    pub vcc: f64,
    /// Negative supply voltage (V)
    pub vee: f64,
    /// Maximum output voltage positive (V)
    pub vout_max: f64,
    /// Maximum output voltage negative (V)
    pub vout_min: f64,
    /// Input common-mode range positive (V)
    pub vcm_max: f64,
    /// Input common-mode range negative (V)
    pub vcm_min: f64,
    /// Quiescent current (A)
    pub iq: f64,
}

impl Default for OpAmpParams {
    fn default() -> Self {
        Self {
            aol: 200000.0,      // 200k V/V (106 dB)
            gbw: 1e6,           // 1 MHz
            slew_rate: 0.5,     // 0.5 V/µs
            vos: 1e-3,          // 1 mV
            ib: 80e-9,          // 80 nA
            ios: 20e-9,         // 20 nA
            cmrr: 90.0,         // 90 dB
            psrr: 90.0,         // 90 dB
            rin: 2e6,           // 2 MΩ
            rout: 75.0,         // 75 Ω
            vcc: 15.0,          // +15V
            vee: -15.0,         // -15V
            vout_max: 13.5,     // Vcc - 1.5V
            vout_min: -13.5,    // Vee + 1.5V
            vcm_max: 13.0,      // Vcc - 2V
            vcm_min: -13.0,     // Vee + 2V
            iq: 1.7e-3,         // 1.7 mA
        }
    }
}

/// Common op-amp models
impl OpAmpParams {
    /// LM741 - Classic general purpose
    pub fn lm741() -> Self {
        Self {
            aol: 200000.0,
            gbw: 1e6,
            slew_rate: 0.5,
            vos: 2e-3,
            ib: 80e-9,
            ios: 20e-9,
            cmrr: 90.0,
            psrr: 90.0,
            rin: 2e6,
            rout: 75.0,
            iq: 1.7e-3,
            ..Default::default()
        }
    }
    
    /// TL072 - Low noise JFET input
    pub fn tl072() -> Self {
        Self {
            aol: 200000.0,
            gbw: 3e6,
            slew_rate: 13.0,
            vos: 3e-3,
            ib: 65e-12,      // pA range for JFET
            ios: 5e-12,
            cmrr: 100.0,
            psrr: 100.0,
            rin: 1e12,       // 1 TΩ for JFET
            rout: 75.0,
            iq: 1.4e-3,
            ..Default::default()
        }
    }
    
    /// LM358 - Single supply
    pub fn lm358() -> Self {
        Self {
            aol: 100000.0,
            gbw: 1e6,
            slew_rate: 0.5,
            vos: 2e-3,
            ib: 45e-9,
            ios: 5e-9,
            cmrr: 80.0,
            psrr: 100.0,
            rin: 2e6,
            rout: 75.0,
            vcc: 5.0,        // Single supply
            vee: 0.0,
            vout_max: 3.5,   // Vcc - 1.5V
            vout_min: 0.0,   // Ground
            vcm_max: 3.5,
            vcm_min: 0.0,
            iq: 0.7e-3,
            ..Default::default()
        }
    }
    
    /// OP07 - Precision
    pub fn op07() -> Self {
        Self {
            aol: 400000.0,
            gbw: 0.6e6,
            slew_rate: 0.3,
            vos: 30e-6,      // 30 µV ultra-low offset
            ib: 1.8e-9,
            ios: 0.4e-9,
            cmrr: 120.0,     // 120 dB
            psrr: 120.0,
            rin: 30e6,       // 30 MΩ
            rout: 60.0,
            iq: 2.7e-3,
            ..Default::default()
        }
    }
}

/// Op-amp SPICE model (simplified macro model)
pub struct OpAmpModel {
    name: String,
    params: OpAmpParams,
}

impl OpAmpModel {
    /// Create new op-amp model
    pub fn new(name: String, params: OpAmpParams) -> Self {
        Self { name, params }
    }
    
    /// Create model from preset
    pub fn from_preset(name: &str, preset: &str) -> Self {
        let params = match preset.to_lowercase().as_str() {
            "741" | "lm741" => OpAmpParams::lm741(),
            "tl072" | "tl082" => OpAmpParams::tl072(),
            "lm358" | "lm324" => OpAmpParams::lm358(),
            "op07" | "op77" => OpAmpParams::op07(),
            _ => OpAmpParams::default(),
        };
        Self::new(name.to_string(), params)
    }
    
    /// Calculate output voltage (simplified model)
    fn output_voltage(&self, vp: f64, vn: f64, vcc: f64, vee: f64) -> f64 {
        // Differential input voltage
        let vid = vp - vn;
        
        // Input offset voltage
        let vid_eff = vid - self.params.vos;
        
        // Open-loop gain (frequency dependent in reality)
        let vout_ideal = vid_eff * self.params.aol;
        
        // Output saturation
        let vout_max = vcc.min(self.params.vout_max);
        let vout_min = vee.max(self.params.vout_min);
        
        // Clamp output
        vout_ideal.max(vout_min).min(vout_max)
    }
    
    /// Calculate input currents (bias and offset)
    fn input_currents(&self) -> (f64, f64) {
        let ip = self.params.ib + self.params.ios / 2.0;
        let in_current = self.params.ib - self.params.ios / 2.0;
        (ip, in_current)
    }
}

impl Default for OpAmpModel {
    fn default() -> Self {
        Self::new("generic_opamp".to_string(), OpAmpParams::default())
    }
}

impl SpiceModel for OpAmpModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::OpAmp
    }
    
    fn current(&self, voltages: &[f64], _temp: f64) -> f64 {
        if voltages.len() < 5 {
            return 0.0;
        }
        
        // Terminals: 0=V+, 1=V-, 2=Vout, 3=Vcc, 4=Vee
        // Return supply current (simplified)
        self.params.iq
    }
    
    fn conductance(&self, _voltages: &[f64], _temp: f64) -> Vec<f64> {
        // Simplified: return input and output conductances
        vec![
            1.0 / self.params.rin,   // Input conductance
            1.0 / self.params.rout,  // Output conductance
        ]
    }
    
    fn num_terminals(&self) -> usize {
        5  // V+, V-, Vout, Vcc, Vee
    }
    
    fn is_nonlinear(&self) -> bool {
        true  // Op-amps have saturation
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("aol".to_string(), self.params.aol);
        params.insert("gbw".to_string(), self.params.gbw);
        params.insert("slew_rate".to_string(), self.params.slew_rate);
        params.insert("vos".to_string(), self.params.vos);
        params.insert("ib".to_string(), self.params.ib);
        params.insert("ios".to_string(), self.params.ios);
        params.insert("cmrr".to_string(), self.params.cmrr);
        params.insert("psrr".to_string(), self.params.psrr);
        params.insert("rin".to_string(), self.params.rin);
        params.insert("rout".to_string(), self.params.rout);
        params.insert("vcc".to_string(), self.params.vcc);
        params.insert("vee".to_string(), self.params.vee);
        params.insert("vout_max".to_string(), self.params.vout_max);
        params.insert("vout_min".to_string(), self.params.vout_min);
        params.insert("vcm_max".to_string(), self.params.vcm_max);
        params.insert("vcm_min".to_string(), self.params.vcm_min);
        params.insert("iq".to_string(), self.params.iq);
        params.insert("sr".to_string(), self.params.slew_rate * 1e6);  // Also provide as V/s
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "aol" => self.params.aol = value,
            "gbw" => self.params.gbw = value,
            "slew_rate" => self.params.slew_rate = value,
            "vos" => self.params.vos = value,
            "ib" => self.params.ib = value,
            "ios" => self.params.ios = value,
            "cmrr" => self.params.cmrr = value,
            "psrr" => self.params.psrr = value,
            "rin" => self.params.rin = value,
            "rout" => self.params.rout = value,
            "vcc" => self.params.vcc = value,
            "vee" => self.params.vee = value,
            "vout_max" => self.params.vout_max = value,
            "vout_min" => self.params.vout_min = value,
            "vcm_max" => self.params.vcm_max = value,
            "vcm_min" => self.params.vcm_min = value,
            "iq" => self.params.iq = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_output_saturation() {
        let opamp = OpAmpModel::from_preset("test", "lm741");
        
        // Large positive input should saturate
        let vout_pos = opamp.output_voltage(1.0, 0.0, 15.0, -15.0);
        assert!(vout_pos > 13.0);
        
        // Large negative input should saturate
        let vout_neg = opamp.output_voltage(0.0, 1.0, 15.0, -15.0);
        assert!(vout_neg < -13.0);
    }
}