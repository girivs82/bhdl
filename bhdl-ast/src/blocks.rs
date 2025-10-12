// BHDL v2.0 AST Blocks
// Only supports v2.0 flow-based syntax

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode};
use rowan::ast::AstNode;
use crate::common::ParamAssign;

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

impl ForLoopGenerate {
    /// Get the loop variable name (e.g., "i" in `generate for i in 0..15`)
    pub fn loop_var(&self) -> Option<String> {
        // Find the first IDENT after the FOR keyword
        let mut found_for = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::FOR_KW {
                    found_for = true;
                } else if found_for && token.kind() == SyntaxKind::IDENT {
                    return Some(token.text().to_string());
                }
            }
        }
        None
    }

    /// Get the range bounds (start, end) for the loop (e.g., `(0, 15)` from `generate for i in 0..15`)
    pub fn range_bounds(&self) -> Option<(i32, i32)> {
        // The range is represented as: VALUE (containing NUMBER) DOT_DOT NUMBER
        // We need to extract both numbers
        let mut numbers = Vec::new();

        // Traverse all tokens to find NUMBER tokens
        for element in self.0.descendants_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::NUMBER {
                    if let Ok(num) = token.text().parse::<i32>() {
                        numbers.push(num);
                    }
                }
            }
        }

        // We should have at least 2 numbers: start and end
        // (may have more if there are numbers in component parameters)
        if numbers.len() >= 2 {
            Some((numbers[0], numbers[1]))
        } else {
            None
        }
    }

    /// Get all component instances within the generate block
    pub fn component_instances(&self) -> impl Iterator<Item = crate::common::ComponentInst> {
        self.0.children().filter_map(crate::common::ComponentInst::cast)
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