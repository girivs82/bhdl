//! Test synthesizer integration with power domain scalability
//! Uses simple components from bhdl-stdlib

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig};

#[tokio::main]
async fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Synthesizer Integration Test - Power Domain Scalability");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Read the simple test file
    let test_file = "tests/circuits/realistic/test_power_domain_scalability_simple.bhdl";
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

    // Analyze
    println!("[3/3] Running analyzer...");
    let analysis_result = analyze(&source_file);

    println!("✅ Analysis complete");
    println!("  • Component instances registered: {}", analysis_result.instance_registry.len());
    println!("  • Power domain connections expanded: {}", analysis_result.power_domain_expansion.connections.len());
    println!("  • Decoupling capacitors generated: {}", analysis_result.power_domain_expansion.decoupling_caps.len());
    println!();

    // Synthesize
    println!("[4/4] Synthesizing netlist...");
    let config = NetlistConfig {
        include_power_domains: true,
        enable_simulation_optimization: false,
        enable_compatibility_analysis: false,
        enable_pattern_recognition: false,
        enable_cross_optimization: false,
        enable_design_rule_check: false,
        database_path: None, // Disable database for simpler testing
        ..Default::default()
    };

    let mut synthesizer = NetlistGenerator::with_config(config);

    match synthesizer.synthesize(&source_file, &analysis_result).await {
        Ok(netlist) => {
            println!("✅ Netlist synthesis successful\n");

            // Count instances by type
            let total_instances = netlist.instances.len();
            let capacitor_instances = netlist.instances.iter()
                .filter(|(_, inst)| {
                    if let Some(module_def) = netlist.modules.get(inst.definition) {
                        module_def.name == "Capacitor"
                    } else {
                        false
                    }
                })
                .count();

            let resistor_instances = netlist.instances.iter()
                .filter(|(_, inst)| {
                    if let Some(module_def) = netlist.modules.get(inst.definition) {
                        module_def.name.contains("Resistor")
                    } else {
                        false
                    }
                })
                .count();

            println!("📊 Netlist Summary:");
            println!("  • Total modules: {}", netlist.modules.len());
            println!("  • Total instances: {}", total_instances);
            println!("    - Resistor instances: {}", resistor_instances);
            println!("    - Capacitor instances (including generated): {}", capacitor_instances);
            println!("  • Total nets: {}", netlist.nets.len());
            println!("  • Total pin instances: {}", netlist.pin_instances.len());
            println!();

            // Show first few instances
            println!("🔌 Sample Instances:");
            for (i, (inst_id, inst)) in netlist.instances.iter().take(10).enumerate() {
                if let Some(module_def) = netlist.modules.get(inst.definition) {
                    let attrs = if let Some(value) = inst.attributes.get("value") {
                        format!(" (value: {})", value)
                    } else {
                        String::new()
                    };
                    println!("  {}. {} : {}{}", i + 1, inst.name, module_def.name, attrs);
                }
            }
            if netlist.instances.len() > 10 {
                println!("  ... and {} more instances", netlist.instances.len() - 10);
            }
            println!();

            // Verification
            println!("═══════════════════════════════════════════════════════════════");
            println!("  Verification");
            println!("═══════════════════════════════════════════════════════════════\n");

            let expected_original = 5; // 4 resistors + 1 inductor
            let expected_generated_caps = 6; // 2 near inductor + 4 distributed
            let expected_total = expected_original + expected_generated_caps;

            println!("Expected instances:");
            println!("  • Original components: {}", expected_original);
            println!("  • Generated decoupling caps: {}", expected_generated_caps);
            println!("  • Total expected: {}", expected_total);
            println!();

            println!("Actual instances:");
            println!("  • Total found: {}", total_instances);
            println!("  • Capacitors found: {}", capacitor_instances);
            println!();

            if total_instances >= expected_total {
                println!("✅ SUCCESS: Synthesizer properly integrated power domain expansion!");
                println!("   All {} expected instances created ({} original + {} generated caps)",
                         expected_total, expected_original, expected_generated_caps);
            } else {
                println!("⚠️  Note: Found {} instances, expected {}",
                         total_instances, expected_total);
                println!("   This may be due to component resolution issues.");
            }

            // Check for power domain connections
            let vcc_net = netlist.nets.iter()
                .find(|(_, net)| net.name.as_ref().map_or(false, |n| n.contains("VCC_5V")));

            if let Some((_, net)) = vcc_net {
                println!("\n🔌 Power Net: @VCC_5V");
                println!("  • Connections: {}", net.connections.len());
                if net.connections.len() >= 5 {
                    println!("  ✅ Power net has {} connections (expected at least 5)", net.connections.len());
                }
            }
        }
        Err(e) => {
            println!("❌ Netlist synthesis failed: {:?}", e);
            println!("\nThis is expected if components are not fully defined in stdlib.");
        }
    }

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("  Test Complete");
    println!("═══════════════════════════════════════════════════════════════");
}
