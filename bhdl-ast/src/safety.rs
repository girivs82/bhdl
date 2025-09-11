//! Safety-related AST nodes for ISO 26262 compliance

use crate::{AstNode, BhdlLanguage, SyntaxKind, SyntaxNode, SyntaxToken};
use rowan::ast::support;

/// Represents a satisfies block containing safety compliance declarations
/// 
/// Example:
/// ```bhdl
/// satisfies {
///     REQ_001: via component_name;
///     REQ_002: { implementation: "description"; }
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SatisfiesBlock {
    pub(crate) syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for SatisfiesBlock {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SATISFIES_BLOCK
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl SatisfiesBlock {
    /// Returns all satisfies items in this block
    pub fn items(&self) -> impl Iterator<Item = SatisfiesItem> {
        support::children(&self.syntax)
    }

    /// Returns the 'satisfies' keyword token
    pub fn satisfies_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        support::token(&self.syntax, SyntaxKind::SATISFIES_KW)
    }
}

/// Represents a single requirement satisfaction declaration
/// 
/// Example:
/// ```bhdl
/// REQ_001: via component_name;
/// REQ_002: { implementation: "description"; }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SatisfiesItem {
    pub(crate) syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for SatisfiesItem {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SATISFIES_ITEM
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl SatisfiesItem {
    /// Returns the requirement ID (the identifier before the colon)
    pub fn requirement_id(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }

    /// Returns the satisfaction specification (either via clause or details)
    pub fn satisfaction(&self) -> Option<SatisfiesSpec> {
        // Try to find a via clause first
        if let Some(via) = support::child::<SatisfiesVia>(&self.syntax) {
            return Some(SatisfiesSpec::Via(via));
        }
        
        // Otherwise look for details
        if let Some(details) = support::child::<SatisfiesDetails>(&self.syntax) {
            return Some(SatisfiesSpec::Details(details));
        }
        
        None
    }
}

/// Represents the specification of how a requirement is satisfied
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SatisfiesSpec {
    Via(SatisfiesVia),
    Details(SatisfiesDetails),
}

/// Represents a 'via component' satisfaction clause
/// 
/// Example:
/// ```bhdl
/// via component_name
/// via module.component
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SatisfiesVia {
    pub(crate) syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for SatisfiesVia {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SATISFIES_VIA
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl SatisfiesVia {
    /// Returns the 'via' keyword token
    pub fn via_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        support::token(&self.syntax, SyntaxKind::VIA_KW)
    }

    /// Returns the component reference path
    pub fn component_path(&self) -> Vec<SyntaxToken<BhdlLanguage>> {
        self.syntax
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::IDENT)
            .collect()
    }

    /// Returns the component reference as a dotted string
    pub fn component_path_string(&self) -> String {
        self.component_path()
            .iter()
            .map(|t| t.text().to_string())
            .collect::<Vec<_>>()
            .join(".")
    }
    
    /// Returns all component references (handles comma-separated list)
    pub fn component_paths(&self) -> Vec<String> {
        let mut components = Vec::new();
        let mut current_path = Vec::new();
        
        for element in self.syntax.children_with_tokens() {
            match element {
                rowan::NodeOrToken::Token(token) => {
                    match token.kind() {
                        SyntaxKind::IDENT => {
                            current_path.push(token.text().to_string());
                        }
                        SyntaxKind::DOT => {
                            // Continue building dotted path
                        }
                        SyntaxKind::COMMA => {
                            // End of current component, start new one
                            if !current_path.is_empty() {
                                components.push(current_path.join("."));
                                current_path.clear();
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        
        // Add the last component if any
        if !current_path.is_empty() {
            components.push(current_path.join("."));
        }
        
        components
    }
}

/// Represents detailed satisfaction specification
/// 
/// Example:
/// ```bhdl
/// {
///     implementation: "description";
///     evidence: "test report";
///     coverage: 99%;
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SatisfiesDetails {
    pub(crate) syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for SatisfiesDetails {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::SATISFIES_DETAILS
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl SatisfiesDetails {
    /// Returns all field-value pairs in the details
    pub fn fields(&self) -> Vec<(String, String)> {
        let mut fields = Vec::new();
        let mut tokens = self.syntax.children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| !matches!(token.kind(), 
                SyntaxKind::L_BRACE | SyntaxKind::R_BRACE | 
                SyntaxKind::COMMA | SyntaxKind::SEMI |
                SyntaxKind::WHITESPACE | SyntaxKind::COMMENT
            ));
        
        while let Some(field_name) = tokens.next() {
            if field_name.kind() == SyntaxKind::IDENT {
                // Skip the colon
                if let Some(colon) = tokens.next() {
                    if colon.kind() == SyntaxKind::COLON {
                        // Get the value (might be multiple tokens)
                        let mut value_parts = Vec::new();
                        while let Some(token) = tokens.next() {
                            if token.kind() == SyntaxKind::IDENT || 
                               token.kind() == SyntaxKind::STRING ||
                               token.kind() == SyntaxKind::NUMBER {
                                value_parts.push(token.text().to_string());
                                // Check if next is not a continuation
                                if let Some(next) = tokens.clone().next() {
                                    if next.kind() == SyntaxKind::IDENT {
                                        // This is the next field, break
                                        break;
                                    }
                                }
                            } else {
                                break;
                            }
                        }
                        let value = value_parts.join(" ");
                        fields.push((field_name.text().to_string(), value));
                    }
                }
            }
        }
        
        fields
    }
}

// Helper trait to check if a node has a satisfies block
pub trait HasSatisfies: AstNode<Language = BhdlLanguage> {
    /// Returns the satisfies block if present
    fn satisfies_block(&self) -> Option<SatisfiesBlock> {
        support::child(self.syntax())
    }
    
    /// Checks if this node satisfies a specific requirement
    fn satisfies_requirement(&self, req_id: &str) -> bool {
        self.satisfies_block()
            .map(|block| {
                block.items().any(|item| {
                    item.requirement_id()
                        .map(|id| id.text() == req_id)
                        .unwrap_or(false)
                })
            })
            .unwrap_or(false)
    }
    
    /// Returns all satisfied requirement IDs
    fn satisfied_requirements(&self) -> Vec<String> {
        self.satisfies_block()
            .map(|block| {
                block.items()
                    .filter_map(|item| {
                        item.requirement_id()
                            .map(|id| id.text().to_string())
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

// Implement HasSatisfies for Board and Module (they can contain satisfies blocks)
impl HasSatisfies for crate::items::Board {}
impl HasSatisfies for crate::items::Module {}