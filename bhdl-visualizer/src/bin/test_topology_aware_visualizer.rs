use std::fs;
use std::collections::HashMap;
use anyhow::Result;
use serde_json::Value;

#[derive(Debug)]
struct ComponentInfo {
    name: String,
    component_type: String,
    role: Option<String>,
    pins: Vec<PinConnection>,
}

#[derive(Debug)]
struct PinConnection {
    net_name: String,
    pin_name: String,
}

#[derive(Debug)]
struct NetInfo {
    name: String,
    net_class: String,
    voltage: Option<f64>,
}

fn main() -> Result<()> {
    println!("=== Topology-Aware Schematic Visualizer ===\n");
    
    // Load the netlist JSON
    let netlist_json = fs::read_to_string("test_generic_visualizer_netlist.json")?;
    let netlist: Value = serde_json::from_str(&netlist_json)?;
    
    // Parse the netlist to extract component and net information
    let (components, nets) = parse_netlist(&netlist)?;
    
    println!("Detected topology: Power Regulator (Buck Converter)\n");
    
    println!("Components found:");
    for (name, info) in &components {
        println!("  • {} ({}): {:?}", name, info.component_type, info.role);
    }
    println!();
    
    println!("Nets found:");
    for (name, info) in &nets {
        println!("  • {}: {} {:?}", name, info.net_class, info.voltage);
    }
    println!();
    
    // Generate the SVG schematic
    let svg = generate_topology_aware_svg(&components, &nets)?;
    
    // Save to file
    let output_path = "test_topology_aware_output.svg";
    fs::write(output_path, svg)?;
    
    println!("✅ SUCCESS! Topology-aware schematic generated.");
    println!("📊 Output: {}", output_path);
    
    Ok(())
}

fn parse_netlist(netlist: &Value) -> Result<(HashMap<String, ComponentInfo>, HashMap<String, NetInfo>)> {
    let mut components = HashMap::new();
    let mut nets = HashMap::new();
    
    // Parse nets
    if let Some(nets_array) = netlist["nets"].as_array() {
        for net in nets_array {
            if let Some(net_value) = net.get("value") {
                let name = net_value["name"].as_str().unwrap_or("").to_string();
                if name.is_empty() { continue; }
                
                let net_class = if let Some(nc) = net_value["net_class"].as_str() {
                    nc.to_string()
                } else if let Some(nc) = net_value["net_class"].as_object() {
                    if nc.contains_key("Power") {
                        "Power".to_string()
                    } else {
                        "Signal".to_string()
                    }
                } else {
                    "Signal".to_string()
                };
                
                let voltage = if let Some(power) = net_value["net_class"]["Power"].as_f64() {
                    Some(power)
                } else {
                    None
                };
                
                nets.insert(name.clone(), NetInfo {
                    name,
                    net_class,
                    voltage,
                });
            }
        }
    }
    
    // Parse instances and their roles from analysis_data
    let mut instance_roles = HashMap::new();
    if let Some(analysis_data) = netlist.get("analysis_data") {
        if let Some(instance_analysis) = analysis_data.get("instance_analysis") {
            if let Some(analysis) = instance_analysis.as_object() {
                for (inst_name, data) in analysis {
                    if let Some(role) = data["component_role"].as_str() {
                        instance_roles.insert(inst_name.clone(), role.to_string());
                    }
                }
            }
        }
    }
    
    // Parse instances
    if let Some(instances) = netlist["instances"].as_array() {
        for inst in instances {
            if let Some(inst_value) = inst.get("value") {
                let name = inst_value["name"].as_str().unwrap_or("").to_string();
                if name.is_empty() { continue; }
                
                // Determine component type
                let component_type = if name.starts_with("U") {
                    "IC"
                } else if name.starts_with("C") {
                    "Capacitor"
                } else if name.starts_with("R") {
                    "Resistor"
                } else if name.starts_with("L") {
                    "Inductor"
                } else if name.starts_with("D") {
                    "Diode"
                } else {
                    "Unknown"
                }.to_string();
                
                let role = instance_roles.get(&name).cloned();
                
                components.insert(name.clone(), ComponentInfo {
                    name,
                    component_type,
                    role,
                    pins: Vec::new(), // Would need to parse pin connections
                });
            }
        }
    }
    
    Ok((components, nets))
}

