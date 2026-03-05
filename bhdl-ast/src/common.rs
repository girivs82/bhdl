use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName}; // Use SyntaxNode/Token
use crate::expr::Expr;
use rowan::ast::AstNode;
// v2.0 doesn't have PinMapBlock - it was used for v1.0 pin mapping
use rowan::NodeOrToken; // Needed for SyntaxNodeExt
// Removed import for TypeDef as it's not used here

// Add helper trait/method for skipping trivia
trait SyntaxNodeExt {
    fn first_non_trivia_token(&self) -> Option<SyntaxToken<BhdlLanguage>>;
}

impl SyntaxNodeExt for SyntaxNode<BhdlLanguage> {
    fn first_non_trivia_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.children_with_tokens()
            .find_map(|element| match element {
                NodeOrToken::Node(_) => None, // Skip child nodes
                NodeOrToken::Token(token) => {
                    // Check for trivia kinds directly
                    if !matches!(token.kind(), SyntaxKind::WHITESPACE | SyntaxKind::COMMENT) {
                        Some(token)
                    } else {
                        None
                    }
                }
            })
    }
}

// --- Common AST Node Structures ---

// --- Identifier --- (Simple identifier wrapper for names)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for Ident {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::IDENT }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

// --- Parameter Declaration --- (within parameter block, no keyword)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamDecl(pub(crate) SyntaxNode<BhdlLanguage>); // This struct might be redundant if only ParamAssign exists
impl AstNode for ParamDecl {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PARAM_DECL }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for ParamDecl {}
impl ParamDecl {
    pub fn type_ref(&self) -> Option<TypeRef> { self.0.children().find_map(TypeRef::cast) }
    pub fn default_value(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
}

// --- Type Reference ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeRef(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for TypeRef {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TYPE_REF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl TypeRef {
    pub fn name_token(&self) -> Option<SyntaxToken<BhdlLanguage>> { 
        self.0.first_non_trivia_token()
    }
    // Add methods for type parameters later
}

// --- Bus Suffix --- `[index]` or `[high:low]`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BusSuffix(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for BusSuffix {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::BUS_SUFFIX }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl BusSuffix {
    pub fn range(&self) -> Option<RangeExpr> { self.0.children().find_map(RangeExpr::cast) }
    pub fn index_expr(&self) -> Option<Expr> { 
        // Find Expr child that isn't part of a RangeExpr child
        self.0.children()
            .find(|node| node.parent().map(|p| p.kind() != SyntaxKind::RANGE_EXPR).unwrap_or(true))
            .and_then(Expr::cast)
    }
    // Helper to get index node directly for simple cases
    pub fn index_expr_node(&self) -> Option<SyntaxNode<BhdlLanguage>> {
        self.0.children()
            .find(|node| node.parent().map(|p| p.kind() != SyntaxKind::RANGE_EXPR).unwrap_or(true) && Expr::can_cast(node.kind()))
    }
}

// --- Range Expression --- `lhs:rhs`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RangeExpr(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for RangeExpr {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::RANGE_EXPR }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl RangeExpr {
    pub fn lhs(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
    pub fn rhs(&self) -> Option<Expr> { self.0.children().filter_map(Expr::cast).nth(1) }
    pub fn separator_kind(&self) -> Option<SyntaxKind> {
        self.0.children_with_tokens()
            .find(|t| t.kind() == SyntaxKind::COLON)
            .map(|t| t.kind())
    }
    // Helpers to get nodes directly
    pub fn lhs_node(&self) -> Option<SyntaxNode<BhdlLanguage>> { self.0.children().find(|n| Expr::can_cast(n.kind())) }
    pub fn rhs_node(&self) -> Option<SyntaxNode<BhdlLanguage>> { self.0.children().filter(|n| Expr::can_cast(n.kind())).nth(1) }
}

// --- Value --- (e.g., number, string literal)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Value(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for Value {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::VALUE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl Value {
    pub fn int_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::NUMBER && !token.text().contains('.'))
    }
    
    pub fn float_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::NUMBER && token.text().contains('.'))
    }
    
    pub fn number_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::NUMBER)
    }
}

// --- Parameter Assignment --- `param_name = value`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamAssign(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for ParamAssign {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PARAM_ASSIGN }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for ParamAssign {}
impl ParamAssign {
    pub fn value(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
    pub fn value_literal_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.value().and_then(|expr| match expr {
            Expr::Value(val) => val.syntax().first_non_trivia_token(),
            _ => None, // Or handle other Expr variants if needed
        })
    }
}

// --- Port Declaration ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortDecl(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for PortDecl {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PORT_DECL }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for PortDecl {}
impl PortDecl {
    pub fn type_ref(&self) -> Option<TypeRef> { self.0.children().find_map(TypeRef::cast) }
    pub fn bus_suffix(&self) -> Option<BusSuffix> { self.0.children().find_map(BusSuffix::cast) }
}

