use crate::{AstNode, HasName, Node, Token, blocks::PinMapBlock, BhdlLanguage};
use bhdl_parser::SyntaxKind;
use rowan::{ast::support, TextRange, SyntaxNode};

// Macro to implement rowan::ast::AstNode for common structs
macro_rules! impl_rowan_ast_node {
    ($struct_name:ident) => {
        impl rowan::ast::AstNode for $struct_name {
            type Language = BhdlLanguage;
            fn can_cast(kind: SyntaxKind) -> bool {
                <$struct_name as crate::AstNode>::can_cast(kind)
            }
            fn cast(node: SyntaxNode<Self::Language>) -> Option<Self> {
                <$struct_name as crate::AstNode>::cast(node)
            }
            fn syntax(&self) -> &SyntaxNode<Self::Language> {
                <$struct_name as crate::AstNode>::syntax(self)
            }
        }
    };
}

// --- Parameter Declaration ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamDecl(Node);

impl AstNode for ParamDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        // Let's assume the parser might produce PARAM_ASSIGN directly inside the block
        // if `const` keyword isn't used, which seems likely based on spec examples.
        // We might need a wrapper enum later (Decl::Param, Decl::ConstParam)
        kind == SyntaxKind::PARAM_DECL || kind == SyntaxKind::PARAM_ASSIGN
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(ParamDecl);
impl HasName for ParamDecl {
    // If it's a PARAM_ASSIGN, name is the first IDENT
    // If it's PARAM_DECL, name follows `const`?
    // Need to check parser details. For now, assume first IDENT generally works.
    fn name(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|tok| tok.kind() == SyntaxKind::IDENT)
    }
}

impl ParamDecl {
    pub fn type_ref(&self) -> Option<TypeRef> {
        // Iterate children to find the TypeRef node
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn default_value(&self) -> Option<Value> { // Can also be an Expression
        // Iterate children AFTER the EQ token
        self.0.children_with_tokens()
            .skip_while(|element| element.kind() != SyntaxKind::EQ)
            .skip(1) // Skip the EQ token itself
            .find_map(|element| element.into_node().and_then(Value::cast))
         // TODO: Handle Expression here too
    }
}

// --- Port Declaration ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDecl(Node);

impl AstNode for PortDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PORT_DECL
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(PortDecl);
impl HasName for PortDecl {} // Uses default impl

impl PortDecl {
    pub fn direction(&self) -> Option<PortDirection> {
        // Iterate tokens, find direction keyword, wrap it
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| PortDirection::can_cast(token.kind()))
            .map(PortDirection)
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        // Iterate children to find the TypeRef node
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        // Iterate children to find the BusSuffix node
        self.0.children().find_map(BusSuffix::cast)
    }
    // TODO: Add methods for properties
}

// --- Type Reference ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeRef(Node);

impl AstNode for TypeRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::TYPE_REF
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(TypeRef);
impl TypeRef {
    pub fn name_token(&self) -> Option<Token> {
        // Iterate children/tokens to find the first IDENT
        self.0
            .children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|tok| tok.kind() == SyntaxKind::IDENT)
    }

    // TODO: Add method for parameters like signal(cmos_3v3)
}

// --- Port Direction ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortDirection(Token);

impl AstNode for PortDirection {
    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, SyntaxKind::IN_KW | SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW | SyntaxKind::INPUT_KW | SyntaxKind::OUTPUT_KW) // Include VHDL style for now
    }

    fn cast(node: Node) -> Option<Self> {
        // Direction is expected to be a single token
        node.first_token().and_then(|token| {
            if Self::can_cast(token.kind()) {
                Some(Self(token))
            } else {
                None
            }
        })
    }

    fn syntax(&self) -> &Node {
        // This is awkward as it's just a token. Consider changing trait?
        // For now, panic or return a dummy node? Let's panic.
        unimplemented!("PortDirection does not have an underlying SyntaxNode, only a Token")
    }
}

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
}

// --- Value (Literal) ---
// Covers Number, String, Boolean for now
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Value(Node);

