//! Simple test for semantic visualizer concept

use bhdl_visualizer::types::{CircuitLayout, Component, Net, Point, RoutingSegment, Orientation};
use bhdl_visualizer::svg::{SvgDocument, SvgRenderer};
use bhdl_netlist::{Netlist, ModuleKind, NetClass, PinDirection, PinType};
use std::collections::HashMap;

fn main() {
    println!("Testing semantic visualizer concept with manual layout");
    
    // Create a simple circuit layout manually to demonstrate the concept
    let mut layout = CircuitLayout::new();
    
    // Create a simple netlist for IDs
    let mut netlist = Netlist::new();
    let reg_mod = netlist.add_module("LM7805".to_string(), ModuleKind::Component);
    let cap_mod = netlist.add_module("Cap".to_string(), ModuleKind::PhysicalComponent);
    
    // Add instances
    let u1 = netlist.add_instance("U1".to_string(), reg_mod).unwrap();
    let c1 = netlist.add_instance("C1".to_string(), cap_mod).unwrap();
    let c2 = netlist.add_instance("C2".to_string(), cap_mod).unwrap();
    
    // Create nets
    let vin = netlist.add_net_with_class(Some("VIN".to_string()), NetClass::Power(12.0));
    let vout = netlist.add_net_with_class(Some("VOUT".to_string()), NetClass::Power(5.0));
    let gnd = netlist.add_net_with_class(Some("GND".to_string()), NetClass::Ground);
    
    // Create components with semantic layout
    // Regulator at center
    let mut regulator = Component::new(u1, Point::new(200.0, 200.0));
    regulator = regulator.with_size(80.0, 60.0);
    regulator.pins.insert("IN".to_string(), Point::new(-40.0, 0.0));
    regulator.pins.insert("GND".to_string(), Point::new(0.0, 30.0));
    regulator.pins.insert("OUT".to_string(), Point::new(40.0, 0.0));
    layout.add_component(regulator);
    
    // Input capacitor to the left, vertical orientation
    let mut input_cap = Component::new(c1, Point::new(100.0, 200.0));
    input_cap = input_cap.with_size(30.0, 50.0).with_rotation(90.0);
    input_cap.pins.insert("1".to_string(), Point::new(0.0, -25.0));
    input_cap.pins.insert("2".to_string(), Point::new(0.0, 25.0));
    layout.add_component(input_cap);
    
    // Output capacitor to the right, vertical orientation
    let mut output_cap = Component::new(c2, Point::new(300.0, 200.0));
    output_cap = output_cap.with_size(30.0, 50.0).with_rotation(90.0);
    output_cap.pins.insert("1".to_string(), Point::new(0.0, -25.0));
    output_cap.pins.insert("2".to_string(), Point::new(0.0, 25.0));
    layout.add_component(output_cap);
    
    // Create nets with semantic routing
    // VIN net
    let mut vin_net = Net::new(vin, Some("VIN".to_string()));
    vin_net.add_connection_point(Point::new(50.0, 200.0)); // Input
    vin_net.add_connection_point(Point::new(100.0, 175.0)); // C1.1
    vin_net.add_connection_point(Point::new(160.0, 200.0)); // U1.IN
    vin_net.add_routing_segment(RoutingSegment::line(Point::new(50.0, 200.0), Point::new(100.0, 200.0)));
    vin_net.add_routing_segment(RoutingSegment::line(Point::new(100.0, 200.0), Point::new(100.0, 175.0)));
    vin_net.add_routing_segment(RoutingSegment::line(Point::new(100.0, 200.0), Point::new(160.0, 200.0)));
    layout.add_net(vin_net);
    
    // VOUT net
    let mut vout_net = Net::new(vout, Some("VOUT".to_string()));
    vout_net.add_connection_point(Point::new(240.0, 200.0)); // U1.OUT
    vout_net.add_connection_point(Point::new(300.0, 175.0)); // C2.1
    vout_net.add_connection_point(Point::new(350.0, 200.0)); // Output
    vout_net.add_routing_segment(RoutingSegment::line(Point::new(240.0, 200.0), Point::new(300.0, 200.0)));
    vout_net.add_routing_segment(RoutingSegment::line(Point::new(300.0, 200.0), Point::new(300.0, 175.0)));
    vout_net.add_routing_segment(RoutingSegment::line(Point::new(300.0, 200.0), Point::new(350.0, 200.0)));
    layout.add_net(vout_net);
    
    // GND net (star ground pattern)
    let mut gnd_net = Net::new(gnd, Some("GND".to_string()));
    let gnd_center = Point::new(200.0, 250.0);
    gnd_net.add_connection_point(Point::new(100.0, 225.0)); // C1.2
    gnd_net.add_connection_point(Point::new(200.0, 230.0)); // U1.GND
    gnd_net.add_connection_point(Point::new(300.0, 225.0)); // C2.2
    gnd_net.add_connection_point(gnd_center); // Star point
    // Star routing
    gnd_net.add_routing_segment(RoutingSegment::line(Point::new(100.0, 225.0), gnd_center));
    gnd_net.add_routing_segment(RoutingSegment::line(Point::new(200.0, 230.0), gnd_center));
    gnd_net.add_routing_segment(RoutingSegment::line(Point::new(300.0, 225.0), gnd_center));
    layout.add_net(gnd_net);
    
    // Generate SVG
    let renderer = SvgRenderer::new();
    let svg_content = renderer.render(&layout).unwrap();
    
    // Save to file
    std::fs::write("semantic_layout_demo.svg", svg_content).unwrap();
    
    println!("✅ Semantic layout demo saved to: semantic_layout_demo.svg");
    println!("This demonstrates:");
    println!("- Linear regulator centered");
    println!("- Input/output capacitors vertically oriented on sides");
    println!("- Power flow from left to right");
    println!("- Star ground routing pattern");
}