use std::fs;
use std::collections::HashMap;
use anyhow::Result;
use serde_json::Value;

/// Working topology visualizer that correctly parses the actual netlist structure
/// This version properly understands the netlist format with separate pin_instances

#[derive(Debug, Clone)]
struct Component {
    name: String,
    component_type: String,
    module_name: String,
    instance_idx: usize,
    position: (f64, f64),
    pins: Vec<PinInfo>,
}

#[derive(Debug, Clone)]
struct PinInfo {
    pin_name: String,
    pin_idx: usize,
    position: (f64, f64),  // Relative to component
    net: Option<String>,
}

#[derive(Debug, Clone)]
struct Net {
    name: String,
    net_idx: usize,
    connections: Vec<usize>, // pin_instance indices
}

struct WorkingVisualizer {
    components: HashMap<String, Component>,
    nets: Vec<Net>,
    pin_instances: HashMap<usize, (usize, usize, usize)>, // pin_inst_idx -> (instance_idx, pin_def_idx, net_idx)
    
    // Layout parameters
    canvas_width: f64,
    canvas_height: f64,
}

impl WorkingVisualizer {
    fn new() -> Self {
        Self {
            components: HashMap::new(),
            nets: Vec::new(),
            pin_instances: HashMap::new(),
            canvas_width: 1000.0,
            canvas_height: 600.0,
        }
    }
    
