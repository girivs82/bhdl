//! Intent System Demonstration
//!
//! This demo showcases the complete BHDL Intent System with all 38 implemented
//! intent functions. It parses a realistic circuit that uses intents from all
//! categories and displays comprehensive analysis.

use bhdl_parser::parse;
use bhdl_analyzer::Analyzer;
use bhdl_stdlib::StandardLibrary;
use std::fs;
use std::collections::HashMap;

fn main() {
    println!("\n╔═══════════════════════════════════════════════════════════════════╗");
    println!("║     BHDL INTENT SYSTEM - COMPREHENSIVE DEMONSTRATION              ║");
    println!("╚═══════════════════════════════════════════════════════════════════╝\n");

    // Read the demo circuit
    let demo_file = "tests/circuits/realistic/intent_system_demo.bhdl";
    let source = fs::read_to_string(demo_file)
        .expect("Failed to read demo circuit");

    println!("📄 Parsing: {}", demo_file);
    println!("   {} lines of BHDL code\n", source.lines().count());

    // Parse the circuit
    let parse_result = parse(&source);
    if !parse_result.errors.is_empty() {
        println!("⚠️  Parse errors:");
        for error in &parse_result.errors {
            println!("   {}", error);
        }
        return;
    }
    println!("✅ Parsing successful\n");

    // Analyze the circuit
    println!("🔍 Analyzing circuit with Intent System...\n");

    let stdlib = StandardLibrary::new();
    let mut analyzer = Analyzer::new(&parse_result.syntax_tree, &source, Some(&stdlib));

    let analysis_result = analyzer.analyze();

    // Display diagnostics if any
    if !analysis_result.diagnostics.is_empty() {
        println!("📋 Analysis diagnostics: {}", analysis_result.diagnostics.len());
        for (i, diag) in analysis_result.diagnostics.iter().take(5).enumerate() {
            println!("   {}. {:?}: {}", i + 1, diag.severity, diag.message);
        }
        if analysis_result.diagnostics.len() > 5 {
            println!("   ... and {} more", analysis_result.diagnostics.len() - 5);
        }
        println!();
    }

    // Analyze flow tracker if available
    if let Some(flow_tracker) = &analysis_result.flow_tracker {
        println!("═══════════════════════════════════════════════════════════════════");
        println!("  INTENT SYSTEM ANALYSIS RESULTS");
        println!("═══════════════════════════════════════════════════════════════════\n");

        let flows = flow_tracker.get_all_flows();
        println!("📊 Total signal flows with intents: {}\n", flows.len());

        // Group intents by category
        let mut category_counts: HashMap<&str, Vec<String>> = HashMap::new();
        let mut sim_mode_counts: HashMap<String, usize> = HashMap::new();
        let mut total_hints = 0;
        let mut total_validations = 0;

        for flow in flows {
            if let Some(intent_result) = &flow.intent_result {
                // Count by simulation mode
                let mode_str = format!("{:?}", intent_result.sim_mode);
                *sim_mode_counts.entry(mode_str).or_insert(0) += 1;

                // Count hints and validations
                total_hints += intent_result.synthesis_hints.len();
                total_validations += intent_result.validation_rules.len();

                // Categorize intent
                if let Some(intent_call) = &flow.intent {
                    let intent_name = &intent_call.name;
                    let category = categorize_intent(intent_name);
                    category_counts.entry(category)
                        .or_insert_with(Vec::new)
                        .push(intent_name.clone());
                }
            }
        }

        // Display category breakdown
        println!("┌─────────────────────────────────────────────────────────────────┐");
        println!("│ INTENT CATEGORIES USED                                          │");
        println!("└─────────────────────────────────────────────────────────────────┘\n");

        let mut categories: Vec<_> = category_counts.iter().collect();
        categories.sort_by_key(|(name, _)| *name);

        for (category, intents) in categories {
            println!("  {} {} ({} intents)",
                get_category_emoji(category),
                category,
                intents.len()
            );

            // Show unique intents in this category
            let mut unique_intents: Vec<_> = intents.iter().collect();
            unique_intents.sort();
            unique_intents.dedup();

            for intent in unique_intents {
                println!("     • {}", intent);
            }
            println!();
        }

        // Display simulation mode distribution
        println!("┌─────────────────────────────────────────────────────────────────┐");
        println!("│ SIMULATION MODE DISTRIBUTION                                    │");
        println!("└─────────────────────────────────────────────────────────────────┘\n");

        for (mode, count) in sim_mode_counts.iter() {
            let percentage = (count * 100) / flows.len();
            println!("  {:20} : {:2} flows ({:3}%)", mode, count, percentage);
        }
        println!();

        // Display synthesis hints and validation rules
        println!("┌─────────────────────────────────────────────────────────────────┐");
        println!("│ GENERATED GUIDANCE                                              │");
        println!("└─────────────────────────────────────────────────────────────────┘\n");

        println!("  💡 Synthesis Hints Generated  : {}", total_hints);
        println!("  ✓  Validation Rules Generated : {}\n", total_validations);

        // Show example intents with details
        println!("┌─────────────────────────────────────────────────────────────────┐");
        println!("│ EXAMPLE INTENT RESOLUTIONS (First 5)                           │");
        println!("└─────────────────────────────────────────────────────────────────┘\n");

        for (i, flow) in flows.iter().take(5).enumerate() {
            if let (Some(intent_call), Some(intent_result)) = (&flow.intent, &flow.intent_result) {
                println!("  {}. Intent: {}", i + 1, intent_call.name);
                println!("     SimMode: {:?}", intent_result.sim_mode);
                println!("     Nets: {}", flow.nets.join(", "));

                if !intent_result.synthesis_hints.is_empty() {
                    println!("     Hints:");
                    for hint in intent_result.synthesis_hints.iter().take(2) {
                        match hint {
                            bhdl_common::SynthesisHint::Custom(s) => {
                                println!("       • {}", s);
                            }
                            _ => println!("       • {:?}", hint),
                        }
                    }
                }
                println!();
            }
        }

        // Summary statistics
        println!("═══════════════════════════════════════════════════════════════════");
        println!("  SUMMARY STATISTICS");
        println!("═══════════════════════════════════════════════════════════════════\n");

        println!("  📊 Total Intent Functions Available : 38");
        println!("  ✅ Intent Functions Demonstrated    : {}", category_counts.values().flatten().collect::<std::collections::HashSet<_>>().len());
        println!("  🎯 Signal Flows with Intents        : {}", flows.len());
        println!("  💡 Total Synthesis Hints            : {}", total_hints);
        println!("  ✓  Total Validation Rules           : {}", total_validations);
        println!("  🔧 Intent Categories Used           : {}", category_counts.len());

        println!("\n═══════════════════════════════════════════════════════════════════");
        println!("  INTENT SYSTEM CAPABILITIES");
        println!("═══════════════════════════════════════════════════════════════════\n");

        println!("  ✅ Voltage Regulation       - Precise power supply specs");
        println!("  ✅ Current Sensing          - High-accuracy measurement");
        println!("  ✅ Communication Interfaces - UART, SPI, I2C, CAN, etc.");
        println!("  ✅ Watchdog Monitoring      - System reliability");
        println!("  ✅ Power Optimization       - Battery-powered designs");
        println!("  ✅ Signal Integrity         - High-speed signal quality");
        println!("  ✅ EMI/EMC Compliance       - Regulatory standards");
        println!("  ✅ Electrical Isolation     - Safety-critical applications");
        println!("  ✅ Thermal Management       - Power component cooling");
        println!("  ✅ Safety Standards         - Automotive, Industrial, Medical");
        println!("  ✅ Power Management         - Sequencing, monitoring");
        println!("  ✅ Digital Timing           - Clock distribution, reset");
        println!("  ✅ Test Points              - Debug and production test");
        println!("  ✅ Redundancy               - Fault-tolerant designs");
        println!();

    } else {
        println!("⚠️  No flow tracker available in analysis result");
        println!("   Intent System may not be fully integrated with analyzer");
    }

    println!("═══════════════════════════════════════════════════════════════════");
    println!("  Demo Complete!");
    println!("═══════════════════════════════════════════════════════════════════\n");

    println!("💡 The BHDL Intent System enables designers to:");
    println!("   • Explicitly capture design intent in the circuit itself");
    println!("   • Automatically generate component recommendations");
    println!("   • Validate designs against requirements");
    println!("   • Optimize simulation strategies based on intent");
    println!("   • Ensure compliance with safety and regulatory standards\n");
}

