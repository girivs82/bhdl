// Comprehensive test for all power domain scalability features
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Power Domain Scalability - Comprehensive Test");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Read the test file
    let test_file = "tests/circuits/realistic/test_power_domain_scalability.bhdl";
    let input = std::fs::read_to_string(test_file)
        .expect("Failed to read test file");

    println!("📄 Test file: {}\n", test_file);

    // Parse
    println!("[1/3] Parsing...");
    let parse_result = parse(&input);
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors:");
        for error in parse_result.errors() {
            println!("  • {:?}", error);
        }
        return;
    }
    println!("✅ Parsing successful\n");

    // Build AST
    println!("[2/3] Building AST...");
    let source_file = match SourceFile::cast(parse_result.syntax()) {
        Some(sf) => sf,
        None => {
            println!("❌ Failed to build AST");
            return;
        }
    };
    println!("✅ AST constructed\n");

    // Run analyzer
    println!("[3/3] Running analyzer...");
    let analysis_result = analyze(&source_file);

    println!("✅ Analysis complete\n");
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Analysis Results");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Report instance registry results
    println!("📋 Component Instance Registry (Pass 1.25):");
    println!("  • Total instances registered: {}", analysis_result.instance_registry.len());
    for instance_name in analysis_result.instance_registry.get_instance_names() {
        if let Some(info) = analysis_result.instance_registry.get_instance(instance_name) {
            println!("    - {} : {}", instance_name, info.component_type);
        }
    }
    println!();

    // Report power domain expansion results
    let expansion = &analysis_result.power_domain_expansion;

    println!("🔌 Power Domain Expansion (Pass 1.5):");
    println!("  • Total connections: {}", expansion.connections.len());
    println!("  • Decoupling capacitors: {}", expansion.decoupling_caps.len());
    println!("  • Diagnostics: {}", expansion.diagnostics.len());
    println!();

    // Show expanded connections by type
    println!("📊 Connection Breakdown:");

    // Count wildcard expansions (sensor instances)
    let sensor_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.component.starts_with("sensor"))
        .collect();
    println!("  • Wildcard expansions (sensors[*]): {}", sensor_connections.len());
    for conn in &sensor_connections {
        println!("    → {}.{}", conn.component, conn.pin);
    }
    println!();

    // Count range expansions (FPGA VCCO pins)
    let vcco_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| c.pin.contains("VCCO["))
        .collect();
    println!("  • Range expansions (VCCO[0..7]): {}", vcco_connections.len());
    for conn in &vcco_connections {
        println!("    → {}.{}", conn.component, conn.pin);
    }
    println!();

    // Count simple references
    let simple_connections: Vec<_> = expansion.connections.iter()
        .filter(|c| !c.component.starts_with("sensor") && !c.pin.contains("VCCO["))
        .collect();
    println!("  • Simple references: {}", simple_connections.len());
    for conn in &simple_connections {
        println!("    → {}.{}", conn.component, conn.pin);
    }
    println!();

    // Show decoupling capacitors by placement
    println!("⚡ Decoupling Capacitor Breakdown:");

    let near_caps: Vec<_> = expansion.decoupling_caps.iter()
        .filter(|c| !c.is_distributed)
        .collect();
    println!("  • Near-component placement: {}", near_caps.len());
    for cap in &near_caps {
        if let Some(ref comp) = cap.near_component {
            println!("    {} = {} (near {})", cap.instance_name, cap.value, comp);
        }
    }
    println!();

    let distributed_caps: Vec<_> = expansion.decoupling_caps.iter()
        .filter(|c| c.is_distributed)
        .collect();
    println!("  • Distributed placement: {}", distributed_caps.len());
    for cap in &distributed_caps {
        println!("    {} = {}", cap.instance_name, cap.value);
    }
    println!();

    // Summary
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Summary");
    println!("═══════════════════════════════════════════════════════════════\n");

    let expected_sensor_connections = 4; // sensor_0, sensor_1, sensor_2, sensor_3
    let expected_vcco_connections = 8;   // VCCO[0..7]
    let expected_simple_connections = 1;  // VCCAUX

    let sensor_pass = sensor_connections.len() == expected_sensor_connections;
    let vcco_pass = vcco_connections.len() == expected_vcco_connections;
    let simple_pass = simple_connections.len() == expected_simple_connections;
    let caps_generated = expansion.decoupling_caps.len() > 0;

    println!("Feature Test Results:");
    println!("  {} Wildcard expansion: {} sensors found (expected {})",
             if sensor_pass { "✅" } else { "❌" },
             sensor_connections.len(), expected_sensor_connections);
    println!("  {} Range expansion: {} VCCO pins found (expected {})",
             if vcco_pass { "✅" } else { "❌" },
             vcco_connections.len(), expected_vcco_connections);
    println!("  {} Simple references: {} connections found (expected {})",
             if simple_pass { "✅" } else { "❌" },
             simple_connections.len(), expected_simple_connections);
    println!("  {} Decoupling generation: {} capacitors generated",
             if caps_generated { "✅" } else { "❌" },
             expansion.decoupling_caps.len());
    println!();

    if sensor_pass && vcco_pass && simple_pass && caps_generated {
        println!("🎉 All scalability features working correctly!");
    } else {
        println!("⚠️  Some features need attention");
    }
}
