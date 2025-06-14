//! KiCad symbol to SVG converter
//! 
//! Converts KiCad symbol graphics to SVG format for database storage

use crate::kicad::parser::{KiCadSymbol, KiCadGraphic, KiCadPin, KiCadTextEffects};
use std::fmt::Write;

/// SVG converter for KiCad symbols
pub struct KiCadSvgConverter {
    /// Scale factor for conversion (KiCad units to SVG units)
    scale: f64,
    /// Default stroke width
    default_stroke_width: f64,
}

impl KiCadSvgConverter {
    pub fn new() -> Self {
        Self {
            scale: 1.0, // 1:1 scale by default
            default_stroke_width: 0.254, // KiCad default
        }
    }

    /// Convert a KiCad symbol to SVG
    pub fn convert_symbol_to_svg(&self, symbol: &KiCadSymbol) -> anyhow::Result<String> {
        let mut svg = String::new();
        
        // Calculate bounding box for the symbol
        let bbox = self.calculate_bounding_box(symbol);
        
        // Start SVG with viewBox
        writeln!(svg, r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="{} {} {} {}""#,
                bbox.x, bbox.y, bbox.width, bbox.height)?;
        writeln!(svg, r#"     xmlns:xlink="http://www.w3.org/1999/xlink">"#)?;
        
        // Add CSS styles
        writeln!(svg, "  <defs>")?;
        writeln!(svg, "    <style type=\"text/css\"><![CDATA[")?;
        writeln!(svg, "      .pin-line {{ stroke: #000; stroke-width: 0.254; fill: none; }}")?;
        writeln!(svg, "      .symbol-line {{ stroke: #000; stroke-width: 0.254; fill: none; }}")?;
        writeln!(svg, "      .symbol-text {{ font-family: monospace; font-size: 1.27px; fill: #000; }}")?;
        writeln!(svg, "      .pin-text {{ font-family: monospace; font-size: 1.0px; fill: #000; }}")?;
        writeln!(svg, "    ]]></style>")?;
        writeln!(svg, "  </defs>")?;
        
        // Convert all units (graphics from all units)
        for unit in &symbol.units {
            // Convert graphics
            for graphic in &unit.graphics {
                self.convert_graphic_to_svg(&mut svg, graphic)?;
            }
            
            // Convert pins
            for pin in &unit.pins {
                self.convert_pin_to_svg(&mut svg, pin)?;
            }
        }
        
        // Add symbol reference and value text
        if !symbol.reference.is_empty() {
            writeln!(svg, r#"  <text x="{}" y="{}" class="symbol-text" text-anchor="middle">{}</text>"#,
                    bbox.x + bbox.width / 2.0, bbox.y - 1.27, symbol.reference)?;
        }
        
        if !symbol.value.is_empty() {
            writeln!(svg, r#"  <text x="{}" y="{}" class="symbol-text" text-anchor="middle">{}</text>"#,
                    bbox.x + bbox.width / 2.0, bbox.y + bbox.height + 2.54, symbol.value)?;
        }
        
        writeln!(svg, "</svg>")?;
        
        Ok(svg)
    }
    
    /// Calculate bounding box for the entire symbol
    fn calculate_bounding_box(&self, symbol: &KiCadSymbol) -> BoundingBox {
        let mut min_x = f64::INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        
        for unit in &symbol.units {
            // Check graphics
            for graphic in &unit.graphics {
                let bbox = self.get_graphic_bounds(graphic);
                min_x = min_x.min(bbox.x);
                min_y = min_y.min(bbox.y);
                max_x = max_x.max(bbox.x + bbox.width);
                max_y = max_y.max(bbox.y + bbox.height);
            }
            
            // Check pins
            for pin in &unit.pins {
                let pin_bbox = self.get_pin_bounds(pin);
                min_x = min_x.min(pin_bbox.x);
                min_y = min_y.min(pin_bbox.y);
                max_x = max_x.max(pin_bbox.x + pin_bbox.width);
                max_y = max_y.max(pin_bbox.y + pin_bbox.height);
            }
        }
        
        // Add padding
        let padding = 5.08; // 2 * 2.54mm
        min_x -= padding;
        min_y -= padding;
        max_x += padding;
        max_y += padding;
        
        // Ensure minimum size
        if max_x <= min_x {
            max_x = min_x + 10.16;
        }
        if max_y <= min_y {
            max_y = min_y + 10.16;
        }
        
        BoundingBox {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }
    
    /// Convert a graphic element to SVG
    fn convert_graphic_to_svg(&self, svg: &mut String, graphic: &KiCadGraphic) -> anyhow::Result<()> {
        match graphic {
            KiCadGraphic::Rectangle { start_x, start_y, end_x, end_y, stroke_width, stroke_type, fill_type } => {
                let x = start_x.min(*end_x);
                let y = start_y.min(*end_y);
                let width = (end_x - start_x).abs();
                let height = (end_y - start_y).abs();
                
                let fill = if fill_type == "none" { "none" } else { "#f0f0f0" };
                
                writeln!(svg, "  <rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" stroke=\"#000\" stroke-width=\"{}\" fill=\"{}\"/>",
                        x, y, width, height, stroke_width, fill)?;
            }
            
            KiCadGraphic::Circle { center_x, center_y, radius, stroke_width, stroke_type, fill_type } => {
                let fill = if fill_type == "none" { "none" } else { "#f0f0f0" };
                
                writeln!(svg, "  <circle cx=\"{}\" cy=\"{}\" r=\"{}\" stroke=\"#000\" stroke-width=\"{}\" fill=\"{}\"/>",
                        center_x, center_y, radius, stroke_width, fill)?;
            }
            
            KiCadGraphic::Arc { start_x, start_y, mid_x, mid_y, end_x, end_y, stroke_width, stroke_type } => {
                // For simplicity, draw as polyline for now
                writeln!(svg, "  <polyline points=\"{},{} {},{} {},{}\" stroke=\"#000\" stroke-width=\"{}\" fill=\"none\"/>",
                        start_x, start_y, mid_x, mid_y, end_x, end_y, stroke_width)?;
            }
            
            KiCadGraphic::Polyline { points, stroke_width, stroke_type } => {
                if !points.is_empty() {
                    let mut point_str = String::new();
                    for (i, (x, y)) in points.iter().enumerate() {
                        if i > 0 {
                            point_str.push(' ');
                        }
                        write!(point_str, "{},{}", x, y)?;
                    }
                    
                    writeln!(svg, "  <polyline points=\"{}\" stroke=\"#000\" stroke-width=\"{}\" fill=\"none\"/>",
                            point_str, stroke_width)?;
                }
            }
            
            KiCadGraphic::Text { text, x, y, angle, effects } => {
                let font_size = effects.font_size;
                let weight = if effects.bold { "bold" } else { "normal" };
                let style = if effects.italic { "italic" } else { "normal" };
                let visibility = if effects.hide { "hidden" } else { "visible" };
                
                writeln!(svg, r#"  <text x="{}" y="{}" font-size="{}" font-weight="{}" font-style="{}" visibility="{}" class="symbol-text">{}</text>"#,
                        x, y, font_size, weight, style, visibility, text)?;
            }
        }
        
        Ok(())
    }
    
    /// Convert a pin to SVG
    fn convert_pin_to_svg(&self, svg: &mut String, pin: &KiCadPin) -> anyhow::Result<()> {
        // Calculate pin end position based on orientation and length
        let (end_x, end_y) = match pin.orientation {
            0 => (pin.x + pin.length, pin.y),     // Right
            90 => (pin.x, pin.y + pin.length),    // Up
            180 => (pin.x - pin.length, pin.y),   // Left
            270 => (pin.x, pin.y - pin.length),   // Down
            _ => (pin.x + pin.length, pin.y),     // Default to right
        };
        
        // Draw pin line
        writeln!(svg, r#"  <line x1="{}" y1="{}" x2="{}" y2="{}" class="pin-line"/>"#,
                pin.x, pin.y, end_x, end_y)?;
        
        // Add pin number
        if !pin.number.is_empty() {
            let (text_x, text_y) = self.calculate_pin_text_position(pin, true);
            writeln!(svg, r#"  <text x="{}" y="{}" class="pin-text" text-anchor="middle">{}</text>"#,
                    text_x, text_y, pin.number)?;
        }
        
        // Add pin name
        if !pin.name.is_empty() && pin.name != "~" {
            let (text_x, text_y) = self.calculate_pin_text_position(pin, false);
            writeln!(svg, r#"  <text x="{}" y="{}" class="pin-text" text-anchor="start">{}</text>"#,
                    text_x, text_y, pin.name)?;
        }
        
        Ok(())
    }
    
    /// Calculate text position for pin number or name
    fn calculate_pin_text_position(&self, pin: &KiCadPin, is_number: bool) -> (f64, f64) {
        let offset = if is_number { 1.27 } else { 2.54 };
        
        match pin.orientation {
            0 => {  // Right
                if is_number {
                    (pin.x - offset, pin.y - 0.5)
                } else {
                    (pin.x + offset, pin.y - 0.5)
                }
            }
            90 => {  // Up
                if is_number {
                    (pin.x + 0.5, pin.y - offset)
                } else {
                    (pin.x + 0.5, pin.y + offset)
                }
            }
            180 => {  // Left
                if is_number {
                    (pin.x + offset, pin.y - 0.5)
                } else {
                    (pin.x - offset, pin.y - 0.5)
                }
            }
            270 => {  // Down
                if is_number {
                    (pin.x + 0.5, pin.y + offset)
                } else {
                    (pin.x + 0.5, pin.y - offset)
                }
            }
            _ => (pin.x, pin.y)
        }
    }
    
    /// Get bounding box for a graphic element
    fn get_graphic_bounds(&self, graphic: &KiCadGraphic) -> BoundingBox {
        match graphic {
            KiCadGraphic::Rectangle { start_x, start_y, end_x, end_y, .. } => {
                let x = start_x.min(*end_x);
                let y = start_y.min(*end_y);
                BoundingBox {
                    x,
                    y,
                    width: (end_x - start_x).abs(),
                    height: (end_y - start_y).abs(),
                }
            }
            
            KiCadGraphic::Circle { center_x, center_y, radius, .. } => {
                BoundingBox {
                    x: center_x - radius,
                    y: center_y - radius,
                    width: radius * 2.0,
                    height: radius * 2.0,
                }
            }
            
            KiCadGraphic::Arc { start_x, start_y, end_x, end_y, .. } => {
                let x = start_x.min(*end_x);
                let y = start_y.min(*end_y);
                BoundingBox {
                    x,
                    y,
                    width: (end_x - start_x).abs(),
                    height: (end_y - start_y).abs(),
                }
            }
            
            KiCadGraphic::Polyline { points, .. } => {
                if points.is_empty() {
                    return BoundingBox { x: 0.0, y: 0.0, width: 0.0, height: 0.0 };
                }
                
                let mut min_x = points[0].0;
                let mut min_y = points[0].1;
                let mut max_x = points[0].0;
                let mut max_y = points[0].1;
                
                for (x, y) in points {
                    min_x = min_x.min(*x);
                    min_y = min_y.min(*y);
                    max_x = max_x.max(*x);
                    max_y = max_y.max(*y);
                }
                
                BoundingBox {
                    x: min_x,
                    y: min_y,
                    width: max_x - min_x,
                    height: max_y - min_y,
                }
            }
            
            KiCadGraphic::Text { x, y, effects, .. } => {
                // Approximate text bounds
                let width = effects.font_size * 6.0; // Rough estimate
                let height = effects.font_size;
                BoundingBox {
                    x: *x,
                    y: *y - height,
                    width,
                    height,
                }
            }
        }
    }
    
    /// Get bounding box for a pin
    fn get_pin_bounds(&self, pin: &KiCadPin) -> BoundingBox {
        let (end_x, end_y) = match pin.orientation {
            0 => (pin.x + pin.length, pin.y),
            90 => (pin.x, pin.y + pin.length),
            180 => (pin.x - pin.length, pin.y),
            270 => (pin.x, pin.y - pin.length),
            _ => (pin.x + pin.length, pin.y),
        };
        
        let min_x = pin.x.min(end_x) - 2.54; // Extra space for text
        let min_y = pin.y.min(end_y) - 1.27;
        let max_x = pin.x.max(end_x) + 2.54;
        let max_y = pin.y.max(end_y) + 1.27;
        
        BoundingBox {
            x: min_x,
            y: min_y,
            width: max_x - min_x,
            height: max_y - min_y,
        }
    }
}

impl Default for KiCadSvgConverter {
    fn default() -> Self {
        Self::new()
    }
}

/// Bounding box for graphics calculations
#[derive(Debug, Clone)]
struct BoundingBox {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kicad::parser::{KiCadSymbol, KiCadUnit, KiCadGraphic, KiCadPin};
    use std::collections::HashMap;

    #[test]
    fn test_svg_conversion() {
        let converter = KiCadSvgConverter::new();
        
        // Create a simple test symbol
        let symbol = KiCadSymbol {
            name: "R".to_string(),
            description: None,
            keywords: None,
            reference: "R".to_string(),
            value: "1k".to_string(),
            footprint: None,
            datasheet: None,
            properties: HashMap::new(),
            pins: vec![],
            graphics: vec![],
            units: vec![
                KiCadUnit {
                    unit_id: 0,
                    convert_id: 1,
                    pins: vec![
                        KiCadPin {
                            number: "1".to_string(),
                            name: "~".to_string(),
                            electrical_type: "passive".to_string(),
                            graphic_style: "line".to_string(),
                            x: 0.0,
                            y: 0.0,
                            length: 2.54,
                            orientation: 0,
                            name_effects: None,
                            number_effects: None,
                        },
                        KiCadPin {
                            number: "2".to_string(),
                            name: "~".to_string(),
                            electrical_type: "passive".to_string(),
                            graphic_style: "line".to_string(),
                            x: 10.16,
                            y: 0.0,
                            length: 2.54,
                            orientation: 180,
                            name_effects: None,
                            number_effects: None,
                        },
                    ],
                    graphics: vec![
                        KiCadGraphic::Rectangle {
                            start_x: 2.54,
                            start_y: -1.27,
                            end_x: 7.62,
                            end_y: 1.27,
                            stroke_width: 0.254,
                            stroke_type: "default".to_string(),
                            fill_type: "none".to_string(),
                        },
                    ],
                }
            ],
        };
        
        let svg = converter.convert_symbol_to_svg(&symbol).unwrap();
        
        // Basic validation
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("viewBox"));
        assert!(svg.contains("<rect")); // Rectangle graphic
        assert!(svg.contains("<line")); // Pin lines
        assert!(svg.contains("R")); // Reference
        assert!(svg.contains("1k")); // Value
    }
    
    #[test]
    fn test_bounding_box_calculation() {
        let converter = KiCadSvgConverter::new();
        
        let symbol = KiCadSymbol {
            name: "Test".to_string(),
            description: None,
            keywords: None,
            reference: "U".to_string(),
            value: "Test".to_string(),
            footprint: None,
            datasheet: None,
            properties: HashMap::new(),
            pins: vec![],
            graphics: vec![],
            units: vec![
                KiCadUnit {
                    unit_id: 0,
                    convert_id: 1,
                    pins: vec![],
                    graphics: vec![
                        KiCadGraphic::Rectangle {
                            start_x: 0.0,
                            start_y: 0.0,
                            end_x: 10.0,
                            end_y: 5.0,
                            stroke_width: 0.254,
                            stroke_type: "default".to_string(),
                            fill_type: "none".to_string(),
                        },
                    ],
                }
            ],
        };
        
        let bbox = converter.calculate_bounding_box(&symbol);
        
        // Should include padding
        assert!(bbox.width > 10.0);
        assert!(bbox.height > 5.0);
        assert!(bbox.x < 0.0);
        assert!(bbox.y < 0.0);
    }
}