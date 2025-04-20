// Content for bhdl-parser/src/items.rs
// Will be populated in the next step.

use crate::syntax::SyntaxKind;
use super::core::{Parser, SyntaxKindExt};

impl<'t> Parser<'t> {
    // Item parsing functions (parse_pin_decl, parse_net_decl, parse_param_assign, etc.)

    // Parses a single pin declaration: pin <name>[bus_suffix] [: <type>] [= <default>];
    pub(crate) fn parse_pin_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_DECL.into());
        self.expect(SyntaxKind::PIN_KW); // Consume PIN_KW
        self.expect(SyntaxKind::IDENT); // Pin name

        // Optional bus suffix parsed HERE (after name, before colon)
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }

        // Optional type annotation
        if self.eat(SyntaxKind::COLON) {
            self.parse_type_ref();
        }

        // Optional default value (not typical for pins, but maybe for future flexibility?)
        if self.eat(SyntaxKind::EQ) {
            self.parse_expr(0); // Parse the default value expression
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node(); // Finish PIN_DECL
    }

    // Parses the bus suffix: [expr] or [expr:expr]
    pub(crate) fn parse_bus_suffix(&mut self) {
        // Assumes L_BRACKET has been consumed by the caller
        // NOTE: Caller MUST eat L_BRACKET first before calling this
        // We need to manually start/finish the node if we call this directly
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

    // Parses an interface instance declaration: interface <instance_name>: <interface_type_name> { ... };
    pub(crate) fn parse_interface_inst(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_INSTANCE.into());
        self.expect(SyntaxKind::INTERFACE_KW); // Expect INTERFACE_KW first
        self.expect(SyntaxKind::IDENT); // Instance Name (e.g., MEM, SPI1)
        self.expect(SyntaxKind::COLON); // Required colon
        self.expect(SyntaxKind::IDENT); // Interface Type Name (e.g., DDR_Interface, SPI)

        // Parse block with pin_map and optional parameter overrides
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                // Check for IDENT "pin_map" or PARAMETER_KW
                Some(SyntaxKind::IDENT) => {
                    // Peek ahead to check if it's pin_map = ...
                    let mut next_non_trivia_pos = self.pos;
                    while self.tokens.get(next_non_trivia_pos).map_or(false, |(k,_)| k.is_trivia()) {
                         next_non_trivia_pos += 1;
                    }
                    let is_pin_map_ident = self.tokens.get(next_non_trivia_pos).map_or(false, |(_, text)| text.as_str() == "pin_map");

                    // Peek even further for the equals sign
                    let mut equals_pos = next_non_trivia_pos + 1;
                    while self.tokens.get(equals_pos).map_or(false, |(k,_)| k.is_trivia()) {
                         equals_pos += 1;
                    }
                    let is_pin_map_assign = self.tokens.get(equals_pos).map_or(false, |(k,_)| *k == SyntaxKind::EQ);


                    if is_pin_map_ident && is_pin_map_assign {
                        // It looks like a pin_map block
                        self.parse_pin_map_block(); // Call helper from blocks.rs
                    } else {
                        // Assume it's a parameter override (which starts with IDENT, not PARAMETER_KW here)
                        self.parse_param_assign_no_kw();
                    }
                }
                // Explicitly check for PARAMETER_KW for parameter assignments
                Some(SyntaxKind::PARAMETER_KW) => {
                    // Parameter assignments within the interface instance body
                    // should NOT require the PARAMETER_KW according to spec examples.
                    self.error("Unexpected 'parameter' keyword inside interface instance body. Parameter overrides should be `name = value;`".to_string());
                    // FIX: Consume the unexpected keyword before attempting recovery
                    self.bump(); // Consume PARAMETER_KW
                    // Attempt recovery by parsing as if keyword wasn't there
                    self.parse_param_assign_no_kw();
                }
                Some(kind) => {
                    self.error(format!("Expected 'pin_map = ...', parameter assignment (`name = value;`), or '}}', found {:?}", kind));
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file inside interface instance block".to_string());
                    break;
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        // Semicolon? Spec doesn't show one, assume no.
        self.builder.finish_node();
    }

    // Parses a pin map entry: LogPin = PhysPin
    pub(crate) fn parse_pin_map_entry(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_MAP_ENTRY.into());
        self.expect(SyntaxKind::IDENT); // Logical Pin Name
        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::IDENT); // Physical Pin Name
        self.builder.finish_node();
    }

    // Parses a single parameter assignment: parameter <name> [: <type>] = <value>;
    pub(crate) fn parse_param_assign(&mut self) { 
        self.builder.start_node(SyntaxKind::PARAM_ASSIGN.into());
        self.expect(SyntaxKind::PARAMETER_KW);
        self.expect(SyntaxKind::IDENT); // Parameter name

        // Optional type annotation
        if self.eat(SyntaxKind::COLON) {
            self.parse_type_ref();
        }

        self.expect(SyntaxKind::EQ); // Expect '='
        self.parse_expr(0); // Parse the value expression
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node(); // Finish PARAM_ASSIGN
    }

    // Helper for param assignments where the keyword isn't expected (e.g., inside layer/design rule/typedef blocks)
    pub(crate) fn parse_param_assign_no_kw(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM_ASSIGN.into());
        self.expect(SyntaxKind::IDENT); // Parameter Name (or rule/property name)
        self.expect(SyntaxKind::EQ);

        // Check for type keywords as direct values before falling back to parse_expr
        match self.peek() {
            Some(SyntaxKind::SIGNAL_KW) |
            Some(SyntaxKind::POWER_KW) |
            Some(SyntaxKind::GROUND_KW) |
            Some(SyntaxKind::CLOCK_KW) |
            Some(SyntaxKind::WIRE_KW) |
            Some(SyntaxKind::TRI_KW) |
            Some(SyntaxKind::TRIREG_KW) |
            Some(SyntaxKind::UWIRE_KW) => {
                // If it's a known type keyword, consume it as the value
                 self.builder.start_node(SyntaxKind::VALUE.into());
                 self.bump(); // Consume the type keyword token
                 self.builder.finish_node(); // Finish the VALUE node
            }
            _ => {
                // Otherwise, parse a general expression
                self.parse_expr(0);
            }
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parses a single port declaration: port <name> : <interface_type>;
    pub(crate) fn parse_port_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PORT_DECL.into());
        self.expect(SyntaxKind::PORT_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COLON);
        self.parse_type_ref(); // Interface type (must be an IDENT for an interface def)
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node(); // Finish PORT_DECL
    }

    // Parses a single net declaration: net <name>[bus_suffix] : <type> [= <default>];
    pub(crate) fn parse_net_decl(&mut self) {
        self.builder.start_node(SyntaxKind::NET_DECL.into());
        self.expect(SyntaxKind::NET_KW);
        self.expect(SyntaxKind::IDENT);

        // Optional bus suffix parsed HERE (after name, before colon)
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }

        self.expect(SyntaxKind::COLON); // Separator
        self.parse_type_ref(); // Parse the type reference

        // Optional default value (not really applicable to nets, but maybe later?)
        if self.eat(SyntaxKind::EQ) {
            self.parse_expr(0); // Parse expression for default value
        }

        self.expect(SyntaxKind::SEMI); // Expect semicolon terminator
        self.builder.finish_node();
    }

    // Parses a type reference: <name> | <name>(<params>)
    pub(crate) fn parse_type_ref(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_REF.into());
        // Allow keywords or identifiers as the base type name
        match self.peek() { // Use peek() here
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
                self.bump(); // Bump the bad token
            }
            None => {
                 self.error("Expected type name, found end of file".to_string());
            }
        }
        // Handle optional parameterized types like signal(foo)
        if self.eat(SyntaxKind::L_PAREN) {
            self.builder.start_node(SyntaxKind::TYPE_PARAMS.into());
            // Simple parse: Expect an IDENT or value, then R_PAREN for now
            match self.peek() {
                Some(SyntaxKind::IDENT) |
                Some(SyntaxKind::NUMBER) |
                Some(SyntaxKind::STRING) => {
                    self.bump();
                }
                _ => {
                     self.error("Expected identifier or literal value inside type parameters".to_string());
                     if self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
                         self.bump_any();
                     }
                }
            }
            self.expect(SyntaxKind::R_PAREN);
            self.builder.finish_node(); // Finish TYPE_PARAMS
        }
        self.builder.finish_node(); // Finish TYPE_REF
    }

    // Parses a component instantiation: component <type> <name> { ... } [;] or component <type> <name>;
    pub(crate) fn parse_component_inst(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENT_INST.into());
        self.expect(SyntaxKind::COMPONENT_KW);
        self.expect(SyntaxKind::IDENT); // Component Type
        self.expect(SyntaxKind::IDENT); // Instance Name

        // Parameters block { ... } or optional semicolon if no params
        if self.eat(SyntaxKind::L_BRACE) {
            self.parse_component_params(); // Calls block parser
            self.expect(SyntaxKind::R_BRACE);
            self.eat(SyntaxKind::SEMI); // Optional semicolon after braces
        } else if self.eat(SyntaxKind::SEMI) {
            // Semicolon is required if no braces
        } else {
            self.error("Expected '{' for parameters or ';' after component instance name".to_string());
            if self.peek().is_some() {
                self.bump_any();
            }
        }
        self.builder.finish_node(); // Finish COMPONENT_INST
    }

    // Revised parsing logic for references (NET_REF, PIN_REF, SIMPLE_IDENT_REF)
    // Accepts the SyntaxKind to use if the reference is just a simple identifier.
    pub(crate) fn parse_ref_revised(&mut self, simple_kind: SyntaxKind) {
        if self.peek() != Some(SyntaxKind::IDENT) {
            self.error("Expected identifier for reference".to_string());
            return;
        }

        let cp = self.builder.checkpoint();
        self.bump(); // Consume the first IDENT

        // Check for dot (pin access: Instance.Pin)
        if self.peek() == Some(SyntaxKind::DOT) {
            self.builder.start_node_at(cp, SyntaxKind::PIN_REF.into());
            self.bump(); // Consume DOT

            // Expect IDENT or NUMBER after dot
            if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::NUMBER) {
                self.bump(); // Consume IDENT or NUMBER

                // Optional bus suffix after dot access
                if self.peek() == Some(SyntaxKind::L_BRACKET) {
                    self.parse_bus_suffix();
                }
                self.builder.finish_node(); // Finish PIN_REF
            } else {
                self.error("Expected identifier or number after '.' in pin reference".to_string());
                self.builder.finish_node();
            }
        // Check for bracket (net with bus suffix: NetName[0])
        } else if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.builder.start_node_at(cp, SyntaxKind::NET_REF.into());
            self.parse_bus_suffix();
            self.builder.finish_node(); // Finish NET_REF

        // Simple identifier reference (Use the provided simple_kind)
        } else {
            self.builder.start_node_at(cp, simple_kind.into());
            self.builder.finish_node();
        }
    }

    // Parses an assign statement: assign LHS = RHS;
    pub(crate) fn parse_assign_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::ASSIGN_STMT.into());
        self.expect(SyntaxKind::ASSIGN_KW);
        self.parse_ref_revised(SyntaxKind::NET_REF); // LHS must be a net reference
        self.expect(SyntaxKind::EQ);
        self.parse_expr(0); // Parse the right-hand side expression
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parses a connection statement: connect LHS OP RHS;
    pub(crate) fn parse_connection_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTION_STMT.into());
        self.expect(SyntaxKind::CONNECT_KW);

        // Parse LHS (one or more refs)
        self.parse_ref_revised(SyntaxKind::SIMPLE_IDENT_REF); // Can be net or pin initially
        while self.eat(SyntaxKind::COMMA) {
            self.parse_ref_revised(SyntaxKind::SIMPLE_IDENT_REF);
        }

        // Expect an arrow or interface connection operator
        if self.eat(SyntaxKind::ARROW) {
            // Parse RHS for ->
            self.parse_ref_revised(SyntaxKind::SIMPLE_IDENT_REF);
            while self.eat(SyntaxKind::COMMA) {
                self.parse_ref_revised(SyntaxKind::SIMPLE_IDENT_REF);
            }
        } else if self.eat(SyntaxKind::IF_CONNECT) {
            // Parse RHS for <=> (likely an interface reference)
            self.parse_ref_revised(SyntaxKind::SIMPLE_IDENT_REF);
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.error("Interface connection operator <=> expects a single target on each side.".to_string());
                 while self.eat(SyntaxKind::COMMA) {
                     self.parse_ref_revised(SyntaxKind::SIMPLE_IDENT_REF);
                 }
            }
        } else {
            self.error(format!("Expected '->' or '<=>' in connection statement, found {:?}", self.peek()));
            while self.peek() != Some(SyntaxKind::SEMI) && self.peek().is_some() {
                self.bump_any();
            }
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node(); // Finish CONNECTION_STMT
    }

    // Parses a range expression used in generate loops: START_EXPR to END_EXPR
    pub(crate) fn parse_range_expr(&mut self) {
        self.builder.start_node(SyntaxKind::RANGE_EXPR.into());
        self.parse_expr(0); // Parse start expression
        self.expect(SyntaxKind::TO_KW);
        self.parse_expr(0); // Parse end expression
        self.builder.finish_node();
    }

    // Parses a layer definition: layer NAME { prop = value; ... }
    pub(crate) fn parse_layer_def(&mut self) {
        self.builder.start_node(SyntaxKind::LAYER_DEF.into());
        self.expect(SyntaxKind::LAYER_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::L_BRACE);
        // Parse assignments inside
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.parse_param_assign_no_kw(); // Properties are key=value
            } else {
                self.error(format!("Expected layer property assignment (identifier = value) or '}}', found {:?}", self.peek()));
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
} 