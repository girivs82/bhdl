// BHDL v2.0 Item Parsing
// Only supports v2.0 flow-based syntax

use crate::syntax::SyntaxKind;
use super::core::Parser;

impl<'t> Parser<'t> {
    // v2.0 item parsing functions
    
    // Parses the bus suffix: [expr] or [expr:expr]
    pub(crate) fn parse_bus_suffix(&mut self) {
        self.builder.start_node(SyntaxKind::BUS_SUFFIX.into());
        self.expect(SyntaxKind::L_BRACKET); // Consume L_BRACKET here now

        // Parse the first expression (high bound or single index)
        let checkpoint_before_first_expr = self.builder.checkpoint();
        self.parse_expr(0);

        // Check if a colon follows, indicating a range
        if self.peek() == Some(SyntaxKind::COLON) {
            // It's a range. Re-wrap the first expression, colon, and second expression
            self.builder.start_node_at(checkpoint_before_first_expr, SyntaxKind::RANGE_EXPR.into());
            self.expect(SyntaxKind::COLON); // Consume colon
            self.parse_expr(0); // Parse second expr (low)
            self.builder.finish_node(); // Finish RANGE_EXPR
        }
        // If no colon, the first expr is just part of BUS_SUFFIX

        self.expect(SyntaxKind::R_BRACKET);
        self.builder.finish_node(); // Finish BUS_SUFFIX
    }

    // Parse net declaration: net net_name[range]: type;
    pub(crate) fn parse_net_decl(&mut self) {
        self.builder.start_node(SyntaxKind::NET_DECL.into());
        self.expect(SyntaxKind::NET_KW);
        self.expect(SyntaxKind::IDENT); // Net name

        // Optional bus suffix
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }

        // Optional type
        if self.eat(SyntaxKind::COLON) {
            self.parse_net_type();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse net type
    fn parse_net_type(&mut self) {
        self.builder.start_node(SyntaxKind::NET_TYPE.into());
        match self.peek() {
            Some(SyntaxKind::SIGNAL_KW) |
            Some(SyntaxKind::WIRE_KW) |
            Some(SyntaxKind::TRI_KW) |
            Some(SyntaxKind::TRIREG_KW) |
            Some(SyntaxKind::UWIRE_KW) => {
                self.bump();
            }
            _ => {
                self.error("Expected net type (signal, wire, tri, etc.)".to_string());
            }
        }
        self.builder.finish_node();
    }

    // Parse parameter assignment: param_name = value
    pub(crate) fn parse_param_assign(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM_ASSIGN.into());
        self.expect(SyntaxKind::IDENT); // Parameter name
        self.expect(SyntaxKind::EQ);
        self.parse_expr(0); // Parameter value
        self.builder.finish_node();
    }

    // Parse parameter assignment without keyword
    pub(crate) fn parse_param_assign_no_kw(&mut self) {
        self.parse_param_assign();
        self.expect(SyntaxKind::SEMI);
    }

    // Parse type reference
    pub(crate) fn parse_type_ref(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_REF.into());
        
        // Base type - can be an identifier or a keyword that's also a type
        match self.peek() {
            Some(SyntaxKind::IDENT) |
            Some(SyntaxKind::POWER_KW) |
            Some(SyntaxKind::SIGNAL_KW) |
            Some(SyntaxKind::GROUND_KW) => {
                self.bump();
            }
            _ => {
                self.error("Expected type name".to_string());
                // Try to recover by treating the next token as the type
                if self.peek().is_some() {
                    self.bump();
                }
            }
        }
        
        // Optional type parameters
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.builder.start_node(SyntaxKind::TYPE_PARAMS.into());
            self.bump(); // L_PAREN
            self.parse_expr(0);
            self.expect(SyntaxKind::R_PAREN);
            self.builder.finish_node();
        }
        
        self.builder.finish_node();
    }
}