//! KiCad S-expression parser for symbol and footprint libraries

use sexp::{Atom, Sexp};
use crate::types::*;
use std::collections::HashMap;

/// Errors that can occur during KiCad parsing
#[derive(thiserror::Error, Debug)]
pub enum KiCadParseError {
    #[error("Invalid S-expression format: {0}")]
    InvalidFormat(String),
    #[error("Missing required field: {0}")]
    MissingField(String),
    #[error("Invalid value for field {field}: {value}")]
    InvalidValue { field: String, value: String },
    #[error("S-expression parsing error: {0}")]
    SexpError(String),
    #[error("Number parsing error: {0}")]
    NumberParseError(#[from] std::num::ParseFloatError),
}

/// Represents a parsed KiCad symbol
#[derive(Debug, Clone)]
pub struct KiCadSymbol {
    pub name: String,
    pub description: Option<String>,
    pub keywords: Option<String>,
    pub reference: String,
    pub value: String,
    pub footprint: Option<String>,
    pub datasheet: Option<String>,
    pub properties: HashMap<String, String>,
    pub pins: Vec<KiCadPin>,
    pub graphics: Vec<KiCadGraphic>,
    pub units: Vec<KiCadUnit>,
}

/// Represents a pin in a KiCad symbol
#[derive(Debug, Clone)]
pub struct KiCadPin {
    pub number: String,
    pub name: String,
    pub electrical_type: String,
    pub graphic_style: String,
    pub x: f64,
    pub y: f64,
    pub length: f64,
    pub orientation: i32, // 0, 90, 180, 270
    pub name_effects: Option<KiCadTextEffects>,
    pub number_effects: Option<KiCadTextEffects>,
}

/// Represents text effects (font, size, etc.)
#[derive(Debug, Clone)]
pub struct KiCadTextEffects {
    pub font_size: f64,
    pub thickness: Option<f64>,
    pub bold: bool,
    pub italic: bool,
    pub hide: bool,
}

/// Represents a graphic element in a KiCad symbol
#[derive(Debug, Clone)]
pub enum KiCadGraphic {
    Rectangle {
        start_x: f64,
        start_y: f64,
        end_x: f64,
        end_y: f64,
        stroke_width: f64,
        stroke_type: String,
        fill_type: String,
    },
    Circle {
        center_x: f64,
        center_y: f64,
        radius: f64,
        stroke_width: f64,
        stroke_type: String,
        fill_type: String,
    },
    Arc {
        start_x: f64,
        start_y: f64,
        mid_x: f64,
        mid_y: f64,
        end_x: f64,
        end_y: f64,
        stroke_width: f64,
        stroke_type: String,
    },
    Polyline {
        points: Vec<(f64, f64)>,
        stroke_width: f64,
        stroke_type: String,
    },
    Text {
        text: String,
        x: f64,
        y: f64,
        angle: f64,
        effects: KiCadTextEffects,
    },
}

/// Represents a unit (gate) in a multi-unit symbol
#[derive(Debug, Clone)]
pub struct KiCadUnit {
    pub unit_id: i32,
    pub convert_id: i32,
    pub pins: Vec<KiCadPin>,
    pub graphics: Vec<KiCadGraphic>,
}

/// Main KiCad symbol parser
pub struct KiCadSymbolParser;

impl KiCadSymbolParser {
    pub fn new() -> Self {
        Self
    }

    /// Parse a KiCad symbol library file (.kicad_sym)
    pub fn parse_symbol_library(&self, content: &str) -> Result<Vec<KiCadSymbol>, KiCadParseError> {
        let sexp = sexp::parse(content)
            .map_err(|e| KiCadParseError::SexpError(e.to_string()))?;
        
        match sexp {
            Sexp::List(list) => {
                if let Some(Sexp::Atom(Atom::S(name))) = list.first() {
                    if name == "kicad_symbol_lib" {
                        return self.parse_symbol_lib_content(&list[1..]);
                    }
                }
                Err(KiCadParseError::InvalidFormat("Expected kicad_symbol_lib".to_string()))
            }
            _ => Err(KiCadParseError::InvalidFormat("Expected list".to_string())),
        }
    }

