use bhdl_parser::parse;

fn main() {
    let source = r#"
import { ResistorParams, RESISTOR_0402_PARAMS } from "../electrical_params.bhdl";
import simple.path;
    "#;
    
    println!("=== Testing Import Parsing ===");
    println!("Source:\n{}", source);
    
    let parse_result = parse(source);
    
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
    
    // Print the syntax tree
    println!("\nSyntax tree:");
    println!("{:#?}", parse_result.syntax());
}