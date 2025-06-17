// BHDL v2.0 AST Blocks
// Only supports v2.0 flow-based syntax

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken};
use rowan::ast::{AstNode, AstChildren};
use crate::common::{ParamAssign, PortDecl, ComponentInst, NetDecl};
use crate::v2_statements::ConnectionStmt;

// v2.0 only has a few specialized blocks, most content is now direct statements

// --- Layer Stackup Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayerStackupBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for LayerStackupBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LAYER_STACKUP_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl LayerStackupBlock {
    pub fn layers(&self) -> impl Iterator<Item = LayerDef> {
        self.0.children().filter_map(LayerDef::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LayerDef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for LayerDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::LAYER_DEF
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

// --- Default Design Rules Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DefaultDesignRulesBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for DefaultDesignRulesBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::DEFAULT_DESIGN_RULES_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl DefaultDesignRulesBlock {
    pub fn rules(&self) -> impl Iterator<Item = ParamAssign> {
        self.0.children().filter_map(ParamAssign::cast)
    }
}

// --- Constrain Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConstrainBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ConstrainBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONSTRAIN_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

// --- Generate Block ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenerateBlock(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for GenerateBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::GENERATE_BLOCK
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl GenerateBlock {
    pub fn for_loops(&self) -> impl Iterator<Item = ForLoopGenerate> {
        self.0.children().filter_map(ForLoopGenerate::cast)
    }
    
    pub fn if_generates(&self) -> impl Iterator<Item = IfGenerate> {
        self.0.children().filter_map(IfGenerate::cast)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ForLoopGenerate(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ForLoopGenerate {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FOR_LOOP_GENERATE
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IfGenerate(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for IfGenerate {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IF_GENERATE
    }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}