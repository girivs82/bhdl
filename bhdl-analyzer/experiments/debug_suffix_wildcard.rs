use bhdl_analyzer::passes::InstanceRegistry;
use bhdl_analyzer::passes::instance_registry::{InstanceInfo, InstanceKind, ModuleContents};
use std::collections::HashMap;

fn main() {
    // Create a registry and manually populate it to match the test circuit
    let mut registry = InstanceRegistry::new();

    // Register the SensorArray module definition
    let mut sensor_array_contents = ModuleContents {
        components: HashMap::new(),
        modules: HashMap::new(),
    };
    sensor_array_contents.components.insert("temp_sensor".to_string(), InstanceInfo {
        type_name: "TempSensor".to_string(),
        is_array_element: false,
        kind: InstanceKind::Component,
    });
    sensor_array_contents.components.insert("humidity_sensor".to_string(), InstanceInfo {
        type_name: "HumiditySensor".to_string(),
        is_array_element: false,
        kind: InstanceKind::Component,
    });
    sensor_array_contents.components.insert("pressure_sensor".to_string(), InstanceInfo {
        type_name: "PressureSensor".to_string(),
        is_array_element: false,
        kind: InstanceKind::Component,
    });

    registry.register_module_definition("SensorArray".to_string(), sensor_array_contents);

    // Register the array instance
    registry.register_module("array".to_string(), "SensorArray".to_string(), false);

    println!("=== Testing Suffix Wildcard Expansion ===\n");

    // Test the expansion
    let path = "array.*sensor.VCC";
    println!("Input path: {}", path);

    let expanded = registry.expand_hierarchical_wildcard(path);

    println!("\nExpanded to {} path(s):", expanded.len());
    for (i, p) in expanded.iter().enumerate() {
        println!("  [{}] {}", i + 1, p);
    }

    println!("\nExpected 3 paths:");
    println!("  array.temp_sensor.VCC");
    println!("  array.humidity_sensor.VCC");
    println!("  array.pressure_sensor.VCC");
}
