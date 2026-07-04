// Test flow intent parsing and analysis
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    println!("Testing Flow Intent Analysis\n");
    
    let test_bhdl = r#"
board TestBoard {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Named flow with intent
    critical_path: @VCC -> Res(10k).1 -> led.A for delay(3ms);
    
    // Direct connection with intent
    @VCC -> Cap(100n).1 -> @GND for decoupling();
    
    // Regular connection without intent
    @VCC -> Res(1k).1 -> status_led.A;
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
    
    // Show diagnostics
    println!("=== Analysis Diagnostics ===");
    if result.diagnostics.is_empty() {
        println!("No diagnostics");
    } else {
        for diag in &result.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    // Show flow tracking results
    println!("\n=== Flow Tracking Results ===");
    
    if let Some(flow_tracker) = &result.flow_tracker {
        let flow_paths = flow_tracker.get_flow_paths();
        println!("Total flow paths: {}", flow_paths.len());
        
        for (i, flow) in flow_paths.iter().enumerate() {
        println!("\nFlow path {}:", i + 1);
        println!("  ID: {}", flow.id);
        println!("  Nets: {}", flow.nets.join(", "));
        println!("  Components: {}", flow.components.join(" -> "));
        
        if let Some(intent) = &flow.intent {
            print!("  Intent: {}(", intent.name);
            let mut first = true;
            for param in &intent.params {
                if !first {
                    print!(", ");
                }
                first = false;
                match param {
                    bhdl_common::IntentParam::Positional(val) => {
                        print!("{:?}", val);
                    }
                    bhdl_common::IntentParam::Named(name, val) => {
                        print!("{}: {:?}", name, val);
                    }
                }
            }
            println!(")");
        } else {
            println!("  Intent: None");
        }
        
        if let Some(result) = &flow.intent_result {
            println!("  Intent result: {:?}", result);
        }
    }
        
        // Show required simulation mode
        println!("\n=== Simulation Mode ===");
        println!("Required mode: {:?}", flow_tracker.get_required_sim_mode());
    } else {
        println!("No flow tracking results available");
    }
}