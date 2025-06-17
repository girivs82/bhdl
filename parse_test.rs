// Simple parser test
use std::fs;

fn main() {
    let content = fs::read_to_string("test_7805_regulator_realistic.bhdl").unwrap();
    let result = bhdl_parser::parse(&content);
    
    println!("Parse errors: {}", result.errors().len());
    for err in result.errors() {
        println!("  - {}", err.message);
    }
    
    if result.errors().is_empty() {
        println!("✅ Parse successful!");
        let root = result.syntax();
        println!("Root kind: {:?}", root.kind());
        println!("Children: {}", root.children().count());
    }
}