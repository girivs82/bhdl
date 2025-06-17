//! Test visualization generation - creates SVG files to demonstrate the new visualizer

use anyhow::Result;
use bhdl_netlist::{Netlist, ModuleKind, ConnectionPoint};
use bhdl_synthesizer::component_mapping::ComponentCategory;
use bhdl_synthesizer::DatabaseComponentInstance;
use bhdl_visualizer::{render_circuit, save_circuit_svg, render_circuit_debug, LayoutConfig, PlacementAlgorithm};

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    println!("🎨 BHDL Visualizer Test - Generating SVG Examples");
    println!("{}", "=".repeat(60));
    
    // Test 1: Simple resistor circuit
    println!("\n📍 Test 1: Simple Resistor Circuit");
    let (netlist1, components1) = create_simple_resistor_circuit();
    
    let svg_content = render_circuit(&netlist1, &components1, None).await?;
    std::fs::write("test_simple_resistor.svg", &svg_content)?;
    println!("✅ Generated: test_simple_resistor.svg");
    
    // Test 2: Linear regulator circuit  
    println!("\n📍 Test 2: Linear Regulator Circuit");
    let (netlist2, components2) = create_linear_regulator_circuit();
    
    save_circuit_svg(&netlist2, &components2, "test_linear_regulator.svg", None).await?;
    println!("✅ Generated: test_linear_regulator.svg");
    
    // Test 3: Debug visualization
    println!("\n📍 Test 3: Debug Visualization with Grid");
    let debug_svg = render_circuit_debug(&netlist2, &components2, None).await?;
    std::fs::write("test_debug_visualization.svg", &debug_svg)?;
    println!("✅ Generated: test_debug_visualization.svg");
    
    // Test 4: Custom layout configuration
    println!("\n📍 Test 4: Custom Layout Configuration");
    let mut config = LayoutConfig::default();
    config.placement_algorithm = PlacementAlgorithm::Grid;
    config.component_spacing = 60.0;
    config.grid_spacing = 15.0;
    
    let custom_svg = render_circuit(&netlist2, &components2, Some(config)).await?;
    std::fs::write("test_custom_layout.svg", &custom_svg)?;
    println!("✅ Generated: test_custom_layout.svg");
    
    println!("\n🎉 All SVG files generated successfully!");
    println!("You can open these SVG files in any web browser or SVG viewer:");
    println!("  - test_simple_resistor.svg");
    println!("  - test_linear_regulator.svg"); 
    println!("  - test_debug_visualization.svg");
    println!("  - test_custom_layout.svg");
    
    Ok(())
}

/// Create a simple resistor circuit for testing
fn create_simple_resistor_circuit() -> (Netlist, Vec<DatabaseComponentInstance>) {
    let mut netlist = Netlist::new();
    
    // Create resistor module
    let resistor_mod = netlist.add_module("Resistor".to_string(), ModuleKind::PhysicalComponent);
    let r1_pin1 = netlist.add_pin(resistor_mod, "1".to_string()).unwrap();
    let r1_pin2 = netlist.add_pin(resistor_mod, "2".to_string()).unwrap();
    
    // Create instances
    let r1 = netlist.add_instance("R1".to_string(), resistor_mod).unwrap();
    let r2 = netlist.add_instance("R2".to_string(), resistor_mod).unwrap();
    
    // Create nets
    let vcc_net = netlist.add_net(Some("VCC".to_string()));
    let signal_net = netlist.add_net(Some("SIGNAL".to_string()));
    let gnd_net = netlist.add_net(Some("GND".to_string()));
    
    // Connect R1
    netlist.connect(vcc_net, ConnectionPoint::InstancePin(r1, r1_pin1)).unwrap();
    netlist.connect(signal_net, ConnectionPoint::InstancePin(r1, r1_pin2)).unwrap();
    
    // Connect R2  
    netlist.connect(signal_net, ConnectionPoint::InstancePin(r2, r1_pin1)).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(r2, r1_pin2)).unwrap();
    
    // Create database components
    let components = vec![
        create_test_resistor("R1", "10kΩ"),
        create_test_resistor("R2", "1kΩ"),
    ];
    
    (netlist, components)
}

