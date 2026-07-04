//! Test program for flow tracking and intent resolution

use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() {
    // Test BHDL code with intent clauses
    let bhdl_code = r#"
board PowerSupply {
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    ground GND;
    
    // Delay intent on a net
    net delayed_trigger: VIN -> Res(10k).1 -> Cap(100n).1 -> GND for delay(3ms);
    
    // Protection intent
    net protected_vin: VIN -> TVSDiode(15V).K for overvoltage_protection(15V);
    
    // Anti-alias filter before ADC
    net adc_input: sensor_out -> Res(1k).1 -> Cap(10n).1 -> GND for anti_alias(before: ADC1, cutoff: 10kHz);
    
    // Low noise power rail
    net clean_vcc: VCC -> L1(10uH).1 -> Cap(100uF).1 -> GND for low_noise(max_ripple: 1mV);
    
    // Debounced switch input
    net switch_signal: SW1.1 -> Res(10k).1 -> MCU.GPIO1 for debounce(SW1, time: 20ms);
}
"#;

    println!("Parsing BHDL code with intent clauses...\n");
    
    let parse_result = parse(bhdl_code);
    let syntax_node = parse_result.syntax();
    
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {:?}", error);
        }
        return;
    }
    
    let source_file = match SourceFile::cast(syntax_node) {
        Some(sf) => sf,
        None => {
            println!("Failed to create SourceFile AST node");
            return;
        }
    };
    
    println!("Running analysis with flow tracking...\n");
    let analysis_result = analyze(&source_file);
    
    // Print diagnostics
    if !analysis_result.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  - {}", diag.message);
        }
        println!();
    }
    
    // Print flow tracking results
    if let Some(ref flow_tracker) = analysis_result.flow_tracker {
        println!("Flow Tracking Results:");
        println!("=====================");
        
        let flow_paths = flow_tracker.get_flow_paths();
        println!("Found {} flow paths with intents\n", flow_paths.len());
        
        for (i, flow_path) in flow_paths.iter().enumerate() {
            println!("Flow Path {}:", i + 1);
            println!("  Nets: {:?}", flow_path.nets);
            println!("  Components: {:?}", flow_path.components);
            
            if let Some(ref intent) = flow_path.intent {
                println!("  Intent: {}", intent.name);
                if !intent.params.is_empty() {
                    println!("  Parameters: {:?}", intent.params);
                }
            }
            
            if let Some(ref result) = flow_path.intent_result {
                println!("  Simulation Mode: {:?}", result.sim_mode);
                if !result.synthesis_hints.is_empty() {
                    println!("  Synthesis Hints:");
                    for hint in &result.synthesis_hints {
                        println!("    - {:?}", hint);
                    }
                }
                if !result.validation_rules.is_empty() {
                    println!("  Validation Rules:");
                    for rule in &result.validation_rules {
                        println!("    - {}: {}", rule.condition, rule.error_message);
                    }
                }
            }
            println!();
        }
        
        let required_mode = flow_tracker.get_required_sim_mode();
        println!("Overall Required Simulation Mode: {:?}", required_mode);
        
        // Test querying simulation modes for specific nets
        println!("\nQuerying simulation modes for specific nets:");
        println!("  delayed_trigger: {:?}", flow_tracker.get_net_sim_mode("delayed_trigger"));
        println!("  protected_vin: {:?}", flow_tracker.get_net_sim_mode("protected_vin"));
        println!("  adc_input: {:?}", flow_tracker.get_net_sim_mode("adc_input"));
        println!("  clean_vcc: {:?}", flow_tracker.get_net_sim_mode("clean_vcc"));
        println!("  switch_signal: {:?}", flow_tracker.get_net_sim_mode("switch_signal"));
    } else {
        println!("No flow tracking data available");
    }
}