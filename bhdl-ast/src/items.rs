// BHDL v2.0 AST Items
// Only supports v2.0 flow-based syntax

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use rowan::ast::{AstNode, AstChildren};
use crate::blocks::{LayerStackupBlock, DefaultDesignRulesBlock, ConstrainBlock};
use crate::common::{TypeRef, ParamAssign, ComponentInst, PortDecl, PinDecl, NetDecl};
use crate::v2_statements::ConnectionStmt;
use crate::expr::Expr;
use crate::v2_statements::{PowerDecl, GroundDecl, FlowStmt};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Board(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Board {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::BOARD_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for Board {}

impl Board {
    // v2.0 boards have power/ground declarations and connection statements
    pub fn power_decls(&self) -> impl Iterator<Item = PowerDecl> {
        self.0.children().filter_map(PowerDecl::cast)
    }
    
    pub fn ground_decls(&self) -> impl Iterator<Item = GroundDecl> {
        self.0.children().filter_map(GroundDecl::cast)
    }
    
    pub fn connections(&self) -> impl Iterator<Item = ConnectionStmt> {
        self.0.children().filter_map(ConnectionStmt::cast)
    }
    
    pub fn flow_statements(&self) -> impl Iterator<Item = FlowStmt> {
        self.0.children().filter_map(FlowStmt::cast)
    }
    
    pub fn layer_stackup_block(&self) -> Option<LayerStackupBlock> { 
        self.0.children().find_map(LayerStackupBlock::cast) 
    }
    
    pub fn default_design_rules_block(&self) -> Option<DefaultDesignRulesBlock> { 
        self.0.children().find_map(DefaultDesignRulesBlock::cast) 
    }
    
    pub fn constrain_block(&self) -> Option<ConstrainBlock> {
        self.0.children().find_map(ConstrainBlock::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Module(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for Module {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MODULE_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for Module {}

impl Module {
    // v2.0 modules have pins and metadata
    pub fn pins(&self) -> impl Iterator<Item = PinDecl> {
        self.0.children().filter_map(PinDecl::cast)
    }
    
    // Keep ports() for compatibility with higher-level modules that might use ports
    pub fn ports(&self) -> impl Iterator<Item = PortDecl> {
        self.0.children().filter_map(PortDecl::cast)
    }
    
    pub fn param_list(&self) -> Option<ParamList> {
        self.0.children().find_map(ParamList::cast)
    }
    
    // Module metadata (attributes)
    pub fn attributes(&self) -> impl Iterator<Item = SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|it| it.into_token())
            .filter(|token| token.kind() == SyntaxKind::AT)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ComponentDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::COMPONENT_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for ComponentDef {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfaceDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::INTERFACE_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for InterfaceDef {}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedefDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TypedefDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPEDEF_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for TypedefDef {}

impl TypedefDef {
    pub fn base_type(&self) -> Option<TypedefBase> {
        self.0.children().find_map(TypedefBase::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypedefBase(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TypedefBase {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPEDEF_BASE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamList(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ParamList {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PARAM_LIST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ParamList {
    pub fn params(&self) -> impl Iterator<Item = ParamAssign> {
        self.0.children().filter_map(ParamAssign::cast)
    }
}