//! Test orthogonal routing to pin positions

use anyhow::Result;
use bhdl_visualizer::types::{Point, Component, Net, RoutingSegment};
use bhdl_visualizer::manhattan_router::{ManhattanRouter, RoutingTopology, Axis};
use bhdl_netlist::{InstanceId, NetId, Netlist, ModuleKind};
use std::collections::HashMap;

fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing orthogonal routing to pin positions...\n");
    
    // Create a netlist to generate proper IDs
    let mut netlist = Netlist::new();
    
    // Create test components with specific pin positions
    let mut components = Vec::new();
    
    // Component 1: Resistor (horizontal)
    let r1_module = netlist.add_module("Resistor".to_string(), ModuleKind::PassiveComponent);
    let r1_instance = netlist.add_instance("R1".to_string(), r1_module).unwrap();
    let mut r1 = Component::new(r1_instance, Point::new(100.0, 100.0));
    r1.label = Some("R1".to_string());
    r1.size = Point::new(120.0, 40.0);
    // Add pin positions relative to component center
    r1.pins.insert("1".to_string(), Point::new(-60.0, 0.0)); // Left pin
    r1.pins.insert("2".to_string(), Point::new(60.0, 0.0));  // Right pin
    components.push(r1.clone());
    
    // Component 2: Capacitor (vertical)
    let c1_module = netlist.add_module("Capacitor".to_string(), ModuleKind::PassiveComponent);
    let c1_instance = netlist.add_instance("C1".to_string(), c1_module).unwrap();
    let mut c1 = Component::new(c1_instance, Point::new(300.0, 200.0));
    c1.label = Some("C1".to_string());
    c1.size = Point::new(60.0, 100.0);
    // Add pin positions relative to component center
    c1.pins.insert("1".to_string(), Point::new(0.0, -50.0)); // Top pin
    c1.pins.insert("2".to_string(), Point::new(0.0, 50.0));  // Bottom pin
    components.push(c1.clone());
    
    // Component 3: IC (rectangular with multiple pins)
    let u1_module = netlist.add_module("IC".to_string(), ModuleKind::IntegratedCircuit);
    let u1_instance = netlist.add_instance("U1".to_string(), u1_module).unwrap();
    let mut u1 = Component::new(u1_instance, Point::new(200.0, 300.0));
    u1.label = Some("U1".to_string());
    u1.size = Point::new(160.0, 120.0);
    // Add pin positions on all sides
    u1.pins.insert("IN".to_string(), Point::new(-80.0, 0.0));    // Left
    u1.pins.insert("OUT".to_string(), Point::new(80.0, 0.0));     // Right
    u1.pins.insert("GND".to_string(), Point::new(0.0, 60.0));     // Bottom
    u1.pins.insert("VCC".to_string(), Point::new(0.0, -60.0));    // Top
    components.push(u1.clone());
    
    // Create router
    let mut router = ManhattanRouter::new(10.0); // 10 unit grid
    
    // Add component bodies as obstacles
    for component in &components {
        let bbox = component.bounding_box();
        router.add_obstacle(
            Point::new(bbox.min_x - 5.0, bbox.min_y - 5.0), // Add margin
            Point::new(bbox.max_x + 5.0, bbox.max_y + 5.0),
        );
    }
    
    // Test routing scenarios
    let mut nets = Vec::new();
    
    // Net 1: R1.2 -> C1.1 (horizontal to vertical)
    let r1_pin2 = r1.get_pin_world_position("2").unwrap();
    let c1_pin1 = c1.get_pin_world_position("1").unwrap();
    
    println!("Routing Net 1: R1.2 ({:.1}, {:.1}) -> C1.1 ({:.1}, {:.1})", 
             r1_pin2.x, r1_pin2.y, c1_pin1.x, c1_pin1.y);
    
    let segments1 = router.route_multi(&[r1_pin2, c1_pin1], RoutingTopology::PointToPoint);
    let net1_id = netlist.add_net(Some("NET1".to_string()));
    let mut net1 = Net::new(net1_id, Some("NET1".to_string()));
    net1.connection_points = vec![r1_pin2, c1_pin1];
    for segment in segments1 {
        net1.add_routing_segment(segment);
    }
    nets.push(net1);
    
    // Net 2: C1.2 -> U1.GND (vertical to IC bottom)
    let c1_pin2 = c1.get_pin_world_position("2").unwrap();
    let u1_gnd = u1.get_pin_world_position("GND").unwrap();
    
    println!("Routing Net 2: C1.2 ({:.1}, {:.1}) -> U1.GND ({:.1}, {:.1})", 
             c1_pin2.x, c1_pin2.y, u1_gnd.x, u1_gnd.y);
    
    let segments2 = router.route_multi(&[c1_pin2, u1_gnd], RoutingTopology::PointToPoint);
    let mut net2 = Net::new(NetId::from_raw(1), Some("GND".to_string()));
    net2.connection_points = vec![c1_pin2, u1_gnd];
    for segment in segments2 {
        net2.add_routing_segment(segment);
    }
    nets.push(net2);
    
    // Net 3: R1.1 -> U1.IN (multiple turns)
    let r1_pin1 = r1.get_pin_world_position("1").unwrap();
    let u1_in = u1.get_pin_world_position("IN").unwrap();
    
    println!("Routing Net 3: R1.1 ({:.1}, {:.1}) -> U1.IN ({:.1}, {:.1})", 
             r1_pin1.x, r1_pin1.y, u1_in.x, u1_in.y);
    
    let segments3 = router.route_multi(&[r1_pin1, u1_in], RoutingTopology::PointToPoint);
    let mut net3 = Net::new(NetId::from_raw(2), Some("INPUT".to_string()));
    net3.connection_points = vec![r1_pin1, u1_in];
    for segment in segments3 {
        net3.add_routing_segment(segment);
    }
    nets.push(net3);
    
    // Net 4: Power distribution (multiple connections)
    let u1_vcc = u1.get_pin_world_position("VCC").unwrap();
    let power_point = Point::new(50.0, 50.0); // Power entry point
    
    println!("\nRouting Net 4: Power distribution");
    println!("  Power ({:.1}, {:.1}) -> U1.VCC ({:.1}, {:.1})", 
             power_point.x, power_point.y, u1_vcc.x, u1_vcc.y);
    
    let segments4 = router.route_multi(
        &[power_point, u1_vcc], 
        RoutingTopology::Bus { main_axis: Axis::Horizontal }
    );
    let mut net4 = Net::new(NetId::from_raw(3), Some("VCC".to_string()));
    net4.connection_points = vec![power_point, u1_vcc];
    for segment in segments4 {
        net4.add_routing_segment(segment);
    }
    nets.push(net4);
    
    // Generate SVG
    println!("\nGenerating SVG...");
    let svg = generate_test_svg(&components, &nets)?;
    
    // Save to test output directory
    let output_path = "tests/outputs/svg/test_orthogonal_routing.svg";
    std::fs::write(output_path, svg)?;
    println!("SVG saved to: {}", output_path);
    
    // Print routing analysis
    println!("\nRouting Analysis:");
    for net in &nets {
        println!("\n{} segments:", net.name.as_ref().unwrap_or(&"Unknown".to_string()));
        for (i, segment) in net.routing_segments.iter().enumerate() {
            match segment {
                RoutingSegment::Line { start, end } => {
                    let is_horizontal = (start.y - end.y).abs() < 0.1;
                    let is_vertical = (start.x - end.x).abs() < 0.1;
                    let orientation = if is_horizontal {
                        "horizontal"
                    } else if is_vertical {
                        "vertical"
                    } else {
                        "diagonal (ERROR!)"
                    };
                    println!("  Segment {}: ({:.1}, {:.1}) -> ({:.1}, {:.1}) [{}]",
                             i + 1, start.x, start.y, end.x, end.y, orientation);
                }
            }
        }
    }
    
    Ok(())
}

