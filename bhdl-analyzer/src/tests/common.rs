use bhdl_parser::parse;
use rowan::ast::{AstNode, SyntaxNodePtr};
use crate::types::Diagnostic;
use bhdl_ast::{Board, HasName, SourceFile}; // Keep Board, HasName, SourceFile
use crate::analyze; // Keep analyze use

// Helper to parse text into a SourceFile for tests
pub fn parse_to_sourcefile(text: &str) -> SourceFile {
    let parse_result = parse(text);
    // Updated error check to print errors before panicking
    if !parse_result.errors().is_empty() {
        eprintln!("Parse errors in test input:
```
{}
```", text);
        for error in parse_result.errors() {
            eprintln!("  - {}", error.message);
        }
        panic!("Parse errors encountered in test setup.");
    }
    SourceFile::cast(parse_result.syntax()).expect("Failed to cast to SourceFile")
}

// Recreated analyze_helper based on expected usage
pub fn analyze_helper(input: &str, expect_error: bool) {
    let source = parse_to_sourcefile(input);
    let result = analyze(&source);
    if expect_error {
        if result.diagnostics.is_empty() {
            panic!("Expected analysis errors, but found none for input:
```
{}
```", input);
        }
    } else {
        if !result.diagnostics.is_empty() {
            eprintln!("Expected no analysis errors, but found some for input:
```
{}
```", input);
            for diag in result.diagnostics {
                eprintln!("  - {:?}: {}", diag.range, diag.message);
            }
            panic!("Unexpected analysis errors found.");
        }
    }
}


// TODO: Update for v2.0 syntax - parameters are now in component instantiation
// Helper to evaluate an expression string within a minimal board context
pub fn eval_expr_str(expr_str: &str) -> Option<i64> {
    // v1.0 syntax not supported - needs rewrite for v2.0
    None
}

// TODO: Update for v2.0 syntax
// Helper to get diagnostics for an expression string
pub fn get_diagnostics_for_expr(expr_str: &str) -> Vec<Diagnostic> {
    // v1.0 syntax not supported - needs rewrite for v2.0
    vec![]
} 