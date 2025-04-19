use crate::{
    blocks::{
        ComponentsBlock, ConnectionsBlock, DefaultDesignRulesBlock, InterfacesBlock, LayerStackupBlock, NetsBlock,
        ParametersBlock, PinsBlock, PortsBlock,
    },
    AstNode, HasName, Node, Token, BhdlLanguage,
};
use bhdl_parser::SyntaxKind;
use rowan::{ast::support, SyntaxNode};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board(Node);

impl AstNode for Board {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BOARD_DEF
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

impl HasName for Board {}

// Methods to access board contents
impl Board {
    pub fn parameters_block(&self) -> Option<ParametersBlock> {
        support::child(&self.0)
    }

    pub fn ports_block(&self) -> Option<PortsBlock> {
        support::child(&self.0)
    }

    pub fn layer_stackup_block(&self) -> Option<LayerStackupBlock> {
        support::child(&self.0)
    }

    pub fn default_design_rules_block(&self) -> Option<DefaultDesignRulesBlock> {
        support::child(&self.0)
    }

    pub fn components_block(&self) -> Option<ComponentsBlock> {
        support::child(&self.0)
    }

    pub fn nets_block(&self) -> Option<NetsBlock> {
        support::child(&self.0)
    }

    pub fn connections_block(&self) -> Option<ConnectionsBlock> {
        support::child(&self.0)
    }

    // TODO: Add methods for other optional blocks like `constrain`, metadata assignments (author, version)
}

// --- Module Definition ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module(Node);

impl AstNode for Module {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::MODULE_DEF
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

impl HasName for Module {}

// Methods to access module contents
impl Module {
    // Modules typically have ports, parameters, internal components, nets, connections, pins, interfaces
    pub fn ports_block(&self) -> Option<PortsBlock> {
        support::child(&self.0)
    }

    pub fn parameters_block(&self) -> Option<ParametersBlock> {
        support::child(&self.0)
    }

    pub fn components_block(&self) -> Option<ComponentsBlock> {
        support::child(&self.0)
    }

    pub fn nets_block(&self) -> Option<NetsBlock> {
        support::child(&self.0)
    }

    pub fn connections_block(&self) -> Option<ConnectionsBlock> {
        support::child(&self.0)
    }

    pub fn pins_block(&self) -> Option<PinsBlock> {
        support::child(&self.0) // Might be present in some module types? Check spec usage. Often in components.
    }

    pub fn interfaces_block(&self) -> Option<InterfacesBlock> {
        support::child(&self.0) // Modules can expose interfaces
    }

    // TODO: Add methods for constrain blocks, etc.
}

// --- Component Definition ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDef(Node);

impl AstNode for ComponentDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENT_DEF
    }

    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }

    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl HasName for ComponentDef {}

// Methods to access component definition contents
impl ComponentDef {
    // Components typically have parameters, pins, and potentially interfaces
    pub fn parameters_block(&self) -> Option<ParametersBlock> {
        support::child(&self.0)
    }

    pub fn pins_block(&self) -> Option<PinsBlock> {
        support::child(&self.0)
    }

    pub fn interfaces_block(&self) -> Option<InterfacesBlock> {
        support::child(&self.0)
    }

    // TODO: Add methods for properties like footprint, package, etc.
}

// --- Interface Definition ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDef(Node);

impl AstNode for InterfaceDef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::INTERFACE_DEF
    }

    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }

    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl HasName for InterfaceDef {}

// Methods to access interface definition contents
impl InterfaceDef {
    // Interfaces typically define parameters and pins
    pub fn parameters_block(&self) -> Option<ParametersBlock> {
        support::child(&self.0)
    }

    pub fn pins_block(&self) -> Option<PinsBlock> {
        support::child(&self.0)
    }
    // Interfaces might also have ports if they are module-like? Check spec. Primarily pins.
}

// Add other top-level items like TypeDef later 