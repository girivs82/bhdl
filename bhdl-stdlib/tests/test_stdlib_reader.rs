//! Test the BHDL stdlib reader functionality

use anyhow::Result;
use bhdl_stdlib::{StdlibReader, get_default_stdlib_path};

#[test]
fn test_stdlib_reader_loads_components() -> Result<()> {
    println!("=== Testing BHDL Stdlib Reader ===\n");
    
    // Create stdlib reader with correct path
    // When running tests from bhdl-stdlib, CARGO_MANIFEST_DIR is already the stdlib directory
    let stdlib_path = env!("CARGO_MANIFEST_DIR").to_string();
    let mut reader = StdlibReader::new(stdlib_path);
    
    // Load all components
    reader.load_all_components()?;
    
    // Test known components
    let test_cases = vec![
        ("Res", vec!["1", "2"]),
        ("Cap", vec!["1", "2"]),
        ("LED", vec!["A", "K"]),
        ("LM7805", vec!["IN", "GND", "OUT"]),
        ("Fuse", vec!["1", "2"]),
        ("TestPoint", vec!["1"]),
    ];
    
    for (component_name, expected_pins) in test_cases {
        println!("Testing component: {}", component_name);
        
        if let Some(component_def) = reader.get_component(component_name) {
            println!("  Found entity: {}", component_def.module_name);
            println!("  Pins: {} total", component_def.pins.len());
            
            for pin in &component_def.pins {
                println!("    - {} ({:?}, {:?})", pin.name, pin.direction, pin.pin_type);
            }
            
            // Check expected pins
            for expected_pin in expected_pins {
                let found = component_def.pins.iter().any(|p| p.name == expected_pin);
                if !found {
                    println!("  ERROR: Missing expected pin '{}'", expected_pin);
                }
            }
        } else {
            println!("  ERROR: Component '{}' not found in stdlib", component_name);
        }
        println!();
    }
    
    // Test get_component_pins method
    println!("Testing get_component_pins method:");
    let resistor_pins = reader.get_component_pins("Resistor");
    println!("  Resistor pins: {:?}", resistor_pins.iter().map(|p| &p.name).collect::<Vec<_>>());
    
    let unknown_pins = reader.get_component_pins("UnknownComponent");
    println!("  Unknown component pins (should be default): {:?}", unknown_pins.iter().map(|p| &p.name).collect::<Vec<_>>());
    
    Ok(())
}