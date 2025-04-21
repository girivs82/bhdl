//! Abstract Syntax Tree (AST) for the BHDL language.

// Re-export core parser types
pub use bhdl_parser::{SyntaxKind, BhdlLanguage}; // Keep base language and kind

// Use rowan types directly for Node/Token
pub use rowan::{SyntaxNode, SyntaxToken};

// AST node trait (re-exported)
pub use rowan::ast::AstNode;

// Module declarations
pub mod blocks;
pub mod items;
pub mod common;
pub mod expr; 
pub mod source_file; // Ensure source_file module is declared

// Core HasName trait (defined here)
pub trait HasName: AstNode<Language = BhdlLanguage> {
    /// Returns the name token associated with this node.
    fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
}

// Consolidated Re-exports
pub use source_file::SourceFile;
pub use items::{Board, Module, ComponentDef, InterfaceDef, TypeDef, ImportStmt, ImportPath, Alias, ImportTarget, ImportTargetGroup, ImportTargetKind};
pub use blocks::{ParametersBlock, PortsBlock, NetsBlock, PinsBlock, LayerStackupBlock, PinMapBlock, ConstrainBlock, DefaultDesignRulesBlock, InterfacesBlock};
pub use common::{ParamAssign, PortDecl, PinDecl, NetDecl, TypeRef, BusSuffix, RangeExpr, Value, ComponentInst, ConnectionStmt, PinRef, NetRef, IdentRef, SimpleIdentRef, ComponentType, PortDirection, ParamDecl, ParamAssignBlock};
pub use expr::{Expr, PrefixExpr, BinaryExpr};

// Add tests module
#[cfg(test)]
mod tests; 