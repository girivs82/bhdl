// Test virtual pin intent resolution in analyzer
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing virtual pin intent resolution...");
    
    // Test module with virtual pins
    let content = r#"
module TestVirtualPins() {
    // Regular pins
    pin VIN: power in;
    pin GND: ground inout;
    
    // Virtual pins - should get default intents
    pin VOUT: virtual power out;
    pin SIGNAL_OUT: virtual signal out;
    pin BIDIR: virtual signal inout;
    pin GND_OUT: virtual ground out;
}
"#;

    // Parse and analyze
    let parsed = parse(content);
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return;
    }

    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    println!("Running analysis with virtual pin intent resolution...");
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis.diagnostics {
            println!("  - {}", diag.message);
        }
        println!();
    }
    
    // Check for virtual pins in symbol table
    println!("Virtual pins found in analysis:");
    for (node_ptr, scope) in &analysis.definition_scopes {
        for (pin_name, symbol) in scope.get_symbols() {
            if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::VirtualPin {
                println!("  {} - {:?} {:?}", 
                         pin_name, 
                         symbol.direction,
                         "virtual pin with intent");
            }
        }
    }
    
    // Check if flow tracker was created with virtual pin flows
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        let flow_paths = flow_tracker.get_flow_paths();
        println!("Flow paths created: {}", flow_paths.len());
        
        for flow in flow_paths {
            if let Some(ref intent) = flow.intent {
                println!("  Flow {}: {} nets, intent: {}", 
                         flow.id,
                         flow.nets.len(),
                         intent.name);
                
                // Show intent parameters
                for param in &intent.params {
                    match param {
                        bhdl_common::IntentParam::Named(name, value) => {
                            println!("    {}: {:?}", name, value);
                        }
                        bhdl_common::IntentParam::Positional(value) => {
                            println!("    positional: {:?}", value);
                        }
                    }
                }
            }
        }
    } else {
        println!("No flow tracker available");
    }
    
    println!("✓ Virtual pin intent resolution test completed");
}