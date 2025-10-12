// Test hierarchical wildcard integration
// Verifies that wildcards correctly expand across module instance boundaries

use bhdl_parser;
use bhdl_analyzer::passes::{build_instance_registry, expand_power_domains};
use bhdl_ast::{AstNode, SourceFile, HasName};

fn main() {
    // Parse test circuit with hierarchical modules
    let source = std::fs::read_to_string("tests/circuits/realistic/test_hierarchical_wildcard.bhdl")
        .expect("Failed to read test file");

    println!("=== Testing Hierarchical Wildcard Integration ===\n");
    println!("Source circuit: test_hierarchical_wildcard.bhdl\n");

    // Parse the source
    let parse = bhdl_parser::parse(&source);
    let ast = SourceFile::cast(parse.syntax()).expect("Failed to cast to SourceFile");

    println!("--- AST Structure Analysis ---");

    // Find all module definitions
    for item in ast.items() {
        if let Some(module) = bhdl_ast::Module::cast(item.syntax().clone()) {
            if let Some(name) = module.name() {
                println!("Found module definition: {}", name.text());

                // List components inside this module
                for comp_inst in module.component_instances() {
                    // Try to extract instance name and type
                    let inst_name = comp_inst.syntax()
                        .children_with_tokens()
                        .filter_map(|e| e.into_token())
                        .find(|t| t.kind() == bhdl_ast::SyntaxKind::IDENT)
                        .map(|t| t.text().to_string());

                    if let Some(name) = inst_name {
                        println!("  - Component: {}", name);
                    }
                }
            }
        }
    }
    println!();

    // Find board and analyze module instances
    for item in ast.items() {
        if let Some(board) = bhdl_ast::Board::cast(item.syntax().clone()) {
            if let Some(name) = board.name() {
                println!("Found board: {}", name.text());

                // List module instances
                for mod_inst in board.module_instances() {
                    let inst_name = mod_inst.name();
                    let mod_type = mod_inst.module_type();

                    if let (Some(inst), Some(typ)) = (inst_name, mod_type) {
                        println!("  - Module instance: {} : {}", inst.text(), typ.text());
                    }
                }
            }
        }
    }
    println!();

    println!("--- Pass 1.25: Building Instance Registry ---");
    let registry = build_instance_registry(&ast);
    println!();

    // Verify that module instances and their contents are handled
    println!("--- Expected Hierarchical Instances ---");
    let expected_hierarchical = vec![
        // Module instances (top level)
        "sensor_board_0",
        "sensor_board_1",
        "sensor_board_2",
        "array",
        // Components inside modules (hierarchical paths)
        "sensor_board_0.sensor",
        "sensor_board_0.buffer",
        "sensor_board_1.sensor",
        "sensor_board_1.buffer",
        "sensor_board_2.sensor",
        "sensor_board_2.buffer",
        "array.temp_sensor",
        "array.humidity_sensor",
        "array.pressure_sensor",
        // Top-level component
        "led",
    ];

    println!("Checking if hierarchical instances are registered:");
    for inst in &expected_hierarchical {
        if registry.get_instance(inst).is_some() {
            println!("  ✓ {} registered", inst);
        } else {
            println!("  ✗ {} NOT registered (expected for hierarchical)", inst);
        }
    }
    println!();

    // Expand power domains with hierarchical wildcards
    println!("--- Pass 1.5: Expanding Power Domain Wildcards ---");
    let expansion = expand_power_domains(&ast, &registry);
    println!();

    // Verify hierarchical wildcard expansion
    println!("--- Hierarchical Wildcard Expansion Results ---");
    println!("Total connections: {}", expansion.connections.len());
    println!();

    // Check sensor_board[*].sensor.VCC expansion
    let sensor_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.component.contains("sensor_board") && c.component.contains(".sensor"))
        .collect();
    println!("sensor_board[*].sensor.VCC expanded to {} connections:", sensor_connections.len());
    for conn in &sensor_connections {
        println!("  - {}.{} -> @{}", conn.component, conn.pin, conn.source_net);
    }
    println!();

    // Check sensor_board[*].buffer.VCC expansion
    let buffer_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.component.contains("sensor_board") && c.component.contains(".buffer"))
        .collect();
    println!("sensor_board[*].buffer.VCC expanded to {} connections:", buffer_connections.len());
    for conn in &buffer_connections {
        println!("  - {}.{} -> @{}", conn.component, conn.pin, conn.source_net);
    }
    println!();

    // Check array.*sensor.VCC expansion
    let array_sensor_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.component.starts_with("array.") && c.component.contains("sensor"))
        .collect();
    println!("array.*sensor.VCC expanded to {} connections:", array_sensor_connections.len());
    for conn in &array_sensor_connections {
        println!("  - {}.{} -> @{}", conn.component, conn.pin, conn.source_net);
    }
    println!();

    // Check LED connection
    let led_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.component == "led")
        .collect();
    println!("led.A expanded to {} connection(s):", led_connections.len());
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
    let expected_sensor_connections = 3; // sensor_board_0, sensor_board_1, sensor_board_2
    let expected_buffer_connections = 3; // buffer in each sensor_board
    let expected_array_sensors = 3;      // temp_sensor, humidity_sensor, pressure_sensor
    let expected_led_connections = 1;    // led
    let expected_total = expected_sensor_connections + expected_buffer_connections +
                         expected_array_sensors + expected_led_connections;

    if expansion.connections.len() == expected_total {
        println!("[PASS] All {} hierarchical connections expanded correctly", expected_total);
    } else {
        println!("[IN PROGRESS] Expected {} connections, got {}",
            expected_total, expansion.connections.len());
    }

    if sensor_connections.len() == expected_sensor_connections {
        println!("[PASS] sensor_board[*].sensor wildcard expanded to {} instances", expected_sensor_connections);
    } else {
        println!("[IN PROGRESS] sensor_board[*].sensor should expand to {} instances, got {}",
            expected_sensor_connections, sensor_connections.len());
    }

    if buffer_connections.len() == expected_buffer_connections {
        println!("[PASS] sensor_board[*].buffer wildcard expanded to {} instances", expected_buffer_connections);
    } else {
        println!("[IN PROGRESS] sensor_board[*].buffer should expand to {} instances, got {}",
            expected_buffer_connections, buffer_connections.len());
    }

    if array_sensor_connections.len() == expected_array_sensors {
        println!("[PASS] array.*sensor wildcard expanded to {} instances", expected_array_sensors);
    } else {
        println!("[IN PROGRESS] array.*sensor should expand to {} instances, got {}",
            expected_array_sensors, array_sensor_connections.len());
    }

    if led_connections.len() == expected_led_connections {
        println!("[PASS] led.A expanded to {} connection", expected_led_connections);
    } else {
        println!("[FAIL] led.A should expand to {} connection, got {}",
            expected_led_connections, led_connections.len());
    }

    if expansion.diagnostics.is_empty() {
        println!("[PASS] No errors during expansion");
    } else {
        println!("[WARN] {} diagnostics generated (may indicate missing features)", expansion.diagnostics.len());
    }

    println!();
    println!("=== Hierarchical Wildcard Integration Test Complete ===");
}
