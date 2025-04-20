// Content for bhdl-parser/src/expressions.rs
// Will be populated in the next step.

use crate::syntax::SyntaxKind;
use super::core::{Parser, SyntaxKindExt};

impl<'t> Parser<'t> {
    // Expression parsing functions (parse_expr, parse_primary_expr, precedence funcs, parse_value, etc.)
    // pub(crate) fn parse_expr(&mut self, min_bp: u8) { ... }
    // ... other implementations ...

    // --- Expression Parsing (Precedence Climbing) ---

    // Get the binding power (precedence) for prefix unary operators
    fn prefix_binding_power(&self, kind: SyntaxKind) -> Option<((), u8)> {
        match kind {
            SyntaxKind::PLUS | SyntaxKind::MINUS => Some(((), 13)), // Unary + / - (Higher than binary +/-)
            SyntaxKind::BANG | SyntaxKind::TILDE => Some(((), 13)), // Logical NOT, Bitwise NOT
            _ => None,
        }
    }

    // Get the binding power (precedence) for infix binary operators
    fn infix_binding_power(&self, kind: SyntaxKind) -> Option<(u8, u8)> {
        match kind {
            // Lowest precedence
            SyntaxKind::PIPEPIPE => Some((1, 2)),    // || (logical OR)
            SyntaxKind::AMPAMP => Some((3, 4)),      // && (logical AND)
            SyntaxKind::PIPE => Some((5, 6)),        // | (bitwise OR)
            SyntaxKind::CARET => Some((7, 8)),       // ^ (bitwise XOR)
            SyntaxKind::AMPERSAND => Some((9, 10)),   // & (bitwise AND)
            SyntaxKind::EQEQ | SyntaxKind::NEQ => Some((11, 12)), // ==, != (equality)
            SyntaxKind::L_ANGLE | SyntaxKind::R_ANGLE | SyntaxKind::LTEQ | SyntaxKind::GTEQ => Some((11, 12)), // <, >, <=, >= (comparison)
            // Shift operators <<, >> could go here if needed
            SyntaxKind::PLUS | SyntaxKind::MINUS => Some((13, 14)), // +, - (additive)
            SyntaxKind::STAR | SyntaxKind::SLASH | SyntaxKind::PERCENT => Some((15, 16)), // *, /, % (multiplicative)
            // Highest precedence (excluding prefix/postfix)
            _ => None,
        }
    }

    // Get the binding power for the ternary '?' operator (treat similar to infix)
    // ':' is handled specially within parse_expr
    fn ternary_binding_power(&self, kind: SyntaxKind) -> Option<(u8, u8)> {
        match kind {
            SyntaxKind::QUESTION => Some((4, 3)), // Use QUESTION now. Precedence slightly above assignment, right-associative for ':' part
            _ => None,
        }
    }

    // Main expression parsing function using precedence climbing
    // Parses expressions with a minimum binding power (precedence level)
    pub(crate) fn parse_expr(&mut self, min_bp: u8) {
        // Start a general EXPR node - Or maybe handle this differently? See below.
        let mut checkpoint = self.builder.checkpoint(); // Checkpoint before LHS

        // Parse the left-hand side (LHS) - Condition or start of binary expr
        // Check for prefix operators first
        // Removed unused lhs_parsed variable and checks
        if let Some(((), r_bp)) = self.peek().and_then(|k| self.prefix_binding_power(k)) {
            self.builder.start_node(SyntaxKind::PREFIX_EXPR.into());
            self.bump(); // Consume the operator token
            self.parse_expr(r_bp); // Parse the operand with higher precedence
            self.builder.finish_node(); // Finish PREFIX_EXPR
        } else {
            // If no prefix operator, parse a primary expression
            self.parse_primary_expr();
        }

        // Loop for infix and ternary operators (Precedence Climbing)
        loop {
            let current_op = match self.peek() {
                Some(op) => op,
                None => break, // End of input
            };

            // Check for ternary first due to its unique structure
            if current_op == SyntaxKind::QUESTION { // Use QUESTION now
                 if let Some((l_bp, r_bp)) = self.ternary_binding_power(current_op) {
                     if l_bp < min_bp {
                         break;
                     }

                    // Start TERNARY_EXPR node, wrapping the LHS (condition)
                    self.builder.start_node_at(checkpoint, SyntaxKind::TERNARY_EXPR.into());
                    self.bump(); // Consume '?' (BANG)

                    // Parse the 'true' expression
                    self.parse_expr(r_bp); // r_bp determines right-associativity for ':'

                    // Expect ':'
                    self.expect(SyntaxKind::COLON);

                    // Parse the 'false' expression
                    self.parse_expr(r_bp); // Use same r_bp for right-associativity

                    self.builder.finish_node(); // Finish TERNARY_EXPR
                    checkpoint = self.builder.checkpoint(); // Update checkpoint after ternary
                    continue; // Restart loop check after ternary
                 }
            }

            // Standard infix binary operators
            if let Some((l_bp, r_bp)) = self.infix_binding_power(current_op) {
                if l_bp < min_bp {
                    break; // Operator precedence is lower than current minimum
                }

                // Consume the operator
                self.bump();

                // Wrap the LHS and operator with RHS into a BINARY_EXPR node
                self.builder.start_node_at(checkpoint, SyntaxKind::BINARY_EXPR.into());
                self.parse_expr(r_bp); // Parse the right-hand side (RHS)
                self.builder.finish_node(); // Finish BINARY_EXPR
                checkpoint = self.builder.checkpoint(); // Update checkpoint after binary expr
            } else {
                break; // Not a binary operator we handle or end of expression part
            }
        }
         // No overall EXPRESSION node finished here - nodes are created by prefix/primary/binary/ternary calls
    }

