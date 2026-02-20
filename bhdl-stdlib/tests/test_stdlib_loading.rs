use bhdl_stdlib::{StdlibReader, get_default_stdlib_path};
use std::path::Path;

#[test]
fn test_stdlib_loads_entities() {
    let stdlib_path = get_default_stdlib_path();
    let full_path = Path::new(&env!("CARGO_MANIFEST_DIR")).parent().unwrap().join(&stdlib_path);
    
    let mut reader = StdlibReader::new(full_path.to_str().unwrap());
    
    // Load all components
    reader.load_all_components().expect("Failed to load stdlib components");
    
    // Test that we can find common components
    let res = reader.get_component("Res");
    assert!(res.is_some(), "Should find Res entity");
    assert_eq!(res.unwrap().module_name, "Res");
    
    let led = reader.get_component("LED");
    assert!(led.is_some(), "Should find LED entity");
    assert_eq!(led.unwrap().module_name, "LED");
    
    let cap = reader.get_component("Cap");
    assert!(cap.is_some(), "Should find Cap entity");
    assert_eq!(cap.unwrap().module_name, "Cap");
    
    // Test aliases work
    let resistor = reader.get_component("Resistor");
    assert!(resistor.is_some(), "Should find Resistor alias");
    
    // Test Power/Ground components
    let power = reader.get_component("Power");
    assert!(power.is_some(), "Should find Power entity");
    
    let ground = reader.get_component("Ground");
    assert!(ground.is_some(), "Should find Ground entity");
    
    // Test getting pin information
    let res_pins = reader.get_component_pins("Res");
    assert_eq!(res_pins.len(), 2, "Resistor should have 2 pins");
    
    let led_pins = reader.get_component_pins("LED");
    assert_eq!(led_pins.len(), 2, "LED should have 2 pins (A and K)");
}