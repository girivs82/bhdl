// Test generate block wildcard integration
// Verifies that wildcards correctly expand to instances created in generate blocks

use bhdl_parser;
use bhdl_analyzer::passes::{build_instance_registry, expand_power_domains};
use bhdl_ast::{AstNode, SourceFile};

fn main() {
    // Parse test circuit with generate blocks
    let source = std::fs::read_to_string("tests/circuits/realistic/test_generate_wildcard.bhdl")
        .expect("Failed to read test file");

    println!("=== Testing Generate Block Wildcard Integration ===\n");
    println!("Source circuit: test_generate_wildcard.bhdl\n");

    // Parse the source
    let parse = bhdl_parser::parse(&source);
    let ast = SourceFile::cast(parse.syntax()).expect("Failed to cast to SourceFile");

    println!("--- Pass 1.25: Building Instance Registry ---");
    let registry = build_instance_registry(&ast);
    println!();

    // Verify that generate-created instances were registered
    println!("--- Verifying Generate-Created Instances ---");
    let expected_sensors = vec![
        "sensor[0]", "sensor[1]", "sensor[2]", "sensor[3]",
        "sensor[4]", "sensor[5]", "sensor[6]", "sensor[7]",
    ];

    for sensor in &expected_sensors {
        if registry.get_instance(sensor).is_some() {
            println!("  ✓ {} registered", sensor);
        } else {
            println!("  ✗ {} NOT registered (ERROR)", sensor);
        }
    }
    println!();

    // Verify manual instances
    println!("--- Verifying Manual Instances ---");
    let expected_leds = vec!["led_0", "led_1", "led_2"];

    for led in &expected_leds {
        if registry.get_instance(led).is_some() {
            println!("  ✓ {} registered", led);
        } else {
            println!("  ✗ {} NOT registered (ERROR)", led);
        }
    }
    println!();

    // Expand power domains with wildcards
    println!("--- Pass 1.5: Expanding Power Domain Wildcards ---");
    let expansion = expand_power_domains(&ast, &registry);
    println!();

    // Verify wildcard expansion
    println!("--- Wildcard Expansion Results ---");
    println!("Total connections: {}", expansion.connections.len());
    println!();

    // Check sensor[*] expansion
    let sensor_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.component.starts_with("sensor["))
        .collect();
    println!("sensor[*] expanded to {} connections:", sensor_connections.len());
    for conn in &sensor_connections {
        println!("  - {}.{} -> @{}", conn.component, conn.pin, conn.source_net);
    }
    println!();

    // Check led[*] expansion (should use wildcard matching)
    let led_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.component.starts_with("led"))
        .collect();
    println!("led[*] expanded to {} connections:", led_connections.len());
    for conn in &led_connections {
        println!("  - {}.{} -> @{}", conn.component, conn.pin, conn.source_net);
    }
    println!();

    // Check decoupling capacitors
    println!("Decoupling capacitors: {}", expansion.decoupling_caps.len());
    for cap in &expansion.decoupling_caps {
        if let Some(ref near) = cap.near_component {
            println!("  - {} = {} (near {})", cap.instance_name, cap.value, near);
        } else if cap.is_distributed {
            println!("  - {} = {} (distributed)", cap.instance_name, cap.value);
        }
    }
    println!();

    // Check for errors
    if !expansion.diagnostics.is_empty() {
        println!("--- Diagnostics ---");
        for diag in &expansion.diagnostics {
            println!("  {}", diag.message);
        }
        println!();
    }

    // Final verification
    println!("=== Test Results ===");
    let expected_sensor_connections = 8; // sensor[0]..sensor[7]
    let expected_led_connections = 3;    // led_0, led_1, led_2
    let expected_total = expected_sensor_connections + expected_led_connections;

    if expansion.connections.len() == expected_total {
        println!("✓ All {} connections expanded correctly", expected_total);
    } else {
        println!("✗ Expected {} connections, got {} (ERROR)",
            expected_total, expansion.connections.len());
    }

    if sensor_connections.len() == expected_sensor_connections {
        println!("✓ sensor[*] wildcard expanded to {} instances", expected_sensor_connections);
    } else {
        println!("✗ sensor[*] wildcard should expand to {} instances, got {} (ERROR)",
            expected_sensor_connections, sensor_connections.len());
    }

    if led_connections.len() == expected_led_connections {
        println!("✓ led[*] wildcard expanded to {} instances", expected_led_connections);
    } else {
        println!("✗ led[*] wildcard should expand to {} instances, got {} (ERROR)",
            expected_led_connections, led_connections.len());
    }

    if expansion.diagnostics.is_empty() {
        println!("✓ No errors during expansion");
    } else {
        println!("✗ {} diagnostics generated (may indicate errors)", expansion.diagnostics.len());
    }

    println!("\n=== Generate Block Wildcard Integration Test Complete ===");
}
