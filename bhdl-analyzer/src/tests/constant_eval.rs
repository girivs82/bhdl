use super::common::*;
use crate::analyze;
use bhdl_ast::{Board, Module, HasName}; // Removed common::ParamDecl
use rowan::ast::{AstNode, SyntaxNodePtr};


#[test]
fn test_eval_const_literal() {
    assert_eq!(eval_expr_str("42"), Some(42));
    assert_eq!(eval_expr_str("-10"), Some(-10));
}

#[test]
fn test_eval_const_param_ref() {
    let text = r#"
        board TestBoard {
            parameters {
                parameter A = 10;
                parameter B = A + 5;
            }
        }
    "#;
    let source = parse_to_sourcefile(text);
    let result = analyze(&source);
    assert!(result.diagnostics.is_empty());
    let board_ptr = result.global_scope.lookup("TestBoard").unwrap().definition_node_ptr.clone().unwrap();
    let board_node = board_ptr.try_to_node(source.syntax()).unwrap();
    let board_def = Board::cast(board_node).unwrap();
    let params_block = board_def.parameters_block().expect("No parameters block");

    // Find ParamDecl 'A' within the block and get its value expr ptr
    let param_a_decl = params_block.parameters()
        .find(|p| p.name().map_or(false, |n| n.text() == "A"))
        .expect("ParamDecl 'A' not found");
    let value_a_expr = param_a_decl.value_expr().unwrap();
    let value_a_ptr = SyntaxNodePtr::new(&value_a_expr);

    // Find ParamDecl 'B' within the block and get its value expr ptr
    let param_b_decl = params_block.parameters()
        .find(|p| p.name().map_or(false, |n| n.text() == "B"))
            .expect("ParamDecl 'B' not found");
    let value_b_expr = param_b_decl.value_expr().unwrap();
    let value_b_ptr = SyntaxNodePtr::new(&value_b_expr);

    let val_a = result.resolved_constants.get(&value_a_ptr).cloned();
    let val_b = result.resolved_constants.get(&value_b_ptr).cloned();

    assert_eq!(val_a, Some(10));
    assert_eq!(val_b, Some(15));
}

#[test]
fn test_eval_const_binary_ops() {
    // Use analyze_helper to check diagnostics first
    analyze_helper("board T { parameters { parameter X = (5 + 3) * 2 - 10 / 5; } }", false);
    assert_eq!(eval_expr_str("(5 + 3) * 2 - 10 / 5"), Some(14));
}

#[test]
fn test_eval_const_unary_minus() {
    analyze_helper("board T { parameters { parameter Y = -(5 + 2); } }", false);
    assert_eq!(eval_expr_str("-(5 + 2)"), Some(-7));
}

#[test]
fn test_eval_const_parens() {
    analyze_helper("board T { parameters { parameter Z = 5 * (2 + 3); } }", false);
    assert_eq!(eval_expr_str("5 * (2 + 3)"), Some(25));
}

#[test]
fn test_eval_const_nested_param() {
    let text = r#"
        board Outer {
            parameters { parameter P_OUT = 10; }
            // Parameter overrides inside {} require the 'parameter' keyword
            components { component Inner M1 { parameter P_IN = P_OUT * 2; } }
        }
        module Inner {
            parameters {
                parameter P_IN: integer;
                parameter P_CALC = P_IN + 5;
            }
        }
    "#;
    // This should now parse correctly and analyze without errors
    analyze_helper(text, false);

    // Re-enable constant checks once eval_expr_str can handle context
    // let source = parse_to_sourcefile(text);
    // let result = analyze(&source);
    // ... Add assertions here to check resolved_constants for P_CALC in M1 context ...
}

// Add tests using get_diagnostics_for_expr if needed
// #[test]
// fn test_eval_const_error() {
//     let diags = get_diagnostics_for_expr("10 / 0");
//     assert!(!diags.is_empty());
//     // Add more specific assertion about the error message
// } 