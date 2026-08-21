// BHDL v2.0 Top-Level Parsing
// Only supports v2.0 flow-based syntax

use crate::syntax::SyntaxKind;
use super::core::Parser;

impl<'t> Parser<'t> {
    // Main parsing entry point
    pub(crate) fn parse_source_file(&mut self) {
        self.builder.start_node(SyntaxKind::SOURCE_FILE.into());
        
        // Loop through tokens and parse top-level items
        while let Some(kind) = self.peek() {
            match kind {
                SyntaxKind::BOARD_KW => self.parse_board_def(),
                SyntaxKind::ENTITY_KW => self.parse_entity_def(),
                SyntaxKind::ALIAS_KW => self.parse_alias_stmt(),
                SyntaxKind::TYPEDEF_KW => self.parse_typedef_def(),
                SyntaxKind::TYPE_KW => self.parse_type_def(),
                SyntaxKind::IMPORT_KW => self.parse_import_stmt(),
                SyntaxKind::INTERFACE_KW => self.parse_interface_def(),
                SyntaxKind::TESTBENCH_KW => self.parse_testbench(),
                SyntaxKind::CONST_KW => self.parse_const_decl(),
                SyntaxKind::ENUM_KW => self.parse_enum_def(),
                SyntaxKind::TRAIT_KW => self.parse_trait_def(),
                SyntaxKind::IMPL_KW => self.parse_trait_impl(),
                SyntaxKind::SAFETY_GOAL_KW => self.parse_safety_goal_def(),
                SyntaxKind::SAFETY_ASSUMPTION_KW => self.parse_safety_assumption_def(),
                SyntaxKind::SAFETY_KW => self.parse_safety_def(),
                SyntaxKind::FAULT_INJECT_KW => self.parse_fault_inject_def(),
                SyntaxKind::SYMBOL_KW => self.parse_symbol_def(),
                SyntaxKind::LAYOUT_KW => self.parse_layout_def(),
                SyntaxKind::PART_FAMILY_KW => self.parse_part_family_def(),
                _ => {
                    // Handle unexpected tokens at the top level
                    self.error(format!("Expected a top-level item (e.g., 'board', 'entity', 'interface', 'testbench', etc.), found {:?}", kind));
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

    // Parse entity definition (v2.0 syntax)
    pub(crate) fn parse_entity_def(&mut self) {
        self.builder.start_node(SyntaxKind::ENTITY_DEF.into());
        self.expect(SyntaxKind::ENTITY_KW);
        self.expect(SyntaxKind::IDENT); // Entity name

        // Optional generic type parameters: <T: Type, ...>
        if self.peek() == Some(SyntaxKind::L_ANGLE) {
            self.parse_generic_params();
        }

        // v2.0: Check for entity parameters
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_entity_parameters();
        }

        // Optional where clause: where V_IN >= 4.5V, V_OUT < V_IN
        if self.peek() == Some(SyntaxKind::WHERE_KW) {
            self.parse_where_clause();
        }

        // Optional PARTNESS declaration (VHDL-flavored role tag):
        //   entity X as part { }    — instantiation mints a physical
        //                             self-part (BOM line, refdes)
        //   entity X as design { }  — hierarchical design block: only
        //                             its children are physical
        // Undeclared entities keep today's derived behavior. The tag
        // is the one declared bit the phantom-stub heuristics have
        // been reconstructing by name-matching.
        if self.peek() == Some(SyntaxKind::AS_KW) {
            self.builder.start_node(SyntaxKind::ENTITY_KIND.into());
            self.bump(); // as
            self.skip_trivia();
            match self.peek_text().as_deref() {
                Some("part") | Some("design") => self.bump(),
                _ => self.error("entity kind must be `part` or `design` (entity X as part { })".to_string()),
            }
            self.builder.finish_node();
            self.skip_trivia();
        }

        self.expect(SyntaxKind::L_BRACE);

        // Parse entity contents (v2.0 syntax)
        self.parse_entity_contents();

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse interface definition
    pub(crate) fn parse_interface_def(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_DEF.into());
        self.expect(SyntaxKind::INTERFACE_KW);
        self.expect(SyntaxKind::IDENT); // Interface name
        
        // Optional parameter list (same as entities)
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_entity_parameters();
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
                Some(SyntaxKind::CONST_KW) => self.parse_const_decl(),
                Some(SyntaxKind::POWER_KW) => self.parse_power_decl(),
                Some(SyntaxKind::GROUND_KW) => self.parse_ground_decl(),
                // Board-level boundary port: `port VIN: power in = 12V @ 3A;`
                Some(SyntaxKind::PORT_KW) => self.parse_board_port_decl(),
                Some(SyntaxKind::POWER_DOMAIN_KW) => self.parse_power_domain_def(),
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_block(),
                Some(SyntaxKind::ATTRIBUTE_KW) => {
                    // Dotted name = SCOPED attribute on an instance
                    // (`attribute u1.expansion_applied = "true";`) —
                    // the elaborate pipeline emits synthesis provenance
                    // this way so re-synthesis restores it as REAL
                    // attributes. Plain name = board attribute.
                    if self.peek_nth_nontrivia(2) == Some(SyntaxKind::DOT) {
                        self.parse_scoped_attribute();
                    } else {
                        self.parse_attribute_decl();
                    }
                }
                Some(SyntaxKind::WHEN_KW) => self.parse_when_block(),
                Some(SyntaxKind::WITH_KW) => self.parse_with_block(),
                Some(SyntaxKind::SATISFIES_KW) => self.parse_satisfies_block(),
                // Board SKU variants: `variant <Name> { ... }` blocks
                // at board level. v0.1 body = DNP + value override
                // only. See docs/spec/Board_SKU_Variants.md.
                Some(SyntaxKind::VARIANT_KW) => self.parse_variant_block(),
                Some(SyntaxKind::IDENT) => {
                    // `supply` is a contextual keyword at board scope (not
                    // lexed as a kw — same discipline as `simulation`): it
                    // opens a power-supply requirement statement
                    // (docs/spec/Power_Supply_Synthesis.md §2).
                    if self.peek_text().as_deref() == Some("supply") {
                        self.parse_supply_stmt();
                        continue;
                    }
                    // `decouple` is a contextual keyword at board scope
                    // (same discipline as `supply`): decap-network
                    // synthesis from a domain's Z(f) mask.
                    if self.peek_text().as_deref() == Some("decouple")
                        && self.peek_nth_nontrivia(1) != Some(SyntaxKind::COLON)
                    {
                        self.parse_decouple_stmt();
                        continue;
                    }
                    // Check if this is an entity/component instantiation or connection
                    use crate::v2_fixes::NamedDeclarationType;

                    match self.is_v2_named_declaration() {
                        NamedDeclarationType::EntityInstance => {
                            self.parse_entity_instance();
                        }
                        NamedDeclarationType::ComponentInstance => {
                            self.parse_component_instance();
                        }
                        NamedDeclarationType::InterfaceInstance => {
                            // Parse as component instance for now - analyzer will differentiate
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

    // Parse entity contents (v2.0 syntax)
    fn parse_entity_contents(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::PIN_KW) => self.parse_entity_pin_decl(),
                Some(SyntaxKind::CONST_KW) => self.parse_const_decl(),
                Some(SyntaxKind::AT) => {
                    // `@` at entity scope is ambiguous between
                    // entity metadata (`@name = expr;`) and a net
                    // connection (`@net -> pin;` /
                    // `@net <- pin;` / `@net <-> pin;`). The first
                    // form expects EQ after IDENT; the second
                    // expects a flow arrow. Look ahead past
                    // `@IDENT` to decide.
                    //
                    // Required because the KiCad importer's
                    // emitter produces `@SIGNAL -> R1.1;` style
                    // connections inside child-sheet `entity`
                    // blocks, which is the natural translation
                    // of KiCad's net-label semantics. Without
                    // this branch the parser falls into
                    // `parse_entity_metadata` and errors with
                    // `Expected EQ, found Some(ARROW)`.
                    if self.is_at_connection_form() {
                        self.parse_v2_connection_expr();
                    } else {
                        self.parse_entity_metadata();
                    }
                }
                Some(SyntaxKind::ATTRIBUTE_KW) => self.parse_attribute_decl(),
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_block(),
                Some(SyntaxKind::WITH_KW) => self.parse_with_block(),
                Some(SyntaxKind::SATISFIES_KW) => self.parse_satisfies_block(),
                // Entity-scope `safety { ... }` = the part's safety DATA
                // (docs/spec/Functional_Safety.md §2.7): failure states,
                // SEooC figures, terminal contract, assumptions, handbook
                // class. Distinct from the top-level `safety X as ns { }`.
                Some(SyntaxKind::SAFETY_KW) => self.parse_safety_data_block(),
                Some(SyntaxKind::EXPANSION_KW) => self.parse_expansion_block(),
                Some(SyntaxKind::DESIGN_KW) => self.parse_design_block(),
                Some(SyntaxKind::PLACEMENT_KW) => self.parse_placement_block(),
                Some(SyntaxKind::INTERFACE_KW) => self.parse_interface_field_decl(),
                Some(SyntaxKind::ALIASES_KW) => self.parse_entity_aliases_block(),
                Some(SyntaxKind::IDENT) => {
                    // `simulation` is a contextual keyword (not lexed as a kw, to
                    // avoid colliding with the testbench `simulation` config block
                    // and stdlib identifiers). At entity scope it opens the
                    // device-simulation IP block (Vendor_Simulation_Blocks.md).
                    if self.peek_text().as_deref() == Some("simulation") {
                        self.parse_sim_block();
                    } else if self.peek_text().as_deref() == Some("domain")
                        && self.peek_nth_nontrivia(1) != Some(SyntaxKind::COLON)
                    {
                        // Entity-scope power-domain contract (design-level):
                        // `domain NAME k=v ...;`. Contextual — `domain` is
                        // not a keyword (an instance may be named domain:
                        // the COLON lookahead disambiguates).
                        self.parse_domain_decl();
                    } else {
                        // Entity instantiation: instance_name: EntityType(params) { ... }
                        // Connection: signal -> other_signal;
                        self.parse_entity_item();
                    }
                }
                Some(_) => {
                    self.error("Unexpected token in entity definition".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in entity definition".to_string());
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
                Some(SyntaxKind::WIRES_KW) => {
                    // v0.7 wire mapping between perspectives
                    self.parse_interface_wires_block();
                }
                Some(SyntaxKind::CONSTRAINTS_KW) => {
                    // v0.8 protocol-derived timing/electrical constraints.
                    self.parse_interface_constraints_block();
                }
                Some(SyntaxKind::INTERFACE_KW) => {
                    // v0.8 hierarchical sub-interfaces: inside an
                    // interface body, `interface SubName fieldName;`
                    // declares a sub-interface field — same shape
                    // as the inside-entity-body form.
                    //
                    //     interface DualUART {
                    //         interface UartChannel ch0;
                    //         interface UartChannel ch1;
                    //     }
                    //
                    // (Nested interface *definitions* — `interface X
                    // { ... }` inside another interface — are not
                    // supported; declare nested defs at the top
                    // level instead.)
                    self.parse_interface_field_decl();
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

    // Parse an interface-field declaration inside an entity body.
    //
    // Three shapes (v0.7):
    //
    //     interface  SPI         spi;                (default perspective, unbound)
    //     interface  SPI:slave   spi;                (explicit perspective, unbound)
    //     interface  SPI         spi { MOSI=PB3; MISO=PB4; SCK=PB5; CS=PB2; }
    //                                                (default perspective, bound to physical pins)
    //
    // The unbound forms materialise fresh `field.signal` pins on
    // the entity. The bound form declares that each interface
    // signal is an alias for an already-declared physical pin —
    // the same wire, two ways to refer to it.
    fn parse_interface_field_decl(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_FIELD_DECL.into());
        self.expect(SyntaxKind::INTERFACE_KW);

        // v0.7c: the legacy `~Interface` sugar (v0.6 and earlier) is
        // now a hard error. Use an explicit `:perspective` selector.
        if self.peek() == Some(SyntaxKind::TILDE) {
            self.error(
                "the `~Interface` direction-flip sugar was removed in v0.7. \
                 Use an explicit perspective selector instead, e.g. \
                 `interface SPI:slave spi;`."
                    .to_string(),
            );
            self.bump(); // consume `~` to keep parsing
        }

        self.expect(SyntaxKind::IDENT); // interface type name

        // v0.7 perspective selector: `:perspective_name` after the
        // interface IDENT and before the optional generic-args block.
        if self.peek() == Some(SyntaxKind::COLON) {
            self.bump(); // consume `:`
            self.expect(SyntaxKind::IDENT); // perspective name
        }

        if self.peek() == Some(SyntaxKind::L_ANGLE) {
            self.parse_type_args();
        }
        self.expect(SyntaxKind::IDENT); // field name

        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.parse_interface_field_bindings();
            // No semicolon after a binding block — the closing `}` is
            // the terminator.
        } else {
            self.expect(SyntaxKind::SEMI);
        }

        self.builder.finish_node();
    }

    // Parse the binding block: `{ SIG = PIN; SIG = PIN; ... }`.
    // Each entry binds an interface signal to an already-declared
    // physical pin (or pin number). The signal name must be a bare
    // identifier; the pin reference is an IDENT or a NUMBER (because
    // some entities have numeric pins like `pin 1:`).
    fn parse_interface_field_bindings(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_FIELD_BINDINGS.into());
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) => {
                    self.builder.start_node(SyntaxKind::INTERFACE_FIELD_BINDING.into());
                    self.expect(SyntaxKind::IDENT); // signal name
                    self.expect(SyntaxKind::EQ);
                    if self.peek() == Some(SyntaxKind::IDENT)
                        || self.peek() == Some(SyntaxKind::NUMBER)
                    {
                        self.bump(); // pin reference
                    } else {
                        self.error(
                            "Expected pin name or pin number after `=` in interface binding"
                                .to_string(),
                        );
                    }
                    self.expect(SyntaxKind::SEMI);
                    self.builder.finish_node();
                }
                _ => {
                    self.error(
                        "Expected signal name in interface binding block".to_string(),
                    );
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse entity pin declaration (v2.0 style)
    fn parse_entity_pin_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_DECL.into());
        self.expect(SyntaxKind::PIN_KW);

        // Pin name can be IDENT or NUMBER (e.g., "pin 1:", "pin VCC:")
        if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::NUMBER) {
            self.bump();
        } else {
            self.error("Expected pin name (identifier or number)".to_string());
        }

        // Optional bus suffix: [N] or [high:low] for array pins
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.parse_bus_suffix();
        }

        self.expect(SyntaxKind::COLON);
        
        // Parse pin type (signal, power, ground, switch, feedback)
        if self.peek() == Some(SyntaxKind::SIGNAL_KW) ||
           self.peek() == Some(SyntaxKind::POWER_KW) ||
           self.peek() == Some(SyntaxKind::GROUND_KW) ||
           self.peek() == Some(SyntaxKind::SWITCH_KW) ||
           self.peek() == Some(SyntaxKind::FEEDBACK_KW) {
            self.bump();
        } else {
            self.error("Expected pin type (signal, power, ground, switch, feedback)".to_string());
        }
        
        // Parse direction for signal pins
        if self.peek() == Some(SyntaxKind::IN_KW) ||
           self.peek() == Some(SyntaxKind::OUT_KW) ||
           self.peek() == Some(SyntaxKind::INOUT_KW) {
            self.bump();
        }
        
        // Check for optional 'virtual' keyword (after direction)
        if self.peek() == Some(SyntaxKind::VIRTUAL_KW) {
            self.bump(); // Consume 'virtual'
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
        
        // Parse optional pin attribute block { ... }
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.parse_pin_attribute_block();
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
    
    // Parse pin attribute block: { key: value, key: value, ... }
    fn parse_pin_attribute_block(&mut self) {
        self.builder.start_node(SyntaxKind::PIN_PROPERTIES.into());
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.skip_trivia();
            
            if self.peek() == Some(SyntaxKind::R_BRACE) {
                break;
            }
            
            // Parse attribute name
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump(); // Consume attribute name
                
                self.expect(SyntaxKind::COLON);
                
                // Parse attribute value (expression)
                self.parse_expression();
                
                // Optional comma
                if self.peek() == Some(SyntaxKind::COMMA) {
                    self.bump();
                }
            } else {
                self.error("Expected attribute name in pin attribute block".to_string());
                self.bump_any();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    // Parse entity metadata (@attributes)
    fn parse_entity_metadata(&mut self) {
        // First, try parsing simulation annotations
        if self.parse_simulation_annotation() {
            return; // Successfully parsed a simulation annotation
        }
        
        // Otherwise, parse as regular attribute
        self.expect(SyntaxKind::AT);
        self.expect(SyntaxKind::IDENT); // Attribute name
        self.expect(SyntaxKind::EQ);
        self.parse_expression(); // Attribute value
        self.expect(SyntaxKind::SEMI);
    }

    // Parse attribute declaration
    fn parse_attribute_decl(&mut self) {
        self.builder.start_node(SyntaxKind::ATTRIBUTE_DECL.into());
        self.expect(SyntaxKind::ATTRIBUTE_KW);
        self.expect(SyntaxKind::IDENT); // Attribute name
        self.expect(SyntaxKind::EQ);
        self.parse_expression(); // Attribute value (supports expressions)
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse when block: when (condition) { statements }
    fn parse_when_block(&mut self) {
        self.builder.start_node(SyntaxKind::WHEN_BLOCK.into());
        self.expect(SyntaxKind::WHEN_KW);
        self.expect(SyntaxKind::L_PAREN);
        self.parse_expression(); // Parse the condition
        self.expect(SyntaxKind::R_PAREN);
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse when block body
        self.parse_when_block_body();
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    // Parse when block body
    fn parse_when_block_body(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::ATTRIBUTE_KW) => {
                    // Attribute assignment: attribute name = expression;
                    self.parse_attribute_assignment();
                }
                Some(SyntaxKind::WHEN_KW) => {
                    // Nested when block
                    self.parse_when_block();
                }
                Some(_) => {
                    self.error("Unexpected token in when block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in when block".to_string());
                    break;
                }
            }
        }
    }
    
    // Parse satisfies block
    fn parse_satisfies_block(&mut self) {
        self.builder.start_node(SyntaxKind::SATISFIES_BLOCK.into());
        self.expect(SyntaxKind::SATISFIES_KW);
        self.expect(SyntaxKind::L_BRACE);
        
        // Parse satisfies items
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    self.parse_satisfies_item();
                }
                Some(_) => {
                    self.error("Expected requirement identifier in satisfies block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in satisfies block".to_string());
                    break;
                }
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    // Parse a single satisfies item
    fn parse_satisfies_item(&mut self) {
        self.builder.start_node(SyntaxKind::SATISFIES_ITEM.into());
        
        // Parse requirement ID
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COLON);
        
        // Parse satisfaction specification
        match self.peek() {
            Some(SyntaxKind::VIA_KW) => {
                // Simple form: REQ_001: via component_name;
                self.parse_satisfies_via();
            }
            Some(SyntaxKind::L_BRACE) => {
                // Detailed form: REQ_001: { field: value, ... }
                self.parse_satisfies_details();
            }
            _ => {
                self.error("Expected 'via' or '{' after requirement ID".to_string());
            }
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    // Parse 'via component_name' clause
    fn parse_satisfies_via(&mut self) {
        self.builder.start_node(SyntaxKind::SATISFIES_VIA.into());
        self.expect(SyntaxKind::VIA_KW);
        
        // Parse component reference (could be dotted path like module.component)
        // Support comma-separated list: via comp1, comp2.sub, comp3
        loop {
            self.expect(SyntaxKind::IDENT);
            while self.peek() == Some(SyntaxKind::DOT) {
                self.bump(); // Consume dot
                self.expect(SyntaxKind::IDENT);
            }
            
            // Check for comma (more components)
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump(); // Consume comma
                self.skip_trivia(); // Skip whitespace after comma
            } else {
                break; // No more components
            }
        }
        
        self.builder.finish_node();
    }
    
    // Parse detailed satisfaction specification
    fn parse_satisfies_details(&mut self) {
        self.builder.start_node(SyntaxKind::SATISFIES_DETAILS.into());
        self.expect(SyntaxKind::L_BRACE);
        
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    // Parse field: value pair
                    self.expect(SyntaxKind::IDENT);
                    self.expect(SyntaxKind::COLON);
                    self.parse_expression();
                    
                    // Optional comma
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                Some(_) => {
                    self.error("Expected field name in satisfies details".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in satisfies details".to_string());
                    break;
                }
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    // Parse attribute assignment: attribute name = expression;
    fn parse_attribute_assignment(&mut self) {
        self.builder.start_node(SyntaxKind::ATTRIBUTE_ASSIGNMENT.into());
        self.expect(SyntaxKind::ATTRIBUTE_KW);
        self.expect(SyntaxKind::IDENT); // Attribute name
        
        // Check for assignment operator
        match self.peek() {
            Some(SyntaxKind::EQ) => {
                self.bump(); // =
            }
            Some(SyntaxKind::PLUS_EQ) => {
                self.bump(); // +=
            }
            Some(SyntaxKind::MINUS_EQ) => {
                self.bump(); // -=
            }
            _ => {
                self.error("Expected assignment operator (=, +=, or -=)".to_string());
            }
        }
        
        self.parse_expression(); // Attribute value
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
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
    // v0.7: parse the wires { } block inside an interface body.
    //
    //     wires {
    //         dte.TX <-> dce.RX;
    //         dte.RX <-> dce.TX;
    //     }
    //
    // Each line is `perspective.signal <-> perspective.signal ;`.
    // Optional in interface declarations; default pairing is by
    // signal name when omitted (correct for SPI/I2C/USB; required
    // for UART/RS-232 etc.).
    /// Parse `aliases { gpio0 = PB0; gpio1 = PB1; ... }` inside an
    /// entity body. v0.9 function-alias mechanism: gives logical
    /// names (gpio0, adc4, …) to physical port pins so board
    /// authors don't need datasheet vocabulary.
    pub(crate) fn parse_entity_aliases_block(&mut self) {
        self.builder.start_node(SyntaxKind::ENTITY_ALIASES_BLOCK.into());
        self.expect(SyntaxKind::ALIASES_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) => {
                    self.builder.start_node(SyntaxKind::ENTITY_ALIAS_MAPPING.into());
                    self.expect(SyntaxKind::IDENT);   // alias name (gpio0, adc4, …)
                    self.expect(SyntaxKind::EQ);
                    self.expect(SyntaxKind::IDENT);   // physical pin name (PB0, PC4, …)
                    self.expect(SyntaxKind::SEMI);
                    self.builder.finish_node();
                }
                _ => {
                    self.error("Expected `alias_name = pin_name;` in aliases block".to_string());
                    self.bump_any();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// v0.8 constraints block — `constraints { stmt; stmt; ... }`
    /// inside an interface body. Each statement carries protocol-
    /// derived timing/electrical metadata (impedance, signal class,
    /// length match, skew bounds). The parser records each statement
    /// as a coarse-grained tree:
    ///
    /// ```text
    /// CONSTRAINTS_BLOCK
    ///   CONSTRAINT_STMT
    ///     CONSTRAINT_LHS    (target list text)
    ///     CONSTRAINT_RHS    (only for `A -> B:` relations)
    ///     CONSTRAINT_PROPS  (property list text)
    /// ```
    ///
    /// LHS/RHS/PROPS are uninterpreted token streams; the synthesizer
    /// re-parses them with its own mini-parser. This keeps the parser
    /// lenient and lets the property vocabulary evolve without
    /// grammar changes.
    pub(crate) fn parse_interface_constraints_block(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRAINTS_BLOCK.into());
        self.expect(SyntaxKind::CONSTRAINTS_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::SEMI) => {
                    // Stray semicolon between statements — consume and continue.
                    self.bump_any();
                }
                _ => self.parse_interface_constraint_stmt(),
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    fn parse_interface_constraint_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRAINT_STMT.into());

        // LHS: tokens up to `->` or `:`.
        self.builder.start_node(SyntaxKind::CONSTRAINT_LHS.into());
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::COLON)
                | Some(SyntaxKind::ARROW)
                | Some(SyntaxKind::SEMI)
                | Some(SyntaxKind::R_BRACE)
                | None => break,
                _ => self.bump_any(),
            }
        }
        self.builder.finish_node();

        // Optional `-> RHS`
        if self.peek() == Some(SyntaxKind::ARROW) {
            self.bump_any();
            self.builder.start_node(SyntaxKind::CONSTRAINT_RHS.into());
            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::COLON)
                    | Some(SyntaxKind::SEMI)
                    | Some(SyntaxKind::R_BRACE)
                    | None => break,
                    _ => self.bump_any(),
                }
            }
            self.builder.finish_node();
        }

        // `:` PROPS `;`
        if self.peek() == Some(SyntaxKind::COLON) {
            self.bump_any();
        } else {
            self.error("expected `:` in constraint statement".to_string());
        }

        self.builder.start_node(SyntaxKind::CONSTRAINT_PROPS.into());
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::SEMI) | Some(SyntaxKind::R_BRACE) | None => break,
                _ => self.bump_any(),
            }
        }
        self.builder.finish_node();

        if self.peek() == Some(SyntaxKind::SEMI) {
            self.bump_any();
        }
        self.builder.finish_node();
    }

