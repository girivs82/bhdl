// bhdl-ast/src/expr.rs
use crate::common::{Value, IdentRef, NetRef, PinRef}; // Import necessary leaf nodes
use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken}; // Add SyntaxNode/Token
use rowan::ast::AstNode;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Expr {
    Value(Value),
    IdentRef(IdentRef),
    NetRef(NetRef),
    PinRef(PinRef),  // Add pin references for behavioral modeling
    PrefixExpr(PrefixExpr),
    BinaryExpr(BinaryExpr),
    TernaryExpr(TernaryExpr),
    FunctionCallExpr(FunctionCallExpr),
    // Add flow-specific expressions
    FlowExpr(FlowExpr),
    ComponentInstExpr(ComponentInstExpr),
    // For simple identifiers (like attribute references)
    Ident(SyntaxNode<BhdlLanguage>),
    // For literal values
    Literal(SyntaxNode<BhdlLanguage>),
}

impl AstNode for Expr {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        Value::can_cast(kind) ||
        IdentRef::can_cast(kind) ||
        NetRef::can_cast(kind) ||
        PinRef::can_cast(kind) ||
        PrefixExpr::can_cast(kind) ||
        BinaryExpr::can_cast(kind) ||
        TernaryExpr::can_cast(kind) ||
        FunctionCallExpr::can_cast(kind) ||
        FlowExpr::can_cast(kind) ||
        ComponentInstExpr::can_cast(kind) ||
        kind == SyntaxKind::IDENT ||
        matches!(kind, SyntaxKind::NUMBER | SyntaxKind::STRING)
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        match syntax.kind() {
            _ if Value::can_cast(syntax.kind()) => 
                Some(Expr::Value(Value::cast(syntax.clone())?)),
            _ if IdentRef::can_cast(syntax.kind()) => 
                Some(Expr::IdentRef(IdentRef::cast(syntax.clone())?)),
            _ if NetRef::can_cast(syntax.kind()) => 
                Some(Expr::NetRef(NetRef::cast(syntax.clone())?)),
            _ if PinRef::can_cast(syntax.kind()) => 
                Some(Expr::PinRef(PinRef::cast(syntax.clone())?)),
            _ if PrefixExpr::can_cast(syntax.kind()) => 
                Some(Expr::PrefixExpr(PrefixExpr::cast(syntax.clone())?)),
            _ if BinaryExpr::can_cast(syntax.kind()) => 
                Some(Expr::BinaryExpr(BinaryExpr::cast(syntax.clone())?)),
            _ if TernaryExpr::can_cast(syntax.kind()) => 
                Some(Expr::TernaryExpr(TernaryExpr::cast(syntax.clone())?)),
            _ if FunctionCallExpr::can_cast(syntax.kind()) => 
                Some(Expr::FunctionCallExpr(FunctionCallExpr::cast(syntax.clone())?)),
            _ if FlowExpr::can_cast(syntax.kind()) => 
                Some(Expr::FlowExpr(FlowExpr::cast(syntax.clone())?)),
            _ if ComponentInstExpr::can_cast(syntax.kind()) => 
                Some(Expr::ComponentInstExpr(ComponentInstExpr::cast(syntax.clone())?)),
            SyntaxKind::IDENT => 
                Some(Expr::Ident(syntax)),
            SyntaxKind::NUMBER | SyntaxKind::STRING => 
                Some(Expr::Literal(syntax)),
            _ => None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        match self {
            Expr::Value(n) => n.syntax(),
            Expr::IdentRef(n) => n.syntax(),
            Expr::NetRef(n) => n.syntax(),
            Expr::PinRef(n) => n.syntax(),
            Expr::PrefixExpr(n) => n.syntax(),
            Expr::BinaryExpr(n) => n.syntax(),
            Expr::TernaryExpr(n) => n.syntax(),
            Expr::FunctionCallExpr(n) => n.syntax(),
            Expr::FlowExpr(n) => n.syntax(),
            Expr::ComponentInstExpr(n) => n.syntax(),
            Expr::Ident(n) => n,
            Expr::Literal(n) => n,
        }
    }
}

