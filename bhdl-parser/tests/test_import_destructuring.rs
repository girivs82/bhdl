//! Test destructuring import syntax

use bhdl_parser::parse;

#[test]
fn test_destructuring_import() {
    let source = r#"
import { ResistorParams, RESISTOR_0402_PARAMS } from "../electrical_params.bhdl";
import { CapacitorParams, CAPACITOR_0402_PARAMS } from "../electrical_params.bhdl";
import simple.path.module;
    "#;
    
    println!("=== Testing Import Syntax ===");
    println!("Source:\n{}", source);
    
    let parse_result = parse(source);
    
    // Check for errors
    let errors = parse_result.errors();
    if !errors.is_empty() {
        println!("\nParse errors:");
        for err in errors {
            println!("  - {}", err.message);
        }
    }
    
    // The parser should now handle destructuring imports
    assert!(errors.is_empty(), "Parser should handle destructuring imports without errors");
    
    println!("\nParse successful!");
}

#[test] 
fn test_stdlib_imports() {
    // Test actual stdlib import patterns
    let source = r#"
import { ResistorParams, RESISTOR_0402_PARAMS, RESISTOR_0603_PARAMS } from "../electrical_params.bhdl";

module Resistor(value: resistance, package: string = "0402") {
    pin 1: signal inout;
    pin 2: signal inout;
}
    "#;
    
    let parse_result = parse(source);
    let errors = parse_result.errors();
    
    if !errors.is_empty() {
        println!("\nErrors parsing stdlib-style import:");
        for err in errors {
            println!("  - {}", err.message);
        }
    }
    
    assert!(errors.is_empty(), "Should parse stdlib import patterns");
}