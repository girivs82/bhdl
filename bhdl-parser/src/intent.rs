// BHDL Intent Parsing
// Supports the 'for' keyword intent system

use crate::syntax::SyntaxKind;
use super::core::{Parser, SyntaxKindExt};

impl<'t> Parser<'t> {
    /// Parse intent clause: for intent_name(params)
    pub(crate) fn parse_intent_clause(&mut self) {
        self.builder.start_node(SyntaxKind::INTENT_CLAUSE.into());
        self.expect(SyntaxKind::FOR_KW);
        
        // Parse intent function call
        self.parse_intent_call();
        
        self.builder.finish_node();
    }
    
    /// Parse intent function call: intent_name(params)
    fn parse_intent_call(&mut self) {
        self.builder.start_node(SyntaxKind::INTENT_CALL.into());
        
        // Intent function name
        self.expect(SyntaxKind::IDENT);
        
        // Parameter list
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_intent_params();
        }
        
        self.builder.finish_node();
    }
    
    /// Parse intent parameters: (param1, param2: value, ...)
    fn parse_intent_params(&mut self) {
        self.builder.start_node(SyntaxKind::INTENT_PARAMS.into());
        self.expect(SyntaxKind::L_PAREN);
        
        while self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
            // Could be named or positional parameter
            let checkpoint = self.builder.checkpoint();
            
            // Try to parse as identifier first
            if self.peek() == Some(SyntaxKind::IDENT) {
                // Look ahead to see if it's a named parameter
                let mut lookahead = self.pos + 1;
                while lookahead < self.tokens.len() && self.tokens[lookahead].0.is_trivia() {
                    lookahead += 1;
                }
                
                if lookahead < self.tokens.len() && self.tokens[lookahead].0 == SyntaxKind::COLON {
                    // Named parameter: name: value
                    self.builder.start_node_at(checkpoint, SyntaxKind::INTENT_NAMED_PARAM.into());
                    self.expect(SyntaxKind::IDENT);
                    self.expect(SyntaxKind::COLON);
                    self.parse_expr(0);
                    self.builder.finish_node();
                } else {
                    // Positional parameter
                    self.parse_expr(0);
                }
            } else {
                // Positional parameter (expression)
                self.parse_expr(0);
            }
            
            // Check for comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else if self.peek() != Some(SyntaxKind::R_PAREN) {
                self.error("Expected ',' or ')' in intent parameter list".to_string());
                break;
            }
        }
        
        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }
    
    /// Check if there's an intent clause (for keyword)
    pub(crate) fn has_intent_clause(&self) -> bool {
        self.peek() == Some(SyntaxKind::FOR_KW)
    }
}