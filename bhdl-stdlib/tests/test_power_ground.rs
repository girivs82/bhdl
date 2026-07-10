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
    
    // NOTE: the v1 `Power` entity (power/power.bhdl) was deleted in the
    // stdlib consolidation (bfaa4ed) — power rails are language-level
    // `power VIN = 5V;` declarations, not a stdlib entity. Only Ground
    // remains as a real entity.

    // Test Ground component
    let ground_component = reader.get_component("Ground");
    assert!(ground_component.is_some(), "Ground component should be loaded");
    
    let ground = ground_component.unwrap();
    assert_eq!(ground.module_name, "Ground");
    assert_eq!(ground.pins.len(), 1);
    assert_eq!(ground.pins[0].name, "GND");
    
    // Test alias (PWR aliased the deleted Power entity; only GND remains)
    assert!(reader.get_component("GND").is_some(), "GND alias should work");

    println!("Ground component loaded successfully!");
}