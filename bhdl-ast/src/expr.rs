// bhdl-ast/src/expr.rs
use crate::common::{Value, IdentRef}; // Import necessary leaf nodes
use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken}; // Add SyntaxNode/Token
use rowan::ast::AstNode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    Value(Value),
    IdentRef(IdentRef),
    PrefixExpr(PrefixExpr),
    BinaryExpr(BinaryExpr),
    // Add others like TernaryExpr, ParenExpr, FunctionCallExpr later
}

impl AstNode for Expr {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        Value::can_cast(kind) ||
        IdentRef::can_cast(kind) ||
        PrefixExpr::can_cast(kind) ||
        BinaryExpr::can_cast(kind)
        // Add others later
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Value::can_cast(syntax.kind()) { Some(Expr::Value(Value::cast(syntax)?)) }
        else if IdentRef::can_cast(syntax.kind()) { Some(Expr::IdentRef(IdentRef::cast(syntax)?)) }
        else if PrefixExpr::can_cast(syntax.kind()) { Some(Expr::PrefixExpr(PrefixExpr::cast(syntax)?)) }
        else if BinaryExpr::can_cast(syntax.kind()) { Some(Expr::BinaryExpr(BinaryExpr::cast(syntax)?)) }
        else { None }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        match self {
            Expr::Value(n) => n.syntax(),
            Expr::IdentRef(n) => n.syntax(),
            Expr::PrefixExpr(n) => n.syntax(),
            Expr::BinaryExpr(n) => n.syntax(),
        }
    }
}

// Wrapper structs for expression kinds

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrefixExpr(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for PrefixExpr { 
    type Language = BhdlLanguage; // Add Language type
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PREFIX_EXPR } 
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } } 
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 } 
}
impl PrefixExpr {
    pub fn op(&self) -> Option<SyntaxKind> { self.0.first_token().map(|t| t.kind()) }
    pub fn expr(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinaryExpr(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for BinaryExpr { 
    type Language = BhdlLanguage; // Add Language type
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::BINARY_EXPR } 
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } } 
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 } 
}
impl BinaryExpr {
    pub fn lhs(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
    pub fn op(&self) -> Option<SyntaxKind> { 
        self.0.children_with_tokens().find_map(|node_or_token| {
            node_or_token.into_token().filter(|token| {
                // Use correct operator kinds
                matches!(token.kind(), 
                    SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH | 
                    SyntaxKind::AMPERSAND | SyntaxKind::PIPE | SyntaxKind::CARET | 
                    SyntaxKind::EQEQ | SyntaxKind::NEQ | // Use EQEQ not EQ
                    SyntaxKind::L_ANGLE | SyntaxKind::LTEQ | // Use L_ANGLE for <
                    SyntaxKind::R_ANGLE | SyntaxKind::GTEQ) // Use R_ANGLE for >
            })
        }).map(|t| t.kind())
    }
    pub fn rhs(&self) -> Option<Expr> {
        // Find the second Expr child
        self.0.children().filter_map(Expr::cast).nth(1)
    }
} 