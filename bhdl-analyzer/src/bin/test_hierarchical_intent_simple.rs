// Test hierarchical intent propagation through module instances
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Hierarchical Intent Propagation (Simple)\n");
    
    let test_bhdl = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Main signal flow with analog intent
    net audio_in: @VCC -> Res(10k).1 -> Cap(100nF).1 for analog(bandwidth: 10kHz);
    
    // Continue the flow
    audio_in -> LED(red).A;
    LED(red).K -> @GND;
}
"#;
    
    // Parse
    let parse_result = parse(test_bhdl);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    let source_file = SourceFile::cast(parse_result.syntax()).expect("Should be a SourceFile");
    
    // Analyze
    let result = analyze(&source_file);
    
    // Check flow tracking results
    println!("=== Flow Tracking Results ===");
    if let Some(ref flow_tracker) = result.flow_tracker {
        println!("Flow paths found: {}", flow_tracker.get_flow_paths().len());
        for (i, flow) in flow_tracker.get_flow_paths().iter().enumerate() {
            println!("\nFlow Path {}:", i + 1);
            println!("  Nets: {:?}", flow.nets);
            println!("  Components: {:?}", flow.components);
            if let Some(ref intent) = flow.intent {
                println!("  Intent: {}({:?})", intent.name, intent.params);
            }
            if let Some(ref result) = flow.intent_result {
                println!("  Sim Mode: {:?}", result.sim_mode);
            }
        }
        
        println!("\nRequired simulation mode: {:?}", flow_tracker.get_required_sim_mode());
    } else {
        println!("No flow tracker available");
    }
    
    // Show diagnostics
    println!("\n=== Diagnostics ===");
    if result.diagnostics.is_empty() {
        println!("No diagnostics");
    } else {
        for diag in &result.diagnostics {
            if !diag.message.contains("Component Inference") {
                println!("  - {}", diag.message);
            }
        }
    }
}