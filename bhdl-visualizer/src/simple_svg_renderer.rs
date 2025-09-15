/// Simple SVG renderer that uses embedded component metadata
use crate::schematic_knowledge::schematic_knowledge::SchematicKnowledge;
use crate::types::{CircuitLayout, Component, Point, Net, RoutingSegment, Junction};
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
                // LED - triangle pointing down with cathode bar (vertical)
                // Anode at top, cathode at bottom
                writeln!(svg,
                    r#"      <line x1="0" y1="-15" x2="0" y2="-8" stroke="black" stroke-width="2"/>
      <path d="M -8 -8 L 8 -8 L 0 2 Z" fill="none" stroke="black" stroke-width="2"/>
      <line x1="-8" y1="2" x2="8" y2="2" stroke="black" stroke-width="2"/>
      <line x1="0" y1="2" x2="0" y2="10" stroke="black" stroke-width="2"/>
      <path d="M 10 -5 l 5 -5 m -1 1 l 0 -2 l 2 0" stroke="black" stroke-width="1" fill="none"/>
      <path d="M 12 -2 l 5 -5 m -1 1 l 0 -2 l 2 0" stroke="black" stroke-width="1" fill="none"/>"#
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
        // If we have routed segments, use those. Otherwise fall back to simple connections
        if !net.routing_segments.is_empty() {
            // Draw routing segments from the orthogonal router
            for segment in &net.routing_segments {
                self.draw_routing_segment(svg, segment);
            }
        } else if net.connection_points.len() >= 2 {
            // Fallback: draw simple point-to-point connections
            for i in 1..net.connection_points.len() {
                writeln!(svg,
                    r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1.5"/>"#,
                    net.connection_points[i-1].x, net.connection_points[i-1].y,
                    net.connection_points[i].x, net.connection_points[i].y
                ).unwrap();
            }
        }
        
        // Draw junctions (dots where wires meet)
        for junction in &net.junctions {
            self.draw_junction(svg, junction);
        }
        
        // Add net label at the first connection point
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
    
    fn draw_routing_segment(&self, svg: &mut String, segment: &RoutingSegment) {
        match segment {
            RoutingSegment::Line { start, end } => {
                writeln!(svg,
                    r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1.5"/>"#,
                    start.x, start.y, end.x, end.y
                ).unwrap();
            }
            RoutingSegment::Arc { center, radius, start_angle, end_angle } => {
                // Convert angles to start and end points for SVG arc
                let start_x = center.x + radius * start_angle.to_radians().cos();
                let start_y = center.y + radius * start_angle.to_radians().sin();
                let end_x = center.x + radius * end_angle.to_radians().cos();
                let end_y = center.y + radius * end_angle.to_radians().sin();
                
                let large_arc = if (end_angle - start_angle).abs() > 180.0 { 1 } else { 0 };
                let sweep = if end_angle > start_angle { 1 } else { 0 };
                
                writeln!(svg,
                    r#"    <path d="M {} {} A {} {} 0 {} {} {} {}" fill="none" stroke="black" stroke-width="1.5"/>"#,
                    start_x, start_y, radius, radius, large_arc, sweep, end_x, end_y
                ).unwrap();
            }
        }
    }
    
    fn draw_junction(&self, svg: &mut String, junction: &Junction) {
        // Draw junction as a solid black circle
        writeln!(svg,
            r#"    <circle cx="{}" cy="{}" r="2" fill="black"/>"#,
            junction.position.x, junction.position.y
        ).unwrap();
    }
    
    fn draw_ground_net(&self, svg: &mut String, net: &Net) {
        // Draw ground as a horizontal rail with vertical drops
        if net.connection_points.len() < 2 {
            return;
        }
        
        // Find the ground rail (should be the first two points defining horizontal line)
        if net.connection_points.len() >= 2 {
            let rail_start = &net.connection_points[0];
            let rail_end = &net.connection_points[1];
            
            // Draw the main ground rail
            writeln!(svg,
                r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2"/>"#,
                rail_start.x, rail_start.y, rail_end.x, rail_end.y
            ).unwrap();
            
            // Draw vertical drops from components to rail
            let mut i = 2;
            while i < net.connection_points.len() {
                if i + 1 < net.connection_points.len() {
                    let drop_start = &net.connection_points[i];
                    let drop_end = &net.connection_points[i + 1];
                    
                    writeln!(svg,
                        r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="1.5"/>"#,
                        drop_start.x, drop_start.y, drop_end.x, drop_end.y
                    ).unwrap();
                    
                    // Add connection dot at junction
                    writeln!(svg,
                        r#"    <circle cx="{}" cy="{}" r="2" fill="black"/>"#,
                        drop_end.x, drop_end.y
                    ).unwrap();
                }
                i += 2;
            }
            
            // Add ground symbol at the end
            let gnd_x = rail_end.x - 20.0;
            let gnd_y = rail_end.y;
            writeln!(svg,
                r#"    <g transform="translate({}, {})">
      <line x1="0" y1="0" x2="0" y2="10" stroke="black" stroke-width="2"/>
      <line x1="-10" y1="10" x2="10" y2="10" stroke="black" stroke-width="2"/>
      <line x1="-7" y1="14" x2="7" y2="14" stroke="black" stroke-width="1.5"/>
      <line x1="-4" y1="18" x2="4" y2="18" stroke="black" stroke-width="1"/>
    </g>"#,
                gnd_x, gnd_y
            ).unwrap();
            
            // Add GND label
            writeln!(svg,
                r#"    <text x="{}" y="{}" font-family="Arial" font-size="10" font-weight="bold">GND</text>"#,
                gnd_x - 15.0, gnd_y + 35.0
            ).unwrap();
        }
    }
}