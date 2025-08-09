//! Testbench parsing for BHDL
//! 
//! Parses testbench definitions and their components:
//! - simulation configuration
//! - scopes for waveform capture
//! - stimulus definitions
//! - verification assertions
//! - measurements

use crate::core::{Parser, SyntaxKindExt};
use crate::syntax::SyntaxKind;

impl<'t> Parser<'t> {
    /// Parse a testbench definition
    /// testbench Name for Board { ... }
    pub(crate) fn parse_testbench(&mut self) {
        self.builder.start_node(SyntaxKind::TESTBENCH_DEF.into());
        
        // 'testbench' keyword
        self.expect(SyntaxKind::TESTBENCH_KW);
        
        // Testbench name
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected testbench name".to_string());
        }
        
        // 'for' keyword
        if !self.eat(SyntaxKind::FOR_KW) {
            self.error("Expected 'for' keyword".to_string());
        }
        
        // Target board name
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected target board name".to_string());
        }
        
        // Body
        if self.peek() == Some(SyntaxKind::L_BRACE) {
            self.parse_testbench_body();
        } else {
            self.error("Expected testbench body".to_string());
        }
        
        self.builder.finish_node();
    }
    
    /// Parse testbench body
    fn parse_testbench_body(&mut self) {
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            // Check for contextual keywords by looking at IDENT text
            if self.peek() == Some(SyntaxKind::IDENT) {
                let text = self.peek_text();
                match text.as_deref() {
                    Some("simulation") => self.parse_simulation_block(),
                    Some("scope") => self.parse_scope_def(),
                    Some("stimulus") => self.parse_stimulus_block(),
                    Some("verify") => self.parse_verify_block(),
                    Some("measure") => self.parse_measure_block(),
                    _ => {
                        self.error("Expected simulation, scope, stimulus, verify, or measure block".to_string());
                        self.bump_any(); // Skip unexpected token
                    }
                }
            } else {
                self.error("Expected simulation, scope, stimulus, verify, or measure block".to_string());
                self.bump_any(); // Skip unexpected token
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
    }
    
    /// Parse simulation configuration block
    /// simulation { duration: 10ms; timestep: 1us; ... }
    fn parse_simulation_block(&mut self) {
        self.builder.start_node(SyntaxKind::SIMULATION_BLOCK.into());
        // Expect "simulation" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("simulation") {
            self.bump();
        } else {
            self.error("Expected 'simulation' keyword".to_string());
        }
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            // Parse configuration items
            if self.peek() == Some(SyntaxKind::IDENT) {
                // Need to peek at the actual token text
                let mut temp_pos = self.pos;
                while temp_pos < self.tokens.len() && self.tokens[temp_pos].0.is_trivia() {
                    temp_pos += 1;
                }
                let config_name = if temp_pos < self.tokens.len() {
                    self.tokens[temp_pos].1.clone()
                } else {
                    "".into()
                };
                self.bump();
                
                self.expect(SyntaxKind::COLON);
                
                match config_name.as_str() {
                    "duration" | "timestep" => self.parse_time_spec(),
                    "temperature" => self.parse_temperature(),
                    "solver" => self.parse_solver_type(),
                    _ => {
                        // Parse as generic expression
                        self.parse_expr(0);
                    }
                }
                
                self.expect(SyntaxKind::SEMI);
            } else {
                self.error("Expected configuration item".to_string());
                break;
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse scope definition
    /// scope "name" { signals: @VIN, @VOUT; capture: continuous; }
    fn parse_scope_def(&mut self) {
        self.builder.start_node(SyntaxKind::SCOPE_DEF.into());
        // Expect "scope" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("scope") {
            self.bump();
        } else {
            self.error("Expected 'scope' keyword".to_string());
        }
        
        // Scope name (string literal)
        if self.peek() == Some(SyntaxKind::STRING) {
            self.bump();
        } else {
            self.error("Expected scope name".to_string());
        }
        
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                let text = self.peek_text();
                match text.as_deref() {
                    Some("signals") => self.parse_signals_list(),
                    Some("capture") => self.parse_capture_mode(),
                    Some("trigger") => self.parse_trigger_condition(),
                    _ => {
                        // Other scope properties
                        self.bump();
                        self.expect(SyntaxKind::COLON);
                        self.parse_expr(0);
                        self.expect(SyntaxKind::SEMI);
                    }
                }
            } else {
                self.error("Expected scope property".to_string());
                break;
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse signals list
    /// signals: @VIN, U1.FB, R1.current;
    fn parse_signals_list(&mut self) {
        // Expect "signals" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("signals") {
            self.bump();
        } else {
            self.error("Expected 'signals' keyword".to_string());
        }
        self.expect(SyntaxKind::COLON);
        
        // Parse signal references
        loop {
            self.parse_signal_ref();
            
            if !self.eat(SyntaxKind::COMMA) {
                break;
            }
        }
        
        self.expect(SyntaxKind::SEMI);
    }
    
    /// Parse signal reference (@VIN, U1.FB, R1.current)
    fn parse_signal_ref(&mut self) {
        self.builder.start_node(SyntaxKind::NET_REF.into());
        
        if self.eat(SyntaxKind::AT) {
            // Net reference
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump();
            } else {
                self.error("Expected net name".to_string());
            }
        } else if self.peek() == Some(SyntaxKind::IDENT) {
            // Component reference
            self.bump();
            
            if self.eat(SyntaxKind::DOT) {
                // Pin or property
                if self.peek() == Some(SyntaxKind::IDENT) {
                    self.bump();
                } else {
                    self.error("Expected pin or property name".to_string());
                }
            }
        } else {
            self.error("Expected signal reference".to_string());
        }
        
        self.builder.finish_node();
    }
    
    /// Parse capture mode
    /// capture: continuous; or capture: on_change(10mV);
    fn parse_capture_mode(&mut self) {
        self.builder.start_node(SyntaxKind::CAPTURE_MODE.into());
        // Expect "capture" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("capture") {
            self.bump();
        } else {
            self.error("Expected 'capture' keyword".to_string());
        }
        self.expect(SyntaxKind::COLON);
        
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.peek_text();
            match text.as_deref() {
                Some("continuous") => {
                    self.bump();
                    // Simple continuous mode
                }
                Some("on_change") => {
                    self.bump();
                    // on_change with threshold
                    self.expect(SyntaxKind::L_PAREN);
                    self.parse_expr(0); // threshold value
                    self.expect(SyntaxKind::R_PAREN);
                }
                Some("periodic") => {
                    self.bump();
                    // periodic with interval
                    self.expect(SyntaxKind::L_PAREN);
                    if self.peek() == Some(SyntaxKind::IDENT) {
                        let mut temp_pos = self.pos;
                        while temp_pos < self.tokens.len() && self.tokens[temp_pos].0.is_trivia() {
                            temp_pos += 1;
                        }
                        let text = if temp_pos < self.tokens.len() {
                            self.tokens[temp_pos].1.clone()
                        } else {
                            "".into()
                        };
                        if text == "interval" {
                            self.bump();
                            self.expect(SyntaxKind::COLON);
                        }
                    }
                    self.parse_time_spec();
                    self.expect(SyntaxKind::R_PAREN);
                }
                _ => {
                    self.error("Expected capture mode".to_string());
                }
            }
        } else {
            self.error("Expected capture mode identifier".to_string());
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse trigger condition
    fn parse_trigger_condition(&mut self) {
        // Expect "trigger" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("trigger") {
            self.bump();
        } else {
            self.error("Expected 'trigger' keyword".to_string());
        }
        self.expect(SyntaxKind::COLON);
        self.parse_expr(0); // Trigger expression
        self.expect(SyntaxKind::SEMI);
    }
    
    /// Parse stimulus block
    /// stimulus { @VIN: ramp(from: 0V, to: 12V, duration: 1ms); }
    fn parse_stimulus_block(&mut self) {
        self.builder.start_node(SyntaxKind::STIMULUS_BLOCK.into());
        // Expect "stimulus" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("stimulus") {
            self.bump();
        } else {
            self.error("Expected 'stimulus' keyword".to_string());
        }
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_stimulus_assignment();
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse stimulus assignment
    fn parse_stimulus_assignment(&mut self) {
        self.builder.start_node(SyntaxKind::STIMULUS_ASSIGN.into());
        
        // Signal reference
        self.parse_signal_ref();
        self.expect(SyntaxKind::COLON);
        
        // Waveform expression
        self.parse_waveform_expr();
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse waveform expression
    fn parse_waveform_expr(&mut self) {
        self.builder.start_node(SyntaxKind::WAVEFORM_EXPR.into());
        
        if self.peek() == Some(SyntaxKind::IDENT) {
            let mut temp_pos = self.pos;
            while temp_pos < self.tokens.len() && self.tokens[temp_pos].0.is_trivia() {
                temp_pos += 1;
            }
            let waveform_type = if temp_pos < self.tokens.len() {
                self.tokens[temp_pos].1.clone()
            } else {
                "".into()
            };
            
            match waveform_type.as_str() {
                "constant" | "ramp" | "sine" | "pulse" | "steps" => {
                    self.bump();
                    // Parse waveform parameters
                    self.expect(SyntaxKind::L_PAREN);
                    self.parse_waveform_params();
                    self.expect(SyntaxKind::R_PAREN);
                }
                _ => {
                    // Generic expression
                    self.parse_expr(0);
                }
            }
        } else {
            // Simple value
            self.parse_expr(0);
        }
        
        self.builder.finish_node();
    }
    
    /// Parse waveform parameters
    fn parse_waveform_params(&mut self) {
        // Named parameters: name: value, name: value
        while self.peek() != Some(SyntaxKind::R_PAREN) && self.peek().is_some() {
            if self.peek() == Some(SyntaxKind::IDENT) {
                self.bump();
                self.expect(SyntaxKind::COLON);
                self.parse_expr(0);
                
                if self.peek() != Some(SyntaxKind::R_PAREN) {
                    self.expect(SyntaxKind::COMMA);
                }
            } else if self.peek() == Some(SyntaxKind::L_BRACKET) {
                // Array of values for steps
                self.parse_array_expr();
                break;
            } else {
                break;
            }
        }
    }
    
    /// Parse verify block
    /// verify { assert @VOUT in range(4.95V, 5.05V) after 2ms message "..."; }
    fn parse_verify_block(&mut self) {
        self.builder.start_node(SyntaxKind::VERIFY_BLOCK.into());
        // Expect "verify" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("verify") {
            self.bump();
        } else {
            self.error("Expected 'verify' keyword".to_string());
        }
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_assertion();
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse assertion
    fn parse_assertion(&mut self) {
        self.builder.start_node(SyntaxKind::ASSERTION.into());
        // Expect "assert" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("assert") {
            self.bump();
        } else {
            self.error("Expected 'assert' keyword".to_string());
        }
        
        // Condition expression
        self.parse_expr(0);
        
        // Time constraint (optional)
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.peek_text();
            match text.as_deref() {
                Some("after") => {
                    self.bump();
                    self.parse_time_spec();
                }
                Some("always") => {
                    self.bump();
                    // Always constraint
                }
                _ => {}
            }
        } else if self.eat(SyntaxKind::WHEN_KW) {
            self.parse_expr(0); // When condition
        }
        
        // Message
        if self.peek() == Some(SyntaxKind::IDENT) {
            let mut temp_pos = self.pos;
            while temp_pos < self.tokens.len() && self.tokens[temp_pos].0.is_trivia() {
                temp_pos += 1;
            }
            let text = if temp_pos < self.tokens.len() {
                self.tokens[temp_pos].1.clone()
            } else {
                "".into()
            };
            if text == "message" {
                self.bump(); // 'message'
                if self.peek() == Some(SyntaxKind::STRING) {
                    self.bump();
                }
            }
        }
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse measure block
    /// measure { efficiency = (@VOUT * @IOUT) / (@VIN * @IIN) * 100%; }
    fn parse_measure_block(&mut self) {
        self.builder.start_node(SyntaxKind::MEASURE_BLOCK.into());
        // Expect "measure" as IDENT
        if self.peek() == Some(SyntaxKind::IDENT) && self.peek_text().as_deref() == Some("measure") {
            self.bump();
        } else {
            self.error("Expected 'measure' keyword".to_string());
        }
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_measurement();
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse measurement
    fn parse_measurement(&mut self) {
        self.builder.start_node(SyntaxKind::MEASUREMENT.into());
        
        // Measurement name
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected measurement name".to_string());
        }
        
        self.expect(SyntaxKind::EQ);
        
        // Measurement expression
        self.parse_expr(0);
        
        self.expect(SyntaxKind::SEMI);
        self.builder.finish_node();
    }
    
    /// Parse time specification (10ms, 1us, etc.)
    fn parse_time_spec(&mut self) {
        self.builder.start_node(SyntaxKind::TIME_SPEC.into());
        
        // Number with unit
        if self.peek() == Some(SyntaxKind::NUMBER) {
            self.bump();
            
            // Time unit
            if self.at_time_unit() {
                self.bump();
            } else {
                self.error("Expected time unit".to_string());
            }
        } else {
            self.error("Expected time value".to_string());
        }
        
        self.builder.finish_node();
    }
    
    /// Parse temperature
    fn parse_temperature(&mut self) {
        if self.peek() == Some(SyntaxKind::NUMBER) {
            self.bump();
            
            // Temperature unit (C, K)
            if self.at_temp_unit() {
                self.bump();
            }
        } else {
            self.parse_expr(0);
        }
    }
    
    /// Parse solver type
    fn parse_solver_type(&mut self) {
        if self.peek() == Some(SyntaxKind::IDENT) {
            self.bump();
        } else {
            self.error("Expected solver type".to_string());
        }
    }
    
    /// Check if at time unit
    fn at_time_unit(&self) -> bool {
        matches!(self.peek(), 
            Some(SyntaxKind::MS_UNIT) | Some(SyntaxKind::US_UNIT) | 
            Some(SyntaxKind::NS_UNIT) | Some(SyntaxKind::S_UNIT) | 
            Some(SyntaxKind::UNIT_IDENTIFIER)
        )
    }
    
    /// Check if at temperature unit  
    fn at_temp_unit(&self) -> bool {
        matches!(self.peek(),
            Some(SyntaxKind::UNIT_IDENTIFIER)
        )
    }
}