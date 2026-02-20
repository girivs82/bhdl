//! Attribute extraction from BHDL AST nodes

use std::collections::HashMap;
use rowan::ast::AstNode;
use bhdl_ast::{Entity, Board};
use bhdl_parser::SyntaxKind;

/// Extract attributes from an entity's syntax tree
pub fn extract_module_attributes(entity: &Entity) -> HashMap<String, String> {
    let mut attributes = HashMap::new();

    // Walk through the entity's syntax tree looking for attribute declarations
    let syntax = entity.syntax();
    
    for child in syntax.children() {
        // Look for attribute declarations (attribute name = value;)
        if child.kind() == SyntaxKind::ATTRIBUTE_KW {
            // The structure should be: ATTRIBUTE_KW IDENT EQ expression SEMI
            let mut tokens = child.siblings_with_tokens(rowan::Direction::Next);
            
            // Skip the ATTRIBUTE_KW
            tokens.next();
            
            // Get the attribute name
            if let Some(name_token) = tokens.next() {
                if let Some(name) = name_token.as_token() {
                    if name.kind() == SyntaxKind::IDENT {
                        let attr_name = name.text().to_string();
                        
                        // Skip the EQ
                        tokens.next();
                        
                        // Get the value expression
                        if let Some(value_elem) = tokens.next() {
                            let value_text = extract_expression_value(value_elem);
                            attributes.insert(attr_name, value_text);
                        }
                    }
                }
            }
        }
    }
    
    // Also handle @ metadata syntax
    for token in syntax.children_with_tokens() {
        if let Some(tok) = token.as_token() {
            if tok.kind() == SyntaxKind::AT {
                // @ metadata follows pattern: @ IDENT = expression
                let mut siblings = tok.siblings_with_tokens(rowan::Direction::Next);
                
                // Get the attribute name
                if let Some(name_token) = siblings.next() {
                    if let Some(name) = name_token.as_token() {
                        if name.kind() == SyntaxKind::IDENT {
                            let attr_name = name.text().to_string();
                            
                            // Skip the EQ
                            siblings.next();
                            
                            // Get the value
                            if let Some(value_elem) = siblings.next() {
                                let value_text = extract_expression_value(value_elem);
                                attributes.insert(attr_name, value_text);
                            }
                        }
                    }
                }
            }
        }
    }
    
    attributes
}

/// Extract attributes from a board's syntax tree
pub fn extract_board_attributes(board: &Board) -> HashMap<String, String> {
    let mut attributes = HashMap::new();
    
    // Similar logic to module attributes
    let syntax = board.syntax();
    
    for child in syntax.children() {
        if child.kind() == SyntaxKind::ATTRIBUTE_KW {
            let mut tokens = child.siblings_with_tokens(rowan::Direction::Next);
            tokens.next(); // Skip ATTRIBUTE_KW
            
            if let Some(name_token) = tokens.next() {
                if let Some(name) = name_token.as_token() {
                    if name.kind() == SyntaxKind::IDENT {
                        let attr_name = name.text().to_string();
                        tokens.next(); // Skip EQ
                        
                        if let Some(value_elem) = tokens.next() {
                            let value_text = extract_expression_value(value_elem);
                            attributes.insert(attr_name, value_text);
                        }
                    }
                }
            }
        }
    }
    
    attributes
}

/// Extract the value from an expression element
fn extract_expression_value(elem: rowan::NodeOrToken<rowan::SyntaxNode<bhdl_ast::BhdlLanguage>, rowan::SyntaxToken<bhdl_ast::BhdlLanguage>>) -> String {
    match elem {
        rowan::NodeOrToken::Node(node) => {
            // For expression nodes, extract their text content
            // This might be a complex expression, for now just get the text
            node.text().to_string().trim().to_string()
        }
        rowan::NodeOrToken::Token(token) => {
            // For simple tokens (strings, numbers, etc.)
            match token.kind() {
                SyntaxKind::STRING => {
                    // Remove quotes from string literals
                    let text = token.text();
                    if text.starts_with('"') && text.ends_with('"') {
                        text[1..text.len()-1].to_string()
                    } else {
                        text.to_string()
                    }
                }
                _ => token.text().to_string(),
            }
        }
    }
}

/// Extract attributes from a component instance in the AST
pub fn extract_component_attributes(component_name: &str, params: &HashMap<String, String>) -> HashMap<String, String> {
    // For component instances like LED(red), Res(10k), etc.
    // We need to map the parameters to expected attribute names
    
    let mut attributes = HashMap::new();
    
    match component_name {
        "LED" => {
            // LED expects color parameter
            if let Some(color) = params.get("color").or_else(|| params.values().next()) {
                attributes.insert("color".to_string(), color.clone());
            }
        }
        "Res" | "Resistor" => {
            // Resistor expects value parameter
            if let Some(value) = params.get("value").or_else(|| params.values().next()) {
                attributes.insert("value".to_string(), value.clone());
            }
        }
        "Cap" | "Capacitor" => {
            // Capacitor expects value parameter
            if let Some(value) = params.get("value").or_else(|| params.values().next()) {
                attributes.insert("value".to_string(), value.clone());
            }
        }
        _ => {
            // For other components, just copy all parameters as attributes
            attributes.extend(params.clone());
        }
    }
    
    attributes
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_attribute_extraction() {
        // TODO: Add tests with sample AST nodes
    }
}