//! AST nodes for the trait system.
//!
//! Provides typed wrappers around trait definitions and implementations:
//! - `TraitDef`: A trait declaration with required pins and constants
//! - `TraitImpl`: An implementation of a trait for a component
//! - `TraitPin`: A pin declaration within a trait
//! - `TraitConst`: A constant declaration within a trait

use crate::{AstNode, HasName, SyntaxKind, SyntaxNode, SyntaxToken, BhdlLanguage};
use rowan::ast::AstNode as RowanAstNode;

/// A trait definition.
///
/// ```bhdl
/// trait SpiPeripheral {
///     pin MOSI: signal in;
///     pin MISO: signal out;
///     const MAX_FREQ: frequency;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TraitDef {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl RowanAstNode for TraitDef {
    type Language = BhdlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        kind == SyntaxKind::TRAIT_DEF
    }

    fn cast(node: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self { syntax: node })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl HasName for TraitDef {}

impl TraitDef {
    /// Get all pin declarations in the trait.
    pub fn pins(&self) -> impl Iterator<Item = TraitPin> {
        self.syntax.children().filter_map(TraitPin::cast)
    }

    /// Get all const declarations in the trait.
    pub fn consts(&self) -> impl Iterator<Item = TraitConst> {
        self.syntax.children().filter_map(TraitConst::cast)
    }

    /// Get pin names as strings.
    pub fn pin_names(&self) -> Vec<String> {
        self.pins()
            .filter_map(|p| p.name())
            .map(|t| t.text().to_string())
            .collect()
    }

    /// Get const names as strings.
    pub fn const_names(&self) -> Vec<String> {
        self.consts()
            .filter_map(|c| c.name())
            .map(|t| t.text().to_string())
            .collect()
    }
}

/// A trait implementation for a component.
///
/// ```bhdl
/// impl PowerRegulator for LM7805 {
///     const DROPOUT = 2.0V;
/// }
/// ```
#[derive(Debug, Clone)]
pub struct TraitImpl {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl RowanAstNode for TraitImpl {
    type Language = BhdlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        kind == SyntaxKind::TRAIT_IMPL
    }

    fn cast(node: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self { syntax: node })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl TraitImpl {
    /// Get trait names being implemented (may include ~ prefix for direction flipping).
    pub fn trait_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        let mut flip_next = false;

        for element in self.syntax.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::IMPL_KW => continue,
                    SyntaxKind::FOR_KW => break, // Everything after 'for' is the component name
                    SyntaxKind::TILDE => {
                        flip_next = true;
                    }
                    SyntaxKind::IDENT => {
                        let prefix = if flip_next { "~" } else { "" };
                        names.push(format!("{}{}", prefix, token.text()));
                        flip_next = false;
                    }
                    SyntaxKind::COMMA => continue,
                    _ => continue,
                }
            }
        }
        names
    }

    /// Get the component name this trait is implemented for.
    pub fn component_name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        let mut found_for = false;
        for element in self.syntax.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::FOR_KW {
                    found_for = true;
                } else if found_for && token.kind() == SyntaxKind::IDENT {
                    return Some(token.clone());
                }
            }
        }
        None
    }

    /// Whether direction flipping (~) is used for any trait.
    pub fn has_direction_flip(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .any(|e| e.as_token().map(|t| t.kind() == SyntaxKind::TILDE).unwrap_or(false))
    }

    /// Get const implementations in the body.
    pub fn consts(&self) -> impl Iterator<Item = TraitConst> {
        self.syntax.children().filter_map(TraitConst::cast)
    }

    /// Get pin overrides in the body (if any).
    pub fn pins(&self) -> impl Iterator<Item = TraitPin> {
        self.syntax.children().filter_map(TraitPin::cast)
    }
}

/// A pin declaration within a trait.
///
/// `pin MOSI: signal in;`
#[derive(Debug, Clone)]
pub struct TraitPin {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl RowanAstNode for TraitPin {
    type Language = BhdlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        kind == SyntaxKind::TRAIT_PIN
    }

    fn cast(node: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self { syntax: node })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl HasName for TraitPin {}

impl TraitPin {
    /// Get the direction of this pin (in, out, inout).
    pub fn direction_text(&self) -> Option<String> {
        for element in self.syntax.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW |
                    SyntaxKind::INPUT_KW | SyntaxKind::OUTPUT_KW => {
                        return Some(token.text().to_string());
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Get the type of this pin (signal, power, ground, etc.).
    pub fn type_text(&self) -> Option<String> {
        for element in self.syntax.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::SIGNAL_KW | SyntaxKind::POWER_KW | SyntaxKind::GROUND_KW => {
                        return Some(token.text().to_string());
                    }
                    _ => {}
                }
            }
        }
        None
    }
}

/// A const declaration within a trait.
///
/// `const MAX_FREQ: frequency;` (in trait def)
/// `const DROPOUT = 2.0V;` (in trait impl)
#[derive(Debug, Clone)]
pub struct TraitConst {
    syntax: SyntaxNode<BhdlLanguage>,
}

impl RowanAstNode for TraitConst {
    type Language = BhdlLanguage;

    fn can_cast(kind: <Self::Language as rowan::Language>::Kind) -> bool {
        kind == SyntaxKind::TRAIT_CONST
    }

    fn cast(node: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            Some(Self { syntax: node })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl HasName for TraitConst {}

impl TraitConst {
    /// Get the type annotation (e.g., "frequency", "voltage").
    pub fn type_annotation(&self) -> Option<String> {
        let mut found_colon = false;
        for element in self.syntax.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::COLON {
                    found_colon = true;
                } else if found_colon && token.kind() == SyntaxKind::IDENT {
                    return Some(token.text().to_string());
                }
            }
        }
        None
    }

    /// Check if this const has a default/implemented value (has `=`).
    pub fn has_value(&self) -> bool {
        self.syntax
            .children_with_tokens()
            .any(|e| e.as_token().map(|t| t.kind() == SyntaxKind::EQ).unwrap_or(false))
    }
}