impl AstNode for Value {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::VALUE // Assuming parser creates a VALUE node wrapper
        // Or maybe check specific literal kinds? TBD based on parser output
        // matches!(kind, SyntaxKind::NUMBER | SyntaxKind::STRING | SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW)
    }

    fn cast(node: Node) -> Option<Self> {
        // Check if the node itself is VALUE, or if its first token is a literal kind
        if node.kind() == SyntaxKind::VALUE {
             Some(Self(node))
        } else if node.children_with_tokens().count() == 1 {
            node.first_token().and_then(|token|{
                 match token.kind() {
                    SyntaxKind::NUMBER | SyntaxKind::STRING | SyntaxKind::TRUE_KW | SyntaxKind::FALSE_KW => Some(Self(node.clone())), // Clone needed if we consume node
                    _ => None
                 }
            })
        } else {
            None
        }
    }

    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(Value);
impl Value {
    // TODO: Add methods to get specific value kinds/tokens (number(), string(), boolean())
}

// --- Identifier Reference ---
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdentRef(Node);

impl AstNode for IdentRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::IDENT_REF
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(IdentRef);
impl IdentRef {
    pub fn token(&self) -> Option<Token> {
        self.0.first_token()
    }
}

// --- Component Instantiation ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentInst(Node);

impl AstNode for ComponentInst {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENT_INST
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(ComponentInst);
impl HasName for ComponentInst {} // Instance name

impl ComponentInst {
    pub fn component_type(&self) -> Option<ComponentType> {
        support::child(&self.0)
    }

    pub fn param_assign_block(&self) -> Option<ParamAssignBlock> {
        support::child(&self.0)
    }
}

// --- Component Type (in Instantiation) ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentType(Node);

impl AstNode for ComponentType {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENT_TYPE
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(ComponentType);
impl ComponentType {
    // The type name is likely an IdentRef or PathRef
    pub fn name_ref(&self) -> Option<Node> { // Returning Node, could try casting to IdentRef/PathRef
        self.0.first_child()
    }
}

// --- Parameter Assignment Block --- `{...}` or `(...)` in instantiation

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamAssignBlock(Node);

impl AstNode for ParamAssignBlock {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PARAM_ASSIGN_BLOCK
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(ParamAssignBlock);
impl ParamAssignBlock {
    pub fn assignments(&self) -> impl Iterator<Item = ParamAssign> {
        support::children(&self.0)
    }
}

// --- Parameter Assignment --- `param_name = value`

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParamAssign(Node);

impl AstNode for ParamAssign {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PARAM_ASSIGN
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(ParamAssign);
impl ParamAssign {
    pub fn name_token(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }

