use crate::{Node, BhdlLanguage};
use bhdl_parser::SyntaxKind;
use rowan::ast::AstNode;

// Add definitions for top-level items later
// use crate::definitions::TopLevelItem;

/// Represents the root of a BHDL file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile(pub(crate) Node);

impl AstNode for SourceFile {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SOURCE_FILE
    }

    fn cast(node: Node) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }

    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl SourceFile {
    /// Returns an iterator over the top-level items in the file.
    pub fn items(&self) -> impl Iterator<Item = Node> { // Return generic Node for now
        // TODO: Map to specific TopLevelItem enum variants later
        self.syntax().children()
    }

    // Add more specific accessors as needed
} 