//! KiCad electrical specification extractor
//! 
//! Extracts electrical specifications and component data from KiCad symbols

use crate::kicad::parser::{KiCadSymbol, KiCadPin};
use crate::types::{Component, ElectricalSpec, PinDefinition, ComponentSymbol, ComponentCategory, PinType, PinShape};
use std::collections::HashMap;

/// Extracts component data from KiCad symbols
pub struct KiCadExtractor;

impl KiCadExtractor {
    pub fn new() -> Self {
        Self
    }

    /// Extract a Component from a KiCad symbol
    pub fn extract_component(&self, symbol: &KiCadSymbol, svg_data: String) -> anyhow::Result<Component> {
        // Validate SVG data
        if !self.validate_svg_data(&svg_data) {
            return Err(anyhow::anyhow!("Invalid or empty SVG data for symbol: {}", symbol.name));
        }
        
        let category = self.infer_component_category(symbol);
        let electrical_specs = self.extract_electrical_specs(symbol)?;
        let pins = self.extract_pins(symbol)?;
        let component_symbol = self.create_component_symbol(symbol, svg_data)?;
        
        Ok(Component {
            id: 0, // Will be assigned by database
            name: symbol.name.clone(),
            description: symbol.description.clone(),
            manufacturer: None, // Not available in KiCad symbols
            part_number: None,   // Not available in KiCad symbols
            package_type: symbol.footprint.clone(),
            category,
            subcategory: None,
            datasheet_url: symbol.datasheet.clone(),
            electrical_specs,
            pins,
            symbol: Some(component_symbol),
            footprint: None, // TODO: Extract footprint data
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        })
    }
    
    /// Validate SVG data
    fn validate_svg_data(&self, svg_data: &str) -> bool {
        // Check if SVG data is not empty
        if svg_data.trim().is_empty() {
            return false;
        }
        
        // Check for basic SVG structure
        if !svg_data.contains("<svg") || !svg_data.contains("</svg>") {
            return false;
        }
        
        // Check for viewBox attribute
        if !svg_data.contains("viewBox") {
            return false;
        }
        
        // Check minimum length (a valid SVG should be at least 100 characters)
        if svg_data.len() < 100 {
            return false;
        }
        
        // Check for some actual content (not just empty SVG tags)
        let has_content = svg_data.contains("<rect") || 
                         svg_data.contains("<circle") || 
                         svg_data.contains("<line") || 
                         svg_data.contains("<polyline") || 
                         svg_data.contains("<path") ||
                         svg_data.contains("<text");
        
        has_content
    }

    /// Infer component category from symbol properties
    fn infer_component_category(&self, symbol: &KiCadSymbol) -> ComponentCategory {
        // Try to infer from reference prefix
        let reference = symbol.reference.to_uppercase();
        if reference.starts_with('R') {
            return ComponentCategory::Resistor;
        } else if reference.starts_with('C') {
            return ComponentCategory::Capacitor;
        } else if reference.starts_with('L') {
            return ComponentCategory::Inductor;
        } else if reference.starts_with('D') {
            return ComponentCategory::Diode;
        } else if reference.starts_with('Q') {
            return ComponentCategory::Transistor;
        } else if reference.starts_with('U') || reference.starts_with("IC") {
            return ComponentCategory::IC;
        }

        // Try to infer from symbol name
        let name = symbol.name.to_lowercase();
        if name.contains("resistor") || name.contains("res") {
            ComponentCategory::Resistor
        } else if name.contains("capacitor") || name.contains("cap") {
            ComponentCategory::Capacitor
        } else if name.contains("inductor") || name.contains("coil") {
            ComponentCategory::Inductor
        } else if name.contains("diode") {
            ComponentCategory::Diode
        } else if name.contains("transistor") || name.contains("fet") || name.contains("bjt") {
            ComponentCategory::Transistor
        } else if name.contains("crystal") || name.contains("xtal") {
            ComponentCategory::Crystal
        } else if name.contains("switch") {
            ComponentCategory::Switch
        } else if name.contains("connector") || name.contains("conn") {
            ComponentCategory::Connector
        } else {
            // Default to IC for multi-pin components, passive for 2-pin
            let total_pins = symbol.units.iter().map(|u| u.pins.len()).sum::<usize>();
            if total_pins <= 2 {
                ComponentCategory::Resistor // Generic passive
            } else {
                ComponentCategory::IC
            }
        }
    }

