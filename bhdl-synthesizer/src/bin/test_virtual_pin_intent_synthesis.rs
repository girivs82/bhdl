// Test complete virtual pin intent-driven synthesis
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("Testing complete virtual pin intent-driven synthesis...");
    
    // Test module with virtual pins that should get intents from analyzer
    let content = r#"
module VirtualPinModule() {
    // Regular pins
    pin VIN: power in;
    pin GND: ground inout;
    
    // Virtual pins - should get default intents from analyzer
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
        return Ok(());
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
    
    // Show virtual pins found by analyzer
    println!("Virtual pins identified by analyzer:");
    for (node_ptr, scope) in &analysis.definition_scopes {
        for (pin_name, symbol) in scope.get_symbols() {
            if symbol.kind == bhdl_analyzer::symbol_table::SymbolKind::VirtualPin {
                println!("  {} - {:?} direction", pin_name, symbol.direction);
            }
        }
    }
    
    // Generate netlist with virtual pin synthesis
    println!("\nGenerating netlist with virtual pin synthesis...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    // Check the synthesis results
    println!("Synthesis results:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    println!("  Pins: {}", netlist.pins.len());
    
    // Show expanded components from virtual pins
    if !netlist.instances.is_empty() {
        println!("\nExpanded components from virtual pins:");
        for (instance_id, instance) in &netlist.instances {
            println!("  Instance: {}", instance.name);
            
            // Show attributes if any
            if !instance.attributes.is_empty() {
                for (attr_name, attr_value) in &instance.attributes {
                    println!("    {}: {}", attr_name, attr_value);
                }
            }
        }
    } else {
        println!("\nNo instances created (virtual pin expansion may not have been triggered)");
    }
    
    // Show created nets
    if !netlist.nets.is_empty() {
        println!("\nCreated nets:");
        for (net_id, net) in &netlist.nets {
            let net_name = net.name.as_ref().map(|s| s.as_str()).unwrap_or("unnamed");
            println!("  Net: {} (class: {:?})", net_name, net.net_class);
        }
    }
    
    // Show pins
    if !netlist.pins.is_empty() {
        println!("\nModule pins:");
        for (pin_id, pin) in &netlist.pins {
            println!("  Pin: {} ({:?} {:?})", pin.name, pin.pin_type, pin.direction);
        }
    }
    
    // Show flow tracker results if available
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        let flow_paths = flow_tracker.get_flow_paths();
        println!("\nFlow paths with intents: {}", flow_paths.len());
        
        for flow in flow_paths {
            if let Some(ref intent) = flow.intent {
                println!("  Flow {}: nets={:?}, intent={}", 
                         flow.id, flow.nets, intent.name);
                
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
                
                // Show intent resolution result
                if let Some(ref result) = flow.intent_result {
                    println!("    Resolved to: {:?} simulation mode", result.sim_mode);
                }
            }
        }
    } else {
        println!("\nNo flow tracker available");
    }
    
    println!("\n✓ Complete virtual pin intent-driven synthesis test completed");
    
    Ok(())
}