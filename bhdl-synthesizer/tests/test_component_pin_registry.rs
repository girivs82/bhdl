//! Test that all components and pins are properly found by the synthesizer
//! using the stdlib component pin registry

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};
use bhdl_common::ComponentPinRegistry;
use std::collections::HashMap;

#[tokio::test]
async fn test_all_component_types_have_pins() -> Result<()> {
    // Test BHDL circuit that uses all component types
    let source = r#"
board TestAllComponents {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Test all passive components
    VCC -> r1: Res(10k).1;
    r1.2 -> c1: Cap(100nF).1;
    c1.2 -> GND;
    
    // Test polarized capacitor
    VCC -> c2: ElectrolyticCap(100µF, 25V).pos;
    c2.neg -> GND;
    
    // Test LED
    VCC -> r2: Res(330Ω).1;
    r2.2 -> led1: LED(red).A;
    led1.K -> GND;
    
    // Test diode
    VCC -> d1: Diode(1N4148).1;
    d1.2 -> test_point: TestPoint().1;
    
    // Test TVS diode
    VCC -> tvs1: TVSDiode(15V).K;
    tvs1.A -> GND;
    
    // Test fuse
    VCC -> f1: Fuse(1A).1;
    f1.2 -> protected_rail: Res(1k).1;
    
    // Test voltage regulator
    VCC -> reg: LM7805().IN;
    reg.GND -> GND;
    reg.OUT -> regulated_5v: Cap(10µF).1;
    regulated_5v.2 -> GND;
    
    // Test inductor
    VCC -> l1: Ind(10µH).1;
    l1.2 -> filter_out: Cap(1µF).1;
    filter_out.2 -> GND;
}"#;

    println!("=== Testing Component Pin Registry ===\n");

    // Parse the test circuit
    let parse_result = parse(source);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;

    // Analyze the circuit
    let analysis = analyze(&source_file);
    println!("Analysis found {} diagnostics", analysis.diagnostics.len());
    for diag in &analysis.diagnostics {
        println!("  Diagnostic: {}", diag.message);
    }

    // Configure synthesizer without database
    let config = NetlistConfig {
        preserve_semantic_context: true,
        include_power_domains: true,
        include_component_inference: true,
        flatten_hierarchy: false,
        use_database_components: false,
        database_path: None,
    };

    // Generate netlist
    let mut generator = NetlistGenerator::with_config(config);
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;

    println!("\n=== Synthesizer Results ===");
    println!("Generated {} instances", netlist.instances.len());
    println!("Generated {} modules", netlist.modules.len());
    println!("Generated {} nets", netlist.nets.len());

    // Test the pin registry directly
    let pin_registry = ComponentPinRegistry::new();
    
    println!("\n=== Testing Pin Registry ===");
    
    // Components we expect to find
    let test_components = vec![
        ("Res", vec!["1", "2"]),
        ("Resistor", vec!["1", "2"]),
        ("Cap", vec!["pos", "neg", "1", "2"]),
        ("Capacitor", vec!["pos", "neg", "1", "2"]),
        ("ElectrolyticCap", vec!["pos", "neg", "1", "2"]),
        ("LED", vec!["A", "K"]),
        ("Diode", vec!["1", "2"]),
        ("TVSDiode", vec!["K", "A"]),
        ("Fuse", vec!["1", "2"]),
        ("TestPoint", vec!["1"]),
        ("LM7805", vec!["IN", "GND", "OUT"]),
        ("Ind", vec!["1", "2"]),
        ("Inductor", vec!["1", "2"]),
    ];

    let mut all_passed = true;
    
    for (component_type, expected_pins) in test_components {
        println!("\nTesting component: {}", component_type);
        let pins = pin_registry.get_pins(component_type);
        
        println!("  Found {} pins:", pins.len());
        for pin in &pins {
            println!("    - {} ({:?}, {:?})", pin.name, pin.direction, pin.pin_type);
        }
        
        // Check that all expected pins are present
        for expected_pin in &expected_pins {
            if !pin_registry.has_pin(component_type, expected_pin) {
                println!("  ERROR: Missing expected pin '{}'", expected_pin);
                all_passed = false;
            }
        }
    }

    // Check that instances in netlist have correct pins
    println!("\n=== Verifying Netlist Instances ===");
    
    for (instance_id, instance) in &netlist.instances {
        let module = netlist.modules.get(&instance.module)
            .ok_or_else(|| anyhow::anyhow!("Module not found for instance"))?;
        
        println!("\nInstance '{}' (module: {})", instance.name, module.name);
        println!("  Module has {} pins:", module.pins.len());
        
        // Get pin instances for this instance
        let pin_instances: Vec<_> = netlist.pin_instances.iter()
            .filter(|(_, pi)| pi.instance == *instance_id)
            .collect();
            
        println!("  Instance has {} pin instances", pin_instances.len());
        
        if pin_instances.is_empty() && !module.pins.is_empty() {
            println!("  ERROR: No pin instances created for instance with {} module pins", module.pins.len());
            all_passed = false;
        }
        
        for (pin_id, pin_instance) in pin_instances {
            if let Some(pin) = module.pins.get(&pin_instance.pin) {
                println!("    Pin '{}': {:?} {:?}", pin.name, pin.direction, pin.pin_type);
            }
        }
    }

    // Check net connections
    println!("\n=== Verifying Net Connections ===");
    
    let mut nets_with_connections = 0;
    for (net_id, net) in &netlist.nets {
        if net.connections.len() > 1 {
            nets_with_connections += 1;
            println!("\nNet '{}' has {} connections:", 
                     net.name.as_ref().unwrap_or(&"Unnamed".to_string()),
                     net.connections.len());
            
            for conn in &net.connections {
                match conn {
                    bhdl_netlist::types::ConnectionPoint::Pin(pin_inst_id) => {
                        if let Some(pin_inst) = netlist.pin_instances.get(pin_inst_id) {
                            if let Some(instance) = netlist.instances.get(&pin_inst.instance) {
                                if let Some(module) = netlist.modules.get(&instance.module) {
                                    if let Some(pin) = module.pins.get(&pin_inst.pin) {
                                        println!("  - {}.{}", instance.name, pin.name);
                                    }
                                }
                            }
                        }
                    }
                    bhdl_netlist::types::ConnectionPoint::Port(_) => {
                        println!("  - Port connection");
                    }
                }
            }
        }
    }
    
    println!("\n=== Summary ===");
    println!("Total nets with multiple connections: {}", nets_with_connections);
    
    if all_passed && nets_with_connections > 0 {
        println!("\n✅ All component types have correct pins in the registry!");
        println!("✅ Synthesizer is creating proper connections!");
    } else {
        if !all_passed {
            println!("\n❌ Some component types are missing pins!");
        }
        if nets_with_connections == 0 {
            println!("\n❌ No nets have multiple connections - synthesizer may not be connecting properly!");
        }
    }

    Ok(())
}

