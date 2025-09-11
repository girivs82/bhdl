// Test board-level virtual pin intent synthesis with flow statements
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("Testing board-level virtual pin intent-driven synthesis...");
    
    // Test board that instantiates modules with virtual pins and uses flow intent
    let content = r#"
board TestBoard {
    // Power domains
    power VCC = 5V @ 1A;
    ground GND;
    
    // Flow with intent - should propagate to virtual pins
    net protection_flow: VCC -> filter: FilterModule().VIN -> filter.VOUT -> load: LoadModule().VIN
        for input_protection(6V, 500mA);
        
    net signal_flow: filter.SIGNAL_OUT -> amplifier: AmpModule().SIGNAL_IN -> amplifier.SIGNAL_OUT
        for signal_amplification(10dB, 100kHz);
    
    // Ground connections
    GND <-> filter.GND;
    GND <-> load.GND;
    GND <-> amplifier.GND;
}

module FilterModule() {
    pin VIN: power in;
    pin GND: ground inout;
    
    // Virtual pins for protection and filtering
    pin VOUT: virtual power out;
    pin SIGNAL_OUT: virtual signal out;
}

module LoadModule() {
    pin VIN: power in;
    pin GND: ground inout;
}

module AmpModule() {
    pin SIGNAL_IN: virtual signal in;
    pin SIGNAL_OUT: virtual signal out;
    pin GND: ground inout;
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
    
    println!("Running board-level analysis with virtual pin intent resolution...");
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
    println!("\nGenerating board netlist with virtual pin synthesis...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    // Check the synthesis results
    println!("Board synthesis results:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    println!("  Pins: {}", netlist.pins.len());
    
    // Show module instances created
    if !netlist.instances.is_empty() {
        println!("\nModule instances created:");
        for (_instance_id, instance) in &netlist.instances {
            println!("  Instance: {} (module: {})", instance.name, 
                     netlist.modules.get(instance.definition).map(|m| &m.name).unwrap_or(&"unknown".to_string()));
        }
    }
    
    // Show expanded components from virtual pins
    if netlist.instances.len() > 3 { // More than just the 3 module instances
        println!("\nExpanded components from virtual pins:");
        let mut component_count = 0;
        for (_instance_id, instance) in &netlist.instances {
            if !instance.name.contains("Module") { // Skip module instances
                component_count += 1;
                println!("  Component: {}", instance.name);
                
                // Show attributes if any
                if !instance.attributes.is_empty() {
                    for (attr_name, attr_value) in &instance.attributes {
                        println!("    {}: {}", attr_name, attr_value);
                    }
                }
            }
        }
        if component_count == 0 {
            println!("  No component expansion detected");
        }
    }
    
    // Show created nets
    if !netlist.nets.is_empty() {
        println!("\nBoard nets created:");
        for (_net_id, net) in &netlist.nets {
            let net_name = net.name.as_ref().map(|s| s.as_str()).unwrap_or("unnamed");
            println!("  Net: {} (class: {:?})", net_name, net.net_class);
        }
    }
    
    // Show flow tracker results if available
    if let Some(ref flow_tracker) = analysis.flow_tracker {
        let flow_paths = flow_tracker.get_flow_paths();
        println!("\nBoard flow paths with intents: {}", flow_paths.len());
        
        for flow in flow_paths {
            if let Some(ref intent) = flow.intent {
                println!("  Flow {}: {} nets, intent: {}", 
                         flow.id, flow.nets.len(), intent.name);
                
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
    
    println!("\n✓ Board-level virtual pin intent-driven synthesis test completed");
    
    Ok(())
}