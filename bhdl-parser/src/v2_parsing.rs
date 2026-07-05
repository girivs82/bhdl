// BHDL v2.0 Parsing Methods
// This file contains parsing methods specific to v2.0 flow-based syntax

use crate::syntax::SyntaxKind;
use super::core::{Parser, SyntaxKindExt};

impl<'t> Parser<'t> {
    /// Parse power declaration: power VIN = 12V @ 1A |> stage1 |> stage2;
    pub(crate) fn parse_power_decl(&mut self) {
        self.builder.start_node(SyntaxKind::POWER_DECL.into());
        self.expect(SyntaxKind::POWER_KW);
        self.expect(SyntaxKind::IDENT); // Power domain name

        if self.peek() == Some(SyntaxKind::EQ) {
            self.bump();
            self.parse_power_spec();
        }

        // Optional stage chain: |> stage1 |> stage2 |> ...
        if self.peek() == Some(SyntaxKind::FLOW_OP) {
            self.builder.start_node(SyntaxKind::POWER_STAGE_CHAIN.into());
            while self.peek() == Some(SyntaxKind::FLOW_OP) {
                self.bump(); // consume |>
                self.builder.start_node(SyntaxKind::STAGE_NAME.into());
                self.expect(SyntaxKind::IDENT); // stage name
                // Optional parameters: |> input_filtering(max_ripple: 50mV)
                if self.peek() == Some(SyntaxKind::L_PAREN) {
                    self.parse_intent_params();
                }
                self.builder.finish_node();
            }
            self.builder.finish_node();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse ground declaration: ground GND;
    pub(crate) fn parse_ground_decl(&mut self) {
        self.builder.start_node(SyntaxKind::GROUND_DECL.into());
        self.expect(SyntaxKind::GROUND_KW);
        self.expect(SyntaxKind::IDENT); // Ground domain name
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse power specification: 12V @ 1A (voltage may be negative —
    /// `power VEE = -12V @ 50mA;` is how a ±supply's negative rail is
    /// declared).
    fn parse_power_spec(&mut self) {
        // Parse voltage (optional leading sign)
        if self.peek() == Some(SyntaxKind::MINUS) {
            self.bump();
        }
        self.expect(SyntaxKind::NUMBER);
        if self.peek_unit_token() {
            self.bump(); // Consume unit
        }
        
        // Optional current spec
        if self.peek() == Some(SyntaxKind::AT) {
            self.bump();
            self.expect(SyntaxKind::NUMBER);
            if self.peek_unit_token() {
                self.bump(); // Consume unit
            }
        }
    }
    
    /// Parse connection or flow statement
    pub(crate) fn parse_connection_or_flow_stmt(&mut self) {
        use crate::v2_fixes::NamedDeclarationType;
        
        match self.is_v2_named_declaration() {
            NamedDeclarationType::FlowStatement => {
                self.parse_flow_stmt();
            }
            NamedDeclarationType::InterfaceInstance => {
                // Parse as interface instance (not a connection)
                self.parse_interface_instance();
            }
            _ => {
                // Regular connection statement
                self.parse_v2_connection_expr();
            }
        }
    }
    
    /// Parse v2.0 connection expression: VIN -> reg: LM7805().IN;
    /// Now also supports: C1.1 -> FB.top where trace_length < 10mm;
    pub(crate) fn parse_v2_connection_expr(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());
        
        // Parse the connection expression directly without wrapping in BINARY_EXPR
        // The expression parser handles binary operators and named handles internally
        self.parse_expr(0);
        
        // Check for optional where clause for connection constraints
        if self.peek() == Some(SyntaxKind::WHERE_KW) {
            self.parse_connection_constraint();
        }
        
        // Check for optional intent clause
        if self.has_intent_clause() {
            self.parse_intent_clause();
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse flow statement: power_flow: source |> regulation |> distribution;
    pub(crate) fn parse_flow_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::FLOW_STMT.into());
        self.expect(SyntaxKind::IDENT); // Flow name
        self.expect(SyntaxKind::COLON);
        
        // Parse flow expression
        self.builder.start_node(SyntaxKind::FLOW_EXPR.into());
        self.parse_expr(0);
        self.builder.finish_node();
        
        // Check for optional intent clause
        if self.has_intent_clause() {
            self.parse_intent_clause();
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse interface instance: name: TypeName(params);
    ///
    /// Also handles the component-declaration form used inside
    /// `expansion { }` blocks with an optional trailing P&R layout
    /// intent clause:
    ///
    ///     C_vcc: Cap(100nF) for high_freq_bypass(rail: VCC, ...);
    ///
    /// The `for INTENT(...)` clause attaches to the COMPONENT_INST node
    /// so the analyzer can lower it to a typed `LayoutIntent`.
    fn parse_interface_instance(&mut self) {
        // In v2.0, interface instances are just named type instantiations
        self.builder.start_node(SyntaxKind::COMPONENT_INST.into());
        self.expect(SyntaxKind::IDENT); // Instance name
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT); // Type name

        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list();
        }

        // Optional P&R layout-intent clause (expansion-block form).
        if self.has_intent_clause() {
            self.parse_intent_clause();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse import statement: 
    /// - import path.to.module;
    /// - import { Item1, Item2 } from "path/to/file.bhdl";
    pub(crate) fn parse_import_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_STMT.into());
        self.expect(SyntaxKind::IMPORT_KW);
        
        // Check for destructuring import { ... } from "..."
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            // Parse destructuring import
            self.parse_import_destructuring();
            
            // Expect 'from' - it's lexed as IDENT, not FROM_KW
            self.skip_trivia();
            if self.peek() == Some(SyntaxKind::IDENT) {
                // Check if it's actually "from"
                let is_from = self.peek_text().map(|t| t == "from").unwrap_or(false);
                if is_from {
                    self.bump(); // Consume "from"
                } else {
                    self.error("Expected 'from' after destructuring import".to_string());
                }
            } else if self.peek() == Some(SyntaxKind::FROM_KW) {
                self.bump(); // In case FROM_KW is added to lexer later
            } else {
                self.error("Expected 'from' after destructuring import".to_string());
            }
            
            // Parse string literal path
            self.skip_trivia();
            self.builder.start_node(SyntaxKind::IMPORT_PATH.into());
            self.expect(SyntaxKind::STRING);
            self.builder.finish_node();
        } else {
            // Parse simple import path
            self.builder.start_node(SyntaxKind::IMPORT_PATH.into());
            self.expect(SyntaxKind::IDENT);
            
            while self.peek() == Some(SyntaxKind::DOT) {
                self.bump();
                self.expect(SyntaxKind::IDENT);
            }
            
            self.builder.finish_node();
            
            // Optional alias
            if self.peek() == Some(SyntaxKind::AS_KW) {
                self.bump();
                self.builder.start_node(SyntaxKind::ALIAS.into());
                self.expect(SyntaxKind::IDENT);
                self.builder.finish_node();
            }
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse import destructuring: { Item1, Item2, Item3 }
    fn parse_import_destructuring(&mut self) {
        self.builder.start_node(SyntaxKind::IMPORT_TARGET_GROUP.into());
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse comma-separated list of identifiers
        loop {
            self.skip_trivia();
            
            if self.peek() == Some(SyntaxKind::R_BRACE) {
                break;
            }
            
            self.builder.start_node(SyntaxKind::IMPORT_TARGET.into());
            // In import context, keywords should be treated as identifiers
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump();
            } else if self.is_contextual_keyword() {
                // Accept keywords as identifiers in import context
                self.bump();
            } else {
                self.error("Expected identifier or keyword in import list".to_string());
            }
            self.builder.finish_node();
            
            // Check for comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
                // Allow trailing comma
                if self.peek() == Some(SyntaxKind::R_BRACE) {
                    break;
                }
            } else if self.peek() != Some(SyntaxKind::R_BRACE) {
                self.error("Expected ',' or '}' in import list".to_string());
                break;
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse entity parameters: (value: resistance, package: string = "0805")
    pub(crate) fn parse_entity_parameters(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM_LIST.into());
        self.expect(SyntaxKind::L_PAREN);
        
        while self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
            self.skip_trivia();
            
            // Start a node for each parameter
            self.builder.start_node(SyntaxKind::PARAM_DECL.into());
            
            // Parameter name — accepts contextual keywords like `package`
            // (a valid param name; only special as a layout-block head).
            self.expect_name();
            self.expect(SyntaxKind::COLON);

            // Parameter type
            self.parse_type_ref();
            
            // Optional default value
            if self.peek() == Some(SyntaxKind::EQ) {
                self.bump();
                self.parse_expr(0);
            }
            
            // Finish the parameter node
            self.builder.finish_node();
            
            // Check for comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else if self.peek() != Some(SyntaxKind::R_PAREN) {
                self.error("Expected ',' or ')' in parameter list".to_string());
                break;
            }
        }
        
        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }
    
    /// Parse parameter list for instantiation: (10k, "0805")
    pub(crate) fn parse_param_list(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM_LIST.into());
        self.expect(SyntaxKind::L_PAREN);
        
        while self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
            // Could be named or positional parameter
            if self.peek_nth(1) == Some(SyntaxKind::EQ) {
                // Named parameter
                self.builder.start_node(SyntaxKind::PARAM_ASSIGN.into());
                self.expect(SyntaxKind::IDENT);
                self.expect(SyntaxKind::EQ);
                self.parse_expr(0);
                self.builder.finish_node();
            } else {
                // Positional parameter
                self.parse_expr(0);
            }
            
            // Check for comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else if self.peek() != Some(SyntaxKind::R_PAREN) {
                self.error("Expected ',' or ')' in parameter list".to_string());
                break;
            }
        }
        
        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }
    
    /// Check if the current token is a unit token
    fn peek_unit_token(&self) -> bool {
        match self.peek() {
            Some(kind) => matches!(kind,
                SyntaxKind::UNIT_IDENTIFIER |
                // Voltage units
                SyntaxKind::V_UNIT | SyntaxKind::MV_UNIT | SyntaxKind::UV_UNIT |
                // Current units  
                SyntaxKind::A_UNIT | SyntaxKind::MA_UNIT | SyntaxKind::UA_UNIT |
                // Resistance units
                SyntaxKind::OHM_UNIT | SyntaxKind::KOHM_UNIT | SyntaxKind::MOHM_UNIT |
                // Capacitance units
                SyntaxKind::F_UNIT | SyntaxKind::UF_UNIT | SyntaxKind::NF_UNIT | SyntaxKind::PF_UNIT |
                // Frequency units
                SyntaxKind::HZ_UNIT | SyntaxKind::KHZ_UNIT | SyntaxKind::MHZ_UNIT | SyntaxKind::GHZ_UNIT |
                // Power units
                SyntaxKind::W_UNIT | SyntaxKind::MW_UNIT | SyntaxKind::UW_UNIT |
                // Time units
                SyntaxKind::S_UNIT | SyntaxKind::MS_UNIT | SyntaxKind::US_UNIT | SyntaxKind::NS_UNIT
            ),
            None => false,
        }
    }
    
    /// Peek at the nth token (0 = current)
    pub(crate) fn peek_nth(&self, n: usize) -> Option<SyntaxKind> {
        let mut pos = self.pos + n;
        while pos < self.tokens.len() && self.tokens[pos].0.is_trivia() {
            pos += 1;
        }
        if pos < self.tokens.len() {
            Some(self.tokens[pos].0)
        } else {
            None
        }
    }
    
    /// Check if the current token is a keyword that can be treated as an identifier in certain contexts
    pub(crate) fn is_contextual_keyword(&self) -> bool {
        match self.peek() {
            Some(kind) => matches!(kind,
                // Common type keywords that can be imported as type names
                SyntaxKind::POWER_KW | SyntaxKind::SIGNAL_KW | SyntaxKind::GROUND_KW |
                SyntaxKind::SWITCH_KW | SyntaxKind::FEEDBACK_KW
            ),
            None => false,
        }
    }
    
    /// Parse connection constraint: where trace_length < 10mm, impedance = 50Ω
    pub(crate) fn parse_connection_constraint(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTION_CONSTRAINT.into());
        self.expect(SyntaxKind::WHERE_KW);
        
        self.builder.start_node(SyntaxKind::CONSTRAINT_LIST.into());
        
        // Parse first constraint
        self.parse_constraint_item();
        
        // Parse additional constraints separated by commas
        while self.peek() == Some(SyntaxKind::COMMA) {
            self.bump();
            self.parse_constraint_item();
        }
        
        self.builder.finish_node(); // CONSTRAINT_LIST
        self.builder.finish_node(); // CONNECTION_CONSTRAINT
    }
    
    /// Parse individual constraint: trace_length < 10mm
    fn parse_constraint_item(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRAINT_ITEM.into());
        
        // Parse constraint expression (could be comparison, assignment, or simple identifier)
        self.parse_expr(0);
        
        self.builder.finish_node();
    }
    
    /// Parse with block: with routing(impedance = 50Ω) { connections }
    pub(crate) fn parse_with_block(&mut self) {
        self.builder.start_node(SyntaxKind::WITH_BLOCK.into());
        self.expect(SyntaxKind::WITH_KW);
        
        // Parse the constraint type (e.g., routing, power)
        self.expect(SyntaxKind::IDENT);
        
        // Parse optional parameters
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list();
        }
        
        // Parse the block
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse connections or nested with blocks
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::WITH_KW) {
                self.parse_with_block();
            } else if self.peek() == Some(SyntaxKind::GENERATE_KW) {
                self.parse_generate_block();
            } else {
                // Regular connection
                self.parse_connection_or_flow_stmt();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
}