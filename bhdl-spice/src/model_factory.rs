//! Factory for creating SPICE models from component specifications

use std::collections::HashMap;
use crate::models::*;
use crate::components::{ComponentModel, ComponentType};

/// Parse a value string that may contain units
pub fn parse_value(value_str: &str) -> Option<f64> {
    // Remove any whitespace
    let value_str = value_str.trim();
    
    // Try to parse as a simple number first
    if let Ok(val) = value_str.parse::<f64>() {
        return Some(val);
    }
    
    // Look for scientific notation with units
    // Examples: "1e-9", "2.2e-6", "100e-12"
    if let Some(e_pos) = value_str.find('e') {
        if let Ok(val) = value_str[..e_pos].parse::<f64>() {
            let exp_part = &value_str[e_pos+1..];
            // Remove any non-numeric suffix (units)
            let exp_str: String = exp_part.chars()
                .take_while(|c| c.is_numeric() || *c == '-' || *c == '+')
                .collect();
            if let Ok(exp) = exp_str.parse::<i32>() {
                return Some(val * 10f64.powi(exp));
            }
        }
    }
    
    // Try to extract numeric part and handle units
    let numeric_part: String = value_str.chars()
        .take_while(|c| c.is_numeric() || *c == '.' || *c == '-' || *c == '+')
        .collect();
    
    // Get the rest of the string (units)
    let unit_part = &value_str[numeric_part.len()..];
    
    // Parse the numeric value
    if let Ok(mut value) = numeric_part.parse::<f64>() {
        // Apply unit multipliers
        match unit_part.trim().to_lowercase().as_str() {
            // SI prefixes
            "t" | "tera" => value *= 1e12,
            "g" | "giga" => value *= 1e9,
            "meg" | "mega" => value *= 1e6,
            "k" | "kilo" => value *= 1e3,
            "m" | "milli" => value *= 1e-3,
            "u" | "micro" | "μ" => value *= 1e-6,
            "n" | "nano" => value *= 1e-9,
            "p" | "pico" => value *= 1e-12,
            "f" | "femto" => value *= 1e-15,
            
            // Common electrical units with prefixes
            s if s.starts_with("k") && (s.ends_with("ω") || s.ends_with("ohm") || s.ends_with("r")) => value *= 1e3,
            s if s.starts_with("m") && (s.ends_with("ω") || s.ends_with("ohm") || s.ends_with("r")) => value *= 1e-3,
            s if s.starts_with("μ") && (s.ends_with("f") || s.ends_with("farad")) => value *= 1e-6,
            s if s.starts_with("n") && (s.ends_with("f") || s.ends_with("farad")) => value *= 1e-9,
            s if s.starts_with("p") && (s.ends_with("f") || s.ends_with("farad")) => value *= 1e-12,
            s if s.starts_with("m") && (s.ends_with("h") || s.ends_with("henry")) => value *= 1e-3,
            s if s.starts_with("μ") && (s.ends_with("h") || s.ends_with("henry")) => value *= 1e-6,
            s if s.starts_with("n") && (s.ends_with("h") || s.ends_with("henry")) => value *= 1e-9,
            s if s.starts_with("m") && (s.ends_with("a") || s.ends_with("amp")) => value *= 1e-3,
            s if s.starts_with("μ") && (s.ends_with("a") || s.ends_with("amp")) => value *= 1e-6,
            s if s.starts_with("n") && (s.ends_with("a") || s.ends_with("amp")) => value *= 1e-9,
            s if s.starts_with("k") && (s.ends_with("v") || s.ends_with("volt")) => value *= 1e3,
            s if s.starts_with("m") && (s.ends_with("v") || s.ends_with("volt")) => value *= 1e-3,
            s if s.starts_with("μ") && (s.ends_with("v") || s.ends_with("volt")) => value *= 1e-6,
            s if s.starts_with("m") && (s.ends_with("w") || s.ends_with("watt")) => value *= 1e-3,
            s if s.starts_with("μ") && (s.ends_with("w") || s.ends_with("watt")) => value *= 1e-6,
            
            // No unit or unrecognized unit - return as is
            _ => {}
        }
        
        Some(value)
    } else {
        None
    }
}