    /// Extract electrical specifications from symbol properties
    fn extract_electrical_specs(&self, symbol: &KiCadSymbol) -> anyhow::Result<Vec<ElectricalSpec>> {
        let mut specs = Vec::new();
        
        // Check properties for electrical specifications
        for (key, value) in &symbol.properties {
            let key_lower = key.to_lowercase();
            
            if let Some(spec) = self.parse_electrical_property(&key_lower, value)? {
                specs.push(spec);
            }
        }
        
        // Try to extract specs from value field (e.g., "1kΩ", "100nF")
        if !symbol.value.is_empty() {
            if let Some(spec) = self.parse_value_string(&symbol.value)? {
                specs.push(spec);
            }
        }
        
        // Try to extract specs from description
        if let Some(description) = &symbol.description {
            if let Some(spec) = self.parse_description_specs(description)? {
                specs.push(spec);
            }
        }
        
        Ok(specs)
    }

    /// Parse an electrical property from key-value pair
    fn parse_electrical_property(&self, key: &str, value: &str) -> anyhow::Result<Option<ElectricalSpec>> {
        let spec = match key {
            "power" | "power_rating" | "power_max" => {
                if let Some((val, unit)) = self.parse_value_with_unit(value)? {
                    Some(ElectricalSpec {
                        spec_name: "power_rating".to_string(),
                        spec_value: val,
                        spec_unit: unit,
                        spec_tolerance: None,
                        min_value: None,
                        max_value: None,
                        conditions: None,
                    })
                } else { None }
            },
            "voltage" | "voltage_rating" | "voltage_max" => {
                if let Some((val, unit)) = self.parse_value_with_unit(value)? {
                    Some(ElectricalSpec {
                        spec_name: "voltage_rating".to_string(),
                        spec_value: val,
                        spec_unit: unit,
                        spec_tolerance: None,
                        min_value: None,
                        max_value: None,
                        conditions: None,
                    })
                } else { None }
            },
            "current" | "current_rating" | "current_max" => {
                if let Some((val, unit)) = self.parse_value_with_unit(value)? {
                    Some(ElectricalSpec {
                        spec_name: "current_rating".to_string(),
                        spec_value: val,
                        spec_unit: unit,
                        spec_tolerance: None,
                        min_value: None,
                        max_value: None,
                        conditions: None,
                    })
                } else { None }
            },
            "tolerance" => {
                if let Ok(tolerance) = value.trim_end_matches('%').parse::<f64>() {
                    // This will be applied to the main component spec
                    None // Handle separately
                } else { None }
            },
            _ => None,
        };
        
        Ok(spec)
    }

    /// Parse value string like "1kΩ", "100nF", "3.3V"
    fn parse_value_string(&self, value: &str) -> anyhow::Result<Option<ElectricalSpec>> {
        if let Some((val, unit)) = self.parse_value_with_unit(value)? {
            let spec_name = match unit.as_str() {
                "Ω" | "ohm" | "ohms" => "resistance",
                "F" | "farad" | "farads" => "capacitance",
                "H" | "henry" | "henries" => "inductance",
                "V" | "volt" | "volts" => "voltage",
                "A" | "amp" | "amps" | "ampere" | "amperes" => "current",
                "Hz" | "hertz" => "frequency",
                "W" | "watt" | "watts" => "power",
                _ => return Ok(None),
            };
            
            Ok(Some(ElectricalSpec {
                spec_name: spec_name.to_string(),
                spec_value: val,
                spec_unit: unit,
                spec_tolerance: None,
                min_value: None,
                max_value: None,
                conditions: None,
            }))
        } else {
            Ok(None)
        }
    }

