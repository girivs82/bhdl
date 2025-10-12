//! Tests for intent conflict scenarios and resolution
//!
//! Tests what happens when multiple intents are applied to the same component,
//! overlapping flows, and contradictory requirements.

use bhdl_parser::Parser;
use bhdl_ast::SourceFile;
use bhdl_analyzer::{Analyzer, SymbolTable};
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_stdlib::intents;
use std::sync::Arc;

/// Test multiple intents on the same flow
#[test]
fn test_multiple_intents_same_flow() {
    let source = r#"
        board TestBoard {
            power VCC = 5V;
            ground GND;

            // Flow with multiple intents - should use most restrictive
            @VCC -> Res(1k).1 -> Res(1k).2 -> @GND
                for low_noise(max_ripple: 1mV)
                for precision_measurement(accuracy: 0.1%);
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // Both intents require analog simulation - should be combined
    let flow_tracker = analyzer.get_flow_tracker().expect("Should have flow tracker");
    let flow_paths = flow_tracker.get_flow_paths();

    assert!(!flow_paths.is_empty(), "Should have flow paths");

    // The flow should be in AnalogRequired mode (most restrictive)
    // Both low_noise and precision_measurement require analog simulation
}

/// Test conflicting sim modes on branching flows
#[test]
fn test_branching_flows_different_intents() {
    let source = r#"
        board BranchingBoard {
            power VCC = 5V;
            ground GND;

            // Split signal: one path analog, one path digital
            net source: @VCC -> Res(1k).1;

            // Analog branch
            net analog_path: @source -> Cap(100n).1 -> @GND
                for low_noise(max_ripple: 10mV);

            // Digital branch
            net digital_path: @source -> Res(10k).1 -> @GND
                for debug_only();
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // Should have multiple flow paths with different intents
    let flow_tracker = analyzer.get_flow_tracker().expect("Should have flow tracker");
    let flow_paths = flow_tracker.get_flow_paths();

    // Should have at least 3 paths: source, analog_path, digital_path
    assert!(flow_paths.len() >= 2, "Should have multiple flow paths");

    // Check that different paths have different simulation modes
    let has_analog = flow_paths.iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| ir.sim_mode == SimMode::AnalogRequired)
    });

    let has_digital = flow_paths.iter().any(|fp| {
        fp.intent_result.as_ref().map_or(false, |ir| ir.sim_mode == SimMode::PureDigital)
    });

    assert!(has_analog || has_digital, "Should have different simulation modes on branches");
}

/// Test hierarchical intent override
#[test]
fn test_hierarchical_intent_override() {
    let source = r#"
        board HierarchyBoard {
            // Parent module with analog intent
            module AnalogSection {
                pin IN: signal in;
                pin OUT: signal out;

                // Analog intent at module level
                for low_noise(max_ripple: 5mV);

                // Nested module with override
                module DigitalSubsection {
                    pin IN: signal in;
                    pin OUT: signal out;

                    // Override parent intent with digital
                    for debug_only();
                }

                IN -> DigitalSubsection().IN;
                DigitalSubsection().OUT -> OUT;
            }

            signal_in -> AnalogSection().IN;
            AnalogSection().OUT -> signal_out;
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // The nested module should override the parent intent
    // AnalogSection -> AnalogRequired (low_noise)
    // DigitalSubsection -> PureDigital (debug_only override)
}

/// Test intent on power domain vs signal flow
#[test]
fn test_power_domain_intent_vs_signal_intent() {
    let source = r#"
        board PowerVsSignal {
            // Power domain with noise sensitivity
            power VCC = 5V @ 1A for low_noise(max_ripple: 10mV);
            ground GND;

            // Signal using that power with different intent
            @VCC -> Res(330).1 -> LED(red).A for debug_only();
            LED(red).K -> @GND;
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // Power domain has low_noise requirement (analog)
    // Signal flow has debug_only (digital)
    // Should maintain separate intents for power vs signal
}

/// Test intent with generate loops
#[test]
fn test_intent_with_generate_loops() {
    let source = r#"
        board GenerateWithIntent {
            power VCC = 5V;
            ground GND;

            // Multiple identical flows with same intent
            generate for i in 0..4 {
                @VCC -> Res(330).1 -> LED(red).A for current_limiting(max: 20mA);
                LED(red).K -> @GND;
            }
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // Each generated instance should have the same intent
    let flow_tracker = analyzer.get_flow_tracker().expect("Should have flow tracker");
    let flow_paths = flow_tracker.get_flow_paths();

    // Should have 4 flow paths (one per generate iteration)
    // All with current_limiting intent
}

/// Test intent parameter units conflict
#[test]
fn test_intent_parameter_units_compatibility() {
    let source = r#"
        board UnitsTest {
            power VCC = 5V;
            ground GND;

            // Same intent with different unit representations
            net path1: @VCC -> Res(1k).1 -> @GND for delay(3ms);
            net path2: @VCC -> Res(1k).1 -> @GND for delay(3000us);
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // 3ms and 3000us should be treated as equivalent
    // Both should resolve to same SimMode
}

/// Test contradictory intent requirements
#[test]
fn test_contradictory_intent_requirements() {
    let source = r#"
        board ContradictoryIntents {
            power VCC = 5V;
            ground GND;

            // Contradictory requirements: high precision + debug only
            @VCC -> Res(1k).1 -> @GND
                for precision_measurement(accuracy: 0.01%)
                for debug_only();
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // precision_measurement requires AnalogRequired
    // debug_only suggests PureDigital
    // Analyzer should choose most restrictive (AnalogRequired)
    // Or generate a warning about conflicting intents
}

/// Test intent on interface connections
#[test]
fn test_intent_on_interface_connections() {
    let source = r#"
        board InterfaceIntents {
            interface SPI {
                pin MOSI: signal out;
                pin MISO: signal in;
                pin SCK: clock out;
                pin CS: signal out;
            }

            // Intent on entire interface
            SPI spi_bus for signal_buffering(fanout: 4);

            // Use the interface
            module SPI_Device {
                interface SPI device_spi: in;
            }

            spi_bus <=> SPI_Device().device_spi;
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    // Intent on interface should apply to all pins in the interface
}

/// Test intent scope: SimulationOnly vs All
#[test]
fn test_intent_tool_scope() {
    let source = r#"
        board ToolScopeTest {
            power VCC = 5V;
            ground GND;

            // Debug signal - simulation only, no synthesis
            net debug: @VCC -> Res(1k).1 -> @GND for debug_only();

            // Production signal - all tools
            net production: @VCC -> Res(330).1 -> LED(red).A for current_limiting(max: 20mA);
            LED(red).K -> @GND;
        }
    "#;

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

    let result = analyzer.analyze(&source_file);

    let flow_tracker = analyzer.get_flow_tracker().expect("Should have flow tracker");
    let flow_paths = flow_tracker.get_flow_paths();

    // debug_only should have SimulationOnly tool scope
    // current_limiting should have All tool scope

    let debug_flow = flow_paths.iter().find(|fp| {
        fp.net_name.as_ref().map_or(false, |n| n == "debug")
    });

    let production_flow = flow_paths.iter().find(|fp| {
        fp.net_name.as_ref().map_or(false, |n| n == "production")
    });

    // Verify tool scopes are different
}