/// Factory for creating sophisticated SPICE models
pub struct SpiceModelFactory {
    /// Model library - maps model names to preset parameters
    model_library: HashMap<String, String>,
}

impl SpiceModelFactory {
    /// Create new factory
    pub fn new() -> Self {
        let mut factory = Self {
            model_library: HashMap::new(),
        };
        
        // Register common models
        factory.register_common_models();
        factory
    }
    
    /// Register common component models
    fn register_common_models(&mut self) {
        // Diodes
        self.model_library.insert("1N4148".to_string(), "1n4148".to_string());
        self.model_library.insert("1N4007".to_string(), "1n4007".to_string());
        self.model_library.insert("1N5819".to_string(), "1n5819".to_string());
        
        // BJTs
        self.model_library.insert("2N2222".to_string(), "2n2222".to_string());
        self.model_library.insert("2N2907".to_string(), "2n2907".to_string());
        self.model_library.insert("2N3904".to_string(), "2n3904".to_string());
        self.model_library.insert("2N3906".to_string(), "2n3906".to_string());
        
        // MOSFETs
        self.model_library.insert("IRF540".to_string(), "irf540".to_string());
        self.model_library.insert("2N7000".to_string(), "2n7000".to_string());
        self.model_library.insert("BS250".to_string(), "bs250".to_string());
        
        // Op-amps
        self.model_library.insert("LM741".to_string(), "lm741".to_string());
        self.model_library.insert("TL072".to_string(), "tl072".to_string());
        self.model_library.insert("LM358".to_string(), "lm358".to_string());
        self.model_library.insert("OP07".to_string(), "op07".to_string());
    }
    
    /// Create SPICE model from component specification
    pub fn create_model(
        &self,
        name: &str,
        component_type: &ComponentType,
        model: &ComponentModel,
        part_number: Option<&str>,
    ) -> Option<Box<dyn SpiceModel>> {
        match component_type {
            ComponentType::Resistor => {
                if let ComponentModel::Resistor { resistance, .. } = model {
                    Some(Box::new(ResistorModel::from_value(
                        name,
                        *resistance,
                        "carbon_film", // Default type
                    )))
                } else {
                    None
                }
            }
            
            ComponentType::Capacitor => {
                if let ComponentModel::Capacitor { capacitance, .. } = model {
                    Some(Box::new(CapacitorModel::from_value(
                        name,
                        *capacitance,
                        "ceramic", // Default type
                        50.0, // Default voltage rating
                    )))
                } else {
                    None
                }
            }
            
            ComponentType::Inductor => {
                if let ComponentModel::Inductor { inductance, .. } = model {
                    Some(Box::new(InductorModel::from_value(
                        name,
                        *inductance,
                        "ferrite", // Default type
                        1.0, // Default current rating
                    )))
                } else {
                    None
                }
            }
            
            ComponentType::Diode => {
                // Check if we have a known model
                if let Some(part) = part_number {
                    if let Some(preset) = self.model_library.get(part) {
                        return Some(Box::new(DiodeModel::from_preset(name, preset)));
                    }
                }
                
                // Create generic diode model
                if let ComponentModel::Diode {
                    forward_voltage,
                    saturation_current,
                    emission_coefficient,
                    ..
                } = model {
                    let mut params = DiodeParams::default();
                    params.vj = *forward_voltage;
                    if let Some(is) = saturation_current {
                        params.is = *is;
                    }
                    if let Some(n) = emission_coefficient {
                        params.n = *n;
                    }
                    Some(Box::new(DiodeModel::new(name.to_string(), params)))
                } else {
                    None
                }
            }
            
            ComponentType::LED => {
                if let ComponentModel::LED {
                    color,
                    forward_voltage,
                    forward_current,
                    ..
                } = model {
                    // Create LED-specific diode model
                    let mut params = match color.to_lowercase().as_str() {
                        "red" => DiodeParams::led_red(),
                        "green" => DiodeParams::led_green(),
                        "blue" => DiodeParams::led_blue(),
                        _ => DiodeParams::led_red(),
                    };
                    params.vj = *forward_voltage;
                    // Calculate Is from forward current
                    let vt = 0.026; // Thermal voltage at room temp
                    params.is = forward_current / (params.n * vt).exp();
                    Some(Box::new(DiodeModel::new(name.to_string(), params)))
                } else {
                    None
                }
            }
            
            ComponentType::BJT => {
                // Check for known models
                if let Some(part) = part_number {
                    if let Some(preset) = self.model_library.get(part) {
                        return Some(Box::new(BjtModel::from_preset(name, preset)));
                    }
                }
                
                // Default NPN model
                Some(Box::new(BjtModel::new(
                    name.to_string(),
                    BjtParams::default(),
                )))
            }
            
            ComponentType::MOSFET => {
                // Check for known models
                if let Some(part) = part_number {
                    if let Some(preset) = self.model_library.get(part) {
                        return Some(Box::new(MosfetModel::from_preset(name, preset)));
                    }
                }
                
                // Default NMOS model
                Some(Box::new(MosfetModel::new(
                    name.to_string(),
                    MosfetParams::default(),
                )))
            }
            
            ComponentType::VoltageRegulator => {
                // Default voltage regulator - actual parameters will come from attributes
                Some(Box::new(VoltageRegulatorModel::new(
                    name.to_string(),
                    VoltageRegulatorParams::default(),
                )))
            }
            
            ComponentType::OpAmp => {
                // Check for known models
                if let Some(part) = part_number {
                    if let Some(preset) = self.model_library.get(part) {
                        return Some(Box::new(OpAmpModel::from_preset(name, preset)));
                    }
                }
                
                // Default op-amp model
                Some(Box::new(OpAmpModel::new(
                    name.to_string(),
                    OpAmpParams::default(),
                )))
            }
            
            _ => None,
        }
    }
    
