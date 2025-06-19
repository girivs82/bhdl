//! Factory for creating SPICE models from component specifications

use std::collections::HashMap;
use crate::models::*;
use crate::components::{ComponentModel, ComponentType};

/// Parse a value string that may contain units
fn parse_value(value_str: &str) -> Option<f64> {
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
    
    numeric_part.parse::<f64>().ok()
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
                let capacitance = parse_value(attributes.get("capacitance")?)?;
                let voltage = attributes.get("voltage_rating")
                    .and_then(|v| parse_value(v))
                    .unwrap_or(50.0);
                
                Some(Box::new(CapacitorModel::from_value(
                    name,
                    capacitance,
                    "ceramic",
                    voltage,
                )))
            }
            
            "inductor" => {
                let inductance = parse_value(attributes.get("inductance")?)?;
                let current = attributes.get("current_rating")
                    .and_then(|v| parse_value(v))
                    .unwrap_or(1.0);
                
                Some(Box::new(InductorModel::from_value(
                    name,
                    inductance,
                    "ferrite",
                    current,
                )))
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