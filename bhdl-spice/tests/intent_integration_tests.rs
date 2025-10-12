//! Integration tests for SPICE intent handling
//!
//! Tests that SPICE analysis scope is correctly determined from intent results,
//! and that components are properly filtered for simulation.

use bhdl_parser::Parser;
use bhdl_ast::SourceFile;
use bhdl_analyzer::{Analyzer, SymbolTable};
use bhdl_synthesizer::Synthesizer;
use bhdl_spice::intent_handler::{determine_spice_scope, AnalysisHint};
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_stdlib::intents;
use std::sync::Arc;

/// Helper to create complete pipeline with intents
fn analyze_with_intents(source: &str) -> (bhdl_netlist::Netlist, bhdl_analyzer::flow_tracking::FlowTracker) {
    let parser = Parser::new(source);
    let (tree, parse_errors) = parser.parse();

    if !parse_errors.is_empty() {
        panic!("Parse errors: {:?}", parse_errors);
    }

    let source_file = SourceFile::cast(tree).expect("Should parse");

    let mut symbol_table = SymbolTable::new();
    let mut intent_registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut intent_registry);

    let mut analyzer = Analyzer::new(&mut symbol_table);
    analyzer.set_intent_registry(Arc::new(intent_registry));

    let analysis_result = analyzer.analyze(&source_file);
    let flow_tracker = analyzer.get_flow_tracker().expect("Should have flow tracker");

    let synthesizer = Synthesizer::new(&symbol_table, &analysis_result.resolved_constants);
    let netlist = synthesizer.synthesize(&source_file, &analysis_result)
        .expect("Synthesis should succeed");

    (netlist, flow_tracker.clone())
}

