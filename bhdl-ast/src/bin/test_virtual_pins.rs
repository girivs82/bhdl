// Test virtual pin AST functionality
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile, HasName};

fn main() {
    let content = r#"
entity TestVirtual() {
    pin VIN: power in;
    pin VOUT: virtual power out;
    pin GND: ground inout;
}
"#;

    let parsed = parse(content);
    if !parsed.errors().is_empty() {
        println!("Parse errors:");
        for error in parsed.errors() {
            println!("  - {}", error.message);
        }
        return;
    }

    let source_file = SourceFile::cast(parsed.syntax()).unwrap();
    
    for entity in source_file.entities() {
        println!("Entity: {}", entity.name().unwrap().text());

        for pin in entity.pins() {
            let pin_name = pin.name().unwrap().text().to_string();
            let is_virtual = pin.is_virtual();
            let pin_type = pin.pin_type().map(|t| t.text().to_string()).unwrap_or("unknown".to_string());
            let direction = pin.direction().map(|t| t.text().to_string()).unwrap_or("none".to_string());
            
            println!("  Pin: {} - {} {} - Virtual: {}", 
                     pin_name, pin_type, direction, is_virtual);
        }
    }
}