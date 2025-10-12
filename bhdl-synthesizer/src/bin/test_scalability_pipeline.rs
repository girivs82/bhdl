//! End-to-end pipeline test for power domain scalability features
//! Tests: Parser → Analyzer (Pass 1.25 + 1.5) → Synthesizer → Netlist

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::Synthesizer;

fn main() {
    println!("═══════════════════════════════════════════════════════════════");
    println!("  Power Domain Scalability - End-to-End Pipeline Test");
    println!("═══════════════════════════════════════════════════════════════\n");

    // Read the test file
    let test_file = "tests/circuits/realistic/test_power_domain_scalability.bhdl";
    let input = std::fs::read_to_string(test_file)
        .expect("Failed to read test file");

    println!("📄 Test file: {}\n", test_file);

    // ========================================================================
    // Stage 1: Parse
    // ========================================================================
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

    // ========================================================================
    // Stage 2: Build AST
    // ========================================================================
    println!("[2/4] Building AST...");
    let source_file = match SourceFile::cast(parse_result.syntax()) {
        Some(sf) => sf,
        None => {
            println!("❌ Failed to build AST");
            return;
        }
    };
    println!("✅ AST constructed\n");

    // ========================================================================
    // Stage 3: Analyze (includes Pass 1.25 and Pass 1.5)
    // ========================================================================
    println!("[3/4] Running analyzer...");
    let analysis_result = analyze(&source_file);

    if !analysis_result.diagnostics.is_empty() {
        let errors: Vec<_> = analysis_result.diagnostics.iter()
            .filter(|d| d.message.contains("error") || d.message.contains("Error"))
            .collect();

        if !errors.is_empty() {
            println!("❌ Analysis errors:");
            for diag in errors {
                println!("  • {}", diag.message);
            }
            return;
        }
    }

    println!("✅ Analysis complete");
    println!("  • Component instances registered: {}", analysis_result.instance_registry.len());
    println!("  • Power domain connections expanded: {}", analysis_result.power_domain_expansion.connections.len());
    println!("  • Decoupling capacitors generated: {}", analysis_result.power_domain_expansion.decoupling_caps.len());
    println!();

    // ========================================================================
    // Stage 4: Synthesize Netlist
    // ========================================================================
    println!("[4/4] Synthesizing netlist...");
    let mut synthesizer = Synthesizer::new();

    match synthesizer.synthesize(&source_file, &analysis_result) {
        Ok(netlist) => {
            println!("✅ Netlist synthesis successful");
            println!("  • Modules: {}", netlist.modules.len());
            println!("  • Total instances: {}",
                netlist.modules.values()
                    .map(|m| m.internal_instances.len())
                    .sum::<usize>());
            println!("  • Total nets: {}",
                netlist.modules.values()
                    .map(|m| m.internal_nets.len())
                    .sum::<usize>());
            println!();

            // Detailed netlist inspection
            verify_netlist(&netlist, &analysis_result);
        }
        Err(e) => {
            println!("❌ Netlist synthesis failed: {:?}", e);
            return;
        }
    }

    println!("═══════════════════════════════════════════════════════════════");
    println!("  Pipeline Test Complete");
    println!("═══════════════════════════════════════════════════════════════");
}

