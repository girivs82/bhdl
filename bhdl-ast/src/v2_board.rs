// Extended Board implementation for v2.0 support

use crate::{Board, SyntaxKind, BhdlLanguage};
use crate::v2_statements::{Statement, PowerDecl, GroundDecl, ConnectionStmt, FlowStmt};
use rowan::ast::AstNode;

/// Extension trait for Board to support v2.0 constructs
pub trait BoardV2Ext {
    /// Get all statements in the board (v2.0 style)
    fn statements(&self) -> impl Iterator<Item = Statement>;
    
    /// Get all power declarations
    fn power_decls(&self) -> impl Iterator<Item = PowerDecl>;
    
    /// Get all ground declarations
    fn ground_decls(&self) -> impl Iterator<Item = GroundDecl>;
    
    /// Get all connection statements
    fn connections(&self) -> impl Iterator<Item = ConnectionStmt>;
    
    /// Get all flow statements
    fn flow_stmts(&self) -> impl Iterator<Item = FlowStmt>;
}

impl BoardV2Ext for Board {
    fn statements(&self) -> impl Iterator<Item = Statement> {
        self.syntax().children().filter_map(Statement::cast)
    }
    
    fn power_decls(&self) -> impl Iterator<Item = PowerDecl> {
        self.syntax().children().filter_map(PowerDecl::cast)
    }
    
    fn ground_decls(&self) -> impl Iterator<Item = GroundDecl> {
        self.syntax().children().filter_map(GroundDecl::cast)
    }
    
    fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.syntax().children().filter_map(ConnectionStmt::cast)
    }
    
    fn flow_stmts(&self) -> impl Iterator<Item = FlowStmt> {
        self.syntax().children().filter_map(FlowStmt::cast)
    }
}

/// Body wrapper for v2.0 boards (the content between braces)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BoardBody(pub(crate) rowan::SyntaxNode<BhdlLanguage>);

impl AstNode for BoardBody {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        // Board body doesn't have a specific kind, it's just the content
        false
    }
    
    fn cast(_syntax: rowan::SyntaxNode<BhdlLanguage>) -> Option<Self> {
        // Board body is extracted differently
        None
    }
    
    fn syntax(&self) -> &rowan::SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl BoardBody {
    /// Get all statements in the board body
    pub fn statements(&self) -> impl Iterator<Item = Statement> {
        self.0.children().filter_map(Statement::cast)
    }
}

/// Extension to get the body of a board (content between braces)
pub trait BoardBodyExt {
    fn body(&self) -> Option<BoardBody>;
}

impl BoardBodyExt for Board {
    fn body(&self) -> Option<BoardBody> {
        // The body is all the children between L_BRACE and R_BRACE
        // For simplicity, we'll consider the board node itself as containing the body
        Some(BoardBody(self.syntax().clone()))
    }
}