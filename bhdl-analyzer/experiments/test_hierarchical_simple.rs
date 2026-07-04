use std::fs;
use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

fn main() {
    println!("=== Simple Hierarchical Entity Test ===\n");
    
    // Test case: Simple hierarchy with entity parameters
    let code = r#"
entity SimplePWM(frequency: frequency = 100kHz) {
    pin VCC: power in;
    pin OUT: signal out;
}

entity SimpleRegulator {
    pin VIN: power in;
    pin VOUT: power out;

    // Entity instance with parameter override
    pwm1: SimplePWM(frequency=500kHz) {
        VCC <- VIN;
        OUT -> switch_node;
    }
    
    // Connection
    switch_node -> VOUT;
}

board SimpleBoard {
    power VDD = 5V;
    
    reg1: SimpleRegulator {
        VIN <- VDD;
        VOUT -> output;
    }
}
"#;

    println!("1. Parsing...");
    let parse_result = parse(code);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    println!("✓ Parsing successful");
    
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax).expect("Failed to cast to SourceFile");
    
    println!("\n2. Running analysis...");
    let analysis_result = analyze(&source_file);
    
    println!("Analysis complete:");
    println!("  - Diagnostics: {}", analysis_result.diagnostics.len());
    for diag in &analysis_result.diagnostics {
        println!("    * {}", diag.message);
    }
    
    println!("\n3. Checking symbol tables:");
    
    // Check global symbols
    println!("\nGlobal symbols:");
    for symbol in analysis_result.global_scope.iter() {
        println!("  - {} ({:?})", symbol.name, symbol.kind);
    }
    
    // Check scopes
    println!("\nDefinition scopes: {}", analysis_result.definition_scopes.len());
    for (_node_ptr, scope) in &analysis_result.definition_scopes {
        let unnamed = "<unnamed>".to_string();
        let scope_name = scope.scope_name.as_ref().unwrap_or(&unnamed);
        println!("  - Scope '{}' has {} symbols", scope_name, scope.iter().count());
        for symbol in scope.iter() {
            println!("    * {} ({:?})", symbol.name, symbol.kind);
        }
    }
    
    println!("\n=== Test Complete ===");
}