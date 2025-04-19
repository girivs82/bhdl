use crate::{AstNode, Node, common::{ParamDecl, PortDecl, ComponentInst, NetDecl, ConnectionStmt, PinDecl, InterfaceInstance}, BhdlLanguage};
use bhdl_parser::SyntaxKind;
use rowan::{ast::support, SyntaxNode};

// Macro to simplify boilerplate for block structs
macro_rules! define_block {
    ($name:ident, $kind:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq)]
        pub struct $name(Node);

        // Implement our local AstNode trait
        impl crate::AstNode for $name {
            fn can_cast(kind: SyntaxKind) -> bool {
                kind == $kind
            }
            fn cast(node: Node) -> Option<Self> {
                if <Self as crate::AstNode>::can_cast(node.kind()) {
                    Some(Self(node))
                } else {
                    None
                }
            }
            fn syntax(&self) -> &Node {
                &self.0
            }
        }

        // Implement rowan::ast::AstNode for compatibility with support functions
        impl rowan::ast::AstNode for $name {
            type Language = BhdlLanguage;

            fn can_cast(kind: SyntaxKind) -> bool {
                <Self as crate::AstNode>::can_cast(kind)
            }

            fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
                 <Self as crate::AstNode>::cast(node)
            }

            fn syntax(&self) -> &SyntaxNode<Self::Language> {
                <Self as crate::AstNode>::syntax(self)
            }
        }
    };
}

define_block!(ParametersBlock, SyntaxKind::PARAMETERS_BLOCK);
impl ParametersBlock {
    pub fn parameters(&self) -> impl Iterator<Item = ParamDecl> {
        support::children(&self.0)
    }
}

define_block!(PortsBlock, SyntaxKind::PORTS_BLOCK);
impl PortsBlock {
    pub fn ports(&self) -> impl Iterator<Item = PortDecl> {
        support::children(&self.0)
    }
}

define_block!(LayerStackupBlock, SyntaxKind::LAYER_STACKUP_BLOCK);
// TODO: Add methods to access individual LayerDef nodes

define_block!(DefaultDesignRulesBlock, SyntaxKind::DEFAULT_DESIGN_RULES_BLOCK);
// TODO: Add methods to access individual rule assignments

define_block!(ComponentsBlock, SyntaxKind::COMPONENTS_BLOCK);
impl ComponentsBlock {
    pub fn components(&self) -> impl Iterator<Item = ComponentInst> {
        support::children(&self.0)
    }
}

define_block!(NetsBlock, SyntaxKind::NETS_BLOCK);
impl NetsBlock {
    pub fn nets(&self) -> impl Iterator<Item = NetDecl> {
        support::children(&self.0)
    }
}

define_block!(ConnectionsBlock, SyntaxKind::CONNECTIONS_BLOCK);
impl ConnectionsBlock {
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        support::children(&self.0)
    }
}

define_block!(PinsBlock, SyntaxKind::PINS_BLOCK);
impl PinsBlock {
    pub fn pins(&self) -> impl Iterator<Item = PinDecl> {
        support::children(&self.0)
    }
}

define_block!(InterfacesBlock, SyntaxKind::INTERFACES_BLOCK);
impl InterfacesBlock {
    pub fn interfaces(&self) -> impl Iterator<Item = InterfaceInstance> {
        support::children(&self.0)
    }
}

define_block!(PinMapBlock, SyntaxKind::PIN_MAP_BLOCK);
// TODO: Add methods to access individual PinMapEntry nodes

define_block!(ConstrainBlock, SyntaxKind::CONSTRAIN_BLOCK);
// TODO: Add methods to access target and constraint assignments

define_block!(TypeDefBlock, SyntaxKind::TYPEDEF_DEF); // Note: This is a definition, not just a block
// TODO: Implement HasName, methods for base type, properties

// define_block!(PropertySetBlock, SyntaxKind::PROPERTY_SET_BLOCK); // Assuming a PROPERTY_SET_BLOCK kind exists or will be added
// TODO: Add methods to access properties

// Add other block types as needed (e.g., GenerateBlock) 