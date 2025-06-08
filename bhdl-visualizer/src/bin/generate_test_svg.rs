use bhdl_visualizer::{
    LayoutEngine, Point,
    create_smart_routing_for_connections_with_obstacles, 
    generate_component_symbol_with_rotation, 
    generate_grid_background
};
use bhdl_netlist::{Netlist, ModuleKind, ConnectionPoint};
use std::collections::HashMap;

fn main() {
    // Create test netlist
    let netlist = create_test_netlist();
    
    // Use semantic analysis engine
    let mut layout_engine = LayoutEngine::new(&netlist);
    let layout_result = layout_engine.run_with_semantic_analysis();
    
    // Get the component layouts which include rotation information
    let (component_layouts, _, _, _, _) = layout_engine.get_layouts_and_debug();
    
    println!("Layout completed with {} component positions", layout_result.component_positions.len());
    
    // Generate SVG
    let mut svg_content = String::new();
    svg_content.push_str(r#"<svg width="800" height="600" xmlns="http://www.w3.org/2000/svg">"#);
    svg_content.push('\n');
    
    // Add background
    svg_content.push_str("  <rect width=\"800\" height=\"600\" fill=\"#f8f8f8\" stroke=\"#dddddd\" stroke-width=\"1\"/>");
    svg_content.push('\n');
    
    // Add grid using crate function
    svg_content.push_str(&generate_grid_background());
    svg_content.push('\n');
    
    // Calculate center offset to place components in the middle of the SVG
    let svg_center_x = 400.0;
    let svg_center_y = 300.0;
    
    // Draw components using crate function and collect pin locations
    let mut all_pin_locations = HashMap::new();
    
    for (instance_id, position) in &layout_result.component_positions {
        if let Some(instance) = netlist.get_instance(*instance_id) {
            let module_name = netlist.get_module(instance.definition)
                .map(|m| m.name.clone())
                .unwrap_or_else(|| "Unknown".to_string());
            
            // Transform layout coordinates to SVG coordinates
            // The layout engine places components in layout space. We need to center this in our SVG viewport.
            let centered_x = position.x + svg_center_x;
            let centered_y = position.y + svg_center_y - 300.0; // Translate so y=300 becomes y=300 (center)
            
            // Get rotation from layout if available
            let rotation = component_layouts.get(instance_id)
                .map(|layout| layout.rotation)
                .unwrap_or(0.0);
            
            let (symbol_svg, pin_locations) = generate_component_symbol_with_rotation(
                &instance.name, 
                &module_name, 
                centered_x, 
                centered_y,
                rotation
            );
            
            svg_content.push_str(&symbol_svg);
            svg_content.push('\n');
            
            // Store pin locations for routing (also offset)
            for (pin_name, pin_pos) in pin_locations {
                let final_pin_pos = Point::new(centered_x + pin_pos.x, centered_y + pin_pos.y);
                all_pin_locations.insert(
                    format!("{}.{}", instance.name, pin_name), 
                    final_pin_pos
                );
            }
        }
    }
    
    // Define connections for LDO circuit
    let connections = vec![
        // VIN rail connections
        ("VIN.PWR".to_string(), "C_IN.1".to_string()),
        ("C_IN.1".to_string(), "U1.VIN".to_string()),
        
        // VOUT rail connections  
        ("U1.VOUT".to_string(), "C_OUT.1".to_string()),
        ("C_OUT.1".to_string(), "VOUT.PWR".to_string()),
        
        // Ground rail connections - all to single ground
        ("C_IN.2".to_string(), "GND.GND".to_string()),
        ("C_OUT.2".to_string(), "GND.GND".to_string()),
        ("U1.GND".to_string(), "GND.GND".to_string()),
    ];
    
    // Create component obstacle information with actual SVG positions and bounds
    let component_obstacles = vec![
        // Only include the LDO as an obstacle - capacitor plates should not block 
        // their own connections since wires connect TO the plates
        (Point::new(400.0, 150.0), 50.0, 30.0),  // LDO center, width, height
    ];
    
    let routing_lines = create_smart_routing_for_connections_with_obstacles(&all_pin_locations, &connections, &component_obstacles);
    
    for line in routing_lines {
        svg_content.push_str(&line);
        svg_content.push('\n');
    }
    
    svg_content.push_str("</svg>");
    
    // Write to file
    std::fs::write("test_output.svg", svg_content).expect("Failed to write SVG file");
    println!("SVG written to test_output.svg");
}



fn create_test_netlist() -> Netlist {
    let mut netlist = Netlist::new();
    
    // Component modules
    let ldo_module = netlist.add_module("VoltageRegulator".to_string(), ModuleKind::PhysicalComponent);
    let capacitor_module = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let ground_module = netlist.add_module("Ground".to_string(), ModuleKind::PhysicalComponent);
    let power_module = netlist.add_module("Power".to_string(), ModuleKind::PhysicalComponent);
    
    // Component instances
    let ldo_instance = netlist.add_instance("U1".to_string(), ldo_module).unwrap();  // LDO regulator
    let cin_instance = netlist.add_instance("C_IN".to_string(), capacitor_module).unwrap();  // Input cap
    let cout_instance = netlist.add_instance("C_OUT".to_string(), capacitor_module).unwrap(); // Output cap
    let gnd_instance = netlist.add_instance("GND".to_string(), ground_module).unwrap();   // Single ground symbol
    let vin_instance = netlist.add_instance("VIN".to_string(), power_module).unwrap();    // Power input
    let vout_instance = netlist.add_instance("VOUT".to_string(), power_module).unwrap();  // Power output
    
    // Define pins for LDO (VIN, VOUT, GND, EN)
    let ldo_vin_pin = netlist.add_pin(ldo_module, "VIN".to_string()).unwrap();
    let ldo_vout_pin = netlist.add_pin(ldo_module, "VOUT".to_string()).unwrap();
    let ldo_gnd_pin = netlist.add_pin(ldo_module, "GND".to_string()).unwrap();
    let ldo_en_pin = netlist.add_pin(ldo_module, "EN".to_string()).unwrap();
    
    // Define pins for capacitors (1, 2)
    let cap_pin1 = netlist.add_pin(capacitor_module, "1".to_string()).unwrap();
    let cap_pin2 = netlist.add_pin(capacitor_module, "2".to_string()).unwrap();
    
    // Define pins for ground and power symbols
    let gnd_pin = netlist.add_pin(ground_module, "GND".to_string()).unwrap();
    let power_pin = netlist.add_pin(power_module, "PWR".to_string()).unwrap();
    
    // Create nets
    let vin_net = netlist.add_net(Some("VIN_Rail".to_string()));      // Input power rail
    let vout_net = netlist.add_net(Some("VOUT_Rail".to_string()));    // Output power rail
    let gnd_net = netlist.add_net(Some("GND_Rail".to_string()));      // Ground rail
    let en_net = netlist.add_net(Some("ENABLE".to_string()));         // Enable signal
    
    // Connect VIN rail: VIN -> C_IN.1 -> U1.VIN
    netlist.connect(vin_net, ConnectionPoint::InstancePin(vin_instance, power_pin)).unwrap();
    netlist.connect(vin_net, ConnectionPoint::InstancePin(cin_instance, cap_pin1)).unwrap();
    netlist.connect(vin_net, ConnectionPoint::InstancePin(ldo_instance, ldo_vin_pin)).unwrap();
    
    // Connect VOUT rail: U1.VOUT -> C_OUT.1 -> VOUT
    netlist.connect(vout_net, ConnectionPoint::InstancePin(ldo_instance, ldo_vout_pin)).unwrap();
    netlist.connect(vout_net, ConnectionPoint::InstancePin(cout_instance, cap_pin1)).unwrap();
    netlist.connect(vout_net, ConnectionPoint::InstancePin(vout_instance, power_pin)).unwrap();
    
    // Connect GND rail: All grounds to single GND symbol
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(cin_instance, cap_pin2)).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(cout_instance, cap_pin2)).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(ldo_instance, ldo_gnd_pin)).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(gnd_instance, gnd_pin)).unwrap();
    
    // Connect enable (for now, just the LDO enable pin - could add pull-up resistor later)
    netlist.connect(en_net, ConnectionPoint::InstancePin(ldo_instance, ldo_en_pin)).unwrap();
    
    netlist
}

 