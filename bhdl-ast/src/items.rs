use crate::{
    blocks::{
        ComponentsBlock, ConnectionsBlock, DefaultDesignRulesBlock, InterfacesBlock, LayerStackupBlock, NetsBlock,
        ParametersBlock, PinsBlock, PortsBlock,
    },
    HasName, Node, Token, BhdlLanguage,
};
// Explicitly import type used in support::children calls below
use bhdl_parser::SyntaxKind;
use rowan::{ast::support, SyntaxNode};
use rowan::ast::AstNode;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Board(Node);

impl rowan::ast::AstNode for Board {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::BOARD_DEF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
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

impl rowan::ast::AstNode for Module {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::MODULE_DEF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
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

impl rowan::ast::AstNode for ComponentDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENT_DEF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
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

impl rowan::ast::AstNode for InterfaceDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::INTERFACE_DEF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
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

// --- Typedef Definition ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDef(Node);

impl rowan::ast::AstNode for TypeDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::TYPEDEF_DEF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl HasName for TypeDef {}

impl TypeDef {
    /// Returns the base type this typedef extends, if any.
    pub fn base_type(&self) -> Option<TypeDefBase> {
        support::child(&self.0)
    }

    /// Returns an iterator over the parameter assignments in the typedef body.
    pub fn param_assigns(&self) -> impl Iterator<Item = crate::common::ParamAssign> {
        // Find all PARAM_ASSIGN nodes within this TypeDef node
        support::children(&self.0)
    }
}

// Wrapper for the base type identifier in `extends BaseType`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeDefBase(Node);

impl rowan::ast::AstNode for TypeDefBase {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::TYPEDEF_BASE
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl TypeDefBase {
    /// Returns the name token of the base type.
    pub fn name(&self) -> Option<Token> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
}

// --- Import Statement ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportStmt(Node);

impl rowan::ast::AstNode for ImportStmt {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::IMPORT_STMT
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ImportStmt {
    /// Returns the import path node.
    pub fn path(&self) -> Option<ImportPath> {
        support::child(&self.0)
    }

    /// Returns the import target (simple or group), if present.
    /// Note: A simple import like `import a.b.c;` technically has an empty `IMPORT_TARGET` node.
    pub fn target(&self) -> Option<ImportTargetKind> {
        if let Some(group) = support::child::<ImportTargetGroup>(&self.0) {
            Some(ImportTargetKind::Group(group))
        } else if let Some(simple) = support::child::<ImportTarget>(&self.0) {
            Some(ImportTargetKind::Simple(simple))
        } else {
            None
        }
    }

    /// Returns the alias node, if present (e.g., `import a as b;`).
    pub fn alias(&self) -> Option<Alias> {
        support::child(&self.0)
    }
}

// Wrapper for the import path
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPath(Node);

impl rowan::ast::AstNode for ImportPath {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::IMPORT_PATH
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

// Wrapper for the simple import target (usually empty, target implied by path)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportTarget(Node);

impl rowan::ast::AstNode for ImportTarget {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::IMPORT_TARGET
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

// Wrapper for the grouped import target { A, B, ... }
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportTargetGroup(Node);

impl rowan::ast::AstNode for ImportTargetGroup {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::IMPORT_TARGET_GROUP
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ImportTargetGroup {
    /// Returns an iterator over the identifier tokens within the group.
    pub fn targets(&self) -> impl Iterator<Item = Token> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::IDENT)
    }
}

// Wrapper for the alias identifier
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alias(Node);

impl rowan::ast::AstNode for Alias {
    type Language = BhdlLanguage;
    fn can_cast(kind: bhdl_parser::SyntaxKind) -> bool {
        kind == SyntaxKind::ALIAS
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if <Self as rowan::ast::AstNode>::can_cast(node.kind()) {
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl Alias {
    /// Returns the alias name token.
    pub fn name(&self) -> Option<Token> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
}

// Enum to represent the kind of import target
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportTargetKind {
    Simple(ImportTarget),
    Group(ImportTargetGroup),
} 