// --- Pin Declaration --- (within entities)
// v2.0 uses pin declarations in entities for physical component pins
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PinDecl(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for PinDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PIN_DECL }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for PinDecl {
    // Override name() to handle both IDENT and NUMBER tokens for pin names
    fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| matches!(token.kind(), SyntaxKind::IDENT | SyntaxKind::NUMBER))
    }
}
impl PinDecl {
    pub fn type_ref(&self) -> Option<TypeRef> { self.0.children().find_map(TypeRef::cast) }
    pub fn bus_suffix(&self) -> Option<BusSuffix> { self.0.children().find_map(BusSuffix::cast) }
    pub fn metadata(&self) -> Option<PinMetadata> { self.0.children().find_map(PinMetadata::cast) }
    
    /// Check if this pin is declared as virtual
    pub fn is_virtual(&self) -> bool {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .any(|token| token.kind() == SyntaxKind::VIRTUAL_KW)
    }
    
    /// Get the pin direction (in, out, inout)
    pub fn direction(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| matches!(token.kind(), SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW))
    }
    
    /// Get the pin type (signal, power, ground, switch, feedback)
    pub fn pin_type(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| matches!(token.kind(),
                SyntaxKind::SIGNAL_KW | SyntaxKind::POWER_KW | SyntaxKind::GROUND_KW |
                SyntaxKind::SWITCH_KW | SyntaxKind::FEEDBACK_KW))
    }

    /// Get the 'when' condition expression (for conditional pins).
    /// e.g., `pin EN: signal in when HAS_EN;` returns the expression for `HAS_EN`.
    pub fn when_clause(&self) -> Option<Expr> {
        let mut found_when = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::WHEN_KW {
                    found_when = true;
                }
            }
            if found_when {
                if let Some(node) = element.as_node() {
                    if let Some(expr) = Expr::cast(node.clone()) {
                        return Some(expr);
                    }
                }
            }
        }
        None
    }

    /// Get the raw text of the 'when' condition (fallback for simple identifier conditions).
    /// Returns the text after WHEN_KW up to the semicolon.
    pub fn when_condition_text(&self) -> Option<String> {
        // First try the structured Expr extraction
        if let Some(expr) = self.when_clause() {
            return Some(expr.syntax().text().to_string());
        }
        // Fallback: look for IDENT token after WHEN_KW
        let mut found_when = false;
        for element in self.0.children_with_tokens() {
            if let Some(token) = element.as_token() {
                if token.kind() == SyntaxKind::WHEN_KW {
                    found_when = true;
                    continue;
                }
                if found_when && token.kind() == SyntaxKind::IDENT {
                    return Some(token.text().to_string());
                }
                if found_when && token.kind() == SyntaxKind::SEMI {
                    break;
                }
            }
        }
        None
    }
}

// --- Net Declaration ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetDecl(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for NetDecl {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::NET_DECL }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for NetDecl {}
impl NetDecl {
    pub fn type_ref(&self) -> Option<TypeRef> { self.0.children().find_map(TypeRef::cast) }
    pub fn bus_suffix(&self) -> Option<BusSuffix> { self.0.children().find_map(BusSuffix::cast) }
    pub fn default_value(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
}

// --- Component Instantiation ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentInst(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for ComponentInst {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::COMPONENT_INST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl HasName for ComponentInst {}
impl ComponentInst {
    pub fn component_type_name(&self) -> Option<SyntaxToken<BhdlLanguage>> { // Changed return type
        // In flow syntax (e.g., Res(330).1), the component type is the first IDENT
        // In assignment syntax (e.g., R1: Res(330)), it's the IDENT after the colon
        
        // First, try to find an IDENT after a colon (assignment syntax)
        let mut after_colon = false;
        let colon_ident = self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| {
                if t.kind() == SyntaxKind::COLON {
                    after_colon = true;
                    false
                } else if after_colon && t.kind() == SyntaxKind::IDENT {
                    true
                } else {
                    false
                }
            });
            
        // If found, return it
        if colon_ident.is_some() {
            return colon_ident;
        }
        
        // Otherwise, return the first IDENT token (flow syntax)
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
    // Reimplement name() using HasName trait default
    // pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> { ... }
    pub fn param_assign_block(&self) -> Option<ParamAssignBlock> {
        self.0.children().find_map(ParamAssignBlock::cast)
    }
    
