//! Diode SPICE models with temperature effects

use std::collections::HashMap;
use super::{SpiceModel, ModelType, thermal_voltage, clamp_exp};

/// Diode model parameters (SPICE compatible)
#[derive(Debug, Clone)]
pub struct DiodeParams {
    /// Saturation current (A)
    pub is: f64,
    /// Emission coefficient
    pub n: f64,
    /// Series resistance (Ω)
    pub rs: f64,
    /// Transit time (s)
    pub tt: f64,
    /// Zero-bias junction capacitance (F)
    pub cjo: f64,
    /// Junction potential (V)
    pub vj: f64,
    /// Grading coefficient
    pub m: f64,
    /// Breakdown voltage (V)
    pub bv: Option<f64>,
    /// Breakdown current (A)
    pub ibv: f64,
    /// Breakdown knee exponent
    pub nbv: f64,
    /// Energy gap (eV)
    pub eg: f64,
    /// Saturation current temperature exponent
    pub xti: f64,
    /// Nominal temperature (°C)
    pub tnom: f64,
    /// Flicker noise coefficient
    pub kf: f64,
    /// Flicker noise exponent
    pub af: f64,
}

impl Default for DiodeParams {
    fn default() -> Self {
        Self {
            is: 1e-14,      // 10 fA
            n: 1.0,         // Ideal diode
            rs: 0.0,        // No series resistance
            tt: 0.0,        // No transit time
            cjo: 0.0,       // No junction capacitance
            vj: 0.7,        // Silicon junction
            m: 0.5,         // Square root junction
            bv: None,       // No breakdown
            ibv: 1e-3,      // 1 mA breakdown current
            nbv: 3.0,       // Breakdown knee exponent
            eg: 1.11,       // Silicon bandgap
            xti: 3.0,       // Temperature exponent
            tnom: 27.0,     // Room temperature
            kf: 0.0,        // No flicker noise
            af: 1.0,        // Flicker noise exponent
        }
    }
}

/// Common diode models
impl DiodeParams {
    /// 1N4148 small signal diode
    pub fn n1n4148() -> Self {
        Self {
            is: 2.682e-9,
            n: 1.836,
            rs: 0.5664,
            tt: 11.54e-9,
            cjo: 4e-12,
            vj: 0.5,
            m: 0.333,
            bv: Some(100.0),
            ibv: 100e-6,
            ..Default::default()
        }
    }
    
    /// 1N4007 rectifier diode
    pub fn n1n4007() -> Self {
        Self {
            is: 7.625e-9,
            n: 1.803,
            rs: 0.04207,
            tt: 4.32e-6,
            cjo: 14.11e-12,
            vj: 0.6561,
            m: 0.3554,
            bv: Some(1000.0),
            ibv: 5e-6,
            ..Default::default()
        }
    }
    
    /// Red LED model
    pub fn led_red() -> Self {
        Self {
            is: 1e-20,
            n: 2.0,
            rs: 10.0,
            vj: 2.0,  // Red LED forward voltage
            bv: Some(5.0),
            ibv: 10e-6,
            ..Default::default()
        }
    }
    
    /// Green LED model
    pub fn led_green() -> Self {
        Self {
            is: 1e-21,
            n: 2.0,
            rs: 12.0,
            vj: 2.2,  // Green LED forward voltage
            bv: Some(5.0),
            ibv: 10e-6,
            ..Default::default()
        }
    }
    
    /// Blue LED model
    pub fn led_blue() -> Self {
        Self {
            is: 1e-22,
            n: 2.0,
            rs: 15.0,
            vj: 3.2,  // Blue LED forward voltage
            bv: Some(5.0),
            ibv: 10e-6,
            ..Default::default()
        }
    }
    
    /// Schottky diode model
    pub fn schottky() -> Self {
        Self {
            is: 1e-8,
            n: 1.05,
            rs: 0.1,
            vj: 0.3,  // Lower forward voltage
            cjo: 50e-12,
            m: 0.4,
            ..Default::default()
        }
    }
}

/// Diode SPICE model with full Shockley equation
pub struct DiodeModel {
    name: String,
    params: DiodeParams,
}

impl DiodeModel {
    /// Create new diode model
    pub fn new(name: String, params: DiodeParams) -> Self {
        Self { name, params }
    }
    
    /// Create model from preset
    pub fn from_preset(name: &str, preset: &str) -> Self {
        let params = match preset.to_lowercase().as_str() {
            "1n4148" => DiodeParams::n1n4148(),
            "1n4007" => DiodeParams::n1n4007(),
            "led_red" => DiodeParams::led_red(),
            "schottky" => DiodeParams::schottky(),
            _ => DiodeParams::default(),
        };
        Self::new(name.to_string(), params)
    }
    
    /// Temperature-adjusted saturation current
    fn is_temp(&self, temp: f64) -> f64 {
        let tnom_k = self.params.tnom + 273.15;
        let temp_k = temp + 273.15;
        let vt_nom = thermal_voltage(self.params.tnom);
        let vt = thermal_voltage(temp);
        
        // Temperature adjustment: IS(T) = IS * (T/Tnom)^(XTI/N) * exp((Eg/N) * (1/VTnom - 1/VT))
        let temp_ratio = temp_k / tnom_k;
        let vt_factor = (self.params.eg / self.params.n) * (1.0 / vt_nom - 1.0 / vt);
        
        self.params.is * temp_ratio.powf(self.params.xti / self.params.n) * vt_factor.exp()
    }
    