    /// Parse electrical specs from description text
    fn parse_description_specs(&self, description: &str) -> anyhow::Result<Option<ElectricalSpec>> {
        // Simple pattern matching for common specs in descriptions
        let desc_lower = description.to_lowercase();
        
        // Look for tolerance information
        if let Some(tolerance_match) = desc_lower.find("±") {
            // Extract tolerance percentage
            if let Some(percent_pos) = desc_lower[tolerance_match..].find('%') {
                let tolerance_str = &desc_lower[tolerance_match+2..tolerance_match+percent_pos];
                if let Ok(tolerance) = tolerance_str.trim().parse::<f64>() {
                    return Ok(Some(ElectricalSpec {
                        spec_name: "tolerance".to_string(),
                        spec_value: tolerance / 100.0, // Convert to fraction
                        spec_unit: "fraction".to_string(),
                        spec_tolerance: None,
                        min_value: None,
                        max_value: None,
                        conditions: Some(description.to_string()),
                    }));
                }
            }
        }
        
        Ok(None)
    }

    /// Parse value with unit (e.g., "1.5k" -> (1500.0, "Ω"))
    fn parse_value_with_unit(&self, input: &str) -> anyhow::Result<Option<(f64, String)>> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(None);
        }

        // Define unit mappings
        let units = [
            // Resistance
            ("Ω", "Ω"), ("ohm", "Ω"), ("ohms", "Ω"),
            // Capacitance  
            ("F", "F"), ("farad", "F"), ("farads", "F"),
            // Inductance
            ("H", "H"), ("henry", "H"), ("henries", "H"),
            // Voltage
            ("V", "V"), ("volt", "V"), ("volts", "V"),
            // Current
            ("A", "A"), ("amp", "A"), ("amps", "A"), ("ampere", "A"), ("amperes", "A"),
            // Power
            ("W", "W"), ("watt", "W"), ("watts", "W"),
            // Frequency
            ("Hz", "Hz"), ("hertz", "Hz"),
        ];

        // Try to find a unit suffix
        for (suffix, canonical_unit) in &units {
            if input.ends_with(suffix) {
                let value_part = &input[..input.len() - suffix.len()];
                if let Some(value) = self.parse_engineering_notation(value_part)? {
                    return Ok(Some((value, canonical_unit.to_string())));
                }
            }
        }

        // Try parsing as pure number (assume base unit)
        if let Some(value) = self.parse_engineering_notation(input)? {
            Ok(Some((value, "".to_string())))
        } else {
            Ok(None)
        }
    }

    /// Parse engineering notation (1k, 2.2M, 330p, etc.)
    fn parse_engineering_notation(&self, input: &str) -> anyhow::Result<Option<f64>> {
        let input = input.trim();
        if input.is_empty() {
            return Ok(None);
        }

        // Engineering prefixes
        let prefixes = [
            ('T', 1e12), ('G', 1e9), ('M', 1e6), ('k', 1e3),
            ('m', 1e-3), ('u', 1e-6), ('μ', 1e-6), ('n', 1e-9), ('p', 1e-12),
        ];

        // Check for prefix
        if let Some(last_char) = input.chars().last() {
            for (prefix_char, multiplier) in &prefixes {
                if last_char == *prefix_char {
                    let number_part = &input[..input.len() - last_char.len_utf8()];
                    if let Ok(base_value) = number_part.parse::<f64>() {
                        return Ok(Some(base_value * multiplier));
                    }
                }
            }
        }

        // Try parsing as plain number
        match input.parse::<f64>() {
            Ok(value) => Ok(Some(value)),
            Err(_) => Ok(None),
        }
    }

    /// Extract pin definitions from KiCad symbol
    fn extract_pins(&self, symbol: &KiCadSymbol) -> anyhow::Result<Vec<PinDefinition>> {
        let mut pins = Vec::new();
        
        for unit in &symbol.units {
            for kicad_pin in &unit.pins {
                pins.push(self.convert_kicad_pin(kicad_pin)?);
            }
        }
        
        Ok(pins)
    }

    /// Convert KiCad pin to our pin definition
    fn convert_kicad_pin(&self, kicad_pin: &KiCadPin) -> anyhow::Result<PinDefinition> {
        let electrical_type = match kicad_pin.electrical_type.as_str() {
            "input" => PinType::Input,
            "output" => PinType::Output,
            "bidirectional" | "tri_state" => PinType::Bidirectional,
            "passive" => PinType::Passive,
            "power_in" => PinType::Power,
            "power_out" => PinType::Power,
            "open_collector" => PinType::Output,
            "open_emitter" => PinType::Output,
            "unspecified" | _ => PinType::Unspecified,
        };

        let pin_shape = match kicad_pin.graphic_style.as_str() {
            "line" => PinShape::Line,
            "inverted" => PinShape::Inverted,
            "clock" => PinShape::Clock,
            "inverted_clock" => PinShape::InvertedClock,
            "input_low" => PinShape::InputLow,
            "clock_low" => PinShape::ClockLow,
            "output_low" => PinShape::OutputLow,
            "edge_clock_high" => PinShape::EdgeClockHigh,
            "non_logic" => PinShape::NonLogic,
            _ => PinShape::Line,
        };

        Ok(PinDefinition {
            pin_number: kicad_pin.number.clone(),
            pin_name: if kicad_pin.name == "~" { None } else { Some(kicad_pin.name.clone()) },
            electrical_type,
            x_position: kicad_pin.x,
            y_position: kicad_pin.y,
            orientation: kicad_pin.orientation,
            length: kicad_pin.length,
            pin_shape,
        })
    }

    /// Create component symbol from KiCad symbol and SVG data
    fn create_component_symbol(&self, symbol: &KiCadSymbol, svg_data: String) -> anyhow::Result<ComponentSymbol> {
        // Calculate bounding box from SVG (simplified)
        let (width, height) = self.extract_svg_dimensions(&svg_data).unwrap_or((40.0, 20.0));
        
        Ok(ComponentSymbol {
            symbol_name: symbol.reference.clone(),
            svg_data,
            bounding_box_width: width,
            bounding_box_height: height,
            reference_point_x: 0.0,
            reference_point_y: height / 2.0,
            style_variant: None,
        })
    }

    /// Extract dimensions from SVG viewBox
    fn extract_svg_dimensions(&self, svg_data: &str) -> Option<(f64, f64)> {
        // Simple regex-like extraction of viewBox
        if let Some(viewbox_start) = svg_data.find("viewBox=\"") {
            let viewbox_content = &svg_data[viewbox_start + 9..];
            if let Some(viewbox_end) = viewbox_content.find('"') {
                let viewbox = &viewbox_content[..viewbox_end];
                let parts: Vec<&str> = viewbox.split_whitespace().collect();
                if parts.len() >= 4 {
                    if let (Ok(width), Ok(height)) = (parts[2].parse::<f64>(), parts[3].parse::<f64>()) {
                        return Some((width, height));
                    }
                }
            }
        }
        None
    }
}

