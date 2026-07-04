//! Real-world intent validation test
//!
//! Tests that realistic circuit intents parse and resolve correctly

use std::fs;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_common::SimMode;

fn main() {
    println!("=== Real-World Intent Validation ===\n");

    // Test realistic circuits
    test_circuit("tests/circuits/realistic/7805_with_intents.bhdl");
    test_circuit("tests/circuits/realistic/buck_converter_with_intents.bhdl");
    test_circuit("tests/circuits/realistic/mixed_signal_with_intents.bhdl");

    println!("\n=== All Real-World Tests Complete ===");
}

fn test_circuit(path: &str) {
    println!("Testing: {}", path);
    println!("{}", "=".repeat(60));

    // Load circuit
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            println!("✗ Failed to read: {}\n", e);
            return;
        }
    };

    // Parse
    let parse_result = parse(&source);
    let syntax_node = parse_result.syntax();
    let source_file = match SourceFile::cast(syntax_node) {
        Some(sf) => sf,
        None => {
            println!("✗ Failed to parse\n");
            return;
        }
    };

    // Analyze
    let analysis = analyze(&source_file);

    // Display results
    println!("Diagnostics: {}", analysis.diagnostics.len());

    if !analysis.diagnostics.is_empty() {
        println!("\nDiagnostics:");
        for diag in &analysis.diagnostics {
            println!("  - {}", diag.message);
        }
    }

    // Analyze intent results
    let flow_tracker = match &analysis.flow_tracker {
        Some(ft) => ft,
        None => {
            println!("✗ No flow tracker available\n");
            return;
        }
    };
    let flow_paths = flow_tracker.get_flow_paths();

    println!("\nIntent Analysis:");
    println!("  Flow paths: {}", flow_paths.len());

    // Count SimModes
    let mut sim_mode_counts = std::collections::HashMap::new();
    let mut hint_count = 0;
    let mut validation_count = 0;

    for flow_path in flow_paths {
        if let Some(ref intent_result) = flow_path.intent_result {
            *sim_mode_counts.entry(intent_result.sim_mode).or_insert(0) += 1;
            hint_count += intent_result.synthesis_hints.len();
            validation_count += intent_result.validation_rules.len();
        }
    }

    println!("  SimMode distribution:");
    for (mode, count) in &sim_mode_counts {
        println!("    {:?}: {}", mode, count);
    }
    println!("  Total synthesis hints: {}", hint_count);
    println!("  Total validation rules: {}", validation_count);

    // Show some example intents
    println!("\n  Example intents:");
    for (idx, flow_path) in flow_paths.iter().take(3).enumerate() {
        if let Some(ref intent_result) = flow_path.intent_result {
            println!("    Flow #{}", idx + 1);
            if !flow_path.nets.is_empty() {
                println!("      Nets: {}", flow_path.nets.join(", "));
            }
            println!("      SimMode: {:?}", intent_result.sim_mode);
            println!("      Hints: {}", intent_result.synthesis_hints.len());
            for hint in intent_result.synthesis_hints.iter().take(2) {
                println!("        - {:?}", hint);
            }
            if !intent_result.validation_rules.is_empty() {
                println!("      Validations:");
                for rule in intent_result.validation_rules.iter().take(1) {
                    println!("        - {}: {}", rule.condition, rule.error_message);
                }
            }
        }
    }

    // Verify critical intent categories
    let has_analog = sim_mode_counts.contains_key(&SimMode::AnalogRequired);
    let has_digital = sim_mode_counts.contains_key(&SimMode::PureDigital) ||
                      sim_mode_counts.contains_key(&SimMode::DigitalWithTiming);
    let has_mixed = sim_mode_counts.contains_key(&SimMode::MixedSignal);

    println!("\n  Intent categories:");
    println!("    ✓ Analog: {}", if has_analog { "Yes" } else { "No" });
    println!("    ✓ Digital: {}", if has_digital { "Yes" } else { "No" });
    println!("    ✓ Mixed-Signal: {}", if has_mixed { "Yes" } else { "No" });

    // Overall assessment
    if !flow_paths.is_empty() {
        println!("\n✓ SUCCESS: Circuit parsed and intents resolved");
    } else {
        println!("\n✗ FAILED: No intents found");
    }

    println!();
}
