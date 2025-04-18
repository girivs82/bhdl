use rowan::{GreenNode, GreenNodeBuilder};
use smol_str::SmolStr;
use logos::Logos;
use crate::lexer::LexerToken;
use crate::syntax::{BhdlLanguage, SyntaxKind, SyntaxNode};

// Represents the output of the parsing process
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseResult {
    green_node: GreenNode,
    errors: Vec<ParseError>, // Keep track of errors encountered
}

impl ParseResult {
    pub fn syntax(&self) -> SyntaxNode {
        SyntaxNode::new_root(self.green_node.clone())
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }
}

// Placeholder for error reporting
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    // Add range/span later
    // pub span: (usize, usize),
}

// Define map_token_stream as a FREE function
fn map_token_stream(text: &str) -> Vec<(SyntaxKind, SmolStr)> {
    let mut mapped_tokens = Vec::new();
    let lexer = LexerToken::lexer(text);
    let mut spanned_iter = lexer.spanned();

    while let Some((result, span)) = spanned_iter.next() {
        let slice = SmolStr::new(&text[span]); // Get slice from span
        let mapped = match result {
            Ok(token) => match token {
                LexerToken::KeywordOrIdent(payload) => (payload.kind, payload.text),
                LexerToken::LParen => (SyntaxKind::L_PAREN, slice),
                LexerToken::RParen => (SyntaxKind::R_PAREN, slice),
                LexerToken::LBrace => (SyntaxKind::L_BRACE, slice),
                LexerToken::RBrace => (SyntaxKind::R_BRACE, slice),
                LexerToken::LBrack => (SyntaxKind::L_BRACKET, slice),
                LexerToken::RBrack => (SyntaxKind::R_BRACKET, slice),
                LexerToken::Semi => (SyntaxKind::SEMI, slice),
                LexerToken::Colon => (SyntaxKind::COLON, slice),
                LexerToken::Comma => (SyntaxKind::COMMA, slice),
                LexerToken::Eq => (SyntaxKind::EQ, slice),
                LexerToken::Dot => (SyntaxKind::DOT, slice),
                LexerToken::Plus => (SyntaxKind::PLUS, slice),
                LexerToken::Minus => (SyntaxKind::MINUS, slice),
                LexerToken::Star => (SyntaxKind::STAR, slice),
                LexerToken::Slash => (SyntaxKind::SLASH, slice),
                LexerToken::Percent => (SyntaxKind::PERCENT, slice),
                LexerToken::Ampersand => (SyntaxKind::AMPERSAND, slice),
                LexerToken::Pipe => (SyntaxKind::PIPE, slice),
                LexerToken::Caret => (SyntaxKind::CARET, slice),
                LexerToken::Bang => (SyntaxKind::BANG, slice),
                LexerToken::Tilde => (SyntaxKind::TILDE, slice),
                LexerToken::LAngle => (SyntaxKind::L_ANGLE, slice),
                LexerToken::RAngle => (SyntaxKind::R_ANGLE, slice),
                LexerToken::At => (SyntaxKind::AT, slice),
                LexerToken::Number => (SyntaxKind::NUMBER, slice),
                LexerToken::String => (SyntaxKind::STRING, slice),
                LexerToken::Arrow => (SyntaxKind::ARROW, slice),
                LexerToken::EqEq => (SyntaxKind::EQEQ, slice),
                LexerToken::Neq => (SyntaxKind::NEQ, slice),
                LexerToken::LtEq => (SyntaxKind::LTEQ, slice),
                LexerToken::GtEq => (SyntaxKind::GTEQ, slice),
                LexerToken::AmpAmp => (SyntaxKind::AMPAMP, slice),
                LexerToken::PipePipe => (SyntaxKind::PIPEPIPE, slice),
                LexerToken::LShift => (SyntaxKind::LSHIFT, slice),
                LexerToken::RShift => (SyntaxKind::RSHIFT, slice),
                LexerToken::Error => (SyntaxKind::ERROR_TOKEN, slice),
            },
            Err(()) => (SyntaxKind::ERROR_TOKEN, slice),
        };
        mapped_tokens.push(mapped);
    }
    mapped_tokens
}

// Parser struct - Correct definition
pub struct Parser<'t> {
    tokens: &'t [(SyntaxKind, SmolStr)],
    builder: GreenNodeBuilder<'static>,
    errors: Vec<ParseError>,
    pos: usize,
}

impl<'t> Parser<'t> {
    // Correct constructor signature
    fn new(tokens: &'t [(SyntaxKind, SmolStr)]) -> Self {
        Parser {
            tokens,
            builder: GreenNodeBuilder::new(),
            errors: Vec::new(),
            pos: 0,
        }
    }