#[test]
fn test_spice_scope_pure_digital() {
    let source = r#"
        board DigitalBoard {
            power VCC = 5V;
            ground GND;

            // Digital-only circuit
            module DigitalLogic {
                pin IN: signal in;
                pin OUT: signal out;

                for debug_only();
            }

            signal_in -> DigitalLogic().IN;
            DigitalLogic().OUT -> signal_out;
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // PureDigital components should be skipped in SPICE
    assert!(scope.analog_required.is_empty(),
            "Pure digital should have no analog-required components");
    assert!(!scope.skip_components.is_empty(),
            "Pure digital components should be in skip list");
}

#[test]
fn test_spice_scope_analog_required() {
    let source = r#"
        board AnalogBoard {
            power VCC = 5V;
            ground GND;

            // Analog circuit requiring SPICE
            net filtered: @VCC -> Res(1k).1 -> Cap(100n).1 -> @GND
                for low_noise(max_ripple: 10mV);
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // Low noise requires analog simulation
    assert!(!scope.analog_required.is_empty(),
            "Low noise should require analog simulation");

    // Should have analysis hints
    assert!(!scope.analysis_hints.is_empty(),
            "Should have analysis hints for low noise");
}

#[test]
fn test_spice_scope_mixed_signal() {
    let source = r#"
        board MixedSignalBoard {
            power VCC = 5V;
            ground GND;

            // Analog section
            net analog: @VCC -> Res(1k).1 -> @GND
                for precision_measurement(accuracy: 0.1%);

            // Digital section
            net digital: @VCC -> Res(10k).1 -> @GND
                for debug_only();
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // Should have both analog and skipped components
    assert!(!scope.analog_required.is_empty(),
            "Should have analog components for precision measurement");
    assert!(!scope.skip_components.is_empty(),
            "Should have digital components to skip");
}

#[test]
fn test_analysis_hints_current_limiting() {
    let source = r#"
        board CurrentLimitBoard {
            power VCC = 5V;
            ground GND;

            @VCC -> Res(330).1 -> LED(red).A
                for current_limiting(max: 20mA);
            LED(red).K -> @GND;
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // Should have current limiting hint
    let has_current_limit_hint = scope.analysis_hints.iter().any(|hint| {
        matches!(hint, AnalysisHint::CurrentLimiting { max_current, .. } if *max_current > 0.0)
    });

    assert!(has_current_limit_hint,
            "Should have current limiting analysis hint");
}

#[test]
fn test_analysis_hints_noise_analysis() {
    let source = r#"
        board NoiseBoard {
            power VCC = 5V;
            ground GND;

            net low_noise: @VCC -> Res(10k).1 -> @GND
                for low_noise(max_ripple: 5mV);
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // Should have noise analysis hint
    let has_noise_hint = scope.analysis_hints.iter().any(|hint| {
        matches!(hint, AnalysisHint::NoiseAnalysis { .. })
    });

    assert!(has_noise_hint,
            "Should have noise analysis hint for low_noise intent");
}

#[test]
fn test_analysis_hints_transient_analysis() {
    let source = r#"
        board TransientBoard {
            power VCC = 5V;
            ground GND;

            net delayed: @VCC -> Res(1k).1 -> Cap(1u).1 -> @GND
                for delay(10ms);
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // Should have transient analysis hint for delay
    let has_transient_hint = scope.analysis_hints.iter().any(|hint| {
        matches!(hint, AnalysisHint::TransientAnalysis { time_constant, .. } if *time_constant > 0.0)
    });

    assert!(has_transient_hint,
            "Should have transient analysis hint for delay intent");
}

#[test]
fn test_analysis_hints_frequency_response() {
    let source = r#"
        board FilterBoard {
            power VCC = 5V;
            ground GND;

            net filtered: @VCC -> Res(1k).1 -> Cap(100n).1 -> @GND
                for anti_alias(before: adc, cutoff: 10kHz);
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // Should have frequency response hint
    let has_freq_hint = scope.analysis_hints.iter().any(|hint| {
        matches!(hint, AnalysisHint::FrequencyResponse { bandwidth, .. } if *bandwidth > 0.0)
    });

    assert!(has_freq_hint,
            "Should have frequency response hint for filter");
}

#[test]
fn test_hierarchical_intent_spice_scope() {
    let source = r#"
        board HierarchicalBoard {
            // Module with analog intent
            module AnalogModule {
                pin IN: signal in;
                pin OUT: signal out;

                for low_noise(max_ripple: 10mV);

                // Submodule inherits intent
                module Amplifier {
                    pin IN: signal in;
                    pin OUT: signal out;
                }

                IN -> Amplifier().IN;
                Amplifier().OUT -> OUT;
            }

            signal_in -> AnalogModule().IN;
            AnalogModule().OUT -> signal_out;
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // All components in analog hierarchy should be included
    assert!(!scope.analog_required.is_empty(),
            "Hierarchical analog modules should be included in SPICE");
}

#[test]
fn test_spice_scope_with_mixed_signal_timing() {
    let source = r#"
        board TimingBoard {
            power VCC = 5V;
            ground GND;

            // Timing-critical path - needs mixed signal
            net timing: @VCC -> Res(1k).1 -> @GND
                for delay(100ns);
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // 100ns delay should use DigitalWithTiming or MixedSignal
    // Should be in mixed_signal list
    assert!(!scope.mixed_signal.is_empty() || !scope.analog_required.is_empty(),
            "Timing-critical path should be in SPICE scope");
}

#[test]
fn test_spice_configuration_from_hints() {
    let source = r#"
        board ConfigBoard {
            power VCC = 5V;
            ground GND;

            // Multiple analysis requirements
            net n1: @VCC -> Res(100).1 -> @GND for current_limiting(max: 50mA);
            net n2: @VCC -> Res(1k).1 -> Cap(1u).1 -> @GND for delay(1ms);
            net n3: @VCC -> Res(10k).1 -> @GND for low_noise(max_ripple: 1mV);
        }
    "#;

    let (netlist, flow_tracker) = analyze_with_intents(source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);

    // Should have multiple hint types
    let hint_types: std::collections::HashSet<_> = scope.analysis_hints.iter()
        .map(|hint| std::mem::discriminant(hint))
        .collect();

    assert!(hint_types.len() >= 2,
            "Should have multiple analysis hint types");

    // Should include components for all three nets
    let total_components = scope.analog_required.len()
                         + scope.mixed_signal.len()
                         + scope.skip_components.len();

    assert!(total_components > 0,
            "Should have components categorized");
}

#[test]
fn test_spice_scope_performance_large_circuit() {
    // Generate large circuit to test performance
    let mut source = String::from("board LargeBoard {\n");
    source.push_str("    power VCC = 5V;\n");
    source.push_str("    ground GND;\n\n");

    // Generate 100 flows with different intents
    for i in 0..100 {
        let intent = match i % 4 {
            0 => "for debug_only()",
            1 => "for low_noise(max_ripple: 10mV)",
            2 => "for current_limiting(max: 20mA)",
            _ => "for delay(1ms)",
        };

        source.push_str(&format!(
            "    net n{}: @VCC -> Res(1k).1 -> @GND {};\n",
            i, intent
        ));
    }

    source.push_str("}\n");

    let start = std::time::Instant::now();
    let (netlist, flow_tracker) = analyze_with_intents(&source);
    let scope = determine_spice_scope(&netlist, &flow_tracker);
    let duration = start.elapsed();

    // Should complete quickly even for large circuits
    assert!(duration.as_secs() < 5,
            "SPICE scope determination should be fast: {:?}", duration);

    // Should have categorized all components
    let total = scope.analog_required.len()
              + scope.mixed_signal.len()
              + scope.skip_components.len();

    println!("Categorized {} components in {:?}", total, duration);
}