impl Default for KiCadExtractor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kicad::parser::{KiCadSymbol, KiCadUnit, KiCadPin};

    #[test]
    fn test_svg_validation() {
        let extractor = KiCadExtractor::new();
        
        // Valid SVG
        let valid_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100">
            <rect x="10" y="10" width="80" height="80" fill="blue"/>
        </svg>"#;
        assert!(extractor.validate_svg_data(valid_svg));
        
        // Empty SVG
        assert!(!extractor.validate_svg_data(""));
        assert!(!extractor.validate_svg_data("   "));
        
        // Invalid SVG (no closing tag)
        assert!(!extractor.validate_svg_data("<svg viewBox=\"0 0 100 100\">"));
        
        // SVG without viewBox
        assert!(!extractor.validate_svg_data("<svg><rect/></svg>"));
        
        // Too short SVG
        assert!(!extractor.validate_svg_data("<svg viewBox=\"0 0 1 1\"></svg>"));
        
        // SVG without content
        let empty_svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 100"></svg>"#;
        assert!(!extractor.validate_svg_data(empty_svg));
    }

    #[test]
    fn test_component_category_inference() {
        let extractor = KiCadExtractor::new();
        
        // Test resistor
        let mut resistor_symbol = create_test_symbol("R", "1kΩ");
        resistor_symbol.reference = "R1".to_string();
        let category = extractor.infer_component_category(&resistor_symbol);
        assert!(matches!(category, ComponentCategory::Resistor));
        
        // Test capacitor
        let mut cap_symbol = create_test_symbol("C", "100nF");
        cap_symbol.reference = "C1".to_string();
        let category = extractor.infer_component_category(&cap_symbol);
        assert!(matches!(category, ComponentCategory::Capacitor));
        
        // Test IC
        let mut ic_symbol = create_test_symbol("LM358", "");
        ic_symbol.reference = "U1".to_string();
        let category = extractor.infer_component_category(&ic_symbol);
        assert!(matches!(category, ComponentCategory::IC));
    }

    #[test]
    fn test_engineering_notation_parsing() {
        let extractor = KiCadExtractor::new();
        
        assert_eq!(extractor.parse_engineering_notation("1k").unwrap(), Some(1000.0));
        assert_eq!(extractor.parse_engineering_notation("2.2M").unwrap(), Some(2200000.0));
        assert_eq!(extractor.parse_engineering_notation("330p").unwrap(), Some(330e-12));
        let result = extractor.parse_engineering_notation("10μ").unwrap().unwrap();
        assert!((result - 10e-6).abs() < 1e-15);
        assert_eq!(extractor.parse_engineering_notation("1.5").unwrap(), Some(1.5));
    }

    #[test]
    fn test_value_with_unit_parsing() {
        let extractor = KiCadExtractor::new();
        
        let (val, unit) = extractor.parse_value_with_unit("1kΩ").unwrap().unwrap();
        assert_eq!(val, 1000.0);
        assert_eq!(unit, "Ω");
        
        let (val, unit) = extractor.parse_value_with_unit("100nF").unwrap().unwrap();
        assert!((val - 100e-9).abs() < 1e-15); // Use epsilon comparison for floating point
        assert_eq!(unit, "F");
        
        let (val, unit) = extractor.parse_value_with_unit("3.3V").unwrap().unwrap();
        assert_eq!(val, 3.3);
        assert_eq!(unit, "V");
    }

    #[test]
    fn test_pin_conversion() {
        let extractor = KiCadExtractor::new();
        
        let kicad_pin = KiCadPin {
            number: "1".to_string(),
            name: "VCC".to_string(),
            electrical_type: "power_in".to_string(),
            graphic_style: "line".to_string(),
            x: 0.0,
            y: 0.0,
            length: 2.54,
            orientation: 0,
            name_effects: None,
            number_effects: None,
        };
        
        let pin_def = extractor.convert_kicad_pin(&kicad_pin).unwrap();
        assert_eq!(pin_def.pin_number, "1");
        assert_eq!(pin_def.pin_name, Some("VCC".to_string()));
        assert!(matches!(pin_def.electrical_type, PinType::Power));
        assert!(matches!(pin_def.pin_shape, PinShape::Line));
    }

    fn create_test_symbol(name: &str, value: &str) -> KiCadSymbol {
        KiCadSymbol {
            name: name.to_string(),
            description: None,
            keywords: None,
            reference: "U".to_string(),
            value: value.to_string(),
            footprint: None,
            datasheet: None,
            properties: HashMap::new(),
            pins: vec![],
            graphics: vec![],
            units: vec![],
        }
    }
}