    /// Parse the content of a symbol library
    fn parse_symbol_lib_content(&self, content: &[Sexp]) -> Result<Vec<KiCadSymbol>, KiCadParseError> {
        let mut symbols = Vec::new();
        
        for item in content {
            if let Sexp::List(list) = item {
                if let Some(Sexp::Atom(Atom::S(name))) = list.first() {
                    if name == "symbol" {
                        symbols.push(self.parse_symbol(list)?);
                    }
                    // Skip version, generator, and other metadata
                }
            }
        }
        
        Ok(symbols)
    }

    /// Parse an individual symbol
    fn parse_symbol(&self, sexp_list: &[Sexp]) -> Result<KiCadSymbol, KiCadParseError> {
        // Extract symbol name from first argument
        let name = if let Some(Sexp::Atom(Atom::S(name))) = sexp_list.get(1) {
            name.clone()
        } else {
            return Err(KiCadParseError::MissingField("symbol name".to_string()));
        };

        let mut symbol = KiCadSymbol {
            name,
            description: None,
            keywords: None,
            reference: "U".to_string(), // Default
            value: String::new(),
            footprint: None,
            datasheet: None,
            properties: HashMap::new(),
            pins: Vec::new(),
            graphics: Vec::new(),
            units: Vec::new(),
        };

        // Parse symbol attributes and content
        for item in &sexp_list[2..] {
            if let Sexp::List(list) = item {
                if let Some(Sexp::Atom(Atom::S(tag))) = list.first() {
                    match tag.as_str() {
                        "pin_numbers" => {
                            // Parse pin number visibility settings
                            if let Some(Sexp::Atom(Atom::S(visibility))) = list.get(1) {
                                if visibility == "hide" {
                                    symbol.properties.insert("pin_numbers_hide".to_string(), "true".to_string());
                                }
                            }
                        }
                        "pin_names" => {
                            // Parse pin name settings
                            self.parse_pin_names(&list[1..], &mut symbol)?;
                        }
                        "in_bom" => {
                            if let Some(Sexp::Atom(Atom::S(value))) = list.get(1) {
                                symbol.properties.insert("in_bom".to_string(), value.clone());
                            }
                        }
                        "on_board" => {
                            if let Some(Sexp::Atom(Atom::S(value))) = list.get(1) {
                                symbol.properties.insert("on_board".to_string(), value.clone());
                            }
                        }
                        "property" => {
                            self.parse_property(&list[1..], &mut symbol)?;
                        }
                        "symbol" => {
                            // Parse sub-symbol (unit/convert)
                            self.parse_sub_symbol(&list[1..], &mut symbol)?;
                        }
                        _ => {
                            // Skip unknown tags for now
                        }
                    }
                }
            }
        }

        Ok(symbol)
    }