    /// Create model from BHDL component attributes
    pub fn create_from_attributes(
        &self,
        name: &str,
        attributes: &HashMap<String, String>,
    ) -> Option<Box<dyn SpiceModel>> {
        // Check spice_model attribute
        let spice_model = attributes.get("spice_model")?;
        
        match spice_model.as_str() {
            "resistor" => {
                let resistance = parse_value(attributes.get("spice_resistance")?)?;
                let tc1 = attributes.get("spice_temp_coeff1")
                    .and_then(|v| parse_value(v))
                    .unwrap_or(0.0);
                let tc2 = attributes.get("spice_temp_coeff2")
                    .and_then(|v| parse_value(v))
                    .unwrap_or(0.0);
                let power = attributes.get("spice_max_power")
                    .and_then(|v| parse_value(v))
                    .unwrap_or(0.25);
                
                let mut params = ResistorParams {
                    resistance,
                    tc1,
                    tc2,
                    power_rating: power,
                    ..ResistorParams::default()
                };
                
                Some(Box::new(ResistorModel::new(name.to_string(), params)))
            }
            
            "diode" => {
                let mut params = DiodeParams::default();
                
                // Extract SPICE parameters from attributes
                if let Some(is) = attributes.get("spice_is").and_then(|v| parse_value(v)) {
                    params.is = is;
                }
                if let Some(n) = attributes.get("spice_n").and_then(|v| parse_value(v)) {
                    params.n = n;
                }
                if let Some(rs) = attributes.get("spice_rs").and_then(|v| parse_value(v)) {
                    params.rs = rs;
                }
                if let Some(cjo) = attributes.get("spice_cjo").and_then(|v| parse_value(v)) {
                    params.cjo = cjo;
                }
                if let Some(vj) = attributes.get("spice_vj").and_then(|v| parse_value(v)) {
                    params.vj = vj;
                }
                if let Some(tt) = attributes.get("spice_tt").and_then(|v| parse_value(v)) {
                    params.tt = tt;
                }
                if let Some(bv) = attributes.get("spice_bv").and_then(|v| parse_value(v)) {
                    params.bv = Some(bv);
                }
                if let Some(ibv) = attributes.get("spice_ibv").and_then(|v| parse_value(v)) {
                    params.ibv = ibv;
                }
                if let Some(nbv) = attributes.get("spice_nbv").and_then(|v| parse_value(v)) {
                    params.nbv = nbv;
                }
                
                // Check if it's an LED
                if let Some(led_type) = attributes.get("spice_type") {
                    if led_type == "led" {
                        // LED-specific adjustments
                        params.n = 2.0;  // Typical for LEDs
                    }
                }
                
                Some(Box::new(DiodeModel::new(name.to_string(), params)))
            }
            
            "capacitor" => {
                let capacitance = parse_value(attributes.get("spice_capacitance")?)?;
                let voltage = attributes.get("spice_voltage_rating")
                    .and_then(|v| parse_value(v))
                    .unwrap_or(50.0);
                
                // Create params with SPICE values
                let mut params = CapacitorParams::default();
                params.capacitance = capacitance;
                params.voltage_rating = voltage;
                
                if let Some(esr) = attributes.get("spice_esr").and_then(|v| parse_value(v)) {
                    params.esr = esr;
                }
                if let Some(esl) = attributes.get("spice_esl").and_then(|v| parse_value(v)) {
                    params.esl = esl;
                }
                if let Some(rleak) = attributes.get("spice_rleak").and_then(|v| parse_value(v)) {
                    params.rleak = rleak;
                }
                if let Some(vc1) = attributes.get("spice_vc1").and_then(|v| parse_value(v)) {
                    params.vc1 = vc1;
                }
                if let Some(tc1) = attributes.get("spice_tc1").and_then(|v| parse_value(v)) {
                    params.tc1 = tc1;
                }
                
                Some(Box::new(CapacitorModel::new(name.to_string(), params)))
            }
            
            "inductor" => {
                let inductance = parse_value(attributes.get("spice_inductance")?)?;
                let current = attributes.get("spice_current_rating")
                    .and_then(|v| parse_value(v))
                    .unwrap_or(1.0);
                
                // Create params with SPICE values
                let mut params = InductorParams::default();
                params.inductance = inductance;
                params.current_rating = current;
                
                if let Some(dcr) = attributes.get("spice_dcr").and_then(|v| parse_value(v)) {
                    params.dcr = dcr;
                }
                if let Some(isat) = attributes.get("spice_isat").and_then(|v| parse_value(v)) {
                    params.isat = isat;
                }
                if let Some(rcore) = attributes.get("spice_rcore").and_then(|v| parse_value(v)) {
                    params.rcore = rcore;
                }
                if let Some(cpar) = attributes.get("spice_cpar").and_then(|v| parse_value(v)) {
                    params.cpar = cpar;
                }
                if let Some(tc1) = attributes.get("spice_tc1").and_then(|v| parse_value(v)) {
                    params.tc1 = tc1;
                }
                
                Some(Box::new(InductorModel::new(name.to_string(), params)))
            }
            
            "bjt" => {
                let bjt_type = match attributes.get("spice_type").map(|s| s.as_str()) {
                    Some("npn") => BjtType::NPN,
                    Some("pnp") => BjtType::PNP,
                    _ => BjtType::NPN,
                };
                
                let mut params = BjtParams::default();
                params.bjt_type = bjt_type;
                
                // Extract all SPICE parameters
                if let Some(is) = attributes.get("spice_is").and_then(|v| parse_value(v)) {
                    params.is = is;
                }
                if let Some(bf) = attributes.get("spice_bf").and_then(|v| parse_value(v)) {
                    params.bf = bf;
                }
                if let Some(nf) = attributes.get("spice_nf").and_then(|v| parse_value(v)) {
                    params.nf = nf;
                }
                if let Some(br) = attributes.get("spice_br").and_then(|v| parse_value(v)) {
                    params.br = br;
                }
                if let Some(nr) = attributes.get("spice_nr").and_then(|v| parse_value(v)) {
                    params.nr = nr;
                }
                if let Some(vaf) = attributes.get("spice_vaf").and_then(|v| parse_value(v)) {
                    params.vaf = vaf;
                }
                if let Some(var) = attributes.get("spice_var").and_then(|v| parse_value(v)) {
                    params.var = var;
                }
                if let Some(rb) = attributes.get("spice_rb").and_then(|v| parse_value(v)) {
                    params.rb = rb;
                }
                if let Some(rc) = attributes.get("spice_rc").and_then(|v| parse_value(v)) {
                    params.rc = rc;
                }
                if let Some(re) = attributes.get("spice_re").and_then(|v| parse_value(v)) {
                    params.re = re;
                }
                if let Some(cje) = attributes.get("spice_cje").and_then(|v| parse_value(v)) {
                    params.cje = cje;
                }
                if let Some(cjc) = attributes.get("spice_cjc").and_then(|v| parse_value(v)) {
                    params.cjc = cjc;
                }
                if let Some(tf) = attributes.get("spice_tf").and_then(|v| parse_value(v)) {
                    params.tf = tf;
                }
                if let Some(tr) = attributes.get("spice_tr").and_then(|v| parse_value(v)) {
                    params.tr = tr;
                }
                
                Some(Box::new(BjtModel::new(name.to_string(), params)))
            }
            
            "opamp" => {
                let mut params = OpAmpParams::default();
                
                // Extract SPICE parameters
                if let Some(aol) = attributes.get("spice_aol").and_then(|v| parse_value(v)) {
                    params.aol = aol;
                }
                if let Some(gbw) = attributes.get("spice_gbw").and_then(|v| parse_value(v)) {
                    params.gbw = gbw;
                }
                if let Some(rin) = attributes.get("spice_rin").and_then(|v| parse_value(v)) {
                    params.rin = rin;
                }
                if let Some(rout) = attributes.get("spice_rout").and_then(|v| parse_value(v)) {
                    params.rout = rout;
                }
                // cin is not in OpAmpParams - skip it
                if let Some(vos) = attributes.get("spice_vos").and_then(|v| parse_value(v)) {
                    params.vos = vos;
                }
                if let Some(ios) = attributes.get("spice_ios").and_then(|v| parse_value(v)) {
                    params.ios = ios;
                }
                if let Some(ib) = attributes.get("spice_ib").and_then(|v| parse_value(v)) {
                    params.ib = ib;
                }
                if let Some(cmrr) = attributes.get("spice_cmrr").and_then(|v| parse_value(v)) {
                    params.cmrr = cmrr;
                }
                if let Some(psrr) = attributes.get("spice_psrr").and_then(|v| parse_value(v)) {
                    params.psrr = psrr;
                }
                if let Some(sr) = attributes.get("spice_sr").and_then(|v| parse_value(v)) {
                    params.slew_rate = sr / 1e6;  // Convert V/s to V/µs
                }
                if let Some(slew_rate) = attributes.get("spice_slew_rate").and_then(|v| parse_value(v)) {
                    params.slew_rate = slew_rate;  // Already in V/µs
                }
                if let Some(vsat_p) = attributes.get("spice_vsat_p").and_then(|v| parse_value(v)) {
                    params.vout_max = vsat_p;
                }
                if let Some(vsat_n) = attributes.get("spice_vsat_n").and_then(|v| parse_value(v)) {
                    params.vout_min = -vsat_n;
                }
                if let Some(iq) = attributes.get("spice_iq").and_then(|v| parse_value(v)) {
                    params.iq = iq;
                }
                
                Some(Box::new(OpAmpModel::new(name.to_string(), params)))
            }
            
            "mosfet" => {
                let mos_type = match attributes.get("spice_type").map(|s| s.as_str()) {
                    Some("nmos") => MosfetType::NMOS,
                    Some("pmos") => MosfetType::PMOS,
                    _ => MosfetType::NMOS,
                };
                
                let mut params = MosfetParams::default();
                params.mos_type = mos_type;
                
                // Extract SPICE parameters
                if let Some(level) = attributes.get("spice_level").and_then(|v| parse_value(v)) {
                    params.level = level as u8;
                }
                if let Some(vto) = attributes.get("spice_vto").and_then(|v| parse_value(v)) {
                    params.vto = vto;
                }
                if let Some(kp) = attributes.get("spice_kp").and_then(|v| parse_value(v)) {
                    params.kp = kp;
                }
                if let Some(gamma) = attributes.get("spice_gamma").and_then(|v| parse_value(v)) {
                    params.gamma = gamma;
                }
                if let Some(phi) = attributes.get("spice_phi").and_then(|v| parse_value(v)) {
                    params.phi = phi;
                }
                if let Some(lambda) = attributes.get("spice_lambda").and_then(|v| parse_value(v)) {
                    params.lambda = lambda;
                }
                if let Some(w) = attributes.get("spice_w").and_then(|v| parse_value(v)) {
                    params.w = w;
                }
                if let Some(l) = attributes.get("spice_l").and_then(|v| parse_value(v)) {
                    params.l = l;
                }
                if let Some(rd) = attributes.get("spice_rd").and_then(|v| parse_value(v)) {
                    params.rd = rd;
                }
                if let Some(rs) = attributes.get("spice_rs").and_then(|v| parse_value(v)) {
                    params.rs = rs;
                }
                if let Some(is) = attributes.get("spice_is").and_then(|v| parse_value(v)) {
                    params.is = is;
                }
                
                Some(Box::new(MosfetModel::new(name.to_string(), params)))
            }
            
            "voltage_regulator" => {
                // Create default parameters
                let mut params = VoltageRegulatorParams::default();
                
                // Set type
                params.reg_type = match attributes.get("spice_type").map(|s| s.as_str()) {
                    Some("adjustable") => RegulatorType::Adjustable,
                    _ => RegulatorType::Fixed,
                };
                
                // Extract parameters
                if let Some(vout) = attributes.get("spice_vout_nom").and_then(|v| parse_value(v)) {
                    params.vout_nom = vout;
                }
                if let Some(vref) = attributes.get("spice_vref").and_then(|v| parse_value(v)) {
                    params.vref = vref;
                }
                if let Some(dropout) = attributes.get("spice_dropout").and_then(|v| parse_value(v)) {
                    params.dropout = dropout;
                }
                if let Some(iout_max) = attributes.get("spice_iout_max").and_then(|v| parse_value(v)) {
                    params.iout_max = iout_max;
                }
                if let Some(iq) = attributes.get("spice_iq").and_then(|v| parse_value(v)) {
                    params.iq = iq;
                }
                if let Some(iadj) = attributes.get("spice_iadj").and_then(|v| parse_value(v)) {
                    params.iadj = iadj;
                }
                if let Some(iload_min) = attributes.get("spice_iload_min").and_then(|v| parse_value(v)) {
                    params.iload_min = iload_min;
                }
                if let Some(load_reg) = attributes.get("spice_load_reg").and_then(|v| parse_value(v)) {
                    params.load_reg = load_reg;
                }
                if let Some(line_reg) = attributes.get("spice_line_reg").and_then(|v| parse_value(v)) {
                    params.line_reg = line_reg;
                }
                if let Some(rout) = attributes.get("spice_rout").and_then(|v| parse_value(v)) {
                    params.rout = rout;
                }
                if let Some(psrr) = attributes.get("spice_psrr").and_then(|v| parse_value(v)) {
                    params.psrr = psrr;
                }
                if let Some(tc) = attributes.get("spice_tc").and_then(|v| parse_value(v)) {
                    params.tc = tc;
                }
                if let Some(ignd_ratio) = attributes.get("spice_ignd_ratio").and_then(|v| parse_value(v)) {
                    params.ignd_ratio = ignd_ratio;
                }
                if let Some(vout_min) = attributes.get("spice_vout_min").and_then(|v| parse_value(v)) {
                    params.vout_min = vout_min;
                }
                if let Some(vout_max) = attributes.get("spice_vout_max").and_then(|v| parse_value(v)) {
                    params.vout_max = vout_max;
                }
                if let Some(vnoise) = attributes.get("spice_vnoise").and_then(|v| parse_value(v)) {
                    params.vnoise = vnoise;
                }
                if let Some(rth) = attributes.get("spice_rth").and_then(|v| parse_value(v)) {
                    params.rth = rth;
                }
                if let Some(tj_max) = attributes.get("spice_tj_max").and_then(|v| parse_value(v)) {
                    params.tj_max = tj_max;
                }
                
                Some(Box::new(VoltageRegulatorModel::new(name.to_string(), params)))
            }
            
            _ => None,
        }
    }
    
