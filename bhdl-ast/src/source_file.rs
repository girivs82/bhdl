use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken};
use rowan::ast::AstNode;
use crate::items::{Board, Module, ComponentDef, InterfaceDef, TypeDef, ImportStmt};

// Add definitions for top-level items later
// use crate::definitions::TopLevelItem;

/// Represents the root of a BHDL file.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SourceFile(SyntaxNode<BhdlLanguage>);

impl AstNode for SourceFile {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SOURCE_FILE
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self(syntax))
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl SourceFile {
    /// Returns an iterator over the top-level items in the file.
    pub fn items(&self) -> impl Iterator<Item = Item> {
        self.0.children().filter_map(Item::cast)
    }

    // Add more specific accessors as needed
}

// Enum to represent any top-level item
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    Board(Board),
    Module(Module),
    ComponentDef(ComponentDef),
    InterfaceDef(InterfaceDef),
    TypeDef(TypeDef),
    ImportStmt(ImportStmt),
    // Add others like StructDef, EnumDef
}

impl AstNode for Item {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        Board::can_cast(kind) || Module::can_cast(kind) || ComponentDef::can_cast(kind) ||
        InterfaceDef::can_cast(kind) || TypeDef::can_cast(kind) || ImportStmt::can_cast(kind)
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Board::can_cast(syntax.kind()) { Some(Item::Board(Board::cast(syntax)?)) }
        else if Module::can_cast(syntax.kind()) { Some(Item::Module(Module::cast(syntax)?)) }
        else if ComponentDef::can_cast(syntax.kind()) { Some(Item::ComponentDef(ComponentDef::cast(syntax)?)) }
        else if InterfaceDef::can_cast(syntax.kind()) { Some(Item::InterfaceDef(InterfaceDef::cast(syntax)?)) }
        else if TypeDef::can_cast(syntax.kind()) { Some(Item::TypeDef(TypeDef::cast(syntax)?)) }
        else if ImportStmt::can_cast(syntax.kind()) { Some(Item::ImportStmt(ImportStmt::cast(syntax)?)) }
        else { None }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        match self {
            Item::Board(i) => i.syntax(),
            Item::Module(i) => i.syntax(),
            Item::ComponentDef(i) => i.syntax(),
            Item::InterfaceDef(i) => i.syntax(),
            Item::TypeDef(i) => i.syntax(),
            Item::ImportStmt(i) => i.syntax(),
        }
    }
} 