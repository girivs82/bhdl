//! Simple test program to validate intent system functionality

use bhdl_parser;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::{SymbolTable, Analyzer, AnalysisContext};
use bhdl_common::{IntentRegistry, SimMode};
use bhdl_stdlib::intents;
use std::sync::Arc;

fn main() {
    println!("BHDL Intent System Validation");
    println!("=============================\n");
    
    // Initialize intent system
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    println!("✓ Loaded stdlib intent functions");
    println!("  Available intents: {:?}", registry.registered_intents());
    
    // Test 1: Simple SPICE intent
    println!("\n1. Testing SPICE Intent:");
    test_spice_intent();
    
    // Test 2: Digital intent  
    println!("\n2. Testing Digital Intent:");
    test_digital_intent();
    
    // Test 3: Mixed signal intent
    println!("\n3. Testing Mixed Signal Intent:");
    test_mixed_signal_intent();
    
    println!("\n✓ All intent validation tests completed!");
}

fn test_spice_intent() {
    let source = r#"
        board TestBoard {
            power VCC = 5V;
            ground GND;
            
            module AnalogSection {
                pin IN: signal in;
                pin OUT: signal out;
                
                // SPICE simulation intent
                for accuracy: use spice;
            }
            
            VCC -> AnalogSection().IN;
            AnalogSection().OUT -> test_out;
        }
    "#;
    
    // Parse and analyze
    let parse_result = bhdl_parser::parse(source);
    if !parse_result.errors().is_empty() {
        println!("  ✗ Parse errors: {:?}", parse_result.errors());
        return;
    }
    println!("  ✓ Parsed successfully");
    
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be SourceFile");
    
    // Analyze with intent support
    let mut symbol_table = SymbolTable::new();
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    
    let mut analyzer = Analyzer::new(&mut symbol_table);
    analyzer.set_intent_registry(Arc::new(registry));
    
    let result = analyzer.analyze(&source_file);
    if result.diagnostics.iter().any(|d| d.is_error()) {
        println!("  ✗ Analysis errors: {:?}", result.diagnostics);
        return;
    }
    println!("  ✓ Analyzed successfully");
    
    // Check flow tracker
    if let Some(flow_tracker) = analyzer.get_flow_tracker() {
        // Check if the flow tracker has the expected simulation mode
        let required_mode = flow_tracker.get_required_sim_mode();
        println!("  ✓ Required simulation mode: {:?}", required_mode);
        
        if required_mode == SimMode::AnalogRequired {
            println!("  ✓ SPICE intent correctly resolved to AnalogRequired");
        } else {
            println!("  ✗ Expected AnalogRequired, got {:?}", required_mode);
        }
    } else {
        println!("  ✗ No flow tracker available");
    }
}

fn test_digital_intent() {
    let source = r#"
        board TestBoard {
            module DigitalLogic {
                pin CLK: clock in;
                pin DATA: signal in;
                pin OUT: signal out;
                
                // Digital simulation intent
                for speed: use digital;
            }
            
            clk_source -> DigitalLogic().CLK;
            data_in -> DigitalLogic().DATA;
            DigitalLogic().OUT -> data_out;
        }
    "#;
    
    // Parse and analyze
    let parse_result = bhdl_parser::parse(source);
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be SourceFile");
    
    let mut symbol_table = SymbolTable::new();
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    
    let mut analyzer = Analyzer::new(&mut symbol_table);
    analyzer.set_intent_registry(Arc::new(registry));
    
    let result = analyzer.analyze(&source_file);
    
    if let Some(flow_tracker) = analyzer.get_flow_tracker() {
        let required_mode = flow_tracker.get_required_sim_mode();
        println!("  ✓ Required simulation mode: {:?}", required_mode);
        
        if required_mode == SimMode::PureDigital {
            println!("  ✓ Digital intent correctly resolved to PureDigital");
        } else {
            println!("  ✗ Expected PureDigital, got {:?}", required_mode);
        }
    }
}

fn test_mixed_signal_intent() {
    let source = r#"
        board TestBoard {
            module MixedModule {
                pin ANALOG_IN: signal in;
                pin DIGITAL_OUT: signal out;
                
                // Mixed signal simulation intent
                for mixed_signal: use mixed;
            }
            
            analog_input -> MixedModule().ANALOG_IN;
            MixedModule().DIGITAL_OUT -> digital_output;
        }
    "#;
    
    // Parse and analyze
    let parse_result = bhdl_parser::parse(source);
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be SourceFile");
    
    let mut symbol_table = SymbolTable::new();
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    
    let mut analyzer = Analyzer::new(&mut symbol_table);
    analyzer.set_intent_registry(Arc::new(registry));
    
    let result = analyzer.analyze(&source_file);
    
    if let Some(flow_tracker) = analyzer.get_flow_tracker() {
        let required_mode = flow_tracker.get_required_sim_mode();
        println!("  ✓ Required simulation mode: {:?}", required_mode);
        
        if required_mode == SimMode::MixedSignal {
            println!("  ✓ Mixed signal intent correctly resolved to MixedSignal");
        } else {
            println!("  ✗ Expected MixedSignal, got {:?}", required_mode);
        }
    }
}