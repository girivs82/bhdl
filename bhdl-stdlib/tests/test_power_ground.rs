use bhdl_stdlib::StdlibReader;
use std::env;
use std::path::PathBuf;

#[test]
fn test_power_and_ground_components_loaded() {
    // Get the project root directory
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let stdlib_path = PathBuf::from(manifest_dir);
    let mut reader = StdlibReader::new(stdlib_path.to_str().unwrap());
    reader.load_all_components().expect("Failed to load components");
    
    // Test Power component
    let power_component = reader.get_component("Power");
    assert!(power_component.is_some(), "Power component should be loaded");
    
    let power = power_component.unwrap();
    assert_eq!(power.module_name, "Power");
    assert_eq!(power.pins.len(), 1);
    assert_eq!(power.pins[0].name, "OUT");
    
    // Test Ground component
    let ground_component = reader.get_component("Ground");
    assert!(ground_component.is_some(), "Ground component should be loaded");
    
    let ground = ground_component.unwrap();
    assert_eq!(ground.module_name, "Ground");
    assert_eq!(ground.pins.len(), 1);
    assert_eq!(ground.pins[0].name, "GND");
    
    // Test aliases
    assert!(reader.get_component("PWR").is_some(), "PWR alias should work");
    assert!(reader.get_component("GND").is_some(), "GND alias should work");
    
    println!("Power and Ground components loaded successfully!");
}