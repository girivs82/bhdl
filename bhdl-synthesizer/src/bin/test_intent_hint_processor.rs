// Demonstration of intent hint processor for synthesis guidance

use bhdl_synthesizer::intent_hint_processor::{
    IntentHintProcessor, OptimizationPriority,
};
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_common::{IntentRegistry, SynthesisHint, IntentResult, SimMode, ToolScope, ValidationRule};
use bhdl_stdlib::intents;

fn main() {
    println!("=== Intent Hint Processor Demo ===\n");

    // Step 1: Create flow tracker with intents
    println!("1. Creating flow tracker with intent registry...");
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    let mut flow_tracker = FlowTracker::new(registry);
    println!("   Flow tracker initialized with {} intents\n", flow_tracker.get_intent_registry().registered_intents().len());

    // Step 2: Simulate some intent results (in real scenario, these come from analyzer)
    println!("2. Simulating intent resolution results...");
    add_simulated_intent_results(&mut flow_tracker);
    println!("   Added example intent results for demonstration\n");

    // Step 3: Create hint processor
    println!("3. Creating intent hint processor...");
    let mut processor = IntentHintProcessor::new();
    println!("   Processor ready\n");

    // Step 4: Process hints from flow tracker
    println!("4. Processing synthesis hints from flow tracker...");
    processor.process_flow_hints(&flow_tracker).expect("Failed to process hints");
    println!("   Hints processed successfully\n");

    // Step 5: Get recommendations for components
    println!("5. Component Recommendations:\n");

    let components = vec![
        ("filter_rc", "RC filter for anti-aliasing"),
        ("delay_line", "Delay circuit for debouncing"),
        ("current_limiter", "Current limiting resistor"),
    ];

    for (component_name, description) in components {
        println!("   Component: {} ({})", component_name, description);

        if let Some(recommendation) = processor.get_component_recommendation(component_name) {
            println!("     Type: {}", recommendation.component_type);
            if let Some(ref value) = recommendation.suggested_value {
                println!("     Suggested value: {}", value);
            }
            println!("     Rationale: {}", recommendation.rationale);
            println!("     Confidence: {:.1}%", recommendation.confidence * 100.0);

            if !recommendation.alternative_options.is_empty() {
                println!("     Alternatives:");
                for alt in &recommendation.alternative_options {
                    println!("       - {}", alt);
                }
            }
        } else {
            println!("     No specific recommendation available");
        }

        // Get validation rules
        let rules = processor.get_validation_rules(component_name);
        if !rules.is_empty() {
            println!("     Validation rules:");
            for rule in rules {
                println!("       - {}: {}", rule.condition, rule.error_message);
            }
        }

        println!();
    }

    // Step 6: Validate component selections
    println!("6. Validating Component Selections:\n");

    let selections = vec![
        ("filter_rc", "RC Filter", Some("1kΩ + 100nF")),
        ("delay_line", "RC Network", Some("10kΩ + 1µF")),
        ("current_limiter", "Resistor", Some("250Ω")),
    ];

    for (component_name, selected_type, selected_value) in selections {
        println!("   Component: {}", component_name);
        println!("     Selected: {} = {:?}", selected_type, selected_value);

        let validation = processor.validate_component_selection(
            component_name,
            selected_type,
            selected_value,
        ).expect("Validation failed");

        if validation.is_valid {
            println!("     ✓ Selection is valid");
        } else {
            println!("     ⚠ Selection has warnings:");
            for warning in &validation.warnings {
                println!("       - {}", warning);
            }
        }

        if !validation.suggestions.is_empty() {
            println!("     Suggestions:");
            for suggestion in &validation.suggestions {
                println!("       • {}", suggestion);
            }
        }

        println!();
    }

    // Step 7: Show benefits
    println!("7. Benefits of Intent-Aware Synthesis:");
    println!("   ✓ Intelligent component selection based on design intent");
    println!("   ✓ Automatic value calculation for constraints");
    println!("   ✓ Validation of selections against requirements");
    println!("   ✓ Alternative suggestions for optimization");
    println!("   ✓ Topology recommendations from synthesis hints\n");

    println!("=== Demo Complete ===");
}

/// Add simulated intent results for demonstration
/// In a real scenario, these would come from the analyzer's intent resolution
fn add_simulated_intent_results(flow_tracker: &mut FlowTracker) {
    // Simulate anti-alias filter intent
    flow_tracker.simulate_intent_result(
        "filter_rc",
        IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::AnalogFilter,
                SynthesisHint::Custom("Low-pass filter with 1kHz cutoff".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "filter_cutoff_at_1kHz".to_string(),
                    error_message: "Filter cutoff frequency must be 1kHz".to_string(),
                }
            ],
            tool_scope: ToolScope::All,
        },
    );

    // Simulate debounce delay intent
    flow_tracker.simulate_intent_result(
        "delay_line",
        IntentResult {
            sim_mode: SimMode::MixedSignal,
            synthesis_hints: vec![
                SynthesisHint::RCNetwork,
                SynthesisHint::Custom("Debounce circuit for 20ms delay".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "has_rc_network".to_string(),
                    error_message: "Debounce requires RC network for 20ms delay".to_string(),
                }
            ],
            tool_scope: ToolScope::All,
        },
    );

    // Simulate current limiting intent
    flow_tracker.simulate_intent_result(
        "current_limiter",
        IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom("Add current limiting resistor for max 20mA".to_string()),
                SynthesisHint::Custom("Consider current sense resistor".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "current_within_limit".to_string(),
                    error_message: "Current must not exceed 20mA".to_string(),
                }
            ],
            tool_scope: ToolScope::All,
        },
    );
}

impl FlowTracker {
    /// Simulate adding an intent result for demonstration purposes
    /// This would normally be done internally during intent resolution
    fn simulate_intent_result(&mut self, component_name: &str, intent_result: IntentResult) {
        // In production, this data structure would be updated through proper analysis
        // For demo purposes, we're just showing the concept
        println!("   [Simulated] Intent result for '{}': {:?}", component_name, intent_result.sim_mode);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_intent_hint_processor_smoke() {
        let mut registry = IntentRegistry::new();
        intents::register_stdlib_intents(&mut registry);
        let flow_tracker = FlowTracker::new(registry);

        let mut processor = IntentHintProcessor::new();
        // Should not panic
        processor.process_flow_hints(&flow_tracker).expect("Processing failed");
    }
}