/// Create a linear regulator circuit for testing
fn create_linear_regulator_circuit() -> (Netlist, Vec<DatabaseComponentInstance>) {
    let mut netlist = Netlist::new();
    
    // Create modules
    let regulator_mod = netlist.add_module("LM7805".to_string(), ModuleKind::Component);
    let reg_input = netlist.add_pin(regulator_mod, "input".to_string()).unwrap();
    let reg_gnd = netlist.add_pin(regulator_mod, "ground".to_string()).unwrap();
    let reg_output = netlist.add_pin(regulator_mod, "output".to_string()).unwrap();
    
    let cap_mod = netlist.add_module("Capacitor".to_string(), ModuleKind::PhysicalComponent);
    let cap_pos = netlist.add_pin(cap_mod, "positive".to_string()).unwrap();
    let cap_neg = netlist.add_pin(cap_mod, "negative".to_string()).unwrap();
    
    let led_mod = netlist.add_module("LED".to_string(), ModuleKind::PhysicalComponent);
    let led_anode = netlist.add_pin(led_mod, "anode".to_string()).unwrap();
    let led_cathode = netlist.add_pin(led_mod, "cathode".to_string()).unwrap();
    
    // Create instances
    let u1 = netlist.add_instance("U1".to_string(), regulator_mod).unwrap();
    let c1 = netlist.add_instance("C1".to_string(), cap_mod).unwrap();
    let c2 = netlist.add_instance("C2".to_string(), cap_mod).unwrap();
    let led1 = netlist.add_instance("LED1".to_string(), led_mod).unwrap();
    
    // Create nets
    let vin_net = netlist.add_net(Some("VIN".to_string()));
    let vout_net = netlist.add_net(Some("VOUT".to_string()));
    let gnd_net = netlist.add_net(Some("GND".to_string()));
    
    // Connect input side: VIN -> C1+ -> U1.input
    netlist.connect(vin_net, ConnectionPoint::InstancePin(c1, cap_pos)).unwrap();
    netlist.connect(vin_net, ConnectionPoint::InstancePin(u1, reg_input)).unwrap();
    
    // Connect output side: U1.output -> C2+ -> LED+ 
    netlist.connect(vout_net, ConnectionPoint::InstancePin(u1, reg_output)).unwrap();
    netlist.connect(vout_net, ConnectionPoint::InstancePin(c2, cap_pos)).unwrap();
    netlist.connect(vout_net, ConnectionPoint::InstancePin(led1, led_anode)).unwrap();
    
    // Connect ground: C1- -> C2- -> U1.gnd -> LED-
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(c1, cap_neg)).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(c2, cap_neg)).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(u1, reg_gnd)).unwrap();
    netlist.connect(gnd_net, ConnectionPoint::InstancePin(led1, led_cathode)).unwrap();
    
    // Create database components
    let components = vec![
        create_test_regulator("U1"),
        create_test_capacitor("C1", "100µF"),
        create_test_capacitor("C2", "10µF"), 
        create_test_led("LED1", "Red"),
    ];
    
    (netlist, components)
}

