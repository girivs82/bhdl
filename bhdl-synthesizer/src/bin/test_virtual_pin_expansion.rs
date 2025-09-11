// Test virtual pin expansion in synthesizer
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;
use bhdl_synthesizer::NetlistGenerator;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();
    
    println!("Testing virtual pin expansion in synthesizer...");
    
    // Test module with virtual pins
    let content = r#"
module TestVirtualModule() {
    // Regular pins
    pin VIN: power in;
    pin GND: ground inout;
    
    // Virtual pins - should be expanded
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
    let analysis = analyze(&source_file);
    
    if !analysis.diagnostics.is_empty() {
        println!("Analysis diagnostics:");
        for diag in &analysis.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    // Generate netlist with virtual pin expansion
    println!("\nGenerating netlist with virtual pin expansion...");
    let mut generator = NetlistGenerator::new();
    let netlist = generator.generate_from_ast_and_analysis(&source_file, &analysis).await?;
    
    // Check the results
    println!("Generated netlist:");
    println!("  Modules: {}", netlist.modules.len());
    println!("  Instances: {}", netlist.instances.len());
    println!("  Nets: {}", netlist.nets.len());
    println!("  Pins: {}", netlist.pins.len());
    
    // Look for expanded components from virtual pins
    for (instance_id, instance) in &netlist.instances {
        println!("  Instance: {} (module: {:?})", 
                 instance.name, 
                 instance.definition);
    }
    
    // Look for created nets
    for (net_id, net) in &netlist.nets {
        println!("  Net: {} (class: {:?})", net.name.as_ref().unwrap_or(&"unnamed".to_string()), net.net_class);
    }
    
    // Look for pins
    for (pin_id, pin) in &netlist.pins {
        println!("  Pin: {} (type: {:?}, direction: {:?})", 
                 pin.name,
                 pin.pin_type,
                 pin.direction);
    }
    
    println!("✓ Virtual pin expansion test completed");
    Ok(())
}