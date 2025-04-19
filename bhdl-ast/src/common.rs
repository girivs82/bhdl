use crate::{Node, Token, BhdlLanguage, blocks::PinMapBlock, HasName};
use bhdl_parser::SyntaxKind;
use rowan::{ast::support, TextRange, SyntaxNode};
// Import the AstNode trait directly for manual implementation
use rowan::ast::AstNode;

// --- Parameter Declaration ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl(Node);

// Add manual impl
impl AstNode for ParamDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        // Allow casting from PARAM_ASSIGN as well, as parser uses it for non-const params
        kind == SyntaxKind::PARAM_DECL || kind == SyntaxKind::PARAM_ASSIGN
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl HasName for ParamDecl {
    fn name(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|tok| tok.kind() == SyntaxKind::IDENT)
    }
}

impl ParamDecl {
    pub fn type_ref(&self) -> Option<TypeRef> {
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn default_value(&self) -> Option<Value> {
        self.0.children_with_tokens()
            .skip_while(|element| element.kind() != SyntaxKind::EQ)
            .skip(1)
            .find_map(|element| element.into_node().and_then(Value::cast))
    }
}

// --- Port Declaration ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDecl(Node);

// Add manual impl
impl AstNode for PortDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PORT_DECL
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl HasName for PortDecl {}

impl PortDecl {
    pub fn direction(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| PortDirection::is_direction_kind(token.kind()))
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        self.0.children().find_map(BusSuffix::cast)
    }
}

// --- Type Reference ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef(Node);

// Add manual impl
impl AstNode for TypeRef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TYPE_REF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl TypeRef {
    pub fn name_token(&self) -> Option<Token> {
        // Find first IDENT or known type keyword
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|tok| {
                matches!(tok.kind(),
                    SyntaxKind::IDENT |
                    SyntaxKind::SIGNAL_KW | // Add keywords
                    SyntaxKind::POWER_KW
                    // TODO: Add more base type keywords if parser uses them here
                )
            })
    }
    // TODO: Add method for parameters like signal(cmos_3v3)
}

// --- Port Direction ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDirection(Token);

impl PortDirection {
    pub fn token(&self) -> &Token {
        &self.0
    }

    pub fn kind(&self) -> SyntaxKind {
        self.0.kind()
    }

    pub fn text_range(&self) -> TextRange {
        self.0.text_range()
    }

    // Add a helper for checking if a token kind is a direction
    pub fn is_direction_kind(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW | SyntaxKind::INPUT_KW | SyntaxKind::OUTPUT_KW)
    }
}

// --- Value (Literal) ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value(Node);

// Add manual impl
impl AstNode for Value {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::VALUE
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl Value {
    // TODO: Add methods to get specific value kinds/tokens (number(), string(), boolean())
}

// --- Identifier Reference ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentRef(Node);

// Add manual impl
impl AstNode for IdentRef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IDENT_REF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl IdentRef {
    pub fn token(&self) -> Option<Token> {
        self.0.first_token()
    }
}

// --- Component Instantiation ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentInst(Node);

// Add manual impl
impl AstNode for ComponentInst {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENT_INST
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl HasName for ComponentInst {
    // Ensure this reliably gets the instance name (second IDENT token)
    fn name(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .filter(|tok| tok.kind() == SyntaxKind::IDENT)
            .nth(1) 
    }
}

impl ComponentInst {
    // Find the component type name (first IDENT token)
    pub fn component_type_name_token(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|tok| tok.kind() == SyntaxKind::IDENT)
    }
    
    // Keep component_type method, but maybe it should return the token?
    // Or try to find a COMPONENT_TYPE node *first* and fall back?
    // For now, let's make it return the first IDENT token like component_type_name_token.
    // This breaks the previous assumption of it returning ComponentType node.
    pub fn component_type(&self) -> Option<Token> { 
        self.component_type_name_token()
    }

