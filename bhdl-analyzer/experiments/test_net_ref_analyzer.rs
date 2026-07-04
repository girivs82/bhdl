use bhdl_parser::parse;
use bhdl_ast::{AstNode, SourceFile};
use bhdl_analyzer::analyze;

fn main() {
    let input = std::fs::read_to_string("tests/test_analyzer_net_ref.bhdl")
        .expect("Failed to read test file");
    
    println!("=== Testing NetRef in Analyzer ===\n");
    
    // Parse
    let parse_result = parse(&input);
    if !parse_result.errors().is_empty() {
        println!("Parse errors:");
        for error in parse_result.errors() {
            println!("  - {}", error.message);
        }
        return;
    }
    
    // Convert to AST
    let syntax_tree = parse_result.syntax();
    let source_file = SourceFile::cast(syntax_tree)
        .expect("Failed to cast to SourceFile");
    
    // Analyze
    println!("Running semantic analysis...");
    let analysis_result = analyze(&source_file);
    
    // Check for diagnostics
    if !analysis_result.diagnostics.is_empty() {
        println!("\nDiagnostics:");
        for diag in &analysis_result.diagnostics {
            println!("  - {}", diag.message);
        }
    } else {
        println!("\n✅ No diagnostics - analysis successful!");
    }
    
    // Check symbol tables for nets
    println!("\n=== Symbol Table Analysis ===");
    
    // Check global scope
    println!("\nGlobal scope symbols:");
    for (name, symbol) in analysis_result.global_scope.get_symbols() {
        println!("  - {} (kind: {:?})", name, symbol.kind);
    }
    
    // Find board scope in definition_scopes
    for (node_ptr, scope) in &analysis_result.definition_scopes {
        if let Some(scope_name) = &scope.scope_name {
            if scope_name == "TestAnalyzerNetRef" {
                println!("\nNets found in board '{}' scope:", scope_name);
                for (name, symbol) in scope.get_nets() {
                    println!("  - @{} (kind: {:?})", name, symbol.kind);
                }
                
                println!("\nOther symbols in board scope:");
                for (name, symbol) in scope.get_symbols() {
                    println!("  - {} (kind: {:?})", name, symbol.kind);
                }
            }
        }
    }
}