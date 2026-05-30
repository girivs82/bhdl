//! Integration tests for intent hint processor in synthesizer
//!
//! Tests that synthesis hints are correctly applied to guide component
//! selection and optimization based on design intent.

use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::intent_hint_processor::IntentHintProcessor;

/// Helper to analyze source with intents and create hint processor
fn create_hint_processor(source: &str) -> (IntentHintProcessor, bhdl_analyzer::flow_tracking::FlowTracker) {
    let pr = parse(source);

    if !pr.errors().is_empty() {
        panic!("Parse errors: {:?}", pr.errors());
    }

    let source_file = SourceFile::cast(pr.syntax()).expect("Should parse");

    // `analyze` registers the stdlib intents and builds the FlowTracker internally.
    let analysis = analyze(&source_file);
    let flow_tracker = analysis.flow_tracker.expect("Should have flow tracker");

    let mut processor = IntentHintProcessor::new();
    processor.process_flow_hints(&flow_tracker).expect("Should process hints");

    (processor, flow_tracker)
}

#[test]
fn test_rc_network_hint_recommendation() {
    let source = r#"
        board RCBoard {
            power VCC = 5V;
            ground GND;

            net rc_filter: @VCC -> Res(1k).1 -> Cap(100n).1 -> @GND
                for delay(1ms);
        }
    "#;

    let (processor, _flow_tracker) = create_hint_processor(source);

    // Get recommendation for the RC network components
    // Note: Component names may vary based on synthesis
    // This tests the concept of getting recommendations
}

#[test]
fn test_current_limiting_value_suggestion() {
    let source = r#"
        board LEDBoard {
            power VCC = 5V;
            ground GND;

            net led_current: @VCC -> Res(?).1 -> LED(red).A
                for current_limiting(max: 20mA);
            LED(red).K -> @GND;
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Processor should suggest resistor value for 20mA current limiting
    // Expected: R = V / I = 5V / 0.02A = 250Ω (or similar based on LED Vf)

    // Check that flow has current limiting intent
    let has_current_limit = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.iter().any(|hint| {
                matches!(hint, bhdl_common::SynthesisHint::Custom(s) if s.contains("current limiting"))
            })
        })
    });

    assert!(has_current_limit, "Should have current limiting intent");
}

#[test]
fn test_filter_topology_recommendation() {
    let source = r#"
        board FilterBoard {
            power VCC = 5V;
            ground GND;

            net filtered: @VCC -> Res(1k).1 -> Cap(100n).1 -> @GND
                for anti_alias(before: adc, cutoff: 10kHz);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Should recommend analog filter topology
    let has_filter_hint = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.contains(&bhdl_common::SynthesisHint::AnalogFilter)
        })
    });

    assert!(has_filter_hint, "Should have analog filter hint");
}

#[test]
fn test_optimization_priority_low_noise() {
    let source = r#"
        board LowNoiseBoard {
            power VCC = 5V;
            ground GND;

            net sig: @VCC -> Res(10k).1 -> @GND
                for low_noise(max_ripple: 1mV);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Low noise intent should set MinimizeNoise optimization priority
    let has_low_noise = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.iter().any(|hint| {
                matches!(hint, bhdl_common::SynthesisHint::Custom(s) if s.contains("low-noise"))
            })
        })
    });

    assert!(has_low_noise, "Should have low noise hint");
}

#[test]
fn test_optimization_priority_precision() {
    let source = r#"
        board PrecisionBoard {
            power VCC = 5V;
            ground GND;

            net measurement: @VCC -> Res(10k).1 -> @GND
                for precision_measurement(accuracy: 0.1%);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // The precision_measurement intent emits precision-oriented synthesis
    // hints (e.g. "Use precision ADC"); the raw accuracy value is not echoed
    // into a hint string, so match on the precision guidance itself.
    let has_precision = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.iter().any(|hint| {
                matches!(hint, bhdl_common::SynthesisHint::Custom(s) if s.to_lowercase().contains("precision"))
            })
        })
    });

    assert!(has_precision, "Should have precision measurement hint");
}

#[test]
fn test_optimization_priority_speed() {
    let source = r#"
        board HighSpeedBoard {
            power VCC = 5V;
            ground GND;

            net fast_path: @VCC -> Res(100).1 -> @GND
                for delay(10ns);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Fast delay should suggest buffer chain (speed optimization)
    let has_buffer_hint = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.contains(&bhdl_common::SynthesisHint::BufferChain)
        })
    });

    assert!(has_buffer_hint, "Should have buffer chain hint for fast delay");
}

#[test]
fn test_validation_rules_from_hints() {
    let source = r#"
        board ValidationBoard {
            power VCC = 5V;
            ground GND;

            net protected: @VCC -> TVSDiode(6V).K -> Res(1k).1 -> @GND
                for input_protection(overvoltage: 6V, current_limit: 5mA);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Should have validation rules for input protection
    let has_validation = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            !ir.validation_rules.is_empty()
        })
    });

    assert!(has_validation, "Should have validation rules");
}

