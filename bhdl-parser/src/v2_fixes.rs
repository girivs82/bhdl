// BHDL v2.0 Parser Fixes
// This file contains the improvements needed to support full v2.0 syntax

use crate::{Parser, SyntaxKind};
use crate::core::SyntaxKindExt;

impl<'t> Parser<'t> {
    /// Enhanced flow statement detection that differentiates between:
    /// - Flow statements: `power_flow: USB_5V |> protection |> distribution;`
    /// - Interface instances: `main_i2c: I2C(voltage=3.3V, frequency=400kHz);`
    /// - Named component handles in connections: `USB_5V -> regulator: LinearReg(3.3V, 1A).IN;`
    pub(crate) fn is_v2_named_declaration(&mut self) -> NamedDeclarationType {
        let mut pos = self.pos;
        
        // Skip leading trivia
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        
        // Must start with IDENT
        if pos >= self.tokens.len() || self.tokens[pos].0 != SyntaxKind::IDENT {
            return NamedDeclarationType::None;
        }
        
        // Skip the IDENT
        pos += 1;
        
        // Skip trivia after IDENT
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        
        // Must have COLON
        if pos >= self.tokens.len() || self.tokens[pos].0 != SyntaxKind::COLON {
            return NamedDeclarationType::None;
        }
        
        // Skip COLON
        pos += 1;
        
        // Skip trivia after COLON
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        
        // Now determine the type based on what follows
        if pos < self.tokens.len() {
            match self.tokens[pos].0 {
                // Module/Component/Interface instance: name: TypeName(params) or name: TypeName { ... }
                SyntaxKind::IDENT => {
                    // Look ahead for what follows the type name
                    let mut next_pos = pos + 1;
                    while next_pos < self.tokens.len() && self.tokens[next_pos].0.is_trivia() {
                        next_pos += 1;
                    }
                    if next_pos < self.tokens.len() {
                        match self.tokens[next_pos].0 {
                            SyntaxKind::L_PAREN => {
                                // Could be interface or component - look further
                                let mut after_paren = next_pos + 1;
                                let mut paren_depth = 1;
                                while after_paren < self.tokens.len() && paren_depth > 0 {
                                    match self.tokens[after_paren].0 {
                                        SyntaxKind::L_PAREN => paren_depth += 1,
                                        SyntaxKind::R_PAREN => paren_depth -= 1,
                                        _ => {}
                                    }
                                    after_paren += 1;
                                }
                                // Skip trivia after closing paren
                                while after_paren < self.tokens.len() && self.tokens[after_paren].0.is_trivia() {
                                    after_paren += 1;
                                }
                                // Check what follows the parentheses
                                if after_paren < self.tokens.len() {
                                    match self.tokens[after_paren].0 {
                                        SyntaxKind::L_BRACE => return NamedDeclarationType::ModuleInstance,
                                        SyntaxKind::SEMI => return NamedDeclarationType::ComponentInstance,
                                        _ => return NamedDeclarationType::InterfaceInstance,
                                    }
                                }
                                return NamedDeclarationType::InterfaceInstance;
                            }
                            SyntaxKind::L_BRACE => {
                                // Module instance without params
                                return NamedDeclarationType::ModuleInstance;
                            }
                            _ => {}
                        }
                    }
                    // Otherwise it's a flow statement
                    return NamedDeclarationType::FlowStatement;
                }
                // Flow statement starts with another identifier or flow operator
                SyntaxKind::FLOW_OP | SyntaxKind::ARROW | SyntaxKind::BI_ARROW => {
                    return NamedDeclarationType::FlowStatement;
                }
                _ => {}
            }
        }
        
        NamedDeclarationType::FlowStatement
    }
    
    /// Parse an interface instance declaration: name: InterfaceType(params);
    pub(crate) fn parse_interface_instance_decl(&mut self) {
        // In v2.0, interface instances are just named type instantiations
        self.builder.start_node(SyntaxKind::COMPONENT_INST.into());
        
        self.expect(SyntaxKind::IDENT); // Instance name
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT); // Interface type
        
        // Parse parameters if present
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_v2();
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse parameter list: (param1=value1, param2=value2)
    pub(crate) fn parse_param_list_v2(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM_LIST.into());
        self.expect(SyntaxKind::L_PAREN);
        
        loop {
            self.skip_trivia();
            if self.peek() == Some(SyntaxKind::R_PAREN) {
                break;
            }
            
            // Parse param=value
            self.expect(SyntaxKind::IDENT);
            self.expect(SyntaxKind::EQ);
            self.parse_expr(0);
            
            // Check for comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else {
                break;
            }
        }
        
        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }
    
    /// Enhanced connection expression parser that handles named component handles
    pub(crate) fn parse_v2_connection_expr_enhanced(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());
        
        // Parse the full connection expression, which may include named handles
        // The expression parser should handle the complex flow with named handles
        self.parse_expr(0);
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse array syntax for interface connections: [item1, item2, ...]
    pub(crate) fn parse_array_expr(&mut self) {
        self.builder.start_node(SyntaxKind::ARRAY_EXPR.into());
        self.expect(SyntaxKind::L_BRACKET);
        
        loop {
            self.skip_trivia();
            if self.peek() == Some(SyntaxKind::R_BRACKET) {
                break;
            }
            
            self.parse_expr(0);
            
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else {
                break;
            }
        }
        
        self.expect(SyntaxKind::R_BRACKET);
        self.builder.finish_node();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NamedDeclarationType {
    None,
    FlowStatement,
    InterfaceInstance,
    ModuleInstance,
    ComponentInstance,
}