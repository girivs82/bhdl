// Content for bhdl-parser/src/top_level.rs
// Will be populated in the next step.

use crate::syntax::SyntaxKind;
use super::core::{Parser, SyntaxKindExt};

impl<'t> Parser<'t> {
    // Main parsing entry point
    pub(crate) fn parse_source_file(&mut self) {
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

    // Reconstructed parse_module_def
    pub(crate) fn parse_module_def(&mut self) {
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

    // Reconstructed parse_typedef_def (incorporating 'extends' logic)
    pub(crate) fn parse_typedef_def(&mut self) {
        self.builder.start_node(SyntaxKind::TYPEDEF_DEF.into());
        self.expect(SyntaxKind::TYPEDEF_KW);
        self.expect(SyntaxKind::IDENT); // Type Name

        let mut extends_parsed = false;
        // Optional: extends BaseType
        if self.eat(SyntaxKind::EXTENDS_KW) {
            extends_parsed = true;
            self.builder.start_node(SyntaxKind::TYPEDEF_BASE.into()); // Node for base type
            // Allow IDENT or specific type keywords as the base type
            match self.peek() {
                Some(SyntaxKind::IDENT) |
                Some(SyntaxKind::SIGNAL_KW) |
                Some(SyntaxKind::POWER_KW) |
                Some(SyntaxKind::GROUND_KW) |
                Some(SyntaxKind::CLOCK_KW) |
                Some(SyntaxKind::WIRE_KW) |
                Some(SyntaxKind::TRI_KW) |
                Some(SyntaxKind::TRIREG_KW) |
                Some(SyntaxKind::UWIRE_KW) => {
                    self.bump(); // Consume the identifier or keyword
                }
                _ => {
                    self.error("Expected base type name (identifier or keyword like 'power', 'signal') after 'extends'".to_string());
                    // Recovery: Don't consume if unexpected, let expect(SEMI) or expect(L_BRACE) fail later
                }
            }
            self.builder.finish_node(); // Finish TYPEDEF_BASE
        }

        // Optional body `{...}` or just semicolon after extends
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            if extends_parsed {
                // If extends was parsed, a body is optional but allowed
            } // else: body is required if no extends

            self.expect(SyntaxKind::L_BRACE);
            // Parse assignments inside
            while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
                if self.peek() == Some(SyntaxKind::IDENT) {
                    self.parse_param_assign_no_kw(); // Call item parser
                } else {
                    self.error(format!("Expected type property assignment (identifier = value) or '}}', found {:?}", self.peek()));
                    self.bump_any();
                }
            }
            self.expect(SyntaxKind::R_BRACE);
        } else if extends_parsed {
            // If extends was parsed AND no body follows, expect SEMI
            self.expect(SyntaxKind::SEMI);
        } else {
            // If no extends AND no body, it's an error
            self.error(format!("Expected 'extends' or '{{' after typedef name, found {:?}", self.peek()));
        }

        self.builder.finish_node();
    }

