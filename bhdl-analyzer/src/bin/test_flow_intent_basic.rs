// Test basic flow tracking with intent
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Basic Flow Tracking with Intent\n");
    
    let test_bhdl = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Flow with intent
    net test_flow: @VCC -> Res(10k).1 -> Cap(100nF).1 for delay(5ms);
    
    // Continue flow
    Cap(100nF).2 -> @GND;
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
        
        // Check specific component modes
        for comp in ["Res", "Cap"] {
            if let Some(mode) = flow_tracker.get_component_sim_mode(comp) {
                println!("{} sim mode: {:?}", comp, mode);
            }
        }
    } else {
        println!("No flow tracker available");
    }
    
    // Show diagnostics
    println!("\n=== Diagnostics ===");
    let non_inference_diags: Vec<_> = result.diagnostics.iter()
        .filter(|d| !d.message.contains("Component Inference"))
        .collect();
    
    if non_inference_diags.is_empty() {
        println!("No diagnostics");
    } else {
        for diag in &non_inference_diags {
            println!("  - {}", diag.message);
        }
    }
}