    pub(crate) fn parse_interface_wires_block(&mut self) {
        self.builder.start_node(SyntaxKind::INTERFACE_WIRES_BLOCK.into());
        self.expect(SyntaxKind::WIRES_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) => {
                    self.builder.start_node(SyntaxKind::INTERFACE_WIRE_MAPPING.into());
                    self.expect(SyntaxKind::IDENT);   // left perspective
                    self.expect(SyntaxKind::DOT);
                    self.expect(SyntaxKind::IDENT);   // left signal
                    self.expect(SyntaxKind::BI_ARROW); // <->
                    self.expect(SyntaxKind::IDENT);   // right perspective
                    self.expect(SyntaxKind::DOT);
                    self.expect(SyntaxKind::IDENT);   // right signal
                    self.expect(SyntaxKind::SEMI);
                    self.builder.finish_node();
                }
                _ => {
                    self.error("Expected `perspective.signal <-> perspective.signal;` in wires block".to_string());
                    self.bump_any();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

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
    pub(crate) fn parse_const_decl(&mut self) {
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

    // Parse part_family declaration (v0.2 BOM catalog grammar).
    //
    //     part_family Yageo_RC0603FR_07 : Resistor<R: *, "1%", "0603"> {
    //         require R in E96(1Ω, 10MΩ);
    //         attribute manufacturer = "Yageo";
    //         attribute mpn_template = "RC0603FR-07{e96_code(R)}L";
    //     }
    //
    //     part_family TI_LM317T : LM317 {
    //         attribute mpn = "LM317T";
    //     }
    //
    // Body items are `require expr;` constraint clauses and
    // `attribute name = expr;` declarations. The class pattern
    // after `:` is the entity name optionally followed by a
    // generic-args block (TYPE_ARGS shape, but allowing `*` as a
    // wildcard).
    //
    // For Phase 2 the parser builds the AST but no downstream
    // pass consumes it yet. The catalog scan in Phase 4 walks
    // PART_FAMILY_DEF nodes to populate the candidate list.
    pub(crate) fn parse_part_family_def(&mut self) {
        self.builder.start_node(SyntaxKind::PART_FAMILY_DEF.into());
        self.expect(SyntaxKind::PART_FAMILY_KW);
        self.expect(SyntaxKind::IDENT); // family name

        // Class pattern: `: EntityName` or `: EntityName<args>`.
        self.builder.start_node(SyntaxKind::CLASS_PATTERN.into());
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT); // entity name
        if self.peek() == Some(SyntaxKind::L_ANGLE) {
            // Reuse parse_type_args (already accepts NUMBER, STRING,
            // IDENT, signed numbers). For Phase 2 we just need it to
            // not error; the wildcard `*` and `R: *` shapes specified
            // in the spec are accepted by parse_type_args's
            // permissive fallback (it consumes unknown tokens) —
            // tightening to spec-conformant patterns is a follow-up.
            self.parse_type_args();
        }
        self.builder.finish_node(); // CLASS_PATTERN

        // Body block: { require ...; attribute ...; }
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::REQUIRE_KW) => self.parse_require_clause(),
                Some(SyntaxKind::ATTRIBUTE_KW) => {
                    // Reuse the existing attribute-decl parser used
                    // inside entities.
                    self.parse_attribute_decl();
                }
                _ => {
                    self.error(
                        "Expected 'require' or 'attribute' in part_family body".to_string(),
                    );
                    self.bump_any();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse a constraint clause inside a part_family body:
    //
    //     require R in E96(1Ω, 10MΩ);
    //     require V_OUT in { 1.5V, 1.8V, 2.5V, 3.3V, 5.0V };
    //     require R >= 0Ω;
    //
    // The RHS uses constructs (`in`, set literals, E-series helpers)
    // that the v0.2 expression grammar doesn't yet recognise as
    // binary operators. For Phase 2 we accept *any* token stream up
    // to the terminating `;` and keep it as raw children of the
    // REQUIRE_CLAUSE node. The catalog-scan pass (Phase 4) will
    // re-parse the inner tokens against a constraint mini-grammar
    // — the parser's only job here is to fence the clause cleanly
    // so the rest of the part_family body parses.
    fn parse_require_clause(&mut self) {
        self.builder.start_node(SyntaxKind::REQUIRE_CLAUSE.into());
        self.expect(SyntaxKind::REQUIRE_KW);
        // Tolerant body: bump everything up to (but not including) SEMI.
        // Bail on R_BRACE in case the user forgot the semicolon — the
        // outer loop will recover at the body brace.
        let mut depth_paren = 0i32;
        let mut depth_brace = 0i32;
        loop {
            match self.peek() {
                None => break,
                Some(SyntaxKind::SEMI) if depth_paren == 0 && depth_brace == 0 => break,
                Some(SyntaxKind::R_BRACE) if depth_brace == 0 => break,
                Some(SyntaxKind::L_PAREN) => { depth_paren += 1; self.bump_any(); }
                Some(SyntaxKind::R_PAREN) => { depth_paren -= 1; self.bump_any(); }
                Some(SyntaxKind::L_BRACE) => { depth_brace += 1; self.bump_any(); }
                Some(SyntaxKind::R_BRACE) => { depth_brace -= 1; self.bump_any(); }
                Some(_) => self.bump_any(),
            }
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse alias statement: alias Name = Target; or alias Name = Target<5V, 3.3V>;
    fn parse_alias_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::ALIAS.into());
        self.expect(SyntaxKind::ALIAS_KW);

        // Optional: alias entity Name = Target;
        if self.peek() == Some(SyntaxKind::ENTITY_KW) {
            self.bump(); // Consume 'entity'
        }

        // Alias name can be IDENT or NUMBER (e.g., "7805", "LM7805")
        if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::NUMBER) {
            self.bump();
        } else {
            self.error("Expected alias name (identifier or number)".to_string());
        }

        self.expect(SyntaxKind::EQ);
        self.expect(SyntaxKind::IDENT); // Target name

        // Optional type arguments: `<5V, 3.3V>` (generic specialization)
        // OR constructor arguments: `(3.3V)` / `("2N2222")` (binds the
        // target entity's regular constructor params). Both forms bind
        // values to the target's parameters positionally; we record them
        // in the same TYPE_ARGS node so the analyzer treats them
        // uniformly (the monomorphization pass distinguishes generic vs
        // regular by which target the alias points at).
        if self.peek() == Some(SyntaxKind::L_ANGLE) {
            self.parse_type_args();
        } else if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_alias_ctor_args();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse a constructor-argument list on an alias: `(value, value, ...)`.
    // Recorded under the same TYPE_ARGS node the generic `<...>` form uses
    // so the analyzer's existing type-arg extraction picks the values up
    // verbatim — the only difference from `<...>` is the paren delimiters
    // (and that `>`/`<` aren't operators here, so no special handling).
    fn parse_alias_ctor_args(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_ARGS.into());
        self.expect(SyntaxKind::L_PAREN);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_PAREN) | None => break,
                Some(SyntaxKind::NUMBER) | Some(SyntaxKind::STRING) |
                Some(SyntaxKind::TRUE_KW) | Some(SyntaxKind::FALSE_KW) |
                Some(SyntaxKind::MINUS) | Some(SyntaxKind::PLUS) => {
                    self.parse_value();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                Some(SyntaxKind::IDENT) => {
                    self.bump();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                _ => {
                    // Unexpected token — bump to make progress and avoid a
                    // hang; the trailing R_PAREN/SEMI expectations report.
                    self.bump();
                }
            }
        }
        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }

    // Parse type argument list: <value, value, ...>
    // Uses parse_value() instead of parse_expression() because `>` and `<` are
    // comparison operators in the expression parser. Type args are simple values
    // (e.g., 5V, 3.3V, "red") not full expressions.
    fn parse_type_args(&mut self) {
        self.builder.start_node(SyntaxKind::TYPE_ARGS.into());
        self.expect(SyntaxKind::L_ANGLE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_ANGLE) | None => break,
                Some(SyntaxKind::NUMBER) | Some(SyntaxKind::STRING) |
                Some(SyntaxKind::TRUE_KW) | Some(SyntaxKind::FALSE_KW) |
                Some(SyntaxKind::MINUS) | Some(SyntaxKind::PLUS) => {
                    self.parse_value();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                Some(SyntaxKind::IDENT) => {
                    // Could be a type name or identifier reference
                    self.bump();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                _ => {
                    // Unknown token — skip it and hope to recover
                    self.bump();
                }
            }
        }

        self.expect(SyntaxKind::R_ANGLE);
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

    // Parse entity item (could be instance declaration or connection)
    fn parse_entity_item(&mut self) {
        use crate::v2_fixes::NamedDeclarationType;
        
        // Look ahead to determine what kind of item this is
        match self.is_v2_named_declaration() {
            NamedDeclarationType::EntityInstance => {
                self.parse_entity_instance();
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
    
    // Parse entity instance: instance_name: EntityType(params) { port mappings }
    fn parse_entity_instance(&mut self) {
        self.builder.start_node(SyntaxKind::ENTITY_INST.into());
        self.expect(SyntaxKind::IDENT); // Instance name
        self.expect(SyntaxKind::COLON);
        self.expect(SyntaxKind::IDENT); // Entity type
        
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

        // v0.2 generic args at instance site: `R1: Resistor<10kΩ>();`
        // The actual values aren't captured into the COMPONENT_INST AST
        // for v0.1 round-trip purposes (the netlist comparator keys on
        // refdes + pin only); they become part of the class identity in
        // the analyzer's mono pass, which already understands TYPE_ARGS.
        if self.peek() == Some(SyntaxKind::L_ANGLE) {
            self.parse_type_args();
        }

        // Parameters
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_expr();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    // Parse port mapping block for entity instances
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
        
        // Left side: entity pin (could be array access)
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

    // Parse power domain definition (Phase 1: Scalability)
    // Syntax: power_domain @VCC_3V3 = 3.3V @ 10A { sources { ... } distribution { ... } ... }
    pub(crate) fn parse_power_domain_def(&mut self) {
        self.builder.start_node(SyntaxKind::POWER_DOMAIN_DEF.into());
        self.expect(SyntaxKind::POWER_DOMAIN_KW);

        // Expect @ prefix for net name
        self.expect(SyntaxKind::AT);
        self.expect(SyntaxKind::IDENT);

        // Power spec: = 3.3V @ 10A
        self.expect(SyntaxKind::EQ);
        self.parse_expression(); // voltage
        self.expect(SyntaxKind::AT);
        self.parse_expression(); // current

        // Body
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::SOURCES_KW) => self.parse_sources_block(),
                Some(SyntaxKind::DISTRIBUTION_KW) => self.parse_distribution_block(),
                Some(SyntaxKind::DECOUPLING_KW) => self.parse_decoupling_block(),
                Some(SyntaxKind::CONSTRAINTS_KW) => self.parse_constraints_block(),
                Some(_) => {
                    self.error("Expected sources, distribution, decoupling, or constraints in power domain".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in power domain definition".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse sources block: sources { reg1: LDO_3V3().OUT; }
    fn parse_sources_block(&mut self) {
        self.builder.start_node(SyntaxKind::SOURCES_BLOCK.into());
        self.expect(SyntaxKind::SOURCES_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    self.parse_source_definition();
                }
                Some(_) => {
                    self.error("Expected source definition in sources block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in sources block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse single source definition: handle: Component().pin;
    fn parse_source_definition(&mut self) {
        self.builder.start_node(SyntaxKind::SOURCE_DEFINITION.into());

        // Handle name
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COLON);

        // Component instantiation: Type(params)
        self.expect(SyntaxKind::IDENT); // Component type
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_expr();
        }

        // Pin reference: .OUT
        self.expect(SyntaxKind::DOT);
        self.expect(SyntaxKind::IDENT); // Pin name

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse distribution block: distribution { fpga.VCCO[0..7]; ics[*].VDD; }
    fn parse_distribution_block(&mut self) {
        self.builder.start_node(SyntaxKind::DISTRIBUTION_BLOCK.into());
        self.expect(SyntaxKind::DISTRIBUTION_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    self.parse_distribution_pin_list();
                }
                Some(_) => {
                    self.error("Expected pin reference in distribution block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in distribution block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse distribution pin reference with support for hierarchical paths
    // Examples:
    //   fpga.VCCO[0..7];                    // simple with range
    //   ics[*].VDD;                         // simple with wildcard
    //   sensor_board[*].sensor.VCC;         // hierarchical with wildcard
    //   array.*sensor.VCC;                  // hierarchical with bare wildcard
    fn parse_distribution_pin_list(&mut self) {
        self.builder.start_node(SyntaxKind::DISTRIBUTION_PIN_LIST.into());

        // Parse first path segment
        self.parse_path_segment();

        // Parse additional path segments (for hierarchical paths)
        // Keep parsing: . IDENT [wildcard|array]
        while self.peek() == Some(SyntaxKind::DOT) {
            self.bump(); // Consume dot

            // Check for bare wildcard: .*sensor
            if self.peek() == Some(SyntaxKind::STAR) {
                self.bump(); // Consume star
            }

            // Parse next segment identifier or number (for numeric pins like .1, .2)
            if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::NUMBER) {
                self.bump();
            } else {
                self.error("Expected pin name or number after dot".to_string());
            }

            // Optional array/wildcard/pattern on this segment
            if self.peek() == Some(SyntaxKind::L_BRACKET) {
                self.bump(); // [
                self.parse_bracket_contents();
                self.expect(SyntaxKind::R_BRACKET);
            }
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse a single path segment: IDENT [wildcard|array|pattern]?
    fn parse_path_segment(&mut self) {
        // Path segment identifier
        self.expect(SyntaxKind::IDENT);

        // Optional array/wildcard/pattern: [0..7] or [*] or [even] or [0,2,4] or [0..7:2]
        if self.peek() == Some(SyntaxKind::L_BRACKET) {
            self.bump(); // [

            self.parse_bracket_contents();

            self.expect(SyntaxKind::R_BRACKET);
        }
    }

    // Parse contents of brackets: wildcard, keyword, range, list, or stepped range
    fn parse_bracket_contents(&mut self) {
        if self.peek() == Some(SyntaxKind::STAR) {
            // Wildcard: [*]
            self.bump();
        } else if self.peek() == Some(SyntaxKind::IDENT) {
            // Check for keywords: even, odd
            let checkpoint = self.builder.checkpoint();

            // Peek at the identifier text
            let ident_text = if self.pos < self.tokens.len() {
                self.tokens[self.pos].1.as_str()
            } else {
                ""
            };

            if ident_text == "even" || ident_text == "odd" {
                // Pattern keyword
                self.builder.start_node_at(checkpoint, SyntaxKind::PATTERN_KEYWORD.into());
                self.bump(); // Consume keyword
                self.builder.finish_node();
            } else {
                self.error(format!("Unknown pattern keyword '{}'", ident_text));
                self.bump(); // Consume the invalid identifier
            }
        } else {
            // Range, list, or stepped range
            self.parse_pattern_range_or_list();
        }
    }

    // Parse pattern range or list: [0..7] or [0,2,4] or [0..7:2]
    fn parse_pattern_range_or_list(&mut self) {
        self.builder.start_node(SyntaxKind::PATTERN_INDICES.into());

        // Parse first expression
        self.parse_expression();

        // Check what follows
        match self.peek() {
            Some(SyntaxKind::DOT_DOT) => {
                // Range: [0..7] or [0..7:2]
                self.bump(); // ..
                self.parse_expression(); // End

                // Check for step
                if self.peek() == Some(SyntaxKind::COLON) {
                    self.bump(); // :
                    self.parse_expression(); // Step
                }
            }
            Some(SyntaxKind::COMMA) => {
                // List: [0,2,4,8]
                while self.peek() == Some(SyntaxKind::COMMA) {
                    self.bump(); // ,
                    self.parse_expression(); // Next index
                }
            }
            _ => {
                // Single index: [5]
            }
        }

        self.builder.finish_node();
    }

    // Parse decoupling block: decoupling { near fpga: [10µF @ 5]; distributed: [0.1µF @ 50]; }
    fn parse_decoupling_block(&mut self) {
        self.builder.start_node(SyntaxKind::DECOUPLING_BLOCK.into());
        self.expect(SyntaxKind::DECOUPLING_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::NEAR_KW) | Some(SyntaxKind::DISTRIBUTED_KW) => {
                    self.parse_decoupling_rule();
                }
                Some(_) => {
                    self.error("Expected 'near' or 'distributed' in decoupling block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in decoupling block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse decoupling rule: near fpga: [10µF @ 5, 1µF @ 10]; or distributed: [0.1µF @ 50];
    fn parse_decoupling_rule(&mut self) {
        self.builder.start_node(SyntaxKind::DECOUPLING_RULE.into());

        // Placement: near <component> or distributed
        if self.peek() == Some(SyntaxKind::NEAR_KW) {
            self.bump(); // near

            // Optional 'each' keyword: near each fpga.VCCO[0..3]
            if self.peek() == Some(SyntaxKind::EACH_KW) {
                self.bump(); // each
            }

            // Component reference
            self.expect(SyntaxKind::IDENT);

            // Optional array indexing directly on the component reference:
            // near each io_banks[0..3]: ...
            if self.peek() == Some(SyntaxKind::L_BRACKET) {
                self.bump(); // [
                if self.peek() == Some(SyntaxKind::STAR) {
                    self.bump(); // *
                } else {
                    self.parse_expression(); // Start index
                    if self.peek() == Some(SyntaxKind::DOT_DOT) {
                        self.bump(); // ..
                        self.parse_expression(); // End index
                    }
                }
                self.expect(SyntaxKind::R_BRACKET);
            }

            // Optional pin reference with dots and arrays
            while self.peek() == Some(SyntaxKind::DOT) {
                self.bump(); // .
                self.expect(SyntaxKind::IDENT); // pin name

                // Optional array indexing on pin: .VCCO[0..3]
                if self.peek() == Some(SyntaxKind::L_BRACKET) {
                    self.bump(); // [
                    if self.peek() == Some(SyntaxKind::STAR) {
                        self.bump(); // *
                    } else {
                        self.parse_expression(); // Start index
                        if self.peek() == Some(SyntaxKind::DOT_DOT) {
                            self.bump(); // ..
                            self.parse_expression(); // End index
                        }
                    }
                    self.expect(SyntaxKind::R_BRACKET);
                }
            }
        } else if self.peek() == Some(SyntaxKind::DISTRIBUTED_KW) {
            self.bump(); // distributed
        }

        self.expect(SyntaxKind::COLON);

        // Capacitor list: 10µF @ 5, 1µF @ 10 (no brackets)
        loop {
            self.skip_trivia();

            // Check if we've reached the semicolon
            if self.peek() == Some(SyntaxKind::SEMI) {
                break;
            }

            // Parse capacitor spec: 10µF @ 5
            self.parse_cap_spec();

            // Optional comma
            if self.peek() == Some(SyntaxKind::COMMA) {
                self.bump();
            } else if self.peek() != Some(SyntaxKind::SEMI) {
                // If not comma and not semicolon, something went wrong
                self.error("Expected ',' or ';' after capacitor specification".to_string());
                break;
            }
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse capacitor specification: 10µF @ 5 (value @ count)
    fn parse_cap_spec(&mut self) {
        self.builder.start_node(SyntaxKind::CAP_SPEC.into());

        // Capacitance value: 10µF
        self.parse_expression();

        // @ count
        self.expect(SyntaxKind::AT);
        self.parse_expression(); // count

        self.builder.finish_node();
    }

    // Parse constraints block: constraints { max_voltage_drop: 50mV; }
    fn parse_constraints_block(&mut self) {
        self.builder.start_node(SyntaxKind::CONSTRAINTS_KW.into()); // Use CONSTRAINTS_KW as block node
        self.expect(SyntaxKind::CONSTRAINTS_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    // Constraint item: name: value;
                    self.expect(SyntaxKind::IDENT); // Constraint name
                    self.expect(SyntaxKind::COLON);
                    self.parse_expression(); // Constraint value
                    self.expect(SyntaxKind::SEMI);
                }
                Some(_) => {
                    self.error("Expected constraint name in constraints block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in constraints block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse expansion block inside entity definitions
    // expansion { internal sw: net; VOUT -> L: Ind(33µH).1 -> L.2 -> sw; ... }
    fn parse_expansion_block(&mut self) {
        self.builder.start_node(SyntaxKind::EXPANSION_BLOCK.into());
        self.expect(SyntaxKind::EXPANSION_KW);
        self.expect(SyntaxKind::L_BRACE);

        // Parse expansion contents — same as board contents but also allows
        // `internal name: net;` declarations for expansion-local nets
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::INTERNAL_KW) => self.parse_expansion_internal_net(),
                Some(SyntaxKind::CONST_KW) => self.parse_const_decl(),
                Some(SyntaxKind::POWER_KW) => self.parse_power_decl(),
                Some(SyntaxKind::GROUND_KW) => self.parse_ground_decl(),
                Some(SyntaxKind::GENERATE_KW) => self.parse_generate_block(),
                Some(SyntaxKind::ATTRIBUTE_KW) => self.parse_attribute_decl(),
                // `socket <held> in <socket>;` — composition pairing for
                // a child instance that is physically held by another
                // child (e.g. a tube in a chassis octal socket). Both
                // children still appear on the BOM with their own SKUs;
                // the held part's footprint is suppressed at PnR time.
                Some(SyntaxKind::SOCKET_KW) => self.parse_expansion_socket_stmt(),
                Some(SyntaxKind::IDENT) => {
                    // Connection statement or component instantiation
                    self.parse_connection_or_flow_stmt();
                }
                Some(SyntaxKind::AT) => {
                    // Net reference in connection
                    self.parse_connection_or_flow_stmt();
                }
                Some(_) => {
                    self.error("Unexpected token in expansion block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in expansion block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse `design for <intent> { ... }` — vendor-authored operating-point
    // design block. The body is a sequence of:
    //   * `const NAME = EXPR;` — immutable bindings (reusing parse_const_decl)
    //   * `require <expr> else "<msg>";` — validation that aborts the design
    //   * `NAME = EXPR;` — assignment to an expansion-child's value
    fn parse_design_block(&mut self) {
        self.builder.start_node(SyntaxKind::DESIGN_BLOCK.into());
        self.expect(SyntaxKind::DESIGN_KW);
        // v0.2: `design for INTENT { … }` (matcher form) OR plain
        // `design { … }` (entity-private runtime computation, spec
        // §5.2). The FOR_KW + IDENT pair is now optional.
        if self.peek() == Some(SyntaxKind::FOR_KW) {
            self.bump(); // consume `for`
            self.expect(SyntaxKind::IDENT); // intent name
        }
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::CONST_KW) => self.parse_design_const_decl(),
                Some(SyntaxKind::REQUIRE_KW) => self.parse_design_require_stmt(),
                // `body <lang> r#"..."#` — Stage-5 foreign-language hook.
                // `body` is already a keyword (it's also used for symbol
                // body hints); the design-block context disambiguates.
                Some(SyntaxKind::BODY_KW) => self.parse_design_body_hook(),
                Some(SyntaxKind::IDENT) => {
                    // `inputs`/`outputs` are contextual keywords inside a
                    // design block: bare IDENTs matched by text so they
                    // don't pollute the global keyword table or collide
                    // with stdlib identifiers. Anything else is a child
                    // assignment (`<child_name> = <expr>;`).
                    let text = self.peek_text();
                    match text.as_deref() {
                        Some("inputs")  => self.parse_design_inputs_decl(),
                        Some("outputs") => self.parse_design_outputs_decl(),
                        _ => self.parse_design_assignment(),
                    }
                }
                Some(_) => {
                    self.error("Unexpected token in design block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in design block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // inputs { name; name; ... }
    // Names are the values the foreign-language script will see in its
    // scope (e.g. `tube`, `intent`, `supply`). The list is for the
    // analyzer's I/O documentation; the evaluator marshals each name into
    // the script regardless.
    fn parse_design_inputs_decl(&mut self) {
        self.builder.start_node(SyntaxKind::DESIGN_INPUTS_DECL.into());
        self.expect(SyntaxKind::IDENT); // "inputs"
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    self.bump();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::SEMI) { self.bump(); }
                }
                Some(_) => {
                    self.error("Expected identifier or '}' in inputs decl".to_string());
                    self.bump_any();
                }
                None => break,
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // outputs { name; name; ... }
    // Names are the expansion children whose values the script will
    // populate. Used by the analyzer to validate the script's return-map
    // keys against the entity's expansion block.
    fn parse_design_outputs_decl(&mut self) {
        self.builder.start_node(SyntaxKind::DESIGN_OUTPUTS_DECL.into());
        self.expect(SyntaxKind::IDENT); // "outputs"
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    self.bump();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::SEMI) { self.bump(); }
                }
                Some(_) => {
                    self.error("Expected identifier or '}' in outputs decl".to_string());
                    self.bump_any();
                }
                None => break,
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // body <lang> r#"..."#
    // <lang> is an IDENT (currently only "rhai"; future languages would
    // be additional matches in the evaluator). The body is a raw-string
    // literal — Rust-flavoured r#"..."# — capturing the foreign-language
    // source verbatim. The closing semicolon is optional; the raw string
    // itself terminates the clause unambiguously.
    fn parse_design_body_hook(&mut self) {
        self.builder.start_node(SyntaxKind::DESIGN_BODY_HOOK.into());
        self.expect(SyntaxKind::BODY_KW);
        self.expect(SyntaxKind::IDENT); // language tag, e.g. "rhai"
        self.expect(SyntaxKind::RAW_STRING);
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::SEMI) { self.bump(); }
        self.builder.finish_node();
    }

    // variant <Name> { <body> }
    //
    // Board-level product-SKU variant. v0.1 body: only `dnp <inst>;`
    // and `<inst>.value = <expr>;` statements. See
    // docs/spec/Board_SKU_Variants.md.
    fn parse_variant_block(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT_BLOCK.into());
        self.expect(SyntaxKind::VARIANT_KW);
        self.expect(SyntaxKind::IDENT); // variant name
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::DNP_KW) => self.parse_variant_dnp_stmt(),
                Some(SyntaxKind::IDENT) => self.parse_variant_value_override(),
                Some(_) => {
                    self.error("Unexpected token in variant block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in variant block".to_string());
                    break;
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // dnp <instance_name>;
    //
    // Marks an instance as do-not-populate for this variant. The
    // instance stays in the netlist and on the PCB layout (footprint
    // + silkscreen) but is omitted from BOM / pick-place for this SKU.
    fn parse_variant_dnp_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT_DNP_STMT.into());
        self.expect(SyntaxKind::DNP_KW);
        self.expect(SyntaxKind::IDENT); // instance name
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // <instance_name>.value = <expr>;
    //
    // Replaces the literal value the base design assigned to that
    // instance. v0.1 only allows the `.value` field; future
    // extensions (`.mpn`, `.tolerance`, etc.) live behind v0.2 spec
    // work — see docs/spec/Board_SKU_Variants.md §4.
    fn parse_variant_value_override(&mut self) {
        self.builder.start_node(SyntaxKind::VARIANT_VALUE_OVERRIDE.into());
        self.expect(SyntaxKind::IDENT);  // instance name
        self.expect(SyntaxKind::DOT);
        // The field name is parsed as IDENT (contextual — `value` here
        // is just a name). v0.1 accepts any IDENT and the analyzer
        // rejects anything other than `value`; surfacing the
        // diagnostic from the analyzer rather than the parser lets us
        // extend to more fields in v0.2 without grammar churn.
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::EQ);
        self.parse_expression();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // const NAME = EXPR;  — design-block flavour, untyped (the entity-level
    // const decl requires `const NAME: TYPE = EXPR;`).
    fn parse_design_const_decl(&mut self) {
        self.builder.start_node(SyntaxKind::PARAM_DECL.into());
        self.expect(SyntaxKind::CONST_KW);
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::EQ);
        self.parse_expression();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // require <expr> else "<msg>";
    fn parse_design_require_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::DESIGN_REQUIRE_STMT.into());
        self.expect(SyntaxKind::REQUIRE_KW);
        self.parse_expression();
        self.expect(SyntaxKind::ELSE_KW);
        self.expect(SyntaxKind::STRING);
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // <child_name> = <expr>;
    fn parse_design_assignment(&mut self) {
        self.builder.start_node(SyntaxKind::DESIGN_ASSIGNMENT.into());
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::EQ);
        self.parse_expression();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Entity-level device-simulation IP block (Vendor_Simulation_Blocks.md §2):
    //   simulation { stress { ... }  model { ... } }
    // `simulation`, `stress`, `model` are contextual keywords (matched by text).
    fn parse_sim_block(&mut self) {
        self.builder.start_node(SyntaxKind::SIM_BLOCK.into());
        self.bump(); // consume the `simulation` IDENT
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => match self.peek_text().as_deref() {
                    Some("stress") => self.parse_stress_block(),
                    Some("model") => self.parse_model_block(),
                    Some("check") => self.parse_check_block(),
                    _ => {
                        self.error("Expected 'stress', 'model' or 'check' in simulation block".to_string());
                        self.bump_any();
                    }
                },
                Some(_) => {
                    self.error("Unexpected token in simulation block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in simulation block".to_string());
                    break;
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // supply @TARGET from @SOURCE { key: value; ... }
    // Power-supply requirement statement (Power_Supply_Synthesis.md §2).
    // The rails carry the electrical operating point; the spec block carries
    // only the axes a rail cannot (ripple_max, efficiency_min, i_q_max,
    // profile, and — S1 — the explicit `using: <Part>`). Values are kept as
    // raw token runs per entry (VALUE `30mV`, IDENT `cost` / `TPS54331`);
    // the desugar pass interprets them.
    fn parse_supply_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::SUPPLY_STMT.into());
        self.bump(); // consume the `supply` IDENT
        self.skip_trivia();
        // Target rail: `@NAME` (the `@` is optional, matching connection refs).
        if self.peek() == Some(SyntaxKind::AT) {
            self.bump();
        }
        self.expect(SyntaxKind::IDENT);
        self.skip_trivia();
        // `from` is contextual here (FROM_KW is only produced in import
        // position by the lexer) — accept either token flavour.
        if self.peek() == Some(SyntaxKind::FROM_KW)
            || self.peek_text().as_deref() == Some("from")
        {
            self.bump_any();
        } else {
            self.error("Expected `from` in supply statement".to_string());
        }
        self.skip_trivia();
        // Source rail.
        if self.peek() == Some(SyntaxKind::AT) {
            self.bump();
        }
        self.expect(SyntaxKind::IDENT);
        self.skip_trivia();
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    // key: value;
                    self.builder.start_node(SyntaxKind::SUPPLY_SPEC_ENTRY.into());
                    self.bump(); // key
                    self.expect(SyntaxKind::COLON);
                    // Value: everything up to the terminating `;` (a VALUE
                    // token, an IDENT, or a short token run like `85 %`).
                    loop {
                        self.skip_trivia();
                        match self.peek() {
                            Some(SyntaxKind::SEMI) => {
                                self.bump();
                                break;
                            }
                            Some(SyntaxKind::R_BRACE) | None => {
                                self.error(
                                    "Expected ';' after supply spec value".to_string(),
                                );
                                break;
                            }
                            Some(_) => self.bump_any(),
                        }
                    }
                    self.builder.finish_node();
                }
                Some(_) => {
                    self.error("Unexpected token in supply spec block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in supply statement".to_string());
                    break;
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // stress { const N = expr; require expr else "msg"; <child>.<axis> = expr; }
    // const/require reuse the design-block parsers (identical grammar).
    fn parse_stress_block(&mut self) {
        self.builder.start_node(SyntaxKind::STRESS_BLOCK.into());
        self.bump(); // consume the `stress` IDENT
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::CONST_KW) => self.parse_design_const_decl(),
                Some(SyntaxKind::REQUIRE_KW) => self.parse_design_require_stmt(),
                Some(SyntaxKind::IDENT) => self.parse_stress_assignment(),
                Some(_) => {
                    self.error("Unexpected token in stress block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in stress block".to_string());
                    break;
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // check { require <pred> else "msg"; ... }  (docs/spec/ERC.md T2/ERC025)
    // Part-carried connection rules. Only `require` statements (each failed
    // require is one ERC finding); the predicate reuses the design-require
    // grammar, so `connected(EN)` parses as an ordinary function-call expr.
    fn parse_check_block(&mut self) {
        self.builder.start_node(SyntaxKind::CHECK_BLOCK.into());
        self.bump(); // consume the `check` IDENT
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::REQUIRE_KW) => self.parse_design_require_stmt(),
                Some(_) => {
                    self.error("Expected 'require' in check block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in check block".to_string());
                    break;
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // <child_name>.<axis> = <expr>;   (e.g. `L_out.i_peak = i_out + d_il / 2;`)
    fn parse_stress_assignment(&mut self) {
        self.builder.start_node(SyntaxKind::STRESS_ASSIGNMENT.into());
        self.expect(SyntaxKind::IDENT); // child name
        self.expect(SyntaxKind::DOT);
        self.expect(SyntaxKind::IDENT); // stress axis (i_peak / v_ripple / i_rms / …)
        self.expect(SyntaxKind::EQ);
        self.parse_expression();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // model { ... }  (Vendor_Simulation_Blocks.md §5 — reserved). Captured as a
    // balanced-brace node so it round-trips, but not yet interpreted (the device
    // model still comes from the hardcoded converter fallback).
    // model { node <net> source = <expr>;  node <net> draws = <expr>; ... }
    // (Vendor_Simulation_Blocks.md §5). The primitive-composition form (`node
    // … source/draws`) is parsed into MODEL_NODE_STMT. The richer `builtin`/
    // `vendor` forms (§5.1 forms 1–2) are deferred: any non-`node` content is
    // consumed with loose recovery (skip to the next `;` at brace-depth 0) so
    // it round-trips without cascading errors.
    /// `ibis "path.ibs" component "NAME" [corner <ident>] [map { PIN = sig; … }] ;`
    /// — the §5 vendor-model form. `ibis`/`component`/`corner`/`map` are
    /// contextual identifiers.
    fn parse_model_ibis_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::MODEL_IBIS_STMT.into());
        self.bump(); // `ibis`
        self.skip_trivia();
        self.expect(SyntaxKind::STRING); // file path
        self.skip_trivia();
        if self.peek_text().as_deref() == Some("component") {
            self.bump();
            self.skip_trivia();
            self.expect(SyntaxKind::STRING); // component name
        }
        self.skip_trivia();
        if self.peek_text().as_deref() == Some("corner") {
            self.bump();
            self.skip_trivia();
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump();
            }
        }
        self.skip_trivia();
        if self.peek_text().as_deref() == Some("map") {
            self.bump();
            self.skip_trivia();
            self.expect(SyntaxKind::L_BRACE);
            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_BRACE) | None => break,
                    Some(SyntaxKind::SEMI) | Some(SyntaxKind::COMMA) => {
                        self.bump();
                    }
                    Some(_) => self.bump_any(), // PIN = signal tokens
                }
            }
            self.expect(SyntaxKind::R_BRACE);
        }
        self.skip_trivia();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    fn parse_model_block(&mut self) {
        self.builder.start_node(SyntaxKind::MODEL_BLOCK.into());
        self.bump(); // consume the `model` IDENT
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) if self.peek_text().as_deref() == Some("node") => {
                    self.parse_model_node_stmt();
                }
                Some(SyntaxKind::IDENT) if self.peek_text().as_deref() == Some("ibis") => {
                    self.parse_model_ibis_stmt();
                }
                Some(_) => {
                    // Deferred builtin/vendor form — skip to the statement end
                    // (next `;` at depth 0), balancing any nested braces.
                    let mut depth = 0i32;
                    loop {
                        match self.peek() {
                            Some(SyntaxKind::SEMI) if depth == 0 => { self.bump(); break; }
                            Some(SyntaxKind::L_BRACE) => { depth += 1; self.bump(); }
                            Some(SyntaxKind::R_BRACE) if depth == 0 => break,
                            Some(SyntaxKind::R_BRACE) => { depth -= 1; self.bump(); }
                            Some(_) => self.bump_any(),
                            None => break,
                        }
                    }
                }
                None => {
                    self.error("Unexpected end of file in model block".to_string());
                    break;
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // node <net_ident> <role_ident: source|draws> = <expr>;
    fn parse_model_node_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::MODEL_NODE_STMT.into());
        self.bump(); // consume the `node` IDENT
        self.expect(SyntaxKind::IDENT); // net name (e.g. VOUT)
        self.expect(SyntaxKind::IDENT); // role: source | draws
        self.expect(SyntaxKind::EQ);
        self.parse_expression();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse internal net declaration: internal name: net;
    // `socket <held_inst> in <socket_inst>;`
    //
    // Composition-pairing declaration inside an expansion block.
    // Marks the `held` child as physically held inside the `socket`
    // child (e.g. a tube in a chassis octal socket, an op-amp in a
    // DIP socket). The synthesizer stamps `socketed_in = "<socket>"`
    // on the held instance after expansion; downstream consumers
    // (PnR, KiCad export) read it to suppress placement of the held
    // part's footprint — the socket carries the footprint. Both
    // children still appear on the BOM as separate orderable SKUs.
    fn parse_expansion_socket_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::EXPANSION_SOCKET_STMT.into());
        self.expect(SyntaxKind::SOCKET_KW);
        self.expect(SyntaxKind::IDENT);  // held instance name
        self.expect(SyntaxKind::IN_KW);
        self.expect(SyntaxKind::IDENT);  // socket instance name
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    fn parse_expansion_internal_net(&mut self) {
        self.builder.start_node(SyntaxKind::EXPANSION_INTERNAL_NET.into());
        self.expect(SyntaxKind::INTERNAL_KW);
        self.expect(SyntaxKind::IDENT); // Net name
        self.expect(SyntaxKind::COLON);

        // Expect 'net' keyword (contextual — parsed as IDENT since it's NET_KW)
        if self.peek() == Some(SyntaxKind::NET_KW) {
            self.bump();
        } else if self.peek() == Some(SyntaxKind::IDENT) {
            // Accept 'net' as an identifier too
            self.bump();
        } else {
            self.error("Expected 'net' after internal net name".to_string());
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse placement block inside entity definitions
    // placement { reference "AP63205 Datasheet Fig.5"; L_out at (-0.5, -5.0) rot 0; ... }
    fn parse_placement_block(&mut self) {
        self.builder.start_node(SyntaxKind::PLACEMENT_BLOCK.into());
        self.expect(SyntaxKind::PLACEMENT_KW);
        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) => break,
                Some(SyntaxKind::IDENT) => {
                    // Check if this is "reference" or a placement item
                    if self.peek_text().as_deref() == Some("reference") {
                        self.parse_placement_reference();
                    } else {
                        self.parse_placement_item();
                    }
                }
                Some(_) => {
                    self.error("Unexpected token in placement block".to_string());
                    self.bump_any();
                }
                None => {
                    self.error("Unexpected end of file in placement block".to_string());
                    break;
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse placement reference: reference "AP63205 Datasheet Fig.5";
    fn parse_placement_reference(&mut self) {
        self.builder.start_node(SyntaxKind::PLACEMENT_REFERENCE.into());
        self.bump(); // consume "reference" IDENT
        if self.peek() == Some(SyntaxKind::STRING) {
            self.bump(); // consume string literal
        } else {
            self.error("Expected string after 'reference'".to_string());
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse placement item: NAME at ( EXPR , EXPR ) rot EXPR ;
    fn parse_placement_item(&mut self) {
        self.builder.start_node(SyntaxKind::PLACEMENT_ITEM.into());
        self.expect(SyntaxKind::IDENT); // component name

        // Expect "at" as contextual keyword (parsed as IDENT)
        if self.peek_text().as_deref() == Some("at") {
            self.bump(); // consume "at"
        } else {
            self.error("Expected 'at' after component name in placement item".to_string());
        }

        self.expect(SyntaxKind::L_PAREN);
        self.parse_expression(); // x coordinate (handles negative via PREFIX_EXPR)
        self.expect(SyntaxKind::COMMA);
        self.parse_expression(); // y coordinate
        self.expect(SyntaxKind::R_PAREN);

        // Optional "rot EXPR"
        self.skip_trivia();
        if self.peek_text().as_deref() == Some("rot") {
            self.bump(); // consume "rot"
            self.parse_expression(); // rotation degrees
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    // Parse enum definition: enum Name { Variant1, Variant2(PayloadType), ... }
    pub(crate) fn parse_enum_def(&mut self) {
        self.builder.start_node(SyntaxKind::ENUM_DEF.into());
        self.expect(SyntaxKind::ENUM_KW);
        self.expect(SyntaxKind::IDENT); // Enum name

        self.expect(SyntaxKind::L_BRACE);

        // Parse variants
        loop {
            self.skip_trivia();

            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) => {
                    self.parse_enum_variant();
                    // Optional comma between variants
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                _ => {
                    self.error("Expected enum variant name or '}'".to_string());
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse a single enum variant: Name or Name(type1, type2)
    fn parse_enum_variant(&mut self) {
        self.builder.start_node(SyntaxKind::ENUM_VARIANT.into());
        self.expect(SyntaxKind::IDENT); // Variant name

        // Optional payload: (Type1, Type2, ...)
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.bump(); // Consume '('

            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_PAREN) | None => break,
                    Some(SyntaxKind::IDENT) => {
                        // Parse payload type name
                        self.bump();
                        if self.peek() == Some(SyntaxKind::COMMA) {
                            self.bump();
                        }
                    }
                    _ => {
                        // Could be a value expression (e.g., BarrelJack(voltage, current))
                        self.parse_expression();
                        if self.peek() == Some(SyntaxKind::COMMA) {
                            self.bump();
                        }
                    }
                }
            }

            self.expect(SyntaxKind::R_PAREN);
        }

        self.builder.finish_node();
    }

    // Parse match expression: match expr { pattern => body, ... }
    pub(crate) fn parse_match_expr(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_EXPR.into());
        self.expect(SyntaxKind::MATCH_KW);

        // Parse the scrutinee expression (what we're matching on)
        self.parse_expression();

        self.expect(SyntaxKind::L_BRACE);

        // Parse match arms
        loop {
            self.skip_trivia();

            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                _ => {
                    self.parse_match_arm();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    // Parse a match arm: pattern => { body } or pattern => expression;
    fn parse_match_arm(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_ARM.into());

        // Parse pattern
        self.parse_match_pattern();

        // Expect =>
        if self.peek() == Some(SyntaxKind::EQ) {
            self.bump(); // '='
            if self.peek() == Some(SyntaxKind::R_ANGLE) {
                self.bump(); // '>'
            } else {
                self.error("Expected '>' after '=' in match arm (use =>)".to_string());
            }
        } else {
            self.error("Expected '=>' in match arm".to_string());
        }

        // Parse body: either a block { ... } or an expression followed by comma/semicolon
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.bump(); // '{'
            // Parse statements inside the block
            while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
                self.skip_trivia();
                if self.peek() == Some(SyntaxKind::R_BRACE) {
                    break;
                }
                // Parse each statement
                self.parse_expression();
                if self.peek() == Some(SyntaxKind::SEMI) {
                    self.bump();
                }
            }
            self.expect(SyntaxKind::R_BRACE);
        } else {
            // Single expression arm
            self.parse_expression();
        }

        // Optional comma after arm
        if self.peek() == Some(SyntaxKind::COMMA) {
            self.bump();
        }

        self.builder.finish_node();
    }

    // Parse a match pattern: wildcard _, literal, ident, or Enum::Variant(bindings)
    fn parse_match_pattern(&mut self) {
        self.builder.start_node(SyntaxKind::MATCH_PATTERN.into());

        match self.peek() {
            // Wildcard pattern: _
            Some(SyntaxKind::IDENT) => {
                // Check for _ (wildcard) or a path like PowerState::Off
                let is_underscore = self.peek_text().map_or(false, |t| t == "_");
                self.bump(); // Consume identifier

                if !is_underscore {
                    // Check for :: (path separator) for qualified enum patterns
                    while self.peek() == Some(SyntaxKind::COLON) {
                        // Look ahead for ::
                        self.bump(); // first ':'
                        if self.peek() == Some(SyntaxKind::COLON) {
                            self.bump(); // second ':'
                            if self.peek() == Some(SyntaxKind::IDENT) {
                                self.bump(); // variant name
                            } else {
                                self.error("Expected identifier after '::'".to_string());
                            }
                        }
                    }

                    // Optional destructuring: Pattern(binding1, binding2)
                    if self.peek() == Some(SyntaxKind::L_PAREN) {
                        self.bump(); // '('
                        loop {
                            self.skip_trivia();
                            match self.peek() {
                                Some(SyntaxKind::R_PAREN) | None => break,
                                Some(SyntaxKind::IDENT) => {
                                    self.bump(); // binding name
                                    if self.peek() == Some(SyntaxKind::COMMA) {
                                        self.bump();
                                    }
                                }
                                _ => {
                                    self.error("Expected binding name or ')' in pattern".to_string());
                                    self.bump_any();
                                }
                            }
                        }
                        self.expect(SyntaxKind::R_PAREN);
                    }
                }
            }
            Some(SyntaxKind::NUMBER) | Some(SyntaxKind::STRING) |
            Some(SyntaxKind::TRUE_KW) | Some(SyntaxKind::FALSE_KW) => {
                // Literal pattern
                self.bump();
            }
            _ => {
                self.error("Expected match pattern (identifier, literal, or '_')".to_string());
                self.bump_any();
            }
        }

        self.builder.finish_node();
    }

    // Parse generic type parameters: <T: Type, V: voltage, ...>
    pub(crate) fn parse_generic_params(&mut self) {
        self.builder.start_node(SyntaxKind::GENERIC_PARAMS.into());
        self.expect(SyntaxKind::L_ANGLE);

        loop {
            self.skip_trivia();

            match self.peek() {
                Some(SyntaxKind::R_ANGLE) | None => break,
                Some(SyntaxKind::IDENT) => {
                    self.parse_generic_param();
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                _ => {
                    self.error("Expected generic parameter name or '>'".to_string());
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_ANGLE);
        self.builder.finish_node();
    }

    // Parse a single generic parameter: T or T: BoundType
    fn parse_generic_param(&mut self) {
        self.builder.start_node(SyntaxKind::GENERIC_PARAM.into());
        self.expect(SyntaxKind::IDENT); // Parameter name

        // Optional type bound: `: TypeName`
        if self.peek() == Some(SyntaxKind::COLON) {
            self.bump(); // consume ':'
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump(); // consume type name
            } else {
                self.error("Expected type name after ':' in generic parameter".to_string());
            }
        }

        // Optional default: `= value`
        // Use parse_value() instead of parse_expression() to avoid consuming '>'
        // as a comparison operator (which would eat the closing angle bracket).
        if self.peek() == Some(SyntaxKind::EQ) {
            self.bump(); // consume '='
            self.parse_value();
        }

        self.builder.finish_node();
    }

    // Parse where clause: where expr1, expr2, ...
    pub(crate) fn parse_where_clause(&mut self) {
        self.builder.start_node(SyntaxKind::WHERE_CLAUSE.into());
        self.expect(SyntaxKind::WHERE_KW);

        // Parse constraint expressions separated by commas, ending at '{'
        loop {
            self.skip_trivia();

            match self.peek() {
                Some(SyntaxKind::L_BRACE) | None => break,
                // Value-set membership: `channel in ("nmos", "pmos")`. A
                // parameter's allowed-value set; scoped to the where clause
                // so `in` never enters the general expression grammar.
                Some(SyntaxKind::IDENT)
                    if self.peek_nth(1) == Some(SyntaxKind::IN_KW) =>
                {
                    self.parse_membership_constraint();
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                _ => {
                    // Parse a constraint expression (continues until comma or '{')
                    self.parse_expression();

                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
            }
        }

        self.builder.finish_node();
    }

    /// Parse `<param> in ( <literal>, <literal>, ... )` — a parameter's
    /// allowed value set. The IDENT is the parameter; the parenthesized
    /// list holds the string/number/ident literals the value must be one of.
    fn parse_membership_constraint(&mut self) {
        self.builder.start_node(SyntaxKind::MEMBERSHIP_CONSTRAINT.into());
        self.expect(SyntaxKind::IDENT); // the parameter name
        self.skip_trivia();
        self.expect(SyntaxKind::IN_KW);
        self.skip_trivia();
        self.expect(SyntaxKind::L_PAREN);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_PAREN) | None => break,
                Some(SyntaxKind::COMMA) => {
                    self.bump();
                }
                Some(_) => {
                    // A single value literal (STRING / NUMBER / IDENT / …).
                    self.bump();
                }
            }
        }
        self.expect(SyntaxKind::R_PAREN);
        self.builder.finish_node();
    }

    // ── Trait system ─────────────────────────────────────────

    /// Parse a trait definition:
    /// ```bhdl
    /// trait SpiPeripheral {
    ///     pin MOSI: signal in;
    ///     pin MISO: signal out;
    ///     const MAX_FREQ: frequency;
    /// }
    /// ```
    pub(crate) fn parse_trait_def(&mut self) {
        self.builder.start_node(SyntaxKind::TRAIT_DEF.into());
        self.expect(SyntaxKind::TRAIT_KW);
        self.skip_trivia();

        // Trait name
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected trait name".to_string());
        }

        self.skip_trivia();

        // Opening brace
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.bump();

            // Parse trait members (pins and consts)
            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_BRACE) | None => break,
                    Some(SyntaxKind::PIN_KW) => self.parse_trait_pin(),
                    Some(SyntaxKind::CONST_KW) => self.parse_trait_const(),
                    _ => {
                        self.error("Expected 'pin' or 'const' in trait body".to_string());
                        self.bump_any();
                    }
                }
            }

            self.expect(SyntaxKind::R_BRACE);
        } else {
            self.error("Expected '{' after trait name".to_string());
        }

        self.builder.finish_node();
    }

    /// Parse a pin declaration within a trait:
    /// `pin MOSI: signal in;`
    fn parse_trait_pin(&mut self) {
        self.builder.start_node(SyntaxKind::TRAIT_PIN.into());
        self.expect(SyntaxKind::PIN_KW);
        self.skip_trivia();

        // Pin name
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected pin name".to_string());
        }

        self.skip_trivia();

        // Colon
        if self.peek() == Some(SyntaxKind::COLON) {
            self.bump();
        }

        self.skip_trivia();

        // Parse type and direction tokens until semicolon
        while self.peek() != Some(SyntaxKind::SEMI) && self.peek().is_some() {
            self.bump_any();
        }

        if self.peek() == Some(SyntaxKind::SEMI) {
            self.bump();
        }

        self.builder.finish_node();
    }

    /// Parse a const declaration within a trait:
    /// `const MAX_FREQ: frequency;`
    fn parse_trait_const(&mut self) {
        self.builder.start_node(SyntaxKind::TRAIT_CONST.into());
        self.expect(SyntaxKind::CONST_KW);
        self.skip_trivia();

        // Const name
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected const name".to_string());
        }

        self.skip_trivia();

        // Colon + type
        if self.peek() == Some(SyntaxKind::COLON) {
            self.bump();
            self.skip_trivia();
            // Type name
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump();
            }
        }

        self.skip_trivia();

        // Optional default: = value
        if self.peek() == Some(SyntaxKind::EQ) {
            self.bump();
            self.skip_trivia();
            self.parse_expression();
        }

        if self.peek() == Some(SyntaxKind::SEMI) {
            self.bump();
        }

        self.builder.finish_node();
    }

    /// Parse a trait implementation:
    /// ```bhdl
    /// impl PowerRegulator for LM7805 {
    ///     const DROPOUT = 2.0V;
    ///     const MAX_CURRENT = 1.5A;
    /// }
    /// ```
    pub(crate) fn parse_trait_impl(&mut self) {
        self.builder.start_node(SyntaxKind::TRAIT_IMPL.into());
        self.expect(SyntaxKind::IMPL_KW);
        self.skip_trivia();

        // Trait name (or ~TraitName for direction flipping)
        if self.peek() == Some(SyntaxKind::TILDE) {
            self.bump(); // consume ~
            self.skip_trivia();
        }

        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump(); // trait name
        } else {
            self.error("Expected trait name after 'impl'".to_string());
        }

        self.skip_trivia();

        // Optional additional traits: impl Trait1, Trait2 for Component
        while self.peek() == Some(SyntaxKind::COMMA) {
            self.bump(); // comma
            self.skip_trivia();
            if self.peek() == Some(SyntaxKind::TILDE) {
                self.bump();
                self.skip_trivia();
            }
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump();
            }
            self.skip_trivia();
        }

        // 'for' keyword
        if self.peek() == Some(SyntaxKind::FOR_KW) {
            self.bump();
            self.skip_trivia();
        }

        // Component name
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected component name after 'for'".to_string());
        }

        self.skip_trivia();

        // Body
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.bump();

            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_BRACE) | None => break,
                    Some(SyntaxKind::CONST_KW) => self.parse_trait_const(),
                    Some(SyntaxKind::PIN_KW) => self.parse_trait_pin(),
                    _ => {
                        self.error("Expected 'const' or 'pin' in impl body".to_string());
                        self.bump_any();
                    }
                }
            }

            self.expect(SyntaxKind::R_BRACE);
        } else {
            self.error("Expected '{' in trait impl".to_string());
        }

        self.builder.finish_node();
    }

    // ── Safety annotations and fault injection ───────────────

    /// Parse a safety goal definition:
    /// ```bhdl
    /// safety_goal SG_OVP {
    ///     id: "SG-001";
    ///     title: "Prevent output overvoltage";
    ///     asil: B;
    ///     ftti: 10ms;
    /// }
    /// ```
    /// Library safety-goal definition (docs/spec/Functional_Safety.md §2.2):
    /// ```bhdl
    /// safety_goal RailOvervoltage(vmax: voltage, level: asil = ASIL_B)
    ///     "No undetected overvoltage on the rail"
    /// {
    ///     signal RAIL: power;
    ///     effect overvoltage = RAIL > vmax severity S3;
    /// }
    /// ```
    /// Legacy flat bodies (`key: value;`) still parse as items so older
    /// fixtures do not break; the semantic pass ignores them.
    pub(crate) fn parse_safety_goal_def(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_GOAL_DEF.into());
        self.expect(SyntaxKind::SAFETY_GOAL_KW);
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected safety goal name".to_string());
        }
        self.skip_trivia();
        // Optional parameter list, same shape as entity parameters.
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.builder.start_node(SyntaxKind::SAFETY_GOAL_PARAMS.into());
            self.parse_entity_parameters();
            self.builder.finish_node();
            self.skip_trivia();
        }
        // Optional title string.
        if self.peek() == Some(SyntaxKind::STRING) {
            self.bump();
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.bump();
            self.parse_safety_goal_body();
            self.expect(SyntaxKind::R_BRACE);
        } else {
            self.error("Expected '{' in safety goal definition".to_string());
        }
        self.builder.finish_node();
    }

    /// Library assumption-of-use definition (docs/spec/Functional_Safety.md §2.5):
    /// ```bhdl
    /// safety_assumption ASM_SUPPLY_WITHIN_ABSMAX(pin: net, vmax: voltage)
    ///     "Supply into {pin} stays below {vmax}";
    /// ```
    /// A `safety` block instantiates it as `assume ASM_SUPPLY_WITHIN_ABSMAX(dut.VIN, 36V);`
    /// and the semantic pass substitutes the arguments into the text.
    pub(crate) fn parse_safety_assumption_def(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_ASSUMPTION_DEF.into());
        self.expect(SyntaxKind::SAFETY_ASSUMPTION_KW);
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected safety assumption name".to_string());
        }
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.builder.start_node(SyntaxKind::SAFETY_GOAL_PARAMS.into());
            self.parse_entity_parameters();
            self.builder.finish_node();
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::STRING) {
            self.bump();
            self.skip_trivia();
        } else {
            self.error("Expected assumption text string".to_string());
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// `mission { ambient = 55degC; on_hours = 8760; cycles = 4000; }` —
    /// board-level mission profile (spec §2.8). Items are token runs to
    /// `;`, same shape as entity safety data items.
    fn parse_safety_mission(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_MISSION.into());
        self.bump(); // `mission`
        self.skip_trivia();
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) if self.peek_text().as_deref() == Some("phase") => {
                    // `phase driving { time = 8%; ambient = 60degC; powered = true; }`
                    self.builder.start_node(SyntaxKind::SAFETY_MISSION_PHASE.into());
                    self.bump(); // `phase`
                    self.skip_trivia();
                    self.expect(SyntaxKind::IDENT); // phase name
                    self.skip_trivia();
                    self.expect(SyntaxKind::L_BRACE);
                    loop {
                        self.skip_trivia();
                        match self.peek() {
                            Some(SyntaxKind::R_BRACE) | None => break,
                            Some(_) => {
                                self.builder.start_node(SyntaxKind::SAFETY_DATA_ITEM.into());
                                while self.peek() != Some(SyntaxKind::SEMI)
                                    && self.peek() != Some(SyntaxKind::R_BRACE)
                                    && self.peek().is_some()
                                {
                                    self.bump_any();
                                }
                                if self.peek() == Some(SyntaxKind::SEMI) {
                                    self.bump();
                                } else {
                                    self.error("Expected ';' after phase item".to_string());
                                }
                                self.builder.finish_node();
                            }
                        }
                    }
                    self.expect(SyntaxKind::R_BRACE);
                    self.builder.finish_node();
                }
                Some(_) => {
                    self.builder.start_node(SyntaxKind::SAFETY_DATA_ITEM.into());
                    while self.peek() != Some(SyntaxKind::SEMI)
                        && self.peek() != Some(SyntaxKind::R_BRACE)
                        && self.peek().is_some()
                    {
                        self.bump_any();
                    }
                    if self.peek() == Some(SyntaxKind::SEMI) {
                        self.bump();
                    } else {
                        self.error("Expected ';' after mission item".to_string());
                    }
                    self.builder.finish_node();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// Body shared by library goals and inline goals:
    /// `signal NAME: kind;` | `effect NAME = expr severity Sx;` | legacy `key: value;`
    fn parse_safety_goal_body(&mut self) {
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::SIGNAL_KW) => {
                    self.builder.start_node(SyntaxKind::SAFETY_SIGNAL_DECL.into());
                    self.bump();
                    self.skip_trivia();
                    self.expect(SyntaxKind::IDENT);
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::COLON) {
                        self.bump();
                        self.skip_trivia();
                        // kind: power | signal | ground | ident
                        self.bump_any();
                    }
                    self.skip_trivia();
                    self.expect(SyntaxKind::SEMI);
                    self.builder.finish_node();
                }
                Some(SyntaxKind::IDENT) if self.peek_text().as_deref() == Some("effect") => {
                    self.parse_safety_effect();
                }
                Some(SyntaxKind::IDENT) => {
                    // legacy `key: value;`
                    self.builder.start_node(SyntaxKind::REQ_PROPERTY.into());
                    self.bump();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::COLON) {
                        self.bump();
                    }
                    self.skip_trivia();
                    self.parse_expression();
                    self.skip_trivia();
                    if self.peek() == Some(SyntaxKind::SEMI) {
                        self.bump();
                    }
                    self.builder.finish_node();
                }
                _ => {
                    self.error("Expected `signal`, `effect` or `key: value;` in safety goal body".to_string());
                    self.bump_any();
                }
            }
        }
    }

    /// `effect NAME = expr severity Sx;`
    fn parse_safety_effect(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_EFFECT.into());
        self.bump(); // `effect`
        self.skip_trivia();
        self.expect(SyntaxKind::IDENT); // effect name
        self.skip_trivia();
        self.expect(SyntaxKind::EQ);
        self.skip_trivia();
        self.parse_expression();
        self.skip_trivia();
        if self.peek_text().as_deref() == Some("severity") {
            self.bump();
            self.skip_trivia();
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump(); // S0..S3
            } else {
                self.error("Expected severity class (S0..S3)".to_string());
            }
        } else {
            self.error("Expected `severity Sx` after effect expression".to_string());
        }
        self.skip_trivia();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// Entity-scope `safety { item; item; ... }` — each item is a head
    /// word (`failure_state`, `seooc`, `terminal`, `assumption`,
    /// `handbook`) followed by free tokens up to `;`. Kept as flat token
    /// runs; the semantic pass interprets them (unknown heads are hard
    /// errors there, so the CST stays total).
    /// Entity-scope `domain NAME k=v ...;` — the vendor PDN contract as a
    /// DESIGN item (docs/spec/Functional_Safety.md §2.10 moved design-side):
    /// the board must meet it whether or not it is a safety product; the
    /// safety case consumes it via `assume pdn(...)`. Token soup up to `;`,
    /// same shape as SAFETY_DATA_ITEM — the extractor does the kv parsing.
    fn parse_domain_decl(&mut self) {
        self.builder.start_node(SyntaxKind::DOMAIN_DECL.into());
        while self.peek() != Some(SyntaxKind::SEMI)
            && self.peek() != Some(SyntaxKind::R_BRACE)
            && self.peek().is_some()
        {
            self.bump_any();
        }
        if self.peek() == Some(SyntaxKind::SEMI) {
            self.bump();
        } else {
            self.error("Expected ';' after domain declaration".to_string());
        }
        self.builder.finish_node();
    }

