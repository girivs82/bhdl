use std::fs;
use bhdl_parser::parse;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let content = fs::read_to_string("test_array_pin_access.bhdl")?;
    let parse_result = parse(&content);
    
    let errors = parse_result.errors();
    if \!errors.is_empty() {
        println\!("Parse errors found:");
        for error in errors {
            println\!("  - {}", error.message);
        }
    } else {
        println\!("No parse errors - checking syntax tree...");
        
        // Check for specific patterns
        let syntax = parse_result.syntax();
        let text = format\!("{:?}", syntax);
        
        if text.contains("ERROR") {
            println\!("Found ERROR nodes in syntax tree\!");
        } else {
            println\!("✅ Array pin access parsing successful\!");
        }
    }
    
    Ok(())
}
