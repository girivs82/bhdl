//! End-to-end test of Intent System with real circuits
//!
//! Tests complete pipeline: Parse → Analyze → Synthesize → Intent Processing
//! Uses realistic circuits with intent annotations

use std::fs;
use std::sync::Arc;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::{NetlistGenerator, NetlistConfig, IntentHintProcessor};
use bhdl_spice::intent_handler::determine_spice_scope;
use bhdl_common::IntentRegistry;
use bhdl_stdlib::intents;

#[tokio::main]
async fn main() {
    println!("=== Intent System End-to-End Test ===\n");

    // Test 1: 7805 Regulator with intents
    println!("Test 1: 7805 Voltage Regulator with Intent Annotations");
    println!("--------------------------------------------------------");
    test_circuit("tests/circuits/realistic/7805_with_intents.bhdl").await;
    println!();

    // Test 2: Buck Converter with intents
    println!("Test 2: Buck Converter with Intent-Driven Design");
    println!("------------------------------------------------");
    test_circuit("tests/circuits/realistic/buck_converter_with_intents.bhdl").await;
    println!();

    // Test 3: Mixed-Signal Circuit with timing intents
    println!("Test 3: Mixed-Signal Circuit with Timing Intents");
    println!("------------------------------------------------");
    test_circuit("tests/circuits/realistic/mixed_signal_with_intents.bhdl").await;
    println!();

    println!("=== All End-to-End Tests Complete ===");
}