// Wrapper structs for expression kinds

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PrefixExpr(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for PrefixExpr { 
    type Language = BhdlLanguage; // Add Language type
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PREFIX_EXPR } 
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } } 
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 } 
}
impl PrefixExpr {
    pub fn op(&self) -> Option<SyntaxKind> { self.0.first_token().map(|t| t.kind()) }
    pub fn expr(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BinaryExpr(pub(crate) SyntaxNode<BhdlLanguage>);
impl AstNode for BinaryExpr { 
    type Language = BhdlLanguage; // Add Language type
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::BINARY_EXPR } 
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } } 
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 } 
}
impl BinaryExpr {
    pub fn lhs(&self) -> Option<Expr> { self.0.children().find_map(Expr::cast) }
    pub fn op(&self) -> Option<SyntaxKind> { 
        self.0.children_with_tokens().find_map(|node_or_token| {
            node_or_token.into_token().filter(|token| {
                // Include flow operators in binary expressions
                matches!(token.kind(), 
                    SyntaxKind::PLUS | SyntaxKind::MINUS | SyntaxKind::STAR | SyntaxKind::SLASH | 
                    SyntaxKind::AMPERSAND | SyntaxKind::PIPE | SyntaxKind::CARET | 
                    SyntaxKind::EQEQ | SyntaxKind::NEQ | // Use EQEQ not EQ
                    SyntaxKind::L_ANGLE | SyntaxKind::LTEQ | // Use L_ANGLE for <
                    SyntaxKind::R_ANGLE | SyntaxKind::GTEQ | // Use R_ANGLE for >
                    // Flow operators
                    SyntaxKind::ARROW | SyntaxKind::BI_ARROW | 
                    SyntaxKind::FLOW_OP | SyntaxKind::INTERFACE_OP
                )
            })
        }).map(|t| t.kind())
    }
    pub fn rhs(&self) -> Option<Expr> {
        // Find the second Expr child
        self.0.children().filter_map(Expr::cast).nth(1)
    }
}

// --- Ternary Expression --- `condition ? true_expr : false_expr`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TernaryExpr(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for TernaryExpr {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::TERNARY_EXPR }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl TernaryExpr {
    pub fn condition(&self) -> Option<Expr> { 
        self.0.children().find_map(Expr::cast) 
    }
    
    pub fn true_expr(&self) -> Option<Expr> { 
        self.0.children().filter_map(Expr::cast).nth(1) 
    }
    
    pub fn false_expr(&self) -> Option<Expr> { 
        self.0.children().filter_map(Expr::cast).nth(2) 
    }
    
    pub fn question_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::QUESTION)
    }
    
    pub fn colon_token(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::COLON)
    }
}

// --- Function Call Expression --- `function_name(arg1, arg2, ...)`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionCallExpr(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for FunctionCallExpr {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::FUNCTION_CALL_EXPR }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl FunctionCallExpr {
    pub fn function_name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
    
    pub fn argument_list(&self) -> Option<ArgumentList> {
        self.0.children().find_map(ArgumentList::cast)
    }
    
    pub fn arguments(&self) -> impl Iterator<Item = Expr> {
        self.argument_list()
            .map(|list| list.arguments())
            .into_iter()
            .flatten()
    }
}

// --- Argument List --- `(arg1, arg2, ...)`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ArgumentList(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ArgumentList {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ARGUMENT_LIST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ArgumentList {
    pub fn arguments(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

// --- Flow Expression (as part of expressions) ---
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowExpr(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for FlowExpr {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::FLOW_EXPR }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl FlowExpr {
    pub fn elements(&self) -> impl Iterator<Item = Expr> {
        self.0.children().filter_map(Expr::cast)
    }
}

// --- Component Instantiation Expression --- `Res(330Ω)` in expressions
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentInstExpr(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ComponentInstExpr {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::COMPONENT_INST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ComponentInstExpr {
    pub fn component_type(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|e| e.into_token())
            .find(|t| t.kind() == SyntaxKind::IDENT)
    }
    
    pub fn parameters(&self) -> impl Iterator<Item = Expr> {
        // Find parameter expressions within the instantiation
        self.0.children().filter_map(Expr::cast)
    }
} 