    /// Parse pin names configuration
    fn parse_pin_names(&self, content: &[Sexp], symbol: &mut KiCadSymbol) -> Result<(), KiCadParseError> {
        for item in content {
            if let Sexp::List(list) = item {
                if let Some(Sexp::Atom(Atom::S(tag))) = list.first() {
                    if tag == "offset" {
                        if let Some(Sexp::Atom(Atom::F(offset))) = list.get(1) {
                            symbol.properties.insert("pin_name_offset".to_string(), offset.to_string());
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Parse a property (Reference, Value, Footprint, etc.)
    fn parse_property(&self, content: &[Sexp], symbol: &mut KiCadSymbol) -> Result<(), KiCadParseError> {
        if content.len() < 2 {
            return Err(KiCadParseError::InvalidFormat("Property requires name and value".to_string()));
        }

        let name = if let Sexp::Atom(Atom::S(name)) = &content[0] {
            name.clone()
        } else {
            return Err(KiCadParseError::InvalidFormat("Property name must be string".to_string()));
        };

        let value = if let Sexp::Atom(Atom::S(value)) = &content[1] {
            value.clone()
        } else {
            return Err(KiCadParseError::InvalidFormat("Property value must be string".to_string()));
        };

        match name.as_str() {
            "Reference" => symbol.reference = value,
            "Value" => symbol.value = value,
            "Footprint" => symbol.footprint = Some(value),
            "Datasheet" => symbol.datasheet = Some(value),
            "Description" => symbol.description = Some(value),
            "ki_keywords" => symbol.keywords = Some(value),
            _ => {
                symbol.properties.insert(name, value);
            }
        }

        Ok(())
    }

    /// Parse a sub-symbol (unit/convert combination)
    fn parse_sub_symbol(&self, content: &[Sexp], symbol: &mut KiCadSymbol) -> Result<(), KiCadParseError> {
        // Sub-symbol name format: "SymbolName_unit_convert"
        let sub_name = if let Some(Sexp::Atom(Atom::S(name))) = content.first() {
            name.clone()
        } else {
            return Err(KiCadParseError::MissingField("sub-symbol name".to_string()));
        };

        // Extract unit and convert IDs from name
        let (unit_id, convert_id) = self.parse_unit_convert_from_name(&sub_name)?;

        let mut unit = KiCadUnit {
            unit_id,
            convert_id,
            pins: Vec::new(),
            graphics: Vec::new(),
        };

        // Parse unit content
        for item in &content[1..] {
            if let Sexp::List(list) = item {
                if let Some(Sexp::Atom(Atom::S(tag))) = list.first() {
                    match tag.as_str() {
                        "pin" => {
                            let pin = self.parse_pin(&list[1..])?;
                            unit.pins.push(pin);
                        }
                        "rectangle" => {
                            let graphic = self.parse_rectangle(&list[1..])?;
                            unit.graphics.push(graphic);
                        }
                        "circle" => {
                            let graphic = self.parse_circle(&list[1..])?;
                            unit.graphics.push(graphic);
                        }
                        "arc" => {
                            let graphic = self.parse_arc(&list[1..])?;
                            unit.graphics.push(graphic);
                        }
                        "polyline" => {
                            let graphic = self.parse_polyline(&list[1..])?;
                            unit.graphics.push(graphic);
                        }
                        "text" => {
                            let graphic = self.parse_text(&list[1..])?;
                            unit.graphics.push(graphic);
                        }
                        _ => {
                            // Skip unknown graphics for now
                        }
                    }
                }
            }
        }

        symbol.units.push(unit);
        Ok(())
    }

    /// Parse unit and convert IDs from sub-symbol name
    fn parse_unit_convert_from_name(&self, name: &str) -> Result<(i32, i32), KiCadParseError> {
        // Format: "SymbolName_unit_convert" e.g., "R_0_1"
        let parts: Vec<&str> = name.split('_').collect();
        if parts.len() >= 3 {
            let unit_id = parts[parts.len() - 2].parse::<i32>()
                .map_err(|_| KiCadParseError::InvalidValue {
                    field: "unit_id".to_string(),
                    value: parts[parts.len() - 2].to_string(),
                })?;
            let convert_id = parts[parts.len() - 1].parse::<i32>()
                .map_err(|_| KiCadParseError::InvalidValue {
                    field: "convert_id".to_string(),
                    value: parts[parts.len() - 1].to_string(),
                })?;
            Ok((unit_id, convert_id))
        } else {
            // Default values if parsing fails
            Ok((0, 1))
        }
    }

    /// Parse a pin definition
    fn parse_pin(&self, content: &[Sexp]) -> Result<KiCadPin, KiCadParseError> {
        if content.len() < 4 {
            return Err(KiCadParseError::InvalidFormat("Pin requires electrical_type, graphic_style, position, length".to_string()));
        }

        let electrical_type = if let Sexp::Atom(Atom::S(et)) = &content[0] {
            et.clone()
        } else {
            return Err(KiCadParseError::InvalidFormat("Pin electrical type must be string".to_string()));
        };

        let graphic_style = if let Sexp::Atom(Atom::S(gs)) = &content[1] {
            gs.clone()
        } else {
            return Err(KiCadParseError::InvalidFormat("Pin graphic style must be string".to_string()));
        };

        // Parse position: (at x y rotation)
        let (x, y, orientation) = if let Sexp::List(at_list) = &content[2] {
            self.parse_position(at_list)?
        } else {
            return Err(KiCadParseError::InvalidFormat("Pin position must be (at x y rotation)".to_string()));
        };

        // Parse length
        let length = if let Sexp::List(length_list) = &content[3] {
            if let Some(Sexp::Atom(Atom::F(len))) = length_list.get(1) {
                *len
            } else if let Some(Sexp::Atom(Atom::I(len))) = length_list.get(1) {
                *len as f64
            } else {
                return Err(KiCadParseError::InvalidFormat("Pin length must be number".to_string()));
            }
        } else {
            return Err(KiCadParseError::InvalidFormat("Pin length must be (length value)".to_string()));
        };

        let mut pin = KiCadPin {
            number: String::new(),
            name: String::new(),
            electrical_type,
            graphic_style,
            x,
            y,
            length,
            orientation: orientation as i32,
            name_effects: None,
            number_effects: None,
        };

        // Parse additional pin properties
        for item in &content[4..] {
            if let Sexp::List(list) = item {
                if let Some(Sexp::Atom(Atom::S(tag))) = list.first() {
                    match tag.as_str() {
                        "name" => {
                            if let Some(Sexp::Atom(Atom::S(name))) = list.get(1) {
                                pin.name = name.clone();
                            }
                        }
                        "number" => {
                            if let Some(Sexp::Atom(Atom::S(number))) = list.get(1) {
                                pin.number = number.clone();
                            }
                        }
                        _ => {
                            // Skip other pin properties for now
                        }
                    }
                }
            }
        }

        Ok(pin)
    }

    /// Parse position (at x y rotation)
    fn parse_position(&self, at_list: &[Sexp]) -> Result<(f64, f64, f64), KiCadParseError> {
        if at_list.len() < 3 {
            return Err(KiCadParseError::InvalidFormat("Position requires at least x and y".to_string()));
        }

        if let Some(Sexp::Atom(Atom::S(tag))) = at_list.first() {
            if tag != "at" {
                return Err(KiCadParseError::InvalidFormat("Expected 'at' keyword".to_string()));
            }
        }

        let x = self.parse_number(&at_list[1])?;
        let y = self.parse_number(&at_list[2])?;
        let rotation = if at_list.len() > 3 {
            self.parse_number(&at_list[3])?
        } else {
            0.0
        };

        Ok((x, y, rotation))
    }

    /// Parse a number from S-expression
    fn parse_number(&self, sexp: &Sexp) -> Result<f64, KiCadParseError> {
        match sexp {
            Sexp::Atom(Atom::F(f)) => Ok(*f),
            Sexp::Atom(Atom::I(i)) => Ok(*i as f64),
            Sexp::Atom(Atom::S(s)) => s.parse::<f64>().map_err(|e| e.into()),
            _ => Err(KiCadParseError::InvalidFormat("Expected number".to_string())),
        }
    }

    /// Parse rectangle graphic
    fn parse_rectangle(&self, content: &[Sexp]) -> Result<KiCadGraphic, KiCadParseError> {
        // Parse start and end positions
        let mut start_x = 0.0;
        let mut start_y = 0.0;
        let mut end_x = 0.0;
        let mut end_y = 0.0;
        let mut stroke_width = 0.0;
        let mut stroke_type = "default".to_string();
        let mut fill_type = "none".to_string();

        for item in content {
            if let Sexp::List(list) = item {
                if let Some(Sexp::Atom(Atom::S(tag))) = list.first() {
                    match tag.as_str() {
                        "start" => {
                            start_x = self.parse_number(&list[1])?;
                            start_y = self.parse_number(&list[2])?;
                        }
                        "end" => {
                            end_x = self.parse_number(&list[1])?;
                            end_y = self.parse_number(&list[2])?;
                        }
                        "stroke" => {
                            if let Some(stroke_info) = self.parse_stroke(&list[1..])? {
                                stroke_width = stroke_info.0;
                                stroke_type = stroke_info.1;
                            }
                        }
                        "fill" => {
                            if let Some(Sexp::Atom(Atom::S(fill))) = list.get(1) {
                                fill_type = fill.clone();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(KiCadGraphic::Rectangle {
            start_x,
            start_y,
            end_x,
            end_y,
            stroke_width,
            stroke_type,
            fill_type,
        })
    }

    /// Parse stroke information
    fn parse_stroke(&self, content: &[Sexp]) -> Result<Option<(f64, String)>, KiCadParseError> {
        let mut width = 0.0;
        let mut stroke_type = "default".to_string();

        for item in content {
            if let Sexp::List(list) = item {
                if let Some(Sexp::Atom(Atom::S(tag))) = list.first() {
                    match tag.as_str() {
                        "width" => {
                            width = self.parse_number(&list[1])?;
                        }
                        "type" => {
                            if let Some(Sexp::Atom(Atom::S(st))) = list.get(1) {
                                stroke_type = st.clone();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }

        Ok(Some((width, stroke_type)))
    }

    /// Placeholder implementations for other graphics
    fn parse_circle(&self, _content: &[Sexp]) -> Result<KiCadGraphic, KiCadParseError> {
        // TODO: Implement circle parsing
        Ok(KiCadGraphic::Circle {
            center_x: 0.0,
            center_y: 0.0,
            radius: 0.0,
            stroke_width: 0.0,
            stroke_type: "default".to_string(),
            fill_type: "none".to_string(),
        })
    }

    fn parse_arc(&self, _content: &[Sexp]) -> Result<KiCadGraphic, KiCadParseError> {
        // TODO: Implement arc parsing
        Ok(KiCadGraphic::Arc {
            start_x: 0.0,
            start_y: 0.0,
            mid_x: 0.0,
            mid_y: 0.0,
            end_x: 0.0,
            end_y: 0.0,
            stroke_width: 0.0,
            stroke_type: "default".to_string(),
        })
    }

    fn parse_polyline(&self, _content: &[Sexp]) -> Result<KiCadGraphic, KiCadParseError> {
        // TODO: Implement polyline parsing
        Ok(KiCadGraphic::Polyline {
            points: vec![],
            stroke_width: 0.0,
            stroke_type: "default".to_string(),
        })
    }

    fn parse_text(&self, _content: &[Sexp]) -> Result<KiCadGraphic, KiCadParseError> {
        // TODO: Implement text parsing
        Ok(KiCadGraphic::Text {
            text: String::new(),
            x: 0.0,
            y: 0.0,
            angle: 0.0,
            effects: KiCadTextEffects {
                font_size: 1.0,
                thickness: None,
                bold: false,
                italic: false,
                hide: false,
            },
        })
    }
}

impl Default for KiCadSymbolParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_resistor() {
        let parser = KiCadSymbolParser::new();
        
        let kicad_content = r#"
(kicad_symbol_lib (version 20220914) (generator kicad_symbol_editor)
  (symbol "R" (pin_numbers hide) (pin_names (offset 0))
    (in_bom yes) (on_board yes)
    (property "Reference" "R" (at 2.032 0 90)
      (effects (font (size 1.27 1.27))))
    (property "Value" "R" (at 0 0 90)  
      (effects (font (size 1.27 1.27))))
    (property "Footprint" "" (at -1.778 0 90)
      (effects (font (size 1.27 1.27)) hide))
    (symbol "R_0_1"
      (rectangle (start -1.016 -2.54) (end 1.016 2.54)
        (stroke (width 0.254) (type default))
        (fill (type none))
      )
      (pin passive line (at 0 3.81 270) (length 1.27)
        (name "~" (effects (font (size 1.27 1.27))))
        (number "1" (effects (font (size 1.27 1.27))))
      )
      (pin passive line (at 0 -3.81 90) (length 1.27)
        (name "~" (effects (font (size 1.27 1.27))))
        (number "2" (effects (font (size 1.27 1.27))))
      )
    )
  )
)
"#;

        let symbols = parser.parse_symbol_library(kicad_content).unwrap();
        assert_eq!(symbols.len(), 1);
        
        let symbol = &symbols[0];
        assert_eq!(symbol.name, "R");
        assert_eq!(symbol.reference, "R");
        assert_eq!(symbol.value, "R");
        assert_eq!(symbol.units.len(), 1);
        
        let unit = &symbol.units[0];
        assert_eq!(unit.pins.len(), 2);
        assert_eq!(unit.graphics.len(), 1);
        
        // Check pins
        assert_eq!(unit.pins[0].number, "1");
        assert_eq!(unit.pins[0].electrical_type, "passive");
        assert_eq!(unit.pins[1].number, "2");
        assert_eq!(unit.pins[1].electrical_type, "passive");
    }

    #[test]
    fn test_parse_unit_convert_from_name() {
        let parser = KiCadSymbolParser::new();
        
        let (unit, convert) = parser.parse_unit_convert_from_name("R_0_1").unwrap();
        assert_eq!(unit, 0);
        assert_eq!(convert, 1);
        
        let (unit, convert) = parser.parse_unit_convert_from_name("IC_74HC00_1_2").unwrap();
        assert_eq!(unit, 1);
        assert_eq!(convert, 2);
    }

    #[test]
    fn test_parse_number() {
        let parser = KiCadSymbolParser::new();
        
        assert_eq!(parser.parse_number(&Sexp::Atom(Atom::F(1.5))).unwrap(), 1.5);
        assert_eq!(parser.parse_number(&Sexp::Atom(Atom::I(42))).unwrap(), 42.0);
        assert_eq!(parser.parse_number(&Sexp::Atom(Atom::S("3.14".to_string()))).unwrap(), 3.14);
    }
}