    pub fn param_assign_block(&self) -> Option<ParamAssignBlock> {
         self.0.children().find_map(ParamAssignBlock::cast)
    }
}

// --- Component Type (in Instantiation) ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentType(Node);

// Add manual impl
impl AstNode for ComponentType {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENT_TYPE
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ComponentType {
    // This logic might need adjustment if it's called
    pub fn name_ref(&self) -> Option<Node> { 
        self.0.first_child()
    }
}

// --- Parameter Assignment Block --- `{...}` or `(...)` in instantiation

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamAssignBlock(Node);

// Add manual impl
impl AstNode for ParamAssignBlock {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PARAM_ASSIGN_BLOCK
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ParamAssignBlock {
    pub fn assignments(&self) -> impl Iterator<Item = ParamAssign> {
        support::children(&self.0)
    }
}

// --- Parameter Assignment --- `param_name = value`

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamAssign(Node);

// Add manual impl
impl AstNode for ParamAssign {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PARAM_ASSIGN
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ParamAssign {
    pub fn name_token(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }

    pub fn value(&self) -> Option<Value> {
        self.0.children_with_tokens()
            .skip_while(|element| element.kind() != SyntaxKind::EQ)
            .skip(1)
            .find_map(|element| element.into_node().and_then(Value::cast))
    }
}

// --- Net Declaration ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetDecl(Node);

// Add manual impl
impl AstNode for NetDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NET_DECL
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl HasName for NetDecl {}

impl NetDecl {
    pub fn net_keyword(&self) -> Option<Token> {
        self.0.first_token().filter(|t| t.kind() == SyntaxKind::NET_KW)
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        self.0.children().find_map(BusSuffix::cast)
    }
}

// --- Pin Reference --- (e.g., U1.PIN_A[0])

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRef(Node);

// Add manual impl
impl AstNode for PinRef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PIN_REF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl PinRef {
    pub fn instance_name(&self) -> Option<Token> {
        let mut children = self.0.children_with_tokens().peekable();
        let first_ident = children.find(|e| e.kind() == SyntaxKind::IDENT).and_then(|e| e.into_token());
        if children.peek().map_or(false, |e| e.kind() == SyntaxKind::DOT) {
            first_ident
        } else {
            None
        }
    }

    pub fn pin_name(&self) -> Option<Token> {
        let has_dot = self.0.children_with_tokens().any(|e| e.kind() == SyntaxKind::DOT);
        let mut idents = self.0.children_with_tokens()
            .filter(|e| e.kind() == SyntaxKind::IDENT)
            .filter_map(|e| e.into_token());

        if has_dot {
            idents.nth(1)
        } else {
            idents.next()
        }
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        self.0.children().find_map(BusSuffix::cast)
    }
}

// --- Net Reference --- (e.g., NetA[0])

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetRef(Node);

// Add manual impl
impl AstNode for NetRef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NET_REF
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl NetRef {
    pub fn name_token(&self) -> Option<Token> {
        self.0.first_token().filter(|t| t.kind() == SyntaxKind::IDENT)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        self.0.children().find_map(BusSuffix::cast)
    }
}

// --- Connection Statement ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionStmt(Node);

// Add manual impl
impl AstNode for ConnectionStmt {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONNECTION_STMT
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl ConnectionStmt {
    pub fn source(&self) -> Option<Node> {
        self.0.children_with_tokens()
            .take_while(|element| element.kind() != SyntaxKind::ARROW)
            .filter_map(|element| element.into_node())
            .last()
    }

    pub fn sink(&self) -> Option<Node> {
        self.0.children_with_tokens()
            .skip_while(|element| element.kind() != SyntaxKind::ARROW)
            .skip(1)
            .find_map(|element| element.into_node())
    }
}

// --- Pin Declaration --- (within component pins { ... })

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinDecl(Node);

// Add manual impl
impl AstNode for PinDecl {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PIN_DECL
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl HasName for PinDecl {}

impl PinDecl {
    pub fn direction(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| PortDirection::is_direction_kind(token.kind()))
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        self.0.children().find_map(BusSuffix::cast)
    }
}

// --- Bus Suffix --- ([index] or [high:low])

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusSuffix(Node);

// Add manual impl
impl AstNode for BusSuffix {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BUS_SUFFIX
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl BusSuffix {
    /// Returns the `Value` node if this suffix represents an index access (e.g., `[0]`).
    pub fn index(&self) -> Option<Value> {
        let child_kinds: Vec<_> = self.0.children_with_tokens()
            .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
            .map(|t| t.kind())
            .collect();

        if child_kinds == [SyntaxKind::L_BRACKET, SyntaxKind::VALUE, SyntaxKind::R_BRACKET] {
            self.0.children().find_map(Value::cast)
        } else {
            None
        }
    }

    /// Returns a `RangeExpr` wrapper if this suffix represents a range access (e.g., `[7:0]`).
    /// Note: This constructs a temporary `RangeExpr` as the parser doesn't create one inside `BUS_SUFFIX`.
    pub fn range(&self) -> Option<RangeExpr> {
        let child_kinds: Vec<_> = self.0.children_with_tokens()
            .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
            .map(|t| t.kind())
            .collect();

        if child_kinds == [SyntaxKind::L_BRACKET, SyntaxKind::VALUE, SyntaxKind::COLON, SyntaxKind::VALUE, SyntaxKind::R_BRACKET] {
            // Wrap the current SyntaxNode in a RangeExpr for API consistency.
            Some(RangeExpr(self.0.clone()))
        } else {
            None
        }
    }
}

/// Represents a range expression like `7:0`.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct RangeExpr(SyntaxNode<BhdlLanguage>);

impl RangeExpr {
    pub fn lhs(&self) -> Option<Value> {
        self.0.children().filter_map(Value::cast).nth(0)
    }

