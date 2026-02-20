use crate::{SyntaxKind, BhdlLanguage, SyntaxNode};
use rowan::ast::AstNode;
use crate::items::{Board, Entity, ComponentDef, InterfaceDef, TypedefDef, ImportStmt};
use crate::testbench::TestbenchDef;

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

    /// Returns an iterator over imports in the file.
    pub fn imports(&self) -> impl Iterator<Item = ImportStmt> {
        self.0.children().filter_map(ImportStmt::cast)
    }

    /// Returns an iterator over boards in the file.
    pub fn boards(&self) -> impl Iterator<Item = Board> {
        self.0.children().filter_map(Board::cast)
    }

    /// Returns an iterator over entities in the file.
    pub fn entities(&self) -> impl Iterator<Item = Entity> {
        self.0.children().filter_map(Entity::cast)
    }

    /// Returns an iterator over testbenches in the file.
    pub fn testbenches(&self) -> impl Iterator<Item = TestbenchDef> {
        self.0.children().filter_map(TestbenchDef::cast)
    }

    // Add more specific accessors as needed
}

// Enum to represent any top-level item
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Item {
    ImportStmt(ImportStmt),
    Board(Board),
    Entity(Entity),
    ComponentDef(ComponentDef),
    InterfaceDef(InterfaceDef),
    TypedefDef(TypedefDef),
    TestbenchDef(TestbenchDef),
    // Add others like StructDef, EnumDef
}

impl AstNode for Item {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        ImportStmt::can_cast(kind) || Board::can_cast(kind) || Entity::can_cast(kind) || ComponentDef::can_cast(kind) ||
        InterfaceDef::can_cast(kind) || TypedefDef::can_cast(kind) || TestbenchDef::can_cast(kind)
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if ImportStmt::can_cast(syntax.kind()) { Some(Item::ImportStmt(ImportStmt::cast(syntax)?)) }
        else if Board::can_cast(syntax.kind()) { Some(Item::Board(Board::cast(syntax)?)) }
        else if Entity::can_cast(syntax.kind()) { Some(Item::Entity(Entity::cast(syntax)?)) }
        else if ComponentDef::can_cast(syntax.kind()) { Some(Item::ComponentDef(ComponentDef::cast(syntax)?)) }
        else if InterfaceDef::can_cast(syntax.kind()) { Some(Item::InterfaceDef(InterfaceDef::cast(syntax)?)) }
        else if TypedefDef::can_cast(syntax.kind()) { Some(Item::TypedefDef(TypedefDef::cast(syntax)?)) }
        else if TestbenchDef::can_cast(syntax.kind()) { Some(Item::TestbenchDef(TestbenchDef::cast(syntax)?)) }
        else { None }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        match self {
            Item::ImportStmt(i) => i.syntax(),
            Item::Board(i) => i.syntax(),
            Item::Entity(i) => i.syntax(),
            Item::ComponentDef(i) => i.syntax(),
            Item::InterfaceDef(i) => i.syntax(),
            Item::TypedefDef(i) => i.syntax(),
            Item::TestbenchDef(i) => i.syntax(),
        }
    }
} 