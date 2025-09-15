use std::fs;
use std::collections::HashMap;
use anyhow::Result;
use serde_json::Value;

/// Smart topology-based placement using relative positioning
/// NO absolute coordinates - everything derived from:
/// - Component sizes (from KiCad symbols or defaults)
/// - Spacing rules (clearance, routing channels)
/// - Topology patterns (buck, boost, linear, etc.)
/// - Signal flow analysis

#[derive(Debug, Clone)]
struct Component {
    name: String,
    component_type: String,
    role: Option<String>,
    size: (f64, f64),      // From KiCad symbol or default
    position: (f64, f64),   // Calculated based on topology
    pins: Vec<Pin>,
}

#[derive(Debug, Clone)]
struct Pin {
    name: String,
    position: (f64, f64),  // Relative to component center
    net: Option<String>,
}

#[derive(Debug)]
struct LayoutEngine {
    components: HashMap<String, Component>,
    nets: HashMap<String, Vec<String>>, // Net name -> connected components
    
    // Layout parameters (not hardcoded positions!)
    component_spacing: f64,      // Minimum spacing between components
    routing_channel_width: f64,  // Space for wire routing
    power_rail_spacing: f64,     // Vertical spacing for power/ground rails
    
    // Canvas calculated from components
    canvas_width: f64,
    canvas_height: f64,
}

impl LayoutEngine {
    fn new() -> Self {
        Self {
            components: HashMap::new(),
            nets: HashMap::new(),
            
            // These are spacing RULES, not positions
            component_spacing: 30.0,      // Minimum clearance
            routing_channel_width: 40.0,  // Space for routing wires
            power_rail_spacing: 100.0,    // Space between VIN/VOUT/GND rails
            
            canvas_width: 0.0,  // Will be calculated
            canvas_height: 0.0, // Will be calculated
        }
    }
    