    pub fn rhs(&self) -> Option<Value> {
        self.0.children().filter_map(Value::cast).nth(1)
    }

    pub fn separator_kind(&self) -> Option<SyntaxKind> {
        self.0.children_with_tokens()
           .filter_map(|it| it.into_token())
           .find(|token| token.kind() == SyntaxKind::COLON)
           .map(|token| token.kind())
    }
}

impl rowan::ast::AstNode for RangeExpr {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        // RangeExpr can be represented by BUS_SUFFIX if it contains the range pattern,
        // or potentially a dedicated RANGE_EXPR node if the parser created one.
        kind == SyntaxKind::BUS_SUFFIX || kind == SyntaxKind::RANGE_EXPR
    }

    fn cast(node: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(node.kind()) {
            // Additional check: Ensure the BUS_SUFFIX actually contains a range pattern
            if node.kind() == SyntaxKind::BUS_SUFFIX {
                 let child_kinds: Vec<_> = node.children_with_tokens()
                    .filter(|t| t.kind() != SyntaxKind::WHITESPACE)
                    .map(|t| t.kind())
                    .collect();
                if child_kinds == [SyntaxKind::L_BRACKET, SyntaxKind::VALUE, SyntaxKind::COLON, SyntaxKind::VALUE, SyntaxKind::R_BRACKET] {
                    Some(Self(node))
                } else {
                    None // BUS_SUFFIX exists but doesn't represent a range here
                }
            } else {
                 Some(Self(node)) // Assume dedicated RANGE_EXPR is valid
            }

        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

// --- Interface Instance --- (within components/modules interfaces { ... })

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceInstance(Node);

// Add manual impl
impl AstNode for InterfaceInstance {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::INTERFACE_INSTANCE
    }
    fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
        if Self::can_cast(node.kind()) { Some(Self(node)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<Self::Language> {
        &self.0
    }
}

impl HasName for InterfaceInstance {}

impl InterfaceInstance {
    pub fn interface_keyword(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::INTERFACE_KW)
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn pin_map_block(&self) -> Option<PinMapBlock> {
        self.0.children().find_map(PinMapBlock::cast)
    }
}

// Add other common nodes like Expression, PinRef, etc. later 