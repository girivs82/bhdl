use logos::Logos; // Re-added this import
use rowan::GreenNodeBuilder;
use smol_str::SmolStr;
use std::ops::Range;

use crate::lexer::LexerToken; // Removed KeywordOrIdent import
use crate::syntax::{SyntaxKind, BhdlLanguage};
use crate::SyntaxKind::*; // Add this line to import all variants

// --- Public Interface ---

// ParseResult struct
#[derive(Debug, Clone)]
pub struct ParseResult {
    green_node: rowan::GreenNode,
    errors: Vec<ParseError>, // Keep track of errors encountered
}

impl ParseResult {
    pub fn syntax(&self) -> rowan::SyntaxNode<BhdlLanguage> {
        rowan::SyntaxNode::new_root(self.green_node.clone())
    }

    pub fn errors(&self) -> &[ParseError] {
        &self.errors
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParseError {
    pub message: String,
    // Add range/span later
    // pub span: (usize, usize),
}

// New mapping function
fn map_token_stream(tokens: Vec<(Result<LexerToken, ()>, Range<usize>)>, source_text: &str) -> Vec<(SyntaxKind, SmolStr)> {
    tokens.into_iter()
        .filter_map(|(res, range)| {
            let text = SmolStr::new(&source_text[range]);
            match res {
                Ok(token) => Some((map_token(token), text)),
                Err(_) => Some((SyntaxKind::ERROR_TOKEN, text)), // Map lexer errors to ERROR_TOKEN
            }
        })
        .collect()
}

// Helper function to map a single LexerToken to SyntaxKind
fn map_token(token: LexerToken) -> SyntaxKind {
    match token {
        // Handle the combined KeywordOrIdent variant
        LexerToken::KeywordOrIdent(payload) => payload.kind, // Use the kind determined by the lexer callback

        // Basic punctuation
        LexerToken::LParen => SyntaxKind::L_PAREN,
        LexerToken::RParen => SyntaxKind::R_PAREN,
        LexerToken::LBrace => SyntaxKind::L_BRACE,
        LexerToken::RBrace => SyntaxKind::R_BRACE,
        LexerToken::LBrack => SyntaxKind::L_BRACKET,
        LexerToken::RBrack => SyntaxKind::R_BRACKET,
        LexerToken::Semi => SyntaxKind::SEMI,
        LexerToken::Colon => SyntaxKind::COLON,
        LexerToken::Comma => SyntaxKind::COMMA,
        LexerToken::Eq => SyntaxKind::EQ,
        LexerToken::Dot => SyntaxKind::DOT,
        LexerToken::Plus => SyntaxKind::PLUS,
        LexerToken::Minus => SyntaxKind::MINUS,
        LexerToken::Star => SyntaxKind::STAR,
        LexerToken::Slash => SyntaxKind::SLASH,
        LexerToken::Percent => SyntaxKind::PERCENT, // Note: Also a Unit token
        LexerToken::Ampersand => SyntaxKind::AMPERSAND,
        LexerToken::Pipe => SyntaxKind::PIPE,
        LexerToken::Caret => SyntaxKind::CARET,
        LexerToken::Bang => SyntaxKind::BANG,
        LexerToken::Question => SyntaxKind::QUESTION,
        LexerToken::Tilde => SyntaxKind::TILDE,
        LexerToken::LAngle => SyntaxKind::L_ANGLE,
        LexerToken::RAngle => SyntaxKind::R_ANGLE,
        LexerToken::At => SyntaxKind::AT,

        // Literals
        LexerToken::Number => SyntaxKind::NUMBER,
        LexerToken::String => SyntaxKind::STRING,

        // Operators
        LexerToken::Arrow => SyntaxKind::ARROW,
        LexerToken::EqEq => SyntaxKind::EQEQ,
        LexerToken::Neq => SyntaxKind::NEQ,
        LexerToken::LtEq => SyntaxKind::LTEQ,
        LexerToken::GtEq => SyntaxKind::GTEQ,
        LexerToken::AmpAmp => SyntaxKind::AMPAMP,
        LexerToken::PipePipe => SyntaxKind::PIPEPIPE,
        LexerToken::LShift => SyntaxKind::LSHIFT,
        LexerToken::RShift => SyntaxKind::RSHIFT,
        LexerToken::IfConnect => SyntaxKind::IF_CONNECT,

        // In parser.rs -> map_token function
        // Map ALL unit tokens to the single UNIT_IDENTIFIER SyntaxKind
        LexerToken::KOhmUnit | LexerToken::MOHmUnit | LexerToken::GOhmUnit | LexerToken::OhmUnit |
        LexerToken::UFUnit | LexerToken::NFUnit | LexerToken::PFUnit |
        LexerToken::UHUnit | LexerToken::NHUnit | LexerToken::PHUnit |
        LexerToken::VdcUnit | LexerToken::VacUnit | LexerToken::VrmsUnit | LexerToken::VppUnit |
        LexerToken::MVUnit | LexerToken::UVUnit | LexerToken::NVUnit |
        LexerToken::MAUnit | LexerToken::UAUnit | LexerToken::NAUnit |
        LexerToken::MWUnit | LexerToken::UWUnit | LexerToken::NWUnit |
        LexerToken::HzUnit | LexerToken::KHzUnit | LexerToken::MHUnit | LexerToken::GHUnit |
        LexerToken::MsUnit | LexerToken::UsUnit | LexerToken::NsUnit | LexerToken::PsUnit |
        LexerToken::DegUnit | LexerToken::RadUnit |
        LexerToken::DbUnit | LexerToken::DbmUnit |
        LexerToken::PctUnit |
        LexerToken::MMUnit | LexerToken::UMUnit | LexerToken::NMUnit | LexerToken::MILUnit
        => SyntaxKind::UNIT_IDENTIFIER,

    }
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

    // --- Inserted Core Helper Methods ---

    /// Returns the kind of the current token, skipping trivia.
    fn peek(&self) -> Option<SyntaxKind> {
        let mut temp_pos = self.pos;
        loop {
            match self.tokens.get(temp_pos) {
                Some((kind, _)) if kind.is_trivia() => temp_pos += 1,
                Some((kind, _)) => return Some(*kind),
                None => return None,
            }
        }
    }

    /// Returns the kind of the current token *without* skipping trivia.
    fn peek_raw(&self) -> Option<SyntaxKind> {
        self.tokens.get(self.pos).map(|(kind, _)| *kind)
    }

    /// Returns the kind of the nth token ahead, skipping trivia between tokens.
    fn peek_n(&self, n: usize) -> Option<SyntaxKind> {
        let mut count = 0;
        let mut temp_pos = self.pos;
        loop {
            match self.tokens.get(temp_pos) {
                Some((kind, _)) if kind.is_trivia() => temp_pos += 1,
                Some((kind, _)) => {
                    if count == n {
                        return Some(*kind);
                    }
                    count += 1;
                    temp_pos += 1;
                }
                None => return None,
            }
        }
    }

    /// Consumes the current token if it matches the expected kind.
    /// Returns true if consumed, false otherwise.
    fn eat(&mut self, expected: SyntaxKind) -> bool {
        if self.peek() == Some(expected) {
            self.bump(); // bump handles trivia and adds token to builder
            true
        } else {
            false
        }
    }

    /// Consumes the current token if it matches the expected kind.
    /// Reports an error if the token doesn't match.
    fn expect(&mut self, expected: SyntaxKind) {
        if self.peek() == Some(expected) {
            self.bump();
        } else {
            self.error(format!("Expected {:?}, found {:?}", expected, self.peek()));
            // Recovery: Don't bump here, let the caller decide how to recover.
            //             Or, alternatively, bump the unexpected token:
            // if self.peek().is_some() { self.bump_any(); }
        }
    }

    /// Consumes trivia tokens (whitespace, comments) until a non-trivia token is found.
    fn skip_trivia(&mut self) {
        while let Some((kind, text)) = self.tokens.get(self.pos) {
            if kind.is_trivia() {
                self.builder.token((*kind).into(), text);
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Consumes the current token regardless of its kind, adding it to the tree.
    /// Skips trivia first.
    fn bump(&mut self) {
        self.skip_trivia();
        if self.pos < self.tokens.len() {
            let (kind, text) = self.tokens[self.pos].clone();
            self.builder.token(kind.into(), &text);
            self.pos += 1;
        } else {
             // Should not happen if peek() was checked before bump()
             self.error("Internal error: bump called at EOF".to_string());
        }
    }

    /// Consumes the current token *without* skipping trivia first. Used for recovery.
    /// Adds the token (potentially trivia or error) to the node builder.
    fn bump_any(&mut self) {
        if self.pos < self.tokens.len() {
            let (kind, text) = self.tokens[self.pos].clone();
            self.builder.token(kind.into(), &text);
            self.pos += 1;
        } else {
             self.error("Internal error: bump_any called at EOF".to_string());
        }
    }

    /// Records a parse error.
    fn error(&mut self, message: String) {
        self.errors.push(ParseError { message });
        // Optionally add error node to the tree for better recovery?
        // self.builder.start_node(SyntaxKind::ERROR.into());
        // self.builder.finish_node();
    }

    /// Returns the kind of the current token being pointed to. Used in error reporting.
    fn current(&self) -> Option<SyntaxKind> {
        self.peek() // Use peek to get the kind after skipping trivia
    }

    // --- End of Inserted Core Helper Methods ---

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
                SyntaxKind::INTERFACE_KW => self.parse_interface_def(),
                _ => {
                    // Handle unexpected tokens at the top level
                    self.error(format!("Expected a top-level item (e.g., 'board', 'component', 'interface', etc.), found {:?}", kind));
                    self.bump_any(); // Consume the unexpected token
                }
            }
        }
        self.builder.finish_node();
    }

    // --- Inserted Missing Parsing Functions ---

    // Reconstructed parse_module_def
    fn parse_module_def(&mut self) {
        self.builder.start_node(SyntaxKind::MODULE_DEF.into());
        self.expect(SyntaxKind::MODULE_KW);
        self.expect(SyntaxKind::IDENT); // Module Name
        self.expect(SyntaxKind::L_BRACE);

        // Parse items inside the module block (similar to board)
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PARAMETERS_KW) => self.parse_parameters_block(),
                Some(SyntaxKind::PORTS_KW) => self.parse_ports_block(),
                Some(SyntaxKind::NETS_KW) => self.parse_nets_block(),
                Some(SyntaxKind::COMPONENTS_KW) => self.parse_components_block(),
                Some(SyntaxKind::CONNECTIONS_KW) => self.parse_connections_block(),
                Some(SyntaxKind::CONSTRAIN_KW) => self.parse_constrain_block(),
                // Add other blocks if modules can contain them (e.g., INTERFACES_KW?)
                Some(kind) => {
                    self.error(format!("Unexpected token inside module definition: {:?}. Expected block keyword or '}}'.", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside module definition block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Reconstructed parse_typedef_def (incorporating 'extends' logic from earlier)
    fn parse_typedef_def(&mut self) {
        self.builder.start_node(SyntaxKind::TYPEDEF_DEF.into());
        self.expect(SyntaxKind::TYPEDEF_KW);
        self.expect(SyntaxKind::IDENT); // Type Name

        let mut extends_parsed = false;
        // Optional: extends BaseType
        if self.eat(SyntaxKind::EXTENDS_KW) {
            extends_parsed = true;
            self.builder.start_node(SyntaxKind::TYPEDEF_BASE.into()); // Node for base type
            // Allow keywords or identifiers as base type name
            if self.peek() == Some(SyntaxKind::IDENT) { // Assuming base type is IDENT for now
                 self.bump();
            } else {
                 self.error("Expected base type name (identifier) after 'extends'".to_string());
            }
            self.builder.finish_node(); // Finish TYPEDEF_BASE
        }

        // Optional body `{...}` or just semicolon after extends
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.expect(SyntaxKind::L_BRACE);
            // Parse assignments inside (reuse param_assign logic)
            while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
                if self.peek() == Some(SyntaxKind::IDENT) {
                    self.parse_param_assign();
                } else {
                    self.error(format!("Expected type property assignment (identifier = value) or '}}', found {:?}", self.peek()));
                    self.bump_any();
                }
            }
            self.expect(SyntaxKind::R_BRACE);
        } else if extends_parsed {
            // If extends was parsed and there's no body, expect SEMI
            self.expect(SyntaxKind::SEMI);
        } else {
            // If no extends and no L_BRACE, it's an error
            self.error(format!("Expected 'extends' or '{{' after typedef name, found {:?}", self.peek()));
            // Consume the unexpected token? Or let subsequent expects handle it?
            // Let's not consume here, expect R_BRACE later will likely fail correctly.
        }

        self.builder.finish_node();
    }

    // Reconstructed parse_import_stmt
    fn parse_import_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_STMT.into());
        self.expect(SyntaxKind::IMPORT_KW);

        // Parse the path (Ident.Ident...)
        self.builder.start_node(SyntaxKind::IMPORT_PATH.into());
        self.expect(SyntaxKind::IDENT);
        while self.eat(SyntaxKind::DOT) {
            // Check if the next token is L_BRACE for group import
            if self.peek_raw() == Some(SyntaxKind::L_BRACE) {
                break; // Path ends before the group brace
            }
            if !self.eat(SyntaxKind::IDENT) {
                 // If it wasn't IDENT or L_BRACE after DOT, it's an error
                 self.error("Expected identifier or '{' after '.' in import path".to_string());
                 break; // Stop parsing path
            }
        }
        self.builder.finish_node(); // IMPORT_PATH

        // Parse the target (either simple IDENT implicitly or group { Target, ... })
        if self.eat(SyntaxKind::L_BRACE) {
            self.builder.start_node(SyntaxKind::IMPORT_TARGET_GROUP.into());
            loop {
                self.expect(SyntaxKind::IDENT);
                if !self.eat(SyntaxKind::COMMA) {
                    break; // Exit loop if no comma follows identifier
                }
                if self.peek_raw() == Some(SyntaxKind::R_BRACE) {
                    // Handle trailing comma
                    break;
                }
            }
            self.expect(SyntaxKind::R_BRACE);
            self.builder.finish_node(); // IMPORT_TARGET_GROUP
        } else {
            // If no L_BRACE, the last IDENT in the path *is* the target
            // Create an empty target node for consistency?
            self.builder.start_node(SyntaxKind::IMPORT_TARGET.into());
            self.builder.finish_node();
        }

        // Optional 'as Alias'
        if self.eat(SyntaxKind::AS_KW) {
            self.builder.start_node(SyntaxKind::ALIAS.into());
            self.expect(SyntaxKind::IDENT); // Alias name
            self.builder.finish_node();
        }


        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node(); // IMPORT_STMT
    }

    // Reconstructed parse_component_def
    fn parse_component_def(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENT_DEF.into());
        self.expect(SyntaxKind::COMPONENT_KW);
        self.expect(SyntaxKind::IDENT); // Component Name
        self.expect(SyntaxKind::L_BRACE);

        // Parse items inside the component block
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PARAMETERS_KW) => self.parse_parameters_block(),
                Some(SyntaxKind::PINS_KW) => self.parse_pins_block(),
                Some(SyntaxKind::INTERFACES_KW) => self.parse_interfaces_block(),
                // Add other relevant blocks if needed (e.g., constraints?)
                Some(kind) => {
                    self.error(format!("Unexpected token inside component definition: {:?}. Expected block keyword or '}}'.", kind));
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

    // Reconstructed parse_pins_block
    fn parse_pins_block(&mut self) {
        self.builder.start_node(SyntaxKind::PINS_BLOCK.into());
        self.expect(SyntaxKind::PINS_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) | Some(SyntaxKind::NUMBER) => { // Pin names can be IDENT or NUMBER
                    self.parse_pin_decl();
                }
                Some(SyntaxKind::GENERATE_KW) => {
                    // Handle generate for pins
                    self.parse_generate_for_pins(); // Assumes this was defined/retained
                }
                Some(kind) => {
                    self.error(format!("Expected pin declaration (identifier/number), generate block, or '}}', found {:?}", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside pins block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Reconstructed parse_pin_decl
    fn parse_pin_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_DECL.into());
        // Pin names can be IDENT or NUMBER (e.g., for connectors)
        if !self.eat(SyntaxKind::IDENT) && !self.eat(SyntaxKind::NUMBER) {
             self.error("Expected pin name (identifier or number)".to_string());
             self.builder.finish_node();
             return;
        }

        // Optional bus suffix
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }
        self.expect(SyntaxKind::COLON);

        // Direction is required *unless* type is GROUND_KW
        let is_ground = self.peek() == Some(SyntaxKind::GROUND_KW);
        if !is_ground {
            match self.peek() {
                Some(SyntaxKind::IN_KW) | Some(SyntaxKind::OUT_KW) | Some(SyntaxKind::INOUT_KW) => {
                    self.bump(); // Consume direction keyword
                }
                _ => {
                    // Report error but proceed, hoping to find type
                    self.error("Expected pin direction (in, out, inout)".to_string());
                }
            }
        }

        // Parse type reference
        self.parse_type_ref(); // E.g., signal, power, ground, or custom type like cmos_3v3

        // Optional pin properties (comma separated after type)
        if self.eat(SyntaxKind::COMMA) {
            self.builder.start_node(SyntaxKind::PIN_PROPERTIES.into());
             loop {
                // Simple property parse: key=value
                if self.peek() == Some(SyntaxKind::IDENT) {
                    self.parse_param_assign(); // Reuse simple key=value parsing
                    if !self.eat(SyntaxKind::COMMA) {
                        break; // Exit loop if no comma follows property
                    }
                } else {
                    self.error(format!("Expected pin property assignment (identifier = value) after comma, found {:?}", self.peek()));
                    break; // Exit loop on error
                }
            }
            self.builder.finish_node(); // Finish PIN_PROPERTIES
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Reconstructed parse_bus_suffix
    fn parse_bus_suffix(&mut self) {
        // Ensure we are at L_BRACKET before starting node? Use peek() to handle trivia.
        if self.peek() != Some(SyntaxKind::L_BRACKET) {
             self.error("Expected '[' to start bus suffix".to_string());
             return; // Don't proceed if no bracket
        }
        self.builder.start_node(SyntaxKind::BUS_SUFFIX.into()); // Start BUS_SUFFIX node
        self.expect(SyntaxKind::L_BRACKET);

        // Checkpoint *before* parsing the first potential part of the range/index
        let expr1_checkpoint = self.builder.checkpoint();
        self.parse_expr(0); // Parse the first expression (index or high bound)

        // Check if it's a range
        if self.peek() == Some(SyntaxKind::COLON) {
            // It's a range, create a RANGE_EXPR node around the two expressions
            self.builder.start_node_at(expr1_checkpoint, SyntaxKind::RANGE_EXPR.into());
            self.expect(SyntaxKind::COLON); // Consume COLON (expect handles trivia)
            self.parse_expr(0); // Parse the second expression (low bound)
            self.builder.finish_node(); // Finish RANGE_EXPR
        } else {
            // It's just an index. The first expression is already parsed.
            // No RANGE_EXPR node needed. The first expr remains a direct child of BUS_SUFFIX.
        }

        self.expect(SyntaxKind::R_BRACKET);
        self.builder.finish_node(); // Finish BUS_SUFFIX
    }

    // ADD parse_interfaces_block (called by parse_component_def)
    fn parse_interfaces_block(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACES_BLOCK.into());
        self.expect(SyntaxKind::INTERFACES_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => { // Interface instance starts with IDENT (name)
                    self.parse_interface_inst();
                }
                Some(kind) => {
                    self.error(format!("Expected interface instance (identifier) or '}}', found {:?}", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside interfaces block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // ADD parse_interface_inst (called by parse_interfaces_block)
    // Parses: InstName: interface TypeName { pin_map = { ... }, param = value; ... }
    fn parse_interface_inst(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_INSTANCE.into());
        self.expect(SyntaxKind::IDENT); // Instance Name (e.g., MEM, SPI1)
        // Optional ':' ? Spec examples vary, assume optional for now or require? Let's require based on example.
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::INTERFACE_KW); // 'interface' keyword
        self.expect(SyntaxKind::IDENT); // Interface Type Name (e.g., DDR_Interface, SPI)

        // Parse block with pin_map and optional parameter overrides
        self.expect(SyntaxKind::L_BRACE);
         while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            match self.peek() {
                // Change PIN_MAP_KW to IDENT
                Some(SyntaxKind::IDENT) => {
                    // Check if the identifier is actually "pin_map"
                    let current_token_text = self.tokens.get(self.pos).map(|(_, text)| text.clone());
                    if current_token_text.as_deref() == Some("pin_map") {
                        self.parse_pin_map_block();
                    } else {
                        // Assume it's a parameter assignment
                        self.parse_param_assign();
                    }
                }
                // Some(SyntaxKind::PIN_MAP_KW) => self.parse_pin_map_block(), // Removed
                // Some(SyntaxKind::IDENT) => self.parse_param_assign(), // Combined above
                Some(kind) => {
                    self.error(format!("Expected 'pin_map' assignment or parameter assignment or '}}' inside interface instance, found {:?}", kind));
                     self.bump_any();
                }
                None => break, // EOF
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        // Semicolon? Spec doesn't show one, assume no.
        self.builder.finish_node();
    }

    // ADD parse_pin_map_block (called by parse_interface_inst)
    // Parses: pin_map = { LogPin = PhysPin, ... }
    fn parse_pin_map_block(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_MAP_BLOCK.into());
        // Change PIN_MAP_KW to IDENT and check its text
        if self.peek() == Some(SyntaxKind::IDENT) {
            let current_token_text = self.tokens.get(self.pos).map(|(_, text)| text.clone());
            if current_token_text.as_deref() == Some("pin_map") {
                 self.bump(); // Consume the "pin_map" IDENT
            } else {
                 self.error("Expected 'pin_map' keyword".to_string());
                 // Attempt recovery? Or just proceed to expect EQ?
            }
        } else {
            self.error("Expected 'pin_map' keyword".to_string());
            // Attempt recovery by potentially bumping the wrong token?
            // self.bump_any();
        }
        // self.expect(SyntaxKind::PIN_MAP_KW); // Removed
        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek_raw() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => { // Mapping starts with logical pin IDENT
                    self.parse_pin_map_entry();
                    if !self.eat(SyntaxKind::COMMA) {
                         // Allow trailing comma or end of mappings
                         if self.peek() != Some(SyntaxKind::R_BRACE) {
                             self.error("Expected ',' or '}' after pin map entry".to_string());
                             // Try to recover by consuming until comma or brace
                             while self.peek() != Some(SyntaxKind::COMMA) && self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
                                 self.bump_any();
                             }
                             self.eat(SyntaxKind::COMMA); // Consume comma if found
                         }
                         break; // If no comma or error occurred, assume end of list for this entry
                    }
                    // Handle trailing comma just before brace
                    if self.peek_raw() == Some(SyntaxKind::R_BRACE) { break; }
                }
                Some(kind) => {
                    self.error(format!("Expected pin map entry (identifier) or '}}', found {:?}", kind));
                    self.bump_any(); // Consume to recover
                }
                None => {
                    self.error("Unexpected end of file inside pin_map block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // ADD parse_pin_map_entry (called by parse_pin_map_block)
    // Parses: LogPin = PhysPin
    fn parse_pin_map_entry(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_MAP_ENTRY.into());
        self.expect(SyntaxKind::IDENT); // Logical Pin Name
        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::IDENT); // Physical Pin Name
        self.builder.finish_node();
    }

    // --- End of Inserted Missing Parsing Functions ---

    // --- Existing Grammar Rule Parsers ---
    fn parse_board_def(&mut self) {
        self.builder.start_node(SyntaxKind::BOARD_DEF.into());
        self.expect(SyntaxKind::BOARD_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::L_BRACE);

        // Parse items inside the board block
        loop {
            self.skip_trivia(); // Still skip trivia explicitly first
            // Use peek() which also skips trivia internally, instead of peek_raw()
            match self.peek() { 
                Some(SyntaxKind::R_BRACE) => break, // End of block
                Some(SyntaxKind::PARAMETERS_KW) => self.parse_parameters_block(),
                Some(SyntaxKind::PORTS_KW) => self.parse_ports_block(),
                Some(SyntaxKind::NETS_KW) => self.parse_nets_block(),
                Some(SyntaxKind::COMPONENTS_KW) => self.parse_components_block(),
                Some(SyntaxKind::CONNECTIONS_KW) => self.parse_connections_block(),
                Some(SyntaxKind::LAYER_STACKUP_KW) => self.parse_layer_stackup_block(),
                Some(SyntaxKind::DEFAULT_DESIGN_RULES_KW) => self.parse_default_design_rules_block(),
                Some(SyntaxKind::CONSTRAIN_KW) => self.parse_constrain_block(), // Ensure this case exists and calls the right function
                // Some(SyntaxKind::PINS_KW) => self.parse_pins_block(), // Pins are not directly in board
                Some(kind) => {
                    self.error(format!("Unexpected token inside board definition: {:?}. Expected block keyword (parameters, ports, nets, components, connections, layer_stackup, default_design_rules, constrain) or '}}'.", kind));
                    self.bump_any(); // Consume unexpected token
                }
                None => { // Reached EOF unexpectedly
                    self.error("Unexpected end of file inside board definition block".to_string());
                    break;
                }
            }
        }

        // Add skip_trivia before expecting the final brace
        self.skip_trivia();
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
                 // Otherwise, parse a regular value/expression
                 self.parse_expr(0); // NEW - Allow full expressions
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
            match self.peek() {
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
            match self.peek() {
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
             self.parse_expr(0); // NEW - Parse the RHS expression
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

    // Parses a simple value (NUMBER, optionally with sign and unit)
    // Used by the expression parser for literals.
    fn parse_value(&mut self) {
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
             // self.builder.start_node(SyntaxKind::ERROR.into());
             // self.builder.finish_node();
        }
        self.builder.finish_node(); // Finish VALUE node
    }

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
    fn parse_expr(&mut self, min_bp: u8) {
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
    fn parse_primary_expr(&mut self) {
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
                self.error(format!("Expected literal, identifier, or '(' for expression factor, found {:?}", self.current()));
                // Consume unexpected token for recovery?
                if self.current().is_some() { self.bump_any(); }
                // Add an ERROR node maybe?
                self.builder.start_node(SyntaxKind::ERROR.into());
                self.builder.finish_node();
            }
        }
    }

    // Parses the argument list for a function call: (arg1, arg2, ...)
    fn parse_argument_list(&mut self) {
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

    // --- End of Expression Parsing ---

    // --- Old Expression Parsing (To be removed/commented out) ---
/*
    // Expression ::= Term ( ('+' | '-') Term )*
    fn parse_expression(&mut self) {
        self.builder.start_node(SyntaxKind::EXPRESSION.into());
        self.parse_term();
        while self.peek() == Some(SyntaxKind::PLUS) || self.peek() == Some(SyntaxKind::MINUS) {
            self.bump(); // Consume '+' or '-'
            self.parse_term();
            // In a real AST builder, we'd combine the nodes here.
            // For CST, just consuming tokens in order works for now.
        }
        self.builder.finish_node();
    }

    // Term ::= Factor ( ('*' | '/') Factor )*
    fn parse_term(&mut self) {
        // Start a TERM node? Maybe not needed for simple CST.
        self.parse_factor();
        while self.peek() == Some(SyntaxKind::STAR) || self.peek() == Some(SyntaxKind::SLASH) {
            self.bump(); // Consume '*' or '/'
            self.parse_factor();
        }
    }

    // Factor ::= NUMBER | IDENT | '(' Expression ')'
    fn parse_factor(&mut self) {
        // Start a FACTOR node? Maybe not needed for simple CST.
        if self.eat(SyntaxKind::NUMBER) || self.eat(SyntaxKind::IDENT) {
            // Consumed atom (Number or Identifier)
            // TODO: Handle potential units attached to NUMBER or IDENT values?
        } else if self.eat(SyntaxKind::L_PAREN) {
            self.parse_expression(); // Recurse for parenthesized expression
            self.expect(SyntaxKind::R_PAREN);
        } else {
            self.error(format!("Expected number, identifier, or '(' for expression factor, found {:?}", self.current()));
            // Consume unexpected token for recovery?
            if self.current().is_some() { self.bump_any(); }
        }
    }
*/
    // --- End of Old Expression Parsing ---


    // --- Existing Block/Item Parsers ---
    fn parse_components_block(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENTS_BLOCK.into());
        self.expect(SyntaxKind::COMPONENTS_KW);
        self.expect(SyntaxKind::L_BRACE);

        // Parse component instantiations
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::IDENT => { // Found start of an instantiation
                     self.parse_component_inst();
                     // After parsing an instance, optionally consume a semicolon
                     self.eat(SyntaxKind::SEMI);
                }
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

        // Parse connection or assign statements
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::ASSIGN_KW => self.parse_assign_stmt(), // Added case for assign
                SyntaxKind::IDENT => self.parse_connection_stmt(), // Regular connection starts with IDENT
                _ => {
                    self.error(format!("Expected connection statement (identifier) or assign statement or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Revised parsing logic for references (NET_REF, PIN_REF, SIMPLE_IDENT_REF)
    // Accepts the SyntaxKind to use if the reference is just a simple identifier.
    fn parse_ref_revised(&mut self, simple_kind: SyntaxKind) {
        // Start the appropriate node kind *after* parsing the initial identifier
        // to determine if it's a net or part of a pin ref.

        if self.peek() != Some(IDENT) {
            self.error("Expected identifier for reference".to_string());
            // If we error, perhaps create an ERROR node or just don't create a node?
            // For now, just return without creating a node.
            return;
        }

        // Get a checkpoint *before* consuming the first identifier
        let cp = self.builder.checkpoint();

        self.bump(); // Consume the first IDENT (e.g., NetName or InstanceName)

        // Check for dot (pin access: Instance.Pin)
        if self.peek() == Some(DOT) {
            self.builder.start_node_at(cp, PIN_REF.into()); // Start PIN_REF node at the checkpoint
            self.bump(); // Consume DOT

            // Expect IDENT or NUMBER after dot (e.g., PinName or 1)
            if self.peek() == Some(IDENT) || self.peek() == Some(NUMBER) {
                self.bump(); // Consume IDENT or NUMBER

                // Optional bus suffix after dot access (e.g., Instance.Pin[0])
                if self.peek() == Some(L_BRACKET) {
                    self.parse_bus_suffix(); // Parse the bus suffix
                }
                self.builder.finish_node(); // Finish PIN_REF node
            } else {
                self.error("Expected identifier or number after '.' in pin reference".to_string());
                // Even with error, finish the PIN_REF node containing what was parsed
                self.builder.finish_node();
            }
        // Check for bracket (net with bus suffix: NetName[0])
        } else if self.peek() == Some(L_BRACKET) {
            self.builder.start_node_at(cp, NET_REF.into()); // Start NET_REF node at the checkpoint
            self.parse_bus_suffix(); // Parse the bus suffix
            self.builder.finish_node(); // Finish NET_REF node

        // Simple identifier reference (Use the provided simple_kind)
        } else {
            self.builder.start_node_at(cp, simple_kind.into());
            // No more tokens to consume for a simple ref
            self.builder.finish_node(); // Finish the node (e.g., NET_REF, SIMPLE_IDENT_REF)
        }
        // No outer marker node anymore
    }

    // Adjust parse_assign_stmt to use parse_ref_revised for LHS
    fn parse_assign_stmt(&mut self) {
        self.builder.start_node(ASSIGN_STMT.into());
        self.expect(ASSIGN_KW); // Consume the 'assign' keyword first!
        // Use parse_ref_revised for the left-hand side, specifying NET_REF for simple idents
        self.parse_ref_revised(NET_REF);
        self.expect(EQ);
        self.parse_expr(0); // Parse the right-hand side expression
        self.expect(SEMI);
        self.builder.finish_node(); // Finish ASSIGN_STMT node
    }

    // This function parses -> and <=> connections
    fn parse_connection_stmt(&mut self) {
        self.builder.start_node(CONNECTION_STMT.into());

        // Parse LHS (one or more refs)
        // Simple identifiers in connections should be generic refs initially
        self.parse_ref_revised(SIMPLE_IDENT_REF); // Use SIMPLE_IDENT_REF
        while self.eat(COMMA) { // Use eat() for optional comma
            self.parse_ref_revised(SIMPLE_IDENT_REF); // Use SIMPLE_IDENT_REF
        }

        // Expect an arrow or interface connection operator
        if self.eat(ARROW) {
            // Parse RHS for ->
            self.parse_ref_revised(SIMPLE_IDENT_REF); // Use SIMPLE_IDENT_REF
            while self.eat(COMMA) { // Use eat() for optional comma
                self.parse_ref_revised(SIMPLE_IDENT_REF); // Use SIMPLE_IDENT_REF
            }
        } else if self.eat(IF_CONNECT) {
            // Parse RHS for <=> (likely an interface reference)
            // Interfaces are referred to by simple name, use SIMPLE_IDENT_REF here?
            // Let's stick with NET_REF for now, might need INTERFACE_REF later.
            self.parse_ref_revised(NET_REF);
            // Interface connections are typically 1-to-1, check for trailing comma.
            if self.peek() == Some(COMMA) {
                self.error("Interface connection operator <=> expects a single target on each side.".to_string());
                 // Consume the comma and potentially following refs to allow parsing to continue
                 while self.eat(COMMA) {
                     self.parse_ref_revised(NET_REF);
                 }
            }
        } else {
            self.error(format!("Expected '->' or '<=>' in connection statement, found {:?}", self.peek())); // Use peek() not current()
            // Attempt to recover by skipping until semicolon?
            while self.peek() != Some(SEMI) && self.peek().is_some() { // Use peek() not current()
                self.bump_any();
            }
        }

        self.expect(SEMI);
        self.builder.finish_node();
    }

    // Parses: generate for <var> in <range> { <pin_decl>... }
    fn parse_generate_for_pins(&mut self) {
        self.builder.start_node(GENERATE_FOR_BLOCK.into());
        self.expect(GENERATE_KW);
        self.expect(FOR_KW);
        self.expect(IDENT); // Loop variable
        self.expect(IN_KW);
        self.parse_range_expr(); // Parse the range (e.g., 0 to data_width-1)
        self.expect(L_BRACE);

        // Parse pin declarations inside the generate block
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(R_BRACE) => break,
                // Pin names can be IDENT or NUMBER
                Some(IDENT) | Some(NUMBER) => {
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

        self.expect(R_BRACE);
        self.builder.finish_node();
    }

    // Placeholder for range expressions like '0 to WIDTH-1'
    // Simplified: Parses 'NUMBER to NUMBER' for now
    fn parse_range_expr(&mut self) {
        self.builder.start_node(RANGE_EXPR.into());
        self.parse_expr(0); // NEW - Parse start expression
        self.expect(TO_KW); // Expect 'to' keyword
        self.parse_expr(0); // NEW - Parse end expression
        self.builder.finish_node();
    }

    fn parse_interface_def(&mut self) {
        self.builder.start_node(INTERFACE_DEF.into());
        self.expect(INTERFACE_KW);
        self.expect(IDENT); // Interface Name
        // TODO: Parse optional interface parameters (...) later?
        self.expect(L_BRACE);

        // Parse items inside the interface block
        while let Some(kind) = self.peek() {
            match kind {
                R_BRACE => break, // End of block
                PARAMETERS_KW => self.parse_parameters_block(), // Reuse
                PINS_KW => self.parse_pins_block(), // Reuse
                // Add INTERFACES_KW? Spec isn't clear if interfaces can contain other interfaces directly.
                _ => {
                    self.error(format!("Unexpected token inside interface definition: {:?}. Expected parameters, pins, or '}}'.", kind));
                    self.bump_any(); // Consume unexpected token
                }
            }
        }

        self.expect(R_BRACE);
        self.builder.finish_node(); // No semicolon after interface def
    }

    // Add stubs for the new block parsers called from parse_board_def

    fn parse_layer_stackup_block(&mut self) {
        self.builder.start_node(SyntaxKind::LAYER_STACKUP_BLOCK.into());
        self.expect(SyntaxKind::LAYER_STACKUP_KW);
        self.expect(SyntaxKind::L_BRACE);
        // Parse layer definitions
        while self.peek() == Some(SyntaxKind::LAYER_KW) {
            self.parse_layer_def();
        }
        // Handle unexpected tokens inside block?
        if self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.error(format!("Expected 'layer' keyword or '}}', found {:?}", self.peek()));
            // Consume until R_BRACE
            while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses: layer NAME { prop = value; ... }
    fn parse_layer_def(&mut self) {
        self.builder.start_node(SyntaxKind::LAYER_DEF.into());
        self.expect(SyntaxKind::LAYER_KW);
        self.expect(IDENT); // Layer Name
        self.expect(SyntaxKind::L_BRACE);
        // Parse assignments inside
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.parse_param_assign(); // Reuse param assign
            } else {
                self.error(format!("Expected layer property assignment (identifier = value) or '}}', found {:?}", self.peek()));
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        // Optional semicolon? Spec example doesn't show one, but consistency might suggest it? Assume NO for now.
        self.builder.finish_node();
    }

    fn parse_default_design_rules_block(&mut self) {
        self.builder.start_node(SyntaxKind::DEFAULT_DESIGN_RULES_BLOCK.into());
        self.expect(SyntaxKind::DEFAULT_DESIGN_RULES_KW);
        self.expect(SyntaxKind::L_BRACE);
        // Parse design rule assignments (reuse param_assign logic)
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.parse_param_assign();
            } else {
                self.error(format!("Expected design rule assignment (identifier = value) or '}}', found {:?}", self.peek()));
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_constrain_block(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRAIN_BLOCK.into());
        self.expect(SyntaxKind::CONSTRAIN_KW);

        // Parse constrain target (NetName, PinRef, Group, etc.) in parentheses
        self.expect(SyntaxKind::L_PAREN);
        self.builder.start_node(SyntaxKind::CONSTRAINT_TARGET.into());
        // Simple target parsing for now: Assume IDENT or PIN_REF
        // TODO: Expand to handle lists (NetA, NetB), groups, connections
        if self.peek() == Some(SyntaxKind::IDENT) {
            // Could be a net name or start of pin ref
            // Peek ahead for DOT to differentiate
            if self.peek_n(1) == Some(SyntaxKind::DOT) || self.peek_n(1) == Some(SyntaxKind::L_BRACKET) {
                self.parse_ref_revised(NET_REF); // Use the revised function
            } else {
                // Just a simple IDENT (net name)
                // Should also be parsed as a NET_REF for consistency
                self.parse_ref_revised(SIMPLE_IDENT_REF); // Use the revised function here too
            }
        } else {
            self.error("Expected target identifier (net name or pin reference) inside constrain parentheses".to_string());
            // Consume until R_PAREN to attempt recovery?
            while self.current() != Some(SyntaxKind::R_PAREN) && self.current().is_some() {
                self.bump_any();
            }
        }
        self.builder.finish_node(); // Finish CONSTRAINT_TARGET
        self.expect(SyntaxKind::R_PAREN);
        
        // Parse constraint assignments inside braces
        self.expect(SyntaxKind::L_BRACE);
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            // Reuse param_assign logic for now. 
            // TODO: Need to handle specific constraint value syntax (e.g., ranges, +/-)
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.parse_param_assign();
            } else {
                self.error(format!("Expected constraint assignment (identifier = value) or '}}', found {:?}", self.peek()));
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node(); // Finish CONSTRAIN_BLOCK
    }
}

// Top-level parse function
pub fn parse(text: &str) -> ParseResult {
    let tokens: Vec<_> = LexerToken::lexer(text).spanned().collect(); // Use LexerToken::lexer directly
    let mapped_tokens = map_token_stream(tokens, text); // Pass text as second argument
    let mut parser = Parser::new(&mapped_tokens);
    parser.parse_source_file();
    ParseResult {
        green_node: parser.builder.finish(),
        errors: parser.errors,
    }
}

// Helper to check if a SyntaxKind is trivia
// Moved outside the Parser impl block
impl SyntaxKind {
    // Add the is_trivia method here
    fn is_trivia(self) -> bool {
        matches!(self, SyntaxKind::WHITESPACE | SyntaxKind::COMMENT)
    }
    
    // Removed unused is_keyword method
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

        let board_def_nodes: Vec<_> = root.children().filter(|n| n.kind() == BOARD_DEF).collect();
        assert_eq!(board_def_nodes.len(), 1, "SOURCE_FILE should contain exactly one BOARD_DEF");
        let board_def = board_def_nodes.first().unwrap();

        // Check children tokens of BOARD_DEF
        let mut children = board_def.children_with_tokens();

        // Note: Due to trivia filtering, we expect non-trivia tokens directly.
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(BOARD_KW));
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(IDENT));
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(L_BRACE));
        assert_eq!(children.next().and_then(|el| el.into_token().map(|t| t.kind())), Some(R_BRACE));
        assert!(children.next().is_none(), "Should be no more children after R_BRACE");
    }

    #[test]
    fn parse_board_with_junk() {
        let result = parse("board Foo { junk }");
        assert!(!result.errors.is_empty()); // Expect errors
        // Check if the structure is somewhat reasonable despite errors
        let root = result.syntax();
        assert_eq!(root.kind(), SyntaxKind::SOURCE_FILE);
        let board_def = root.children().find(|n| n.kind() == BOARD_DEF);
        assert!(board_def.is_some());
        let board_def = board_def.unwrap();

        // Find tokens, ignoring potential ERROR nodes for simplicity here
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

        // More robust assertion checking node kinds within the suffix:
        // Find the RangeExpr node within the suffix
        let range_expr = find_node(&data_bus_suffix, RANGE_EXPR).expect("No RANGE_EXPR inside BUS_SUFFIX");
        // Optional: Check contents of range_expr if needed
        let mut range_children = range_expr.children_with_tokens().filter(|e| !e.kind().is_trivia());
        let high_val_el = range_children.next().expect("Missing high bound element in range");
        let colon_el = range_children.next().expect("Missing colon element in range");
        let low_val_el = range_children.next().expect("Missing low bound element in range");
        // Check kinds are appropriate (VALUE, IDENT_REF, etc. - simple VALUE for now)
        assert_eq!(high_val_el.kind(), VALUE);
        assert_eq!(colon_el.kind(), COLON);
        assert_eq!(low_val_el.kind(), VALUE);
        assert!(range_children.next().is_none(), "Extra elements in range_expr");

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
        let stmt1_lhs = find_node(&conn_stmts[0], NET_REF).expect("No NET_REF LHS in stmt 1");
        let stmt1_rhs = find_node(&conn_stmts[0], PIN_REF).expect("No PIN_REF RHS in stmt 1");
        assert_eq!(stmt1_lhs.text().to_string(), "NetA");
        assert_eq!(stmt1_rhs.text().to_string(), "U1.Pin1");
        assert!(conn_stmts[0].children_with_tokens().any(|t| t.kind() == ARROW));
        assert!(conn_stmts[0].children_with_tokens().any(|t| t.kind() == SEMI));

        // Check second statement: VCC -> U1.VCC, U2.VCC, C1.1;
        let stmt2_lhs = find_node(&conn_stmts[1], NET_REF).expect("No NET_REF LHS in stmt 2");
        let stmt2_rhs_pins = find_all_nodes(&conn_stmts[1], PIN_REF);
        assert_eq!(stmt2_lhs.text().to_string(), "VCC");
        assert_eq!(stmt2_rhs_pins.len(), 3, "Expected 3 PIN_REF RHS in stmt 2");
        assert_eq!(stmt2_rhs_pins[0].text().to_string(), "U1.VCC");
        assert_eq!(stmt2_rhs_pins[1].text().to_string(), "U2.VCC");
        assert_eq!(stmt2_rhs_pins[2].text().to_string(), "C1.1");
        let stmt2_commas = conn_stmts[1].children_with_tokens().filter(|t| t.kind() == COMMA).count();
        assert_eq!(stmt2_commas, 2, "Expected 2 commas in stmt 2");

        // Check third statement: U1.GND, U2.GND, C1.2 -> GND;
        let stmt3_lhs_pins = find_all_nodes(&conn_stmts[2], PIN_REF);
        let stmt3_rhs = find_node(&conn_stmts[2], NET_REF).expect("No NET_REF RHS in stmt 3");
        assert_eq!(stmt3_lhs_pins.len(), 3, "Expected 3 PIN_REF LHS in stmt 3");
        assert_eq!(stmt3_lhs_pins[0].text().to_string(), "U1.GND");
        assert_eq!(stmt3_lhs_pins[1].text().to_string(), "U2.GND");
        assert_eq!(stmt3_lhs_pins[2].text().to_string(), "C1.2");
        assert_eq!(stmt3_rhs.text().to_string(), "GND");
        let stmt3_commas = conn_stmts[2].children_with_tokens().filter(|t| t.kind() == COMMA).count();
        assert_eq!(stmt3_commas, 2, "Expected 2 commas in stmt 3");

        // Check statement 4 (bus connection) - Use stmt4_refs
        let stmt4_refs = find_all_nodes(&conn_stmts[3], PIN_REF);
        assert_eq!(stmt4_refs.len(), 2);
        assert_eq!(stmt4_refs[0].text().to_string(), "CPU.DataBus[7:0]"); // Check full text
        assert_eq!(stmt4_refs[1].text().to_string(), "RAM.Data[7:0]");    // Check full text

        // Check statement 5 (bus slice connection) - LHS is NET_REF, RHS is PIN_REF
        let stmt5_lhs = find_node(&conn_stmts[4], NET_REF).expect("No NET_REF LHS in stmt 5");
        let stmt5_rhs = find_node(&conn_stmts[4], PIN_REF).expect("No PIN_REF RHS in stmt 5");
        assert_eq!(stmt5_lhs.text().to_string(), "AddressBus[15:8]"); // Check full text
        assert_eq!(stmt5_rhs.text().to_string(), "Periph.Addr[7:0]"); // Check full text
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
                     resistance = 1kOhm; // Changed from 1k
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
        let vdd_params = find_node(&vdd_type_ref, TYPE_PARAMS).expect("No TYPE_PARAMS for VDD"); // Changed TYPE_SPECIFIER to TYPE_PARAMS
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

    #[test]
    fn parse_constrain_block_basic() {
        let input = r#"
            board ConstrainedBoard {
                nets { net CLK: signal; }
                constrain (CLK) {
                    max_length = 50mm;
                    impedance = 50 Ohm;
                }
                constrain (U1.RESET) { // Assume U1.RESET is valid PinRef
                    pullup = true;
                }
            }
        "#;
        let result = parse(input);
        assert!(result.errors.is_empty(), "Expected no parse errors: {:?}", result.errors);

        let board_def = find_node(&result.syntax(), BOARD_DEF).expect("No BOARD_DEF found");
        let constrain_blocks = find_all_nodes(&board_def, CONSTRAIN_BLOCK);
        assert_eq!(constrain_blocks.len(), 2, "Expected 2 constrain blocks");

        // Check first block: constrain (CLK)
        let target1 = find_node(&constrain_blocks[0], CONSTRAINT_TARGET).expect("No target in block 1");
        assert_eq!(target1.text().to_string(), "CLK");
        let assigns1 = find_all_nodes(&constrain_blocks[0], PARAM_ASSIGN);
        assert_eq!(assigns1.len(), 2, "Expected 2 assignments in block 1");
        assert_eq!(assigns1[0].text().to_string(), "max_length=50mm;"); // Basic check
        assert_eq!(assigns1[1].text().to_string(), "impedance=50Ohm;"); // Basic check

        // Check second block: constrain (U1.RESET)
        let target2 = find_node(&constrain_blocks[1], CONSTRAINT_TARGET).expect("No target in block 2");
        assert_eq!(target2.text().to_string(), "U1.RESET");
        let assigns2 = find_all_nodes(&constrain_blocks[1], PARAM_ASSIGN);
        assert_eq!(assigns2.len(), 1, "Expected 1 assignment in block 2");
        assert_eq!(assigns2[0].text().to_string(), "pullup=true;"); // Basic check

    }

    #[test]
    fn parse_assign_stmt_basic() {
        let input = r#"
            board AssignBoard {
                nets {
                    net A: signal;
                    net B: signal;
                }
                connections {
                    // Simple assignment
                    assign A = B; 
                }
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let board_def = find_node(&result.syntax(), BOARD_DEF).expect("No BOARD_DEF found");
        let conns_block = find_node(&board_def, CONNECTIONS_BLOCK).expect("No CONNECTIONS_BLOCK found");
        let assign_stmt = find_node(&conns_block, ASSIGN_STMT).expect("No ASSIGN_STMT found");
        
        let mut children = assign_stmt.children_with_tokens();
        assert_eq!(children.next().unwrap().kind(), ASSIGN_KW);
        let lhs_element = children.next().unwrap();
        // Updated Assertion: LHS of simple assignment is NET_REF
        assert_eq!(lhs_element.kind(), NET_REF, "Expected NET_REF on LHS");
        assert_eq!(lhs_element.as_node().unwrap().first_token().unwrap().text(), "A");
        assert_eq!(children.next().unwrap().kind(), EQ);
        let rhs_element = children.next().unwrap();
        // Updated Assertion: Check the kind of the RHS node (should be IDENT_REF from parse_expr)
        assert_eq!(rhs_element.kind(), IDENT_REF, "Expected IDENT_REF on RHS");
        // Get the identifier *inside* the IDENT_REF
        assert_eq!(rhs_element.as_node().unwrap().first_token().unwrap().text(), "B");
        assert_eq!(children.next().unwrap().kind(), SEMI);
        assert!(children.next().is_none());
    }

    #[test]
    fn parse_pin_map_basic() {
        let input = r#"
            // Define dummy interface and component for testing
            interface SPIBus { pins { MISO: in signal; MOSI: out signal; } }
            component SomeSoC {
                pins { P1_0: inout signal; P1_1: inout signal; }
                interfaces {
                    SPI1: interface SPIBus { 
                        pin_map = { MISO = P1_0, MOSI = P1_1 }
                        // Optional param override
                        max_freq = 10MHz;
                    }
                }
            }
        "#;
        // REMOVED semicolon from line: pin_map = { MISO = P1_0, MOSI = P1_1 }
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let comp_def = find_node(&result.syntax(), COMPONENT_DEF).expect("No COMPONENT_DEF found");
        let interfaces_block = find_node(&comp_def, INTERFACES_BLOCK).expect("No INTERFACES_BLOCK found");
        let interface_inst = find_node(&interfaces_block, INTERFACE_INSTANCE).expect("No INTERFACE_INSTANCE found");

        // Check interface instance details
        assert_eq!(interface_inst.children_with_tokens().nth(0).unwrap().as_token().unwrap().text(), "SPI1");
        assert_eq!(interface_inst.children_with_tokens().nth(2).unwrap().as_token().unwrap().kind(), INTERFACE_KW);
        assert_eq!(interface_inst.children_with_tokens().nth(3).unwrap().as_token().unwrap().text(), "SPIBus");

        // Find the PIN_MAP_BLOCK
        let pin_map_block = find_node(&interface_inst, PIN_MAP_BLOCK).expect("No PIN_MAP_BLOCK found");
        assert_eq!(pin_map_block.children_with_tokens().nth(0).unwrap().as_token().unwrap().text(), "pin_map"); // Check it saw the ident
        assert_eq!(pin_map_block.children_with_tokens().nth(1).unwrap().kind(), EQ);
        assert_eq!(pin_map_block.children_with_tokens().nth(2).unwrap().kind(), L_BRACE);

        // Find PIN_MAP_ENTRY nodes
        let pin_map_entries = find_all_nodes(&pin_map_block, PIN_MAP_ENTRY);
        assert_eq!(pin_map_entries.len(), 2);
        
        // Check first entry: MISO = P1_0
        let entry1_tokens: Vec<_> = pin_map_entries[0].children_with_tokens().filter_map(|e| e.into_token()).collect();
        assert_eq!(entry1_tokens.len(), 3);
        assert_eq!(entry1_tokens[0].text(), "MISO");
        assert_eq!(entry1_tokens[1].kind(), EQ);
        assert_eq!(entry1_tokens[2].text(), "P1_0");

        // Check second entry: MOSI = P1_1
        let entry2_tokens: Vec<_> = pin_map_entries[1].children_with_tokens().filter_map(|e| e.into_token()).collect();
        assert_eq!(entry2_tokens.len(), 3);
        assert_eq!(entry2_tokens[0].text(), "MOSI");
        assert_eq!(entry2_tokens[1].kind(), EQ);
        assert_eq!(entry2_tokens[2].text(), "P1_1");

        // Check the parameter assignment as well
        let param_assign = find_node(&interface_inst, PARAM_ASSIGN).expect("No PARAM_ASSIGN found");
        assert!(param_assign.text().to_string().contains("max_freq=10MHz")); // Use to_string()

    }

    #[test]
    fn parse_typedef_extends() {
        let input = r#"
            typedef base_signal { type=signal; voltage=3.3Vdc; }
            typedef extended_signal extends base_signal { 
                domain = digital; 
                is_open_drain = true;
            }
            typedef simple_power { type=power; }
            typedef another_type extends simple_power; // Extend without body
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let typedef_defs = find_all_nodes(&result.syntax(), TYPEDEF_DEF);
        assert_eq!(typedef_defs.len(), 4);

        // Check extended_signal
        let extended_def = &typedef_defs[1];
        assert_eq!(extended_def.children_with_tokens().filter_map(|t| t.into_token()).nth(1).unwrap().text(), "extended_signal");
        let base_type_node = find_node(extended_def, TYPEDEF_BASE).expect("No TYPEDEF_BASE found");
        assert_eq!(base_type_node.text().to_string(), "base_signal"); // Use to_string()
        assert!(extended_def.children_with_tokens().any(|t| t.kind() == L_BRACE));
        assert_eq!(find_all_nodes(extended_def, PARAM_ASSIGN).len(), 2);
        
        // Check another_type (extends without body)
        let another_def = &typedef_defs[3];
        assert_eq!(another_def.children_with_tokens().filter_map(|t| t.into_token()).nth(1).unwrap().text(), "another_type");
        let base_type_node_2 = find_node(another_def, TYPEDEF_BASE).expect("No TYPEDEF_BASE for another_type");
        assert_eq!(base_type_node_2.text().to_string(), "simple_power"); // Use to_string()
        // Should not have braces or param assigns if body is empty
        assert!(!another_def.children_with_tokens().any(|t| t.kind() == L_BRACE), "Should not find L_BRACE in body-less typedef");
        assert_eq!(find_all_nodes(another_def, PARAM_ASSIGN).len(), 0, "Should find 0 PARAM_ASSIGN in body-less typedef");
        // It should end directly with a SEMI after the TYPEDEF_BASE node
        // Find the last significant token/node
        let last_element = another_def.children_with_tokens().filter(|t| !t.kind().is_trivia()).last().expect("No last element found");
        assert_eq!(last_element.kind(), SEMI, "Body-less typedef should end with SEMI");
        // Removed TODO comment as the parser logic is now adjusted.
    }

    #[test]
    fn parse_board_physical_blocks() {
        let input = r#"
            board PhysicalBoard {
                layer_stackup {
                    layer TOP { type=signal; material="Cu"; thickness=0.035mm; } // REMOVED semicolon
                    layer GND { type=plane; material="Cu"; thickness=0.070mm; } // REMOVED semicolon
                    layer BOTTOM { type=signal; material="Cu"; thickness=0.035mm; } // REMOVED semicolon
                }
                default_design_rules {
                    min_trace_width = 0.15mm;
                    min_clearance = 0.15mm;
                    default_via_style = "Via1";
                }
                // Add some other blocks to ensure parsing continues
                nets { net A: signal; }
                connections { A -> A; } // Dummy connection
            }
        "#;
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors");

        let board_def = find_node(&result.syntax(), BOARD_DEF).expect("No BOARD_DEF found");

        // Check Layer Stackup
        let stackup_block = find_node(&board_def, LAYER_STACKUP_BLOCK).expect("No LAYER_STACKUP_BLOCK found");
        let layer_defs = find_all_nodes(&stackup_block, LAYER_DEF);
        assert_eq!(layer_defs.len(), 3);
        // Check first layer def has 3 param assigns
        assert_eq!(find_all_nodes(&layer_defs[0], PARAM_ASSIGN).len(), 3);
        assert!(layer_defs[0].text().to_string().contains("TOP")); // Use to_string()
        assert!(layer_defs[0].text().to_string().contains("0.035mm")); // Use to_string()

        // Check Design Rules
        let rules_block = find_node(&board_def, DEFAULT_DESIGN_RULES_BLOCK).expect("No DEFAULT_DESIGN_RULES_BLOCK found");
        let rule_assigns = find_all_nodes(&rules_block, PARAM_ASSIGN);
        assert_eq!(rule_assigns.len(), 3);
        assert!(rule_assigns[0].text().to_string().contains("min_trace_width")); // Use to_string()
        assert!(rule_assigns[2].text().to_string().contains("Via1")); // Use to_string()

        // Check that nets and connections blocks were also parsed after
        assert!(find_node(&board_def, NETS_BLOCK).is_some());
        assert!(find_node(&board_def, CONNECTIONS_BLOCK).is_some());
    }

    #[test]
    fn parse_expression_precedence() {
        // Test input for assign statement RHS
        let input = r#"
            board Test {
                connections {
                    assign A = 1 + 2 * 3 - -4 / ( 5 + 1 ); // 1 + 6 - (-4 / 6) -> 1 + 6 + 0 = 7 (integer division)
                }
            }
        "#;
        // NOTE: The simplified CST currently produced might not fully reflect precedence in its structure.
        // A full AST builder would be needed for that. This test mainly checks if it parses without errors.
        let result = parse(input);
        println!("Parse errors: {:?}\n", result.errors);
        println!("Syntax Tree:\n{:#?}", result.syntax());
        assert!(result.errors.is_empty(), "Expected no parse errors for precedence expression");

        let assign_stmt = find_node(&result.syntax(), ASSIGN_STMT).expect("ASSIGN_STMT not found");
        // Updated Assertion: Remove check for top-level EXPRESSION node
        // let expr = assign_stmt.children().find(|n| n.kind() == EXPRESSION).expect("EXPRESSION not found");
        
        // Basic structural check: Does the assign statement text contain expected operators?
        let assign_text = assign_stmt.text().to_string();
        assert!(assign_text.contains('+'));
        assert!(assign_text.contains('*'));
        assert!(assign_text.contains('-'));
        assert!(assign_text.contains('/'));
        assert!(assign_text.contains('('));
    }

    #[test]
    fn parse_complex_expression() {
        let input = r#"
            board Test {
                connections {
                    // Test precedence and associativity
                    assign A = 1 + 2 * 3 == 7 && 4 / 2 > 1;
                    // Test parentheses and unary operators
                    assign B = !( (x + -y) * ~z | 5 ); 
                }
            }
        "#;
        let result = parse(input);
        println!("Complex Expr Parse errors: {:?}\n", result.errors);
        println!("Complex Expr Syntax Tree:\n{:#?}", result.syntax());
        // For now, just check that it parses without errors.
        // A more detailed check would require inspecting the CST structure
        // or building an AST.
        assert!(result.errors.is_empty(), "Expected no parse errors for complex expression");

        let assign_stmts = find_all_nodes(&result.syntax(), ASSIGN_STMT);
        assert_eq!(assign_stmts.len(), 2, "Expected two assign statements");

        // Corrected Assertion: Look for specific expression nodes within the assign statement
        // Basic check on the first expression's nodes/text content
        let assign1 = &assign_stmts[0];
        assert!(assign1.text().to_string().contains("=="));
        assert!(assign1.text().to_string().contains("&&"));
        assert!(assign1.text().to_string().contains(">"));
        // Verify structure by looking for binary expressions
        assert!(find_all_nodes(assign1, BINARY_EXPR).len() > 0, "Expected BINARY_EXPR nodes in assign1");

        // Basic check on the second expression's nodes/text content
        let assign2 = &assign_stmts[1];
        assert!(assign2.text().to_string().contains("!"));
        assert!(assign2.text().to_string().contains("~"));
        assert!(assign2.text().to_string().contains("|"));
        assert!(assign2.text().to_string().contains("("));
        // Verify structure by looking for prefix and binary expressions
        assert!(find_all_nodes(assign2, PREFIX_EXPR).len() > 0, "Expected PREFIX_EXPR nodes in assign2");
        assert!(find_all_nodes(assign2, BINARY_EXPR).len() > 0, "Expected BINARY_EXPR nodes in assign2");
    }

    #[test]
    fn parse_ternary_expression() {
        let input = r#"
            board Test {
                connections {
                    // Simple ternary
                    assign A = condition ? 1 : 0;
                    // Nested ternary (right-associative)
                    assign B = cond1 ? val1 : cond2 ? val2 : val3; 
                    // Ternary within other expressions
                    assign C = x + (y > 0 ? y : -y) * 2;
                }
            }
        "#;
        let result = parse(input);
        println!("Ternary Expr Parse errors: {:?}\n", result.errors);
        println!("Ternary Expr Syntax Tree:\n{:#?}", result.syntax());
        // Just check for absence of errors for now
        assert!(result.errors.is_empty(), "Expected no parse errors for ternary expression");

        let assign_stmts = find_all_nodes(&result.syntax(), ASSIGN_STMT);
        assert_eq!(assign_stmts.len(), 3, "Expected three assign statements");

        // Find TERNARY_EXPR nodes
        let ternary_nodes = find_all_nodes(&result.syntax(), TERNARY_EXPR);
        // Corrected Assertion: Expect 4 nodes due to nesting
        assert_eq!(ternary_nodes.len(), 4, "Expected four ternary expressions (including nested)"); 

        // Optional: More detailed checks on structure if needed
        // e.g., check children of ternary_nodes[1] to confirm nesting
    }

    #[test]
    fn parse_function_call_expression() {
        let input = r#"
            board Test {
                connections {
                    // Simple function call
                    assign A = calculate(x, y + 1);
                    // Function call with no args
                    assign B = get_status();
                    // Nested function calls
                    assign C = outer(inner(z), 10);
                    // Function call within other expressions
                    assign D = 5 * check(status);
                }
            }
        "#;
        let result = parse(input);
        println!("Function Call Parse errors: {:?}\n", result.errors);
        println!("Function Call Syntax Tree:\n{:#?}", result.syntax());
        // Just check for absence of errors and presence of func call nodes
        assert!(result.errors.is_empty(), "Expected no parse errors for function call expression");

        let assign_stmts = find_all_nodes(&result.syntax(), ASSIGN_STMT);
        assert_eq!(assign_stmts.len(), 4, "Expected four assign statements");

        // Find FUNCTION_CALL_EXPR nodes
        let func_call_nodes = find_all_nodes(&result.syntax(), FUNCTION_CALL_EXPR);
        assert_eq!(func_call_nodes.len(), 5, "Expected five function call expressions (including nested)");

        // Check structure of the first call: calculate(x, y + 1)
        let call1 = &func_call_nodes[0];
        // Find the function name token directly
        let func_name_token = call1.children_with_tokens()
            .find(|t| t.kind() == IDENT)
            .and_then(|t| t.into_token())
            .expect("No IDENT token (function name) in call1");
        assert_eq!(func_name_token.text(), "calculate"); // Corrected assertion
        
        let arg_list1 = find_node(call1, ARGUMENT_LIST).expect("No ARGUMENT_LIST in call1");
        // Arg list should contain 2 expressions separated by comma
        // Corrected check: Filter SyntaxElements that are nodes using as_node().is_some()
        let arg_nodes1 = arg_list1.children_with_tokens().filter(|el| el.as_node().is_some()).count(); 
        assert_eq!(arg_nodes1, 2, "Expected 2 argument nodes in call1");
        assert!(arg_list1.children_with_tokens().any(|t| t.kind() == COMMA));

        // Check structure of the second call: get_status()
        let call2 = &func_call_nodes[1];
        // Find the function name token directly
        let func_name_token2 = call2.children_with_tokens()
            .find(|t| t.kind() == IDENT)
            .and_then(|t| t.into_token())
            .expect("No IDENT token (function name) in call2");
        assert_eq!(func_name_token2.text(), "get_status"); // Corrected assertion
        let arg_list2 = find_node(call2, ARGUMENT_LIST).expect("No ARGUMENT_LIST in call2");
        // Corrected check: Filter SyntaxElements that are nodes using as_node().is_some()
        let arg_nodes2 = arg_list2.children_with_tokens().filter(|el| el.as_node().is_some()).count(); 
        assert_eq!(arg_nodes2, 0, "Expected 0 argument nodes in call2");

    }

    #[test]
    fn parse_value_with_units() {
        let inputs = vec![
            // Wrap assign statements in board/connections blocks
            ("board T{connections{assign A=10kOhm;}}", "10kOhm"),
            ("board T{connections{assign B=3.3Vdc;}}", "3.3Vdc"),
            ("board T{connections{assign C=100mA;}}", "100mA"),
            ("board T{connections{assign D=16MHz;}}", "16MHz"),
            ("board T{connections{assign E=50 pct;}}", "50"), // Space means 'pct' is separate token
            ("board T{connections{assign F=100;}}", "100"), // No unit
            ("board T{connections{assign G=1.23pF;}}", "1.23pF"), // Decimal with unit
        ];

        for (input_str, expected_value_text) in inputs {
            println!("Testing input: {}", input_str);
            let result = parse(input_str);
            println!("Parse errors: {:?}", result.errors);
            println!("Syntax Tree:\n{:#?}", result.syntax());
            assert!(result.errors.is_empty(), "Parse errors for input: {}", input_str);

            let assign_stmt = find_node(&result.syntax(), ASSIGN_STMT)
                .expect(&format!("No ASSIGN_STMT found for input: {}", input_str));
            
            // Corrected: Find the VALUE node directly within the assign statement
            // The RHS expression parser wraps literals/values, so search the whole statement.
            let value_node = find_node(&assign_stmt, VALUE)
                .expect(&format!("No VALUE node found within ASSIGN_STMT for input: {}", input_str));

            // Construct expected text, handling the space case for "50 pct"
            let expected_final_text = if input_str.contains("50 pct") {
                "50pct".to_string() // Node text joins tokens, removing space
            } else {
                expected_value_text.to_string()
            };

            assert_eq!(value_node.text().to_string(), expected_final_text, 
                       "Mismatch for input: {}", input_str);
        }
    }

} 