    // Main parsing entry point
    fn parse_source_file(&mut self) {
        self.builder.start_node(SyntaxKind::SOURCE_FILE.into());
        // Loop through tokens and parse top-level items
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::BOARD_KW => self.parse_board_def(),
                SyntaxKind::MODULE_KW => self.parse_module_def(),
                SyntaxKind::TYPEDEF_KW => self.parse_typedef_def(),
                SyntaxKind::IMPORT_KW => self.parse_import_stmt(),
                SyntaxKind::COMPONENT_KW => self.parse_component_def(),
                SyntaxKind::INTERFACE_KW => self.parse_interface_def(), // Add case
                _ => {
                    // Handle unexpected tokens at the top level
                    self.error(format!("Expected a top-level item (e.g., 'board', 'component', 'interface', etc.), found {:?}", kind));
                    self.bump_any(); // Consume the unexpected token
                }
            }
        }
        self.builder.finish_node();
    }

    // Helper methods (examples)
    fn current(&self) -> Option<SyntaxKind> {
        self.tokens.get(self.pos).map(|(kind, _)| *kind)
    }

    fn bump(&mut self) {
        self.skip_trivia();
        if self.pos < self.tokens.len() {
            let (kind, text) = self.tokens[self.pos].clone();
            if kind != SyntaxKind::WHITESPACE && kind != SyntaxKind::COMMENT {
                 self.builder.token(kind.into(), &text);
                self.pos += 1;
            } else {
                self.error("Internal error: bump called on trivia".to_string());
                if self.pos < self.tokens.len() { self.pos += 1; }
            }
        }
    }

    // Add methods like peek(), expect(), eat(), etc.

    // --- Grammar Rule Parsers ---

    fn parse_board_def(&mut self) {
        self.builder.start_node(SyntaxKind::BOARD_DEF.into());
        self.expect(SyntaxKind::BOARD_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::L_BRACE);

        // Parse items inside the board block
        loop {
            self.skip_trivia(); // Skip trivia at the start of each loop iteration
            match self.peek_raw() { // Use peek_raw after skipping trivia
                Some(SyntaxKind::R_BRACE) => break, // End of block
                Some(SyntaxKind::PARAMETERS_KW) => self.parse_parameters_block(),
                Some(SyntaxKind::PORTS_KW) => self.parse_ports_block(),
                Some(SyntaxKind::NETS_KW) => self.parse_nets_block(),
                Some(SyntaxKind::COMPONENTS_KW) => self.parse_components_block(),
                Some(SyntaxKind::CONNECTIONS_KW) => self.parse_connections_block(),
                Some(SyntaxKind::LAYER_STACKUP_KW) => self.parse_layer_stackup_block(), // Assuming function exists
                Some(SyntaxKind::DEFAULT_DESIGN_RULES_KW) => self.parse_default_design_rules_block(), // Assuming function exists
                Some(SyntaxKind::CONSTRAIN_KW) => self.parse_constrain_block(), // Added case for constrain
                // Some(SyntaxKind::PINS_KW) => self.parse_pins_block(), // Pins are not directly in board
                Some(kind) => {
                    self.error(format!("Unexpected token inside board definition: {:?}. Expected block keyword (parameters, ports, nets, components, connections, etc.) or '}}'.", kind));
                    self.bump_any(); // Consume unexpected token
                }
                None => { // Reached EOF unexpectedly
                    self.error("Unexpected end of file inside board definition block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_parameters_block(&mut self) {
        self.builder.start_node(SyntaxKind::PARAMETERS_BLOCK.into());
        self.expect(SyntaxKind::PARAMETERS_KW);
        self.expect(SyntaxKind::L_BRACE);
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::IDENT => self.parse_param_assign(),
                _ => {
                    self.error(format!("Expected parameter assignment (identifier) or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_param_assign(&mut self) {
         self.builder.start_node(SyntaxKind::PARAM_ASSIGN.into());
         self.expect(SyntaxKind::IDENT); // Name (e.g., value, tolerance)
         // Optional type annotation
         if self.eat(SyntaxKind::COLON) {
             self.parse_type_ref(); // TODO: Define and implement parse_type_ref properly if needed here
         }
         self.expect(SyntaxKind::EQ);    // =

         // Special check for type keywords in assignment (common in typedef)
         match self.peek() {
            Some(SyntaxKind::SIGNAL_KW) |
            Some(SyntaxKind::POWER_KW) |
            Some(SyntaxKind::GROUND_KW) => {
                // Consume the keyword directly as the value
                // We might wrap this in a VALUE node for consistency?
                self.builder.start_node(SyntaxKind::VALUE.into());
                self.bump(); 
                self.builder.finish_node();
            }
            _ => {
                 // Otherwise, parse a regular value
                 self.parse_value();
            }
         }

         self.expect(SyntaxKind::SEMI);
         self.builder.finish_node();
    }

    fn parse_ports_block(&mut self) {
        self.builder.start_node(SyntaxKind::PORTS_BLOCK.into());
        self.expect(SyntaxKind::PORTS_KW);
        self.expect(SyntaxKind::L_BRACE);

        // Parse port declarations
        loop {
            self.skip_trivia();
            match self.peek_raw() {
                Some(SyntaxKind::R_BRACE) => break, // End of block
                Some(SyntaxKind::IDENT) => {
                    // Looks like the start of a port declaration
                    self.parse_port_decl();
                }
                Some(kind) => {
                    self.error(format!("Expected port declaration (identifier) or '}}', found {:?}", kind));
                    self.bump_any(); // Consume unexpected token to allow recovery
                }
                None => {
                    self.error("Unexpected end of file inside ports block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_port_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PORT_DECL.into());
        self.expect(SyntaxKind::IDENT); // Port name
        // Optional bus suffix
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }
        self.expect(SyntaxKind::COLON);

        // Direction is *required* for ports according to spec 2.4.3 / 3.1 examples
        match self.peek() {
            Some(SyntaxKind::IN_KW) | Some(SyntaxKind::OUT_KW) | Some(SyntaxKind::INOUT_KW) => {
                self.bump(); // Consume direction keyword
            }
            _ => {
                // Report error but proceed, hoping to find type
                self.error("Expected port direction (in, out, inout)".to_string());
            }
        }

        // Parse type reference
        self.parse_type_ref(); // E.g., signal, power, ground, or custom type like cmos_3v3

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    fn parse_nets_block(&mut self) {
        self.builder.start_node(SyntaxKind::NETS_BLOCK.into());
        self.expect(SyntaxKind::NETS_KW);
        self.expect(SyntaxKind::L_BRACE);

        // Parse net declarations
        loop {
            self.skip_trivia();
            match self.peek_raw() {
                Some(SyntaxKind::R_BRACE) => break, // End of block
                Some(SyntaxKind::NET_KW) => {
                    // Consume NET_KW and parse the rest of the declaration
                    self.parse_net_decl(); // parse_net_decl now consumes NET_KW
                }
                Some(kind) => {
                     self.error(format!("Expected 'net' keyword or '}}', found {:?}", kind));
                     self.bump_any(); // Consume unexpected token
                }
                None => {
                    self.error("Unexpected end of file inside nets block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_net_decl(&mut self) {
         self.builder.start_node(SyntaxKind::NET_DECL.into());
         // Expect NET_KW at the beginning of the declaration
         self.expect(SyntaxKind::NET_KW);

         // Parse IDENT [BUS_SUFFIX]? COLON TYPE_REF [= EXPRESSION]? SEMI
         if self.eat(SyntaxKind::IDENT) {
             // Optional bus suffix
             if self.peek() == Some(SyntaxKind::L_BRACKET) {
                 self.parse_bus_suffix();
             }
         } else {
             self.error("Expected net name (identifier) after 'net' keyword".to_string());
             // Recovery: finish node and return?
             self.builder.finish_node();
             return;
         }

         self.expect(SyntaxKind::COLON);
         self.parse_type_ref();

         // Optional default assignment
         if self.eat(SyntaxKind::EQ) {
             self.parse_expression();
         }
         self.expect(SyntaxKind::SEMI);
         self.builder.finish_node();
    }

    fn parse_type_ref(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_REF.into());
        // Allow keywords or identifiers as the base type name
        match self.current() {
            Some(SyntaxKind::IDENT) | Some(SyntaxKind::SIGNAL_KW) |
            Some(SyntaxKind::POWER_KW) | Some(SyntaxKind::GROUND_KW) |
            Some(SyntaxKind::CLOCK_KW) | Some(SyntaxKind::WIRE_KW) |
            Some(SyntaxKind::TRI_KW) | Some(SyntaxKind::TRIREG_KW) |
            Some(SyntaxKind::UWIRE_KW) => {
                self.bump(); // Consume the identifier or keyword
            }
            Some(kind) => {
                self.error(format!("Expected type name (identifier or keyword), found {:?}", kind));
                // Consume the unexpected token to attempt recovery
                self.bump();
            }
            None => {
                 self.error("Expected type name, found end of file".to_string());
            }
        }
        // Handle optional parameterized types like signal(foo)
        if self.eat(SyntaxKind::L_PAREN) {
            self.builder.start_node(SyntaxKind::TYPE_PARAMS.into());
            // Simple parse: Expect an IDENT or value, then R_PAREN for now
            // TODO: Allow comma-separated list, complex expressions etc.
            match self.current() {
                Some(SyntaxKind::IDENT) |
                Some(SyntaxKind::NUMBER) |
                Some(SyntaxKind::STRING) => {
                    // Might be an identifier ref or a literal value
                    // For now, just consume it. We could use parse_value() if it becomes complex.
                    self.bump();
                }
                _ => {
                     self.error("Expected identifier or literal value inside type parameters".to_string());
                     // Attempt to consume the unexpected token to potentially find R_PAREN
                     if self.current() != Some(SyntaxKind::R_PAREN) && self.current().is_some() {
                         self.bump_any();
                     }
                }
            }
            self.expect(SyntaxKind::R_PAREN);
            self.builder.finish_node(); // Finish TYPE_PARAMS
        }

        // TODO: Handle bus suffixes like [7:0] if not handled in parent rule (e.g., in pin/net decl)
        self.builder.finish_node();
    }

    // Parses a simple value (NUMBER, NUMBER.NUMBER, STRING literal, bool, optionally followed by a unit IDENT)
    // *** Removed general keyword handling - handled contextually in parse_param_assign ***
    fn parse_value(&mut self) {
        self.builder.start_node(SyntaxKind::VALUE.into());
        // Use `eat` to attempt consumption and simplify logic
        if self.eat(SyntaxKind::NUMBER) {
            // Check for optional decimal part
            if self.eat(SyntaxKind::DOT) {
                self.expect(SyntaxKind::NUMBER); // Expect number after dot
            }
            // Check for optional unit identifier after the number (or decimal)
            if self.current() == Some(SyntaxKind::IDENT) {
                // TODO: Validate if IDENT is a known unit?
                self.bump(); // Consume the unit identifier
            }
        } else if self.eat(SyntaxKind::STRING) {
            // String literal, nothing more to expect
        } else if self.eat(SyntaxKind::TRUE_KW) || self.eat(SyntaxKind::FALSE_KW) {
            // Boolean keywords
        } else if self.eat(SyntaxKind::IDENT) {
             // Allow plain identifiers as values (e.g., enum refs, parameter refs)
        } else {
            self.error(format!(
                "Expected a value (literal, boolean, or identifier), found {:?}",
                self.current()
            ));
            // Don't bump here, let expect() in the calling function handle recovery.
        }
        self.builder.finish_node();
    }

    // ADD Placeholder for expression parsing
    fn parse_expression(&mut self) {
        self.builder.start_node(SyntaxKind::EXPRESSION.into());
        // Very basic: consume one number or identifier
        if self.current() == Some(SyntaxKind::NUMBER) || self.current() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected number or identifier for expression".to_string());
            // Consume unexpected token
             if self.current().is_some() { self.bump(); }
        }
        self.builder.finish_node();
    }

    // --- New Parsing Functions ---

    fn parse_components_block(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENTS_BLOCK.into());
        self.expect(SyntaxKind::COMPONENTS_KW);
        self.expect(SyntaxKind::L_BRACE);

        // Parse component instantiations
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::IDENT => self.parse_component_inst(), // Instantiation starts with Type IDENT
                _ => {
                    self.error(format!("Expected component instantiation (identifier) or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_component_inst(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENT_INST.into());
        self.expect(SyntaxKind::IDENT); // Component Type
        self.expect(SyntaxKind::IDENT); // Instance Name
        self.expect(SyntaxKind::L_BRACE);
        self.parse_component_params(); // Parse parameters inside {}
        self.expect(SyntaxKind::R_BRACE);
        // Note: Semicolon is not specified after component instantiation in the spec examples
        self.builder.finish_node();
    }

    // Parses parameters within component instantiation braces {}
    fn parse_component_params(&mut self) {
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::IDENT => self.parse_param_assign(), // Reuse param assign logic
                _ => {
                    self.error(format!("Expected parameter assignment (identifier) or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }
    }

    fn parse_connections_block(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTIONS_BLOCK.into());
        self.expect(SyntaxKind::CONNECTIONS_KW);
        self.expect(SyntaxKind::L_BRACE);

        // Parse connection statements
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::IDENT => self.parse_connection_stmt(), // Connection starts with an identifier (Pin or Net ref)
                _ => {
                    self.error(format!("Expected connection statement (identifier) or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_connection_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());

        // Parse LHS (one or more refs)
        self.parse_pin_or_net_ref(); 
        while self.peek() == Some(SyntaxKind::COMMA) {
            self.bump(); // Consume comma
            self.parse_pin_or_net_ref();
        }

        self.expect(SyntaxKind::ARROW);

        // Parse RHS (one or more refs)
        self.parse_pin_or_net_ref();
        while self.peek() == Some(SyntaxKind::COMMA) {
            self.bump(); // Consume comma
            self.parse_pin_or_net_ref();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parses an identifier, possibly followed by .identifier/.number and/or [high:low]
    fn parse_pin_or_net_ref(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_REF.into());
        self.expect(SyntaxKind::IDENT); // First identifier (Net name or Component instance)

        // Check for optional second part (Pin name or number)
        if self.peek() == Some(SyntaxKind::DOT) {
            self.bump(); // Consume the dot
            match self.peek() {
                Some(SyntaxKind::IDENT) | Some(SyntaxKind::NUMBER) => {
                    self.bump(); // Consume IDENT or NUMBER (Pin name/number)
                }
                _ => {
                    self.error("Expected pin identifier or number after dot".to_string());
                }
            }
            // Check for optional bus suffix *after* the pin identifier/number
            if self.peek() == Some(SyntaxKind::L_BRACKET) {
                self.parse_bus_suffix();
            }
        } else {
            // If there was no DOT, check for bus suffix on the initial IDENT (for net refs)
            if self.peek() == Some(SyntaxKind::L_BRACKET) {
                 self.parse_bus_suffix();
            }
        }

        self.builder.finish_node();
    }

    // Parses [N] or [N:M]
    fn parse_bus_suffix(&mut self) {
        if self.current() != Some(SyntaxKind::L_BRACKET) {
            self.error("Internal error: parse_bus_suffix called without L_BRACKET".to_string());
            return;
        }
        self.builder.start_node(SyntaxKind::BUS_SUFFIX.into());
        self.bump(); // Consume '['

        self.expect(SyntaxKind::NUMBER); // Expect the first number (high index or single index)

        // Check for optional colon and second number (for range)
        if self.eat(SyntaxKind::COLON) { // Use eat() for optional colon
            self.expect(SyntaxKind::NUMBER); // Low number
        }
        // Single index case: If no colon was eaten, we just expect R_BRACKET

        self.expect(SyntaxKind::R_BRACKET);
        self.builder.finish_node();
    }

    // --- Parser Helper Methods ---

    // Skips trivia tokens (whitespace, comments) - directly modifies pos
    fn skip_trivia(&mut self) {
        while let Some(kind) = self.peek_raw() {
             if kind == SyntaxKind::WHITESPACE || kind == SyntaxKind::COMMENT {
                 self.pos += 1;
             } else {
                 break;
             }
        }
    }

    // Peek at the next non-trivia token kind
    fn peek(&self) -> Option<SyntaxKind> {
        self.peek_n(0) // Call peek_n directly
    }

    // Peek at the raw next token kind (including trivia)
    fn peek_raw(&self) -> Option<SyntaxKind> {
        self.tokens.get(self.pos).map(|(kind, _)| *kind)
    }

    // Peek n tokens ahead (raw, including trivia)
    fn peek_n_raw(&self, n: usize) -> Option<SyntaxKind> {
        self.tokens.get(self.pos + n).map(|(kind, _)| *kind)
    }

    // Peek n non-trivia tokens ahead
    fn peek_n(&self, n: usize) -> Option<SyntaxKind> {
        self.peek_n_raw(n)
    }

    // Consume the current token (including trivia) and add it to the builder
    fn bump_any(&mut self) {
        if self.pos < self.tokens.len() {
            let (kind, text) = self.tokens[self.pos].clone();
            self.builder.token(kind.into(), &text);
            self.pos += 1;
        }
    }

    // Expect a specific non-trivia token kind, consume it, or report an error
    fn expect(&mut self, expected: SyntaxKind) {
         self.skip_trivia(); // Skip trivia before expecting
         match self.peek_raw() { // Use peek_raw to check without skipping again
            Some(kind) if kind == expected => {
                self.bump(); // Consumes the token and adds to builder
            }
            Some(kind) => {
                 self.error(format!("Expected {:?}, found {:?}", expected, kind));
                 // TODO: Improve error recovery? Maybe insert expected token?
            }
            None => {
                self.error(format!("Expected {:?}, found end of file", expected));
            }
         }
    }

    // Report a parsing error
    fn error(&mut self, message: String) {
        self.errors.push(ParseError { message });
        // TODO: Add span information
        // TODO: Potentially add an ERROR node to the CST?
        // Example: self.builder.start_node(SyntaxKind::ERROR.into()); ... self.builder.finish_node();
    }

    // --- New Module Parsing ---

    fn parse_module_def(&mut self) {
        self.builder.start_node(SyntaxKind::MODULE_DEF.into());
        self.expect(SyntaxKind::MODULE_KW);
        self.expect(SyntaxKind::IDENT); // Module Name
        self.expect(SyntaxKind::L_BRACE);

        // Parse items inside the module block
        loop {
            self.skip_trivia(); // Skip trivia at the start of each loop iteration
            match self.peek_raw() { // Use peek_raw after skipping trivia
                Some(SyntaxKind::R_BRACE) => break, // End of block
                Some(SyntaxKind::PARAMETERS_KW) => self.parse_parameters_block(),
                Some(SyntaxKind::PORTS_KW) => self.parse_ports_block(),
                Some(SyntaxKind::NETS_KW) => self.parse_nets_block(),
                Some(SyntaxKind::COMPONENTS_KW) => self.parse_components_block(),
                Some(SyntaxKind::CONNECTIONS_KW) => self.parse_connections_block(),
                Some(SyntaxKind::CONSTRAIN_KW) => self.parse_constrain_block(), // Added case for constrain
                // Add other valid blocks inside modules if needed
                Some(kind) => {
                    self.error(format!("Unexpected token inside module definition: {:?}. Expected block keyword (parameters, ports, nets, etc.) or '}}'.", kind));
                    self.bump_any(); // Consume unexpected token
                }
                 None => { // Reached EOF unexpectedly
                    self.error("Unexpected end of file inside module definition block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_typedef_def(&mut self) {
        self.builder.start_node(SyntaxKind::TYPEDEF_DEF.into());
        self.expect(SyntaxKind::TYPEDEF_KW);
        self.expect(SyntaxKind::IDENT); // Type Name
        self.expect(SyntaxKind::L_BRACE);

        // Parse parameter assignments inside the braces
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::IDENT => self.parse_param_assign(), // Reuse param assign logic
                _ => {
                    self.error(format!("Expected parameter assignment (identifier = value) or '}}' in typedef, found {:?}", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node(); // No semicolon after typedef
    }

    // --- Import Parsing ---

    fn parse_import_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_STMT.into());
        self.expect(SyntaxKind::IMPORT_KW);
        self.parse_import_path_and_target(); // Renamed for clarity
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parses the full path including the final target (IDENT or {group})
    fn parse_import_path_and_target(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_PATH.into());
        self.expect(SyntaxKind::IDENT); // First identifier

        while self.peek() == Some(SyntaxKind::DOT) {
            self.bump(); // Consume DOT

            match self.peek() {
                Some(SyntaxKind::IDENT) => {
                    self.bump(); // Consume IDENT (part of path for now)
                    // If the *next* token is not DOT or SEMI, this IDENT might be the start of the target group
                    // But let's handle the simple case first. If it's L_BRACE next, it *must* be a group.
                }
                Some(SyntaxKind::L_BRACE) => {
                    self.builder.finish_node(); // Finish IMPORT_PATH node
                    self.parse_import_target_group(); // Parse the { group } which is the target
                    return; // Successfully parsed path and group target
                }
                _ => {
                    self.error("Expected identifier or group import brace after dot".to_string());
                    // Attempt to finish the path node gracefully before returning
                    // Find the next semicolon if possible to recover?
                    self.builder.finish_node(); 
                    return;
                }
            }
        }

        // If we exit the loop here, it means the path ended with an IDENT
        // or was just a single IDENT. The last IDENT consumed is the target.
        self.builder.finish_node(); // Finish IMPORT_PATH
        // Create an empty IMPORT_TARGET node because the target IDENT was consumed by the path.
        self.builder.start_node(SyntaxKind::IMPORT_TARGET.into()); 
        self.builder.finish_node();
    }

    // Parses { TargetA, TargetB, ... }
    fn parse_import_target_group(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_TARGET_GROUP.into());
        self.expect(SyntaxKind::L_BRACE);
        let mut expect_comma = false; // Flag to track if comma is expected
        while let Some(kind) = self.peek() {
            if kind == SyntaxKind::R_BRACE { break; }

            if expect_comma {
                if kind == SyntaxKind::COMMA {
                    self.bump(); // Consume comma
                    expect_comma = false;
                    // Handle trailing comma
                    if self.peek() == Some(SyntaxKind::R_BRACE) { break; }
                } else {
                    self.error("Expected comma or '}' after item in import group".to_string());
                    break; // Avoid infinite loop on error
                }
            } else {
                if kind == SyntaxKind::IDENT {
                    // Could wrap each IDENT in an IMPORT_ITEM node if needed
                    self.bump(); 
                    expect_comma = true; // After an item, expect a comma or brace
                } else {
                     self.error(format!("Expected identifier or '}}' in import group, found {:?}", kind));
                     break; // Avoid infinite loop on error
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // --- Component Definition Parsing ---

    fn parse_component_def(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENT_DEF.into());
        self.expect(SyntaxKind::COMPONENT_KW);
        self.expect(SyntaxKind::IDENT); // Component Name
        self.expect(SyntaxKind::L_BRACE);

        // Parse parameters, pins, interfaces blocks inside component
        loop {
            self.skip_trivia();
            match self.peek_raw() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PARAMETERS_KW) => self.parse_parameters_block(),
                Some(SyntaxKind::PINS_KW) => self.parse_pins_block(),
                Some(SyntaxKind::INTERFACES_KW) => self.parse_interfaces_block(),
                Some(SyntaxKind::CONSTRAIN_KW) => self.parse_constrain_block(), // Added case for constrain
                // Add other valid blocks if needed (e.g., footprint, package?)
                Some(kind) => {
                    self.error(format!("Expected parameters, pins, interfaces, or '}}' in component definition, found {:?}", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside component definition block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // --- Interfaces Block Parsing (within Component Def) ---

    fn parse_interfaces_block(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACES_BLOCK.into());
        self.expect(SyntaxKind::INTERFACES_KW);
        self.expect(SyntaxKind::L_BRACE);
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                 self.parse_interface_instance();
            } else {
                 self.error(format!("Expected interface instance (identifier) or '}}', found {:?}", self.peek()));
                 self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses: InstanceName: interface TypeName { pin_map = {...}, other_params... };
    fn parse_interface_instance(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_INSTANCE.into());
        self.expect(SyntaxKind::IDENT); // Instance Name
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::INTERFACE_KW);
        self.expect(SyntaxKind::IDENT); // Interface Type Name
        
        // Parse optional parameters block { ... }
        self.expect(SyntaxKind::L_BRACE);
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            match self.peek() {
                Some(SyntaxKind::IDENT) => {
                    // Check if it's pin_map or regular param assign
                    // This requires peeking ahead for EQ
                    // For now, assume any IDENT = VALUE is a param assign
                    // A more robust approach might check if IDENT is specifically 'pin_map'
                    self.parse_param_assign(); // Reuse for now, assuming pin_map looks like param_assign
                }
                _ => {
                    self.error(format!("Expected parameter assignment or pin_map inside interface instance, found {:?}", self.peek()));
                    self.bump_any();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        // Spec is unclear if semicolon is needed here, assuming NO based on examples
        // self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Placeholder - TODO: Refine to specifically parse `pin_map = { Log = Phys, ... }`
    // Currently reusing parse_param_assign which might be incorrect structure.
    fn parse_pin_map_block(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_MAP_BLOCK.into());
        self.expect(SyntaxKind::IDENT); // Expect 'pin_map' identifier
        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::L_BRACE);
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_pin_map_entry();
            self.eat(SyntaxKind::COMMA); // Consume optional comma
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Placeholder - Parses `LogicalPin = PhysicalPin`
    fn parse_pin_map_entry(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_MAP_ENTRY.into());
        self.expect(SyntaxKind::IDENT); // Logical Pin Name
        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::IDENT); // Physical Pin Name
        self.builder.finish_node();
    }

    fn parse_pins_block(&mut self) {
        self.builder.start_node(SyntaxKind::PINS_BLOCK.into());
        self.expect(SyntaxKind::PINS_KW);
        self.expect(SyntaxKind::L_BRACE);

        // Parse pin declarations or generate blocks
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::IDENT | SyntaxKind::NUMBER => self.parse_pin_decl(), // Pin decl starts with name (IDENT or NUMBER)
                SyntaxKind::GENERATE_KW => self.parse_generate_for_pins(), // Handle generate for blocks
                _ => {
                    self.error(format!("Expected pin declaration (identifier or number), generate, or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses a pin declaration like: Name[bus]: direction type(spec), properties... ; or Name: direction type;
    fn parse_pin_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_DECL.into());
        // Pin name (IDENT or NUMBER)
        if self.eat(SyntaxKind::IDENT) || self.eat(SyntaxKind::NUMBER) {
            // Name consumed successfully
        } else {
            self.error("Expected pin name (identifier or number)".to_string());
            // Attempt recovery? Bumping might misalign things badly.
            // Let's try finishing the node here and returning to let the outer loop handle recovery.
            self.builder.finish_node(); // Finish potentially empty/incomplete node
            return;
        }

        // Optional bus suffix
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }
        
        self.expect(SyntaxKind::COLON);

        // Optional direction (Unlike ports, direction can be omitted for ground/passive)
        let _has_direction = self.eat(SyntaxKind::IN_KW) || self.eat(SyntaxKind::OUT_KW) || self.eat(SyntaxKind::INOUT_KW);

        // Parse type reference (This must succeed after colon and optional direction)
        // We need to ensure parse_type_ref correctly handles the next token
        // if _has_direction was false and the token is a valid type.
        self.parse_type_ref(); // This might error internally if no valid type is found

        // Optional pin properties
        if self.eat(SyntaxKind::COMMA) { // Use eat for the optional comma
            self.parse_pin_properties(); // This function needs implementation
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parses optional pin properties like 'functions = [...]' after a comma
    fn parse_pin_properties(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_PROPERTIES.into());
        // Expect 'functions' keyword for now, potentially others later
        if self.current() == Some(SyntaxKind::FUNCTIONS_KW) { // Check for FUNCTIONS_KW specifically
            self.bump(); // Consume 'functions'
            self.expect(SyntaxKind::EQ);
            // Expect a list/array structure, parse as simple value for now
            self.expect(SyntaxKind::L_BRACKET); // Expect '['
            while self.current() != Some(SyntaxKind::R_BRACKET) && self.current().is_some() {
                 if self.current() == Some(SyntaxKind::STRING) {
                     self.bump();
                     self.eat(SyntaxKind::COMMA);
                 } else {
                     self.error("Expected string literal inside functions list".to_string());
                     self.bump_any(); // Consume unexpected token
                 }
            }
            self.expect(SyntaxKind::R_BRACKET); // Expect ']'
        } else {
             self.error(format!("Expected pin property assignment (e.g., functions = ...), found {:?}", self.current()));
             // Consume the unexpected token if present
             if self.current().is_some() { self.bump_any(); }
        }
        self.builder.finish_node();
    }

    // Parses: generate for <var> in <range> { <pin_decl>... }
    fn parse_generate_for_pins(&mut self) {
        self.builder.start_node(SyntaxKind::GENERATE_FOR_BLOCK.into());
        self.expect(SyntaxKind::GENERATE_KW);
        self.expect(SyntaxKind::FOR_KW);
        self.expect(SyntaxKind::IDENT); // Loop variable
        self.expect(SyntaxKind::IN_KW);
        self.parse_range_expr(); // Parse the range (e.g., 0 to data_width-1)
        self.expect(SyntaxKind::L_BRACE);

        // Parse pin declarations inside the generate block
        loop {
            self.skip_trivia();
            match self.peek_raw() {
                Some(SyntaxKind::R_BRACE) => break,
                // Pin names can be IDENT or NUMBER
                Some(SyntaxKind::IDENT) | Some(SyntaxKind::NUMBER) => {
                    self.parse_pin_decl();
                }
                Some(kind) => {
                    self.error(format!("Expected pin declaration (identifier or number) or '}}' in generate for block, found {:?}", kind));
                    self.bump_any(); // Consume unexpected token
                }
                None => {
                    self.error("Unexpected end of file inside generate for block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Placeholder for range expressions like '0 to WIDTH-1'
    // Simplified: Parses 'NUMBER to NUMBER' for now
    fn parse_range_expr(&mut self) {
        self.builder.start_node(SyntaxKind::RANGE_EXPR.into());
        self.expect(SyntaxKind::NUMBER); // Start of range
        self.expect(SyntaxKind::IDENT); // Expect 'to' (lexed as IDENT for now)
        // TODO: Check if the ident text is actually "to"?
        self.expect(SyntaxKind::NUMBER); // End of range
        // TODO: Allow identifiers/expressions later
        self.builder.finish_node();
    }

    fn parse_interface_def(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_DEF.into());
        self.expect(SyntaxKind::INTERFACE_KW);
        self.expect(SyntaxKind::IDENT); // Interface Name
        // TODO: Parse optional interface parameters (...) later?
        self.expect(SyntaxKind::L_BRACE);

        // Parse items inside the interface block
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break, // End of block
                SyntaxKind::PARAMETERS_KW => self.parse_parameters_block(), // Reuse
                SyntaxKind::PINS_KW => self.parse_pins_block(), // Reuse
                // Add INTERFACES_KW? Spec isn't clear if interfaces can contain other interfaces directly.
                _ => {
                    self.error(format!("Unexpected token inside interface definition: {:?}. Expected parameters, pins, or '}}'.", kind));
                    self.bump_any(); // Consume unexpected token
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node(); // No semicolon after interface def
    }

    // ADD BACK the eat method
    fn eat(&mut self, expected: SyntaxKind) -> bool {
        if self.current() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    // Add stubs for the new block parsers called from parse_board_def

    fn parse_layer_stackup_block(&mut self) {
        self.builder.start_node(SyntaxKind::LAYER_STACKUP_BLOCK.into());
        self.expect(SyntaxKind::LAYER_STACKUP_KW);
        self.expect(SyntaxKind::L_BRACE);
        // TODO: Parse layer definitions
        while self.current() != Some(SyntaxKind::R_BRACE) && self.current().is_some() {
             self.error("Layer stackup parsing not yet implemented".to_string());
             self.bump_any(); // Consume tokens until R_BRACE for now
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_default_design_rules_block(&mut self) {
        self.builder.start_node(SyntaxKind::DEFAULT_DESIGN_RULES_BLOCK.into());
        self.expect(SyntaxKind::DEFAULT_DESIGN_RULES_KW);
        self.expect(SyntaxKind::L_BRACE);
        // TODO: Parse design rule assignments
        while self.current() != Some(SyntaxKind::R_BRACE) && self.current().is_some() {
            // Reuse param_assign logic for now, as it's 'ident = value;'
             if self.peek() == Some(SyntaxKind::IDENT) {
                 self.parse_param_assign();
             } else {
                 self.error("Expected design rule assignment (identifier = value)".to_string());
                 self.bump_any();
             }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_constrain_block(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRAIN_BLOCK.into());
        self.expect(SyntaxKind::CONSTRAIN_KW);
        // TODO: Parse constrain target (net, pin, connection etc.)
        self.expect(SyntaxKind::L_PAREN); // Assuming target is in () for now
        while self.current() != Some(SyntaxKind::R_PAREN) && self.current().is_some() {
            self.bump_any(); // Consume target for now
        }
        self.expect(SyntaxKind::R_PAREN);
        self.expect(SyntaxKind::L_BRACE);
        // TODO: Parse constraint assignments
        while self.current() != Some(SyntaxKind::R_BRACE) && self.current().is_some() {
            // Reuse param_assign logic for now
             if self.peek() == Some(SyntaxKind::IDENT) {
                 self.parse_param_assign();
             } else {
                 self.error("Expected constraint assignment (identifier = value)".to_string());
                 self.bump_any();
             }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
}

// Top-level parse function
pub fn parse(text: &str) -> ParseResult {
    let mapped_tokens = map_token_stream(text);
    // Call new with only the mapped tokens slice
    let mut parser = Parser::new(&mapped_tokens);

    parser.parse_source_file(); // Start parsing from the top level

    ParseResult {
        green_node: parser.builder.finish(),
        errors: parser.errors,
    }
}

// Helper to check if a SyntaxKind is a keyword (adjust as needed)
// Placed outside the Parser impl block
impl SyntaxKind {
    fn is_keyword(self) -> bool {
        matches!(self,
            SyntaxKind::IMPORT_KW | SyntaxKind::BOARD_KW | SyntaxKind::MODULE_KW |
            SyntaxKind::COMPONENT_KW | SyntaxKind::TYPEDEF_KW | SyntaxKind::INTERFACE_KW |
            SyntaxKind::PARAMETERS_KW | SyntaxKind::PORTS_KW | SyntaxKind::COMPONENTS_KW |
            SyntaxKind::NETS_KW | SyntaxKind::CONNECTIONS_KW | SyntaxKind::PINS_KW |
            SyntaxKind::INTERFACES_KW | SyntaxKind::NET_KW | SyntaxKind::LAYER_STACKUP_KW |
            SyntaxKind::LAYER_KW | SyntaxKind::DEFAULT_DESIGN_RULES_KW | SyntaxKind::CONSTRAIN_KW |
            SyntaxKind::GENERATE_KW | SyntaxKind::FOR_KW | SyntaxKind::IN_KW |
            SyntaxKind::OUT_KW | SyntaxKind::INOUT_KW | SyntaxKind::SIGNAL_KW |
            SyntaxKind::POWER_KW | SyntaxKind::GROUND_KW | SyntaxKind::TRUE_KW |
            SyntaxKind::FALSE_KW
            // Add any other keywords here if they are introduced
        )
    }
}

// Basic test
#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::{BhdlLanguage, SyntaxKind};
    use rowan::SyntaxNode;
    use crate::syntax::SyntaxKind::*;

    // Helper to find the first node of a specific kind
    fn find_node(root: &SyntaxNode<BhdlLanguage>, kind: SyntaxKind) -> Option<SyntaxNode<BhdlLanguage>> {
        root.descendants().find(|n| n.kind() == kind)
    }

    // Helper to find all nodes of a specific kind - moved outside test function
    fn find_all_nodes(root: &SyntaxNode<BhdlLanguage>, kind: SyntaxKind) -> Vec<SyntaxNode<BhdlLanguage>> {
        root.descendants().filter(|n| n.kind() == kind).collect()
    }

    #[test]
    fn parse_empty_file() {
        let result = parse("");
        assert!(result.errors.is_empty());
        let root = result.syntax();
        assert_eq!(root.kind(), SyntaxKind::SOURCE_FILE);
        assert_eq!(root.children().count(), 0);
        assert_eq!(root.children_with_tokens().count(), 0);
    }

    #[test]
    fn parse_minimal_board_def() {
        let result = parse("board Foo { }");
        println!("Parse errors: {:?}", result.errors); // Debug print errors
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let root = result.syntax();
        assert_eq!(root.kind(), SyntaxKind::SOURCE_FILE);

        let board_def_nodes: Vec<_> = root.children().filter(|n| n.kind() == SyntaxKind::BOARD_DEF).collect();
        assert_eq!(board_def_nodes.len(), 1, "SOURCE_FILE should contain exactly one BOARD_DEF");
        let board_def = board_def_nodes.first().unwrap();

        // Check children tokens of BOARD_DEF
        let mut children = board_def.children_with_tokens();

        // Note: Due to trivia filtering, we expect non-trivia tokens directly.
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(SyntaxKind::BOARD_KW));
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(SyntaxKind::IDENT));
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(SyntaxKind::L_BRACE));
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(SyntaxKind::R_BRACE));
        assert!(children.next().is_none(), "Should be no more children after R_BRACE");
    }

    #[test]
    fn parse_board_with_junk() {
        let result = parse("board Foo { junk }");
        assert!(!result.errors.is_empty()); // Expect errors
        // Check if the structure is somewhat reasonable despite errors
        let root = result.syntax();
        assert_eq!(root.kind(), SyntaxKind::SOURCE_FILE);
        let board_def = root.children().find(|n| n.kind() == SyntaxKind::BOARD_DEF);
        assert!(board_def.is_some());
        let board_def = board_def.unwrap();

        // Find tokens, ignoring potential ERROR nodes for simplicity here
        assert!(board_def.children_with_tokens().any(|t| t.kind() == SyntaxKind::BOARD_KW));
        assert!(board_def.children_with_tokens().any(|t| t.kind() == IDENT && t.as_token().map(|tok| tok.text()) == Some(&SmolStr::new("Foo")) ));
        assert!(board_def.children_with_tokens().any(|t| t.kind() == L_BRACE));
        assert!(board_def.children_with_tokens().any(|t| t.kind() == R_BRACE));
        // Check that 'junk' was consumed (might be IDENT or ERROR_TOKEN)
        // Since 'junk' is a valid identifier according to the lexer rule,
        // and our parser expects specific keywords or '}' inside the board,
        // 'junk' will be lexed as IDENT and cause a parser error "Unexpected token..."
        assert!(board_def.children_with_tokens().any(|t| t.kind() == IDENT && t.as_token().map(|tok| tok.text()) == Some(&SmolStr::new("junk"))));
    }

     #[test]
    fn parse_multiple_boards() { // Test multiple top-level items
        let result = parse("board Foo {} board Bar {}");
        assert!(result.errors.is_empty());
        let root = result.syntax();
        assert_eq!(root.kind(), SOURCE_FILE);
        assert_eq!(root.children().filter(|n| n.kind() == BOARD_DEF).count(), 2);
    }

    #[test]
    fn parse_board_with_ports() {
        let input = r#"
            board PortBoard {
                ports {
                    CLK: in signal(system_clock);
                    DATA: out signal;
                    BIDIR: inout cmos_3v3;
                    VBUS: in power(lv_power);
                    GND_PORT: inout ground; // Added direction for ground port
                }
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");
        let board_def = find_node(&result.syntax(), BOARD_DEF).expect("No BOARD_DEF found");
        let ports_block = find_node(&board_def, PORTS_BLOCK).expect("No PORTS_BLOCK found");
        let port_decls = find_all_nodes(&ports_block, PORT_DECL);
        assert_eq!(port_decls.len(), 5);

        // Check CLK port with specifier
        let clk_type_ref = find_node(&port_decls[0], TYPE_REF).expect("No TYPE_REF for CLK");
        assert_eq!(clk_type_ref.first_token().unwrap().kind(), SIGNAL_KW);
        let clk_params = find_node(&clk_type_ref, TYPE_PARAMS).expect("No TYPE_PARAMS for CLK"); // Changed TYPE_SPECIFIER to TYPE_PARAMS
        // Check content of TYPE_PARAMS
        let clk_param_ident = clk_params.children_with_tokens().find(|t| t.kind() == IDENT).expect("No IDENT found in CLK TYPE_PARAMS");
        assert_eq!(clk_param_ident.as_token().unwrap().text(), "system_clock");

        // Check BIDIR port (no specifier, just type IDENT)
        let bidir_type_ref = find_node(&port_decls[2], TYPE_REF).expect("No TYPE_REF for BIDIR");
        assert_eq!(bidir_type_ref.first_token().unwrap().kind(), IDENT);
        assert_eq!(bidir_type_ref.first_token().unwrap().text(), "cmos_3v3");
        assert!(find_node(&bidir_type_ref, TYPE_PARAMS).is_none(), "Should be no TYPE_PARAMS for BIDIR"); // Changed TYPE_SPECIFIER to TYPE_PARAMS

        // Check VBUS port with specifier
        let vbus_type_ref = find_node(&port_decls[3], TYPE_REF).expect("No TYPE_REF for VBUS");
        assert_eq!(vbus_type_ref.first_token().unwrap().kind(), POWER_KW);
        let vbus_params = find_node(&vbus_type_ref, TYPE_PARAMS).expect("No TYPE_PARAMS for VBUS"); // Changed TYPE_SPECIFIER to TYPE_PARAMS
        // Check content of TYPE_PARAMS
        let vbus_param_ident = vbus_params.children_with_tokens().find(|t| t.kind() == IDENT).expect("No IDENT found in VBUS TYPE_PARAMS");
        assert_eq!(vbus_param_ident.as_token().unwrap().text(), "lv_power");
    }

    #[test]
    fn parse_board_with_nets() {
        let input = r#"
            board NetBoard {
                nets {
                    net SPI_MOSI: signal;
                    net VCC_3V3: power;
                    net DataBus[7:0]: signal;
                    net AddrBus[15:0]: custom_bus_type;
                }
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");
        let board_def = find_node(&result.syntax(), BOARD_DEF).expect("No BOARD_DEF found");
        let nets_block = find_node(&board_def, NETS_BLOCK).expect("No NETS_BLOCK found");

        let net_decls = find_all_nodes(&nets_block, NET_DECL);
        assert_eq!(net_decls.len(), 4, "Expected 4 net declarations");

        // Check DataBus declaration
        let data_bus_decl = &net_decls[2];
        let data_bus_ident = data_bus_decl.children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).unwrap();
        assert_eq!(data_bus_ident.text(), "DataBus");
        let data_bus_suffix = find_node(data_bus_decl, BUS_SUFFIX).expect("No BUS_SUFFIX found for DataBus");
        let suffix_tokens: Vec<_> = data_bus_suffix.children_with_tokens().filter_map(|e| e.into_token()).collect();
        assert_eq!(suffix_tokens.len(), 5);
        assert_eq!(suffix_tokens[0].kind(), L_BRACKET);
        assert_eq!(suffix_tokens[1].kind(), NUMBER);
        assert_eq!(suffix_tokens[1].text(), "7");
        assert_eq!(suffix_tokens[2].kind(), COLON);
        assert_eq!(suffix_tokens[3].kind(), NUMBER);
        assert_eq!(suffix_tokens[3].text(), "0");
        assert_eq!(suffix_tokens[4].kind(), R_BRACKET);
        let data_bus_type = find_node(data_bus_decl, TYPE_REF).unwrap().first_token().unwrap();
        assert_eq!(data_bus_type.kind(), SIGNAL_KW);

    }

    #[test]
    fn parse_board_with_components() {
        let input = r#"
            board ComponentBoard {
                components {
                    Resistor R1 { value = 1kOhm; tolerance = 5pct; }
                    Capacitor C1 { value = 10uF; }
                    LED LED1 { }
                }
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let board_def = find_node(&result.syntax(), BOARD_DEF).expect("No BOARD_DEF found");
        let components_block = find_node(&board_def, COMPONENTS_BLOCK).expect("No COMPONENTS_BLOCK found");

        let comp_insts = find_all_nodes(&components_block, COMPONENT_INST);
        assert_eq!(comp_insts.len(), 3, "Expected 3 component instantiations");

        // Check first component instantiation (Resistor R1)
        let r1_inst = &comp_insts[0];
        let mut r1_tokens = r1_inst.children_with_tokens().filter_map(|e| e.into_token());
        assert_eq!(r1_tokens.next().unwrap().text(), "Resistor");
        assert_eq!(r1_tokens.next().unwrap().text(), "R1");
        assert_eq!(r1_tokens.next().unwrap().kind(), L_BRACE);

        // Check parameters inside R1
        let r1_params = find_all_nodes(r1_inst, PARAM_ASSIGN);
        assert_eq!(r1_params.len(), 2, "Expected 2 parameters for R1");
        
        // Parameter 1: value = 1kOhm;
        let p1_ident = r1_params[0].children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).expect("No IDENT found for p1");
        let p1_value_node = find_node(&r1_params[0], VALUE).expect("No VALUE node found for p1");
        let p1_value_text = p1_value_node.text().to_string();
        assert_eq!(p1_ident.text(), "value");
        assert_eq!(p1_value_text, "1kOhm");

        // Parameter 2: tolerance = 5pct;
        let p2_ident = r1_params[1].children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).expect("No IDENT found for p2");
        let p2_value_node = find_node(&r1_params[1], VALUE).expect("No VALUE node found for p2");
        let p2_value_text = p2_value_node.text().to_string();
        assert_eq!(p2_ident.text(), "tolerance");
        assert_eq!(p2_value_text, "5pct");

        // Check last component (LED1) - has no parameters
        let led1_inst = &comp_insts[2];
        let led1_params = find_all_nodes(led1_inst, PARAM_ASSIGN);
        assert_eq!(led1_params.len(), 0, "Expected 0 parameters for LED1");
        let mut led1_tokens = led1_inst.children_with_tokens().filter_map(|e| e.into_token());
        assert_eq!(led1_tokens.next().unwrap().text(), "LED");
        assert_eq!(led1_tokens.next().unwrap().text(), "LED1");
        assert_eq!(led1_tokens.next().unwrap().kind(), L_BRACE);
        assert_eq!(led1_tokens.next().unwrap().kind(), R_BRACE);
        assert!(led1_tokens.next().is_none());
    }

    #[test]
    fn parse_board_with_junk_inside() {
        let result = parse("board Foo { junk }");
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(!result.errors.is_empty());
        let root = result.syntax();
        assert_eq!(root.kind(), SOURCE_FILE);
        let board_def = find_node(&root, BOARD_DEF);
        assert!(board_def.is_some());
        let board_def = board_def.unwrap();
        assert!(board_def.children_with_tokens().any(|t| t.kind() == BOARD_KW));
        assert!(board_def.children_with_tokens().any(|t| t.kind() == IDENT && t.as_token().map(|tok| tok.text()) == Some(&SmolStr::new("Foo")) ));
        assert!(board_def.children_with_tokens().any(|t| t.kind() == L_BRACE));
        assert!(board_def.children_with_tokens().any(|t| t.kind() == R_BRACE));
        // Check that 'junk' was consumed (might be IDENT or ERROR_TOKEN)
        // Since 'junk' is a valid identifier according to the lexer rule,
        // and our parser expects specific keywords or '}' inside the board,
        // 'junk' will be lexed as IDENT and cause a parser error "Unexpected token..."
        assert!(board_def.children_with_tokens().any(|t| t.kind() == IDENT && t.as_token().map(|tok| tok.text()) == Some(&SmolStr::new("junk"))));
    }

    #[test]
    fn parse_board_missing_brace() { /* ... */ }

    #[test]
    fn parse_board_extra_brace() { /* ... */ }

    #[test]
    fn parse_board_with_connections() {
        let input = r#"
            board ConnectionBoard {
                connections {
                    NetA -> U1.Pin1;
                    VCC -> U1.VCC, U2.VCC, C1.1;
                    U1.GND, U2.GND, C1.2 -> GND;
                    // Bus connections
                    CPU.DataBus[7:0] -> RAM.Data[7:0];
                    AddressBus[15:8] -> Periph.Addr[7:0];
                }
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let board_def = find_node(&result.syntax(), BOARD_DEF).expect("No BOARD_DEF found");
        let conns_block = find_node(&board_def, CONNECTIONS_BLOCK).expect("No CONNECTIONS_BLOCK found");

        let conn_stmts = find_all_nodes(&conns_block, CONNECTION_STMT);
        assert_eq!(conn_stmts.len(), 5, "Expected 5 connection statements");

        // Check first statement: NetA -> U1.Pin1;
        let stmt1_refs = find_all_nodes(&conn_stmts[0], PIN_REF);
        assert_eq!(stmt1_refs.len(), 2);
        assert_eq!(stmt1_refs[0].text().to_string(), "NetA");
        assert_eq!(stmt1_refs[1].text().to_string(), "U1.Pin1");
        assert!(conn_stmts[0].children_with_tokens().any(|t| t.kind() == ARROW));
        assert!(conn_stmts[0].children_with_tokens().any(|t| t.kind() == SEMI));

        // Check second statement: VCC -> U1.VCC, U2.VCC, C1.1;
        let stmt2_refs = find_all_nodes(&conn_stmts[1], PIN_REF);
        assert_eq!(stmt2_refs.len(), 4, "Expected 4 refs in stmt 2 (1 LHS, 3 RHS)");
        assert_eq!(stmt2_refs[0].text().to_string(), "VCC");
        assert_eq!(stmt2_refs[1].text().to_string(), "U1.VCC");
        assert_eq!(stmt2_refs[2].text().to_string(), "U2.VCC");
        assert_eq!(stmt2_refs[3].text().to_string(), "C1.1");
        let stmt2_commas = conn_stmts[1].children_with_tokens().filter(|t| t.kind() == COMMA).count();
        assert_eq!(stmt2_commas, 2, "Expected 2 commas in stmt 2");

        // Check third statement: U1.GND, U2.GND, C1.2 -> GND;
        let stmt3_refs = find_all_nodes(&conn_stmts[2], PIN_REF);
        assert_eq!(stmt3_refs.len(), 4, "Expected 4 refs in stmt 3 (3 LHS, 1 RHS)");
        assert_eq!(stmt3_refs[0].text().to_string(), "U1.GND");
        assert_eq!(stmt3_refs[1].text().to_string(), "U2.GND");
        assert_eq!(stmt3_refs[2].text().to_string(), "C1.2");
        assert_eq!(stmt3_refs[3].text().to_string(), "GND");
        let stmt3_commas = conn_stmts[2].children_with_tokens().filter(|t| t.kind() == COMMA).count();
        assert_eq!(stmt3_commas, 2, "Expected 2 commas in stmt 3");

        // Check statement 3 (bus connection)
        let stmt3_refs = find_all_nodes(&conn_stmts[3], PIN_REF);
        assert_eq!(stmt3_refs.len(), 2);
        assert_eq!(stmt3_refs[0].text().to_string(), "CPU.DataBus[7:0]"); // Check full text
        assert_eq!(stmt3_refs[1].text().to_string(), "RAM.Data[7:0]");    // Check full text

        // Check statement 4 (bus slice connection)
        let stmt4_refs = find_all_nodes(&conn_stmts[4], PIN_REF);
        assert_eq!(stmt4_refs.len(), 2);
        assert_eq!(stmt4_refs[0].text().to_string(), "AddressBus[15:8]"); // Check full text
        assert_eq!(stmt4_refs[1].text().to_string(), "Periph.Addr[7:0]"); // Check full text
    }

    #[test]
    fn parse_module_definition() {
        let input = r#"
            module MyModule {
                parameters {
                    gain = 10;
                }
                ports {
                    Input: in signal;
                    Output: out signal;
                }
                // Modules can contain internal components, nets, connections
                components {
                    OpAmp U1 { gain_setting = gain; }
                }
                nets {
                    net Feedback: signal;
                }
                connections {
                    Input -> U1.IN_POS;
                    U1.OUT -> Output;
                    U1.OUT -> Feedback; U1.IN_NEG -> Feedback;
                }
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let source_file = result.syntax();
        let module_def = find_node(&source_file, MODULE_DEF).expect("No MODULE_DEF found");
        assert_eq!(module_def.children_with_tokens().filter_map(|e| e.into_token()).nth(0).unwrap().kind(), MODULE_KW);
        assert_eq!(module_def.children_with_tokens().filter_map(|e| e.into_token()).nth(1).unwrap().text(), "MyModule");

        // Check for presence of internal blocks
        assert!(find_node(&module_def, PARAMETERS_BLOCK).is_some());
        assert!(find_node(&module_def, PORTS_BLOCK).is_some());
        assert!(find_node(&module_def, COMPONENTS_BLOCK).is_some());
        assert!(find_node(&module_def, NETS_BLOCK).is_some());
        assert!(find_node(&module_def, CONNECTIONS_BLOCK).is_some());

        // Basic check on connections block content
        let conns_block = find_node(&module_def, CONNECTIONS_BLOCK).unwrap();
        assert_eq!(find_all_nodes(&conns_block, CONNECTION_STMT).len(), 4);

    }

    #[test]
    fn parse_typedef_definition() {
        let input = r#"
            typedef cmos_3v3 {
                type = signal;
                domain = digital;
                voltage_high = 3.3Vdc;
                voltage_low = 0Vdc;
            }
            // Allow multiple typedefs
            typedef power_rail { type = power; }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let source_file = result.syntax();
        let typedef_defs = find_all_nodes(&source_file, TYPEDEF_DEF);
        assert_eq!(typedef_defs.len(), 2, "Expected 2 typedef definitions");

        // Check first typedef
        let cmos_def = &typedef_defs[0];
        let cmos_ident = cmos_def.children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).unwrap();
        assert_eq!(cmos_ident.text(), "cmos_3v3");
        assert_eq!(find_all_nodes(cmos_def, PARAM_ASSIGN).len(), 4, "Expected 4 param assigns in cmos_3v3");

        // Check second typedef
        let power_def = &typedef_defs[1];
        let power_ident = power_def.children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).unwrap();
        assert_eq!(power_ident.text(), "power_rail");
        assert_eq!(find_all_nodes(power_def, PARAM_ASSIGN).len(), 1, "Expected 1 param assign in power_rail");
        let p1_ident = find_all_nodes(power_def, PARAM_ASSIGN)[0]
            .children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).unwrap();
        let p1_value = find_node(&find_all_nodes(power_def, PARAM_ASSIGN)[0], VALUE)
            .unwrap().first_token().unwrap();
        assert_eq!(p1_ident.text(), "type");
        assert_eq!(p1_value.kind(), POWER_KW);
        assert_eq!(p1_value.text(), "power");
    }

    #[test]
    fn parse_import_statements() {
        let input = r#"
            import Simple.Path.Item;
            import Group.Path.{ItemA, ItemB, ItemC};
            import JustIdent;
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let source_file = result.syntax();
        let import_stmts = find_all_nodes(&source_file, IMPORT_STMT);
        assert_eq!(import_stmts.len(), 3, "Expected 3 import statements");

        // Check first import (Simple.Path.Item)
        let stmt1 = &import_stmts[0];
        let path1 = find_node(stmt1, IMPORT_PATH).expect("No path in stmt1");
        let target1 = find_node(stmt1, IMPORT_TARGET).expect("No target in stmt1");
        assert_eq!(path1.text().to_string(), "Simple.Path.Item"); // Path includes final Item
        assert!(target1.text().is_empty()); // Target node is empty for simple import

        // Check second import (Group.Path.{ItemA, ItemB, ItemC})
        let stmt2 = &import_stmts[1];
        let path2 = find_node(stmt2, IMPORT_PATH).expect("No path in stmt2");
        let target_group2 = find_node(stmt2, IMPORT_TARGET_GROUP).expect("No target group in stmt2");
        assert_eq!(path2.text().to_string(), "Group.Path."); // Path ends before group
        assert_eq!(target_group2.children_with_tokens().filter(|t| t.kind() == IDENT).count(), 3);
        assert_eq!(target_group2.children_with_tokens().filter(|t| t.kind() == COMMA).count(), 2);

        // Check third import (JustIdent)
        let stmt3 = &import_stmts[2];
        let path3 = find_node(stmt3, IMPORT_PATH).expect("No path in stmt3");
        let target3 = find_node(stmt3, IMPORT_TARGET).expect("No target in stmt3");
        assert_eq!(path3.text().to_string(), "JustIdent"); // Path is the ident itself
        assert!(target3.text().is_empty()); // Target node is empty for simple import
    }

    #[test]
    fn parse_component_definition() {
        let input = r#"
            component Resistor {
                pins {
                    p: inout power;
                    n: inout power;
                }
                parameters {
                     resistance = 1k;
                }
            }
            component ComplexIC {
                 pins {
                    VDD: in power(core_power);
                    VSS: ground; 
                    IO[0]: inout signal(lvcmos_1v8);
                 }
            }
        "#;
        let result = parse(input);
        assert!(result.errors.is_empty());
        let root = result.syntax();
        let comp_defs = find_all_nodes(&root, COMPONENT_DEF);
        assert_eq!(comp_defs.len(), 2);

        // Check ComplexIC pins
        let complex_def = &comp_defs[1];
        let pins_block = find_node(complex_def, PINS_BLOCK).expect("No PINS_BLOCK in ComplexIC");
        let pin_decls = find_all_nodes(&pins_block, PIN_DECL);
        assert_eq!(pin_decls.len(), 3);

        // VDD pin
        let vdd_type_ref = find_node(&pin_decls[0], TYPE_REF).expect("No TYPE_REF for VDD");
        assert_eq!(vdd_type_ref.first_token().unwrap().kind(), POWER_KW);
        let vdd_params = find_node(&vdd_type_ref, TYPE_PARAMS).expect("No TYPE_PARAMS for VDD"); // Changed to TYPE_PARAMS
        let vdd_param_ident = vdd_params.children_with_tokens().find(|t| t.kind() == IDENT).expect("No IDENT found in VDD TYPE_PARAMS");
        assert_eq!(vdd_param_ident.as_token().unwrap().text(), "core_power");

        // IO[0] pin - Check bus suffix interaction
        let io_type_ref = find_node(&pin_decls[2], TYPE_REF).expect("No TYPE_REF for IO[0]");
        assert_eq!(io_type_ref.first_token().unwrap().kind(), SIGNAL_KW);
        let io_params = find_node(&io_type_ref, TYPE_PARAMS).expect("No TYPE_PARAMS for IO[0]"); // Changed to TYPE_PARAMS
        let io_param_ident = io_params.children_with_tokens().find(|t| t.kind() == IDENT).expect("No IDENT found in IO TYPE_PARAMS");
        assert_eq!(io_param_ident.as_token().unwrap().text(), "lvcmos_1v8");
        // Bus suffix parsing is handled in parse_pin_decl before parse_type_ref, need to ensure it still works
        // Let's check the pin name part directly for now
        let io_pin_name = pin_decls[2].children_with_tokens().filter_map(|e| e.into_token()).nth(0).unwrap();
        assert_eq!(io_pin_name.text(), "IO"); 
        // We implicitly tested bus suffix in the name part during previous steps.
    }

    #[test]
    fn parse_interface_definition() {
        let input = r#"
            interface SimpleSPI {
                pins {
                    MOSI: out signal;
                    MISO: in signal;
                    SCK: out signal;
                    CS_N: out signal;
                }
            }
            interface PowerDelivery {
                 parameters { max_current = 2A; }
                 pins {
                     VOUT: out power;
                     GND: ground;
                 }
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let source_file = result.syntax();
        let intf_defs = find_all_nodes(&source_file, INTERFACE_DEF);
        assert_eq!(intf_defs.len(), 2, "Expected 2 interface definitions");

        // Check first interface (SimpleSPI)
        let spi_def = &intf_defs[0];
        let spi_ident = spi_def.children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).unwrap();
        assert_eq!(spi_ident.text(), "SimpleSPI");
        assert!(find_node(spi_def, PARAMETERS_BLOCK).is_none()); // No params block
        let spi_pins = find_node(spi_def, PINS_BLOCK).expect("No PINS_BLOCK in SimpleSPI");
        assert_eq!(find_all_nodes(&spi_pins, PIN_DECL).len(), 4);

        // Check second interface (PowerDelivery)
        let power_def = &intf_defs[1];
        let power_ident = power_def.children_with_tokens().filter_map(|e| e.into_token()).find(|t| t.kind() == IDENT).unwrap();
        assert_eq!(power_ident.text(), "PowerDelivery");
        assert!(find_node(power_def, PARAMETERS_BLOCK).is_some());
        let power_pins = find_node(power_def, PINS_BLOCK).expect("No PINS_BLOCK in PowerDelivery");
        assert_eq!(find_all_nodes(&power_pins, PIN_DECL).len(), 2);
    }

    #[test]
    fn parse_pin_bus_suffix() {
        let input = r#"
            interface Test {
                pins {
                    data[7:0]: in signal;
                    addr[15]: out cmos_3v3;
                }
            }
        "#;
        let result = parse(input);
        assert!(result.errors.is_empty(), "Parse errors: {:?}\n\nSyntax Tree:\n{:?}", result.errors, result.syntax());

        // Find the pin declarations
        let root = result.syntax();
        let pins = find_all_nodes(&root, SyntaxKind::PIN_DECL);
        assert_eq!(pins.len(), 2);

        // Check first pin
        let data_pin = &pins[0];
        let data_suffix = find_node(data_pin, SyntaxKind::BUS_SUFFIX);
        assert!(data_suffix.is_some(), "Missing bus suffix for data pin");
        // TODO: Check content of suffix [7:0]

        // Check second pin
        let addr_pin = &pins[1];
        let addr_suffix = find_node(addr_pin, SyntaxKind::BUS_SUFFIX);
        assert!(addr_suffix.is_some(), "Missing bus suffix for addr pin");
        // TODO: Check content of suffix [15]
    }
} 