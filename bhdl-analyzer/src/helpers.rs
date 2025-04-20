use bhdl_ast::common::Value;
use rowan::ast::AstNode;

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