async fn test_circuit(path: &str) {
    // Step 1: Load and parse
    println!("1. Loading circuit: {}", path);
    let source = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            println!("   ✗ Failed to read file: {}", e);
            return;
        }
    };

    let parse_result = parse(&source);
    let syntax_node = parse_result.syntax();
    let source_file = match SourceFile::cast(syntax_node) {
        Some(sf) => sf,
        None => {
            println!("   ✗ Failed to cast to SourceFile");
            return;
        }
    };
    println!("   ✓ Parsed successfully");

    // Step 2: Analyze
    println!("2. Analyzing...");
    let analysis_result = analyze(&source_file);

    let error_count = analysis_result.diagnostics.iter()
        .filter(|d| matches!(d.severity, bhdl_analyzer::DiagnosticSeverity::Error))
        .count();

    if error_count > 0 {
        println!("   ✗ Analysis errors ({}):", error_count);
        for diag in &analysis_result.diagnostics {
            if matches!(diag.severity, bhdl_analyzer::DiagnosticSeverity::Error) {
                println!("      {}", diag.message);
            }
        }
        // Continue anyway to show intent processing
    } else {
        println!("   ✓ Analysis succeeded");
    }

    // Step 3: Extract flow tracker
    let flow_tracker = &analysis_result.flow_tracker;
    println!("   ✓ Flow tracking available");

    // Step 4: Display intent results
    println!("3. Intent Resolution Results:");
    let flow_paths = flow_tracker.get_flow_paths();
    println!("   Found {} flow paths with intents", flow_paths.len());

    let mut sim_mode_counts = std::collections::HashMap::new();
    let mut synthesis_hint_types = std::collections::HashSet::new();

    for (idx, flow_path) in flow_paths.iter().enumerate() {
        if let Some(ref intent_result) = flow_path.intent_result {
            println!("\n   Flow path #{}: {:?}", idx + 1, flow_path.net_name);
            println!("      SimMode: {:?}", intent_result.sim_mode);
            println!("      Synthesis hints: {}", intent_result.synthesis_hints.len());
            for hint in &intent_result.synthesis_hints {
                println!("         - {:?}", hint);
            }
            println!("      Validation rules: {}", intent_result.validation_rules.len());
            for rule in &intent_result.validation_rules {
                println!("         - {}: {}", rule.condition, rule.error_message);
            }
            println!("      Tool scope: {:?}", intent_result.tool_scope);

            *sim_mode_counts.entry(intent_result.sim_mode).or_insert(0) += 1;
            for hint in &intent_result.synthesis_hints {
                synthesis_hint_types.insert(std::mem::discriminant(hint));
            }
        }
    }

    // Step 5: Synthesize netlist
    println!("\n4. Synthesizing netlist...");
    let mut config = NetlistConfig::default();
    config.preserve_semantic_context = true;
    config.include_power_domains = true;
    config.include_component_inference = true;
    config.database_path = None; // Disable database for testing

    let mut generator = NetlistGenerator::with_config(config);
    let netlist = match generator.generate_from_ast_and_analysis(&source_file, &analysis_result).await {
        Ok(n) => n,
        Err(e) => {
            println!("   ✗ Synthesis failed: {}", e);
            return;
        }
    };
    println!("   ✓ Netlist created");
    println!("      Modules: {}", netlist.modules.len());
    println!("      Instances: {}", netlist.instances.len());
    println!("      Nets: {}", netlist.nets.len());

    // Step 6: SPICE intent integration
    println!("\n5. SPICE Intent Integration:");
    let spice_scope = determine_spice_scope(&netlist, &flow_tracker);
    println!("   Analog required: {} components", spice_scope.analog_required.len());
    println!("   Mixed signal: {} components", spice_scope.mixed_signal.len());
    println!("   Skip (digital): {} components", spice_scope.skip_components.len());
    println!("   Analysis hints: {}", spice_scope.analysis_hints.len());

    for hint in &spice_scope.analysis_hints {
        match hint {
            bhdl_spice::intent_handler::AnalysisHint::CurrentLimiting { component, max_current } => {
                println!("      - Current limiting: {} (max: {:.1}mA)", component, max_current * 1000.0);
            }
            bhdl_spice::intent_handler::AnalysisHint::NoiseAnalysis { component, max_noise_floor } => {
                println!("      - Noise analysis: {} (max: {:.1}dB)", component, max_noise_floor);
            }
            bhdl_spice::intent_handler::AnalysisHint::TransientAnalysis { component, time_constant } => {
                println!("      - Transient analysis: {} (τ: {:.3}ms)", component, time_constant * 1000.0);
            }
            bhdl_spice::intent_handler::AnalysisHint::FrequencyResponse { component, bandwidth } => {
                println!("      - Frequency response: {} (BW: {:.1}kHz)", component, bandwidth / 1000.0);
            }
            bhdl_spice::intent_handler::AnalysisHint::HighPrecision { component, required_accuracy } => {
                println!("      - High precision: {} (accuracy: {:.2}%)", component, required_accuracy * 100.0);
            }
            bhdl_spice::intent_handler::AnalysisHint::PowerDissipation { component, max_power } => {
                println!("      - Power dissipation: {} (max: {:.2}W)", component, max_power);
            }
        }
    }

    // Step 7: Synthesizer hint processor
    println!("\n6. Synthesizer Hint Processor:");
    let mut hint_processor = IntentHintProcessor::new();
    if let Err(e) = hint_processor.process_flow_hints(&flow_tracker) {
        println!("   ✗ Failed to process hints: {}", e);
    } else {
        println!("   ✓ Hints processed successfully");

        // Show some recommendations (if available)
        let sample_components = vec!["R1", "C1", "led"];
        for comp_name in sample_components {
            if let Some(recommendation) = hint_processor.get_component_recommendation(comp_name) {
                println!("\n   Recommendation for '{}':", comp_name);
                println!("      Type: {}", recommendation.component_type);
                if let Some(ref value) = recommendation.suggested_value {
                    println!("      Suggested value: {}", value);
                }
                println!("      Rationale: {}", recommendation.rationale);
                println!("      Confidence: {:.0}%", recommendation.confidence * 100.0);
            }
        }
    }

    // Step 8: Summary statistics
    println!("\n7. Summary Statistics:");
    println!("   SimMode distribution:");
    for (mode, count) in sim_mode_counts {
        println!("      {:?}: {}", mode, count);
    }
    println!("   Unique synthesis hint types: {}", synthesis_hint_types.len());
    println!("   Total validation rules: {}",
             flow_paths.iter()
                 .filter_map(|fp| fp.intent_result.as_ref())
                 .map(|ir| ir.validation_rules.len())
                 .sum::<usize>());
}