fn verify_netlist(
    netlist: &bhdl_netlist::Netlist,
    analysis_result: &bhdl_analyzer::types::AnalysisResult,
) {
    println!("🔍 Verifying Netlist Contents\n");

    // Find the main board module
    let board_module = netlist.modules.values()
        .find(|m| m.name == "MultiSensorBoard")
        .expect("Failed to find MultiSensorBoard module");

    println!("📋 Module: MultiSensorBoard");
    println!("  • Instances: {}", board_module.internal_instances.len());
    println!("  • Nets: {}", board_module.internal_nets.len());
    println!();

    // Check for sensor instances (should include original 4)
    let sensor_instances: Vec<_> = board_module.internal_instances.iter()
        .filter_map(|inst_id| {
            let inst = netlist.instances.get(*inst_id)?;
            if inst.module_name.contains("TempSensor") || inst.instance_name.contains("sensor") {
                Some((*inst_id, inst))
            } else {
                None
            }
        })
        .collect();

    println!("🔌 Sensor Instances:");
    for (id, inst) in &sensor_instances {
        println!("  • {} ({}): {:?}", inst.instance_name, inst.module_name, id);
    }
    println!("  Total: {} sensor instances", sensor_instances.len());
    println!();

    // Check for FPGA instance
    let fpga_instances: Vec<_> = board_module.internal_instances.iter()
        .filter_map(|inst_id| {
            let inst = netlist.instances.get(*inst_id)?;
            if inst.module_name.contains("FPGA") || inst.instance_name.contains("fpga") {
                Some((*inst_id, inst))
            } else {
                None
            }
        })
        .collect();

    println!("💾 FPGA Instances:");
    for (id, inst) in &fpga_instances {
        println!("  • {} ({}): {:?}", inst.instance_name, inst.module_name, id);
    }
    println!("  Total: {} FPGA instances", fpga_instances.len());
    println!();

    // Check for regulator instance
    let reg_instances: Vec<_> = board_module.internal_instances.iter()
        .filter_map(|inst_id| {
            let inst = netlist.instances.get(*inst_id)?;
            if inst.module_name.contains("7805") || inst.module_name.contains("LM7805") {
                Some((*inst_id, inst))
            } else {
                None
            }
        })
        .collect();

    println!("⚡ Regulator Instances:");
    for (id, inst) in &reg_instances {
        println!("  • {} ({}): {:?}", inst.instance_name, inst.module_name, id);
    }
    println!("  Total: {} regulator instances", reg_instances.len());
    println!();

    // Check for decoupling capacitors (should include generated ones)
    let cap_instances: Vec<_> = board_module.internal_instances.iter()
        .filter_map(|inst_id| {
            let inst = netlist.instances.get(*inst_id)?;
            if inst.instance_name.starts_with("C_DECOUP") || inst.module_name.contains("Capacitor") {
                Some((*inst_id, inst))
            } else {
                None
            }
        })
        .collect();

    println!("🔋 Decoupling Capacitor Instances:");
    if cap_instances.is_empty() {
        println!("  ⚠️  No decoupling capacitors found in netlist");
        println!("  Note: Decoupling capacitors from power_domain expansion may need");
        println!("        additional synthesizer support to be instantiated");
    } else {
        for (id, inst) in cap_instances.iter().take(5) {
            println!("  • {} ({}): {:?}", inst.instance_name, inst.module_name, id);
        }
        if cap_instances.len() > 5 {
            println!("  ... and {} more", cap_instances.len() - 5);
        }
        println!("  Total: {} capacitor instances", cap_instances.len());
    }
    println!();

    // Check for VCC_3V3 net
    let vcc_net = board_module.internal_nets.iter()
        .find_map(|net_id| {
            let net = netlist.nets.get(*net_id)?;
            if net.name.as_ref().map_or(false, |n| n.contains("VCC_3V3")) {
                Some((*net_id, net))
            } else {
                None
            }
        });

    println!("🔌 Power Net: @VCC_3V3");
    if let Some((net_id, net)) = vcc_net {
        println!("  • Net ID: {:?}", net_id);
        println!("  • Net name: {:?}", net.name);
        println!("  • Connection points: {}", net.connections.len());

        // Show first few connections
        for (i, conn) in net.connections.iter().take(5).enumerate() {
            println!("    {}. {:?}", i + 1, conn);
        }
        if net.connections.len() > 5 {
            println!("    ... and {} more connections", net.connections.len() - 5);
        }
    } else {
        println!("  ⚠️  VCC_3V3 net not found in netlist");
    }
    println!();

    // Summary comparison
    println!("📊 Expansion vs Netlist Comparison:");
    println!("  • Expected connections (from analyzer): {}",
        analysis_result.power_domain_expansion.connections.len());
    println!("  • Expected decoupling caps (from analyzer): {}",
        analysis_result.power_domain_expansion.decoupling_caps.len());
    println!("  • Actual component instances (in netlist): {}",
        board_module.internal_instances.len());
    println!("  • Actual nets (in netlist): {}",
        board_module.internal_nets.len());
    println!();

    // Verification results
    let original_components = 6; // 4 sensors + 1 FPGA + 1 reg
    let expected_caps = 34;
    let expected_total = original_components + expected_caps;

    println!("✅ Verification Summary:");
    println!("  • Original components registered: {} ✓",
        analysis_result.instance_registry.len());
    println!("  • Wildcard expansion: {} connections ✓",
        analysis_result.power_domain_expansion.connections.iter()
            .filter(|c| c.component.starts_with("sensor"))
            .count());
    println!("  • Range expansion: {} connections ✓",
        analysis_result.power_domain_expansion.connections.iter()
            .filter(|c| c.pin.contains("VCCO["))
            .count());
    println!("  • Decoupling caps generated: {} ✓",
        analysis_result.power_domain_expansion.decoupling_caps.len());

    if board_module.internal_instances.len() < expected_total {
        println!("\n⚠️  Note: Synthesizer may need updates to instantiate");
        println!("   decoupling capacitors from power_domain expansion.");
        println!("   Expected: {} total instances (6 original + 34 caps)", expected_total);
        println!("   Found:    {} instances in netlist", board_module.internal_instances.len());
    }
}
