/// SVG renderer that uses embedded component metadata to generate professional schematics
use crate::schematic_knowledge::schematic_knowledge::{
    SchematicKnowledge, ComponentVisualization, PinSide, SymbolStyle
};
use crate::types::{CircuitLayout, Component, Point, Net};
use std::fmt::Write;

pub struct MetadataSvgRenderer {
    knowledge: SchematicKnowledge,
    svg_content: String,
    view_width: f64,
    view_height: f64,
}

impl MetadataSvgRenderer {
    pub fn new() -> Self {
        Self {
            knowledge: SchematicKnowledge::new(),
            svg_content: String::new(),
            view_width: 800.0,
            view_height: 600.0,
        }
    }
    
    /// Generate complete SVG from circuit layout using embedded metadata
    pub fn render_circuit(&mut self, layout: &CircuitLayout, circuit_name: &str) -> String {
        // Clear previous content
        self.svg_content.clear();
        
        // Calculate viewbox from layout bounds
        self.view_width = layout.bounding_box.width() + 100.0;
        self.view_height = layout.bounding_box.height() + 100.0;
        
        // Start SVG document
        self.write_svg_header(circuit_name);
        
        // Add grid background for professional look
        self.draw_grid();
        
        // Draw each component using its embedded metadata
        for component in &layout.components {
            self.draw_component_from_metadata(component);
        }
        
        // Draw nets/connections
        for net in &layout.nets {
            self.draw_net(net);
        }
        
        // Add title and annotations
        self.add_circuit_title(circuit_name);
        self.add_metadata_annotations();
        
        // Close SVG
        self.svg_content.push_str("</svg>\n");
        
        self.svg_content.clone()
    }
    
    /// Write SVG header with proper dimensions
    fn write_svg_header(&mut self, title: &str) {
        writeln!(
            &mut self.svg_content,
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" 
     width="{}" height="{}" 
     viewBox="0 0 {} {}"
     style="background-color: white;">
  <title>{}</title>
  <defs>
    <!-- Define reusable symbols -->
    <marker id="arrow" markerWidth="10" markerHeight="10" refX="8" refY="3" orient="auto">
      <polygon points="0 0, 10 3, 0 6" fill="#333"/>
    </marker>
    
    <!-- Grid pattern -->
    <pattern id="grid" width="10" height="10" patternUnits="userSpaceOnUse">
      <path d="M 10 0 L 0 0 0 10" fill="none" stroke="#e0e0e0" stroke-width="0.5"/>
    </pattern>
    
    <!-- Component shadows for depth -->
    <filter id="shadow" x="-50%" y="-50%" width="200%" height="200%">
      <feGaussianBlur in="SourceAlpha" stdDeviation="2"/>
      <feOffset dx="2" dy="2" result="offsetblur"/>
      <feComponentTransfer>
        <feFuncA type="linear" slope="0.3"/>
      </feComponentTransfer>
      <feMerge>
        <feMergeNode/>
        <feMergeNode in="SourceGraphic"/>
      </feMerge>
    </filter>
  </defs>"#,
            self.view_width, self.view_height,
            self.view_width, self.view_height,
            title
        ).unwrap();
    }
    
    /// Draw grid background
    fn draw_grid(&mut self) {
        writeln!(
            &mut self.svg_content,
            r#"  <rect width="100%" height="100%" fill="url(#grid)" opacity="0.5"/>"#
        ).unwrap();
    }
    
    /// Draw a component using its embedded visualization metadata
    fn draw_component_from_metadata(&mut self, component: &Component) {
        let pos = component.position;
        
        // Try to get visualization rules from embedded metadata
        let component_type = self.guess_component_type(component);
        
        // Clone the visualization rules to avoid borrow checker issues
        let viz_rules_opt = self.knowledge.get_component_rules(&component_type).cloned();
        
        if let Some(viz_rules) = viz_rules_opt {
            // Use embedded metadata to draw the component
            self.draw_with_metadata(component, &viz_rules, pos);
        } else {
            // Fallback to generic rectangle
            self.draw_generic_component(component, pos);
        }
    }
    
