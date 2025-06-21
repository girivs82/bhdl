// BHDL v2.0 Attribute AST nodes
// Support for behavioral modeling with extended attributes

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use rowan::ast::AstNode;
use crate::expr::Expr;

/// Attribute declaration: attribute name = expression;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttributeDecl(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for AttributeDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ATTRIBUTE_DECL }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for AttributeDecl {}

impl AttributeDecl {
    /// Get the attribute name
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the value expression (can be literal or complex expression)
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    
    /// Check if this attribute uses an expression (not just a literal)
    pub fn is_expression_attribute(&self) -> bool {
        if let Some(expr) = self.value() {
            match expr {
                // Literals and simple values are not expressions
                Expr::Literal(_) => false,
                Expr::Value(_) => false,
                // Everything else is considered an expression
                _ => true,
            }
        } else {
            false
        }
    }
    
    /// Get all pin references in the attribute expression
    pub fn referenced_pins(&self) -> Vec<String> {
        if let Some(expr) = self.value() {
            collect_pin_references(&expr)
        } else {
            vec![]
        }
    }
    
    /// Get all attribute references in the expression
    pub fn referenced_attributes(&self) -> Vec<String> {
        if let Some(expr) = self.value() {
            collect_attribute_references(&expr)
        } else {
            vec![]
        }
    }
}

/// Helper function to collect pin references from an expression
fn collect_pin_references(expr: &Expr) -> Vec<String> {
    let mut refs = Vec::new();
    
    match expr {
        Expr::PinRef(pin_ref) => {
            let text = pin_ref.syntax().text().to_string();
            if let Some(name) = text.split('.').last() {
                refs.push(name.to_string());
            }
        },
        Expr::BinaryExpr(binary) => {
            if let Some(left) = binary.lhs() {
                refs.extend(collect_pin_references(&left));
            }
            if let Some(right) = binary.rhs() {
                refs.extend(collect_pin_references(&right));
            }
        },
        Expr::PrefixExpr(prefix) => {
            if let Some(operand) = prefix.expr() {
                refs.extend(collect_pin_references(&operand));
            }
        },
        Expr::TernaryExpr(ternary) => {
            if let Some(condition) = ternary.condition() {
                refs.extend(collect_pin_references(&condition));
            }
            if let Some(true_expr) = ternary.true_expr() {
                refs.extend(collect_pin_references(&true_expr));
            }
            if let Some(false_expr) = ternary.false_expr() {
                refs.extend(collect_pin_references(&false_expr));
            }
        },
        Expr::FunctionCallExpr(call) => {
            for arg in call.arguments() {
                refs.extend(collect_pin_references(&arg));
            }
        },
        _ => {}
    }
    
    refs
}

/// Helper function to collect attribute references from an expression
fn collect_attribute_references(expr: &Expr) -> Vec<String> {
    let mut refs = Vec::new();
    
    match expr {
        Expr::Ident(node) => {
            // Simple identifier that's not a pin reference or built-in
            let text = node.text().to_string().trim().to_string();
            if !text.is_empty() && !text.contains('.') && !is_builtin_variable(&text) {
                refs.push(text);
            }
        },
        Expr::IdentRef(ident_ref) => {
            // IdentRef nodes represent identifier references
            if let Some(token) = ident_ref.token() {
                let text = token.text().to_string().trim().to_string();
                if !text.is_empty() && !text.contains('.') && !is_builtin_variable(&text) {
                    refs.push(text);
                }
            }
        },
        Expr::BinaryExpr(binary) => {
            if let Some(left) = binary.lhs() {
                refs.extend(collect_attribute_references(&left));
            }
            if let Some(right) = binary.rhs() {
                refs.extend(collect_attribute_references(&right));
            }
        },
        Expr::PrefixExpr(prefix) => {
            if let Some(operand) = prefix.expr() {
                refs.extend(collect_attribute_references(&operand));
            }
        },
        Expr::TernaryExpr(ternary) => {
            if let Some(condition) = ternary.condition() {
                refs.extend(collect_attribute_references(&condition));
            }
            if let Some(true_expr) = ternary.true_expr() {
                refs.extend(collect_attribute_references(&true_expr));
            }
            if let Some(false_expr) = ternary.false_expr() {
                refs.extend(collect_attribute_references(&false_expr));
            }
        },
        Expr::FunctionCallExpr(call) => {
            for arg in call.arguments() {
                refs.extend(collect_attribute_references(&arg));
            }
        },
        _ => {}
    }
    
    refs
}

/// Attribute type information for semantic analysis
#[derive(Debug, Clone, PartialEq)]
pub enum AttributeType {
    Static(String),           // Static value (literal)
    Expression(Vec<String>),  // Expression with dependencies
    Mutable,                  // Modified in when blocks
}

/// Attribute dependency information
#[derive(Debug, Clone)]
pub struct AttributeDependency {
    pub attribute: String,
    pub depends_on: Vec<String>,
    pub pin_refs: Vec<String>,
    pub is_mutable: bool,
}

/// Check if a name is a built-in variable that should be excluded from dependencies
fn is_builtin_variable(name: &str) -> bool {
    matches!(name, "dt" | "t" | "pi" | "e")
}