    // Get parameters from PARAM_LIST (used for interface instances)
    pub fn param_list(&self) -> Option<crate::items::ParamList> {
        self.0.children().find_map(crate::items::ParamList::cast)
    }
}

// --- Parameter Assignment Block --- `(...)` or `{...}` in ComponentInst
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamAssignBlock(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for ParamAssignBlock {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PARAM_ASSIGN_BLOCK }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl ParamAssignBlock {
    pub fn assignments(&self) -> impl Iterator<Item = ParamAssign> { 
        self.0.children().filter_map(ParamAssign::cast)
    }
    
    /// Check if this parameter block contains a placeholder for SPICE generation
    pub fn has_placeholder(&self) -> bool {
        self.0.children().any(|child| child.kind() == SyntaxKind::PARAM_PLACEHOLDER)
    }
    
    /// Get the placeholder node if present
    pub fn placeholder(&self) -> Option<ParamPlaceholder> {
        self.0.children().find_map(ParamPlaceholder::cast)
    }
}

// --- Parameter Placeholder --- Empty () or (?) for SPICE generation
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParamPlaceholder(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for ParamPlaceholder {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PARAM_PLACEHOLDER }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl ParamPlaceholder {
    /// Check if this is an explicit placeholder (has ?)
    pub fn is_explicit(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .any(|t| t.kind() == SyntaxKind::QUESTION)
    }
    
    /// Get constraint assignments if present (for ?, constraint=value syntax)
    pub fn constraints(&self) -> impl Iterator<Item = ParamAssign> {
        self.0.children().filter_map(ParamAssign::cast)
    }
}

// --- Pin Metadata --- @metadata(key=value, ...)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PinMetadata(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for PinMetadata {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PIN_METADATA }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl PinMetadata {
    /// Get all metadata pairs
    pub fn pairs(&self) -> impl Iterator<Item = MetadataPair> {
        self.0.children().filter_map(MetadataPair::cast)
    }
    
    /// Get a specific metadata value by key
    pub fn get(&self, key: &str) -> Option<String> {
        self.pairs()
            .find(|pair| pair.key().map(|k| k.text() == key).unwrap_or(false))
            .and_then(|pair| pair.value())
    }
}

// --- Metadata Pair --- key=value
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MetadataPair(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for MetadataPair {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::METADATA_PAIR }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl MetadataPair {
    /// Get the key identifier
    pub fn key(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the value as a string
    pub fn value(&self) -> Option<String> {
        // First check for a STRING token
        if let Some(string_token) = self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::STRING)
        {
            // Remove quotes from string
            let text = string_token.text();
            return Some(text.trim_matches('"').to_string());
        }
        
        // Otherwise get the expression text after the '='
        self.0.children()
            .find_map(Expr::cast)
            .map(|expr| expr.syntax().text().to_string())
    }
}

// ConnectionStmt has been moved to v2_statements.rs for v2.0 support

// --- Pin Reference --- `instance.pin` or `pin`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PinRef(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for PinRef {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PIN_REF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl PinRef {
    pub fn instance_name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT && t.next_token().map(|nt| nt.kind() == SyntaxKind::DOT).unwrap_or(false))
    }
    pub fn pin_name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|t| t.kind() == SyntaxKind::IDENT)
            .last()
    }
    pub fn bus_suffix(&self) -> Option<BusSuffix> { self.0.children().find_map(BusSuffix::cast) }
}

// --- Net Reference --- `net_name`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct NetRef(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for NetRef {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::NET_REF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl NetRef {
    pub fn name_token(&self) -> Option<SyntaxToken<BhdlLanguage>> { 
        self.0.children_with_tokens()
            .filter_map(|el| el.into_token())
            .find(|tok| tok.kind() == SyntaxKind::IDENT)
    }
    pub fn bus_suffix(&self) -> Option<BusSuffix> { self.0.children().find_map(BusSuffix::cast) }
    
    /// Check if this net reference has the @ prefix (always true for valid NetRef)
    pub fn has_at_prefix(&self) -> bool {
        self.0.children_with_tokens()
            .filter_map(|el| el.into_token())
            .any(|tok| tok.kind() == SyntaxKind::AT)
    }
    
    /// Get the net name without the @ prefix
    pub fn name(&self) -> Option<String> {
        self.name_token().map(|t| t.text().to_string())
    }
}

// --- Identifier Reference --- (Used in expressions, etc.)
#[derive(Debug, Clone, PartialEq, Eq, Hash)] 
pub struct IdentRef(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for IdentRef {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::IDENT_REF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl IdentRef {
    pub fn token(&self) -> Option<SyntaxToken<BhdlLanguage>> { 
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
}

// --- Simple Identifier Reference --- (Used where only a simple name is allowed, e.g., LHS of assign?)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SimpleIdentRef(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for SimpleIdentRef {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SIMPLE_IDENT_REF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl SimpleIdentRef {
    pub fn name_token(&self) -> Option<SyntaxToken<BhdlLanguage>> { self.0.first_non_trivia_token() }
}

// v2.0 doesn't have INTERFACE_INSTANCE - interfaces are instantiated like components

// --- Component Type --- (Used in ComponentInst)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentType(pub(crate) SyntaxNode<BhdlLanguage>); // Use SyntaxNode
impl AstNode for ComponentType {
    type Language = BhdlLanguage; // Added Language
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::COMPONENT_TYPE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}
impl ComponentType {
    pub fn name_token(&self) -> Option<SyntaxToken<BhdlLanguage>> { self.0.first_token() }
}

// --- Port Direction --- (Keywords: in, out, inout)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortDirection(pub(crate) SyntaxToken<BhdlLanguage>); // Use SyntaxToken
impl PortDirection {
    pub fn is_direction_kind(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW)
    }
    pub fn cast(token: SyntaxToken<BhdlLanguage>) -> Option<Self> {
        if Self::is_direction_kind(token.kind()) { Some(Self(token)) } else { None }
    }
    pub fn syntax(&self) -> &SyntaxToken<BhdlLanguage> { &self.0 }
    pub fn kind(&self) -> SyntaxKind { self.0.kind() }
}