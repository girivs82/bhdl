// Content for bhdl-parser/src/blocks.rs
// Will be populated in the next step.

use crate::syntax::SyntaxKind;
use super::core::{Parser, SyntaxKindExt};

impl<'t> Parser<'t> {
    // Block parsing functions

    // Parses a block of pin declarations: pins { ... }
    pub(crate) fn parse_pins_block(&mut self) {
        self.builder.start_node(SyntaxKind::PINS_BLOCK.into());
        self.expect(SyntaxKind::PINS_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PIN_KW) => self.parse_pin_decl(), // Expect PIN_KW
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_for_pins(), // Added for generate blocks
                Some(kind) => {
                    self.error(format!("Expected pin declaration (starting with 'pin'), 'generate' or '}}', found {:?}", kind));
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

    // Parses a block of interface instances: interfaces { ... }
    pub(crate) fn parse_interfaces_block(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACES_BLOCK.into());
        self.expect(SyntaxKind::INTERFACES_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::INTERFACE_KW) => {
                    self.parse_interface_inst();
                }
                Some(kind) => {
                    self.error(format!("Expected interface instance (starting with 'interface') or '}}', found {:?}", kind));
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

    // Parses a pin map block: pin_map = { ... }
    pub(crate) fn parse_pin_map_block(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_MAP_BLOCK.into());
        // Expect IDENT "pin_map"
        if self.peek() == Some(SyntaxKind::IDENT) {
            let mut next_non_trivia_pos = self.pos;
            while self.tokens.get(next_non_trivia_pos).map_or(false, |(k,_)| k.is_trivia()) {
                next_non_trivia_pos += 1;
            }
            let token_text = self.tokens.get(next_non_trivia_pos).map(|(_, text)| text.clone());

            if token_text.as_deref() == Some("pin_map") {
                 self.bump(); // Consume the "pin_map" IDENT
            } else {
                 self.error("Expected 'pin_map' keyword".to_string());
            }
        } else {
            self.error("Expected 'pin_map' keyword".to_string());
        }
        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() { // Use peek() not peek_raw()
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => { // Mapping starts with logical pin IDENT
                    self.parse_pin_map_entry();
                    if !self.eat(SyntaxKind::COMMA) {
                         if self.peek() != Some(SyntaxKind::R_BRACE) {
                             self.error("Expected ',' or '}' after pin map entry".to_string());
                             // Simple recovery: break loop
                             break;
                         }
                         // Trailing comma case or end of list
                         break;
                    }
                    // Handle trailing comma just before brace
                    if self.peek() == Some(SyntaxKind::R_BRACE) { break; }
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

    // Parses a block of parameter assignments: parameters { ... }
    pub(crate) fn parse_parameters_block(&mut self) {
        self.builder.start_node(SyntaxKind::PARAMETERS_BLOCK.into());
        self.expect(SyntaxKind::PARAMETERS_KW);
        self.expect(SyntaxKind::L_BRACE);

        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            // Parameters inside a parameters block do NOT start with the 'parameter' keyword
            // They are simple assignments: name = value;
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.parse_param_assign_no_kw(); // Use the version without the keyword
            } else {
                self.error(format!(
                    "Expected parameter assignment (identifier = value;) or '}}', found {:?}",
                    self.peek()
                ));
                self.bump_any(); // Recovery
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node(); // Finish PARAMETERS_BLOCK
    }

    // Parses a block of port declarations: ports { ... }
    pub(crate) fn parse_ports_block(&mut self) {
        self.builder.start_node(SyntaxKind::PORTS_BLOCK.into());
        self.expect(SyntaxKind::PORTS_KW);
        self.expect(SyntaxKind::L_BRACE);

        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::PORT_KW) {
                self.parse_port_decl();
            } else {
                 self.error(format!("Expected port declaration (starting with 'port') or '}}' in ports block, found {:?}", self.peek())); // Use peek()
                 self.bump_any(); // Recovery
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node(); // Finish PORTS_BLOCK
    }

    // Parses a block of net declarations: nets { ... }
    pub(crate) fn parse_nets_block(&mut self) {
        self.builder.start_node(SyntaxKind::NETS_BLOCK.into());
        self.expect(SyntaxKind::NETS_KW);
        self.expect(SyntaxKind::L_BRACE);

        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            match self.peek() {
                Some(SyntaxKind::NET_KW) => {
                    self.parse_net_decl();
                }
                Some(kind) => { // Changed from _ to kind
                    self.error(format!("Expected 'net' keyword or '}}', found {:?}", kind)); // Updated error msg
                    self.bump_any();
                }
                None => break,
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses a block of component instantiations: components { ... }
    pub(crate) fn parse_components_block(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENTS_BLOCK.into());
        self.expect(SyntaxKind::COMPONENTS_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            let kind = self.peek();
            match kind {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::COMPONENT_KW) => {
                    self.parse_component_inst();
                }
                None => {
                    self.error("Unexpected end of file inside components block".to_string());
                    break;
                }
                _ => {
                    self.error(format!("Expected 'component' keyword or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses parameters within component instantiation braces { parameter ... }
    pub(crate) fn parse_component_params(&mut self) {
        // Expect PARAMETER_KW before each assignment
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::R_BRACE => break,
                SyntaxKind::PARAMETER_KW => {
                    self.parse_param_assign();
                }
                _ => {
                    self.error(format!("Expected parameter assignment (starting with 'parameter') or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }
    }

    // Parses a block of connections: connections { ... }
    pub(crate) fn parse_connections_block(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTIONS_BLOCK.into());
        self.expect(SyntaxKind::CONNECTIONS_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            let kind = self.peek();
            match kind {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::CONNECT_KW) => self.parse_connection_stmt(),
                Some(SyntaxKind::ASSIGN_KW) => self.parse_assign_stmt(),
                None => {
                    self.error("Unexpected end of file inside connections block".to_string());
                    break;
                }
                _ => {
                    self.error(format!("Expected 'connect' or 'assign' keyword or '}}', found {:?}", kind));
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses a generate-for block (for pins): generate for <var> in <range> { ... }
    pub(crate) fn parse_generate_for_pins(&mut self) {
        self.builder.start_node(SyntaxKind::GENERATE_FOR_BLOCK.into());
        self.expect(SyntaxKind::GENERATE_KW);
        self.expect(SyntaxKind::FOR_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::IN_KW);
        self.parse_range_expr();
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PIN_KW) => {
                    self.parse_pin_decl();
                }
                Some(kind) => {
                    self.error(format!("Expected 'pin' keyword or '}}' in generate for block, found {:?}", kind));
                    self.bump_any();
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

    // Parses a layer stackup block: layer_stackup { ... }
    pub(crate) fn parse_layer_stackup_block(&mut self) {
        self.builder.start_node(SyntaxKind::LAYER_STACKUP_BLOCK.into());
        self.expect(SyntaxKind::LAYER_STACKUP_KW);
        self.expect(SyntaxKind::L_BRACE);

        while self.peek() == Some(SyntaxKind::LAYER_KW) {
            self.parse_layer_def();
        }

        if self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.error(format!("Expected 'layer' keyword or '}}', found {:?}", self.peek()));
            while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses a default design rules block: default_design_rules { ... }
    pub(crate) fn parse_default_design_rules_block(&mut self) {
        self.builder.start_node(SyntaxKind::DEFAULT_DESIGN_RULES_BLOCK.into());
        self.expect(SyntaxKind::DEFAULT_DESIGN_RULES_KW);
        self.expect(SyntaxKind::L_BRACE);

        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.parse_param_assign_no_kw(); // Rules are key=value
            } else {
                self.error(format!("Expected design rule assignment (identifier = value) or '}}', found {:?}", self.peek()));
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parses a constrain block: constrain (target) { ... }
    pub(crate) fn parse_constrain_block(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRAIN_BLOCK.into());
        self.expect(SyntaxKind::CONSTRAIN_KW);

        self.expect(SyntaxKind::L_PAREN);
        self.builder.start_node(SyntaxKind::CONSTRAINT_TARGET.into());
        if self.peek() == Some(SyntaxKind::IDENT) {
            // Use parse_ref_revised to handle simple names, pin refs, net refs
             self.parse_ref_revised(SyntaxKind::SIMPLE_IDENT_REF);
        } else {
            self.error("Expected target identifier (net name or pin reference) inside constrain parentheses".to_string());
            while self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
                self.bump_any();
            }
        }
        self.builder.finish_node(); // Finish CONSTRAINT_TARGET
        self.expect(SyntaxKind::R_PAREN);
        
        self.expect(SyntaxKind::L_BRACE);
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.parse_param_assign_no_kw(); // Constraints are key=value
            } else {
                self.error(format!("Expected constraint assignment (identifier = value) or '}}', found {:?}", self.peek()));
                self.bump_any();
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node(); // Finish CONSTRAIN_BLOCK
    }
} 