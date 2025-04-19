use crate::{Node, BhdlLanguage};
// Explicitly import types used in support::children calls below
use crate::common::{ParamDecl, PortDecl, ComponentInst, NetDecl, ConnectionStmt, PinDecl, InterfaceInstance};
use bhdl_parser::SyntaxKind;
use rowan::{ast::support, SyntaxNode};
use rowan::ast::AstNode;

// --- Parameters Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParametersBlock(Node);

// Manual impl
impl AstNode for ParametersBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PARAMETERS_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ParametersBlock {
    pub fn parameters(&self) -> impl Iterator<Item = ParamDecl> {
        support::children(&self.0)
    }
}

// --- Ports Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortsBlock(Node);

// Manual impl
impl AstNode for PortsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PORTS_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl PortsBlock {
    pub fn ports(&self) -> impl Iterator<Item = PortDecl> {
        support::children(&self.0)
    }
}

// --- Layer Stackup Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LayerStackupBlock(Node);

// Manual impl
impl AstNode for LayerStackupBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LAYER_STACKUP_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

// TODO: Add methods

// --- Default Design Rules Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefaultDesignRulesBlock(Node);

// Manual impl
impl AstNode for DefaultDesignRulesBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::DEFAULT_DESIGN_RULES_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

// TODO: Add methods

// --- Components Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentsBlock(Node);

// Manual impl
impl AstNode for ComponentsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENTS_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ComponentsBlock {
    pub fn components(&self) -> impl Iterator<Item = ComponentInst> {
        support::children(&self.0)
    }
}

// --- Nets Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetsBlock(Node);

// Manual impl
impl AstNode for NetsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NETS_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl NetsBlock {
    pub fn nets(&self) -> impl Iterator<Item = NetDecl> {
        support::children(&self.0)
    }
}

// --- Connections Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionsBlock(Node);

// Manual impl
impl AstNode for ConnectionsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONNECTIONS_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ConnectionsBlock {
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        support::children(&self.0)
    }
}

// --- Pins Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinsBlock(Node);

// Manual impl
impl AstNode for PinsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PINS_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl PinsBlock {
    pub fn pins(&self) -> impl Iterator<Item = PinDecl> {
        support::children(&self.0)
    }
}

// --- Interfaces Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfacesBlock(Node);

// Manual impl
impl AstNode for InterfacesBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::INTERFACES_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl InterfacesBlock {
    pub fn interfaces(&self) -> impl Iterator<Item = InterfaceInstance> {
        support::children(&self.0)
    }
}

// --- Pin Map Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinMapBlock(Node);

// Manual impl
impl AstNode for PinMapBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PIN_MAP_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

// TODO: Add methods

// --- Constrain Block ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstrainBlock(Node);

// Manual impl
impl AstNode for ConstrainBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONSTRAIN_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

// TODO: Add methods

// --- TypeDef Block (This might be better placed in items.rs) ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDefBlock(Node); // Should this be TypeDef from items.rs?

// Manual impl - NOTE: Should match items::TypeDef if it's the same concept
impl AstNode for TypeDefBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TYPEDEF_DEF // Assuming this block corresponds to a TypeDef node
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

// TODO: Implement methods, possibly merge with items::TypeDef

// Add other block types as needed (e.g., GenerateBlock) 