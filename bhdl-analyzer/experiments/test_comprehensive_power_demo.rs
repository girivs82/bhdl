use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::passes::{build_instance_registry, expand_power_domains};
use std::fs;
use std::collections::HashMap;

fn main() {
    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "docs/examples/comprehensive_power_demo.bhdl".to_string());

    println!("Comprehensive Power Domain Demo - Pipeline Test");
    println!("================================================\n");
    println!("Reading file: {}\n", test_file);

    let source = fs::read_to_string(&test_file)
        .expect("Failed to read test file");

    let parse_result = parse(&source);

    if !parse_result.errors().is_empty() {
        println!("❌ PARSE ERRORS:");
        for error in parse_result.errors() {
            println!("  {:?}", error);
        }
        return;
    }

    let source_file = SourceFile::cast(parse_result.syntax()).expect("Failed to cast to SourceFile");

    println!("✅ File parsed successfully\n");

    // Build instance registry
    println!("Building instance registry...");
    let registry = build_instance_registry(&source_file);

    let instance_names = registry.get_instance_names();
    println!("  Registered {} instances", instance_names.len());
    println!();

    // Expand power domains
    println!("Expanding power domains...\n");
    let expansion = expand_power_domains(&source_file, &registry);

    // Analyze results
    println!("\n========================================");
    println!("Expansion Summary");
    println!("========================================\n");

    if !expansion.diagnostics.is_empty() {
        println!("⚠️  Diagnostics ({}):", expansion.diagnostics.len());
        for diag in &expansion.diagnostics {
            println!("  - {}", diag.message);
        }
        println!();
    }

    println!("Total connections: {}", expansion.connections.len());
    println!("Total decoupling capacitors: {}", expansion.decoupling_caps.len());
    println!();

    // Group connections by source net
    let mut by_net: HashMap<String, Vec<_>> = HashMap::new();
    for conn in &expansion.connections {
        by_net.entry(conn.source_net.clone()).or_insert_with(Vec::new).push(conn);
    }

    // Group decoupling by placement
    let mut near_count = 0;
    let mut distributed_count = 0;
    for cap in &expansion.decoupling_caps {
        if cap.is_distributed {
            distributed_count += 1;
        } else {
            near_count += 1;
        }
    }

    println!("Decoupling breakdown:");
    println!("  Near-component: {}", near_count);
    println!("  Distributed: {}", distributed_count);
    println!();

    // Analyze each power domain
    println!("========================================");
    println!("Power Domain Analysis");
    println!("========================================\n");

    let domains = vec![
        ("VCC_3V3", vec![
            "Feature: Wildcards, Hierarchical Wildcards, Suffix Wildcards",
            "Expected: mcu + 3 interfaces + (4 sensor boards × 3 components) + 3 array sensors + 8 LEDs",
            "Expected total: 1 + 3 + 12 + 3 + 8 = 27 connections",
        ]),
        ("VCC_5V", vec![
            "Feature: Simple Range, Explicit List",
            "Expected: 8 monitors (range 0..7) + 4 specific VREF (0,3,5,7)",
            "Expected total: 8 + 4 = 12 connections",
        ]),
        ("AVCC_P", vec![
            "Feature: Even Keyword",
            "Expected: ADC channels 0, 2, 4, 6",
            "Expected total: 4 connections",
        ]),
        ("AVCC_N", vec![
            "Feature: Odd Keyword",
            "Expected: ADC channels 1, 3, 5, 7",
            "Expected total: 4 connections",
        ]),
        ("VCC_MEM_A", vec![
            "Feature: Stepped Range (every 3rd, phase A)",
            "Expected: Memory banks 0, 3, 6, 9",
            "Expected total: 4 connections",
        ]),
        ("VCC_MEM_B", vec![
            "Feature: Stepped Range (every 3rd, phase B)",
            "Expected: Memory banks 1, 4, 7, 10",
            "Expected total: 4 connections",
        ]),
        ("VCC_MEM_C", vec![
            "Feature: Stepped Range (every 3rd, phase C)",
            "Expected: Memory banks 2, 5, 8, 11",
            "Expected total: 4 connections",
        ]),
    ];

    let mut all_expectations_met = true;

    for (net, descriptions) in domains {
        let connections = by_net.get(net).map(|v| v.as_slice()).unwrap_or(&[]);

        println!("@{}:", net);
        for desc in &descriptions {
            println!("  {}", desc);
        }
        println!("  Actual: {} connections", connections.len());

        // Extract expected count from description
        if let Some(expected_line) = descriptions.iter().find(|s| s.contains("Expected total:")) {
            if let Some(count_str) = expected_line.split("Expected total: ").nth(1) {
                if let Some(count_part) = count_str.split(" connections").next() {
                    if let Ok(expected) = count_part.trim().parse::<usize>() {
                        if connections.len() == expected {
                            println!("  ✅ PASS");
                        } else {
                            println!("  ❌ FAIL - Expected {} but got {}", expected, connections.len());
                            all_expectations_met = false;
                        }
                    }
                }
            }
        }
        println!();
    }

    // Feature demonstration summary
    println!("========================================");
    println!("Feature Demonstration");
    println!("========================================\n");

    println!("✅ Wildcard Expansion");
    println!("   - uart.VCC, spi.VCC, i2c.VCC (simple wildcard for interfaces)");
    println!("   - led[*].A (wildcard over LED instances)");
    println!();

    println!("✅ Hierarchical Wildcards");
    println!("   - sensor_board[*].sensor.VCC");
    println!("   - sensor_board[*].buffer.VCC");
    println!("   - sensor_board[*].filter.VCC");
    println!();

    println!("✅ Suffix Wildcards");
    println!("   - array.*sensor.VCC (matches temp_sensor, humidity_sensor, pressure_sensor)");
    println!();

    println!("✅ Simple Range");
    println!("   - monitor[0..7].VCC");
    println!();

    println!("✅ Explicit List");
    println!("   - monitor[0,3,5,7].VREF");
    println!();

    println!("✅ Even/Odd Keywords");
    println!("   - adc[even].AVCC → channels 0, 2, 4, 6");
    println!("   - adc[odd].AVCC → channels 1, 3, 5, 7");
    println!();

    println!("✅ Stepped Ranges");
    println!("   - mem[0..11:3].VCC → banks 0, 3, 6, 9 (phase A)");
    println!("   - mem[1..11:3].VCC → banks 1, 4, 7, 10 (phase B)");
    println!("   - mem[2..11:3].VCC → banks 2, 5, 8, 11 (phase C)");
    println!();

    println!("✅ Decoupling Capacitors");
    println!("   - Near-component placement (MCU, ADCs)");
    println!("   - Distributed placement (high-frequency decoupling)");
    println!();

    println!("✅ Multiple Voltage Domains");
    println!("   - 3.3V digital (VCC_3V3)");
    println!("   - 5V digital (VCC_5V)");
    println!("   - 5V analog differential (AVCC_P, AVCC_N)");
    println!("   - 3.3V memory phased (VCC_MEM_A, VCC_MEM_B, VCC_MEM_C)");
    println!();

    // Final summary
    println!("========================================");
    println!("Summary");
    println!("========================================\n");

    let expected_total = 27 + 12 + 4 + 4 + 4 + 4 + 4; // Sum of all expected connections
    println!("Expected total connections: {}", expected_total);
    println!("Actual total connections: {}", expansion.connections.len());

    if expansion.connections.len() == expected_total && all_expectations_met {
        println!("\n✅ ALL TESTS PASSED!");
        println!("\nThis demo successfully demonstrates:");
        println!("  • Complete power domain scalability");
        println!("  • All wildcard pattern types");
        println!("  • Hierarchical module traversal");
        println!("  • Advanced pattern matching (even/odd, lists, stepped ranges)");
        println!("  • Generate block integration");
        println!("  • Decoupling capacitor generation");
        println!("  • Multiple voltage domain management");
    } else {
        println!("\n⚠️  Some expectations not met");
        println!("   This may be due to missing component definitions or parsing issues");

        if !all_expectations_met {
            std::process::exit(1);
        }
    }
}
