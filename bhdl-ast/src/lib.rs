use bhdl_parser::{SyntaxKind, BhdlLanguage};
use rowan::{SyntaxNode, SyntaxToken};

pub type Node = SyntaxNode<BhdlLanguage>;
pub type Token = SyntaxToken<BhdlLanguage>;

/// Helper trait for AST nodes that have a name.
pub trait HasName: rowan::ast::AstNode<Language = BhdlLanguage> {
    /// Returns the name token associated with this node.
    fn name(&self) -> Option<Token> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
}

// Modules for specific AST node types
pub mod blocks;
pub mod items;
pub mod common;
// pub mod expressions; // Future module
// pub mod types; // Future module for type expressions etc.

pub mod source_file; // Declare the module

// Re-export core types
pub use source_file::SourceFile;

// Re-export items
pub use items::{Board, ComponentDef, InterfaceDef, Module, TypeDef, ImportStmt};
// Add other items like InterfaceDef, etc. here as they are defined

// Re-export blocks
pub use blocks::{
    ComponentsBlock, ConnectionsBlock, ConstrainBlock, DefaultDesignRulesBlock, InterfacesBlock,
    LayerStackupBlock, NetsBlock, ParametersBlock, PinMapBlock, PinsBlock, PortsBlock,
    /* PropertySetBlock, */ TypeDefBlock,
}; // Commented out PropertySetBlock
// Add other blocks here as they are defined

// Re-export common elements
pub use common::{
    BusSuffix, ComponentInst, ComponentType, ConnectionStmt, IdentRef, InterfaceInstance, NetDecl,
    NetRef, ParamAssign, ParamAssignBlock, ParamDecl, PinDecl, PinRef, PortDecl, PortDirection,
    RangeExpr, TypeRef, Value,
};
// Add other common elements here as they are defined

// Add tests module
#[cfg(test)]
mod tests; 