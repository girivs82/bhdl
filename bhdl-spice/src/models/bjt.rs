//! Bipolar Junction Transistor (BJT) SPICE models

use std::collections::HashMap;
use super::{SpiceModel, ModelType, thermal_voltage, clamp_exp};

/// BJT type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum BjtType {
    NPN,
    PNP,
}

/// BJT model parameters (Ebers-Moll and Gummel-Poon compatible)
#[derive(Debug, Clone)]
pub struct BjtParams {
    /// Transistor type
    pub bjt_type: BjtType,
    /// Forward current gain
    pub bf: f64,
    /// Reverse current gain
    pub br: f64,
    /// Saturation current (A)
    pub is: f64,
    /// Forward emission coefficient
    pub nf: f64,
    /// Reverse emission coefficient
    pub nr: f64,
    /// Base resistance (Ω)
    pub rb: f64,
    /// Collector resistance (Ω)
    pub rc: f64,
    /// Emitter resistance (Ω)
    pub re: f64,
    /// Forward Early voltage (V)
    pub vaf: f64,
    /// Reverse Early voltage (V)
    pub var: f64,
    /// Base-emitter junction capacitance (F)
    pub cje: f64,
    /// Base-emitter junction potential (V)
    pub vje: f64,
    /// Base-emitter grading coefficient
    pub mje: f64,
    /// Base-collector junction capacitance (F)
    pub cjc: f64,
    /// Base-collector junction potential (V)
    pub vjc: f64,
    /// Base-collector grading coefficient
    pub mjc: f64,
    /// Forward transit time (s)
    pub tf: f64,
    /// Reverse transit time (s)
    pub tr: f64,
    /// Energy gap (eV)
    pub eg: f64,
    /// Temperature exponent for IS
    pub xti: f64,
    /// Nominal temperature (°C)
    pub tnom: f64,
}

impl Default for BjtParams {
    fn default() -> Self {
        Self {
            bjt_type: BjtType::NPN,
            bf: 100.0,
            br: 1.0,
            is: 1e-16,
            nf: 1.0,
            nr: 1.0,
            rb: 0.0,
            rc: 0.0,
            re: 0.0,
            vaf: f64::INFINITY,
            var: f64::INFINITY,
            cje: 0.0,
            vje: 0.75,
            mje: 0.33,
            cjc: 0.0,
            vjc: 0.75,
            mjc: 0.33,
            tf: 0.0,
            tr: 0.0,
            eg: 1.11,
            xti: 3.0,
            tnom: 27.0,
        }
    }
}

/// Common BJT models
impl BjtParams {
    /// 2N2222 NPN general purpose
    pub fn n2n2222() -> Self {
        Self {
            bjt_type: BjtType::NPN,
            bf: 255.9,
            br: 6.092,
            is: 14.34e-15,
            nf: 1.307,
            nr: 1.0,
            rb: 10.0,
            rc: 1.0,
            re: 0.0,
            vaf: 74.03,
            var: f64::INFINITY,
            cje: 22.01e-12,
            vje: 0.6531,
            mje: 0.377,
            cjc: 7.306e-12,
            vjc: 0.3416,
            mjc: 0.23,
            tf: 411.1e-12,
            tr: 46.91e-9,
            ..Default::default()
        }
    }
    
    /// BC547 NPN small signal
    pub fn bc547() -> Self {
        Self {
            bjt_type: BjtType::NPN,
            bf: 373.5,
            br: 4.78,
            is: 13.09e-15,
            nf: 1.025,
            nr: 1.0,
            rb: 277.2,
            rc: 1.0,
            re: 0.665,
            vaf: 109.5,
            var: 20.0,
            cje: 13.41e-12,
            vje: 0.732,
            mje: 0.346,
            cjc: 4.64e-12,
            vjc: 0.574,
            mjc: 0.27,
            tf: 0.641e-9,
            tr: 10e-9,
            ..Default::default()
        }
    }
    