    // Parses primary expressions: Literals, Identifiers, Parenthesized expressions
    pub(crate) fn parse_primary_expr(&mut self) {
        match self.peek() {
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::STRING) |
            Some(SyntaxKind::TRUE_KW) | Some(SyntaxKind::FALSE_KW) => {
                // Can potentially wrap this in a LITERAL_EXPR node
                self.parse_value(); // Handles literals and units
            }
            Some(SyntaxKind::IDENT) => {
                let checkpoint = self.builder.checkpoint(); // Checkpoint before IDENT
                self.bump(); // Consume IDENT (don't wrap in node yet)

                // Check what follows the identifier
                match self.peek() {
                    Some(SyntaxKind::L_PAREN) => {
                        // Function Call: Wrap IDENT in FUNCTION_CALL_EXPR
                        self.builder.start_node_at(checkpoint, SyntaxKind::FUNCTION_CALL_EXPR.into());
                        self.parse_argument_list(); // Consumes (...) including parens
                        self.builder.finish_node();
                    }
                    Some(SyntaxKind::L_BRACKET) => {
                        // Net/Pin Reference with Suffix: Wrap IDENT in NET_REF (or similar)
                        // For now, assume NET_REF. Might need refinement later.
                        // This should call parse_bus_suffix which is in items.rs now
                        // Need to ensure parse_bus_suffix is pub(crate)
                        self.builder.start_node_at(checkpoint, SyntaxKind::NET_REF.into());
                        self.parse_bus_suffix(); // Consumes [...] including brackets
                        self.builder.finish_node();
                    }
                    _ => {
                        // Simple Identifier Reference: Wrap IDENT in IDENT_REF
                        self.builder.start_node_at(checkpoint, SyntaxKind::IDENT_REF.into());
                        self.builder.finish_node(); // Finishes node containing only the IDENT bumped earlier
                    }
                }
            }
            Some(SyntaxKind::L_PAREN) => {
                self.bump(); // Consume '('
                self.parse_expr(0); // Parse nested expression (reset precedence)
                self.expect(SyntaxKind::R_PAREN); // Expect ')'
            }
            _ => {
                self.error(format!("Expected literal, identifier, or '(' for expression factor, found {:?}", self.peek())); // Use peek()
                // Consume unexpected token for recovery?
                if self.peek().is_some() { self.bump_any(); }
                // Add an ERROR node maybe?
                self.builder.start_node(SyntaxKind::ERROR.into());
                self.builder.finish_node();
            }
        }
    }

    // Parses the argument list for a function call: (arg1, arg2, ...)
    pub(crate) fn parse_argument_list(&mut self) {
        self.builder.start_node(SyntaxKind::ARGUMENT_LIST.into());
        self.expect(SyntaxKind::L_PAREN);

        // Parse comma-separated expressions until ')'
        let mut first_arg = true;
        while self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
            if !first_arg {
                self.expect(SyntaxKind::COMMA);
                // Handle potential trailing comma before ')'
                if self.peek() == Some(SyntaxKind::R_PAREN) { break; }
            }
            first_arg = false;
            self.parse_expr(0); // Parse argument expression
        }

        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }

    // Parses a simple value (NUMBER, STRING, BOOL, optionally with sign and unit)
    pub(crate) fn parse_value(&mut self) {
        self.builder.start_node(SyntaxKind::VALUE.into()); // Start VALUE node
        let mut _has_sign = false;
        if self.peek() == Some(SyntaxKind::MINUS) || self.peek() == Some(SyntaxKind::PLUS) {
            self.bump(); // Consume the sign
            _has_sign = true;
        }

        if self.eat(SyntaxKind::NUMBER) {
            // Check if the *next* token is an IDENT matching a known single-letter unit.
            // Use peek() because trivia (like spaces) might be valid *between* number and unit,
            // although adjacent is more common (e.g., 10k vs 10 k). Let's allow space for now.
            if self.peek() == Some(SyntaxKind::IDENT) {
                // Need to get the actual text of the upcoming IDENT token
                // We can't use self.tokens[self.pos] directly because peek() advances internally.
                // Let's find the actual index peek() stops at.
                let mut next_token_pos = self.pos;
                while self.tokens.get(next_token_pos).map_or(false, |(k, _)| k.is_trivia()) {
                    next_token_pos += 1;
                }

                if let Some((_, text)) = self.tokens.get(next_token_pos) {
                    // Check if the IDENT text is a single-letter unit
                    match text.as_str() {
                        "F" | "H" | "V" | "A" | "W" | "s" | "%" => {
                            // It's a unit, consume it as part of the VALUE node.
                            // We need to bump up to and including this token.
                            self.skip_trivia(); // Consume any trivia before the unit IDENT
                            self.bump(); // Consume the unit IDENT itself
                        }
                        _ => { /* Not a single-letter unit IDENT, do nothing extra */ }
                    }
                }
            } 
            // Also check for multi-letter units (which are UNIT_IDENTIFIER kind)
            else if self.peek() == Some(SyntaxKind::UNIT_IDENTIFIER) { 
                 self.bump(); // Consume the multi-letter unit
            }

        } else if self.eat(SyntaxKind::STRING) {
            // String literals are also values
        } else if self.eat(SyntaxKind::TRUE_KW) || self.eat(SyntaxKind::FALSE_KW) {
            // Boolean literals are also values
        } else {
            // If no number, string, or bool was found (after optional sign), report error
            let found = self.peek(); // Peek after potential sign consumption
            self.error(format!("Expected number, string, or boolean literal, found {:?}", found));
             // Add an ERROR node to mark the location if nothing was parsed?
             // self.builder.start_node(ERROR.into());
             // self.builder.finish_node();
        }
        self.builder.finish_node(); // Finish VALUE node
    }

    // --- End of Expression Parsing ---
} 