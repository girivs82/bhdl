// Test analyzer for the 7805 regulator circuit
use std::fs;
use bhdl_analyzer::analyze;
use bhdl_ast::{AstNode, SourceFile};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Testing Analyzer for 7805 Circuit ===\n");
    
    let content = fs::read_to_string("test_7805_regulator_realistic.bhdl")?;
    let parse_result = bhdl_parser::parse(&content);
    
    if !parse_result.errors().is_empty() {
        println!("❌ Parse errors found:");
        for err in parse_result.errors() {
            println!("  - {}", err.message);
        }
        return Ok(());
    }
    
    println!("✅ Parse successful, converting to AST...");
    
    // Convert to AST
    let syntax = parse_result.syntax();
    let source_file = SourceFile::cast(syntax.clone()).ok_or("Failed to cast to SourceFile")?;
    
    println!("✅ AST created, running analysis...\n");
    
    // Run the analyzer
    let analysis_result = analyze(&source_file);
    
    // Print analysis results
    print_analysis_results(&analysis_result);
    
    Ok(())
}

fn print_analysis_results(result: &bhdl_analyzer::types::AnalysisResult) {
    // Print diagnostics (note: Diagnostic struct doesn't have severity field)
    let diagnostic_count = result.diagnostics.len();
    
    println!("Diagnostics:");
    println!("  Total: {}", diagnostic_count);
    
    if !result.diagnostics.is_empty() {
        println!("\nDiagnostic details:");
        for diag in &result.diagnostics {
            println!("  - {}", diag.message);
        }
    }
    
    // Print symbol table summary
    println!("\nGlobal Scope:");
    // Since SymbolTable fields are not public, we can only report on what we can access
    println!("  Global symbols: {}", result.global_scope.get_symbols().len());
    
    // Print definition scopes
    println!("\nDefinition Scopes:");
    println!("  Total scopes: {}", result.definition_scopes.len());
    
    // Print resolved constants
    println!("\nResolved Constants:");
    if result.resolved_constants.is_empty() {
        println!("  (none)");
    } else {
        for (ptr, value) in &result.resolved_constants {
            println!("  Node@{:?} = {}", ptr, value);
        }
    }
    
    // Print power analysis results
    println!("\nPower Analysis:");
    println!("  Power domains: {}", result.power_analysis.domains.len());
    println!("  Level shifters: {}", result.power_analysis.level_shifted_signals.len());
    if !result.power_analysis.errors.is_empty() {
        println!("  Errors:");
        for err in &result.power_analysis.errors {
            println!("    - {}", err);
        }
    }
    
    // Print component inference results
    println!("\nComponent Inference:");
    let inferred_count = result.component_inference.get_inferred_components().len();
    println!("  Inferred components: {}", inferred_count);
    if inferred_count > 0 {
        println!("  Components:");
        for comp in result.component_inference.get_inferred_components() {
            println!("    - {} ({})", comp.component_type, comp.reasoning);
        }
    }
    
    // Overall result
    if diagnostic_count == 0 {
        println!("\n✅ Analysis completed successfully!");
    } else {
        println!("\n⚠️ Analysis completed with {} diagnostics", diagnostic_count);
    }
}