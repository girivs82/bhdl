use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use rowan::ast::{AstNode, AstChildren};
use crate::blocks::{ParametersBlock, PortsBlock, NetsBlock, PinsBlock, LayerStackupBlock, DefaultDesignRulesBlock, InterfacesBlock, ConstrainBlock};
use crate::common::{TypeRef, ParamAssign, ComponentInst, ConnectionStmt, PortDecl, NetDecl, PinDecl};
use crate::expr::Expr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Board(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Board {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::BOARD_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for Board {}

impl Board {
    pub fn parameters_block(&self) -> Option<ParametersBlock> { self.0.children().find_map(ParametersBlock::cast) }
    pub fn ports_block(&self) -> Option<PortsBlock> { self.0.children().find_map(PortsBlock::cast) }
    pub fn layer_stackup_block(&self) -> Option<LayerStackupBlock> { self.0.children().find_map(LayerStackupBlock::cast) }
    pub fn default_design_rules_block(&self) -> Option<DefaultDesignRulesBlock> { self.0.children().find_map(DefaultDesignRulesBlock::cast) }
    pub fn nets_block(&self) -> Option<NetsBlock> { self.0.children().find_map(NetsBlock::cast) }
    pub fn interfaces_block(&self) -> Option<InterfacesBlock> { self.0.children().find_map(InterfacesBlock::cast) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Module(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Module {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MODULE_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for Module {}

impl Module {
    pub fn ports_block(&self) -> Option<PortsBlock> { self.0.children().find_map(PortsBlock::cast) }
    pub fn parameters_block(&self) -> Option<ParametersBlock> { self.0.children().find_map(ParametersBlock::cast) }
    pub fn nets_block(&self) -> Option<NetsBlock> { self.0.children().find_map(NetsBlock::cast) }
    pub fn pins_block(&self) -> Option<PinsBlock> { self.0.children().find_map(PinsBlock::cast) }
    pub fn interfaces_block(&self) -> Option<InterfacesBlock> { self.0.children().find_map(InterfacesBlock::cast) }
    pub fn component_instances(&self) -> impl Iterator<Item = ComponentInst> {
        self.0.children().filter_map(ComponentInst::cast)
    }
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ComponentDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::COMPONENT_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for ComponentDef {}

impl ComponentDef {
    pub fn parameters_block(&self) -> Option<ParametersBlock> { self.0.children().find_map(ParametersBlock::cast) }
    pub fn pins_block(&self) -> Option<PinsBlock> { self.0.children().find_map(PinsBlock::cast) }
    pub fn interfaces_block(&self) -> Option<InterfacesBlock> { self.0.children().find_map(InterfacesBlock::cast) }
    pub fn constraint_block(&self) -> Option<ConstrainBlock> { self.0.children().find_map(ConstrainBlock::cast) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfaceDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::INTERFACE_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for InterfaceDef {}

impl InterfaceDef {
    pub fn parameters_block(&self) -> Option<ParametersBlock> { self.0.children().find_map(ParametersBlock::cast) }
    pub fn pins_block(&self) -> Option<PinsBlock> { self.0.children().find_map(PinsBlock::cast) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TypeDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPEDEF_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for TypeDef {}

impl TypeDef {
    pub fn base_type(&self) -> Option<TypeDefBase> { self.0.children().find_map(TypeDefBase::cast) }
    pub fn param_assigns(&self) -> impl Iterator<Item = ParamAssign> {
        self.0.children().filter_map(ParamAssign::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeDefBase(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TypeDefBase {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPEDEF_BASE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl TypeDefBase {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> { self.0.first_token() }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportStmt(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ImportStmt {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::IMPORT_STMT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ImportStmt {
    pub fn path(&self) -> Option<ImportPath> { self.0.children().find_map(ImportPath::cast) }
    pub fn target(&self) -> Option<ImportTargetKind> {
        self.0.children().find_map(ImportTarget::cast).map(ImportTargetKind::Simple)
            .or_else(|| self.0.children().find_map(ImportTargetGroup::cast).map(ImportTargetKind::Group))
    }
    pub fn alias(&self) -> Option<Alias> { self.0.children().find_map(Alias::cast) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportPath(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ImportPath {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::IMPORT_PATH }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ImportPath {
    pub fn segments(&self) -> impl Iterator<Item = SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportTarget(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ImportTarget {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::IMPORT_TARGET }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ImportTarget {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> { self.0.first_token() }
    pub fn is_wildcard(&self) -> bool { self.0.first_token().map_or(false, |t| t.kind() == SyntaxKind::STAR) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ImportTargetGroup(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ImportTargetGroup {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::IMPORT_TARGET_GROUP }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ImportTargetGroup {
    pub fn targets(&self) -> impl Iterator<Item = SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ImportTargetKind {
    Simple(ImportTarget),
    Group(ImportTargetGroup),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Alias(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Alias {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ALIAS }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl Alias {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
} 