// Interface-specific AST nodes

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken, HasName};
use rowan::ast::AstNode;
use crate::items::ParamList;
use crate::expr::Expr;

/// Interface signal declaration: signal name: direction optional?;
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceSignal(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfaceSignal {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool { 
        kind == SyntaxKind::INTERFACE_SIGNAL 
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { 
            Some(Self(syntax)) 
        } else { 
            None 
        } 
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { 
        &self.0 
    }
}

impl InterfaceSignal {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        // The signal name is the IDENT token
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    pub fn direction(&self) -> Option<SignalDirection> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find_map(|token| match token.kind() {
                SyntaxKind::IN_KW => Some(SignalDirection::In),
                SyntaxKind::OUT_KW => Some(SignalDirection::Out),
                SyntaxKind::INOUT_KW => Some(SignalDirection::InOut),
                _ => None,
            })
    }
    
    pub fn is_optional(&self) -> bool {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .any(|token| token.kind() == SyntaxKind::OPTIONAL_KW)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SignalDirection {
    In,
    Out,
    InOut,
}

/// Interface requirement: require pullup(SDA, 4.7k);
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceRequirement(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfaceRequirement {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool { 
        kind == SyntaxKind::INTERFACE_REQUIREMENT 
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { 
            Some(Self(syntax)) 
        } else { 
            None 
        } 
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { 
        &self.0 
    }
}

impl InterfaceRequirement {
    pub fn requirement_type(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    pub fn arguments(&self) -> Vec<Expr> {
        self.syntax()
            .children()
            .find(|n| n.kind() == SyntaxKind::ARGUMENT_LIST)
            .map(|args| args.children().filter_map(Expr::cast).collect())
            .unwrap_or_default()
    }
}

/// Interface perspective: perspective master { ... }
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfacePerspective(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfacePerspective {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool { 
        kind == SyntaxKind::INTERFACE_PERSPECTIVE 
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { 
            Some(Self(syntax)) 
        } else { 
            None 
        } 
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { 
        &self.0 
    }
}

impl InterfacePerspective {
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    pub fn signals(&self) -> impl Iterator<Item = InterfaceSignal> {
        self.syntax().children().filter_map(InterfaceSignal::cast)
    }
}

/// Interface instantiation: bus: I2C();
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InterfaceInst(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for InterfaceInst {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool { 
        kind == SyntaxKind::INTERFACE_INST 
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> { 
        if Self::can_cast(syntax.kind()) { 
            Some(Self(syntax)) 
        } else { 
            None 
        } 
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> { 
        &self.0 
    }
}

impl HasName for InterfaceInst {}

impl InterfaceInst {
    pub fn interface_type(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        // The interface type is the identifier after the colon
        let mut after_colon = false;
        self.syntax()
            .children_with_tokens()
            .filter_map(|it| it.into_token())
            .find(|token| {
                if token.kind() == SyntaxKind::COLON {
                    after_colon = true;
                    false
                } else if after_colon && token.kind() == SyntaxKind::IDENT {
                    true
                } else {
                    false
                }
            })
    }
    
    pub fn params(&self) -> Option<ParamList> {
        self.syntax().children().find_map(ParamList::cast)
    }
}