    /// Create model from BHDL type and parameters
    pub fn create_from_bhdl(
        &self,
        name: &str,
        bhdl_type: &str,
        parameters: &HashMap<String, f64>,
    ) -> Option<Box<dyn SpiceModel>> {
        match bhdl_type.to_lowercase().as_str() {
            "res" | "resistor" => {
                let resistance = parameters.get("value").copied()?;
                Some(Box::new(ResistorModel::from_value(
                    name,
                    resistance,
                    "carbon_film",
                )))
            }
            
            "cap" | "capacitor" => {
                let capacitance = parameters.get("value").copied()?;
                let voltage = parameters.get("voltage").copied().unwrap_or(50.0);
                Some(Box::new(CapacitorModel::from_value(
                    name,
                    capacitance,
                    "ceramic",
                    voltage,
                )))
            }
            
            "ind" | "inductor" => {
                let inductance = parameters.get("value").copied()?;
                let current = parameters.get("current").copied().unwrap_or(1.0);
                Some(Box::new(InductorModel::from_value(
                    name,
                    inductance,
                    "ferrite",
                    current,
                )))
            }
            
            "diode" => {
                let mut params = DiodeParams::default();
                if let Some(vf) = parameters.get("forward_voltage") {
                    params.vj = *vf;
                }
                if let Some(is) = parameters.get("saturation_current") {
                    params.is = *is;
                }
                Some(Box::new(DiodeModel::new(name.to_string(), params)))
            }
            
            "led" => {
                let color = match parameters.get("forward_voltage") {
                    Some(v) if *v < 2.0 => "red",
                    Some(v) if *v < 2.5 => "yellow",
                    Some(v) if *v < 3.0 => "green",
                    _ => "blue",
                };
                let params = match parameters.get("forward_voltage") {
                    Some(v) if *v < 2.0 => DiodeParams::led_red(),
                    Some(v) if *v < 2.5 => DiodeParams::led_green(),
                    Some(v) if *v < 3.0 => DiodeParams::led_green(),
                    _ => DiodeParams::led_blue(),
                };
                Some(Box::new(DiodeModel::new(name.to_string(), params)))
            }
            
            _ => None,
        }
    }
}

impl Default for SpiceModelFactory {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_create_resistor() {
        let factory = SpiceModelFactory::new();
        let model = factory.create_from_bhdl(
            "R1",
            "Res",
            &[("value".to_string(), 1000.0)].into_iter().collect(),
        );
        
        assert!(model.is_some());
        let model = model.unwrap();
        assert_eq!(model.name(), "R1");
        assert_eq!(model.model_type(), ModelType::Resistor);
    }
    
    #[test]
    fn test_create_known_diode() {
        let factory = SpiceModelFactory::new();
        let model = factory.create_model(
            "D1",
            &ComponentType::Diode,
            &ComponentModel::Diode {
                forward_voltage: 0.7,
                forward_resistance: 10.0,
                reverse_current: 1e-9,
                saturation_current: Some(1e-12),
                emission_coefficient: Some(1.0),
                limits: Default::default(),
            },
            Some("1N4148"),
        );
        
        assert!(model.is_some());
        let model = model.unwrap();
        assert_eq!(model.model_type(), ModelType::Diode);
    }
}