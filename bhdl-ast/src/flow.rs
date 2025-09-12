//! AST nodes for circuit flow paradigm constructs

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use crate::expr::Expr;
use crate::common::{RangeExpr, ParamAssign, ParamAssignBlock};
use rowan::ast::AstNode;

// --- Flow Statement --- `name: flow_expr;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowStmt(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for FlowStmt {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::FLOW_STMT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for FlowStmt {}

impl FlowStmt {
    pub fn flow_expr(&self) -> Option<FlowExpr> {
        self.0.children().find_map(FlowExpr::cast)
    }
    
    pub fn colon_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::COLON)
    }
}

// --- Flow Expression --- `element |> element |> element`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowExpr(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for FlowExpr {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::FLOW_EXPR }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl FlowExpr {
    /// Returns all flow elements in order
    pub fn elements(&self) -> impl Iterator<Item = FlowElement> {
        self.0.children().filter_map(FlowElement::cast)
    }
    
    /// Returns flow operators (|>) between elements
    pub fn flow_operators(&self) -> impl Iterator<Item = SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::FLOW_OP)
    }
    
    /// Get the underlying expression that represents this flow
    pub fn as_expr(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

// --- Flow Element --- Components of a flow expression
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum FlowElement {
    Identifier(SyntaxToken<BhdlLanguage>),
    ComponentInstantiation(ComponentInstantiation),
    ConditionalExpr(ConditionalExpr),
}

impl FlowElement {
    pub fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if ComponentInstantiation::can_cast(syntax.kind()) {
            Some(FlowElement::ComponentInstantiation(ComponentInstantiation::cast(syntax)?))
        } else if ConditionalExpr::can_cast(syntax.kind()) {
            Some(FlowElement::ConditionalExpr(ConditionalExpr::cast(syntax)?))
        } else {
            // Check if it's a simple identifier
            syntax.first_token()
                .filter(|t| t.kind() == SyntaxKind::IDENT)
                .map(FlowElement::Identifier)
        }
    }
}

// --- Component Instantiation --- `Res(330Ω).1` or `LED(red).A`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentInstantiation(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ComponentInstantiation {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::COMPONENT_INST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ComponentInstantiation {
    /// Get the component type name (e.g., "Res", "LED")
    pub fn component_type(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the parameter assignments block (parameters in parentheses)
    pub fn parameters(&self) -> Option<ParamAssignBlock> {
        self.0.children().find_map(ParamAssignBlock::cast)
    }
    
    /// Get individual parameter assignments
    pub fn parameter_assignments(&self) -> impl Iterator<Item = ParamAssign> {
        self.parameters()
            .map(|block| block.assignments())
            .into_iter()
            .flatten()
    }
    
    /// Get the pin access part (e.g., ".1", ".A")
    pub fn pin_access(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        // Look for DOT followed by IDENT or NUMBER
        let mut found_dot = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.into_token() {
                if token.kind() == SyntaxKind::DOT {
                    found_dot = true;
                } else if found_dot && (token.kind() == SyntaxKind::IDENT || token.kind() == SyntaxKind::NUMBER) {
                    return Some(token);
                }
            }
        }
        None
    }
}

// --- Generate Statement --- `generate for var in range { ... }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenerateStmt(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for GenerateStmt {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::GENERATE_STMT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl GenerateStmt {
    /// Get the loop variable name
    pub fn loop_variable(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        // Find the IDENT token after FOR_KW
        let mut found_for = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.into_token() {
                if token.kind() == SyntaxKind::FOR_KW {
                    found_for = true;
                } else if found_for && token.kind() == SyntaxKind::IDENT {
                    return Some(token);
                }
            }
        }
        None
    }
    
    /// Get the range expression
    pub fn range(&self) -> Option<RangeExpr> {
        self.0.children().find_map(RangeExpr::cast)
    }
    
    /// Get the body statements
    pub fn body_statements(&self) -> impl Iterator<Item = SyntaxNode<BhdlLanguage>> {
        self.0.children().filter(|node| {
            // Look for various statement types that can appear in generate body
            matches!(node.kind(), 
                SyntaxKind::CONNECTION_STMT | 
                SyntaxKind::FLOW_STMT | 
                SyntaxKind::ASSIGN_STMT
            )
        })
    }
}

// --- Conditional Statement --- `if (condition) { ... } else { ... }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConditionalStmt(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ConditionalStmt {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CONDITIONAL_STMT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ConditionalStmt {
    /// Get the condition expression
    pub fn condition(&self) -> Option<Expr> {
        // Find the first expression (should be the condition)
        self.0.children().find_map(Expr::cast)
    }
    
    /// Get statements in the if block
    pub fn if_statements(&self) -> impl Iterator<Item = SyntaxNode<BhdlLanguage>> {
        // This is complex to implement precisely without more context about the tree structure
        // For now, return all statement-like children before any ELSE_KW
        self.0.children().take_while(|node| {
            !node.children_with_tokens().any(|t| 
                t.into_token().map(|tok| tok.kind() == SyntaxKind::ELSE_KW).unwrap_or(false)
            )
        }).filter(|node| {
            matches!(node.kind(), 
                SyntaxKind::CONNECTION_STMT | 
                SyntaxKind::FLOW_STMT | 
                SyntaxKind::ASSIGN_STMT
            )
        })
    }
    
    /// Get statements in the else block (if present)
    pub fn else_statements(&self) -> impl Iterator<Item = SyntaxNode<BhdlLanguage>> {
        // Find statements after ELSE_KW
        let mut found_else = false;
        self.0.children().filter(move |node| {
            // Check if this node or its children contain ELSE_KW
            if node.children_with_tokens().any(|t| 
                t.into_token().map(|tok| tok.kind() == SyntaxKind::ELSE_KW).unwrap_or(false)
            ) {
                found_else = true;
                false // Don't include the else token itself
            } else if found_else {
                matches!(node.kind(), 
                    SyntaxKind::CONNECTION_STMT | 
                    SyntaxKind::FLOW_STMT | 
                    SyntaxKind::ASSIGN_STMT
                )
            } else {
                false
            }
        })
    }
    
    /// Check if there's an else block
    pub fn has_else(&self) -> bool {
        self.0.children_with_tokens()
            .any(|element| element.into_token()
                .map(|t| t.kind() == SyntaxKind::ELSE_KW)
                .unwrap_or(false))
    }
}

// --- Conditional Expression --- `if (condition) { action }` (used in flows)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConditionalExpr(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ConditionalExpr {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { 
        // This might map to a different syntax kind or be part of expressions
        kind == SyntaxKind::CONDITIONAL_STMT || kind == SyntaxKind::TERNARY_EXPR
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ConditionalExpr {
    /// Get the condition
    pub fn condition(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    
    /// Get the action/then expression
    pub fn then_expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(1)
    }
    
    /// Get the else expression (if ternary)
    pub fn else_expr(&self) -> Option<Expr> {
        self.0.children().filter_map(Expr::cast).nth(2)
    }
}

// --- Assignment Statement --- `variable = expression;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AssignStmt(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for AssignStmt {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ASSIGN_STMT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl AssignStmt {
    /// Get the variable being assigned to
    pub fn variable(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the assignment expression
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
    
    /// Get the equals token
    pub fn equals_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::EQ)
    }
}