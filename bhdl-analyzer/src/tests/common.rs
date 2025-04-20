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


// Helper to evaluate an expression string within a minimal board context
pub fn eval_expr_str(expr_str: &str) -> Option<i64> {
    let text = format!(r#"
        board TestEval {{
            parameters {{ parameter P_EVAL = {}; }}
        }}
    "#, expr_str);
    let source = parse_to_sourcefile(&text);
    let result = analyze(&source);
    
    // Avoid panic on error for this helper, just return None
    if !result.diagnostics.is_empty() {
        return None;
    }

    let board_ptr = result.global_scope.lookup("TestEval")?.definition_node_ptr.as_ref()?;
    
    // Need the expression node pointer from the ParamDecl
    let param_decl_node = board_ptr.try_to_node(source.syntax())?;
    let board_node = Board::cast(param_decl_node)?;
    let params_block = board_node.parameters_block()?;
    let param_decl = params_block.parameters().find(|p: &bhdl_ast::common::ParamDecl| p.name().map_or(false, |n| n.text() == "P_EVAL"))?;
    let value_expr_node = param_decl.value_expr()?;
    let value_expr_ptr = SyntaxNodePtr::new(&value_expr_node);

    result.resolved_constants.get(&value_expr_ptr).copied()
}

// Helper to get diagnostics for an expression string
pub fn get_diagnostics_for_expr(expr_str: &str) -> Vec<Diagnostic> {
     let text = format!(r#"
        board TestEval {{
            parameters {{ parameter P_EVAL = {}; }}
        }}
    "#, expr_str);
    // Parse even if there are errors, we want diagnostics
    let parse_result = parse(&text);
    let source = SourceFile::cast(parse_result.syntax()).expect("Failed to cast to SourceFile");
    analyze(&source).diagnostics
} 