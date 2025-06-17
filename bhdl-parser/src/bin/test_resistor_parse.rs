use bhdl_parser::parse;

fn main() {
    let simple_resistor = r#"
module Res(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
}
    "#;
    
    println!("=== Testing Simple Resistor Module ===");
    println!("Source:\n{}", simple_resistor);
    
    let parse_result = parse(simple_resistor);
    
    // Check errors
    let errors = parse_result.errors();
    if !errors.is_empty() {
        println!("\nErrors:");
        for err in errors {
            println!("  - {}", err.message);
        }
    } else {
        println!("\nNo errors!");
    }
    
    // Print syntax tree for debugging
    println!("\nSyntax tree:");
    println!("{:#?}", parse_result.syntax());
}