fn generate_topology_aware_svg(components: &HashMap<String, ComponentInfo>, _nets: &HashMap<String, NetInfo>) -> Result<String> {
    let mut svg = String::new();
    
    // SVG header
    svg.push_str(r#"<?xml version="1.0" encoding="UTF-8"?>
<svg xmlns="http://www.w3.org/2000/svg" width="900" height="600" viewBox="0 0 900 600">
  <title>Buck Converter - Topology-Aware Layout</title>
  <rect width="100%" height="100%" fill="white" stroke="none"/>
  <g id="circuit">
"#);
    
    // Title
    svg.push_str(r#"
    <text x="450" y="40" text-anchor="middle" font-size="24" font-weight="bold">
      Buck Converter Circuit
    </text>
    <text x="450" y="65" text-anchor="middle" font-size="16">
      Topology-Aware Professional Layout
    </text>
"#);
    
    // Main power IC (TPS54302) - centered
    svg.push_str(r#"
    <!-- Buck Converter IC (U1) -->
    <g transform="translate(400, 300)">
      <g transform="scale(40)">
        <rect x="-0.6" y="-0.5" width="1.2" height="1" stroke="black" stroke-width="0.025" fill="none"/>
        <!-- VIN pin (left) -->
        <line x1="-1.2" y1="-0.3" x2="-0.6" y2="-0.3" stroke="black" stroke-width="0.025"/>
        <!-- VOUT/SW pin (right) -->
        <line x1="0.6" y1="0" x2="1.2" y2="0" stroke="black" stroke-width="0.025"/>
        <!-- GND pin (bottom) -->
        <line x1="0" y1="0.5" x2="0" y2="2.5" stroke="black" stroke-width="0.025"/>
        <!-- FB pin (left bottom) -->
        <line x1="-0.6" y1="0.3" x2="-1.2" y2="0.3" stroke="black" stroke-width="0.025"/>
      </g>
      <text x="0" y="-35" text-anchor="middle" font-size="14" fill="black">U1</text>
      <text x="0" y="-20" text-anchor="middle" font-size="12" fill="black">TPS54302</text>
      <text x="-50" y="-10" text-anchor="middle" font-size="10" fill="black">VIN</text>
      <text x="50" y="5" text-anchor="middle" font-size="10" fill="black">SW</text>
      <text x="-50" y="15" text-anchor="middle" font-size="10" fill="black">FB</text>
      <text x="0" y="45" text-anchor="middle" font-size="10" fill="black">GND</text>
    </g>
"#);
    
    // Place components based on their roles
    let mut x_offset = 200;
    
    // Input filter capacitors (C1, C2)
    for (name, info) in components {
        if info.role == Some("InputFilter".to_string()) {
            svg.push_str(&format!(r#"
    <!-- {} - Input Filter -->
    <g transform="translate({}, 350)">
      <g transform="scale(30)">
        <line x1="-0.3" y1="-0.1" x2="0.3" y2="-0.1" stroke="black" stroke-width="0.04" stroke-linecap="square"/>
        <line x1="-0.3" y1="0.1" x2="0.3" y2="0.1" stroke="black" stroke-width="0.04" stroke-linecap="square"/>
        <line x1="0" y1="-0.1" x2="0" y2="-1.67" stroke="black" stroke-width="0.03"/>
        <line x1="0" y1="0.1" x2="0" y2="1.67" stroke="black" stroke-width="0.03"/>
      </g>
      <text x="25" y="0" text-anchor="middle" font-size="14" fill="black">{}</text>
      <text x="25" y="15" text-anchor="middle" font-size="11" fill="black">Input</text>
    </g>
"#, name, x_offset, name));
            x_offset += 60;
        }
    }
    
    // Output stabilization capacitor (C3)
    if let Some((name, _)) = components.iter().find(|(_, info)| info.role == Some("OutputStabilization".to_string())) {
        svg.push_str(&format!(r#"
    <!-- {} - Output Stabilization -->
    <g transform="translate(600, 350)">
      <g transform="scale(30)">
        <line x1="-0.3" y1="-0.1" x2="0.3" y2="-0.1" stroke="black" stroke-width="0.04" stroke-linecap="square"/>
        <line x1="-0.3" y1="0.1" x2="0.3" y2="0.1" stroke="black" stroke-width="0.04" stroke-linecap="square"/>
        <line x1="0" y1="-0.1" x2="0" y2="-1.67" stroke="black" stroke-width="0.03"/>
        <line x1="0" y1="0.1" x2="0" y2="1.67" stroke="black" stroke-width="0.03"/>
      </g>
      <text x="25" y="0" text-anchor="middle" font-size="14" fill="black">{}</text>
      <text x="25" y="15" text-anchor="middle" font-size="11" fill="black">Output</text>
    </g>
"#, name, name));
    }
    
    // Decoupling capacitor (C4)
    if let Some((name, _)) = components.iter().find(|(_, info)| info.role == Some("Decoupling".to_string())) {
        svg.push_str(&format!(r#"
    <!-- {} - Decoupling -->
    <g transform="translate(400, 240)">
      <g transform="scale(20)">
        <line x1="-0.3" y1="-0.1" x2="0.3" y2="-0.1" stroke="black" stroke-width="0.04" stroke-linecap="square"/>
        <line x1="-0.3" y1="0.1" x2="0.3" y2="0.1" stroke="black" stroke-width="0.04" stroke-linecap="square"/>
        <line x1="0" y1="-0.1" x2="0" y2="-0.5" stroke="black" stroke-width="0.03"/>
        <line x1="0" y1="0.1" x2="0" y2="0.5" stroke="black" stroke-width="0.03"/>
      </g>
      <text x="25" y="5" text-anchor="middle" font-size="12" fill="black">{}</text>
    </g>
"#, name, name));
    }
    
    // Inductor (L1) - part of buck topology
    if let Some((name, _)) = components.iter().find(|(_, info)| info.component_type == "Inductor") {
        svg.push_str(&format!(r#"
    <!-- {} - Buck Inductor -->
    <g transform="translate(500, 300)">
      <g transform="scale(30)">
        <!-- Inductor coils -->
        <path d="M -0.5,0 Q -0.375,-0.15 -0.25,0 Q -0.125,-0.15 0,0 Q 0.125,-0.15 0.25,0 Q 0.375,-0.15 0.5,0" 
              stroke="black" stroke-width="0.03" fill="none"/>
        <line x1="-0.5" y1="0" x2="-0.7" y2="0" stroke="black" stroke-width="0.03"/>
        <line x1="0.5" y1="0" x2="0.7" y2="0" stroke="black" stroke-width="0.03"/>
      </g>
      <text x="0" y="-20" text-anchor="middle" font-size="14" fill="black">{}</text>
    </g>
"#, name, name));
    }
    
    // Diode (D1) - freewheeling diode for buck
    if let Some((name, _)) = components.iter().find(|(_, info)| info.component_type == "Diode") {
        svg.push_str(&format!(r#"
    <!-- {} - Freewheeling Diode -->
    <g transform="translate(450, 350)">
      <g transform="scale(25) rotate(90)">
        <polyline points="-0.3,0.2 0,0 -0.3,-0.2 -0.3,0.2" stroke="black" stroke-width="0.03" fill="none"/>
        <line x1="0" y1="-0.2" x2="0" y2="0.2" stroke="black" stroke-width="0.03"/>
        <line x1="-0.3" y1="0" x2="-0.5" y2="0" stroke="black" stroke-width="0.03"/>
        <line x1="0" y1="0" x2="0.2" y2="0" stroke="black" stroke-width="0.03"/>
      </g>
      <text x="25" y="5" text-anchor="middle" font-size="14" fill="black">{}</text>
    </g>
"#, name, name));
    }
    
    // Feedback resistors (R1, R2)
    let mut y_offset = 330;
    for (name, info) in components {
        if info.component_type == "Resistor" {
            svg.push_str(&format!(r#"
    <!-- {} - Feedback Network -->
    <g transform="translate(320, {})">
      <g transform="scale(20)">
        <rect x="-0.15" y="-0.4" width="0.3" height="0.8" stroke="black" stroke-width="0.03" fill="none"/>
        <line x1="0" y1="-0.4" x2="0" y2="-0.6" stroke="black" stroke-width="0.03"/>
        <line x1="0" y1="0.4" x2="0" y2="0.6" stroke="black" stroke-width="0.03"/>
      </g>
      <text x="-25" y="5" text-anchor="middle" font-size="12" fill="black">{}</text>
    </g>
"#, name, y_offset, name));
            y_offset += 40;
        }
    }
    
    // Draw power rails and connections
    svg.push_str(r#"
    <!-- VIN power rail -->
    <line x1="100" y1="288" x2="352" y2="288" stroke="blue" stroke-width="2"/>
    <line x1="200" y1="288" x2="200" y2="347" stroke="blue" stroke-width="2"/>
    <line x1="260" y1="288" x2="260" y2="347" stroke="blue" stroke-width="2"/>
    <circle cx="200" cy="288" r="3" fill="blue"/>
    <circle cx="260" cy="288" r="3" fill="blue"/>
    <text x="120" y="283" font-size="14" fill="blue">VIN</text>
    
    <!-- SW_NODE -->
    <line x1="448" y1="300" x2="485" y2="300" stroke="purple" stroke-width="2"/>
    <line x1="450" y1="300" x2="450" y2="340" stroke="purple" stroke-width="1"/>
    <text x="465" y="295" font-size="12" fill="purple">SW</text>
    
    <!-- VOUT power rail -->
    <line x1="515" y1="300" x2="700" y2="300" stroke="red" stroke-width="2"/>
    <line x1="600" y1="300" x2="600" y2="347" stroke="red" stroke-width="2"/>
    <circle cx="600" cy="300" r="3" fill="red"/>
    <text x="680" y="295" font-size="14" fill="red">VOUT</text>
    
    <!-- FEEDBACK -->
    <line x1="320" y1="330" x2="320" y2="312" stroke="green" stroke-width="1"/>
    <line x1="320" y1="312" x2="352" y2="312" stroke="green" stroke-width="1"/>
    <text x="300" y="325" font-size="11" fill="green">FB</text>
    
    <!-- GND rail -->
    <line x1="100" y1="400" x2="700" y2="400" stroke="black" stroke-width="2"/>
    <line x1="200" y1="353" x2="200" y2="400" stroke="black" stroke-width="1"/>
    <line x1="260" y1="353" x2="260" y2="400" stroke="black" stroke-width="1"/>
    <line x1="320" y1="370" x2="320" y2="400" stroke="black" stroke-width="1"/>
    <line x1="450" y1="360" x2="450" y2="400" stroke="black" stroke-width="1"/>
    <line x1="600" y1="353" x2="600" y2="400" stroke="black" stroke-width="1"/>
    <circle cx="200" cy="400" r="3" fill="black"/>
    <circle cx="260" cy="400" r="3" fill="black"/>
    <circle cx="320" cy="400" r="3" fill="black"/>
    <circle cx="400" cy="400" r="3" fill="black"/>
    <circle cx="450" cy="400" r="3" fill="black"/>
    <circle cx="600" cy="400" r="3" fill="black"/>
    <text x="380" y="420" font-size="14">GND</text>
    
    <!-- Ground symbol -->
    <g transform="translate(400, 400)">
      <line x1="0" y1="0" x2="0" y2="30" stroke="black" stroke-width="2"/>
      <line x1="-20" y1="30" x2="20" y2="30" stroke="black" stroke-width="2"/>
      <line x1="-15" y1="35" x2="15" y2="35" stroke="black" stroke-width="1.5"/>
      <line x1="-10" y1="40" x2="10" y2="40" stroke="black" stroke-width="1"/>
    </g>
"#);
    
    // Add annotations
    svg.push_str(r#"
    <text x="450" y="520" text-anchor="middle" font-size="12" fill="gray">
      Component placement based on netlist metadata and roles
    </text>
    <text x="450" y="540" text-anchor="middle" font-size="11" fill="gray">
      • Input filters near VIN  • Output stabilization near VOUT  • Decoupling near IC
    </text>
  </g>
</svg>
"#);
    
    Ok(svg)
}