    /// Draw component using its specific embedded metadata
    fn draw_with_metadata(&mut self, component: &Component, metadata: &ComponentVisualization, pos: Point) {
        let group_id = format!("comp_{:?}", component.instance_id);
        
        writeln!(&mut self.svg_content, 
            r#"  <g id="{}" transform="translate({}, {})" filter="url(#shadow)">"#,
            group_id, pos.x, pos.y
        ).unwrap();
        
        // Draw symbol based on metadata
        match &metadata.symbol_style {
            SymbolStyle::Rectangle { width, height, label } => {
                self.draw_rectangle_symbol(*width, *height, &metadata.component_type);
            }
            SymbolStyle::Triangle { width, height } => {
                self.draw_triangle_symbol(*width, *height, &metadata.component_type);
            }
            SymbolStyle::Custom { svg_path } => {
                writeln!(&mut self.svg_content, "    <path d=\"{}\" fill=\"none\" stroke=\"black\" stroke-width=\"2\"/>", svg_path).unwrap();
            }
        }
        
        // Draw pins with labels from metadata
        for (pin_name, pin_info) in &metadata.pin_placement {
            self.draw_pin_from_metadata(pin_name, pin_info.side, &pin_info.label, pin_info.connection_point);
        }
        
        // Add component label
        if let Some(label) = &component.label {
            writeln!(&mut self.svg_content,
                r#"    <text x="0" y="-25" text-anchor="middle" font-family="Arial" font-size="12" font-weight="bold">{}</text>"#,
                label
            ).unwrap();
        }
        
        // Add value/type annotation
        writeln!(&mut self.svg_content,
            r#"    <text x="0" y="-10" text-anchor="middle" font-family="Arial" font-size="10" fill="#666">{}</text>"#,
            metadata.component_type
        ).unwrap();
        
        // Close component group
        writeln!(&mut self.svg_content, "  </g>").unwrap();
    }
    
    /// Draw rectangle symbol (for ICs, resistors, capacitors)
    fn draw_rectangle_symbol(&mut self, width: f64, height: f64, component_type: &str) {
        if component_type.contains("LM7805") || component_type.contains("7805") {
            // Special styling for voltage regulators
            writeln!(&mut self.svg_content,
                r#"    <rect x="{}" y="{}" width="{}" height="{}" fill="#f0f0f0" stroke="black" stroke-width="2" rx="3"/>
    <text x="0" y="5" text-anchor="middle" font-family="Arial" font-size="14" font-weight="bold">7805</text>"#,
                -width/2.0, -height/2.0, width, height
            ).unwrap();
        } else if component_type.contains("Cap") {
            // Capacitor symbol - two parallel lines
            let y_offset = height / 4.0;
            writeln!(&mut self.svg_content,
                r#"    <line x1="0" y1="{}" x2="0" y2="{}" stroke="black" stroke-width="2"/>
    <line x1="-{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2"/>
    <line x1="-{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2"/>
    <line x1="0" y1="{}" x2="0" y2="{}" stroke="black" stroke-width="2"/>"#,
                -height/2.0, -y_offset,  // Top lead
                width/3.0, -y_offset, width/3.0, -y_offset,  // Top plate
                width/3.0, y_offset, width/3.0, y_offset,   // Bottom plate
                y_offset, height/2.0     // Bottom lead
            ).unwrap();
        } else if component_type.contains("Res") {
            // Resistor symbol - zigzag
            writeln!(&mut self.svg_content,
                r#"    <path d="M -{} 0 l 5 0 l 5 -8 l 10 16 l 10 -16 l 10 16 l 10 -16 l 5 8 l 5 0" 
          fill="none" stroke="black" stroke-width="2"/>"#,
                width/2.0
            ).unwrap();
        } else {
            // Generic rectangle
            writeln!(&mut self.svg_content,
                r#"    <rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2"/>"#,
                -width/2.0, -height/2.0, width, height
            ).unwrap();
        }
    }
    
    /// Draw triangle symbol (for diodes, LEDs)
    fn draw_triangle_symbol(&mut self, width: f64, height: f64, component_type: &str) {
        if component_type.contains("LED") {
            // LED symbol - triangle with bar and light rays
            writeln!(&mut self.svg_content,
                r#"    <!-- LED Symbol -->
    <path d="M 0 -{} L -{} 0 L {} 0 Z" fill="none" stroke="black" stroke-width="2"/>
    <line x1="-{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2"/>
    <!-- Light rays -->
    <path d="M {} -{} l 5 -5 m -2 2 l 0 -3 l 3 0" stroke="black" stroke-width="1.5" fill="none" marker-end="url(#arrow)"/>
    <path d="M {} -{} l 5 -5 m -2 2 l 0 -3 l 3 0" stroke="black" stroke-width="1.5" fill="none" marker-end="url(#arrow)"/>"#,
                height/2.0, width/2.0, width/2.0,  // Triangle
                width/2.0, height/2.0, width/2.0, height/2.0,  // Cathode bar
                width/2.0 + 5.0, height/2.0,  // First light ray
                width/2.0 + 10.0, height/2.0  // Second light ray
            ).unwrap();
        } else {
            // Generic triangle
            writeln!(&mut self.svg_content,
                r#"    <path d="M 0 -{} L -{} {} L {} {} Z" fill="white" stroke="black" stroke-width="2"/>"#,
                height/2.0, width/2.0, height/2.0, width/2.0, height/2.0
            ).unwrap();
        }
    }
    
