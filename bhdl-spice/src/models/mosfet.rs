//! MOSFET SPICE models (Level 1, 2, 3)

use std::collections::HashMap;
use super::{SpiceModel, ModelType, thermal_voltage};

/// MOSFET type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MosfetType {
    NMOS,
    PMOS,
}

/// MOSFET model parameters (Level 1 - Shichman-Hodges)
#[derive(Debug, Clone)]
pub struct MosfetParams {
    /// Transistor type
    pub mos_type: MosfetType,
    /// Model level (1, 2, or 3)
    pub level: u8,
    /// Threshold voltage (V)
    pub vto: f64,
    /// Transconductance parameter (A/V²)
    pub kp: f64,
    /// Channel length modulation (1/V)
    pub lambda: f64,
    /// Surface potential (V)
    pub phi: f64,
    /// Substrate doping (1/cm³)
    pub nsub: f64,
    /// Oxide thickness (m)
    pub tox: f64,
    /// Channel width (m)
    pub w: f64,
    /// Channel length (m)
    pub l: f64,
    /// Lateral diffusion (m)
    pub ld: f64,
    /// Bulk threshold parameter (V^0.5)
    pub gamma: f64,
    /// Gate-source capacitance (F)
    pub cgs: f64,
    /// Gate-drain capacitance (F)
    pub cgd: f64,
    /// Gate-bulk capacitance (F)
    pub cgb: f64,
    /// Drain resistance (Ω)
    pub rd: f64,
    /// Source resistance (Ω)
    pub rs: f64,
    /// Bulk junction saturation current (A)
    pub is: f64,
    /// Junction potential (V)
    pub pb: f64,
    /// Temperature coefficient of VTO (V/°C)
    pub tnom: f64,
}

impl Default for MosfetParams {
    fn default() -> Self {
        Self {
            mos_type: MosfetType::NMOS,
            level: 1,
            vto: 1.0,
            kp: 20e-6,
            lambda: 0.0,
            phi: 0.6,
            nsub: 1e15,
            tox: 100e-9,
            w: 10e-6,
            l: 2e-6,
            ld: 0.0,
            gamma: 0.5,
            cgs: 0.0,
            cgd: 0.0,
            cgb: 0.0,
            rd: 0.0,
            rs: 0.0,
            is: 1e-14,
            pb: 0.8,
            tnom: 27.0,
        }
    }
}

/// Common MOSFET models
impl MosfetParams {
    /// IRF540 N-channel power MOSFET
    pub fn irf540() -> Self {
        Self {
            mos_type: MosfetType::NMOS,
            level: 1,
            vto: 3.8,
            kp: 20.85,
            lambda: 0.001,
            w: 0.68,
            l: 2e-6,
            rd: 0.044,
            rs: 0.0,
            cgs: 1945e-12,
            cgd: 283e-12,
            ..Default::default()
        }
    }
    
    /// 2N7000 N-channel small signal MOSFET
    pub fn n2n7000() -> Self {
        Self {
            mos_type: MosfetType::NMOS,
            level: 1,
            vto: 1.8,
            kp: 0.2133,
            lambda: 0.0264,
            w: 0.035,
            l: 2.5e-6,
            rd: 1.387,
            rs: 0.0,
            cgs: 29e-12,
            cgd: 6.6e-12,
            ..Default::default()
        }
    }
    
    /// BS250 P-channel small signal MOSFET
    pub fn bs250() -> Self {
        Self {
            mos_type: MosfetType::PMOS,
            level: 1,
            vto: -3.2,
            kp: 0.195,
            lambda: 0.008,
            w: 0.19,
            l: 2e-6,
            rd: 5.0,
            rs: 0.0,
            cgs: 60e-12,
            cgd: 40e-12,
            ..Default::default()
        }
    }
}

/// MOSFET SPICE model
pub struct MosfetModel {
    name: String,
    params: MosfetParams,
}

impl MosfetModel {
    /// Create new MOSFET model
    pub fn new(name: String, params: MosfetParams) -> Self {
        Self { name, params }
    }
    
    /// Create model from preset
    pub fn from_preset(name: &str, preset: &str) -> Self {
        let params = match preset.to_lowercase().as_str() {
            "irf540" => MosfetParams::irf540(),
            "2n7000" => MosfetParams::n2n7000(),
            "bs250" => MosfetParams::bs250(),
            _ => MosfetParams::default(),
        };
        Self::new(name.to_string(), params)
    }
    
