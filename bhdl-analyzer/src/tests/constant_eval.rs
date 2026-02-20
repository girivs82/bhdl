use super::common::*;
use crate::analyze;
use bhdl_ast::{Board, Entity, HasName}; // Removed common::ParamDecl
use rowan::ast::{AstNode, SyntaxNodePtr};


// TODO: Update for v2.0 syntax
// #[test]
// fn test_eval_const_literal() {
//     assert_eq!(eval_expr_str("42"), Some(42));
//     assert_eq!(eval_expr_str("-10"), Some(-10));
// }

// TODO: Update for v2.0 syntax
// #[test]
// fn test_eval_const_param_ref() {
//     // v1.0 test using parameters block - needs rewrite for v2.0
// }

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