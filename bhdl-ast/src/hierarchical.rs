// BHDL v2.0 Hierarchical Entity AST Nodes
// Support for entities containing entities and components

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use rowan::ast::AstNode;
use crate::items::ParamList;
use crate::expr::Expr;

/// Entity instantiation within an entity: instance_name: EntityType(params) { port mappings }
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityInst(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for EntityInst {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ENTITY_INST }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None }
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for EntityInst {}

impl EntityInst {
    /// Get the entity type name (e.g., "PWMController")
    pub fn entity_type(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        let mut found_colon = false;
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| {
                if token.kind() == SyntaxKind::COLON {
                    found_colon = true;
                    false
                } else if found_colon && token.kind() == SyntaxKind::IDENT {
                    true
                } else {
                    false
                }
            })
    }
    
    /// Get the parameter list if present
    pub fn param_list(&self) -> Option<ParamList> {
        self.0.children().find_map(ParamList::cast)
    }
    
    /// Get all port mappings
    pub fn port_mappings(&self) -> impl Iterator<Item = PortMapping> {
        self.0.children().filter_map(PortMapping::cast)
    }
    
    /// Get all scoped attributes
    pub fn scoped_attributes(&self) -> impl Iterator<Item = ScopedAttribute> {
        self.0.children().filter_map(ScopedAttribute::cast)
    }
}

/// Port mapping: PIN <- signal; or PIN -> signal;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortMapping(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PortMapping {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PORT_MAPPING }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl PortMapping {
    /// Get the pin reference (left side)
    pub fn pin_ref(&self) -> Option<PortPinRef> {
        self.0.children().find_map(PortPinRef::cast)
    }
    
    /// Get the connection target (right side)
    pub fn connection_target(&self) -> Option<ConnectionTarget> {
        self.0.children().find_map(ConnectionTarget::cast)
    }
    
    /// Get the connection operator (<-, ->, <->)
    pub fn operator(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| matches!(token.kind(), 
                SyntaxKind::LEFT_ARROW | SyntaxKind::ARROW | SyntaxKind::BI_ARROW))
    }
}

/// Pin reference in port mapping (could include array indexing)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PortPinRef(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for PortPinRef {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::PIN_REF }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl HasName for PortPinRef {}

impl PortPinRef {
    /// Get the bus suffix if present (e.g., [0] or [7:0])
    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        self.0.children().find_map(BusSuffix::cast)
    }
}

/// Connection target (signal or instance.pin)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionTarget(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ConnectionTarget {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::CONNECTION_TARGET }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ConnectionTarget {
    /// Get the target name (signal or instance name)
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the pin name if this is a qualified reference (instance.pin)
    pub fn pin_name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        let mut found_dot = false;
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| {
                if token.kind() == SyntaxKind::DOT {
                    found_dot = true;
                    false
                } else if found_dot && token.kind() == SyntaxKind::IDENT {
                    true
                } else {
                    false
                }
            })
    }
    
    /// Get the bus suffix if present
    pub fn bus_suffix(&self) -> Option<BusSuffix> {
        self.0.children().find_map(BusSuffix::cast)
    }
}

/// Scoped attribute: attribute path.to.attr = value;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ScopedAttribute(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ScopedAttribute {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::SCOPED_ATTRIBUTE }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl ScopedAttribute {
    /// Get the attribute path
    pub fn attribute_path(&self) -> Option<AttributePath> {
        self.0.children().find_map(AttributePath::cast)
    }
    
    /// Get the value expression
    pub fn value(&self) -> Option<Expr> {
        self.0.children().find_map(Expr::cast)
    }
}

/// Attribute path: path.to.attribute
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct AttributePath(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for AttributePath {
    type Language = BhdlLanguage;
    fn can_cast(kind: SyntaxKind) -> bool { kind == SyntaxKind::ATTRIBUTE_PATH }
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { Some(Self(syntax)) } else { None } 
    }
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { &self.0 }
}

impl AttributePath {
    /// Get all segments of the path
    pub fn segments(&self) -> Vec<SyntaxToken<BhdlLanguage>> {
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .filter(|token| token.kind() == SyntaxKind::IDENT)
            .collect()
    }
    
    /// Get the full path as a string
    pub fn as_string(&self) -> String {
        self.segments()
            .into_iter()
            .map(|token| token.text().to_string())
            .collect::<Vec<_>>()
            .join(".")
    }
}

// Re-export BusSuffix from common module
pub use crate::common::BusSuffix;