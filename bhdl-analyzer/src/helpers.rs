use bhdl_ast::{Expr, common::Value};
use rowan::ast::AstNode;

/// Attempts to parse a bhdl_ast::Expr node as an i64 integer literal.
/// Handles both Value expressions and other expression types.
pub fn parse_expr_as_i64(expr_node: &Expr) -> Option<i64> {
    match expr_node {
        Expr::Value(value) => parse_value_as_i64(value),
        _ => {
            // For other expression types, try to parse the full text
            expr_node
                .syntax()
                .text()
                .to_string()
                .parse::<i64>()
                .ok()
        }
    }
}

/// Attempts to parse a bhdl_ast::common::Value node as an i64 integer literal.
/// Assumes the Value node directly represents the number (with optional sign handled by parser).
pub fn parse_value_as_i64(value_node: &Value) -> Option<i64> {
    value_node
        .syntax()
        .text()
        .to_string()
        .parse::<i64>()
        .ok()
} 