// AST nodes for BHDL v2.0 statements

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode, SyntaxToken};
use rowan::ast::AstNode;

// Extension trait for SyntaxKind
trait SyntaxKindExt {
    fn is_trivia(self) -> bool;
}

impl SyntaxKindExt for SyntaxKind {
    fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }
}

/// Power declaration: `power VCC = 5V @ 1A;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PowerDecl(SyntaxNode<BhdlLanguage>);

impl AstNode for PowerDecl {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::POWER_DECL
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

impl PowerDecl {
    /// Get the power domain name (e.g., "VCC")
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the full specification text (e.g., "12V @ 1A")
    pub fn spec_text(&self) -> Option<String> {
        // Find everything after the equals sign up to the semicolon
        let tokens: Vec<_> = self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .collect();
        
        let mut found_eq = false;
        let mut spec_parts = Vec::new();
        
        for token in tokens {
            if token.kind() == SyntaxKind::EQ {
                found_eq = true;
                continue;
            }
            if found_eq && token.kind() == SyntaxKind::SEMI {
                break;
            }
            if found_eq && !token.kind().is_trivia() {
                spec_parts.push(token.text().to_string());
            }
        }
        
        if spec_parts.is_empty() {
            None
        } else {
            Some(spec_parts.join(" "))
        }
    }
    
    /// Get the voltage value (e.g., "5V")
    pub fn voltage(&self) -> Option<String> {
        // Find NUMBER and UNIT after equals sign
        let tokens: Vec<_> = self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .collect();
        
        let mut found_eq = false;
        let mut voltage_num = None;
        let mut voltage_unit = None;
        
        for token in tokens {
            match token.kind() {
                SyntaxKind::EQ => {
                    found_eq = true;
                }
                SyntaxKind::NUMBER if found_eq && voltage_num.is_none() => {
                    voltage_num = Some(token.text().to_string());
                }
                SyntaxKind::UNIT_IDENTIFIER | SyntaxKind::IDENT if found_eq && voltage_num.is_some() && voltage_unit.is_none() => {
                    voltage_unit = Some(token.text().to_string());
                }
                SyntaxKind::AT => {
                    break; // Stop at current specification
                }
                _ => {}
            }
        }
        
        match (voltage_num, voltage_unit) {
            (Some(num), Some(unit)) => Some(format!("{}{}", num, unit)),
            (Some(num), None) => Some(num),
            _ => None,
        }
    }
    
    /// Get the current value (e.g., "1A")
    pub fn current(&self) -> Option<String> {
        // Find NUMBER and UNIT after @ sign
        let tokens: Vec<_> = self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .collect();
        
        let mut found_at = false;
        let mut current_num = None;
        let mut current_unit = None;
        
        for token in tokens {
            match token.kind() {
                SyntaxKind::AT => {
                    found_at = true;
                }
                SyntaxKind::NUMBER if found_at && current_num.is_none() => {
                    current_num = Some(token.text().to_string());
                }
                SyntaxKind::UNIT_IDENTIFIER | SyntaxKind::IDENT if found_at && current_num.is_some() && current_unit.is_none() => {
                    current_unit = Some(token.text().to_string());
                }
                SyntaxKind::SEMI => {
                    break;
                }
                _ => {}
            }
        }
        
        match (current_num, current_unit) {
            (Some(num), Some(unit)) => Some(format!("{}{}", num, unit)),
            (Some(num), None) => Some(num),
            _ => None,
        }
    }
}

/// Ground declaration: `ground GND;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GroundDecl(SyntaxNode<BhdlLanguage>);

impl AstNode for GroundDecl {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::GROUND_DECL
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

impl GroundDecl {
    /// Get the ground net name (e.g., "GND")
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
}

/// Connection statement for v2.0 flow syntax
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConnectionStmt(SyntaxNode<BhdlLanguage>);

impl AstNode for ConnectionStmt {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONNECTION_STMT
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

impl ConnectionStmt {
    /// Get the connection expression (usually a BINARY_EXPR with ->)
    pub fn expr(&self) -> Option<SyntaxNode<BhdlLanguage>> {
        self.syntax().children().find(|n| n.kind() == SyntaxKind::BINARY_EXPR)
    }
    
    /// Get the full text of the connection
    pub fn text(&self) -> String {
        self.syntax().text().to_string().trim().to_string()
    }
}

/// Flow statement: `flow_name: expr |> expr |> expr;`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FlowStmt(SyntaxNode<BhdlLanguage>);

impl AstNode for FlowStmt {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::FLOW_STMT
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

impl FlowStmt {
    /// Get the flow name (before the colon)
    pub fn name(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax()
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the flow expression
    pub fn expr(&self) -> Option<SyntaxNode<BhdlLanguage>> {
        // The expression is after the colon
        let mut found_colon = false;
        for child in self.syntax().children() {
            if found_colon {
                return Some(child);
            }
            // Check for colon in tokens
            for token in child.children_with_tokens() {
                if let Some(t) = token.as_token() {
                    if t.kind() == SyntaxKind::COLON {
                        found_colon = true;
                        break;
                    }
                }
            }
        }
        None
    }
}

// v2.0 doesn't have INTERFACE_INSTANCE - interfaces are used like components

/// Generate statement for v2.0
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct GenerateStmt(SyntaxNode<BhdlLanguage>);

impl AstNode for GenerateStmt {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::GENERATE_STMT
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

/// Conditional statement for v2.0
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ConditionalStmt(SyntaxNode<BhdlLanguage>);

impl AstNode for ConditionalStmt {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::CONDITIONAL_STMT
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

/// Enum for any v2.0 statement that can appear in a board
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Statement {
    PowerDecl(PowerDecl),
    GroundDecl(GroundDecl),
    ConnectionStmt(ConnectionStmt),
    FlowStmt(FlowStmt),
    GenerateStmt(GenerateStmt),
    ConditionalStmt(ConditionalStmt),
}

impl AstNode for Statement {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        PowerDecl::can_cast(kind) || 
        GroundDecl::can_cast(kind) || 
        ConnectionStmt::can_cast(kind) ||
        FlowStmt::can_cast(kind) ||
        GenerateStmt::can_cast(kind) ||
        ConditionalStmt::can_cast(kind)
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if PowerDecl::can_cast(syntax.kind()) {
            Some(Statement::PowerDecl(PowerDecl::cast(syntax)?))
        } else if GroundDecl::can_cast(syntax.kind()) {
            Some(Statement::GroundDecl(GroundDecl::cast(syntax)?))
        } else if ConnectionStmt::can_cast(syntax.kind()) {
            Some(Statement::ConnectionStmt(ConnectionStmt::cast(syntax)?))
        } else if FlowStmt::can_cast(syntax.kind()) {
            Some(Statement::FlowStmt(FlowStmt::cast(syntax)?))
        } else if GenerateStmt::can_cast(syntax.kind()) {
            Some(Statement::GenerateStmt(GenerateStmt::cast(syntax)?))
        } else if ConditionalStmt::can_cast(syntax.kind()) {
            Some(Statement::ConditionalStmt(ConditionalStmt::cast(syntax)?))
        } else {
            None
        }
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        match self {
            Statement::PowerDecl(s) => s.syntax(),
            Statement::GroundDecl(s) => s.syntax(),
            Statement::ConnectionStmt(s) => s.syntax(),
            Statement::FlowStmt(s) => s.syntax(),
            Statement::GenerateStmt(s) => s.syntax(),
            Statement::ConditionalStmt(s) => s.syntax(),
        }
    }
}