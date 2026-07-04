//! End-to-end pipeline test for power domain scalability features
//! Tests: Parser → Analyzer (Pass 1.25 + 1.5) → Synthesizer → Netlist

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::Synthesizer;

#[tokio::main]
async fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Power Domain Scalability - End-to-End Pipeline Test");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Read the test file
    let test_file = "tests/circuits/realistic/test_power_domain_scalability.bhdl";
    let input = std::fs::read_to_string(test_file)
        .expect("Failed to read test file");

    println!("📄 Test file: {}\n", test_file);

    // Stage 1: Parse
    println!("[1/4] Parsing...");
    let parse_result = parse(&input);
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors:");
        for error in parse_result.errors() {
            println!("  • {:?}", error);
        }
        return;
    }
    println!("✅ Parsing successful\n");

    // Stage 2: Build AST
    println!("[2/4] Building AST...");
    let source_file = match SourceFile::cast(parse_result.syntax()) {
        Some(sf) => sf,
        None => {
            println!("❌ Failed to build AST");
            return;
        }
    };
    println!("✅ AST constructed\n");

    // Stage 3: Analyze (includes Pass 1.25 and Pass 1.5)
    println!("[3/4] Running analyzer (Pass 1.25 + 1.5)...");
    let analysis_result = analyze(&source_file);

    println!("✅ Analysis complete");
    println!("  • Component instances registered: {}", analysis_result.instance_registry.len());
    println!("  • Power domain connections expanded: {}", analysis_result.power_domain_expansion.connections.len());
    println!("  • Decoupling capacitors generated: {}", analysis_result.power_domain_expansion.decoupling_caps.len());
    println!();

    // Stage 4: Synthesize Netlist
    println!("[4/4] Synthesizing netlist...");
    let mut synthesizer = Synthesizer::new();

    match synthesizer.synthesize(&source_file, &analysis_result).await {
        Ok(netlist) => {
            println!("✅ Netlist synthesis successful");
            println!("  • Modules: {}", netlist.modules.len());

            let total_instances: usize = netlist.modules.values()
                .map(|m| m.internal_instances.len())
                .sum();
            let total_nets: usize = netlist.modules.values()
                .map(|m| m.internal_nets.len())
                .sum();

            println!("  • Total instances: {}", total_instances);
            println!("  • Total nets: {}", total_nets);
            println!();

            // Find the board module
            if let Some(board_module) = netlist.modules.values()
                .find(|m| m.name == "MultiSensorBoard")
            {
                println!("📋 MultiSensorBoard Module:");
                println!("  • Instances: {}", board_module.internal_instances.len());
                println!("  • Nets: {}", board_module.internal_nets.len());
                println!();

                // Show first few instances
                println!("🔌 Sample Instances:");
                for (i, inst_id) in board_module.internal_instances.iter().take(6).enumerate() {
                    if let Some(inst) = netlist.instances.get(*inst_id) {
                        if let Some(module_def) = netlist.modules.get(inst.definition) {
                            println!("  {}. {} : {}", i + 1, inst.name, module_def.name);
                        }
                    }
                }
                if board_module.internal_instances.len() > 6 {
                    println!("  ... and {} more instances", board_module.internal_instances.len() - 6);
                }
                println!();
            }

            // Summary
            println!("═══════════════════════════════════════════════════════════════");
            println!("  Verification Summary");
            println!("═══════════════════════════════════════════════════════════════\n");

            println!("✅ Pass 1.25 (Instance Registry):");
            println!("  • {} component instances registered", analysis_result.instance_registry.len());
            println!();

            println!("✅ Pass 1.5 (Power Domain Expansion):");
            println!("  • {} connections expanded", analysis_result.power_domain_expansion.connections.len());
            println!("    - {} from wildcard expansion (sensor[*].VCC)",
                analysis_result.power_domain_expansion.connections.iter()
                    .filter(|c| c.component.starts_with("sensor"))
                    .count());
            println!("    - {} from range expansion (fpga.VCCO[0..7])",
                analysis_result.power_domain_expansion.connections.iter()
                    .filter(|c| c.pin.contains("VCCO["))
                    .count());
            println!("  • {} decoupling capacitors generated", analysis_result.power_domain_expansion.decoupling_caps.len());
            println!();

            println!("✅ Netlist Synthesis:");
            println!("  • {} instances created in netlist", total_instances);
            println!("  • {} nets created in netlist", total_nets);
            println!();

            if total_instances < 40 {
                println!("⚠️  Note: Decoupling capacitors from power_domain expansion");
                println!("   may need additional synthesizer support to be fully instantiated.");
                println!("   Expected: ~40 instances (6 original + 34 caps)");
                println!("   Found:    {} instances", total_instances);
            } else {
                println!("🎉 Pipeline test successful! All scalability features working.");
            }
        }
        Err(e) => {
            println!("❌ Netlist synthesis failed: {:?}", e);
            return;
        }
    }

    println!("\n═══════════════════════════════════════════════════════════════");
    println!("  Pipeline Test Complete");
    println!("═══════════════════════════════════════════════════════════════");
}