    /// 2N3906 PNP general purpose
    pub fn n2n3906() -> Self {
        Self {
            bjt_type: BjtType::PNP,
            bf: 201.2,
            br: 3.73,
            is: 1.41e-15,
            nf: 1.094,
            nr: 1.0,
            rb: 10.0,
            rc: 2.5,
            re: 0.0,
            vaf: 18.7,
            var: f64::INFINITY,
            cje: 9.728e-12,
            vje: 0.719,
            mje: 0.34,
            cjc: 5.699e-12,
            vjc: 0.425,
            mjc: 0.21,
            tf: 239.5e-12,
            tr: 33.42e-9,
            ..Default::default()
        }
    }
}

/// BJT SPICE model using Ebers-Moll equations
pub struct BjtModel {
    name: String,
    params: BjtParams,
}

impl BjtModel {
    /// Create new BJT model
    pub fn new(name: String, params: BjtParams) -> Self {
        Self { name, params }
    }
    
    /// Create model from preset
    pub fn from_preset(name: &str, preset: &str) -> Self {
        let params = match preset.to_lowercase().as_str() {
            "2n2222" => BjtParams::n2n2222(),
            "bc547" => BjtParams::bc547(),
            "2n3906" => BjtParams::n2n3906(),
            _ => BjtParams::default(),
        };
        Self::new(name.to_string(), params)
    }
    
    /// Temperature-adjusted saturation current
    fn is_temp(&self, temp: f64) -> f64 {
        let tnom_k = self.params.tnom + 273.15;
        let temp_k = temp + 273.15;
        let vt_nom = thermal_voltage(self.params.tnom);
        let vt = thermal_voltage(temp);
        
        let temp_ratio = temp_k / tnom_k;
        let vt_factor = self.params.eg * (1.0 / vt_nom - 1.0 / vt);
        
        self.params.is * temp_ratio.powf(self.params.xti) * vt_factor.exp()
    }
    
    /// Calculate BJT currents using Ebers-Moll model
    /// Returns (Ic, Ib, Ie) given Vbe and Vce
    fn bjt_currents(&self, vbe: f64, vce: f64, temp: f64) -> (f64, f64, f64) {
        let vt = thermal_voltage(temp);
        let is_t = self.is_temp(temp);
        
        // Calculate Vbc from Vbe and Vce
        let vbc = vbe - vce;
        
        // Forward and reverse currents
        let if_current = is_t * (clamp_exp(vbe / (self.params.nf * vt), 40.0).exp() - 1.0);
        let ir_current = is_t * (clamp_exp(vbc / (self.params.nr * vt), 40.0).exp() - 1.0);
        
        // Early effect
        let early_factor = if vce > 0.0 && self.params.vaf.is_finite() {
            1.0 + vce / self.params.vaf
        } else if vce < 0.0 && self.params.var.is_finite() {
            1.0 - vce / self.params.var
        } else {
            1.0
        };
        
        // Terminal currents (Ebers-Moll)
        let ic = (if_current / self.params.bf - ir_current) * early_factor;
        let ib = if_current / self.params.bf + ir_current / self.params.br;
        let ie = ic + ib;
        
        // Apply polarity for PNP
        match self.params.bjt_type {
            BjtType::NPN => (ic, ib, ie),
            BjtType::PNP => (-ic, -ib, -ie),
        }
    }
    
    /// Calculate small-signal parameters
    fn small_signal_params(&self, vbe: f64, vce: f64, temp: f64) -> (f64, f64, f64, f64) {
        let vt = thermal_voltage(temp);
        let (ic, _, _) = self.bjt_currents(vbe, vce, temp);
        
        // Transconductance gm = IC/VT
        let gm = ic.abs() / vt;
        
        // Input conductance gpi = gm/beta
        let gpi = gm / self.params.bf;
        
        // Output conductance go = IC/VAF (Early effect)
        let go = if self.params.vaf.is_finite() {
            ic.abs() / self.params.vaf
        } else {
            0.0
        };
        
        // Base resistance effect
        let rb_eff = self.params.rb;
        
        (gm, gpi, go, rb_eff)
    }
}

impl Default for BjtModel {
    fn default() -> Self {
        Self::new("generic_bjt".to_string(), BjtParams::default())
    }
}