    /// Calculate diode current using Shockley equation
    fn diode_current(&self, vd: f64, temp: f64) -> f64 {
        let vt = thermal_voltage(temp);
        let is_t = self.is_temp(temp);
        
        // Account for series resistance iteration if needed
        let vd_junction = if self.params.rs > 0.0 {
            // Simple approximation - for accurate results need iterative solution
            vd // TODO: Implement Newton-Raphson for self-consistent solution
        } else {
            vd
        };
        
        // Check for breakdown
        if let Some(bv) = self.params.bv {
            if vd_junction < -bv {
                // More realistic breakdown model using a softer transition
                // I = -IBV * [(V/BV)^m - 1] for V < -BV
                // This gives a more gradual increase in reverse current
                let v_ratio = vd_junction / -bv;
                let m = self.params.nbv; // Breakdown exponent
                
                // For numerical stability, limit the ratio
                let v_ratio_clamped = v_ratio.min(10.0);
                let breakdown_current = -self.params.ibv * (v_ratio_clamped.powf(m) - 1.0);
                return breakdown_current;
            }
        }
        
        // Normal Shockley equation: I = IS * (exp(V/(n*VT)) - 1)
        let exp_arg = clamp_exp(vd_junction / (self.params.n * vt), 40.0);
        is_t * (exp_arg.exp() - 1.0)
    }
    
    /// Calculate dynamic conductance (di/dv)
    fn diode_conductance(&self, vd: f64, temp: f64) -> f64 {
        let vt = thermal_voltage(temp);
        let is_t = self.is_temp(temp);
        
        // Check for breakdown
        if let Some(bv) = self.params.bv {
            if vd < -bv {
                // Breakdown conductance: d/dV of breakdown current
                let v_ratio = vd / -bv;
                let m = self.params.nbv; // Same exponent as current calculation
                
                // Limit for numerical stability
                let v_ratio_clamped = v_ratio.min(10.0);
                
                // g = d/dV[-IBV * (V/BV)^m] = IBV * m * (V/BV)^(m-1) / BV
                return self.params.ibv * m * v_ratio_clamped.powf(m - 1.0) / bv;
            }
        }
        
        // Normal region conductance: g = (IS/(n*VT)) * exp(V/(n*VT))
        let exp_arg = clamp_exp(vd / (self.params.n * vt), 40.0);
        (is_t / (self.params.n * vt)) * exp_arg.exp()
    }
}

impl Default for DiodeModel {
    fn default() -> Self {
        Self::new("generic_diode".to_string(), DiodeParams::default())
    }
}

impl SpiceModel for DiodeModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::Diode
    }
    
    fn current(&self, voltages: &[f64], temp: f64) -> f64 {
        if voltages.len() != 2 {
            return 0.0;
        }
        let vd = voltages[1] - voltages[0]; // Anode - Cathode
        self.diode_current(vd, temp)
    }
    
    fn conductance(&self, voltages: &[f64], temp: f64) -> Vec<f64> {
        if voltages.len() != 2 {
            return vec![0.0];
        }
        let vd = voltages[1] - voltages[0];
        let g = self.diode_conductance(vd, temp);
        
        // Add series resistance effect
        if self.params.rs > 0.0 {
            let g_total = 1.0 / (1.0 / g + self.params.rs);
            vec![g_total]
        } else {
            vec![g]
        }
    }
    
    fn num_terminals(&self) -> usize {
        2
    }
    
    fn is_nonlinear(&self) -> bool {
        true
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("is".to_string(), self.params.is);
        params.insert("n".to_string(), self.params.n);
        params.insert("rs".to_string(), self.params.rs);
        params.insert("tt".to_string(), self.params.tt);
        params.insert("cjo".to_string(), self.params.cjo);
        params.insert("vj".to_string(), self.params.vj);
        params.insert("m".to_string(), self.params.m);
        if let Some(bv) = self.params.bv {
            params.insert("bv".to_string(), bv);
        }
        params.insert("ibv".to_string(), self.params.ibv);
        params.insert("eg".to_string(), self.params.eg);
        params.insert("xti".to_string(), self.params.xti);
        params.insert("tnom".to_string(), self.params.tnom);
        params.insert("kf".to_string(), self.params.kf);
        params.insert("af".to_string(), self.params.af);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "is" => self.params.is = value,
            "n" => self.params.n = value,
            "rs" => self.params.rs = value,
            "tt" => self.params.tt = value,
            "cjo" => self.params.cjo = value,
            "vj" => self.params.vj = value,
            "m" => self.params.m = value,
            "bv" => self.params.bv = Some(value),
            "ibv" => self.params.ibv = value,
            "eg" => self.params.eg = value,
            "xti" => self.params.xti = value,
            "tnom" => self.params.tnom = value,
            "kf" => self.params.kf = value,
            "af" => self.params.af = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_diode_forward() {
        let diode = DiodeModel::default();
        let current = diode.current(&[0.0, 0.7], 27.0);
        assert!(current > 1e-3); // Should conduct at 0.7V
    }
    
    #[test]
    fn test_diode_reverse() {
        let diode = DiodeModel::default();
        let current = diode.current(&[0.0, -5.0], 27.0);
        assert!(current.abs() < 1e-12); // Should be near saturation current
    }
    
    #[test]
    fn test_temperature_effect() {
        let diode = DiodeModel::from_preset("test", "1n4148");
        let i_25 = diode.current(&[0.0, 0.6], 25.0);
        let i_85 = diode.current(&[0.0, 0.6], 85.0);
        assert!(i_85 > i_25); // Current increases with temperature
    }
}