    pub fn value(&self) -> Option<Value> { // Assuming value is wrapped in a VALUE node
        // Iterate children after the EQ token
         self.0.children_with_tokens()
            .skip_while(|element| element.kind() != SyntaxKind::EQ)
            .skip(1) // Skip the EQ token itself
            .find_map(|element| element.into_node().and_then(Value::cast))
        // TODO: Handle expressions here as well
    }
}

// --- Net Declaration ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetDecl(Node);

impl AstNode for NetDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::NET_DECL
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(NetDecl);
impl HasName for NetDecl {} // Net name

impl NetDecl {
    pub fn net_keyword(&self) -> Option<Token> {
        self.0.first_token().filter(|t| t.kind() == SyntaxKind::NET_KW)
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        // Iterate children to find the TypeRef node
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        // Iterate children to find the BusSuffix node
        self.0.children().find_map(BusSuffix::cast)
    }
    // TODO: Add method for bus suffix [range]
}

// --- Pin Reference --- (e.g., U1.PIN_A[0])

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinRef(Node);

impl AstNode for PinRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        // Only cast if it's explicitly PIN_REF *or* an IDENT_REF containing a DOT (parser workaround)
        kind == SyntaxKind::PIN_REF
        // || (kind == SyntaxKind::IDENT_REF && node.children_with_tokens().any(|t| t.kind() == SyntaxKind::DOT)) // Need node here, hard to do in can_cast
    }
    fn cast(node: Node) -> Option<Self> {
        // Cast if kind is PIN_REF, OR if it's IDENT_REF *and* contains a DOT
        if node.kind() == SyntaxKind::PIN_REF {
            Some(Self(node))
        } else if node.kind() == SyntaxKind::IDENT_REF && node.children_with_tokens().any(|t| t.kind() == SyntaxKind::DOT) {
             // This case might not be needed if parser always makes PIN_REF correctly when DOT exists
             // Let's rely on PIN_REF kind for now.
             // Some(Self(node))
             None // Assume parser makes PIN_REF if DOT exists
        } else if node.kind() == SyntaxKind::PIN_REF { // Added check just for PIN_REF node type
            Some(Self(node))
        } else {
            None
        }
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(PinRef);
impl PinRef {
    // A PinRef is typically Instance.PinName or just PinName (if within component def?)
    // Or potentially NetName (if representing a port connection target? TBC)

    pub fn instance_name(&self) -> Option<Token> {
        // Find first IDENT *if* a DOT follows
        let mut children = self.0.children_with_tokens().peekable();
        let first_ident = children.find(|e| e.kind() == SyntaxKind::IDENT).and_then(|e| e.into_token());
        if children.peek().map_or(false, |e| e.kind() == SyntaxKind::DOT) {
            first_ident
        } else {
            None
        }
    }

    pub fn pin_name(&self) -> Option<Token> {
        // If DOT exists, find second IDENT. Otherwise, find first IDENT.
        let has_dot = self.0.children_with_tokens().any(|e| e.kind() == SyntaxKind::DOT);
        let mut idents = self.0.children_with_tokens()
            .filter(|e| e.kind() == SyntaxKind::IDENT)
            .filter_map(|e| e.into_token());

        if has_dot {
            idents.nth(1) // Second identifier
        } else {
            idents.next() // First identifier
        }
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        // Iterate children to find the BusSuffix node
        self.0.children().find_map(BusSuffix::cast)
    }
    // TODO: Add method for bus suffix [slice]
}

// --- Net Reference --- (e.g., NetA[0])

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetRef(Node);

impl AstNode for NetRef {
    fn can_cast(kind: SyntaxKind) -> bool {
        // Allow casting from PIN_REF (parser workaround) or explicit NET_REF
        kind == SyntaxKind::NET_REF || kind == SyntaxKind::PIN_REF || kind == SyntaxKind::IDENT_REF
    }
     fn cast(node: Node) -> Option<Self> {
        // If it's NET_REF, cast.
        // If it's PIN_REF, only cast if it DOES NOT contain a DOT (parser workaround).
        // If it's IDENT_REF, cast only if parent is not PIN_REF (previous logic).
        match node.kind() {
            SyntaxKind::NET_REF => Some(Self(node)),
            SyntaxKind::PIN_REF => {
                if node.children_with_tokens().any(|t| t.kind() == SyntaxKind::DOT) {
                    None // It's a real PinRef (Instance.Pin)
                } else {
                    Some(Self(node)) // It's a NetRef parsed as PinRef
                }
            },
            SyntaxKind::IDENT_REF => {
                 if node.parent().map_or(true, |p| p.kind() != SyntaxKind::PIN_REF) {
                    Some(Self(node))
                } else {
                    None
                }
            }
            _ => None,
        }
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(NetRef);
impl NetRef {
    // NetRef is just an identifier (potentially with a bus suffix)
    pub fn name_token(&self) -> Option<Token> {
        self.0.first_token().filter(|t| t.kind() == SyntaxKind::IDENT)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        // Iterate children to find the BusSuffix node
        self.0.children().find_map(BusSuffix::cast)
    }
    // TODO: Add method for bus suffix [slice]
}

// --- Connection Statement ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionStmt(Node);

impl AstNode for ConnectionStmt {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONNECTION_STMT
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(ConnectionStmt);
impl ConnectionStmt {
    // A connection is Left -> Right, where Left and Right can be PinRef or NetRef
    // It might also contain multiple comma-separated refs on either side.

    // Find node before ARROW token
    pub fn source(&self) -> Option<Node> { // Could be PinRef or NetRef
        self.0.children_with_tokens()
            .take_while(|element| element.kind() != SyntaxKind::ARROW)
            .filter_map(|element| element.into_node())
            .last() // Get the last node before the arrow
    }

    // Find node after ARROW token
    pub fn sink(&self) -> Option<Node> { // Could be PinRef or NetRef
        self.0.children_with_tokens()
            .skip_while(|element| element.kind() != SyntaxKind::ARROW)
            .skip(1) // Skip the arrow itself
            .find_map(|element| element.into_node())
    }

    // TODO: Add methods to handle comma-separated lists of sources/sinks
    // TODO: Distinguish between PinRef and NetRef for source/sink
    // TODO: Handle <=> interface connections
}

// --- Pin Declaration --- (within component pins { ... })

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PinDecl(Node);

impl AstNode for PinDecl {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::PIN_DECL
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(PinDecl);
impl HasName for PinDecl {} // Pin name

impl PinDecl {
    pub fn direction(&self) -> Option<PortDirection> {
        // Iterate tokens, find direction keyword, wrap it
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| PortDirection::can_cast(token.kind()))
            .map(PortDirection)
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        // Iterate children to find the TypeRef node
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        // Iterate children to find the BusSuffix node
        self.0.children().find_map(BusSuffix::cast)
    }
    // TODO: Add method for pin properties (e.g., pullup = true, functions = [...])
}

// --- Bus Suffix --- ([index] or [high:low])

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BusSuffix(Node);

impl AstNode for BusSuffix {
    fn can_cast(kind: SyntaxKind) -> bool {
        // Check specific kinds used by parser
        kind == SyntaxKind::BUS_SUFFIX || kind == SyntaxKind::PIN_BUS_SUFFIX
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(BusSuffix);
impl BusSuffix {
    // Inside the brackets [], we can have a single NUMBER (index) or a RangeExpr (high:low)
    pub fn index(&self) -> Option<Token> {
        // Iterate children tokens within BusSuffix for NUMBER
        self.0.children_with_tokens()
           .filter_map(|e| e.into_token())
           .find(|t| t.kind() == SyntaxKind::NUMBER)
    }

    pub fn range(&self) -> Option<RangeExpr> {
        // Iterate children nodes within BusSuffix for RangeExpr
        self.0.children().find_map(RangeExpr::cast)
    }
}

// --- Range Expression --- (e.g., 7:0, start to end)

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RangeExpr(Node);

impl AstNode for RangeExpr {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::RANGE_EXPR
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(RangeExpr);
impl RangeExpr {
    // Assuming format is LHS separator RHS (separator is : or to)
    pub fn lhs(&self) -> Option<Node> { // Could be NUMBER, IDENT_REF, etc.
        self.0.first_child()
    }

    pub fn rhs(&self) -> Option<Node> { // Could be NUMBER, IDENT_REF, etc.
         self.0.last_child()
    }

    pub fn separator_kind(&self) -> Option<SyntaxKind> {
        self.0.children_with_tokens()
            .find(|e| e.kind() == SyntaxKind::COLON || e.kind() == SyntaxKind::TO_KW)
            .map(|e| e.kind())
    }
}

// --- Interface Instance --- (within components/modules interfaces { ... })

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceInstance(Node);

impl AstNode for InterfaceInstance {
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::INTERFACE_INSTANCE
    }
    fn cast(node: Node) -> Option<Self> {
        Self::can_cast(node.kind()).then_some(Self(node))
    }
    fn syntax(&self) -> &Node {
        &self.0
    }
}

impl_rowan_ast_node!(InterfaceInstance);
impl HasName for InterfaceInstance {} // Instance name (e.g., SPI1)

impl InterfaceInstance {
    // Instance has a name (SPI1), a type (interface SPI), and optionally a pin_map or parameters

    pub fn interface_keyword(&self) -> Option<Token> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::INTERFACE_KW)
    }

    pub fn type_ref(&self) -> Option<TypeRef> {
        // Iterate children to find the TypeRef node
        self.0.children().find_map(TypeRef::cast)
    }

    pub fn pin_map_block(&self) -> Option<PinMapBlock> {
        // Iterate children to find the PinMapBlock node
        self.0.children().find_map(PinMapBlock::cast)
    }

    // TODO: Add method for parameter overrides (e.g., max_freq = 50MHz)
}

// Add other common nodes like Expression, PinRef, etc. later 