/// Create a test resistor component
fn create_test_resistor(name: &str, value: &str) -> DatabaseComponentInstance {
    DatabaseComponentInstance {
        instance_name: name.to_string(),
        bhdl_type: "Resistor".to_string(),
        component_id: 1,
        component_name: format!("R_{}_{}", name, value),
        component_description: Some(format!("Resistor {}", value)),
        svg_data: generate_resistor_svg(),
        pin_mapping: [
            ("1".to_string(), "1".to_string()),
            ("2".to_string(), "2".to_string()),
        ].iter().cloned().collect(),
        category: ComponentCategory::PassiveResistor,
        electrical_specs: vec![],
        pins: vec![
            bhdl_components::types::PinDefinition {
                pin_number: "1".to_string(),
                pin_name: Some("1".to_string()),
                electrical_type: bhdl_components::types::PinType::Passive,
                x_position: -20.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
            bhdl_components::types::PinDefinition {
                pin_number: "2".to_string(),
                pin_name: Some("2".to_string()),
                electrical_type: bhdl_components::types::PinType::Passive,
                x_position: 20.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
        ],
    }
}

/// Create a test capacitor component
fn create_test_capacitor(name: &str, value: &str) -> DatabaseComponentInstance {
    DatabaseComponentInstance {
        instance_name: name.to_string(),
        bhdl_type: "Capacitor".to_string(),
        component_id: 2,
        component_name: format!("C_{}_{}", name, value),
        component_description: Some(format!("Capacitor {}", value)),
        svg_data: generate_capacitor_svg(),
        pin_mapping: [
            ("positive".to_string(), "1".to_string()),
            ("negative".to_string(), "2".to_string()),
        ].iter().cloned().collect(),
        category: ComponentCategory::PassiveCapacitor,
        electrical_specs: vec![],
        pins: vec![
            bhdl_components::types::PinDefinition {
                pin_number: "1".to_string(),
                pin_name: Some("positive".to_string()),
                electrical_type: bhdl_components::types::PinType::Passive,
                x_position: -15.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
            bhdl_components::types::PinDefinition {
                pin_number: "2".to_string(),
                pin_name: Some("negative".to_string()),
                electrical_type: bhdl_components::types::PinType::Passive,
                x_position: 15.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
        ],
    }
}

/// Create a test regulator component
fn create_test_regulator(name: &str) -> DatabaseComponentInstance {
    DatabaseComponentInstance {
        instance_name: name.to_string(),
        bhdl_type: "LM7805".to_string(),
        component_id: 3,
        component_name: format!("LM7805_{}", name),
        component_description: Some("5V Linear Regulator".to_string()),
        svg_data: generate_regulator_svg(),
        pin_mapping: [
            ("input".to_string(), "1".to_string()),
            ("ground".to_string(), "2".to_string()),
            ("output".to_string(), "3".to_string()),
        ].iter().cloned().collect(),
        category: ComponentCategory::PowerRegulator,
        electrical_specs: vec![],
        pins: vec![
            bhdl_components::types::PinDefinition {
                pin_number: "1".to_string(),
                pin_name: Some("input".to_string()),
                electrical_type: bhdl_components::types::PinType::Input,
                x_position: -25.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
            bhdl_components::types::PinDefinition {
                pin_number: "2".to_string(),
                pin_name: Some("ground".to_string()),
                electrical_type: bhdl_components::types::PinType::Ground,
                x_position: 0.0,
                y_position: 15.0,
                orientation: 90,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
            bhdl_components::types::PinDefinition {
                pin_number: "3".to_string(),
                pin_name: Some("output".to_string()),
                electrical_type: bhdl_components::types::PinType::Output,
                x_position: 25.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
        ],
    }
}

/// Create a test LED component
fn create_test_led(name: &str, color: &str) -> DatabaseComponentInstance {
    DatabaseComponentInstance {
        instance_name: name.to_string(),
        bhdl_type: "LED".to_string(),
        component_id: 4,
        component_name: format!("LED_{}_{}", name, color),
        component_description: Some(format!("{} LED", color)),
        svg_data: generate_led_svg(),
        pin_mapping: [
            ("anode".to_string(), "1".to_string()),
            ("cathode".to_string(), "2".to_string()),
        ].iter().cloned().collect(),
        category: ComponentCategory::Semiconductor,
        electrical_specs: vec![],
        pins: vec![
            bhdl_components::types::PinDefinition {
                pin_number: "1".to_string(),
                pin_name: Some("anode".to_string()),
                electrical_type: bhdl_components::types::PinType::Input,
                x_position: -15.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
            bhdl_components::types::PinDefinition {
                pin_number: "2".to_string(),
                pin_name: Some("cathode".to_string()),
                electrical_type: bhdl_components::types::PinType::Output,
                x_position: 15.0,
                y_position: 0.0,
                orientation: 0,
                length: 10.0,
                pin_shape: bhdl_components::types::PinShape::Line,
            },
        ],
    }
}

/// Generate resistor SVG symbol
fn generate_resistor_svg() -> String {
    r#"<g>
        <rect x="-15" y="-5" width="30" height="10" fill="white" stroke="black" stroke-width="1.5"/>
        <line x1="-20" y1="0" x2="-15" y2="0" stroke="black" stroke-width="1"/>
        <line x1="15" y1="0" x2="20" y2="0" stroke="black" stroke-width="1"/>
        <text x="0" y="15" text-anchor="middle" font-size="8" fill="black">R</text>
    </g>"#.to_string()
}

/// Generate capacitor SVG symbol
fn generate_capacitor_svg() -> String {
    r#"<g>
        <line x1="-5" y1="-12" x2="-5" y2="12" stroke="black" stroke-width="2"/>
        <line x1="5" y1="-12" x2="5" y2="12" stroke="black" stroke-width="2"/>
        <line x1="-15" y1="0" x2="-5" y2="0" stroke="black" stroke-width="1"/>
        <line x1="5" y1="0" x2="15" y2="0" stroke="black" stroke-width="1"/>
        <text x="0" y="25" text-anchor="middle" font-size="8" fill="black">C</text>
    </g>"#.to_string()
}

/// Generate voltage regulator SVG symbol
fn generate_regulator_svg() -> String {
    r#"<g>
        <rect x="-20" y="-10" width="40" height="20" fill="white" stroke="black" stroke-width="1.5"/>
        <text x="0" y="3" text-anchor="middle" font-size="8" fill="black">LM7805</text>
        <line x1="-25" y1="0" x2="-20" y2="0" stroke="black" stroke-width="1"/>
        <line x1="20" y1="0" x2="25" y2="0" stroke="black" stroke-width="1"/>
        <line x1="0" y1="10" x2="0" y2="15" stroke="black" stroke-width="1"/>
        <text x="-25" y="-5" text-anchor="middle" font-size="6" fill="black">IN</text>
        <text x="25" y="-5" text-anchor="middle" font-size="6" fill="black">OUT</text>
        <text x="0" y="28" text-anchor="middle" font-size="6" fill="black">GND</text>
    </g>"#.to_string()
}

/// Generate LED SVG symbol
fn generate_led_svg() -> String {
    r#"<g>
        <polygon points="-5,-10 5,0 -5,10" fill="white" stroke="black" stroke-width="1.5"/>
        <line x1="5" y1="-10" x2="5" y2="10" stroke="black" stroke-width="2"/>
        <line x1="-15" y1="0" x2="-5" y2="0" stroke="black" stroke-width="1"/>
        <line x1="5" y1="0" x2="15" y2="0" stroke="black" stroke-width="1"/>
        <line x1="8" y1="-8" x2="12" y2="-12" stroke="red" stroke-width="1.5" marker-end="url(#arrowhead)"/>
        <line x1="8" y1="-5" x2="12" y2="-9" stroke="red" stroke-width="1.5" marker-end="url(#arrowhead)"/>
        <text x="0" y="20" text-anchor="middle" font-size="8" fill="black">LED</text>
        <defs>
            <marker id="arrowhead" markerWidth="10" markerHeight="7" refX="9" refY="3.5" orient="auto">
                <polygon points="0 0, 10 3.5, 0 7" fill="red"/>
            </marker>
        </defs>
    </g>"#.to_string()
}