impl SpiceModel for BjtModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::BJT
    }
    
    fn current(&self, voltages: &[f64], temp: f64) -> f64 {
        if voltages.len() != 3 {
            return 0.0;
        }
        
        // Terminals: 0=Base, 1=Collector, 2=Emitter
        let vbe = voltages[0] - voltages[2];
        let vce = voltages[1] - voltages[2];
        
        let (ic, _, _) = self.bjt_currents(vbe, vce, temp);
        ic
    }
    
    fn conductance(&self, voltages: &[f64], temp: f64) -> Vec<f64> {
        if voltages.len() != 3 {
            return vec![0.0; 4];
        }
        
        let vbe = voltages[0] - voltages[2];
        let vce = voltages[1] - voltages[2];
        
        let (gm, gpi, go, _) = self.small_signal_params(vbe, vce, temp);
        
        // Return linearized conductance matrix elements
        // [gm, gpi, go, gm] for [dic/dvbe, dib/dvbe, dic/dvce, die/dvbe]
        vec![gm, gpi, go, gm + gpi]
    }
    
    fn num_terminals(&self) -> usize {
        3
    }
    
    fn is_nonlinear(&self) -> bool {
        true
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("type".to_string(), if self.params.bjt_type == BjtType::NPN { 1.0 } else { -1.0 });
        params.insert("bf".to_string(), self.params.bf);
        params.insert("br".to_string(), self.params.br);
        params.insert("is".to_string(), self.params.is);
        params.insert("nf".to_string(), self.params.nf);
        params.insert("nr".to_string(), self.params.nr);
        params.insert("rb".to_string(), self.params.rb);
        params.insert("rc".to_string(), self.params.rc);
        params.insert("re".to_string(), self.params.re);
        params.insert("vaf".to_string(), self.params.vaf);
        params.insert("var".to_string(), self.params.var);
        params.insert("cje".to_string(), self.params.cje);
        params.insert("vje".to_string(), self.params.vje);
        params.insert("mje".to_string(), self.params.mje);
        params.insert("cjc".to_string(), self.params.cjc);
        params.insert("vjc".to_string(), self.params.vjc);
        params.insert("mjc".to_string(), self.params.mjc);
        params.insert("tf".to_string(), self.params.tf);
        params.insert("tr".to_string(), self.params.tr);
        params.insert("eg".to_string(), self.params.eg);
        params.insert("xti".to_string(), self.params.xti);
        params.insert("tnom".to_string(), self.params.tnom);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "bf" => self.params.bf = value,
            "br" => self.params.br = value,
            "is" => self.params.is = value,
            "nf" => self.params.nf = value,
            "nr" => self.params.nr = value,
            "rb" => self.params.rb = value,
            "rc" => self.params.rc = value,
            "re" => self.params.re = value,
            "vaf" => self.params.vaf = value,
            "var" => self.params.var = value,
            "cje" => self.params.cje = value,
            "vje" => self.params.vje = value,
            "mje" => self.params.mje = value,
            "cjc" => self.params.cjc = value,
            "vjc" => self.params.vjc = value,
            "mjc" => self.params.mjc = value,
            "tf" => self.params.tf = value,
            "tr" => self.params.tr = value,
            "eg" => self.params.eg = value,
            "xti" => self.params.xti = value,
            "tnom" => self.params.tnom = value,
            _ => return Err(format!("Unknown parameter: {}", name)),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_bjt_amplification() {
        let bjt = BjtModel::from_preset("test", "2n2222");
        let vbe = 0.7;
        let vce = 5.0;
        let (ic, ib, _) = bjt.bjt_currents(vbe, vce, 27.0);
        let beta = ic / ib;
        assert!(beta > 100.0 && beta < 300.0); // Should be around BF
    }
    
    #[test]
    fn test_pnp_polarity() {
        let bjt = BjtModel::from_preset("test", "2n3906");
        let (ic, ib, ie) = bjt.bjt_currents(0.7, 5.0, 27.0);
        assert!(ic < 0.0); // PNP currents are negative
        assert!(ib < 0.0);
        assert!(ie < 0.0);
    }
}