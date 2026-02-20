use bhdl_ast::{Expr, common::Value};
use bhdl_common::ConstValue;
use bhdl_common::const_value::{parse_unit_suffix, parse_si_prefix};
use bhdl_parser::SyntaxKind;
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

/// Parse a Value AST node into a rich ConstValue, handling units and prefixes.
///
/// A VALUE node may contain:
/// - NUMBER token (possibly with '.' for float)
/// - Optional IDENT token for single-letter unit/prefix ("V", "A", "k", "m", etc.)
/// - Optional UNIT_IDENTIFIER token for multi-letter units ("kΩ", "mA", "MHz", etc.)
/// - Optional MINUS/PLUS token for sign
/// - STRING token for string literals
/// - TRUE_KW/FALSE_KW for booleans
pub fn parse_value_as_const(value_node: &Value) -> Option<ConstValue> {
    let syntax = value_node.syntax();
    let mut tokens = syntax
        .children_with_tokens()
        .filter_map(|elem| elem.into_token());

    // Collect sign and number tokens
    let mut sign: f64 = 1.0;
    let mut number_text: Option<String> = None;
    let mut unit_text: Option<String> = None;
    let mut is_string = false;
    let mut is_bool = false;
    let mut bool_val = false;

    for token in tokens {
        match token.kind() {
            SyntaxKind::MINUS => sign = -1.0,
            SyntaxKind::PLUS => { /* positive, default */ }
            SyntaxKind::NUMBER => {
                number_text = Some(token.text().to_string());
            }
            SyntaxKind::IDENT => {
                // Single-letter unit or prefix (V, A, k, m, etc.)
                unit_text = Some(token.text().to_string());
            }
            SyntaxKind::UNIT_IDENTIFIER => {
                // Multi-letter unit (kΩ, mA, MHz, etc.)
                unit_text = Some(token.text().to_string());
            }
            SyntaxKind::STRING => {
                is_string = true;
                let text = token.text();
                let trimmed = text.trim_matches('"');
                return Some(ConstValue::String(trimmed.to_string()));
            }
            SyntaxKind::TRUE_KW => {
                is_bool = true;
                bool_val = true;
            }
            SyntaxKind::FALSE_KW => {
                is_bool = true;
                bool_val = false;
            }
            _ => {}
        }
    }

    if is_bool {
        return Some(ConstValue::Bool(bool_val));
    }
    if is_string {
        return None; // Already handled above
    }

    let num_str = number_text?;
    let is_float = num_str.contains('.');

    // Parse the numeric value
    let raw_number: f64 = num_str.parse().ok()?;
    let signed_number = sign * raw_number;

    match unit_text {
        Some(unit) => {
            // Try as a full unit suffix first (e.g., "kΩ", "mA", "V")
            if let Some((scale, constructor)) = parse_unit_suffix(&unit) {
                return Some(constructor(signed_number * scale));
            }
            // Try as a standalone SI prefix (e.g., "k" = 1e3, "m" = 1e-3)
            if let Some(scale) = parse_si_prefix(&unit) {
                let val = signed_number * scale;
                // Pure scaled number (no base unit) → Float
                return Some(ConstValue::Float(val));
            }
            // Unknown unit text — treat as plain number with warning
            if is_float {
                Some(ConstValue::Float(signed_number))
            } else {
                Some(ConstValue::Integer(signed_number as i64))
            }
        }
        None => {
            // No unit — pure number
            if is_float {
                Some(ConstValue::Float(signed_number))
            } else {
                Some(ConstValue::Integer(signed_number as i64))
            }
        }
    }
}