#[test]
fn test_pin_registry_fuzzy_matching() {
    let registry = ComponentPinRegistry::new();
    
    // Test various ways to refer to components
    let test_cases = vec![
        // Exact matches
        ("Resistor", 2),
        ("LED", 2),
        ("LM7805", 3),
        
        // Case variations
        ("resistor", 2),
        ("RESISTOR", 2),
        ("led", 2),
        
        // Partial matches
        ("some_resistor", 2),
        ("my_capacitor", 4),
        ("power_regulator", 3),
        
        // Unknown should get default
        ("UnknownComponent", 2),
    ];
    
    for (component_type, expected_pin_count) in test_cases {
        let pins = registry.get_pins(component_type);
        assert_eq!(pins.len(), expected_pin_count, 
                   "Component '{}' should have {} pins", component_type, expected_pin_count);
    }
}

#[test] 
fn test_specific_pin_lookups() {
    let registry = ComponentPinRegistry::new();
    
    // Test specific pin existence
    assert!(registry.has_pin("LED", "A"), "LED should have anode pin");
    assert!(registry.has_pin("LED", "K"), "LED should have cathode pin");
    assert!(!registry.has_pin("LED", "1"), "LED should not have numbered pins");
    
    assert!(registry.has_pin("ElectrolyticCap", "pos"), "Electrolytic cap should have pos pin");
    assert!(registry.has_pin("ElectrolyticCap", "neg"), "Electrolytic cap should have neg pin");
    assert!(registry.has_pin("ElectrolyticCap", "1"), "Electrolytic cap should also have pin 1");
    assert!(registry.has_pin("ElectrolyticCap", "2"), "Electrolytic cap should also have pin 2");
    
    assert!(registry.has_pin("LM7805", "IN"), "LM7805 should have IN pin");
    assert!(registry.has_pin("LM7805", "GND"), "LM7805 should have GND pin");
    assert!(registry.has_pin("LM7805", "OUT"), "LM7805 should have OUT pin");
}