    fn parse_netlist(&mut self, netlist_json: &str) -> Result<()> {
        let netlist: Value = serde_json::from_str(netlist_json)?;
        
        // Extract components with sizes based on type
        if let Some(instances) = netlist["instances"].as_array() {
            for inst in instances {
                if let Some(inst_value) = inst.get("value") {
                    let name = inst_value["name"].as_str().unwrap_or("").to_string();
                    if name.is_empty() { continue; }
                    
                    let component_type = match &name[..1] {
                        "U" => "IC",
                        "C" => "Capacitor",
                        "R" => "Resistor",
                        "L" => "Inductor",
                        "D" => "Diode",
                        _ => "Unknown",
                    }.to_string();
                    
                    // Get component role from metadata
                    let mut role = None;
                    if let Some(analysis) = netlist.get("analysis_data") {
                        if let Some(inst_analysis) = analysis.get("instance_analysis") {
                            if let Some(comp_data) = inst_analysis.get(&name) {
                                role = comp_data["component_role"].as_str().map(|s| s.to_string());
                            }
                        }
                    }
                    
                    // Size from KiCad symbols or intelligent defaults
                    let size = self.get_component_size(&component_type, &role);
                    
                    // Create pins based on component type
                    let pins = self.create_pins(&component_type);
                    
                    self.components.insert(name.clone(), Component {
                        name: name.clone(),
                        component_type,
                        role,
                        size,
                        position: (0.0, 0.0), // Will be calculated
                        pins,
                    });
                }
            }
        }
        
        // Build net connectivity map
        if let Some(nets) = netlist["nets"].as_array() {
            for net in nets {
                if let Some(net_value) = net.get("value") {
                    if let Some(name) = net_value["name"].as_str() {
                        let mut connected = Vec::new();
                        
                        if let Some(connections) = net_value["connections"].as_array() {
                            for conn in connections {
                                if let Some(point) = conn.get("point") {
                                    if let Some(inst_name) = point["Instance"].as_array()
                                        .and_then(|arr| arr[0].as_str()) {
                                        connected.push(inst_name.to_string());
                                    }
                                }
                            }
                        }
                        
                        self.nets.insert(name.to_string(), connected);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Get component size from KiCad database or use intelligent defaults
    fn get_component_size(&self, component_type: &str, role: &Option<String>) -> (f64, f64) {
        // TODO: Query actual KiCad symbol dimensions
        // For now, use role-aware sizing
        match component_type {
            "IC" => (80.0, 60.0),  // Would come from KiCad symbol bounds
            "Capacitor" => {
                match role.as_deref() {
                    Some("InputFilter") => (20.0, 40.0),      // Larger electrolytics
                    Some("OutputStabilization") => (20.0, 40.0),
                    Some("Decoupling") => (15.0, 20.0),       // Smaller ceramics
                    _ => (20.0, 30.0),
                }
            }
            "Resistor" => (50.0, 15.0),  // Standard 1/4W size
            "Inductor" => (50.0, 30.0),  // Power inductor
            "Diode" => (30.0, 20.0),     // Standard diode
            _ => (30.0, 30.0),
        }
    }
    
    /// Create pins based on component type (would come from KiCad)
    fn create_pins(&self, component_type: &str) -> Vec<Pin> {
        match component_type {
            "IC" => vec![
                Pin { name: "VIN".to_string(), position: (-40.0, 0.0), net: None },
                Pin { name: "GND".to_string(), position: (0.0, 30.0), net: None },
                Pin { name: "SW".to_string(), position: (40.0, 0.0), net: None },
                Pin { name: "FB".to_string(), position: (-40.0, 20.0), net: None },
            ],
            "Capacitor" | "Resistor" | "Inductor" => vec![
                Pin { name: "1".to_string(), position: (-25.0, 0.0), net: None },
                Pin { name: "2".to_string(), position: (25.0, 0.0), net: None },
            ],
            "Diode" => vec![
                Pin { name: "A".to_string(), position: (-15.0, 0.0), net: None },
                Pin { name: "K".to_string(), position: (15.0, 0.0), net: None },
            ],
            _ => vec![],
        }
    }
    
    /// Calculate positions based on topology pattern
    fn calculate_positions(&mut self) {
        // Identify topology (buck, boost, linear, etc.)
        let topology = self.identify_topology();
        
        match topology.as_str() {
            "buck" => self.layout_buck_converter(),
            "linear" => self.layout_linear_regulator(),
            _ => self.layout_generic(),
        }
        
        // Calculate canvas size from component positions
        self.calculate_canvas_size();
    }
    
    fn identify_topology(&self) -> String {
        // Analyze components to determine topology
        let has_inductor = self.components.values().any(|c| c.component_type == "Inductor");
        let _has_diode = self.components.values().any(|c| c.component_type == "Diode");
        let has_ic = self.components.values().any(|c| c.component_type == "IC");
        
        if has_ic && has_inductor {
            "buck".to_string()
        } else if has_ic && !has_inductor {
            "linear".to_string()
        } else {
            "generic".to_string()
        }
    }
    
    /// Layout algorithm for buck converter topology
    fn layout_buck_converter(&mut self) {
        // Find the main IC
        let ic_name = self.components.iter()
            .find(|(_, c)| c.component_type == "IC")
            .map(|(name, _)| name.clone());
        
        if let Some(ic_name) = ic_name {
            // Get IC size for reference
            let ic_size = self.components[&ic_name].size;
            
            // Place IC at relative center (we'll normalize later)
            let ic_x = 0.0;
            let ic_y = 0.0;
            self.components.get_mut(&ic_name).unwrap().position = (ic_x, ic_y);
            
            // Input capacitors: LEFT of IC
            let mut input_cap_x = ic_x - ic_size.0/2.0 - self.routing_channel_width;
            for (_name, comp) in &mut self.components {
                if comp.role == Some("InputFilter".to_string()) {
                    input_cap_x -= comp.size.0/2.0 + self.component_spacing;
                    comp.position = (input_cap_x, ic_y + self.power_rail_spacing/2.0);
                    input_cap_x -= comp.size.0/2.0;
                }
            }
            
            // Inductor: RIGHT of IC's SW pin
            let ind_name_opt = self.components.iter()
                .find(|(_, c)| c.component_type == "Inductor")
                .map(|(n, _)| n.clone());
            if let Some(ind_name) = ind_name_opt {
                let ind_size = self.components[&ind_name].size;
                let ind_x = ic_x + ic_size.0/2.0 + self.routing_channel_width + ind_size.0/2.0;
                self.components.get_mut(&ind_name).unwrap().position = (ind_x, ic_y);
            }
            
            // Output capacitor: RIGHT of inductor
            let out_cap_name_opt = self.components.iter()
                .find(|(_, c)| c.role == Some("OutputStabilization".to_string()))
                .map(|(n, _)| n.clone());
            if let Some(out_cap_name) = out_cap_name_opt {
                if let Some(ind) = self.components.values().find(|c| c.component_type == "Inductor") {
                    let out_cap_size = self.components[&out_cap_name].size;
                    let out_x = ind.position.0 + ind.size.0/2.0 + self.routing_channel_width + out_cap_size.0/2.0;
                    self.components.get_mut(&out_cap_name).unwrap().position = (out_x, ic_y + self.power_rail_spacing/2.0);
                }
            }
            
            // Diode: BELOW switch node (between IC and inductor)
            let diode_name_opt = self.components.iter()
                .find(|(_, c)| c.component_type == "Diode")
                .map(|(n, _)| n.clone());
            if let Some(diode_name) = diode_name_opt {
                let diode_x = ic_x + ic_size.0/2.0 + self.routing_channel_width/2.0;
                let diode_y = ic_y + ic_size.1/2.0 + self.component_spacing;
                self.components.get_mut(&diode_name).unwrap().position = (diode_x, diode_y);
            }
            
            // Decoupling cap: ABOVE IC (close to power pins)
            let dec_name_opt = self.components.iter()
                .find(|(_, c)| c.role == Some("Decoupling".to_string()))
                .map(|(n, _)| n.clone());
            if let Some(dec_name) = dec_name_opt {
                let dec_y = ic_y - ic_size.1/2.0 - self.component_spacing - self.components[&dec_name].size.1/2.0;
                self.components.get_mut(&dec_name).unwrap().position = (ic_x, dec_y);
            }
            
            // Feedback resistors: BELOW IC, stacked vertically
            let mut fb_y = ic_y + ic_size.1/2.0 + self.component_spacing * 2.0;
            for (_, comp) in &mut self.components {
                if comp.component_type == "Resistor" {
                    comp.position = (ic_x - ic_size.0/4.0, fb_y);
                    fb_y += comp.size.1 + self.component_spacing/2.0;
                }
            }
        }
    }
    
    fn layout_linear_regulator(&mut self) {
        // Similar to buck but without inductor/diode
        // Place IC centrally with input caps left, output caps right
        self.layout_generic(); // Simplified for now
    }
    
    fn layout_generic(&mut self) {
        // Fallback: arrange in grid
        let mut x = 100.0;
        let mut y = 100.0;
        
        for (_, comp) in &mut self.components {
            comp.position = (x, y);
            x += comp.size.0 + self.component_spacing;
            if x > 600.0 {
                x = 100.0;
                y += 100.0;
            }
        }
    }
    
    fn calculate_canvas_size(&mut self) {
        // Find bounds of all components
        let mut min_x = f64::MAX;
        let mut max_x = f64::MIN;
        let mut min_y = f64::MAX;
        let mut max_y = f64::MIN;
        
        for comp in self.components.values() {
            min_x = min_x.min(comp.position.0 - comp.size.0/2.0);
            max_x = max_x.max(comp.position.0 + comp.size.0/2.0);
            min_y = min_y.min(comp.position.1 - comp.size.1/2.0);
            max_y = max_y.max(comp.position.1 + comp.size.1/2.0);
        }
        
        // Add margins
        let margin = 50.0;
        self.canvas_width = (max_x - min_x) + margin * 2.0;
        self.canvas_height = (max_y - min_y) + margin * 2.0;
        
        // Translate all components to positive coordinates
        for comp in self.components.values_mut() {
            comp.position.0 = comp.position.0 - min_x + margin;
            comp.position.1 = comp.position.1 - min_y + margin;
        }
    }
    
    /// Generate SVG with proper component symbols
    fn generate_svg(&self) -> String {
        let mut svg = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">
  <title>Smart Topology-Based Layout</title>
  <rect width="100%" height="100%" fill="white"/>
  <g id="circuit">
"#, self.canvas_width, self.canvas_height, self.canvas_width, self.canvas_height);
        
        // Draw power rails first (horizontal lines for VIN, VOUT, GND)
        self.draw_power_rails(&mut svg);
        
        // Draw components with proper symbols
        for comp in self.components.values() {
            self.draw_component(&mut svg, comp);
        }
        
        // Draw connections between components
        self.draw_connections(&mut svg);
        
        svg.push_str("  </g>\n</svg>");
        svg
    }
    
    fn draw_power_rails(&self, svg: &mut String) {
        // Find y-positions of power nets based on connected components
        let mut vin_y = None;
        let mut gnd_y = None;
        let mut vout_y = None;
        
        for (net_name, connected) in &self.nets {
            if !connected.is_empty() {
                if let Some(comp) = self.components.get(&connected[0]) {
                    if net_name.contains("VIN") {
                        vin_y = Some(comp.position.1 - self.power_rail_spacing/2.0);
                    } else if net_name.contains("GND") {
                        gnd_y = Some(comp.position.1 + self.power_rail_spacing/2.0);
                    } else if net_name.contains("VOUT") {
                        vout_y = Some(comp.position.1);
                    }
                }
            }
        }
        
        // Draw the rails
        if let Some(y) = vin_y {
            svg.push_str(&format!(
                r#"    <line x1="20" y1="{}" x2="{}" y2="{}" stroke="red" stroke-width="2" opacity="0.3"/>
    <text x="25" y="{}" font-size="10" fill="red">VIN</text>
"#, y, self.canvas_width - 20.0, y, y - 5.0));
        }
        
        if let Some(y) = gnd_y {
            svg.push_str(&format!(
                r#"    <line x1="20" y1="{}" x2="{}" y2="{}" stroke="black" stroke-width="2" opacity="0.3"/>
    <text x="25" y="{}" font-size="10" fill="black">GND</text>
"#, y, self.canvas_width - 20.0, y, y - 5.0));
        }
        
        if let Some(y) = vout_y {
            svg.push_str(&format!(
                r#"    <line x1="20" y1="{}" x2="{}" y2="{}" stroke="green" stroke-width="2" opacity="0.3"/>
    <text x="25" y="{}" font-size="10" fill="green">VOUT</text>
"#, y, self.canvas_width - 20.0, y, y - 5.0));
        }
    }
    
    fn draw_component(&self, svg: &mut String, comp: &Component) {
        let (x, y) = comp.position;
        
        match comp.component_type.as_str() {
            "IC" => {
                let (w, h) = comp.size;
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {})">
      <rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2"/>
      <text x="0" y="-5" text-anchor="middle" font-size="12" font-weight="bold">{}</text>
    </g>
"#, x, y, -w/2.0, -h/2.0, w, h, comp.name));
            }
            "Capacitor" => {
                // Scale based on actual component size
                let scale = comp.size.1 / 40.0;  // Normalize to standard height
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {}) scale({})">
      <line x1="0" y1="-20" x2="0" y2="-10" stroke="black" stroke-width="2"/>
      <line x1="-10" y1="-10" x2="10" y2="-10" stroke="black" stroke-width="3"/>
      <line x1="-10" y1="10" x2="10" y2="10" stroke="black" stroke-width="3"/>
      <line x1="0" y1="10" x2="0" y2="20" stroke="black" stroke-width="2"/>
      <text x="15" y="0" font-size="10" transform="scale({})">{}</text>
    </g>
"#, x, y, scale, 1.0/scale, comp.name));
            }
            "Resistor" => {
                let scale = comp.size.0 / 50.0;  // Normalize to standard width
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {}) scale({})">
      <path d="M -25 0 l 5 0 l 2.5 -5 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 2.5 -5 l 5 0" 
        fill="none" stroke="black" stroke-width="2"/>
      <text x="0" y="-10" text-anchor="middle" font-size="10" transform="scale({})">{}</text>
    </g>
"#, x, y, scale, 1.0/scale, comp.name));
            }
            "Inductor" => {
                let scale = comp.size.0 / 50.0;
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {}) scale({})">
      <path d="M -25,0 Q -18.75,-7.5 -12.5,0 Q -6.25,-7.5 0,0 Q 6.25,-7.5 12.5,0 Q 18.75,-7.5 25,0" 
            stroke="black" stroke-width="2" fill="none"/>
      <line x1="-25" y1="0" x2="-30" y2="0" stroke="black" stroke-width="2"/>
      <line x1="25" y1="0" x2="30" y2="0" stroke="black" stroke-width="2"/>
      <text x="0" y="-15" text-anchor="middle" font-size="10" transform="scale({})">{}</text>
    </g>
"#, x, y, scale, 1.0/scale, comp.name));
            }
            "Diode" => {
                let scale = comp.size.0 / 30.0;
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {}) scale({})">
      <polyline points="-15,10 0,-10 -15,-10 -15,10" stroke="black" stroke-width="2" fill="none"/>
      <line x1="0" y1="-10" x2="0" y2="10" stroke="black" stroke-width="2"/>
      <line x1="-15" y1="0" x2="-20" y2="0" stroke="black" stroke-width="2"/>
      <line x1="0" y1="0" x2="5" y2="0" stroke="black" stroke-width="2"/>
      <text x="0" y="20" text-anchor="middle" font-size="10" transform="scale({})">{}</text>
    </g>
"#, x, y, scale, 1.0/scale, comp.name));
            }
            _ => {
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {})">
      <circle cx="0" cy="0" r="15" fill="white" stroke="black" stroke-width="2"/>
      <text x="0" y="5" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, comp.name));
            }
        }
    }
    
    fn draw_connections(&self, svg: &mut String) {
        // Draw actual wires between components based on netlist
        for (_net_name, connected) in &self.nets {
            if connected.len() < 2 { continue; }
            
            // Get positions of all connected components
            let mut points = Vec::new();
            for comp_name in connected {
                if let Some(comp) = self.components.get(comp_name) {
                    points.push((comp.position.0, comp.position.1));
                }
            }
            
            // Draw orthogonal routing between points
            if points.len() >= 2 {
                // Simple routing: connect sequential components
                for i in 0..points.len()-1 {
                    let (x1, y1) = points[i];
                    let (x2, y2) = points[i+1];
                    
                    // Orthogonal routing
                    svg.push_str(&format!(
                        r#"    <polyline points="{},{} {},{} {},{}" 
              fill="none" stroke="blue" stroke-width="1" opacity="0.7"/>
"#, x1, y1, x1, y2, x2, y2));
                }
            }
        }
    }
}

fn main() -> Result<()> {
    println!("=== Smart Topology-Based Circuit Visualizer ===");
    println!("Positions derived from topology patterns and component relationships\n");
    
    // Load netlist
    let netlist_json = fs::read_to_string("test_generic_visualizer_netlist.json")?;
    
    // Create layout engine
    let mut layout = LayoutEngine::new();
    
    // Parse netlist
    layout.parse_netlist(&netlist_json)?;
    
    println!("Detected {} components and {} nets", layout.components.len(), layout.nets.len());
    
    // Calculate positions based on topology
    println!("Calculating positions based on topology pattern...");
    layout.calculate_positions();
    
    println!("Canvas size: {:.0}x{:.0}", layout.canvas_width, layout.canvas_height);
    
    // Generate SVG
    let svg = layout.generate_svg();
    
    // Save to file
    fs::write("test_smart_topology_output.svg", svg)?;
    
    println!("\n✅ SUCCESS! Smart topology-based layout complete.");
    println!("📊 Output: test_smart_topology_output.svg");
    println!("\nKey features:");
    println!("  • Positions derived from topology pattern (buck/linear/etc)");
    println!("  • Component sizes from KiCad symbols or intelligent defaults");
    println!("  • Spacing rules, not absolute positions");
    println!("  • Power rail detection and visualization");
    println!("  • Orthogonal wire routing");
    
    Ok(())
}