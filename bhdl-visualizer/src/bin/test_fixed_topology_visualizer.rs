use std::fs;
use std::collections::HashMap;
use anyhow::Result;
use serde_json::Value;

/// Fixed topology visualizer that properly reads netlist and draws connections
/// This version:
/// 1. Correctly parses pin instances to component mappings
/// 2. Places components based on actual connectivity
/// 3. Draws wires between connected components

#[derive(Debug, Clone)]
struct Component {
    name: String,
    component_type: String,
    module_name: String,
    position: (f64, f64),
    pins: HashMap<String, PinInfo>,
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
    connections: Vec<(String, String)>, // (component_name, pin_name)
}

struct TopologyVisualizer {
    components: HashMap<String, Component>,
    nets: Vec<Net>,
    pin_to_component: HashMap<usize, (String, String)>, // pin_idx -> (component, pin_name)
    
    // Layout parameters
    canvas_width: f64,
    canvas_height: f64,
}

impl TopologyVisualizer {
    fn new() -> Self {
        Self {
            components: HashMap::new(),
            nets: Vec::new(),
            pin_to_component: HashMap::new(),
            canvas_width: 800.0,
            canvas_height: 600.0,
        }
    }
    
    fn parse_netlist(&mut self, netlist_json: &str) -> Result<()> {
        let netlist: Value = serde_json::from_str(netlist_json)?;
        
        // First pass: Extract instances and build pin mappings
        if let Some(instances) = netlist["instances"].as_array() {
            for inst in instances {
                if let Some(inst_value) = inst.get("value") {
                    let name = inst_value["name"].as_str().unwrap_or("").to_string();
                    if name.is_empty() { continue; }
                    
                    let module_idx = inst_value["module"]["idx"].as_u64().unwrap_or(0);
                    
                    // Get module info
                    let module_name = if let Some(modules) = netlist["modules"].as_array() {
                        if let Some(module) = modules.get(module_idx as usize) {
                            module["value"]["name"].as_str().unwrap_or("Unknown").to_string()
                        } else {
                            "Unknown".to_string()
                        }
                    } else {
                        "Unknown".to_string()
                    };
                    
                    // Determine component type from name prefix
                    let component_type = match &name[..1] {
                        "U" => "IC",
                        "C" => "Capacitor", 
                        "R" => "Resistor",
                        "L" => "Inductor",
                        "D" => "Diode",
                        _ => "Unknown",
                    }.to_string();
                    
                    // Create component with pins
                    let mut pins = HashMap::new();
                    
                    // Map pin instances to this component
                    if let Some(pin_instances) = inst_value["pin_instances"].as_array() {
                        for (i, pin_inst) in pin_instances.iter().enumerate() {
                            let pin_idx = pin_inst["idx"].as_u64().unwrap_or(0) as usize;
                            let pin_name = self.get_pin_name(&module_name, i);
                            
                            pins.insert(pin_name.clone(), PinInfo {
                                pin_name: pin_name.clone(),
                                pin_idx,
                                position: self.get_pin_position(&component_type, &pin_name),
                                net: None,
                            });
                            
                            self.pin_to_component.insert(pin_idx, (name.clone(), pin_name));
                        }
                    }
                    
                    self.components.insert(name.clone(), Component {
                        name: name.clone(),
                        component_type,
                        module_name,
                        position: (0.0, 0.0),
                        pins,
                    });
                }
            }
        }
        
        // Second pass: Extract nets and map connections
        if let Some(nets) = netlist["nets"].as_array() {
            for net in nets {
                if let Some(net_value) = net.get("value") {
                    if let Some(name) = net_value["name"].as_str() {
                        let mut connections = Vec::new();
                        
                        if let Some(conns) = net_value["connections"].as_array() {
                            for conn in conns {
                                if let Some(pin_inst) = conn.get("PinInstance") {
                                    let pin_idx = pin_inst["idx"].as_u64().unwrap_or(0) as usize;
                                    
                                    if let Some((comp_name, pin_name)) = self.pin_to_component.get(&pin_idx) {
                                        connections.push((comp_name.clone(), pin_name.clone()));
                                        
                                        // Update component's pin with net info
                                        if let Some(comp) = self.components.get_mut(comp_name) {
                                            if let Some(pin) = comp.pins.get_mut(pin_name) {
                                                pin.net = Some(name.to_string());
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        
                        if !connections.is_empty() {
                            self.nets.push(Net {
                                name: name.to_string(),
                                connections,
                            });
                        }
                    }
                }
            }
        }
        
        Ok(())
    }
    
    fn get_pin_name(&self, module_name: &str, pin_index: usize) -> String {
        match module_name {
            "TPS54302" => match pin_index {
                0 => "VIN".to_string(),
                1 => "GND".to_string(), 
                2 => "SW".to_string(),
                3 => "FB".to_string(),
                4 => "EN".to_string(),
                5 => "BOOT".to_string(),
                6 => "PH".to_string(),
                _ => format!("P{}", pin_index + 1),
            },
            "Capacitor" | "Resistor" | "Inductor" => match pin_index {
                0 => "1".to_string(),
                1 => "2".to_string(),
                _ => format!("{}", pin_index + 1),
            },
            "Diode" => match pin_index {
                0 => "A".to_string(),
                1 => "K".to_string(),
                _ => format!("{}", pin_index + 1),
            },
            _ => format!("{}", pin_index + 1),
        }
    }
    
    fn get_pin_position(&self, component_type: &str, pin_name: &str) -> (f64, f64) {
        match component_type {
            "IC" => match pin_name {
                "VIN" => (-40.0, -20.0),
                "GND" => (0.0, 30.0),
                "SW" => (40.0, 0.0),
                "FB" => (-40.0, 20.0),
                "EN" => (-40.0, 0.0),
                "BOOT" => (40.0, -20.0),
                "PH" => (40.0, 20.0),
                _ => (0.0, 0.0),
            },
            "Capacitor" | "Resistor" | "Inductor" => match pin_name {
                "1" => (-25.0, 0.0),
                "2" => (25.0, 0.0),
                _ => (0.0, 0.0),
            },
            "Diode" => match pin_name {
                "A" => (-15.0, 0.0),
                "K" => (15.0, 0.0),
                _ => (0.0, 0.0),
            },
            _ => (0.0, 0.0),
        }
    }
    
    fn layout_components(&mut self) {
        // Find the main IC
        let ic_name = self.components.iter()
            .find(|(_, c)| c.component_type == "IC")
            .map(|(name, _)| name.clone());
        
        if let Some(ic_name) = ic_name {
            // Place IC at center
            self.components.get_mut(&ic_name).unwrap().position = (400.0, 300.0);
            
            // Find components connected to VIN
            let mut vin_components = Vec::new();
            for net in &self.nets {
                if net.name == "VIN" {
                    for (comp, _pin) in &net.connections {
                        if comp != &ic_name {
                            vin_components.push(comp.clone());
                        }
                    }
                }
            }
            
            // Place VIN components (input caps) to the left
            let mut x = 250.0;
            for comp_name in vin_components {
                if let Some(comp) = self.components.get_mut(&comp_name) {
                    comp.position = (x, 300.0);
                    x -= 80.0;
                }
            }
            
            // Find components connected to SW (inductor)
            for net in &self.nets {
                if net.name == "SW" {
                    for (comp, _pin) in &net.connections {
                        if comp != &ic_name {
                            if let Some(c) = self.components.get_mut(comp) {
                                if c.component_type == "Inductor" {
                                    c.position = (500.0, 300.0);
                                }
                            }
                        }
                    }
                }
            }
            
            // Find components connected to VOUT
            let mut vout_components = Vec::new();
            for net in &self.nets {
                if net.name == "VOUT" {
                    for (comp, _pin) in &net.connections {
                        if comp != &ic_name && !comp.starts_with("L") {
                            vout_components.push(comp.clone());
                        }
                    }
                }
            }
            
            // Place VOUT components (output caps) to the right
            let mut x = 600.0;
            for comp_name in vout_components {
                if let Some(comp) = self.components.get_mut(&comp_name) {
                    comp.position = (x, 300.0);
                    x += 80.0;
                }
            }
            
            // Place GND-connected components below
            let mut y = 400.0;
            for net in &self.nets {
                if net.name == "GND" {
                    for (comp, _pin) in &net.connections {
                        if let Some(c) = self.components.get_mut(comp) {
                            if c.position == (0.0, 0.0) { // Not yet placed
                                c.position = (400.0, y);
                                y += 50.0;
                            }
                        }
                    }
                }
            }
            
            // Place any remaining components
            let mut x = 100.0;
            let mut y = 100.0;
            for (_, comp) in &mut self.components {
                if comp.position == (0.0, 0.0) {
                    comp.position = (x, y);
                    x += 100.0;
                    if x > 700.0 {
                        x = 100.0;
                        y += 100.0;
                    }
                }
            }
        }
    }
    
    fn generate_svg(&self) -> String {
        let mut svg = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}">
  <title>Fixed Topology Circuit Visualization</title>
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
        
        svg.push_str("  </g>\n</svg>");
        svg
    }
    
    fn draw_net(&self, svg: &mut String, net: &Net) {
        // Get all pin positions for this net
        let mut points = Vec::new();
        for (comp_name, pin_name) in &net.connections {
            if let Some(comp) = self.components.get(comp_name) {
                if let Some(pin) = comp.pins.get(pin_name) {
                    let x = comp.position.0 + pin.position.0;
                    let y = comp.position.1 + pin.position.1;
                    points.push((x, y));
                }
            }
        }
        
        // Draw connections between points
        if points.len() >= 2 {
            // Choose wire color based on net type
            let color = match net.name.as_str() {
                "VIN" => "red",
                "VOUT" => "green",
                "GND" => "black",
                "SW" => "blue",
                _ => "gray",
            };
            
            // Connect all points with orthogonal routing
            for i in 0..points.len()-1 {
                let (x1, y1) = points[i];
                let (x2, y2) = points[i+1];
                
                // Simple orthogonal routing
                let mid_x = (x1 + x2) / 2.0;
                svg.push_str(&format!(
                    r#"    <path d="M {},{} L {},{} L {},{} L {},{}" fill="none" stroke="{}" stroke-width="2" opacity="0.7"/>
"#, x1, y1, mid_x, y1, mid_x, y2, x2, y2, color));
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
      <text x="0" y="0" text-anchor="middle" font-size="12" font-weight="bold">{}</text>
      <text x="0" y="15" text-anchor="middle" font-size="8">{}</text>
"#, x, y, comp.name, comp.module_name));
                
                // Draw pin labels
                for pin in comp.pins.values() {
                    svg.push_str(&format!(
                        r#"      <text x="{}" y="{}" text-anchor="middle" font-size="6">{}</text>
"#, pin.position.0, pin.position.1 - 5.0, pin.pin_name));
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
      <text x="20" y="0" font-size="10">{}</text>
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
    println!("=== Fixed Topology Circuit Visualizer ===");
    println!("Properly parsing netlist and drawing connections\n");
    
    // Load netlist
    let netlist_json = fs::read_to_string("test_generic_visualizer_netlist.json")?;
    
    // Create visualizer
    let mut visualizer = TopologyVisualizer::new();
    
    // Parse netlist
    visualizer.parse_netlist(&netlist_json)?;
    
    println!("Found {} components and {} nets", visualizer.components.len(), visualizer.nets.len());
    
    // Layout components
    println!("Laying out components based on connectivity...");
    visualizer.layout_components();
    
    // Generate SVG
    let svg = visualizer.generate_svg();
    
    // Save to file
    fs::write("test_fixed_topology_output.svg", svg)?;
    
    println!("\n✅ SUCCESS! Fixed topology visualization complete.");
    println!("📊 Output: test_fixed_topology_output.svg");
    println!("\nKey improvements:");
    println!("  • Correctly parses pin instances to component mappings");
    println!("  • Places components based on actual net connectivity");
    println!("  • Draws colored wires between connected pins");
    println!("  • Uses orthogonal routing for professional appearance");
    
    Ok(())
}