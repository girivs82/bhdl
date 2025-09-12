//! Test import processing in analyzer Pass 1

use std::fs;
use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing import processing in analyzer...\n");
    
    // Load test file with imports
    let test_file = "tests/circuits/realistic/test_7805_with_imports.bhdl";
    let source_content = fs::read_to_string(&test_file)?;
    
    println!("=== Source File ===");
    println!("{}", source_content);
    
    // Parse
    let parse_result = parse(&source_content);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Analyze
    println!("\n=== Analysis Phase ===");
    let analysis = analyze(&source_file);
    
    // Check global scope for imported modules
    println!("\n=== Global Scope Symbols ===");
    let global_symbols = analysis.global_scope.get_symbols();
    for (name, symbol) in global_symbols {
        println!("  {} ({:?})", name, symbol.kind);
    }
    
    // Check diagnostics
    println!("\n=== Diagnostics ===");
    if analysis.diagnostics.is_empty() {
        println!("  ✅ No diagnostics!");
    } else {
        for diag in &analysis.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    // Check if LM7805 is now defined
    let has_lm7805 = global_symbols.contains_key("LM7805");
    let has_fuse = global_symbols.contains_key("Fuse");
    let has_led = global_symbols.contains_key("LED");
    
    println!("\n=== Import Resolution ===");
    println!("  LM7805 imported: {}", has_lm7805);
    println!("  Fuse imported: {}", has_fuse);
    println!("  LED imported: {}", has_led);
    
    // Check for undefined component errors
    let undefined_count = analysis.diagnostics.iter()
        .filter(|d| d.message.contains("Undefined component"))
        .count();
    
    println!("\n=== Result ===");
    if undefined_count == 0 && has_lm7805 {
        println!("✅ Import processing successful! All components resolved.");
    } else {
        println!("❌ Import processing failed. {} undefined components.", undefined_count);
    }
    
    Ok(())
}