    /// Board-scope `decouple <inst>.<domain> from "<lib>" k=v ...;` —
    /// decap-network synthesis from the domain's Z(f) mask (arc (c) of
    /// the PDN plan). Token soup up to `;`; the synthesizer parses it.
    fn parse_decouple_stmt(&mut self) {
        self.builder.start_node(SyntaxKind::DECOUPLE_STMT.into());
        while self.peek() != Some(SyntaxKind::SEMI)
            && self.peek() != Some(SyntaxKind::R_BRACE)
            && self.peek().is_some()
        {
            self.bump_any();
        }
        if self.peek() == Some(SyntaxKind::SEMI) {
            self.bump();
        } else {
            self.error("Expected ';' after decouple statement".to_string());
        }
        self.builder.finish_node();
    }

    fn parse_safety_data_block(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_DATA_BLOCK.into());
        self.expect(SyntaxKind::SAFETY_KW);
        self.skip_trivia();
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) => {
                    self.builder.start_node(SyntaxKind::SAFETY_DATA_ITEM.into());
                    while self.peek() != Some(SyntaxKind::SEMI)
                        && self.peek() != Some(SyntaxKind::R_BRACE)
                        && self.peek().is_some()
                    {
                        self.bump_any();
                    }
                    if self.peek() == Some(SyntaxKind::SEMI) {
                        self.bump();
                    } else {
                        self.error("Expected ';' after safety data item".to_string());
                    }
                    self.builder.finish_node();
                }
                _ => {
                    self.error("Expected `failure_state|seooc|terminal|assumption|handbook ...;` in entity safety block".to_string());
                    self.bump_any();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// `safety Name [of Entity] as ns { statements }` (docs/spec/Functional_Safety.md §2.1)
    pub(crate) fn parse_safety_def(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_DEF.into());
        self.expect(SyntaxKind::SAFETY_KW);
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump(); // block name
        } else {
            self.error("Expected safety block name".to_string());
        }
        self.skip_trivia();
        if self.peek_text().as_deref() == Some("of") {
            self.builder.start_node(SyntaxKind::SAFETY_LINK.into());
            self.bump();
            self.skip_trivia();
            self.expect(SyntaxKind::IDENT);
            self.builder.finish_node();
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::AS_KW) {
            self.builder.start_node(SyntaxKind::SAFETY_NS.into());
            self.bump();
            self.skip_trivia();
            self.expect(SyntaxKind::IDENT);
            self.builder.finish_node();
            self.skip_trivia();
        } else {
            self.error("Expected `as <namespace>` in safety block header (e.g. `safety Reg as dut { }`)".to_string());
        }
        self.expect(SyntaxKind::L_BRACE);
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) => {
                    let head = self.peek_text().unwrap_or_default();
                    match head.as_str() {
                        "goal" => self.parse_safety_goal_inline(),
                        "mission" => self.parse_safety_mission(),
                        "mechanism" => self.parse_safety_mechanism(),
                        "fault" => self.parse_safety_fault(),
                        "waive" => self.parse_safety_waive(),
                        "assume" => self.parse_safety_assume(),
                        _ => {
                            // `SG: Goal(...) { ... } (...);`  or
                            // `ns.inst.Goal refines SG;`      or
                            // `ns.inst.Id satisfied_by ns.h;` / `... waived "...";`
                            let mut k = 1;
                            while matches!(self.peek_nth(k), Some(SyntaxKind::WHITESPACE) | Some(SyntaxKind::COMMENT)) { k += 1; }
                            if self.peek_nth(k) == Some(SyntaxKind::COLON) {
                                self.parse_safety_goal_inst();
                            } else {
                                self.parse_safety_compose_stmt();
                            }
                        }
                    }
                }
                _ => {
                    self.error("Expected a safety statement (goal, mechanism, fault, waive, assume, `SG: Goal(...)`, `x refines y`, `x satisfied_by y`)".to_string());
                    self.bump_any();
                }
            }
        }
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// `goal SG: LEVEL "title" (kwargs) { body }`
    fn parse_safety_goal_inline(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_GOAL_INLINE.into());
        self.bump(); // `goal`
        self.skip_trivia();
        self.expect(SyntaxKind::IDENT);
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::COLON) {
            self.bump();
            self.skip_trivia();
            self.expect(SyntaxKind::IDENT); // level
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::STRING) {
            self.bump();
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_expr();
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.bump();
            self.parse_safety_goal_body();
            self.expect(SyntaxKind::R_BRACE);
        } else {
            self.expect(SyntaxKind::SEMI);
        }
        self.builder.finish_node();
    }

    /// `SG: Goal(params) { formal: ns.h; ... } (kwargs);`
    fn parse_safety_goal_inst(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_GOAL_INST.into());
        self.expect(SyntaxKind::IDENT); // instance name
        self.skip_trivia();
        self.expect(SyntaxKind::COLON);
        self.skip_trivia();
        self.expect(SyntaxKind::IDENT); // goal type
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_expr();
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.bump();
            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_BRACE) | None => break,
                    Some(SyntaxKind::IDENT) => {
                        self.builder.start_node(SyntaxKind::SAFETY_BIND_ITEM.into());
                        self.bump(); // formal
                        self.skip_trivia();
                        self.expect(SyntaxKind::COLON);
                        self.skip_trivia();
                        self.parse_safety_path(); // ns.handle / @net
                        self.skip_trivia();
                        self.expect(SyntaxKind::SEMI);
                        self.builder.finish_node();
                    }
                    _ => {
                        self.error("Expected `formal: ns.handle;` in goal binding".to_string());
                        self.bump_any();
                    }
                }
            }
            self.expect(SyntaxKind::R_BRACE);
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.parse_param_list_expr(); // per-instance kwargs
            self.skip_trivia();
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// Dotted handle path: `ns.inst.pin` / `ns.r.1` — IDENT (`.` IDENT|NUMBER)*.
    /// Used where `parse_expression` would mis-read `x: Y(...)` as a named
    /// declaration (mechanism subjects) or swallow trailing keywords.
    fn parse_safety_path(&mut self) {
        self.builder.start_node(SyntaxKind::NET_REF.into());
        if self.peek() == Some(SyntaxKind::AT) {
            self.bump();
        }
        self.expect(SyntaxKind::IDENT);
        loop {
            if self.peek() == Some(SyntaxKind::DOT) {
                self.bump();
                match self.peek() {
                    Some(SyntaxKind::IDENT) | Some(SyntaxKind::NUMBER) | Some(SyntaxKind::UNIT_IDENTIFIER) => self.bump(),
                    _ => { self.error("Expected identifier after '.' in path".to_string()); break; }
                }
            } else {
                break;
            }
        }
        self.builder.finish_node();
    }

    /// `mechanism ns.h: psm(Goal, ...);`
    fn parse_safety_mechanism(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_MECHANISM.into());
        self.bump(); // `mechanism`
        self.skip_trivia();
        self.parse_safety_path(); // ns.handle
        self.skip_trivia();
        self.expect(SyntaxKind::COLON);
        self.skip_trivia();
        self.parse_expression(); // psm(...) / lsm(...)
        self.skip_trivia();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// `fault kind(targets) expect Goal.effect [detected_by ns.h] [within dur];`
    fn parse_safety_fault(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_FAULT.into());
        self.bump(); // `fault`
        self.skip_trivia();
        self.parse_expression(); // kind(targets)
        self.skip_trivia();
        if self.peek_text().as_deref() == Some("expect") {
            self.bump();
            self.skip_trivia();
            self.parse_safety_path(); // Goal.effect
            self.skip_trivia();
        } else {
            self.error("Expected `expect Goal.effect` in fault statement".to_string());
        }
        loop {
            match self.peek_text().as_deref() {
                Some("detected_by") => {
                    self.bump();
                    self.skip_trivia();
                    self.parse_safety_path();
                    self.skip_trivia();
                }
                Some("within") => {
                    self.bump();
                    self.skip_trivia();
                    self.parse_expression(); // duration
                    self.skip_trivia();
                }
                _ => break,
            }
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// `waive ns.h qm "reason";`
    fn parse_safety_waive(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_WAIVE.into());
        self.bump(); // `waive`
        self.skip_trivia();
        self.parse_safety_path(); // ns.handle
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump(); // qm
            self.skip_trivia();
        }
        if self.peek() == Some(SyntaxKind::STRING) {
            self.bump();
        } else {
            self.error("Expected reason string in waive".to_string());
        }
        self.skip_trivia();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// `assume Id(args);` | `assume Id "text";`
    fn parse_safety_assume(&mut self) {
        self.builder.start_node(SyntaxKind::SAFETY_ASSUME.into());
        self.bump(); // `assume`
        self.skip_trivia();
        // `Id` or `Id(args…)` where args may be design paths (dut.VIN) or
        // values (40V, within=10ms). Wrapped in one NET_REF node; the
        // semantic pass re-parses the text against the library definition.
        self.builder.start_node(SyntaxKind::NET_REF.into());
        self.expect(SyntaxKind::IDENT);
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            let mut depth = 0i32;
            loop {
                match self.peek() {
                    Some(SyntaxKind::L_PAREN) => { depth += 1; self.bump(); }
                    Some(SyntaxKind::R_PAREN) => { depth -= 1; self.bump(); if depth == 0 { break; } }
                    Some(_) => self.bump_any(),
                    None => { self.error("Unclosed assumption argument list".to_string()); break; }
                }
            }
        }
        self.builder.finish_node();
        self.skip_trivia();
        if self.peek() == Some(SyntaxKind::STRING) {
            self.bump();
            self.skip_trivia();
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// `ns.inst.Goal refines SG;` | `ns.inst.Id satisfied_by ns.h;` | `ns.inst.Id waived "reason";`
    fn parse_safety_compose_stmt(&mut self) {
        let checkpoint = self.builder.checkpoint();
        self.parse_safety_path(); // subject path
        self.skip_trivia();
        match self.peek_text().as_deref() {
            Some("refines") => {
                self.builder.start_node_at(checkpoint, SyntaxKind::SAFETY_REFINES.into());
                self.bump();
                self.skip_trivia();
                self.parse_safety_path();
            }
            Some("satisfied_by") => {
                self.builder.start_node_at(checkpoint, SyntaxKind::SAFETY_SATISFIED.into());
                self.bump();
                self.skip_trivia();
                self.parse_safety_path();
            }
            Some("waived") => {
                self.builder.start_node_at(checkpoint, SyntaxKind::SAFETY_SATISFIED.into());
                self.bump();
                self.skip_trivia();
                if self.peek() == Some(SyntaxKind::STRING) { self.bump(); } else { self.error("Expected reason string after `waived`".to_string()); }
            }
            _ => {
                self.builder.start_node_at(checkpoint, SyntaxKind::SAFETY_REFINES.into());
                self.error("Expected `refines`, `satisfied_by` or `waived` after path".to_string());
            }
        }
        self.skip_trivia();
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// Parse a fault injection definition:
    /// ```bhdl
    /// fault_inject short(reg.VOUT, VIN) -> verify {
    ///     assert comparator.OUT == low within 100us;
    /// }
    /// ```
    pub(crate) fn parse_fault_inject_def(&mut self) {
        self.builder.start_node(SyntaxKind::FAULT_INJECT_DEF.into());
        self.expect(SyntaxKind::FAULT_INJECT_KW);
        self.skip_trivia();

        // Fault type (short, open, drift, etc.)
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected fault type (e.g., 'short', 'open', 'drift')".to_string());
        }

        self.skip_trivia();

        // Target arguments in parentheses
        if self.peek() == Some(SyntaxKind::L_PAREN) {
            self.bump();
            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_PAREN) | None => break,
                    Some(SyntaxKind::COMMA) => { self.bump(); }
                    _ => { self.parse_expression(); }
                }
            }
            self.expect(SyntaxKind::R_PAREN);
        }

        self.skip_trivia();

        // Optional -> verify
        if self.peek() == Some(SyntaxKind::ARROW) {
            self.bump();
            self.skip_trivia();
            // "verify" keyword (treated as IDENT)
            if self.peek() == Some(SyntaxKind::IDENT) || self.peek() == Some(SyntaxKind::VERIFY_KW) {
                self.bump();
            }
        }

        self.skip_trivia();

        // Body with assertions
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.bump();

            loop {
                self.skip_trivia();
                match self.peek() {
                    Some(SyntaxKind::R_BRACE) | None => break,
                    Some(SyntaxKind::ASSERT_KW) => {
                        self.bump(); // assert
                        self.skip_trivia();
                        // Parse assertion expression until ;
                        while self.peek() != Some(SyntaxKind::SEMI) && self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
                            self.bump_any();
                        }
                        if self.peek() == Some(SyntaxKind::SEMI) {
                            self.bump();
                        }
                    }
                    _ => {
                        // Consume unknown tokens in fault body
                        self.bump_any();
                    }
                }
            }

            self.expect(SyntaxKind::R_BRACE);
        }

        self.builder.finish_node();
    }

    // ── Symbol and layout definitions ────────────────────────────

    /// Parse a symbol definition:
    /// ```bhdl
    /// symbol TPS54331 {
    ///     body rectangle;
    ///     left   { VIN, EN, BOOT }
    ///     right  { VOUT, SW }
    ///     bottom { GND, FB }
    /// }
    /// ```
    pub(crate) fn parse_symbol_def(&mut self) {
        self.builder.start_node(SyntaxKind::SYMBOL_DEF.into());
        self.expect(SyntaxKind::SYMBOL_KW);
        self.expect(SyntaxKind::IDENT); // Entity name this symbol is for

        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::BODY_KW) => self.parse_symbol_body_hint(),
                Some(SyntaxKind::LEFT_KW) | Some(SyntaxKind::RIGHT_KW) |
                Some(SyntaxKind::TOP_KW) | Some(SyntaxKind::BOTTOM_KW) => {
                    self.parse_symbol_side();
                }
                Some(SyntaxKind::PART_KW) => self.parse_symbol_part(),
                _ => {
                    self.error("Expected 'body', 'left', 'right', 'top', 'bottom', or 'part' in symbol definition".to_string());
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// Parse body hint: `body rectangle;`
    fn parse_symbol_body_hint(&mut self) {
        self.builder.start_node(SyntaxKind::SYMBOL_BODY_HINT.into());
        self.expect(SyntaxKind::BODY_KW);
        self.expect(SyntaxKind::IDENT); // "rectangle", "triangle", etc.
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// Parse a symbol side: `left { VIN, EN }` or `left { group "Power" { VDD, VDDA } }`
    fn parse_symbol_side(&mut self) {
        self.builder.start_node(SyntaxKind::SYMBOL_SIDE.into());

        // Consume the side keyword (left/right/top/bottom)
        self.bump();

        self.expect(SyntaxKind::L_BRACE);

        // Determine if this side has groups or bare pin lists
        // Peek: if we see GROUP_KW, parse groups; otherwise parse comma-separated IDENTs
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::GROUP_KW) => self.parse_symbol_group(),
                Some(SyntaxKind::IDENT) | Some(SyntaxKind::NUMBER) => {
                    // Bare pin name (identifier or number)
                    self.bump();
                    // Optional trailing comma
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                _ => {
                    self.error("Expected pin name or 'group' in symbol side".to_string());
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// Parse a symbol group: `group "Power" { VDD, VDDA, VBAT }`
    fn parse_symbol_group(&mut self) {
        self.builder.start_node(SyntaxKind::SYMBOL_GROUP.into());
        self.expect(SyntaxKind::GROUP_KW);
        self.expect(SyntaxKind::STRING); // Group label

        self.expect(SyntaxKind::L_BRACE);

        // Parse comma-separated pin names
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::IDENT) | Some(SyntaxKind::NUMBER) => {
                    self.bump();
                    if self.peek() == Some(SyntaxKind::COMMA) {
                        self.bump();
                    }
                }
                _ => {
                    self.error("Expected pin name in symbol group".to_string());
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// Parse a symbol part (Phase 2 stub): `part "Power" { left { ... } bottom { ... } }`
    fn parse_symbol_part(&mut self) {
        self.builder.start_node(SyntaxKind::SYMBOL_PART.into());
        self.expect(SyntaxKind::PART_KW);
        self.expect(SyntaxKind::STRING); // Part label

        self.expect(SyntaxKind::L_BRACE);

        // Parse sides within the part
        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::LEFT_KW) | Some(SyntaxKind::RIGHT_KW) |
                Some(SyntaxKind::TOP_KW) | Some(SyntaxKind::BOTTOM_KW) => {
                    self.parse_symbol_side();
                }
                _ => {
                    self.error("Expected side keyword in symbol part".to_string());
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// Parse a layout definition:
    /// ```bhdl
    /// layout TPS54331 {
    ///     package HTSSOP-16;
    /// }
    /// ```
    pub(crate) fn parse_layout_def(&mut self) {
        self.builder.start_node(SyntaxKind::LAYOUT_DEF.into());
        self.expect(SyntaxKind::LAYOUT_KW);
        self.expect(SyntaxKind::IDENT); // Entity name this layout is for

        self.expect(SyntaxKind::L_BRACE);

        loop {
            self.skip_trivia();
            match self.peek() {
                Some(SyntaxKind::R_BRACE) | None => break,
                Some(SyntaxKind::PACKAGE_KW) => self.parse_layout_package(),
                Some(SyntaxKind::LAYER_STACKUP_KW) => self.parse_layout_stackup(),
                // Mechanical-contract statements are CONTEXTUAL idents
                // (never global keywords — the `package` collision
                // lesson): place / outline / mounting_hole / keepout.
                Some(SyntaxKind::IDENT) => {
                    let kind = match self.peek_text().as_deref() {
                        Some("place") => SyntaxKind::LAYOUT_PLACE,
                        Some("outline") => SyntaxKind::LAYOUT_OUTLINE,
                        Some("mounting_hole") => SyntaxKind::LAYOUT_MOUNTING_HOLE,
                        Some("keepout") => SyntaxKind::LAYOUT_KEEPOUT,
                        Some("cutout") => SyntaxKind::LAYOUT_CUTOUT,
                        Some("mech_check") => SyntaxKind::LAYOUT_KEEPOUT, // token capture; AST reads first token
                        Some("assembly") => SyntaxKind::LAYOUT_KEEPOUT, // token capture; AST reads first token
                        Some("route_bias") => SyntaxKind::LAYOUT_KEEPOUT, // token capture; AST reads first token
                        Some("pour") => SyntaxKind::LAYOUT_KEEPOUT, // token capture; AST reads first token
                        Some("track_width") => SyntaxKind::LAYOUT_KEEPOUT, // token capture; AST reads first token
                        Some("clearance") => SyntaxKind::LAYOUT_KEEPOUT, // token capture; AST reads first token
                        _ => {
                            self.error(
                                "Expected 'package', 'layer_stackup', 'place', \
                                 'outline', 'mounting_hole' or 'keepout' in layout \
                                 definition"
                                    .to_string(),
                            );
                            self.bump_any();
                            continue;
                        }
                    };
                    self.parse_layout_mech_stmt(kind);
                }
                _ => {
                    self.error(
                        "Expected a layout statement (package, layer_stackup, \
                         place, outline, mounting_hole, keepout)"
                            .to_string(),
                    );
                    self.bump_any();
                }
            }
        }

        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }

    /// Parse layout package: `package HTSSOP-16;`
    /// Package name can include hyphens, so we consume tokens until semicolon.
    fn parse_layout_package(&mut self) {
        self.builder.start_node(SyntaxKind::LAYOUT_PACKAGE.into());
        self.expect(SyntaxKind::PACKAGE_KW);

        // Package name — can be a single IDENT, or IDENT-NUMBER sequences
        // (e.g., "HTSSOP-16", "QFN-48"). Consume all tokens until semicolon.
        while self.peek() != Some(SyntaxKind::SEMI) &&
              self.peek() != Some(SyntaxKind::R_BRACE) &&
              self.peek().is_some() {
            self.bump_any();
        }

        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// Parse one mechanical-contract statement: everything up to the
    /// semicolon becomes children of the given node kind; the AST layer
    /// interprets the token stream (numbers, idents, parens).
    fn parse_layout_mech_stmt(&mut self, kind: SyntaxKind) {
        self.builder.start_node(kind.into());
        while self.peek() != Some(SyntaxKind::SEMI)
            && self.peek() != Some(SyntaxKind::R_BRACE)
            && self.peek().is_some()
        {
            self.bump_any();
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }

    /// Parse layout stackup: `layer_stackup 4;` — the BOARD-level layer
    /// count. A stackup is a design decision (cost, EMI, current
    /// capacity) the PnR consumes as INPUT; without a declaration the
    /// synthesizer infers one and reports it.
    fn parse_layout_stackup(&mut self) {
        self.builder.start_node(SyntaxKind::LAYOUT_STACKUP.into());
        self.expect(SyntaxKind::LAYER_STACKUP_KW);
        while self.peek() != Some(SyntaxKind::SEMI)
            && self.peek() != Some(SyntaxKind::R_BRACE)
            && self.peek().is_some()
        {
            self.bump_any();
        }
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
}