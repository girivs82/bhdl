//! SPICE parameter extraction from BHDL component attributes


/// Extract SPICE model parameters from entity attributes
// Commented out to avoid cyclic dependency with bhdl_spice
/*
pub fn extract_spice_model_from_entity(entity: &Entity) -> Option<ComponentModel> {
    let attrs = extract_module_attributes(entity);
    let spice_model = attrs.get("spice_model")?;
    
    match spice_model.as_str() {
        "resistor" => extract_resistor_model(&attrs),
        "led" => extract_led_model(&attrs),
        "diode" => extract_diode_model(&attrs),
        "voltage_source" => extract_voltage_source_model(&attrs),
        "capacitor" => extract_capacitor_model(&attrs),
        "inductor" => extract_inductor_model(&attrs),
        _ => None,
    }
}

*/

/// Extract SPICE model from component instance parameters
// Commented out to avoid cyclic dependency with bhdl_spice
/*
pub fn extract_spice_model_from_params(component_type: &str, params: &HashMap<String, String>) -> Option<ComponentModel> {
    match component_type {
        "Res" | "Resistor" => extract_resistor_from_params(params),
        "Cap" | "Capacitor" => extract_capacitor_from_params(params),
        "LED" => extract_led_from_params(params),
        "Diode" => extract_diode_from_params(params),
        _ => None,
    }
}

/// Extract resistor model from parameters
fn extract_resistor_from_params(params: &HashMap<String, String>) -> Option<ComponentModel> {
    let resistance = params.get("value")
        .or_else(|| params.values().next()) // First param if unnamed
        .and_then(|v| parse_unit_value(v))?;
    
    let tolerance = params.get("tolerance")
        .and_then(|v| v.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(5.0);
    
    let power_rating = params.get("power_rating")
        .and_then(|v| parse_unit_value(v));
    
    Some(ComponentModel::Resistor {
        resistance,
        tolerance,
        limits: ElectricalLimits {
            max_power: power_rating,
            ..Default::default()
        },
    })
}

/// Extract capacitor model from parameters
fn extract_capacitor_from_params(params: &HashMap<String, String>) -> Option<ComponentModel> {
    let capacitance = params.get("value")
        .or_else(|| params.values().next())
        .and_then(|v| parse_unit_value(v))?;
    
    let voltage_rating = params.get("voltage_rating")
        .or_else(|| params.get("voltage"))
        .and_then(|v| parse_unit_value(v));
    
    Some(ComponentModel::Capacitor {
        capacitance,
        esr: None,
        limits: ElectricalLimits {
            max_voltage: voltage_rating,
            ..Default::default()
        },
    })
}

/// Extract LED model from parameters
fn extract_led_from_params(params: &HashMap<String, String>) -> Option<ComponentModel> {
    let color = params.get("color")
        .or_else(|| params.values().next())
        .cloned()
        .unwrap_or_else(|| "red".to_string());
    
    // Default values based on color - these would be overridden by stdlib
    let (forward_voltage, wavelength) = match color.as_str() {
        "red" => (2.0, Some(630.0)),
        "green" => (2.2, Some(525.0)),
        "blue" => (3.2, Some(470.0)),
        "white" => (3.3, None),
        "yellow" => (2.1, Some(590.0)),
        "ir" => (1.4, Some(940.0)),
        _ => (2.0, None),
    };
    
    Some(ComponentModel::LED {
        color,
        forward_voltage,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits {
            max_current: Some(0.030),
            max_power: Some(0.100),
            ..Default::default()
        },
    })
}

/// Extract diode model from parameters
fn extract_diode_from_params(params: &HashMap<String, String>) -> Option<ComponentModel> {
    let part = params.get("part")
        .or_else(|| params.values().next())
        .map(|s| s.as_str())
        .unwrap_or("1N4148");
    
    // Default values based on part number
    let (forward_voltage, max_current, reverse_voltage) = match part {
        "1N4148" => (0.7, 0.3, 100.0),
        "1N4007" => (1.1, 1.0, 1000.0),
        "1N5819" => (0.45, 1.0, 40.0),
        _ => (0.7, 0.1, 50.0),
    };
    
    Some(ComponentModel::Diode {
        forward_voltage,
        forward_resistance: 10.0,
        reverse_current: 1e-9,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits {
            max_current: Some(max_current),
            max_voltage: Some(reverse_voltage),
            ..Default::default()
        },
    })
}

/// Parse a value with units (e.g., "10mA" -> 0.01)
fn parse_unit_value(value: &str) -> Option<f64> {
    // Extract number and unit parts
    let (num_str, unit_str) = value
        .char_indices()
        .find(|(_, c)| c.is_alphabetic())
        .map(|(idx, _)| value.split_at(idx))
        .unwrap_or((value, ""));
    
    let mut number: f64 = num_str.parse().ok()?;
    
    // Apply unit multiplier
    match unit_str {
        "T" => number *= 1e12,
        "G" => number *= 1e9,
        "M" | "MEG" => number *= 1e6,
        "k" | "K" => number *= 1e3,
        "m" => number *= 1e-3,
        "u" | "μ" => number *= 1e-6,
        "n" => number *= 1e-9,
        "p" => number *= 1e-12,
        "f" => number *= 1e-15,
        // Unit suffixes (ignore for now)
        "V" | "A" | "W" | "F" | "H" | "Ω" | "ohm" | "Hz" | "s" => {},
        _ => {},
    }
    
    Some(number)
}

/// Extract resistor model from attributes
fn extract_resistor_model(attrs: &HashMap<String, String>) -> Option<ComponentModel> {
    let resistance = attrs.get("spice_resistance")
        .or_else(|| attrs.get("resistance"))
        .and_then(|v| parse_unit_value(v))?;
    
    let tolerance = attrs.get("tolerance")
        .and_then(|v| v.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(5.0);
    
    let limits = ElectricalLimits {
        max_power: attrs.get("spice_max_power")
            .or_else(|| attrs.get("power_rating"))
            .and_then(|v| parse_unit_value(v)),
        max_voltage: attrs.get("max_voltage")
            .and_then(|v| parse_unit_value(v)),
        ..Default::default()
    };
    
    Some(ComponentModel::Resistor {
        resistance,
        tolerance,
        limits,
    })
}

/// Extract LED model from attributes
fn extract_led_model(attrs: &HashMap<String, String>) -> Option<ComponentModel> {
    let color = attrs.get("color")
        .cloned()
        .unwrap_or_else(|| "red".to_string());
    
    let forward_voltage = attrs.get("spice_forward_voltage")
        .or_else(|| attrs.get("forward_voltage"))
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(2.0);
    
    let forward_current = attrs.get("forward_current")
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(0.020);
    
    let dynamic_resistance = attrs.get("spice_dynamic_resistance")
        .or_else(|| attrs.get("dynamic_resistance"))
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(10.0);
    
    let limits = ElectricalLimits {
        max_current: attrs.get("spice_max_current")
            .or_else(|| attrs.get("max_current"))
            .and_then(|v| parse_unit_value(v)),
        max_power: Some(forward_voltage * forward_current * 2.0), // Rough estimate
        ..Default::default()
    };
    
    Some(ComponentModel::LED {
        color,
        forward_voltage,
        forward_current,
        dynamic_resistance,
        limits,
    })
}

/// Extract diode model from attributes
fn extract_diode_model(attrs: &HashMap<String, String>) -> Option<ComponentModel> {
    let forward_voltage = attrs.get("spice_forward_voltage")
        .or_else(|| attrs.get("forward_voltage"))
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(0.7);
    
    let forward_resistance = 10.0; // Default dynamic resistance
    
    let reverse_current = attrs.get("reverse_current")
        .and_then(|v| parse_unit_value(v))
        .unwrap_or(1e-9);
    
    let saturation_current = attrs.get("spice_saturation_current")
        .and_then(|v| parse_unit_value(v));
    
    let emission_coefficient = attrs.get("spice_emission_coefficient")
        .and_then(|v| v.parse::<f64>().ok());
    
    let limits = ElectricalLimits {
        max_current: attrs.get("max_forward_current")
            .and_then(|v| parse_unit_value(v)),
        max_voltage: attrs.get("reverse_voltage")
            .and_then(|v| parse_unit_value(v)),
        ..Default::default()
    };
    
    Some(ComponentModel::Diode {
        forward_voltage,
        forward_resistance,
        reverse_current,
        saturation_current,
        emission_coefficient,
        limits,
    })
}

/// Extract voltage source model from attributes
fn extract_voltage_source_model(attrs: &HashMap<String, String>) -> Option<ComponentModel> {
    let voltage = attrs.get("output_voltage")
        .or_else(|| attrs.get("voltage"))
        .and_then(|v| parse_unit_value(v))?;
    
    let internal_resistance = attrs.get("internal_resistance")
        .and_then(|v| parse_unit_value(v));
    
    Some(ComponentModel::VoltageSource {
        voltage,
        internal_resistance,
    })
}

/// Extract capacitor model from attributes
fn extract_capacitor_model(attrs: &HashMap<String, String>) -> Option<ComponentModel> {
    let capacitance = attrs.get("capacitance")
        .or_else(|| attrs.get("value"))
        .and_then(|v| parse_unit_value(v))?;
    
    let esr = attrs.get("esr")
        .and_then(|v| parse_unit_value(v));
    
    let tolerance = attrs.get("tolerance")
        .and_then(|v| v.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(20.0);
    
    let limits = ElectricalLimits {
        max_voltage: attrs.get("voltage_rating")
            .and_then(|v| parse_unit_value(v)),
        ..Default::default()
    };
    
    Some(ComponentModel::Capacitor {
        capacitance,
        esr,
        limits,
    })
}

/// Extract inductor model from attributes
fn extract_inductor_model(attrs: &HashMap<String, String>) -> Option<ComponentModel> {
    let inductance = attrs.get("inductance")
        .or_else(|| attrs.get("value"))
        .and_then(|v| parse_unit_value(v))?;
    
    let dcr = attrs.get("dcr")
        .or_else(|| attrs.get("dc_resistance"))
        .and_then(|v| parse_unit_value(v));
    
    let tolerance = attrs.get("tolerance")
        .and_then(|v| v.trim_end_matches('%').parse::<f64>().ok())
        .unwrap_or(10.0);
    
    let limits = ElectricalLimits {
        max_current: attrs.get("saturation_current")
            .and_then(|v| parse_unit_value(v)),
        ..Default::default()
    };
    
    Some(ComponentModel::Inductor {
        inductance,
        dcr,
        limits,
    })
}
*/