fn generate_test_svg(components: &[Component], nets: &[Net]) -> Result<String> {
    let mut svg = String::new();
    
    // Calculate bounds
    let mut min_x = f64::INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    
    for component in components {
        let bbox = component.bounding_box();
        min_x = min_x.min(bbox.min_x);
        min_y = min_y.min(bbox.min_y);
        max_x = max_x.max(bbox.max_x);
        max_y = max_y.max(bbox.max_y);
    }
    
    // Add margin
    let margin = 50.0;
    min_x -= margin;
    min_y -= margin;
    max_x += margin;
    max_y += margin;
    
    let width = max_x - min_x;
    let height = max_y - min_y;
    
    // SVG header
    svg.push_str(&format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<svg width="{}" height="{}" viewBox="{} {} {} {}" xmlns="http://www.w3.org/2000/svg">
  <rect width="100%" height="100%" fill="white"/>
  <g id="grid">
"#,
        width, height, min_x, min_y, width, height
    ));
    
    // Draw grid
    for x in ((min_x / 10.0).floor() as i32..=(max_x / 10.0).ceil() as i32).map(|i| i as f64 * 10.0) {
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#f0f0f0" stroke-width="0.5"/>"#,
            x, min_y, x, max_y
        ));
        svg.push('\n');
    }
    for y in ((min_y / 10.0).floor() as i32..=(max_y / 10.0).ceil() as i32).map(|i| i as f64 * 10.0) {
        svg.push_str(&format!(
            r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#f0f0f0" stroke-width="0.5"/>"#,
            min_x, y, max_x, y
        ));
        svg.push('\n');
    }
    svg.push_str("  </g>\n");
    
    // Draw components
    svg.push_str("  <g id=\"components\">\n");
    for component in components {
        let bbox = component.bounding_box();
        
        // Component body
        svg.push_str(&format!(
            r#"    <rect x="{}" y="{}" width="{}" height="{}" fill="white" stroke="black" stroke-width="2"/>
"#,
            bbox.min_x, bbox.min_y, bbox.width(), bbox.height()
        ));
        
        // Component label
        if let Some(label) = &component.label {
            svg.push_str(&format!(
                r#"    <text x="{}" y="{}" text-anchor="middle" font-family="Arial" font-size="14">{}</text>
"#,
                component.position.x, component.position.y, label
            ));
        }
        
        // Draw pins
        for (pin_name, relative_pos) in &component.pins {
            let world_pos = component.get_pin_world_position(pin_name).unwrap();
            
            // Pin circle
            svg.push_str(&format!(
                r#"    <circle cx="{}" cy="{}" r="3" fill="red" stroke="darkred" stroke-width="1"/>
"#,
                world_pos.x, world_pos.y
            ));
            
            // Pin label
            svg.push_str(&format!(
                r#"    <text x="{}" y="{}" text-anchor="middle" font-family="Arial" font-size="10" fill="red">{}</text>
"#,
                world_pos.x, world_pos.y - 5.0, pin_name
            ));
        }
    }
    svg.push_str("  </g>\n");
    
    // Draw nets
    svg.push_str("  <g id=\"nets\">\n");
    for net in nets {
        let color = match net.name.as_deref() {
            Some("GND") => "#0000ff",
            Some("VCC") => "#ff0000",
            _ => "#00aa00",
        };
        
        // Draw routing segments
        for segment in &net.routing_segments {
            match segment {
                RoutingSegment::Line { start, end } => {
                    svg.push_str(&format!(
                        r#"    <line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="2"/>
"#,
                        start.x, start.y, end.x, end.y, color
                    ));
                }
            }
        }
        
        // Draw connection points
        for point in &net.connection_points {
            svg.push_str(&format!(
                r#"    <circle cx="{}" cy="{}" r="2" fill="{}" stroke="none"/>
"#,
                point.x, point.y, color
            ));
        }
        
        // Net label
        if let Some(name) = &net.name {
            if let Some(first_point) = net.connection_points.first() {
                svg.push_str(&format!(
                    r#"    <text x="{}" y="{}" font-family="Arial" font-size="10" fill="{}">{}</text>
"#,
                    first_point.x + 5.0, first_point.y - 5.0, color, name
                ));
            }
        }
    }
    svg.push_str("  </g>\n");
    
    svg.push_str("</svg>\n");
    
    Ok(svg)
}