use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken};
use rowan::ast::{AstNode, AstChildren};
use crate::common::{ParamAssign, PortDecl, ComponentInst, NetDecl, ConnectionStmt, PinDecl, InterfaceInstance};

// --- Parameters Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParametersBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ParametersBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PARAMETERS_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl ParametersBlock {
    pub fn parameters(&self) -> impl Iterator<Item = ParamAssign> {
        self.0.children().filter_map(ParamAssign::cast)
    }
}

// --- Ports Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortsBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PortsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PORTS_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl PortsBlock {
    pub fn ports(&self) -> impl Iterator<Item = PortDecl> {
        self.0.children().filter_map(PortDecl::cast)
    }
}

// --- Layer Stackup Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayerStackupBlock(pub(crate) SyntaxNode<BhdlLanguage>);

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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DefaultDesignRulesBlock(pub(crate) SyntaxNode<BhdlLanguage>);

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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentsBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ComponentsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENTS_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl ComponentsBlock {
    pub fn components(&self) -> impl Iterator<Item = ComponentInst> {
        self.0.children().filter_map(ComponentInst::cast)
    }
}

// --- Nets Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetsBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for NetsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NETS_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl NetsBlock {
    pub fn nets(&self) -> impl Iterator<Item = NetDecl> {
        self.0.children().filter_map(NetDecl::cast)
    }
}

// --- Connections Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionsBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ConnectionsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONNECTIONS_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl ConnectionsBlock {
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }
}

// --- Pins Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PinsBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PinsBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PINS_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl PinsBlock {
    pub fn pins(&self) -> impl Iterator<Item = PinDecl> {
        self.0.children().filter_map(PinDecl::cast)
    }
}

// --- Interfaces Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfacesBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfacesBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::INTERFACES_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl InterfacesBlock {
    pub fn interfaces(&self) -> impl Iterator<Item = InterfaceInstance> {
        self.0.children().filter_map(InterfaceInstance::cast)
    }
}

// --- Pin Map Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PinMapBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PinMapBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PIN_MAP_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl PinMapBlock {
    // Add methods later if needed, e.g., entries()
}

// --- Constrain Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstrainBlock(pub(crate) SyntaxNode<BhdlLanguage>);

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
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDefBlock(pub(crate) SyntaxNode<BhdlLanguage>);

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