    /// Calculate effective threshold voltage with body effect
    fn vth_effective(&self, vsb: f64) -> f64 {
        let sign = match self.params.mos_type {
            MosfetType::NMOS => 1.0,
            MosfetType::PMOS => -1.0,
        };
        
        // Body effect: VTH = VTO + gamma * (sqrt(2*phi + VSB) - sqrt(2*phi))
        if vsb.abs() > 0.0 && self.params.gamma > 0.0 {
            let phi2 = 2.0 * self.params.phi;
            self.params.vto + sign * self.params.gamma * 
                ((phi2 + vsb.abs()).sqrt() - phi2.sqrt())
        } else {
            self.params.vto
        }
    }
    
    /// Calculate drain current using Level 1 model
    fn drain_current_level1(&self, vgs: f64, vds: f64, vsb: f64) -> f64 {
        let vth = self.vth_effective(vsb);
        let k = 0.5 * self.params.kp * (self.params.w / self.params.l);
        
        let (vgs_eff, vds_eff) = match self.params.mos_type {
            MosfetType::NMOS => (vgs, vds),
            MosfetType::PMOS => (-vgs, -vds),
        };
        
        if vgs_eff <= vth {
            // Cutoff region
            0.0
        } else if vds_eff <= vgs_eff - vth {
            // Linear region: ID = k * [2*(VGS-VTH)*VDS - VDS²] * (1 + λ*VDS)
            let id = k * (2.0 * (vgs_eff - vth) * vds_eff - vds_eff * vds_eff);
            id * (1.0 + self.params.lambda * vds_eff)
        } else {
            // Saturation region: ID = k * (VGS-VTH)² * (1 + λ*VDS)
            let id = k * (vgs_eff - vth).powi(2);
            id * (1.0 + self.params.lambda * vds_eff)
        }
    }
    
    /// Calculate transconductance gm = dID/dVGS
    fn transconductance(&self, vgs: f64, vds: f64, vsb: f64) -> f64 {
        let vth = self.vth_effective(vsb);
        let k = 0.5 * self.params.kp * (self.params.w / self.params.l);
        
        let (vgs_eff, vds_eff) = match self.params.mos_type {
            MosfetType::NMOS => (vgs, vds),
            MosfetType::PMOS => (-vgs, -vds),
        };
        
        if vgs_eff <= vth {
            0.0
        } else if vds_eff <= vgs_eff - vth {
            // Linear region
            2.0 * k * vds_eff * (1.0 + self.params.lambda * vds_eff)
        } else {
            // Saturation region
            2.0 * k * (vgs_eff - vth) * (1.0 + self.params.lambda * vds_eff)
        }
    }
    
    /// Calculate output conductance gds = dID/dVDS
    fn output_conductance(&self, vgs: f64, vds: f64, vsb: f64) -> f64 {
        let vth = self.vth_effective(vsb);
        let k = 0.5 * self.params.kp * (self.params.w / self.params.l);
        
        let (vgs_eff, vds_eff) = match self.params.mos_type {
            MosfetType::NMOS => (vgs, vds),
            MosfetType::PMOS => (-vgs, -vds),
        };
        
        if vgs_eff <= vth {
            0.0
        } else if vds_eff <= vgs_eff - vth {
            // Linear region
            let base = 2.0 * (vgs_eff - vth) - 2.0 * vds_eff;
            k * base * (1.0 + self.params.lambda * vds_eff) + 
            k * (2.0 * (vgs_eff - vth) * vds_eff - vds_eff * vds_eff) * self.params.lambda
        } else {
            // Saturation region (only channel length modulation)
            k * (vgs_eff - vth).powi(2) * self.params.lambda
        }
    }
}

impl Default for MosfetModel {
    fn default() -> Self {
        Self::new("generic_mosfet".to_string(), MosfetParams::default())
    }
}

impl SpiceModel for MosfetModel {
    fn name(&self) -> &str {
        &self.name
    }
    
    fn model_type(&self) -> ModelType {
        ModelType::MOSFET
    }
    
