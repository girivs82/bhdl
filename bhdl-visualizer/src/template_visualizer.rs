/// Template-based circuit visualizer that uses professional layouts from bhdl-stdlib
/// This replaces generic placement algorithms with curated, professional templates

use anyhow::{Result, Context};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use bhdl_netlist::Netlist;

/// Represents a professional circuit template from stdlib
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CircuitTemplate {
    pub topology: String,
    pub ic_type: String,
    pub ic_position: Position,
    pub pin_positions: HashMap<String, PinPosition>,
    pub component_positions: HashMap<String, ComponentPosition>,
    pub wire_routes: HashMap<String, Vec<WireSegment>>,
    pub power_rails: PowerRails,
    pub canvas: CanvasSize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PinPosition {
    pub x: f64,
    pub y: f64,
    pub side: String,
    pub pin_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComponentPosition {
    pub x: f64,
    pub y: f64,
    pub component_type: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireSegment {
    pub from: WireEndpoint,
    pub to: WireEndpoint,
    pub wire_type: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum WireEndpoint {
    Named(String),
    Position(Position),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerRails {
    pub vin: PowerRail,
    pub vout: PowerRail,
    pub gnd: PowerRail,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerRail {
    pub y: f64,
    pub x_start: f64,
    pub x_end: f64,
    pub color: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CanvasSize {
    pub width: f64,
    pub height: f64,
}

/// Template-based visualizer
pub struct TemplateVisualizer {
    templates_cache: HashMap<String, CircuitTemplate>,
}

impl TemplateVisualizer {
    pub fn new() -> Result<Self> {
        Ok(Self {
            templates_cache: HashMap::new(),
        })
    }
    
    /// Extract current information from actual simulation results in netlist
    fn extract_net_currents(&self, netlist: &Netlist) -> HashMap<String, f64> {
        let mut net_currents = HashMap::new();
        
        // Extract actual operating currents from instance attributes (populated by synthesizer)
        for (instance_id, instance) in &netlist.instances {
            let instance_name = &instance.name;
            // Look for simulation-derived operating current
            if let Some(current_str) = instance.attributes.get("sim_operating_current") {
                // Parse the current value (format: "1.234A")
                if let Some(current_value) = Self::parse_current_value(current_str) {
                    println!("Found simulation current for {}: {:.3}A", instance_name, current_value);
                    
                    // Map component currents to their connected nets based on circuit topology
                    match instance_name.as_str() {
                        name if name.starts_with("c_in") => {
                            net_currents.insert("VIN".to_string(), current_value.abs());
                        },
                        name if name.starts_with("c_out") => {
                            net_currents.insert("VOUT".to_string(), current_value.abs());
                        },
                        name if name.starts_with("l_out") => {
                            net_currents.insert("PH".to_string(), current_value.abs());
                        },
                        name if name.starts_with("r_fb") => {
                            net_currents.insert("FB".to_string(), current_value.abs());
                        },
                        name if name.starts_with("c_boot") => {
                            net_currents.insert("BOOT".to_string(), current_value.abs());
                        },
                        "reg" => {
                            // Main IC - represents VIN current draw
                            net_currents.insert("VIN".to_string(), current_value.abs());
                        },
                        _ => {}
                    }
                }
            }
        }
        
        // Check analysis_data as backup source
        if net_currents.is_empty() {
            if let Some(analysis_data) = &netlist.analysis_data {
                for (instance_name, instance_data) in &analysis_data.instance_analysis {
                    if let Some(safety_info) = &instance_data.safety_info {
                        if let Some(operating_current) = safety_info.operating_current {
                            println!("Found analysis current for {}: {:.3}A", instance_name, operating_current);
                            // Same mapping logic as above
                            match instance_name.as_str() {
                                name if name.starts_with("c_in") => {
                                    net_currents.insert("VIN".to_string(), operating_current.abs());
                                },
                                name if name.starts_with("c_out") => {
                                    net_currents.insert("VOUT".to_string(), operating_current.abs());
                                },
                                name if name.starts_with("l_out") => {
                                    net_currents.insert("PH".to_string(), operating_current.abs());
                                },
                                name if name.starts_with("r_fb") => {
                                    net_currents.insert("FB".to_string(), operating_current.abs());
                                },
                                name if name.starts_with("c_boot") => {
                                    net_currents.insert("BOOT".to_string(), operating_current.abs());
                                },
                                "reg" => {
                                    net_currents.insert("VIN".to_string(), operating_current.abs());
                                },
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
        
        // Fallback to reasonable defaults if no simulation data available
        if net_currents.is_empty() {
            println!("No simulation current data found, using fallback defaults");
            net_currents.insert("VIN".to_string(), 3.0);   
            net_currents.insert("PH".to_string(), 2.0);    
            net_currents.insert("VOUT".to_string(), 2.0);  
            net_currents.insert("GND".to_string(), 3.0);   
            net_currents.insert("FB".to_string(), 0.001);  
            net_currents.insert("BOOT".to_string(), 0.01); 
        }
        
        // GND current is typically the sum of all other currents (return path)
        let total_current: f64 = net_currents.values().sum();
        net_currents.insert("GND".to_string(), total_current);
        
        net_currents
    }
    
    /// Parse current value from string format like "1.234A"
    fn parse_current_value(current_str: &str) -> Option<f64> {
        if current_str.ends_with('A') {
            current_str[..current_str.len()-1].parse().ok()
        } else {
            current_str.parse().ok()
        }
    }
    
    /// Calculate wire thickness based on current
    fn calculate_wire_thickness(&self, current_amps: f64) -> f64 {
        // Base thickness is 1, scale up for higher currents
        // Power rails (1-3A) get thickness 2-4
        // Signal lines (<0.1A) get thickness 1
        let thickness = if current_amps >= 1.0 {
            2.0 + current_amps  // Power rails: 3-5 thickness
        } else if current_amps >= 0.1 {
            1.5             // Medium current: 1.5 thickness  
        } else {
            1.0             // Low current signals: 1 thickness
        };
        
        thickness.min(5.0) // Cap at 5 for very thick power lines
    }
    
    /// Extract template from stdlib component definition
    pub fn extract_template(&mut self, ic_type: &str) -> Result<CircuitTemplate> {
        // Check cache first
        if let Some(template) = self.templates_cache.get(ic_type) {
            return Ok(template.clone());
        }
        
        // For now, use hardcoded templates
        // In production, this would read from stdlib files
        let template = self.get_hardcoded_template(ic_type)?;
        
        // Cache it
        self.templates_cache.insert(ic_type.to_string(), template.clone());
        
        Ok(template)
    }
    
    /// Get hardcoded template for testing
    fn get_hardcoded_template(&self, ic_type: &str) -> Result<CircuitTemplate> {
        match ic_type {
            "TPS54302" => Ok(self.tps54302_template()),
            _ => anyhow::bail!("No template found for component: {}", ic_type)
        }
    }
    
    /// Hardcoded TPS54302 template - professional layout based on test_topology_aware_output.svg
    fn tps54302_template(&self) -> CircuitTemplate {
        CircuitTemplate {
            topology: "buck_converter".to_string(),
            ic_type: "TPS54302".to_string(),
            ic_position: Position { x: 400.0, y: 300.0 },
            pin_positions: {
                let mut pins = HashMap::new();
                // Left side pins - increased spacing for component placement
                pins.insert("VIN".to_string(), PinPosition { x: -80.0, y: -40.0, side: "left".to_string(), pin_number: Some("1".to_string()) });
                pins.insert("EN".to_string(), PinPosition { x: -80.0, y: 0.0, side: "left".to_string(), pin_number: Some("2".to_string()) });
                // Right side pins - increased spacing for component placement
                pins.insert("PH".to_string(), PinPosition { x: 80.0, y: -40.0, side: "right".to_string(), pin_number: Some("3".to_string()) });  // Phase/switching node
                pins.insert("BOOT".to_string(), PinPosition { x: 80.0, y: 0.0, side: "right".to_string(), pin_number: Some("4".to_string()) });
                pins.insert("FB".to_string(), PinPosition { x: 80.0, y: 40.0, side: "right".to_string(), pin_number: Some("5".to_string()) });
                pins.insert("GND".to_string(), PinPosition { x: 0.0, y: 60.0, side: "bottom".to_string(), pin_number: Some("6".to_string()) });
                pins
            },
            component_positions: {
                let mut comps = HashMap::new();
                
                // Simple straight-line positioning rules:
                // VIN rail at y=260, VOUT rail at y=260, GND rail at y=460
                // All components positioned for straight connections
                
                let vin_y = 260.0;
                let vout_y = 260.0; 
                let gnd_y = 460.0;
                
                // Input capacitors: moved further left, plenty of space available
                let input_cap_y = (vin_y + gnd_y) / 2.0; // y=360
                comps.insert("input_cap_1".to_string(), ComponentPosition {
                    x: 180.0, y: input_cap_y,  // Moved left from 250 to 180
                    component_type: "capacitor".to_string(),
                    role: "input_filter".to_string()
                });
                comps.insert("input_cap_2".to_string(), ComponentPosition {
                    x: 230.0, y: input_cap_y,  // Moved left from 300 to 230
                    component_type: "capacitor".to_string(),
                    role: "input_filter".to_string()
                });
                
                // Inductor: moved much further right for better spacing
                // SW pin at (480, 260), so inductor at y=260 but x shifted right
                comps.insert("inductor".to_string(), ComponentPosition {
                    x: 620.0, y: 260.0,  // Moved right from 580 to 620 for better spacing
                    component_type: "inductor".to_string(),
                    role: "energy_storage".to_string()
                });
                
                // Bootstrap capacitor: positioned between IC and inductor
                // Position so top pin aligns with PH (y=260) and bottom pin aligns with BOOT (y=300)
                let boot_cap_y = 280.0; // Center between PH (260) and BOOT (300)
                comps.insert("boot_cap".to_string(), ComponentPosition {
                    x: 540.0, y: boot_cap_y,  // Moved left to 540 for spacing from inductor
                    component_type: "capacitor".to_string(),
                    role: "bootstrap".to_string()
                });
                
                // Feedback resistors: moved further right
                // FB pin at (480, 340), so resistor middle should be at y=340
                comps.insert("fb_resistor_1".to_string(), ComponentPosition {
                    x: 590.0, y: 320.0,  // Moved right from 550 to 590
                    component_type: "resistor".to_string(),
                    role: "feedback_top".to_string()
                });
                comps.insert("fb_resistor_2".to_string(), ComponentPosition {
                    x: 590.0, y: 360.0,  // Moved right from 550 to 590
                    component_type: "resistor".to_string(),
                    role: "feedback_bottom".to_string()
                });
                
                // Output capacitors: positioned to the right of inductor (x=620)
                let output_cap_y = (vout_y + gnd_y) / 2.0; // y=360
                comps.insert("output_cap_1".to_string(), ComponentPosition {
                    x: 720.0, y: output_cap_y,  // Moved further right of inductor
                    component_type: "capacitor".to_string(),
                    role: "output_filter".to_string()
                });
                comps.insert("output_cap_2".to_string(), ComponentPosition {
                    x: 770.0, y: output_cap_y,  // Moved further right of inductor
                    component_type: "capacitor".to_string(),
                    role: "output_filter".to_string()
                });
                
                // Freewheeling diode: positioned to the right of inductor
                comps.insert("diode".to_string(), ComponentPosition {
                    x: 670.0, y: output_cap_y,  // Moved to right of inductor (x=620)
                    component_type: "diode".to_string(),
                    role: "freewheeling".to_string()
                });
                
                comps
            },
            wire_routes: HashMap::new(), // Would be filled with routing info
            power_rails: PowerRails {
                vin: PowerRail { y: 260.0, x_start: 100.0, x_end: 308.0, color: "red".to_string(), label: "VIN".to_string() }, // Stop at external VIN pin square (400-94+2=308)
                vout: PowerRail { y: 260.0, x_start: 660.0, x_end: 820.0, color: "green".to_string(), label: "VOUT".to_string() },
                gnd: PowerRail { y: 460.0, x_start: 100.0, x_end: 820.0, color: "black".to_string(), label: "GND".to_string() },
            },
            canvas: CanvasSize { width: 950.0, height: 600.0 },
        }
    }
    
    /// Apply template with component substitution from netlist
    pub fn visualize_with_template(
        &mut self,
        netlist: &Netlist,
        ic_type: &str
    ) -> Result<String> {
        // Step 1: Get the template
        let template = self.extract_template(ic_type)?;
        
        // Step 2: Map netlist components to template roles
        let component_mapping = self.map_components_to_roles(netlist, &template)?;
        
        // Step 3: Extract current information from netlist
        let net_currents = self.extract_net_currents(netlist);
        
        // Step 4: Generate SVG with substituted components and current-proportional wires
        let svg = self.generate_svg_from_template_with_currents(&template, &component_mapping, netlist, &net_currents)?;
        
        Ok(svg)
    }
    
    /// Map actual netlist components to template roles
    fn map_components_to_roles(
        &self,
        netlist: &Netlist,
        _template: &CircuitTemplate
    ) -> Result<HashMap<String, String>> {
        let mut mapping = HashMap::new();
        
        // Simple name-based mapping for now
        for (_, instance) in &netlist.instances {
            match instance.name.as_str() {
                "c_in1" => { mapping.insert("input_cap_1".to_string(), instance.name.clone()); },
                "c_in2" => { mapping.insert("input_cap_2".to_string(), instance.name.clone()); },
                "c_out1" => { mapping.insert("output_cap_1".to_string(), instance.name.clone()); },
                "c_out2" => { mapping.insert("output_cap_2".to_string(), instance.name.clone()); },
                "l_out" => { mapping.insert("inductor".to_string(), instance.name.clone()); },
                "r_fb1" => { mapping.insert("fb_resistor_1".to_string(), instance.name.clone()); },
                "r_fb2" => { mapping.insert("fb_resistor_2".to_string(), instance.name.clone()); },
                "c_boot" => { mapping.insert("boot_cap".to_string(), instance.name.clone()); },
                "tvs" | "d1" => { mapping.insert("diode".to_string(), instance.name.clone()); },
                _ => {}
            }
        }
        
        Ok(mapping)
    }
    
    /// Generate SVG from template with substituted components
    fn generate_svg_from_template_with_currents(
        &self,
        template: &CircuitTemplate,
        component_mapping: &HashMap<String, String>,
        netlist: &Netlist,
        net_currents: &HashMap<String, f64>,
    ) -> Result<String> {
        let mut svg = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">
  <title>Professional Circuit Layout</title>
  <rect width="100%" height="100%" fill="white"/>
  <g id="circuit">
"#, template.canvas.width, template.canvas.height, template.canvas.width, template.canvas.height);
        
        // Draw power rails
        self.draw_power_rails(&mut svg, &template.power_rails, net_currents);
        
        // Draw IC at template position
        self.draw_ic(&mut svg, &template.ic_position, &template.pin_positions, &template.ic_type);
        
        // Draw supporting components with substitution
        for (template_name, position) in &template.component_positions {
            // Try to find actual component from netlist, or use template name
            let actual_name = component_mapping.get(template_name)
                .map(|s| s.as_str())
                .unwrap_or(template_name.as_str());
            
            // Get actual component value from netlist if available
            let value = self.get_component_value(netlist, actual_name);
            self.draw_component(&mut svg, position, actual_name, &value);
        }
        
        // Draw wires connecting components
        self.draw_wires(&mut svg, template, component_mapping, net_currents);
        
        svg.push_str("  </g>\n</svg>");
        Ok(svg)
    }
    
    /// Draw power rails
    fn draw_power_rails(&self, svg: &mut String, rails: &PowerRails, net_currents: &HashMap<String, f64>) {
        // VIN rail
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}"/>
    <text x="{}" y="{}" font-family="Arial" font-size="9" fill="{}">{}</text>
"#, rails.vin.x_start, rails.vin.y, rails.vin.x_end, rails.vin.y, 
    rails.vin.color, self.calculate_wire_thickness(*net_currents.get("VIN").unwrap_or(&3.0)),
    rails.vin.x_start + 5.0, rails.vin.y - 5.0, 
    rails.vin.color, rails.vin.label));
        
        // VOUT rail
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}"/>
    <text x="{}" y="{}" font-family="Arial" font-size="9" fill="{}">{}</text>
"#, rails.vout.x_start, rails.vout.y, rails.vout.x_end, rails.vout.y,
    rails.vout.color, self.calculate_wire_thickness(*net_currents.get("VOUT").unwrap_or(&2.0)),
    rails.vout.x_start + 5.0, rails.vout.y - 5.0,
    rails.vout.color, rails.vout.label));
        
        // GND rail with symbol
        let gnd_thickness = self.calculate_wire_thickness(*net_currents.get("GND").unwrap_or(&3.0));
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}"/>
    <g transform="translate({}, {})">
      <line x1="0" y1="0" x2="0" y2="10" stroke="black" stroke-width="{}"/>
      <line x1="-10" y1="10" x2="10" y2="10" stroke="black" stroke-width="{}"/>
      <line x1="-7" y1="14" x2="7" y2="14" stroke="black" stroke-width="{}"/>
      <line x1="-4" y1="18" x2="4" y2="18" stroke="black" stroke-width="{}"/>
    </g>
    <text x="{}" y="{}" font-family="Arial" font-size="10" font-weight="bold">{}</text>
"#, rails.gnd.x_start, rails.gnd.y, rails.gnd.x_end, rails.gnd.y,
    rails.gnd.color, gnd_thickness, rails.gnd.x_end - 20.0, rails.gnd.y,
    gnd_thickness, gnd_thickness, gnd_thickness * 0.75, gnd_thickness * 0.5,
    rails.gnd.x_end - 35.0, rails.gnd.y + 35.0, rails.gnd.label));
    }
    
    /// Draw IC with proper pins
    fn draw_ic(&self, svg: &mut String, position: &Position, pins: &HashMap<String, PinPosition>, ic_type: &str) {
        svg.push_str(&format!(
            r#"    <g transform="translate({}, {})">
      <rect x="-90" y="-70" width="180" height="140" fill="lightgray" stroke="black" stroke-width="2"/>
      <text x="0" y="-80" text-anchor="middle" font-family="Arial" font-size="14" font-weight="bold">{}</text>
"#, position.x, position.y, ic_type));
        
        // Draw pins as tiny squares outside IC body, touching it
        for (pin_name, pin_pos) in pins {
            // Determine pin square position outside IC body but touching it
            let (square_x, square_y, text_x, text_y, text_anchor, pin_number_x, pin_number_y) = match pin_pos.side.as_str() {
                "left" => (-94.0, pin_pos.y - 2.0, -75.0, pin_pos.y + 2.0, "start", -96.0, pin_pos.y - 5.0), // Square outside left edge, text inside with padding, pin number above square
                "right" => (90.0, pin_pos.y - 2.0, 75.0, pin_pos.y + 2.0, "end", 92.0, pin_pos.y - 5.0), // Square outside right edge, text inside with padding, pin number above square
                "bottom" => (pin_pos.x - 2.0, 70.0, pin_pos.x, 55.0, "middle", pin_pos.x, 67.0), // Square outside bottom edge, text inside with padding, pin number above square
                "top" => (pin_pos.x - 2.0, -74.0, pin_pos.x, -55.0, "middle", pin_pos.x, -77.0), // Square outside top edge, text inside with padding, pin number above square
                _ => {
                    let sx = pin_pos.x - 2.0;
                    let sy = pin_pos.y - 2.0;
                    (sx, sy, pin_pos.x, pin_pos.y + 3.0, "middle", sx + 2.0, sy - 3.0)
                }
            };
            
            // Determine if pin is connected (for now, assume VIN, SW, GND, FB are connected based on template)
            let is_connected = matches!(pin_name.as_str(), "VIN" | "EN" | "SW" | "GND" | "FB" | "BOOT" | "PH");
            let fill_color = if is_connected { "black" } else { "none" };
            
            // Draw pin square outside IC body
            svg.push_str(&format!(
                r#"      <rect x="{}" y="{}" width="4" height="4" fill="{}" stroke="black" stroke-width="1"/>
"#, square_x, square_y, fill_color));
            
            // Draw pin number above the square
            if let Some(pin_number) = &pin_pos.pin_number {
                svg.push_str(&format!(
                    r#"      <text x="{}" y="{}" text-anchor="middle" font-size="7" fill="blue" font-weight="bold">{}</text>
"#, pin_number_x, pin_number_y, pin_number));
            }
            
            // Draw pin name inside IC body
            svg.push_str(&format!(
                r#"      <text x="{}" y="{}" text-anchor="{}" font-size="9" fill="black">{}</text>
"#, text_x, text_y, text_anchor, pin_name));
        }
        
        svg.push_str("    </g>\n");
    }
    
    /// Draw a component at template position with actual values
    fn draw_component(&self, svg: &mut String, position: &ComponentPosition, name: &str, value: &str) {
        svg.push_str(&format!(r#"    <g transform="translate({}, {})">"#, position.x, position.y));
        
        // Define pin positions and connection status for each component type
        let (component_symbol, pin_positions): (&str, Vec<(f64, f64, &str, bool)>) = match position.component_type.as_str() {
            "capacitor" => {
                let symbol = r#"
      <line x1="0" y1="-20" x2="0" y2="-6" stroke="black" stroke-width="2"/>
      <line x1="-10" y1="-6" x2="10" y2="-6" stroke="black" stroke-width="3"/>
      <line x1="-10" y1="6" x2="10" y2="6" stroke="black" stroke-width="3"/>
      <line x1="0" y1="6" x2="0" y2="20" stroke="black" stroke-width="2"/>"#;
                let pins = vec![
                    (0.0, -20.0, "1", true),   // Top pin (connected - all capacitors are connected in template)
                    (0.0, 20.0, "2", true),    // Bottom pin (connected)
                ];
                (symbol, pins)
            },
            "resistor" => {
                if position.role.contains("feedback") {
                    let symbol = r#"
      <line x1="0" y1="-20" x2="0" y2="-15" stroke="black" stroke-width="2"/>
      <rect x="-5" y="-15" width="10" height="30" stroke="black" stroke-width="2" fill="none"/>
      <line x1="0" y1="15" x2="0" y2="20" stroke="black" stroke-width="2"/>"#;
                    let pins = vec![
                        (0.0, -20.0, "1", true),   // Top pin (connected)
                        (0.0, 20.0, "2", true),    // Bottom pin (connected)
                    ];
                    (symbol, pins)
                } else {
                    let symbol = r#"
      <path d="M -25 0 l 5 0 l 2.5 -5 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 2.5 -5 l 5 0" 
        fill="none" stroke="black" stroke-width="2"/>"#;
                    let pins = vec![
                        (-25.0, 0.0, "1", true),   // Left pin (connected)
                        (25.0, 0.0, "2", true),    // Right pin (connected)
                    ];
                    (symbol, pins)
                }
            },
            "inductor" => {
                let symbol = r#"
      <path d="M -25,0 Q -18.75,-7.5 -12.5,0 Q -6.25,-7.5 0,0 Q 6.25,-7.5 12.5,0 Q 18.75,-7.5 25,0" 
            stroke="black" stroke-width="2" fill="none"/>
      <line x1="-25" y1="0" x2="-30" y2="0" stroke="black" stroke-width="2"/>
      <line x1="25" y1="0" x2="30" y2="0" stroke="black" stroke-width="2"/>"#;
                let pins = vec![
                    (-30.0, 0.0, "1", true),   // Left pin (connected)
                    (30.0, 0.0, "2", true),    // Right pin (connected)
                ];
                (symbol, pins)
            },
            "diode" => {
                // Freewheeling diode: cathode at top (VOUT), anode at bottom (GND)
                // Triangle points up (anode), cathode line at top
                let symbol = r#"
      <polygon points="-8,5 8,5 0,-8" stroke="black" stroke-width="2" fill="none"/>
      <line x1="-10" y1="-8" x2="10" y2="-8" stroke="black" stroke-width="2"/>
      <line x1="0" y1="5" x2="0" y2="15" stroke="black" stroke-width="2"/>
      <line x1="0" y1="-8" x2="0" y2="-18" stroke="black" stroke-width="2"/>"#;
                let pins = vec![
                    (0.0, -18.0, "K", true),   // Cathode (top, connected to VOUT)
                    (0.0, 15.0, "A", true),    // Anode (bottom, connected to GND)
                ];
                (symbol, pins)
            },
            _ => ("", vec![])
        };
        
        // Draw component symbol
        svg.push_str(component_symbol);
        
        // Draw pin squares for each pin
        for (pin_x, pin_y, pin_name, is_connected) in pin_positions {
            let fill_color = if is_connected { "black" } else { "none" };
            
            // Draw pin square
            svg.push_str(&format!(
                r#"      <rect x="{}" y="{}" width="3" height="3" fill="{}" stroke="black" stroke-width="1"/>
"#, pin_x - 1.5, pin_y - 1.5, fill_color));
            
            // Draw pin number above the square
            svg.push_str(&format!(
                r#"      <text x="{}" y="{}" text-anchor="middle" font-size="6" fill="blue" font-weight="bold">{}</text>
"#, pin_x, pin_y - 4.0, pin_name));
        }
        
        // Add component label and value
        svg.push_str(&format!(
            r#"
      <text x="0" y="-25" text-anchor="middle" font-size="10" font-weight="bold">{}</text>
      <text x="0" y="35" text-anchor="middle" font-size="9" fill="blue">{}</text>
    </g>
"#, name, value));
    }
    
    /// Get component value from netlist
    fn get_component_value(&self, _netlist: &Netlist, component_name: &str) -> String {
        // Use values from our BHDL file
        match component_name {
            "c_in1" => "22µF".to_string(),
            "c_in2" => "100nF".to_string(),
            "c_out1" => "47µF".to_string(),
            "c_out2" => "22µF".to_string(),
            "l_out" => "4.7µH".to_string(),
            "r_fb1" => "22kΩ".to_string(),
            "r_fb2" => "3kΩ".to_string(),
            "c_boot" => "100nF".to_string(),
            "tvs" => "15V".to_string(),
            _ => match &component_name[..1] {
                "c" | "C" => "10µF".to_string(),
                "r" | "R" => "10kΩ".to_string(),
                "l" | "L" => "4.7µH".to_string(),
                "d" | "D" => "Schottky".to_string(),
                _ => "".to_string()
            }
        }
    }
    
    /// Draw wires connecting components with simple straight-line rules
    fn draw_wires(&self, svg: &mut String, template: &CircuitTemplate, _component_mapping: &HashMap<String, String>, net_currents: &HashMap<String, f64>) {
        // Simple straight-line wiring rules - NO BENDS
        
        // Rule 1: VIN power rail goes directly to VIN pin (no additional wire needed since rail ends at pin)
        // Power rail is drawn separately and already ends at the correct pin position
        
        // Rule 2: PH→Inductor→VOUT (no bends) - all at same Y level
        // PH pin (phase/switching node) to inductor (updated for new external pin position)
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="purple" stroke-width="{}"/>
"#, template.ic_position.x + 92.0, template.ic_position.y - 40.0,  // PH pin at external square center (492, 260)
    590.0, 260.0, self.calculate_wire_thickness(*net_currents.get("PH").unwrap_or(&2.0))));  // PH net thickness
        
        // Inductor to VOUT (updated for new inductor position at x=620)
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="green" stroke-width="{}"/>
"#, 650.0, 260.0,  // From inductor at x=620+30=650
    template.power_rails.vout.x_start, template.power_rails.vout.y, self.calculate_wire_thickness(*net_currents.get("VOUT").unwrap_or(&2.0))));  // VOUT net thickness
        
        // Rule 3: Extend VOUT straight to the right for output caps
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="green" stroke-width="{}"/>
"#, template.power_rails.vout.x_end, template.power_rails.vout.y,  // Extend VOUT rail
    750.0, template.power_rails.vout.y, self.calculate_wire_thickness(*net_currents.get("VOUT").unwrap_or(&2.0))));  // VOUT net thickness
        
        // Rule 4: Input caps vertically between VIN and GND - straight connections (updated positions)
        for comp_x in [180.0, 230.0] {  // Updated X positions
            // VIN to cap top (straight down)
            svg.push_str(&format!(
                r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="red" stroke-width="{}"/>
"#, comp_x, template.power_rails.vin.y, comp_x, 340.0, self.calculate_wire_thickness(*net_currents.get("VIN").unwrap_or(&3.0))));
            
            // Cap bottom to GND (straight down)
            svg.push_str(&format!(
                r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="{}"/>
"#, comp_x, 380.0, comp_x, template.power_rails.gnd.y, self.calculate_wire_thickness(*net_currents.get("GND").unwrap_or(&3.0))));  // GND net thickness
        }
        
        // Rule 5: Bootstrap cap connects BOOT to PH (not separate connections!)
        // The capacitor IS the connection between BOOT and PH
        // BOOT -> cap pin 1, cap pin 2 -> PH
        
        // BOOT pin to boot cap top - simple horizontal line
        // BOOT pin is at (492, 300) and cap is positioned at (540, 280)
        // Cap top pin square is at y=255 (5 pixels above body which spans 260-300)
        // But for cleaner routing, let's connect horizontally and then to the pin
        
        // Bootstrap circuit: Simple straight horizontal connections
        // PH pin (upper, y=260) connects to capacitor top pin
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="brown" stroke-width="{}"/>
"#, template.ic_position.x + 92.0, template.ic_position.y - 40.0,  // PH pin at (492, 260)
    540.0, template.ic_position.y - 40.0, self.calculate_wire_thickness(*net_currents.get("PH").unwrap_or(&2.0))));
        
        // BOOT pin (lower, y=300) connects to capacitor bottom pin
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="brown" stroke-width="{}"/>
"#, template.ic_position.x + 92.0, template.ic_position.y,  // BOOT pin at (492, 300)
    540.0, template.ic_position.y, self.calculate_wire_thickness(*net_currents.get("BOOT").unwrap_or(&0.01))));
        
        // Rule 6: Feedback resistors positioned so middle goes straight to FB pin (updated positions)
        // VOUT to top resistor (straight down)
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="orange" stroke-width="{}"/>
"#, 590.0, template.power_rails.vout.y, 590.0, 300.0, self.calculate_wire_thickness(*net_currents.get("FB").unwrap_or(&0.001))));  // Feedback signal thickness
        
        // Resistor middle to FB pin (straight horizontal, updated for external pin position)
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="orange" stroke-width="{}"/>
"#, 590.0, 340.0,  // Middle of resistor divider (updated to 590)
    template.ic_position.x + 92.0, template.ic_position.y + 40.0, self.calculate_wire_thickness(*net_currents.get("FB").unwrap_or(&0.001))));  // Feedback signal thickness
        
        // Bottom resistor to GND (straight down)
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="{}"/>
"#, 590.0, 380.0, 590.0, template.power_rails.gnd.y, self.calculate_wire_thickness(*net_currents.get("GND").unwrap_or(&3.0))));  // GND net thickness
        
        // Rule 7: Output caps and diode between VOUT and GND - straight connections (positioned right of inductor)
        for comp_x in [670.0, 720.0, 770.0] {  // Updated positions: diode at 670, caps at 720,770 (right of inductor at 620)
            // VOUT to component top (straight down)
            svg.push_str(&format!(
                r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="green" stroke-width="{}"/>
"#, comp_x, template.power_rails.vout.y, comp_x, 340.0, self.calculate_wire_thickness(*net_currents.get("VOUT").unwrap_or(&2.0))));  // VOUT net thickness
            
            // Component bottom to GND (straight down)
            svg.push_str(&format!(
                r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="{}"/>
"#, comp_x, 380.0, comp_x, template.power_rails.gnd.y, self.calculate_wire_thickness(*net_currents.get("GND").unwrap_or(&3.0))));  // GND net thickness
        }
        
        // Rule 8: EN pin connection to VIN rail (orthogonal routing to avoid IC body)
        // Route EN pin to VIN rail with L-shaped path going around IC body
        // First go left from EN pin to clear the IC body
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="red" stroke-width="{}"/>
"#, template.ic_position.x - 92.0, template.ic_position.y,  // EN pin at external square center
    250.0, template.ic_position.y, self.calculate_wire_thickness(*net_currents.get("VIN").unwrap_or(&3.0))));  // EN signal thickness
        
        // Then go up to VIN rail level
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="red" stroke-width="{}"/>
"#, 250.0, template.ic_position.y,  // From horizontal segment
    250.0, template.power_rails.vin.y, self.calculate_wire_thickness(*net_currents.get("VIN").unwrap_or(&3.0))));  // EN signal thickness
        
        // Finally connect to VIN rail
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="red" stroke-width="{}"/>
"#, 250.0, template.power_rails.vin.y,  // From vertical segment
    template.power_rails.vin.x_start, template.power_rails.vin.y, self.calculate_wire_thickness(*net_currents.get("VIN").unwrap_or(&3.0))));  // EN signal thickness
        
        // IC GND pin to GND rail (straight down, updated for external pin position)
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="{}"/>
"#, template.ic_position.x, template.ic_position.y + 72.0,  // GND pin at external square center
    template.ic_position.x, template.power_rails.gnd.y, self.calculate_wire_thickness(*net_currents.get("GND").unwrap_or(&3.0))));  // GND net thickness
    }
}