use std::fs;
use anyhow::Result;
use bhdl_parser::parse;
use bhdl_ast::{SourceFile, AstNode};
use bhdl_analyzer::analyze;

fn main() -> Result<()> {
    env_logger::init();
    
    println!("Testing simple import processing...\n");
    
    // Load test file with simple imports
    let source_content = fs::read_to_string("simple_test_main.bhdl")?;
    
    println!("=== Source File ===");
    println!("{}", source_content);
    
    // Parse
    let parse_result = parse(&source_content);
    let syntax_node = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_node)
        .ok_or_else(|| anyhow::anyhow!("Failed to cast to SourceFile"))?;
    
    // Check parse errors
    if !parse_result.errors().is_empty() {
        println!("\n=== Parse Errors ===");
        for error in parse_result.errors() {
            println!("  - {:?}", error);
        }
        return Ok(());
    } else {
        println!("\n✅ No parse errors!");
    }
    
    // Analyze
    println!("\n=== Analysis Phase ===");
    let analysis = analyze(&source_file);
    
    // Check global scope for imported modules
    println!("\n=== Global Scope Symbols ===");
    let global_symbols = analysis.global_scope.get_symbols();
    println!("Found {} symbols in global scope", global_symbols.len());
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
    
    // Check if modules are defined
    let has_lm7805 = analysis.global_scope.lookup("LM7805").is_some();
    let has_res = analysis.global_scope.lookup("Res").is_some();
    
    println!("\n=== Import Resolution ===");
    println!("  LM7805 imported: {}", has_lm7805);
    println!("  Res imported: {}", has_res);
    
    Ok(())
}