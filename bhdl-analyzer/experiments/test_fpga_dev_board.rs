use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::passes::{build_instance_registry, expand_power_domains};
use std::fs;

fn main() {
    println!("=============================================================================");
    println!("FPGA Development Board Power Domain Test");
    println!("=============================================================================\n");

    let test_file = std::env::args().nth(1)
        .unwrap_or_else(|| "docs/examples/04_fpga_dev_board.bhdl".to_string());

    let source = fs::read_to_string(&test_file)
        .expect("Failed to read test file");

    let parse_result = parse(&source);

    if !parse_result.errors().is_empty() {
        println!("❌ PARSE ERRORS:");
        for error in parse_result.errors() {
            println!("  {:?}", error);
        }
        std::process::exit(1);
    }

    let source_file = SourceFile::cast(parse_result.syntax())
        .expect("Failed to cast to SourceFile");

    println!("✅ File parsed successfully\n");

    // Build instance registry
    println!("Building instance registry...");
    let registry = build_instance_registry(&source_file);
    println!("  Registered {} instances\n", registry.get_instance_names().len());

    // Expand power domains
    println!("Expanding power domains...\n");
    let expansion = expand_power_domains(&source_file, &registry);

    if !expansion.diagnostics.is_empty() {
        println!("⚠️  Diagnostics:");
        for diag in &expansion.diagnostics {
            println!("  - {}", diag.message);
        }
        println!();
    }

    println!("=============================================================================");
    println!("Power Domain Expansion Results");
    println!("=============================================================================\n");

    // Expected connection counts per domain
    let expected = vec![
        ("VCCINT", 13),       // 1 reg + 12 FPGA pins
        ("VCCAUX", 7),        // 1 reg + 4 FPGA pins + 1 flash + 1 DDR ctrl
        ("VCCO_0", 18),       // 1 reg + 16 FPGA pins + 1 DDR ctrl
        ("VCCO_1", 28),       // 1 reg + 16 FPGA pins + 3 peripherals + 8 LEDs
        ("VCC_CLOCK", 1),     // 1 oscillator
    ];

    // Group connections by source net
    let mut by_net: std::collections::HashMap<String, Vec<_>> = std::collections::HashMap::new();
    for conn in &expansion.connections {
        by_net.entry(conn.source_net.clone()).or_insert_with(Vec::new).push(conn);
    }

    let mut all_passed = true;

    for (domain_name, expected_count) in &expected {
        let connections = by_net.get(*domain_name).map(|v| v.as_slice()).unwrap_or(&[]);
        let actual_count = connections.len();

        let status = if actual_count == *expected_count {
            "✅"
        } else {
            all_passed = false;
            "❌"
        };

        println!("{} Power Domain: @{}", status, domain_name);
        println!("   Expected: {} connections", expected_count);
        println!("   Actual:   {} connections", actual_count);

        if actual_count != *expected_count {
            println!("   Connections:");
            for conn in connections {
                println!("     - @{} -> {}.{}", domain_name, conn.component, conn.pin);
            }
        }
        println!();
    }

    println!("=============================================================================");
    println!("Summary");
    println!("=============================================================================\n");
    println!("Total connections: {}", expansion.connections.len());
    println!("Total decoupling capacitors: {}", expansion.decoupling_caps.len());
    println!("Power domains: {}", by_net.len());

    if all_passed {
        println!("\n✅ ALL TESTS PASSED!");
        println!("\nThis FPGA development board demonstrates:");
        println!("  • Multi-voltage domain design (1.0V, 1.8V, 2.5V, 3.3V)");
        println!("  • Range patterns for FPGA power pins");
        println!("  • Sophisticated decoupling strategies");
        println!("  • Real-world power architecture");
        println!("  • Bank-based I/O voltage organization");
        println!("\nPower Distribution:");
        println!("  VCCINT:    13 connections (FPGA core logic @ 1.0V/30A)");
        println!("  VCCAUX:     7 connections (PLLs and analog @ 1.8V/3A)");
        println!("  VCCO_0:    18 connections (I/O banks 0-1 @ 2.5V/2A)");
        println!("  VCCO_1:    28 connections (I/O banks 2-3 @ 3.3V/2A)");
        println!("  VCC_CLOCK:  1 connection  (Oscillator @ 3.3V/100mA)");
        std::process::exit(0);
    } else {
        println!("\n❌ SOME TESTS FAILED");
        std::process::exit(1);
    }
}
