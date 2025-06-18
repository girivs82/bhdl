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
                Some(SyntaxKind::IDENT) | Some(SyntaxKind::AT) => {
                    // v2.0 connection or flow statement
                    // Can start with IDENT or @ (for net references)
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
                Some(SyntaxKind::IDENT) => {
                    // Interface signal declarations
                    self.parse_interface_signal();
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
        
        self.expect(SyntaxKind::SEMI);
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

    // Parse interface signal
    fn parse_interface_signal(&mut self) {
        self.expect(SyntaxKind::IDENT); // Signal name
        self.expect(SyntaxKind::COLON);
        
        // Parse signal type and direction
        if self.peek() == Some(SyntaxKind::SIGNAL_KW) {
            self.bump();
        }
        
        if self.peek() == Some(SyntaxKind::INPUT_KW) ||
           self.peek() == Some(SyntaxKind::OUTPUT_KW) ||
           self.peek() == Some(SyntaxKind::INOUT_KW) {
            self.bump();
        }
        
        self.expect(SyntaxKind::SEMI);
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