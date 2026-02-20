//! AST nodes for enum types and match expressions.

use crate::{SyntaxKind, BhdlLanguage, HasName};
use rowan::{SyntaxNode, SyntaxToken, ast::AstNode};

// --- Enum Definition ---
// `enum Name { Variant1, Variant2(Payload), ... }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumDef(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for EnumDef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ENUM_DEF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for EnumDef {}
impl EnumDef {
    /// Iterate over the enum variants.
    pub fn variants(&self) -> impl Iterator<Item = EnumVariant> + '_ {
        self.0.children().filter_map(EnumVariant::cast)
    }
}

// --- Enum Variant ---
// `VariantName` or `VariantName(PayloadType1, PayloadType2)`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumVariant(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for EnumVariant {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ENUM_VARIANT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for EnumVariant {}
impl EnumVariant {
    /// Returns the payload type identifiers (if any).
    /// For `Fault(FaultKind)`, returns `["FaultKind"]`.
    /// For `BarrelJack(voltage, current)`, returns `["voltage", "current"]`.
    pub fn payload_types(&self) -> Vec<SyntaxToken<BhdlLanguage>> {
        // Skip the first IDENT (variant name), then collect all remaining IDENTs
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
            .skip(1)
            .collect()
    }

    /// Returns true if this variant has a payload (parenthesized types).
    pub fn has_payload(&self) -> bool {
        self.0.children_with_tokens()
            .any(|e| e.kind() == SyntaxKind::L_PAREN)
    }
}

// --- Match Expression ---
// `match expr { pattern => body, ... }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchExpr(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for MatchExpr {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MATCH_EXPR }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl MatchExpr {
    /// The scrutinee expression being matched on.
    /// This is the first child node that isn't the MATCH_KW or braces.
    pub fn scrutinee(&self) -> Option<SyntaxNode<BhdlLanguage>> {
        self.0.children().next()
    }

    /// Iterate over the match arms.
    pub fn arms(&self) -> impl Iterator<Item = MatchArm> + '_ {
        self.0.children().filter_map(MatchArm::cast)
    }
}

// --- Match Arm ---
// `pattern => body` or `pattern => { statements }`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchArm(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for MatchArm {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MATCH_ARM }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl MatchArm {
    /// The pattern for this arm.
    pub fn pattern(&self) -> Option<MatchPattern> {
        self.0.children().find_map(MatchPattern::cast)
    }

    /// The body expression/block for this arm (all children after the pattern).
    pub fn body_nodes(&self) -> impl Iterator<Item = SyntaxNode<BhdlLanguage>> + '_ {
        let pattern_end = self.pattern()
            .map(|p| p.syntax().text_range().end())
            .unwrap_or_default();
        self.0.children()
            .filter(move |n| n.text_range().start() > pattern_end)
            .filter(|n| !matches!(n.kind(), SyntaxKind::MATCH_PATTERN))
    }
}

// --- Match Pattern ---
// Wildcard `_`, identifier, qualified path `Enum::Variant`, or literal
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MatchPattern(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for MatchPattern {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::MATCH_PATTERN }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl MatchPattern {
    /// Returns the kind of pattern.
    pub fn kind(&self) -> PatternKind {
        let tokens: Vec<_> = self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia())
            .collect();

        if tokens.is_empty() {
            return PatternKind::Error;
        }

        // Check for wildcard _
        if tokens.len() == 1 && tokens[0].kind() == SyntaxKind::IDENT && tokens[0].text() == "_" {
            return PatternKind::Wildcard;
        }

        // Check for literal
        if tokens.len() == 1 && matches!(tokens[0].kind(),
            SyntaxKind::NUMBER | SyntaxKind::STRING | SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW)
        {
            return PatternKind::Literal;
        }

        // Check for path pattern (contains ::)
        let has_path_sep = tokens.windows(2).any(|w| {
            w[0].kind() == SyntaxKind::COLON && w[1].kind() == SyntaxKind::COLON
        });
        if has_path_sep {
            let has_paren = tokens.iter().any(|t| t.kind() == SyntaxKind::L_PAREN);
            if has_paren {
                return PatternKind::Destructure;
            }
            return PatternKind::Path;
        }

        // Simple identifier pattern (binding)
        if tokens.len() == 1 && tokens[0].kind() == SyntaxKind::IDENT {
            return PatternKind::Variable;
        }

        // Identifier with destructuring
        let has_paren = tokens.iter().any(|t| t.kind() == SyntaxKind::L_PAREN);
        if has_paren {
            return PatternKind::Destructure;
        }

        PatternKind::Variable
    }

    /// Get all IDENT tokens in the pattern (the path components).
    pub fn ident_tokens(&self) -> Vec<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
            .collect()
    }

    /// Get the full path text (e.g., "PowerState::Off" or just "kind").
    pub fn path_text(&self) -> String {
        let tokens: Vec<_> = self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| !t.kind().is_trivia() && t.kind() != SyntaxKind::L_PAREN && t.kind() != SyntaxKind::R_PAREN && t.kind() != SyntaxKind::COMMA)
            .collect();
        tokens.iter().map(|t| t.text().to_string()).collect::<Vec<_>>().join("")
    }

    /// Get binding names for destructure patterns.
    /// For `Fault(kind)` returns `["kind"]`.
    pub fn bindings(&self) -> Vec<SyntaxToken<BhdlLanguage>> {
        let mut in_parens = false;
        let mut bindings = Vec::new();
        for elem in self.0.children_with_tokens() {
            if let Some(token) = elem.as_token() {
                match token.kind() {
                    SyntaxKind::L_PAREN => in_parens = true,
                    SyntaxKind::R_PAREN => in_parens = false,
                    SyntaxKind::IDENT if in_parens => bindings.push(token.clone()),
                    _ => {}
                }
            }
        }
        bindings
    }
}

/// Classification of match pattern kinds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternKind {
    /// `_` — matches anything.
    Wildcard,
    /// A binding variable (e.g., `x`).
    Variable,
    /// A literal value (number, string, bool).
    Literal,
    /// A qualified enum path (e.g., `PowerState::Off`).
    Path,
    /// A destructuring pattern (e.g., `PowerState::Fault(kind)`).
    Destructure,
    /// Parse error.
    Error,
}

// Helper trait for trivia checking
trait SyntaxKindExt {
    fn is_trivia(&self) -> bool;
}
impl SyntaxKindExt for SyntaxKind {
    fn is_trivia(&self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }
}