/// Parse a value with units (e.g., "10mA" -> 0.01)
/// This function is kept as it's useful for other parts of the analyzer
pub fn parse_unit_value(value: &str) -> Option<f64> {
    // Extract number and unit parts
    let (num_str, unit_str) = value
        .char_indices()
        .find(|(_, c)| c.is_alphabetic() || *c == 'μ' || *c == 'Ω')
        .map(|(idx, _)| value.split_at(idx))
        .unwrap_or((value, ""));

    let mut number: f64 = num_str.parse().ok()?;

    // Strip trailing base unit letters to isolate the SI prefix
    let base_units: &[&str] = &["Hz", "ohm", "Ω", "V", "A", "W", "F", "H", "s"];
    let prefix = {
        let mut p = unit_str;
        for bu in base_units {
            if let Some(stripped) = p.strip_suffix(bu) {
                p = stripped;
                break;
            }
        }
        p
    };

    // Apply SI prefix multiplier
    match prefix {
        "T" => number *= 1e12,
        "G" => number *= 1e9,
        "M" | "MEG" => number *= 1e6,
        "k" | "K" => number *= 1e3,
        "m" => number *= 1e-3,
        "u" | "μ" => number *= 1e-6,
        "n" => number *= 1e-9,
        "p" => number *= 1e-12,
        "f" => number *= 1e-15,
        _ => {},
    }

    Some(number)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_approx(actual: Option<f64>, expected: f64) {
        let v = actual.expect("expected Some");
        assert!((v - expected).abs() < expected.abs() * 1e-12 + 1e-18,
            "expected ~{}, got {}", expected, v);
    }

    #[test]
    fn test_parse_unit_value() {
        assert_approx(parse_unit_value("10k"), 10_000.0);
        assert_approx(parse_unit_value("4.7k"), 4_700.0);
        assert_approx(parse_unit_value("100m"), 0.1);
        assert_approx(parse_unit_value("20mA"), 0.02);
        assert_approx(parse_unit_value("3.3V"), 3.3);
        assert_approx(parse_unit_value("1μF"), 1e-6);
        assert_approx(parse_unit_value("100nF"), 100e-9);
    }
}