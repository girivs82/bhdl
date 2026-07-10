// BHDL v2.0 Block Parsing
// Only supports v2.0 flow-based syntax

use crate::syntax::SyntaxKind;
use super::core::Parser;

impl<'t> Parser<'t> {
    // v2.0 only supports module definitions with simple pin declarations
    // No more v1.0 block structures like pins {}, interfaces {}, etc.
    
    // Parse generate blocks (still supported in v2.0)
    pub(crate) fn parse_generate_block(&mut self) {
        self.builder.start_node(SyntaxKind::GENERATE_BLOCK.into());
        self.expect(SyntaxKind::GENERATE_KW);
        
        // Check if this is a for loop
        if self.peek() == Some(SyntaxKind::FOR_KW) {
            self.parse_generate_for();
        } else if self.peek() == Some(SyntaxKind::IF_KW) {
            self.parse_generate_if();
        } else {
            self.expect(SyntaxKind::L_BRACE);
            self.parse_block_contents();
            self.expect(SyntaxKind::R_BRACE);
        }
        
        self.builder.finish_node();
    }
    
    // Parse generate for loops
    pub(crate) fn parse_generate_for(&mut self) {
        self.builder.start_node(SyntaxKind::FOR_LOOP_GENERATE.into());
        self.expect(SyntaxKind::FOR_KW);
        
        // Variable name
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::IN_KW);
        
        // Range expression: `0..8` (start .. end). The expression parser
        // does not treat `..` as an operator, so consume it here — the AST
        // layer (bhdl_ast::ForLoopGenerate::range_bounds) expects the
        // range tokens `NUMBER DOT_DOT NUMBER` inside FOR_LOOP_GENERATE.
        self.parse_expression();
        if self.peek() == Some(SyntaxKind::DOT_DOT) {
            self.bump(); // ..
            self.parse_expression(); // end bound
        }

        self.expect(SyntaxKind::L_BRACE);
        self.parse_block_contents();
        self.expect(SyntaxKind::R_BRACE);
        
        self.builder.finish_node();
    }
    
    // Parse generate if conditions
    pub(crate) fn parse_generate_if(&mut self) {
        self.builder.start_node(SyntaxKind::IF_GENERATE.into());
        self.expect(SyntaxKind::IF_KW);
        self.expect(SyntaxKind::L_PAREN);
        self.parse_expression();
        self.expect(SyntaxKind::R_PAREN);
        
        self.expect(SyntaxKind::L_BRACE);
        self.parse_block_contents();
        self.expect(SyntaxKind::R_BRACE);
        
        // Optional else
        if self.peek() == Some(SyntaxKind::ELSE_KW) {
            self.bump();
            self.expect(SyntaxKind::L_BRACE);
            self.parse_block_contents();
            self.expect(SyntaxKind::R_BRACE);
        }
        
        self.builder.finish_node();
    }
    
    // Parse block contents for v2.0 (connection statements, power/ground declarations)
    fn parse_block_contents(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::POWER_KW) => self.parse_power_decl(),
                Some(SyntaxKind::GROUND_KW) => self.parse_ground_decl(),
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_block(),
                Some(SyntaxKind::IDENT) | Some(SyntaxKind::AT) => {
                    // Could be a connection statement or flow statement
                    // Can start with IDENT or @ (for net references)
                    self.parse_connection_or_flow_stmt();
                }
                Some(_) => {
                    self.error("Unexpected token in block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in block".to_string());
                    break;
                }
            }
        }
    }
}