    fn current(&self, voltages: &[f64], temp: f64) -> f64 {
        if voltages.len() != 4 {
            return 0.0;
        }
        
        // Terminals: 0=Gate, 1=Drain, 2=Source, 3=Bulk
        let vgs = voltages[0] - voltages[2];
        let vds = voltages[1] - voltages[2];
        let vsb = voltages[2] - voltages[3];
        
        // Temperature adjustment for threshold voltage
        let temp_delta = temp - self.params.tnom;
        let vth_temp_adj = -2.5e-3 * temp_delta; // Typical -2.5mV/°C
        
        // Adjust threshold voltage
        let original_vto = self.params.vto;
        let adjusted_params = MosfetParams {
            vto: original_vto + vth_temp_adj,
            ..self.params.clone()
        };
        let adjusted_model = MosfetModel::new(self.name.clone(), adjusted_params);
        
        match self.params.level {
            1 => adjusted_model.drain_current_level1(vgs, vds, vsb),
            _ => adjusted_model.drain_current_level1(vgs, vds, vsb), // Default to Level 1
        }
    }
    
    fn conductance(&self, voltages: &[f64], _temp: f64) -> Vec<f64> {
        if voltages.len() != 4 {
            return vec![0.0; 2];
        }
        
        let vgs = voltages[0] - voltages[2];
        let vds = voltages[1] - voltages[2];
        let vsb = voltages[2] - voltages[3];
        
        let gm = self.transconductance(vgs, vds, vsb);
        let gds = self.output_conductance(vgs, vds, vsb);
        
        vec![gm, gds]
    }
    
    fn num_terminals(&self) -> usize {
        4
    }
    
    fn is_nonlinear(&self) -> bool {
        true
    }
    
    fn parameters(&self) -> HashMap<String, f64> {
        let mut params = HashMap::new();
        params.insert("type".to_string(), if self.params.mos_type == MosfetType::NMOS { 1.0 } else { -1.0 });
        params.insert("level".to_string(), self.params.level as f64);
        params.insert("vto".to_string(), self.params.vto);
        params.insert("kp".to_string(), self.params.kp);
        params.insert("lambda".to_string(), self.params.lambda);
        params.insert("phi".to_string(), self.params.phi);
        params.insert("nsub".to_string(), self.params.nsub);
        params.insert("tox".to_string(), self.params.tox);
        params.insert("w".to_string(), self.params.w);
        params.insert("l".to_string(), self.params.l);
        params.insert("ld".to_string(), self.params.ld);
        params.insert("gamma".to_string(), self.params.gamma);
        params.insert("cgs".to_string(), self.params.cgs);
        params.insert("cgd".to_string(), self.params.cgd);
        params.insert("cgb".to_string(), self.params.cgb);
        params.insert("rd".to_string(), self.params.rd);
        params.insert("rs".to_string(), self.params.rs);
        params.insert("is".to_string(), self.params.is);
        params.insert("pb".to_string(), self.params.pb);
        params.insert("tnom".to_string(), self.params.tnom);
        params
    }
    
    fn set_parameter(&mut self, name: &str, value: f64) -> Result<(), String> {
        match name {
            "level" => self.params.level = value as u8,
            "vto" => self.params.vto = value,
            "kp" => self.params.kp = value,
            "lambda" => self.params.lambda = value,
            "phi" => self.params.phi = value,
            "nsub" => self.params.nsub = value,
            "tox" => self.params.tox = value,
            "w" => self.params.w = value,
            "l" => self.params.l = value,
            "ld" => self.params.ld = value,
            "gamma" => self.params.gamma = value,
            "cgs" => self.params.cgs = value,
            "cgd" => self.params.cgd = value,
            "cgb" => self.params.cgb = value,
            "rd" => self.params.rd = value,
            "rs" => self.params.rs = value,
            "is" => self.params.is = value,
            "pb" => self.params.pb = value,
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
    fn test_mosfet_regions() {
        let mosfet = MosfetModel::from_preset("test", "2n7000");
        
        // Cutoff
        let id_off = mosfet.drain_current_level1(0.0, 5.0, 0.0);
        assert!(id_off.abs() < 1e-12);
        
        // Linear
        let id_lin = mosfet.drain_current_level1(3.0, 0.5, 0.0);
        assert!(id_lin > 0.0);
        
        // Saturation
        let id_sat = mosfet.drain_current_level1(3.0, 5.0, 0.0);
        assert!(id_sat > 0.0);
    }
}