    fn parse_netlist(&mut self, netlist_json: &str) -> Result<()> {
        let netlist: Value = serde_json::from_str(netlist_json)?;
        
        // First: Parse pin_instances to understand the connections
        println!("Parsing pin instances...");
        if let Some(pin_insts) = netlist["pin_instances"].as_array() {
            for (idx, pin_inst) in pin_insts.iter().enumerate() {
                if let Some(value) = pin_inst.get("value") {
                    if !value.is_null() {
                        let instance_idx = value["instance"]["idx"].as_u64().unwrap_or(0) as usize;
                        let pin_def_idx = value["pin_def"]["idx"].as_u64().unwrap_or(0) as usize;
                        let net_idx = value["net"]["idx"].as_u64().unwrap_or(0) as usize;
                        
                        self.pin_instances.insert(idx, (instance_idx, pin_def_idx, net_idx));
                    }
                }
            }
        }
        println!("  Found {} pin instances", self.pin_instances.len());
        
        // Second: Parse instances (components)
        println!("Parsing instances...");
        if let Some(instances) = netlist["instances"].as_array() {
            for (idx, inst) in instances.iter().enumerate() {
                if let Some(inst_value) = inst.get("value") {
                    if !inst_value.is_null() {
                        let name = inst_value["name"].as_str().unwrap_or("").to_string();
                        if name.is_empty() { continue; }
                        
                        // Get module info
                        let def_idx = inst_value["definition"]["idx"].as_u64().unwrap_or(0) as usize;
                        let module_name = if let Some(modules) = netlist["modules"].as_array() {
                            if let Some(module) = modules.get(def_idx) {
                                module["value"]["name"].as_str().unwrap_or("Unknown").to_string()
                            } else {
                                "Unknown".to_string()
                            }
                        } else {
                            "Unknown".to_string()
                        };
                        
                        // Determine component type
                        let component_type = match &name[..1] {
                            "U" => "IC",
                            "C" => "Capacitor", 
                            "R" => "Resistor",
                            "L" => "Inductor",
                            "D" => "Diode",
                            _ => "Unknown",
                        }.to_string();
                        
                        // Create pins based on module
                        let pins = self.create_pins_for_module(&module_name);
                        
                        self.components.insert(name.clone(), Component {
                            name: name.clone(),
                            component_type,
                            module_name,
                            instance_idx: idx,
                            position: (0.0, 0.0),
                            pins,
                        });
                    }
                }
            }
        }
        println!("  Found {} components", self.components.len());
        
        // Third: Parse nets
        println!("Parsing nets...");
        if let Some(nets) = netlist["nets"].as_array() {
            for (idx, net) in nets.iter().enumerate() {
                if let Some(net_value) = net.get("value") {
                    if !net_value.is_null() {
                        if let Some(name) = net_value["name"].as_str() {
                            let mut connections = Vec::new();
                            
                            // Find all pin instances connected to this net
                            for (pin_inst_idx, (_, _, net_idx)) in &self.pin_instances {
                                if *net_idx == idx {
                                    connections.push(*pin_inst_idx);
                                }
                            }
                            
                            if !connections.is_empty() {
                                self.nets.push(Net {
                                    name: name.to_string(),
                                    net_idx: idx,
                                    connections,
                                });
                            }
                        }
                    }
                }
            }
        }
        println!("  Found {} nets", self.nets.len());
        
        // Update component pins with net information
        for net in &self.nets {
            for pin_inst_idx in &net.connections {
                if let Some((instance_idx, pin_def_idx, _)) = self.pin_instances.get(pin_inst_idx) {
                    // Find the component with this instance_idx
                    for comp in self.components.values_mut() {
                        if comp.instance_idx == *instance_idx {
                            // Assuming pin_def_idx maps to pin array index
                            if let Some(pin) = comp.pins.get_mut(*pin_def_idx - 1) {
                                pin.net = Some(net.name.clone());
                            }
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn create_pins_for_module(&self, module_name: &str) -> Vec<PinInfo> {
        match module_name {
            "TPS54302" => vec![
                PinInfo { pin_name: "VIN".to_string(), pin_idx: 1, position: (-40.0, -20.0), net: None },
                PinInfo { pin_name: "GND".to_string(), pin_idx: 2, position: (0.0, 30.0), net: None },
                PinInfo { pin_name: "SW".to_string(), pin_idx: 3, position: (40.0, 0.0), net: None },
                PinInfo { pin_name: "FB".to_string(), pin_idx: 4, position: (-40.0, 20.0), net: None },
                PinInfo { pin_name: "EN".to_string(), pin_idx: 5, position: (-40.0, 0.0), net: None },
                PinInfo { pin_name: "BOOT".to_string(), pin_idx: 6, position: (40.0, -20.0), net: None },
                PinInfo { pin_name: "PH".to_string(), pin_idx: 7, position: (40.0, 20.0), net: None },
            ],
            "Capacitor" | "Resistor" | "Inductor" => vec![
                PinInfo { pin_name: "1".to_string(), pin_idx: 1, position: (-25.0, 0.0), net: None },
                PinInfo { pin_name: "2".to_string(), pin_idx: 2, position: (25.0, 0.0), net: None },
            ],
            "Diode" => vec![
                PinInfo { pin_name: "A".to_string(), pin_idx: 1, position: (-15.0, 0.0), net: None },
                PinInfo { pin_name: "K".to_string(), pin_idx: 2, position: (15.0, 0.0), net: None },
            ],
            _ => vec![],
        }
    }
    
    fn layout_components(&mut self) {
        println!("\nLayouting components based on topology...");
        
        // Find the main IC
        let ic_name = self.components.iter()
            .find(|(_, c)| c.component_type == "IC")
            .map(|(name, _)| name.clone());
        
        if let Some(ic_name) = ic_name {
            println!("  Found IC: {}", ic_name);
            
            // Place IC at center
            self.components.get_mut(&ic_name).unwrap().position = (500.0, 300.0);
            
            // Layout based on net connections
            // Find components connected to VIN
            let mut vin_components = Vec::new();
            for net in &self.nets {
                if net.name == "VIN" {
                    println!("  Processing VIN net with {} connections", net.connections.len());
                    for pin_inst_idx in &net.connections {
                        if let Some((inst_idx, _, _)) = self.pin_instances.get(pin_inst_idx) {
                            for (name, comp) in &self.components {
                                if comp.instance_idx == *inst_idx && name != &ic_name {
                                    vin_components.push(name.clone());
                                }
                            }
                        }
                    }
                }
            }
            
            // Place VIN components (input caps) to the left
            let mut x = 350.0;
            for comp_name in vin_components {
                if let Some(comp) = self.components.get_mut(&comp_name) {
                    comp.position = (x, 300.0);
                    x -= 80.0;
                }
            }
            
            // Find components connected to SW (should include inductor)
            let mut sw_inductor = None;
            for net in &self.nets {
                if net.name == "SW" {
                    println!("  Processing SW net");
                    for pin_inst_idx in &net.connections {
                        if let Some((inst_idx, _, _)) = self.pin_instances.get(pin_inst_idx) {
                            for (name, comp) in &self.components {
                                if comp.instance_idx == *inst_idx && comp.component_type == "Inductor" {
                                    sw_inductor = Some(name.clone());
                                }
                            }
                        }
                    }
                }
            }
            if let Some(inductor_name) = sw_inductor {
                if let Some(c) = self.components.get_mut(&inductor_name) {
                    c.position = (620.0, 300.0);
                }
            }
            
            // Find components connected to VOUT
            let mut vout_components = Vec::new();
            for net in &self.nets {
                if net.name == "VOUT" {
                    println!("  Processing VOUT net");
                    for pin_inst_idx in &net.connections {
                        if let Some((inst_idx, _, _)) = self.pin_instances.get(pin_inst_idx) {
                            for (name, comp) in &self.components {
                                if comp.instance_idx == *inst_idx && comp.component_type == "Capacitor" {
                                    vout_components.push(name.clone());
                                }
                            }
                        }
                    }
                }
            }
            
            // Place VOUT components to the right
            let mut x = 720.0;
            for comp_name in vout_components {
                if let Some(comp) = self.components.get_mut(&comp_name) {
                    comp.position = (x, 300.0);
                    x += 80.0;
                }
            }
            
            // Place remaining components (feedback resistors, etc.)
            let mut y = 400.0;
            for (_, comp) in &mut self.components {
                if comp.position == (0.0, 0.0) {
                    if comp.component_type == "Resistor" {
                        comp.position = (450.0, y);
                        y += 50.0;
                    } else if comp.component_type == "Diode" {
                        comp.position = (570.0, 350.0);
                    } else {
                        comp.position = (300.0, y);
                        y += 60.0;
                    }
                }
            }
        } else {
            // Fallback layout
            let mut x = 100.0;
            let mut y = 100.0;
            for (_, comp) in &mut self.components {
                comp.position = (x, y);
                x += 120.0;
                if x > 900.0 {
                    x = 100.0;
                    y += 100.0;
                }
            }
        }
        
        println!("Layout complete!");
    }
    
    fn generate_svg(&self) -> String {
        let mut svg = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">
  <title>Working Topology Circuit Visualization</title>
  <rect width="100%" height="100%" fill="white"/>
  <g id="circuit">
"#, self.canvas_width, self.canvas_height, self.canvas_width, self.canvas_height);
        
        // Draw nets/wires first
        for net in &self.nets {
            self.draw_net(&mut svg, net);
        }
        
        // Draw components on top
        for comp in self.components.values() {
            self.draw_component(&mut svg, comp);
        }
        
        // Add legend
        svg.push_str(r#"
    <g transform="translate(20, 20)">
      <text x="0" y="0" font-size="14" font-weight="bold">Net Colors:</text>
      <line x1="0" y1="10" x2="20" y2="10" stroke="red" stroke-width="2"/>
      <text x="25" y="15" font-size="12">VIN</text>
      <line x1="0" y1="25" x2="20" y2="25" stroke="green" stroke-width="2"/>
      <text x="25" y="30" font-size="12">VOUT</text>
      <line x1="0" y1="40" x2="20" y2="40" stroke="black" stroke-width="2"/>
      <text x="25" y="45" font-size="12">GND</text>
      <line x1="0" y1="55" x2="20" y2="55" stroke="blue" stroke-width="2"/>
      <text x="25" y="60" font-size="12">SW</text>
    </g>
"#);
        
        svg.push_str("  </g>\n</svg>");
        svg
    }
    
    fn draw_net(&self, svg: &mut String, net: &Net) {
        // Get all pin positions for this net
        let mut points = Vec::new();
        
        for pin_inst_idx in &net.connections {
            if let Some((inst_idx, pin_def_idx, _)) = self.pin_instances.get(pin_inst_idx) {
                // Find component with this instance index
                for comp in self.components.values() {
                    if comp.instance_idx == *inst_idx {
                        // Get pin position (pin_def_idx is 1-based)
                        if let Some(pin) = comp.pins.get(*pin_def_idx - 1) {
                            let x = comp.position.0 + pin.position.0;
                            let y = comp.position.1 + pin.position.1;
                            points.push((x, y, comp.name.clone(), pin.pin_name.clone()));
                        }
                    }
                }
            }
        }
        
        // Choose wire color based on net type
        let color = match net.name.as_str() {
            "VIN" => "red",
            "VOUT" => "green",
            "GND" => "black",
            "SW" => "blue",
            "FB" => "orange",
            _ => "gray",
        };
        
        // Draw connections with proper routing
        if points.len() >= 2 {
            // For power nets, draw a bus
            if net.name == "VIN" || net.name == "VOUT" || net.name == "GND" {
                // Find average Y position
                let avg_y = points.iter().map(|(_, y, _, _)| *y).sum::<f64>() / points.len() as f64;
                
                // Draw horizontal bus
                let min_x = points.iter().map(|(x, _, _, _)| *x).fold(f64::INFINITY, f64::min) - 20.0;
                let max_x = points.iter().map(|(x, _, _, _)| *x).fold(f64::NEG_INFINITY, f64::max) + 20.0;
                
                svg.push_str(&format!(
                    r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="3" opacity="0.5"/>
"#, min_x, avg_y, max_x, avg_y, color));
                
                // Draw connections from components to bus
                for (x, y, _, _) in &points {
                    svg.push_str(&format!(
                        r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2"/>
"#, x, y, x, avg_y, color));
                }
            } else {
                // For signal nets, draw point-to-point connections
                for i in 0..points.len()-1 {
                    let (x1, y1, _, _) = points[i];
                    let (x2, y2, _, _) = points[i+1];
                    
                    // Orthogonal routing
                    let mid_y = (y1 + y2) / 2.0;
                    svg.push_str(&format!(
                        r#"    <path d="M {},{} L {},{} L {},{} L {},{}" fill="none" stroke="{}" stroke-width="2"/>
"#, x1, y1, x1, mid_y, x2, mid_y, x2, y2, color));
                }
            }
            
            // Draw junction dots at connection points
            for (x, y, comp_name, pin_name) in &points {
                svg.push_str(&format!(
                    r#"    <circle cx="{}" cy="{}" r="3" fill="{}"/>
"#, x, y, color));
                
                // Add hover text
                svg.push_str(&format!(
                    r#"    <title>{}.{} - {}</title>
"#, comp_name, pin_name, net.name));
            }
        }
    }
    
    fn draw_component(&self, svg: &mut String, comp: &Component) {
        let (x, y) = comp.position;
        
        match comp.component_type.as_str() {
            "IC" => {
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {})">
      <rect x="-40" y="-30" width="80" height="60" fill="white" stroke="black" stroke-width="2"/>
      <text x="0" y="-5" text-anchor="middle" font-size="12" font-weight="bold">{}</text>
      <text x="0" y="10" text-anchor="middle" font-size="8">{}</text>
"#, x, y, comp.name, comp.module_name));
                
                // Draw pin labels
                for pin in &comp.pins {
                    let px = pin.position.0;
                    let py = pin.position.1;
                    
                    // Position label appropriately
                    let (tx, ty, anchor) = if px < 0.0 {
                        (px - 5.0, py, "end")
                    } else if px > 0.0 {
                        (px + 5.0, py, "start")
                    } else if py < 0.0 {
                        (px, py - 5.0, "middle")
                    } else {
                        (px, py + 10.0, "middle")
                    };
                    
                    svg.push_str(&format!(
                        r#"      <text x="{}" y="{}" text-anchor="{}" font-size="6">{}</text>
"#, tx, ty, anchor, pin.pin_name));
                }
                
                svg.push_str("    </g>\n");
            }
            "Capacitor" => {
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {})">
      <line x1="0" y1="-20" x2="0" y2="-10" stroke="black" stroke-width="2"/>
      <line x1="-10" y1="-10" x2="10" y2="-10" stroke="black" stroke-width="3"/>
      <line x1="-10" y1="10" x2="10" y2="10" stroke="black" stroke-width="3"/>
      <line x1="0" y1="10" x2="0" y2="20" stroke="black" stroke-width="2"/>
      <text x="15" y="0" font-size="10">{}</text>
    </g>
"#, x, y, comp.name));
            }
            "Resistor" => {
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {})">
      <path d="M -25 0 l 5 0 l 2.5 -5 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 5 -10 l 5 10 l 2.5 -5 l 5 0" 
        fill="none" stroke="black" stroke-width="2"/>
      <text x="0" y="-15" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, comp.name));
            }
            "Inductor" => {
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {})">
      <path d="M -25,0 Q -18.75,-7.5 -12.5,0 Q -6.25,-7.5 0,0 Q 6.25,-7.5 12.5,0 Q 18.75,-7.5 25,0" 
            stroke="black" stroke-width="2" fill="none"/>
      <line x1="-25" y1="0" x2="-30" y2="0" stroke="black" stroke-width="2"/>
      <line x1="25" y1="0" x2="30" y2="0" stroke="black" stroke-width="2"/>
      <text x="0" y="-15" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, comp.name));
            }
            "Diode" => {
                svg.push_str(&format!(
                    r#"    <g transform="translate({}, {})">
      <polyline points="-15,10 0,-10 -15,-10 -15,10" stroke="black" stroke-width="2" fill="none"/>
      <line x1="0" y1="-10" x2="0" y2="10" stroke="black" stroke-width="2"/>
      <line x1="-15" y1="0" x2="-20" y2="0" stroke="black" stroke-width="2"/>
      <line x1="0" y1="0" x2="5" y2="0" stroke="black" stroke-width="2"/>
      <text x="0" y="25" text-anchor="middle" font-size="10">{}</text>
    </g>
"#, x, y, comp.name));
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
}

fn main() -> Result<()> {
    println!("=== Working Topology Circuit Visualizer ===");
    println!("Correctly parsing netlist structure with proper connections\n");
    
    // Load netlist
    let netlist_json = fs::read_to_string("test_generic_visualizer_netlist.json")?;
    
    // Create visualizer
    let mut visualizer = WorkingVisualizer::new();
    
    // Parse netlist
    visualizer.parse_netlist(&netlist_json)?;
    
    // Layout components
    visualizer.layout_components();
    
    // Generate SVG
    let svg = visualizer.generate_svg();
    
    // Save to file
    fs::write("test_working_topology_output.svg", svg)?;
    
    println!("\n✅ SUCCESS! Working topology visualization complete.");
    println!("📊 Output: test_working_topology_output.svg");
    println!("\nFeatures:");
    println!("  • Correctly parses netlist structure");
    println!("  • Places components based on topology");
    println!("  • Draws colored wires for different net types");
    println!("  • Power buses for VIN/VOUT/GND");
    println!("  • Orthogonal routing for signal nets");
    println!("  • Junction dots at connection points");
    println!("  • Pin labels on IC");
    
    Ok(())
}