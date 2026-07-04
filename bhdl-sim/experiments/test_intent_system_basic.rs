//! Basic test of intent system functionality

use bhdl_parser;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_stdlib::intents;

fn main() {
    println!("BHDL Intent System Basic Test");
    println!("==============================\n");
    
    // Initialize intent registry to show available intents
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    println!("✓ Loaded stdlib intent functions");
    println!("  Available intents: {:?}\n", registry.registered_intents());
    
    // Test 1: SPICE intent
    println!("Test 1: SPICE Intent for Analog Simulation");
    println!("------------------------------------------");
    test_intent(r#"
        board TestBoard {
            power VCC = 5V;
            ground GND;
            
            // Flow with SPICE intent
            net critical_path: VCC -> Res(10k).1 -> LED(red).A for accuracy: use spice;
            LED.K -> GND;
        }
    "#, "SPICE", SimMode::AnalogRequired);
    
    // Test 2: Digital intent  
    println!("\nTest 2: Digital Intent for Speed");
    println!("---------------------------------");
    test_intent(r#"
        board TestBoard {
            // Digital logic flow
            net digital_path: clk -> Buffer().IN -> Buffer().OUT for speed: use digital;
        }
    "#, "Digital", SimMode::PureDigital);
    
    // Test 3: Timing intent
    println!("\nTest 3: Digital with Timing Intent");
    println!("-----------------------------------");
    test_intent(r#"
        board TestBoard {
            // Timing-critical path
            net timing_path: data_in -> DFF().D -> DFF().Q for timing_analysis: use digital with propagation_delays;
        }
    "#, "Timed Digital", SimMode::DigitalWithTiming);
    
    // Test 4: Mixed signal intent
    println!("\nTest 4: Mixed Signal Intent");
    println!("----------------------------");
    test_intent(r#"
        board TestBoard {
            // Mixed analog/digital
            net mixed_path: analog_in -> ADC().IN -> ADC().OUT for mixed_signal: use mixed;
        }
    "#, "Mixed Signal", SimMode::MixedSignal);
    
    println!("\n✓ All intent tests completed!");
}

fn test_intent(source: &str, intent_name: &str, expected_mode: SimMode) {
    // Parse the source
    let parse_result = bhdl_parser::parse(source);
    if !parse_result.errors().is_empty() {
        println!("  ✗ Parse error: {:?}", parse_result.errors());
        return;
    }
    
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be SourceFile");
    
    // Run analysis (which includes flow tracking)
    let analysis_result = analyze(&source_file);
    
    // Check for diagnostics (treating all as potential errors for now)
    if !analysis_result.diagnostics.is_empty() {
        println!("  ⚠ Analysis diagnostics: {} messages", analysis_result.diagnostics.len());
        for diag in &analysis_result.diagnostics {
            println!("    - {}", diag.message);
        }
    }
    
    println!("  ✓ Parsed and analyzed successfully");
    
    // Check flow tracker results
    if let Some(ref flow_tracker) = analysis_result.flow_tracker {
        let required_mode = flow_tracker.get_required_sim_mode();
        println!("  ✓ Flow tracker detected simulation mode: {:?}", required_mode);
        
        if required_mode == expected_mode {
            println!("  ✓ {} intent correctly resolved to {:?}", intent_name, expected_mode);
        } else {
            println!("  ✗ Expected {:?}, but got {:?}", expected_mode, required_mode);
        }
        
        // Show flow paths
        let flow_paths = flow_tracker.get_flow_paths();
        if !flow_paths.is_empty() {
            println!("  ✓ Found {} flow path(s) with intents", flow_paths.len());
            for path in flow_paths {
                if let Some(ref intent) = path.intent {
                    println!("    - Flow {}: intent '{}' with {} params", 
                             path.id, intent.name, intent.params.len());
                }
            }
        }
    } else {
        println!("  ✗ No flow tracker available");
    }
}