// BHDL v2.0 Behavioral modeling AST nodes
// Support for when blocks and behavioral statements

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken};
use crate::expr::Expr;
use rowan::ast::AstNode;

/// When block for behavioral modeling: when (condition) { statements }
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct WhenBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for WhenBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::WHEN_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl WhenBlock {
    /// Get the when condition expression
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    
    /// Get all statements in the when block body
    pub fn statements(&self) -> impl Iterator<Item = BehavioralStmt> {
        self.0.children().filter_map(BehavioralStmt::cast)
    }
    
    /// Get attribute assignments in this when block
    pub fn attribute_assignments(&self) -> impl Iterator<Item = AttributeAssignment> {
        self.statements().filter_map(|stmt| match stmt {
            BehavioralStmt::AttributeAssignment(assign) => Some(assign),
            _ => None,
        })
    }
}

/// Behavioral statement types that can appear in when blocks
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BehavioralStmt {
    AttributeAssignment(AttributeAssignment),
    Expression(Expr),
    WhenBlock(WhenBlock), // Nested when blocks
}

impl AstNode for BehavioralStmt {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        AttributeAssignment::can_cast(kind) ||
        Expr::can_cast(kind) ||
        WhenBlock::can_cast(kind)
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if let Some(assign) = AttributeAssignment::cast(syntax.clone()) {
            Some(BehavioralStmt::AttributeAssignment(assign))
        } else if let Some(when_block) = WhenBlock::cast(syntax.clone()) {
            Some(BehavioralStmt::WhenBlock(when_block))
        } else if let Some(expr) = Expr::cast(syntax.clone()) {
            Some(BehavioralStmt::Expression(expr))
        } else {
            None
        }
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        match self {
            BehavioralStmt::AttributeAssignment(n) => n.syntax(),
            BehavioralStmt::Expression(n) => n.syntax(),
            BehavioralStmt::WhenBlock(n) => n.syntax(),
        }
    }
}

/// Attribute assignment: attribute name = expression;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttributeAssignment(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for AttributeAssignment {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ATTRIBUTE_ASSIGNMENT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl AttributeAssignment {
    /// Get the attribute name being assigned to
    pub fn attribute_name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        // Find the identifier after 'attribute' keyword
        let mut found_attribute_kw = false;
        for token in self.0.children_with_tokens().filter_map(|e| e.into_token()) {
            if token.kind() == SyntaxKind::ATTRIBUTE_KW {
                found_attribute_kw = true;
            } else if found_attribute_kw && token.kind() == SyntaxKind::IDENT {
                return Some(token);
            }
        }
        None
    }
    
    /// Alias for attribute_name for consistency
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.attribute_name()
    }
    
    /// Get the assignment operator token
    pub fn op_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| matches!(t.kind(), 
                SyntaxKind::EQ | SyntaxKind::PLUS_EQ | SyntaxKind::MINUS_EQ
            ))
    }
    
    /// Get the value expression being assigned
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    
    /// Check if this is an increment operation (+=)
    pub fn is_increment(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| t.kind() == SyntaxKind::PLUS_EQ)
    }
    
    /// Check if this is a decrement operation (-=)
    pub fn is_decrement(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| t.kind() == SyntaxKind::MINUS_EQ)
    }
}

/// Find all when blocks in a syntax tree
pub fn find_when_blocks(root: &SyntaxNode<BhdlLanguage>) -> Vec<WhenBlock> {
    let mut blocks = Vec::new();
    for node in root.descendants() {
        if let Some(when_block) = WhenBlock::cast(node) {
            blocks.push(when_block);
        }
    }
    blocks
}

/// Find all attributes that are modified in when blocks
pub fn find_mutable_attributes(root: &SyntaxNode<BhdlLanguage>) -> Vec<String> {
    let mut mutable_attrs = std::collections::HashSet::new();
    
    for when_block in find_when_blocks(root) {
        for assignment in when_block.attribute_assignments() {
            if let Some(name) = assignment.attribute_name() {
                mutable_attrs.insert(name.text().to_string());
            }
        }
    }
    
    mutable_attrs.into_iter().collect()
}