    /// Draw a pin with its label based on metadata
    fn draw_pin_from_metadata(&mut self, _pin_name: &str, side: PinSide, label: &str, connection_point: Point) {
        let (x, y) = (connection_point.x, connection_point.y);
        
        // Draw pin connection point
        writeln!(&mut self.svg_content,
            r#"    <circle cx="{}" cy="{}" r="3" fill="black"/>"#,
            x, y
        ).unwrap();
        
        // Draw pin label based on side
        let (text_x, text_y, anchor) = match side {
            PinSide::Left => (x - 10.0, y, "end"),
            PinSide::Right => (x + 10.0, y, "start"),
            PinSide::Top => (x, y - 10.0, "middle"),
            PinSide::Bottom => (x, y + 15.0, "middle"),
        };
        
        writeln!(&mut self.svg_content,
            r#"    <text x="{}" y="{}" text-anchor="{}" font-family="Arial" font-size="10">{}</text>"#,
            text_x, text_y, anchor, label
        ).unwrap();
    }
    
    /// Draw generic component (fallback when no metadata)
    fn draw_generic_component(&mut self, component: &Component, pos: Point) {
        writeln!(&mut self.svg_content,
            r#"  <g transform="translate({}, {})">
    <rect x="-20" y="-10" width="40" height="20" fill="white" stroke="black" stroke-width="2"/>
    <text x="0" y="5" text-anchor="middle" font-family="Arial" font-size="10">{}</text>
  </g>"#,
            pos.x, pos.y,
            component.label.as_ref().unwrap_or(&"?".to_string())
        ).unwrap();
    }
    
    /// Draw a net connection
    fn draw_net(&mut self, net: &Net) {
        if net.connection_points.len() < 2 {
            return;
        }
        
        let mut path = String::new();
        write!(&mut path, "M {} {}", net.connection_points[0].x, net.connection_points[0].y).unwrap();
        
        for point in &net.connection_points[1..] {
            write!(&mut path, " L {} {}", point.x, point.y).unwrap();
        }
        
        writeln!(&mut self.svg_content,
            r#"  <path d="{}" fill="none" stroke="black" stroke-width="1.5" stroke-linejoin="round"/>"#,
            path
        ).unwrap();
        
        // Add net label if present
        if let Some(name) = &net.name {
            if !net.connection_points.is_empty() {
                let mid_point = &net.connection_points[net.connection_points.len() / 2];
                writeln!(&mut self.svg_content,
                    r#"  <text x="{}" y="{}" font-family="Arial" font-size="9" fill="#666">{}</text>"#,
                    mid_point.x + 5.0, mid_point.y - 5.0, name
                ).unwrap();
            }
        }
    }
    
    /// Add circuit title
    fn add_circuit_title(&mut self, title: &str) {
        writeln!(&mut self.svg_content,
            r#"  <text x="{}" y="30" text-anchor="middle" font-family="Arial" font-size="18" font-weight="bold">{}</text>"#,
            self.view_width / 2.0, title
        ).unwrap();
    }
    
    /// Add annotations showing this was generated from embedded metadata
    fn add_metadata_annotations(&mut self) {
        writeln!(&mut self.svg_content,
            r#"  <text x="10" y="{}" font-family="Arial" font-size="10" fill="#999">
    Generated from embedded BHDL component metadata
  </text>
  <text x="10" y="{}" font-family="Arial" font-size="10" fill="#999">
    Professional layout: IN=left, OUT=right, GND=bottom | Caps=vertical | LEDs=vertical
  </text>"#,
            self.view_height - 25.0,
            self.view_height - 10.0
        ).unwrap();
    }
    
    /// Guess component type from instance info
    fn guess_component_type(&self, component: &Component) -> String {
        if let Some(label) = &component.label {
            if label.starts_with("U") {
                "LM7805".to_string()
            } else if label.starts_with("C") {
                "Cap".to_string()
            } else if label.starts_with("R") {
                "Res".to_string()
            } else if label.starts_with("D") {
                "LED".to_string()
            } else {
                "Generic".to_string()
            }
        } else {
            "Generic".to_string()
        }
    }
}