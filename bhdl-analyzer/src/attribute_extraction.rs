//! Attribute extraction from BHDL AST nodes

use std::collections::HashMap;
use rowan::ast::AstNode;
use bhdl_ast::{Entity, Board};
use bhdl_parser::SyntaxKind;

/// Extract attributes from an entity's syntax tree
pub fn extract_module_attributes(entity: &Entity) -> HashMap<String, String> {
    let mut attributes = HashMap::new();

    let syntax = entity.syntax();

    for child in syntax.children() {
        // ATTRIBUTE_DECL nodes contain: ATTRIBUTE_KW IDENT EQ <expr> SEMI
        if child.kind() == SyntaxKind::ATTRIBUTE_DECL {
            let mut name: Option<String> = None;
            let mut found_eq = false;

            for elem in child.children_with_tokens() {
                match elem {
                    rowan::NodeOrToken::Token(token) => {
                        match token.kind() {
                            SyntaxKind::ATTRIBUTE_KW => { /* skip keyword */ }
                            SyntaxKind::IDENT if name.is_none() => {
                                name = Some(token.text().to_string());
                            }
                            SyntaxKind::EQ => {
                                found_eq = true;
                            }
                            SyntaxKind::SEMI => { /* skip semicolon */ }
                            SyntaxKind::WHITESPACE => { /* skip whitespace */ }
                            _ if found_eq && name.is_some() => {
                                // Simple token value (number, true/false, etc.)
                                let value = token.text().to_string();
                                if let Some(attr_name) = name.take() {
                                    attributes.insert(attr_name, value.trim().to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    rowan::NodeOrToken::Node(node) if found_eq && name.is_some() => {
                        // Expression node — extract its text content
                        let value = extract_node_value(&node);
                        if let Some(attr_name) = name.take() {
                            attributes.insert(attr_name, value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    attributes
}

/// Extract a cleaned value from an expression node
fn extract_node_value(node: &rowan::SyntaxNode<bhdl_ast::BhdlLanguage>) -> String {
    let text = node.text().to_string().trim().to_string();
    // Remove surrounding quotes from string literals
    if text.starts_with('"') && text.ends_with('"') && text.len() >= 2 {
        text[1..text.len()-1].to_string()
    } else {
        text
    }
}

/// Extract attributes from a board's syntax tree
pub fn extract_board_attributes(board: &Board) -> HashMap<String, String> {
    let mut attributes = HashMap::new();

    let syntax = board.syntax();

    for child in syntax.children() {
        if child.kind() == SyntaxKind::ATTRIBUTE_DECL {
            let mut name: Option<String> = None;
            let mut found_eq = false;

            for elem in child.children_with_tokens() {
                match elem {
                    rowan::NodeOrToken::Token(token) => {
                        match token.kind() {
                            SyntaxKind::ATTRIBUTE_KW => {}
                            SyntaxKind::IDENT if name.is_none() => {
                                name = Some(token.text().to_string());
                            }
                            SyntaxKind::EQ => {
                                found_eq = true;
                            }
                            SyntaxKind::SEMI | SyntaxKind::WHITESPACE => {}
                            _ if found_eq && name.is_some() => {
                                let value = token.text().to_string();
                                if let Some(attr_name) = name.take() {
                                    attributes.insert(attr_name, value.trim().to_string());
                                }
                            }
                            _ => {}
                        }
                    }
                    rowan::NodeOrToken::Node(node) if found_eq && name.is_some() => {
                        let value = extract_node_value(&node);
                        if let Some(attr_name) = name.take() {
                            attributes.insert(attr_name, value);
                        }
                    }
                    _ => {}
                }
            }
        }
    }

    attributes
}

/// Substitute generic parameter references in attribute values with concrete values.
///
/// For example, if an entity has `attribute output_voltage = V_OUT;` and
/// `V_OUT` was specialized to `Voltage(5.0)`, this replaces the attribute
/// value "V_OUT" with "5" (the numeric representation).
pub fn substitute_generic_params(
    attrs: &mut HashMap<String, String>,
    concrete_params: &std::collections::BTreeMap<String, bhdl_common::ConstValue>,
) {
    for (_attr_name, attr_value) in attrs.iter_mut() {
        let trimmed = attr_value.trim();
        // Check if the attribute value matches a generic param name
        if let Some(cv) = concrete_params.get(trimmed) {
            *attr_value = const_value_to_attr_string(cv);
        }
    }
}

/// Convert a ConstValue to a string suitable for attribute values.
/// Returns the raw numeric value (in base SI units) for physical quantities.
fn const_value_to_attr_string(cv: &bhdl_common::ConstValue) -> String {
    match cv {
        bhdl_common::ConstValue::Integer(n) => format!("{}", n),
        bhdl_common::ConstValue::Float(f) => format_f64(*f),
        bhdl_common::ConstValue::Bool(b) => format!("{}", b),
        bhdl_common::ConstValue::String(s) => s.clone(),
        bhdl_common::ConstValue::Voltage(v) => format_f64(*v),
        bhdl_common::ConstValue::Current(a) => format_f64(*a),
        bhdl_common::ConstValue::Resistance(r) => format_f64(*r),
        bhdl_common::ConstValue::Capacitance(c) => format_f64(*c),
        bhdl_common::ConstValue::Inductance(l) => format_f64(*l),
        bhdl_common::ConstValue::Power(w) => format_f64(*w),
        bhdl_common::ConstValue::Frequency(hz) => format_f64(*hz),
        bhdl_common::ConstValue::Time(t) => format_f64(*t),
    }
}

/// Format an f64, dropping the decimal if it's an integer value.
fn format_f64(v: f64) -> String {
    if (v - v.round()).abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{}", v)
    }
}

/// Extract attributes from a component instance in the AST
pub fn extract_component_attributes(component_name: &str, params: &HashMap<String, String>) -> HashMap<String, String> {
    // For component instances like LED(red), Res(10k), etc.
    // We need to map the parameters to expected attribute names

    let mut attributes = HashMap::new();

    match component_name {
        "LED" => {
            if let Some(color) = params.get("color").or_else(|| params.values().next()) {
                attributes.insert("color".to_string(), color.clone());
            }
        }
        "Res" | "Resistor" => {
            if let Some(value) = params.get("value").or_else(|| params.values().next()) {
                attributes.insert("value".to_string(), value.clone());
            }
        }
        "Cap" | "Capacitor" => {
            if let Some(value) = params.get("value").or_else(|| params.values().next()) {
                attributes.insert("value".to_string(), value.clone());
            }
        }
        _ => {
            attributes.extend(params.clone());
        }
    }

    attributes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_attributes_from_entity() {
        // Parse a simple entity with attributes
        let source = r#"
entity Res(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
    attribute component_class = "resistor";
    attribute tolerance = 0.05;
}
"#;
        let parse = bhdl_parser::parse(source);
        let source_file = bhdl_ast::SourceFile::cast(parse.syntax()).unwrap();

        let entities: Vec<_> = source_file.entities().collect();
        assert_eq!(entities.len(), 1);

        let attrs = extract_module_attributes(&entities[0]);
        assert_eq!(attrs.get("component_class"), Some(&"resistor".to_string()));
    }
}