fn categorize_intent(intent_name: &str) -> &'static str {
    match intent_name {
        "delay" | "debounce" | "pulse_stretch" | "stable_for" => "⏱️  Timing",
        "noise_filtering" | "anti_alias" | "fast_response" => "🔊 Signal Processing",
        "input_protection" | "overvoltage_clamp" | "current_limiting" => "🛡️  Protection",
        "low_noise" | "signal_amplification" | "level_shifting" => "⚡ Power/Analog",
        "signal_buffering" | "output_buffering" | "signal_distribution" => "💻 Digital",
        "precision_measurement" | "control_loop" | "data_logging" => "📏 Measurement",
        "automotive_safety" | "industrial_control" | "medical_safety" | "esd_protection" => "🏥 Safety",
        "power_sequencing" | "voltage_monitoring" | "power_good_signal" | "inrush_limiting" => "🔋 Power Management",
        "clock_distribution" | "reset_generation" | "boot_sequencing" => "⏰ Digital Timing",
        "signal_integrity" | "emi_filtering" | "isolation" | "thermal_management" => "🔬 Advanced Features",
        "voltage_regulation" | "current_sensing" | "communication_interface" |
        "watchdog_monitoring" | "power_optimization" | "test_point" | "redundancy" => "🎯 Specialized",
        "debug_only" => "🐛 Development",
        _ => "❓ Unknown",
    }
}

fn get_category_emoji(category: &str) -> &str {
    if category.starts_with("⏱️") { "⏱️" }
    else if category.starts_with("🔊") { "🔊" }
    else if category.starts_with("🛡️") { "🛡️" }
    else if category.starts_with("⚡") { "⚡" }
    else if category.starts_with("💻") { "💻" }
    else if category.starts_with("📏") { "📏" }
    else if category.starts_with("🏥") { "🏥" }
    else if category.starts_with("🔋") { "🔋" }
    else if category.starts_with("⏰") { "⏰" }
    else if category.starts_with("🔬") { "🔬" }
    else if category.starts_with("🎯") { "🎯" }
    else if category.starts_with("🐛") { "🐛" }
    else { "❓" }
}
