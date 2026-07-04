use bhdl_parser::parse;
use std::fs;

fn main() {
    let file = std::env::args().nth(1).unwrap_or_else(|| "test_led_parsing.bhdl".to_string());
    let content = fs::read_to_string(&file).expect("Failed to read file");
    let parsed = parse(&content);
    
    println!("=== Parsing const declarations test ===");
    println!("Content:\n{}", content);
    println!("\nParsed successfully: {}", parsed.errors().is_empty());
    
    if !parsed.errors().is_empty() {
        println!("Errors:");
        for err in parsed.errors() {
            println!("  - {:?}", err);
        }
    } else {
        println!("\nSyntax tree:");
        println!("{}", parsed.syntax());
    }
}