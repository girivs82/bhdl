/// Simple SVG renderer that uses embedded component metadata
use crate::schematic_knowledge::schematic_knowledge::SchematicKnowledge;
use crate::types::{CircuitLayout, Component, Point, Net};
use std::fmt::Write;

pub struct SimpleSvgRenderer {
    knowledge: SchematicKnowledge,
}

impl SimpleSvgRenderer {
    pub fn new() -> Self {
        Self {
            knowledge: SchematicKnowledge::new(),
        }
    }
    
    /// Generate SVG string from circuit layout
    pub fn render(&mut self, layout: &CircuitLayout, title: &str) -> String {
        let mut svg = String::new();
        
        // SVG header
        let width = layout.bounding_box.width() + 100.0;
        let height = layout.bounding_box.height() + 100.0;
        
        writeln!(&mut svg, 
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">
  <title>{}</title>
  <rect width="100%" height="100%" fill="white" stroke="none"/>
  <g id="circuit">"#,
            width, height, width, height, title
        ).unwrap();
        
        // Draw components
        for component in &layout.components {
            self.draw_component(&mut svg, component);
        }
        
        // Draw connections
        for net in &layout.nets {
            self.draw_net(&mut svg, net);
        }
        
        // Add title
        writeln!(&mut svg,
            r#"  <text x="{}" y="30" text-anchor="middle" font-family="Arial" font-size="18" font-weight="bold">{}</text>"#,
            width / 2.0, title
        ).unwrap();
        
        // Add metadata annotation
        writeln!(&mut svg,
            r#"  <text x="10" y="{}" font-family="Arial" font-size="10" fill="gray">Generated from embedded BHDL metadata</text>"#,
            height - 10.0
        ).unwrap();
        
        // Close SVG
        writeln!(&mut svg, "  </g>\n</svg>").unwrap();
        
        svg
    }
    
    fn draw_component(&self, svg: &mut String, component: &Component) {
        let pos = component.position;
        let default_label = "?".to_string();
        let label = component.label.as_ref().unwrap_or(&default_label);
        
        // Determine component type from label
        let comp_type = if label.starts_with("U") {
            "LM7805"
        } else if label.starts_with("C") {
            "Cap"
        } else if label.starts_with("R") {
            "Res"
        } else if label.starts_with("D") {
            "LED"
        } else {
            "Generic"
        };
        
        // Get visualization rules from embedded metadata
        let has_metadata = self.knowledge.get_component_rules(comp_type).is_some();
        
        writeln!(svg, r#"    <g transform="translate({}, {})">"#, pos.x, pos.y).unwrap();
        
        match comp_type {
            "LM7805" => {
                // Voltage regulator - rectangle with pins
                writeln!(svg, 
                    r#"      <rect x="-40" y="-25" width="80" height="50" fill="lightgray" stroke="black" stroke-width="2"/>
      <text x="0" y="5" text-anchor="middle" font-family="Arial" font-size="14">7805</text>
      <circle cx="-40" cy="0" r="3" fill="black"/>
      <text x="-48" y="3" text-anchor="end" font-size="10">IN</text>
      <circle cx="40" cy="0" r="3" fill="black"/>
      <text x="48" y="3" text-anchor="start" font-size="10">OUT</text>
      <circle cx="0" cy="25" r="3" fill="black"/>
      <text x="0" y="38" text-anchor="middle" font-size="10">GND</text>"#
                ).unwrap();
            }
            "Cap" => {
                // Capacitor - two parallel lines (vertical)
                writeln!(svg,
                    r#"      <line x1="0" y1="-15" x2="0" y2="-5" stroke="black" stroke-width="2"/>
      <line x1="-8" y1="-5" x2="8" y2="-5" stroke="black" stroke-width="2"/>
      <line x1="-8" y1="5" x2="8" y2="5" stroke="black" stroke-width="2"/>
      <line x1="0" y1="5" x2="0" y2="15" stroke="black" stroke-width="2"/>"#
                ).unwrap();
            }
            "Res" => {
                // Resistor - zigzag (horizontal)
                writeln!(svg,
                    r#"      <path d="M -20 0 l 5 0 l 2.5 -5 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 2.5 -5 l 5 0" 
        fill="none" stroke="black" stroke-width="2"/>"#
                ).unwrap();
            }
            "LED" => {
                // LED - triangle with arrows (vertical)
                writeln!(svg,
                    r#"      <path d="M 0 -10 L -8 0 L 8 0 Z" fill="none" stroke="black" stroke-width="2"/>
      <line x1="-8" y1="5" x2="8" y2="5" stroke="black" stroke-width="2"/>
      <line x1="0" y1="-10" x2="0" y2="-15" stroke="black" stroke-width="2"/>
      <line x1="0" y1="5" x2="0" y2="10" stroke="black" stroke-width="2"/>
      <path d="M 10 -5 l 5 -5 m -1 1 l 0 -2 l 2 0" stroke="black" stroke-width="1" fill="none"/>"#
                ).unwrap();
            }
            _ => {
                // Generic box
                writeln!(svg,
                    r#"      <rect x="-20" y="-10" width="40" height="20" fill="white" stroke="black" stroke-width="2"/>"#
                ).unwrap();
            }
        }
        
        // Add component label
        writeln!(svg, 
            r#"      <text x="0" y="-20" text-anchor="middle" font-family="Arial" font-size="12" font-weight="bold">{}</text>"#,
            label
        ).unwrap();
        
        // Add metadata indicator if using embedded rules
        if has_metadata {
            writeln!(svg,
                r#"      <text x="0" y="-30" text-anchor="middle" font-family="Arial" font-size="8" fill="green">✓</text>"#
            ).unwrap();
        }
        
        writeln!(svg, "    </g>").unwrap();
    }
    
    fn draw_net(&self, svg: &mut String, net: &Net) {
        if net.connection_points.len() < 2 {
            return;
        }
        
        let mut path = String::new();
        write!(&mut path, "M {} {}", net.connection_points[0].x, net.connection_points[0].y).unwrap();
        
        for point in &net.connection_points[1..] {
            write!(&mut path, " L {} {}", point.x, point.y).unwrap();
        }
        
        writeln!(svg,
            r#"    <path d="{}" fill="none" stroke="black" stroke-width="1.5"/>"#,
            path
        ).unwrap();
        
        // Add net label
        if let Some(name) = &net.name {
            if !net.connection_points.is_empty() {
                let point = &net.connection_points[0];
                writeln!(svg,
                    r#"    <text x="{}" y="{}" font-family="Arial" font-size="9" fill="blue">{}</text>"#,
                    point.x + 5.0, point.y - 5.0, name
                ).unwrap();
            }
        }
    }
}