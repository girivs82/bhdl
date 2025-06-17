use std::fs;
use bhdl_parser::parse;

fn main() {
    let content = fs::read_to_string("examples/linear_regulator.bhdl").unwrap();
    let parsed = parse(&content);
    
    println!("Parse errors: {}", parsed.errors().len());
    for error in parsed.errors() {
        println!("  - {}", error.message);
    }
    
    println!("Syntax tree:");
    println!("{:#?}", parsed.syntax());
}