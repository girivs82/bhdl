// Test hierarchical intent propagation through module instances
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Hierarchical Intent Propagation with Modules\n");
    
    let test_bhdl = r#"
// Simple filter module
module Filter(cutoff: frequency = 1kHz) {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: signal inout;
    
    IN -> Cap(100nF).1 -> OUT;
    Cap(100nF).2 -> GND;
}

board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Module instance
    filter1: Filter(cutoff=10kHz) {
        IN <- input_signal;
        OUT -> output_net;
        GND <- gnd_net;
    }
    
    // Flow with intent that should propagate to module
    net signal_path: @VCC -> input_signal for analog(bandwidth: 20kHz);
    input_signal -> filter1.IN;
    
    // Output
    output_net -> LED(red).A;
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
        
        // Check if filter1 module instance has inherited intent
        println!("\n=== Hierarchical Intent Check ===");
        if let Some(mode) = flow_tracker.get_component_sim_mode("filter1") {
            println!("Module instance 'filter1' sim mode: {:?}", mode);
        } else {
            println!("Module instance 'filter1' has no sim mode assigned");
        }
        
        // Check for propagated flow paths
        let propagated_flows = flow_tracker.get_flow_paths().iter()
            .filter(|f| f.nets.iter().any(|n| n.contains("._internal")))
            .count();
        println!("Propagated flow paths: {}", propagated_flows);
        
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