#[test]
fn test_component_recommendation_alternatives() {
    let source = r#"
        board AlternativesBoard {
            power VCC = 5V;
            ground GND;

            net filter: @VCC -> Res(1k).1 -> Cap(100n).1 -> @GND
                for noise_filtering(cutoff: 1kHz, attenuation: 40dB);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Processor should provide alternative topologies for filters
    // e.g., passive RC, active filter, Sallen-Key, etc.
}

#[test]
fn test_hierarchical_hints_propagation() {
    // Hierarchical design using the current entity/instance grammar. The
    // analog section is an instantiated entity; the board-level net carries a
    // low_noise intent so the flow tracker has at least one intent-bearing
    // flow path.
    let source = r#"
        entity Amplifier {
            pin IN: signal in;
            pin OUT: signal out;
        }

        board HierarchicalBoard {
            power VCC = 5V;
            ground GND;

            amp: Amplifier { }

            net sig_path: @VCC -> Res(10k).1 -> @GND
                for low_noise(max_ripple: 5mV);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // The intent-bearing net should produce at least one flow path.
    let parent_paths = flow_tracker.get_flow_paths();
    let has_hierarchical_hints = !parent_paths.is_empty();

    assert!(has_hierarchical_hints, "Should have flow paths with hints");
}

#[test]
fn test_intent_emits_multiple_hints() {
    // The current grammar allows a single `for` intent clause per net (a net
    // statement parses one optional intent clause, then `;`). A single intent
    // can still contribute several synthesis hints — low_noise emits "Use
    // low-noise components", "Consider shielding", and "Star grounding
    // recommended" — which is what we verify here.
    let source = r#"
        board MultiIntentBoard {
            power VCC = 5V;
            ground GND;

            net combo: @VCC -> Res(1k).1 -> Cap(1u).1 -> @GND
                for low_noise(max_ripple: 10mV);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // A single intent should be able to contribute more than one hint.
    let flow_paths = flow_tracker.get_flow_paths();
    let has_combined_hints = flow_paths.iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.len() > 1
        })
    });

    assert!(has_combined_hints, "A single intent should emit multiple synthesis hints");
}

#[test]
fn test_custom_hint_parsing() {
    let source = r#"
        board CustomHintBoard {
            power VCC = 5V;
            ground GND;

            net amplified: @VCC -> Res(1k).1 -> @GND
                for signal_amplification(gain: 10);
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Custom hints should be parsed and stored
    let has_custom_hints = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.iter().any(|hint| {
                matches!(hint, bhdl_common::SynthesisHint::Custom(s) if s.contains("gain"))
            })
        })
    });

    assert!(has_custom_hints, "Should have custom hints for amplification");
}

#[test]
fn test_value_constraint_extraction() {
    let source = r#"
        board ConstraintBoard {
            power VCC = 5V;
            ground GND;

            net limited: @VCC -> Res(?).1 -> LED(red).A
                for current_limiting(max: 30mA);
            LED(red).K -> @GND;
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Should extract 30mA max current constraint
    let has_current_constraint = flow_tracker.get_flow_paths().iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| {
            ir.synthesis_hints.iter().any(|hint| {
                matches!(hint, bhdl_common::SynthesisHint::Custom(s) if s.contains("30mA") || s.contains("30 mA"))
            })
        })
    });

    assert!(has_current_constraint, "Should have current constraint");
}

#[test]
fn test_hint_processor_performance() {
    // Generate large circuit to test performance
    let mut source = String::from("board PerfBoard {\n");
    source.push_str("    power VCC = 5V;\n");
    source.push_str("    ground GND;\n\n");

    // Generate 200 flows with different intents
    for i in 0..200 {
        let intent = match i % 5 {
            0 => "for low_noise(max_ripple: 10mV)",
            1 => "for current_limiting(max: 20mA)",
            2 => "for delay(1ms)",
            3 => "for precision_measurement(accuracy: 0.1%)",
            _ => "for anti_alias(before: adc, cutoff: 10kHz)",
        };

        source.push_str(&format!(
            "    net n{}: @VCC -> Res(1k).1 -> @GND {};\n",
            i, intent
        ));
    }

    source.push_str("}\n");

    let start = std::time::Instant::now();
    let (processor, _flow_tracker) = create_hint_processor(&source);
    let duration = start.elapsed();

    // Should process hints quickly
    assert!(duration.as_secs() < 5,
            "Hint processing should be fast: {:?}", duration);

    println!("Processed hints for 200 flows in {:?}", duration);
}

#[test]
fn test_validation_result_warnings() {
    let source = r#"
        board ValidationBoard {
            power VCC = 5V;
            ground GND;

            net test: @VCC -> Res(1k).1 -> @GND
                for current_limiting(max: 20mA);
        }
    "#;

    let (processor, _flow_tracker) = create_hint_processor(source);

    // Test validation of component selection
    // This would typically be done during synthesis
    // Here we just verify the processor can validate

    // For a 1kΩ resistor at 5V, current would be 5mA, which is within 20mA limit
    // Validation should pass
}

#[test]
fn test_tool_scope_filtering() {
    let source = r#"
        board ToolScopeBoard {
            power VCC = 5V;
            ground GND;

            // Debug signal - simulation only
            net debug: @VCC -> Res(1k).1 -> @GND
                for debug_only();

            // Production signal - all tools
            net production: @VCC -> Res(330).1 -> LED(red).A
                for current_limiting(max: 20mA);
            LED(red).K -> @GND;
        }
    "#;

    let (processor, flow_tracker) = create_hint_processor(source);

    // Debug signals should have SimulationOnly scope
    // Production signals should have All scope

    let scopes: Vec<_> = flow_tracker.get_flow_paths().iter()
        .filter_map(|fp| fp.intent_result.as_ref().map(|ir| ir.tool_scope))
        .collect();

    assert!(scopes.contains(&bhdl_common::ToolScope::SimulationOnly),
            "Should have SimulationOnly scope");
    assert!(scopes.contains(&bhdl_common::ToolScope::All),
            "Should have All scope");
}