    // Reconstructed parse_import_stmt
    pub(crate) fn parse_import_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_STMT.into());
        self.expect(SyntaxKind::IMPORT_KW);

        // Parse the path (Ident.Ident...)
        self.builder.start_node(SyntaxKind::IMPORT_PATH.into());
        self.expect(SyntaxKind::IDENT);
        while self.eat(SyntaxKind::DOT) {
            if self.peek() == Some(SyntaxKind::L_BRACE) { // Use peek() to skip trivia
                break;
            }
            if !self.eat(SyntaxKind::IDENT) {
                 self.error("Expected identifier or '{' after '.' in import path".to_string());
                 break;
            }
        }
        self.builder.finish_node(); // IMPORT_PATH

        // Parse the target (either simple IDENT implicitly or group { Target, ... })
        if self.eat(SyntaxKind::L_BRACE) {
            self.builder.start_node(SyntaxKind::IMPORT_TARGET_GROUP.into());
            loop {
                self.expect(SyntaxKind::IDENT);
                if !self.eat(SyntaxKind::COMMA) {
                    break;
                }
                if self.peek() == Some(SyntaxKind::R_BRACE) { // Use peek()
                    break;
                }
            }
            self.expect(SyntaxKind::R_BRACE);
            self.builder.finish_node(); // IMPORT_TARGET_GROUP
        } else {
            // No L_BRACE means the last IDENT in path is the target.
            // Create an empty target node for consistency in the AST?
            // It might be better NOT to create an empty node here.
            // The analyzer can figure out the target from the path node.
            // Let's omit the empty IMPORT_TARGET node creation.
        }

        // Optional 'as Alias'
        if self.eat(SyntaxKind::AS_KW) {
            self.builder.start_node(SyntaxKind::ALIAS.into());
            self.expect(SyntaxKind::IDENT);
            self.builder.finish_node();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node(); // IMPORT_STMT
    }

    // Reconstructed parse_component_def
    pub(crate) fn parse_component_def(&mut self) {
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

    // --- Existing Grammar Rule Parsers ---
    pub(crate) fn parse_board_def(&mut self) {
        self.builder.start_node(SyntaxKind::BOARD_DEF.into());
        self.expect(SyntaxKind::BOARD_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::L_BRACE);

        // Parse items inside the board block
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PARAMETERS_KW) => self.parse_parameters_block(),
                Some(SyntaxKind::PORTS_KW) => self.parse_ports_block(),
                Some(SyntaxKind::NETS_KW) => self.parse_nets_block(),
                Some(SyntaxKind::COMPONENTS_KW) => self.parse_components_block(),
                Some(SyntaxKind::CONNECTIONS_KW) => self.parse_connections_block(),
                Some(SyntaxKind::LAYER_STACKUP_KW) => self.parse_layer_stackup_block(),
                Some(SyntaxKind::DEFAULT_DESIGN_RULES_KW) => self.parse_default_design_rules_block(),
                Some(SyntaxKind::CONSTRAIN_KW) => self.parse_constrain_block(),
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_stmt(),
                Some(SyntaxKind::IF_KW) => self.parse_conditional_stmt(),
                // Support direct circuit flow statements (e.g., VCC -> Res(330Ω).1 -> LED.A;)
                Some(SyntaxKind::IDENT) => {
                    // Check if this is a flow statement (name: flow_expr) or direct connection
                    if self.is_flow_statement() {
                        self.parse_flow_stmt();
                    } else {
                        self.parse_connection_expr();
                    }
                }
                Some(kind) => {
                    self.error(format!("Unexpected token inside board definition: {:?}. Expected block keyword, flow statement, or '}}'.", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside board definition block".to_string());
                    break;
                }
            }
        }

        self.skip_trivia();
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses an interface definition: interface NAME { ... }
    pub(crate) fn parse_interface_def(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_DEF.into());
        self.expect(SyntaxKind::INTERFACE_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::L_BRACE);

        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::PARAMETERS_KW => self.parse_parameters_block(),
                SyntaxKind::PINS_KW => self.parse_pins_block(),
                _ => {
                    self.error(format!("Unexpected token inside interface definition: {:?}. Expected parameters, pins, or '}}'.", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node(); // No semicolon after interface def
    }

    // Helper to determine if an IDENT starts a flow statement (name: expr)
    fn is_flow_statement(&self) -> bool {
        // Look ahead to see if IDENT is followed by COLON
        let mut pos = self.pos;
        
        // Skip trivia after IDENT
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        
        // Skip the IDENT itself
        if pos < self.tokens.len() && self.tokens[pos].0 == SyntaxKind::IDENT {
            pos += 1;
        }
        
        // Skip trivia after IDENT
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        
        // Check if next non-trivia token is COLON
        pos < self.tokens.len() && self.tokens[pos].0 == SyntaxKind::COLON
    }

    // Parse a flow statement: name: flow_expr;
    pub(crate) fn parse_flow_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::FLOW_STMT.into());
        self.expect(SyntaxKind::IDENT); // Flow name
        self.expect(SyntaxKind::COLON);
        self.parse_flow_expr(); // Parse the flow expression
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse a flow expression: element |> element |> element
    pub(crate) fn parse_flow_expr(&mut self) {
        self.builder.start_node(SyntaxKind::FLOW_EXPR.into());
        self.parse_expr(0); // Parse as general expression which handles flow operators
        self.builder.finish_node();
    }

    // Parse a direct connection expression: VCC -> Res(330Ω).1 -> LED.A;
    pub(crate) fn parse_connection_expr(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());
        self.parse_expr(0); // Parse as general expression which handles connection operators
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse a generate statement: generate for var in range { ... }
    pub(crate) fn parse_generate_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::GENERATE_STMT.into());
        self.expect(SyntaxKind::GENERATE_KW);
        self.expect(SyntaxKind::FOR_KW);
        self.expect(SyntaxKind::IDENT); // Loop variable
        self.expect(SyntaxKind::IN_KW);
        self.parse_range_expr(); // Range expression
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse statements inside generate block
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    if self.is_flow_statement() {
                        self.parse_flow_stmt();
                    } else {
                        self.parse_connection_expr();
                    }
                }
                Some(kind) => {
                    self.error(format!("Unexpected token inside generate block: {:?}. Expected connection statement or '}}'.", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside generate block".to_string());
                    break;
                }
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse a conditional statement: if (condition) { ... } else { ... }
    pub(crate) fn parse_conditional_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::CONDITIONAL_STMT.into());
        self.expect(SyntaxKind::IF_KW);
        self.expect(SyntaxKind::L_PAREN);
        self.parse_expr(0); // Parse condition
        self.expect(SyntaxKind::R_PAREN);
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse statements inside if block
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    if self.is_flow_statement() {
                        self.parse_flow_stmt();
                    } else if self.is_assignment_statement() {
                        self.parse_assignment_expr();
                    } else {
                        self.parse_connection_expr();
                    }
                }
                Some(kind) => {
                    self.error(format!("Unexpected token inside if block: {:?}. Expected statement or '}}'.", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside if block".to_string());
                    break;
                }
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        
        // Optional else block
        if self.eat(SyntaxKind::ELSE_KW) {
            self.expect(SyntaxKind::L_BRACE);
            
            // Parse statements inside else block
            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_BRACE) => break,
                    Some(SyntaxKind::IDENT) => {
                        if self.is_flow_statement() {
                            self.parse_flow_stmt();
                        } else if self.is_assignment_statement() {
                            self.parse_assignment_expr();
                        } else {
                            self.parse_connection_expr();
                        }
                    }
                    Some(kind) => {
                        self.error(format!("Unexpected token inside else block: {:?}. Expected statement or '}}'.", kind));
                        self.bump_any();
                    }
                    None => {
                        self.error("Unexpected end of file inside else block".to_string());
                        break;
                    }
                }
            }
            
            self.expect(SyntaxKind::R_BRACE);
        }
        
        self.builder.finish_node();
    }

    // Helper to determine if an IDENT starts an assignment statement (name = expr)
    fn is_assignment_statement(&self) -> bool {
        // Look ahead to see if IDENT is followed by EQ
        let mut pos = self.pos;
        
        // Skip trivia after IDENT
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        
        // Skip the IDENT itself
        if pos < self.tokens.len() && self.tokens[pos].0 == SyntaxKind::IDENT {
            pos += 1;
        }
        
        // Skip trivia after IDENT
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        
        // Check if next non-trivia token is EQ
        pos < self.tokens.len() && self.tokens[pos].0 == SyntaxKind::EQ
    }

    // Parse an assignment expression: name = expr;
    pub(crate) fn parse_assignment_expr(&mut self) {
        self.builder.start_node(SyntaxKind::ASSIGN_STMT.into());
        self.expect(SyntaxKind::IDENT); // Variable name
        self.expect(SyntaxKind::EQ);
        self.parse_expr(0); // Parse the assignment value
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

} 