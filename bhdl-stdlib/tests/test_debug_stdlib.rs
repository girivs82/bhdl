//! Debug test to understand why pins aren't being found

use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode, Module, HasName};

#[test]
fn test_parse_actual_resistor_file() -> Result<()> {
    // Read the actual resistor.bhdl file
    let resistor_path = format!("{}/passives/resistor.bhdl", env!("CARGO_MANIFEST_DIR"));
    let source = std::fs::read_to_string(&resistor_path)?;
    
    println!("=== Parsing actual resistor.bhdl ===");
    println!("First 500 chars of source:\n{}\n...", &source[..500.min(source.len())]);
    
    let parse_result = parse(&source);
    let syntax_node = parse_result.syntax();
    
    // Check for parse errors
    let errors = parse_result.errors();
    if !errors.is_empty() {
        println!("Parse errors:");
        for err in errors {
            println!("  {:?}", err);
        }
    }
    
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Find modules
    let mut module_count = 0;
    for item in source_file.items() {
        if let Some(module) = Module::cast(item.syntax().clone()) {
            module_count += 1;
            let name = module.name().map(|n| n.text().to_string()).unwrap_or_default();
            println!("\nFound module: {}", name);
            
            // Print module syntax
            let module_text = module.syntax().text().to_string();
            println!("Module text (first 200 chars):\n{}", &module_text[..200.min(module_text.len())]);
            
            // Count pins
            let pins: Vec<_> = module.pins().collect();
            println!("Pin count: {}", pins.len());
            
            // Print each pin's syntax
            for (i, pin) in pins.iter().enumerate() {
                println!("  Pin {}: '{}'", i, pin.syntax().text());
                if let Some(name) = pin.name() {
                    println!("    Name: '{}'", name.text());
                }
            }
        }
    }
    
    println!("\nTotal modules found: {}", module_count);
    
    Ok(())
}