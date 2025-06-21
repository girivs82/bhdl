// BHDL v2.0 Top-Level Parsing
// Only supports v2.0 flow-based syntax

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
                SyntaxKind::ALIAS_KW => self.parse_alias_stmt(),
                SyntaxKind::TYPEDEF_KW => self.parse_typedef_def(),
                SyntaxKind::TYPE_KW => self.parse_type_def(),
                SyntaxKind::IMPORT_KW => self.parse_import_stmt(),
                SyntaxKind::INTERFACE_KW => self.parse_interface_def(),
                _ => {
                    // Handle unexpected tokens at the top level
                    self.error(format!("Expected a top-level item (e.g., 'board', 'module', 'interface', etc.), found {:?}", kind));
                    self.bump_any(); // Consume the unexpected token
                }
            }
        }
        self.builder.finish_node();
    }

    // Parse board definition (v2.0 flow syntax)
    pub(crate) fn parse_board_def(&mut self) {
        self.builder.start_node(SyntaxKind::BOARD_DEF.into());
        self.expect(SyntaxKind::BOARD_KW);
        self.expect(SyntaxKind::IDENT); // Board name
        self.expect(SyntaxKind::L_BRACE);

        // Parse board contents (v2.0 flow syntax)
        self.parse_board_contents();

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse module definition (v2.0 syntax)
    pub(crate) fn parse_module_def(&mut self) {
        self.builder.start_node(SyntaxKind::MODULE_DEF.into());
        self.expect(SyntaxKind::MODULE_KW);
        self.expect(SyntaxKind::IDENT); // Module name
        
        // v2.0: Check for module parameters
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_module_parameters();
        }
        
        self.expect(SyntaxKind::L_BRACE);

        // Parse module contents (v2.0 syntax)
        self.parse_module_contents();

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse interface definition
    pub(crate) fn parse_interface_def(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_DEF.into());
        self.expect(SyntaxKind::INTERFACE_KW);
        self.expect(SyntaxKind::IDENT); // Interface name
        
        // Optional parameter list (same as modules)
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_module_parameters();
        }
        
        self.expect(SyntaxKind::L_BRACE);

        // Parse interface contents
        self.parse_interface_contents();

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse typedef definition
    pub(crate) fn parse_typedef_def(&mut self) {
        self.builder.start_node(SyntaxKind::TYPEDEF_DEF.into());
        self.expect(SyntaxKind::TYPEDEF_KW);
        self.expect(SyntaxKind::IDENT); // Type name

        // Check for extends
        if self.peek() == Some(SyntaxKind::EXTENDS_KW) {
            self.bump();
            self.builder.start_node(SyntaxKind::TYPEDEF_BASE.into());
            self.expect(SyntaxKind::IDENT); // Base type
            self.builder.finish_node();
        }

        self.expect(SyntaxKind::L_BRACE);
        // Parse typedef body
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse board contents (v2.0 flow syntax)
    fn parse_board_contents(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::POWER_KW) => self.parse_power_decl(),
                Some(SyntaxKind::GROUND_KW) => self.parse_ground_decl(),
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_block(),
                Some(SyntaxKind::ATTRIBUTE_KW) => self.parse_attribute_decl(),
                Some(SyntaxKind::IDENT) => {
                    // Check if this is a module/component instantiation or connection
                    use crate::v2_fixes::NamedDeclarationType;
                    
                    match self.is_v2_named_declaration() {
                        NamedDeclarationType::ModuleInstance => {
                            self.parse_module_instance();
                        }
                        NamedDeclarationType::ComponentInstance => {
                            self.parse_component_instance();
                        }
                        _ => {
                            // Connection or flow statement
                            self.parse_connection_or_flow_stmt();
                        }
                    }
                }
                Some(SyntaxKind::AT) => {
                    // Net reference in connection
                    self.parse_connection_or_flow_stmt();
                }
                Some(_) => {
                    self.error("Unexpected token in board definition".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in board definition".to_string());
                    break;
                }
            }
        }
    }

    // Parse module contents (v2.0 syntax)
    fn parse_module_contents(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PIN_KW) => self.parse_module_pin_decl(),
                Some(SyntaxKind::CONST_KW) => self.parse_const_decl(),
                Some(SyntaxKind::AT) => self.parse_module_metadata(),
                Some(SyntaxKind::ATTRIBUTE_KW) => self.parse_attribute_decl(),
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_block(),
                Some(SyntaxKind::IDENT) => {
                    // Check if this is a module instantiation or connection
                    // Module instantiation: instance_name: ModuleType(params) { ... }
                    // Connection: signal -> other_signal;
                    self.parse_module_item();
                }
                Some(_) => {
                    self.error("Unexpected token in module definition".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in module definition".to_string());
                    break;
                }
            }
        }
    }

    // Parse interface contents
    fn parse_interface_contents(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::SIGNAL_KW) => {
                    // Interface signal declarations
                    self.parse_interface_signal();
                }
                Some(SyntaxKind::REQUIRE_KW) => {
                    // Interface requirements
                    self.parse_interface_requirement();
                }
                Some(SyntaxKind::PERSPECTIVE_KW) => {
                    // Interface perspectives
                    self.parse_interface_perspective();
                }
                Some(SyntaxKind::INTERFACE_KW) => {
                    // Nested interface (hierarchical)
                    self.parse_interface_def();
                }
                Some(_) => {
                    self.error("Unexpected token in interface definition".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in interface definition".to_string());
                    break;
                }
            }
        }
    }

    // Parse module pin declaration (v2.0 style)
    fn parse_module_pin_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_DECL.into());
        self.expect(SyntaxKind::PIN_KW);
        
        // Pin name can be IDENT or NUMBER (e.g., "pin 1:", "pin VCC:")
        if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::NUMBER) {
            self.bump();
        } else {
            self.error("Expected pin name (identifier or number)".to_string());
        }
        
        self.expect(SyntaxKind::COLON);
        
        // Parse pin type (signal, power, ground)
        if self.peek() == Some(SyntaxKind::SIGNAL_KW) ||
           self.peek() == Some(SyntaxKind::POWER_KW) ||
           self.peek() == Some(SyntaxKind::GROUND_KW) {
            self.bump();
        } else {
            self.error("Expected pin type (signal, power, ground)".to_string());
        }
        
        // Parse direction for signal pins
        if self.peek() == Some(SyntaxKind::IN_KW) ||
           self.peek() == Some(SyntaxKind::OUT_KW) ||
           self.peek() == Some(SyntaxKind::INOUT_KW) {
            self.bump();
        }
        
        // Parse optional 'when' clause for conditional pins
        if self.peek() == Some(SyntaxKind::WHEN_KW) {
            self.bump(); // Consume 'when'
            self.parse_expression(); // Parse the condition
        }
        
        // Parse optional @metadata annotation
        if self.peek() == Some(SyntaxKind::AT) {
            self.parse_pin_metadata();
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse pin metadata annotation: @metadata(key=value, ...)
    fn parse_pin_metadata(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_METADATA.into());
        self.expect(SyntaxKind::AT);
        
        // Expect 'metadata' keyword
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.tokens[self.pos].1.clone();
            if text == "metadata" {
                self.bump();
            } else {
                self.error("Expected 'metadata' after @".to_string());
            }
        }
        
        // Parse parameter list
        self.expect(SyntaxKind::L_PAREN);
        
        // Parse key-value pairs
        while self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
            self.builder.start_node(SyntaxKind::METADATA_PAIR.into());
            
            // Key
            self.expect(SyntaxKind::IDENT);
            self.expect(SyntaxKind::EQ);
            
            // Value (could be string or identifier)
            if self.peek() == Some(SyntaxKind::STRING) {
                self.bump();
            } else {
                self.parse_expression();
            }
            
            self.builder.finish_node();
            
            // Check for comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else if self.peek() != Some(SyntaxKind::R_PAREN) {
                self.error("Expected ',' or ')' in metadata".to_string());
                break;
            }
        }
        
        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }
    
    // Parse module metadata (@attributes)
    fn parse_module_metadata(&mut self) {
        self.expect(SyntaxKind::AT);
        self.expect(SyntaxKind::IDENT); // Attribute name
        self.expect(SyntaxKind::EQ);
        self.parse_expression(); // Attribute value
        self.expect(SyntaxKind::SEMI);
    }

    // Parse attribute declaration
    fn parse_attribute_decl(&mut self) {
        self.expect(SyntaxKind::ATTRIBUTE_KW);
        self.expect(SyntaxKind::IDENT); // Attribute name
        self.expect(SyntaxKind::EQ);
        self.parse_expression(); // Attribute value
        self.expect(SyntaxKind::SEMI);
    }

    // Parse interface signal: signal name: direction optional?;
    fn parse_interface_signal(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_SIGNAL.into());
        
        self.expect(SyntaxKind::SIGNAL_KW);
        self.expect(SyntaxKind::IDENT); // Signal name
        self.expect(SyntaxKind::COLON);
        
        // Parse signal direction (in, out, inout)
        if self.peek() == Some(SyntaxKind::IN_KW) ||
           self.peek() == Some(SyntaxKind::OUT_KW) ||
           self.peek() == Some(SyntaxKind::INOUT_KW) {
            self.bump();
        } else {
            self.error("Expected signal direction (in, out, inout)".to_string());
        }
        
        // Optional 'optional' keyword
        if self.peek() == Some(SyntaxKind::OPTIONAL_KW) {
            self.bump();
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    // Parse interface requirement: require pullup(SDA, 4.7k);
    fn parse_interface_requirement(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_REQUIREMENT.into());
        
        self.expect(SyntaxKind::REQUIRE_KW);
        
        // Parse requirement type (identifier like pullup, termination, etc.)
        self.expect(SyntaxKind::IDENT);
        
        // Parse arguments if present
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_argument_list();
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    // Parse interface perspective: perspective master { ... }
    fn parse_interface_perspective(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_PERSPECTIVE.into());
        
        self.expect(SyntaxKind::PERSPECTIVE_KW);
        self.expect(SyntaxKind::IDENT); // Perspective name (master, slave, etc.)
        
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse perspective contents (signals with different directions)
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.skip_trivia();
            if self.peek() == Some(SyntaxKind::SIGNAL_KW) {
                self.parse_interface_signal();
            } else if self.peek() == Some(SyntaxKind::R_BRACE) {
                break;
            } else {
                self.error("Expected signal declaration in perspective".to_string());
                self.bump_any();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse const declaration: const name: type = value;
    fn parse_const_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM_DECL.into());
        self.expect(SyntaxKind::CONST_KW);
        self.expect(SyntaxKind::IDENT); // Const name
        self.expect(SyntaxKind::COLON);
        
        // Parse type reference (potentially nullable)
        let checkpoint = self.builder.checkpoint();
        self.parse_type_ref();
        
        // Check for nullable type suffix
        if self.peek() == Some(SyntaxKind::QUESTION) {
            self.builder.start_node_at(checkpoint, SyntaxKind::NULLABLE_TYPE.into());
            self.bump(); // Consume '?'
            self.builder.finish_node();
        }
        
        self.expect(SyntaxKind::EQ);
        
        // Parse initializer expression
        self.parse_expression();
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse alias statement: alias Name = Target;
    fn parse_alias_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::ALIAS.into());
        self.expect(SyntaxKind::ALIAS_KW);
        
        // Optional: alias module Name = Target;
        if self.peek() == Some(SyntaxKind::MODULE_KW) {
            self.bump(); // Consume 'module'
        }
        
        // Alias name can be IDENT or NUMBER (e.g., "7805", "LM7805")
        if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::NUMBER) {
            self.bump();
        } else {
            self.error("Expected alias name (identifier or number)".to_string());
        }
        
        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::IDENT); // Target name
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse type definition: type Name = TypeExpression;
    fn parse_type_def(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_DEF.into());
        self.expect(SyntaxKind::TYPE_KW);
        self.expect(SyntaxKind::IDENT); // Type name
        self.expect(SyntaxKind::EQ);
        
        // Parse type expression (could be struct literal, identifier, nullable type, etc.)
        self.parse_type_expression();
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse module item (could be instance declaration or connection)
    fn parse_module_item(&mut self) {
        use crate::v2_fixes::NamedDeclarationType;
        
        // Look ahead to determine what kind of item this is
        match self.is_v2_named_declaration() {
            NamedDeclarationType::ModuleInstance => {
                self.parse_module_instance();
            }
            NamedDeclarationType::ComponentInstance => {
                self.parse_component_instance();
            }
            _ => {
                // Assume it's a connection statement
                self.parse_v2_connection_expr();
            }
        }
    }
    
    // Parse module instance: instance_name: ModuleType(params) { port mappings }
    fn parse_module_instance(&mut self) {
        self.builder.start_node(SyntaxKind::MODULE_INST.into());
        self.expect(SyntaxKind::IDENT); // Instance name
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT); // Module type
        
        // Optional parameters
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_expr();
        }
        
        // Port mapping block
        self.expect(SyntaxKind::L_BRACE);
        self.parse_port_mapping_block();
        self.expect(SyntaxKind::R_BRACE);
        
        self.builder.finish_node();
    }
    
    // Parse component instance: instance_name: ComponentType(params);
    fn parse_component_instance(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENT_INST.into());
        self.expect(SyntaxKind::IDENT); // Instance name
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT); // Component type
        
        // Parameters
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_expr();
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    // Parse port mapping block for module instances
    fn parse_port_mapping_block(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::ATTRIBUTE_KW) => {
                    // Scoped attribute setting
                    self.parse_scoped_attribute();
                }
                Some(SyntaxKind::IDENT) => {
                    // Port mapping: PIN <- signal or PIN -> signal
                    self.parse_port_mapping();
                }
                Some(_) => {
                    self.error("Unexpected token in port mapping block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in port mapping block".to_string());
                    break;
                }
            }
        }
    }
    
    // Parse single port mapping: PIN <- signal; or PIN -> signal;
    fn parse_port_mapping(&mut self) {
        self.builder.start_node(SyntaxKind::PORT_MAPPING.into());
        
        // Left side: module pin (could be array access)
        self.parse_pin_reference();
        
        // Connection operator
        match self.peek() {
            Some(SyntaxKind::LEFT_ARROW) => self.bump(),    // <-
            Some(SyntaxKind::ARROW) => self.bump(),         // ->
            Some(SyntaxKind::BI_ARROW) => self.bump(),      // <->
            _ => self.error("Expected connection operator (<-, ->, <->)".to_string()),
        }
        
        // Right side: signal or qualified pin reference
        self.parse_connection_target();
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    // Parse pin reference (could include array indexing)
    fn parse_pin_reference(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_REF.into());
        self.expect(SyntaxKind::IDENT); // Pin name
        
        // Optional array indexing
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }
        
        self.builder.finish_node();
    }
    
    // Parse connection target (signal or instance.pin)
    fn parse_connection_target(&mut self) {
        self.builder.start_node(SyntaxKind::CONNECTION_TARGET.into());
        
        // Could be qualified (instance.pin) or simple signal name
        // Allow keywords like "output" to be used as signal names
        self.expect_ident_or_contextual_keyword();
        
        if self.peek() == Some(SyntaxKind::DOT) {
            self.bump(); // Consume dot
            self.expect_ident_or_contextual_keyword(); // Pin name
        }
        
        // Optional array indexing
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }
        
        self.builder.finish_node();
    }
    
    // Parse scoped attribute: attribute path.to.attr = value;
    fn parse_scoped_attribute(&mut self) {
        self.builder.start_node(SyntaxKind::SCOPED_ATTRIBUTE.into());
        self.expect(SyntaxKind::ATTRIBUTE_KW);
        
        // Parse attribute path (could be nested)
        self.parse_attribute_path();
        
        self.expect(SyntaxKind::EQ);
        self.parse_expression();
        self.expect(SyntaxKind::SEMI);
        
        self.builder.finish_node();
    }
    
    // Parse attribute path: simple or nested.path.to.attr
    fn parse_attribute_path(&mut self) {
        self.builder.start_node(SyntaxKind::ATTRIBUTE_PATH.into());
        self.expect(SyntaxKind::IDENT);
        
        while self.peek() == Some(SyntaxKind::DOT) {
            self.bump(); // Consume dot
            self.expect(SyntaxKind::IDENT);
        }
        
        self.builder.finish_node();
    }

    // Parse type expression (type references, struct literals, nullable types)
    fn parse_type_expression(&mut self) {
        self.parse_type_expression_with_depth(0);
    }
    
    // Parse type expression with recursion depth tracking
    fn parse_type_expression_with_depth(&mut self, depth: usize) {
        // Prevent infinite recursion
        if depth > 50 {
            self.error("Type expression too deeply nested (max depth: 50)".to_string());
            return;
        }
        
        match self.peek() {
            Some(SyntaxKind::L_BRACE) => {
                // Struct literal: { field1: type1, field2: type2 }
                self.parse_struct_literal_with_depth(depth + 1);
            }
            Some(SyntaxKind::IDENT) => {
                // Type reference, possibly with nullable suffix
                self.parse_type_ref();
                
                // Check for nullable type suffix
                if self.peek() == Some(SyntaxKind::QUESTION) {
                    self.builder.start_node(SyntaxKind::NULLABLE_TYPE.into());
                    self.bump(); // Consume '?'
                    self.builder.finish_node();
                }
            }
            _ => {
                self.error("Expected type expression".to_string());
            }
        }
    }

    // Parse struct literal: { field1: type1, field2: type2 }
    fn parse_struct_literal(&mut self) {
        self.parse_struct_literal_with_depth(0);
    }
    
    fn parse_struct_literal_with_depth(&mut self, depth: usize) {
        self.builder.start_node(SyntaxKind::STRUCT_LITERAL.into());
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse fields
        let mut field_count = 0;
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.skip_trivia();
            
            if self.peek() == Some(SyntaxKind::R_BRACE) {
                break;
            }
            
            field_count += 1;
            if field_count > 100 {
                self.error("Too many fields in struct literal (max: 100)".to_string());
                break;
            }
            
            // Field name
            self.expect(SyntaxKind::IDENT);
            self.expect(SyntaxKind::COLON);
            
            // Field type
            self.parse_type_expression_with_depth(depth);
            
            // Check for comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else if self.peek() != Some(SyntaxKind::R_BRACE) {
                self.error("Expected ',' or '}'".to_string());
                // Try to recover by looking for the next comma or brace
                while self.peek().is_some() && 
                      self.peek() != Some(SyntaxKind::COMMA) && 
                      self.peek() != Some(SyntaxKind::R_BRACE) {
                    self.bump_any();
                }
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
}