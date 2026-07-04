//! Test that pins are properly parsed and extracted

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Entity, HasName};

fn main() -> Result<()> {
    env_logger::init();
    
    let source = r#"
entity TestModule {
    pin test_pin: signal inout;
    pin power_pin: power in;
    pin gnd_pin: ground;
}

entity Resistor(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
}
"#;

    println!("=== Testing Pin Parsing ===\n");
    println!("Source:\n{}", source);
    
    let parse_result = parse(source);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Find all modules
    for item in source_file.items() {
        if let Some(entity) = Entity::cast(item.syntax().clone()) {
            println!("\nEntity: {}", entity.name().map(|n| n.text().to_string()).unwrap_or_default());

            // Try to get pins
            let pins: Vec<_> = entity.pins().collect();
            println!("  Found {} pins", pins.len());
            
            for pin in pins {
                println!("    Pin: {}", pin.name().map(|n| n.text().to_string()).unwrap_or_default());
                // Print the full pin syntax for debugging
                println!("      Syntax: '{}'", pin.syntax().text());
            }
            
            // Also try ports for comparison
            let ports: Vec<_> = entity.ports().collect();
            println!("  Found {} ports", ports.len());
        }
    }
    
    // Now test with the stdlib reader
    println!("\n=== Testing StdLib Reader ===");
    
    use bhdl_stdlib::{StdlibReader, get_default_stdlib_path};
    
    // Create a test stdlib reader
    let mut reader = StdlibReader::new("/tmp/test_stdlib");
    
    // Manually add our test module
    std::fs::create_dir_all("/tmp/test_stdlib/test").ok();
    std::fs::write("/tmp/test_stdlib/test/resistor.bhdl", source)?;
    
    // Load it
    reader.load_all_components().ok();
    
    // Try to get the Resistor module
    if let Some(resistor) = reader.get_component("Resistor") {
        println!("\nFound Resistor module with {} pins:", resistor.pins.len());
        for pin in &resistor.pins {
            println!("  - {} ({:?}, {:?})", pin.name, pin.direction, pin.pin_type);
        }
    }
    
    Ok(())
}