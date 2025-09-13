// Parser for component-embedded simulation annotations
// Handles @behavioral_model, @optimization_strategy, etc.

use crate::syntax::SyntaxKind;
use crate::core::Parser;

impl Parser<'_> {
    /// Parse simulation annotations within a module/component
    /// Called when we see @ token in module body
    pub fn parse_simulation_annotation(&mut self) -> bool {
        if self.peek() != Some(SyntaxKind::AT) {
            return false;
        }
        
        // Look ahead to see what kind of annotation
        let next_token = self.tokens.get(self.pos + 1).map(|(kind, _)| *kind);
        let next_text = self.tokens.get(self.pos + 1).map(|(_, text)| text.as_str());
        
        match next_text {
            Some("behavioral_model") => {
                self.parse_behavioral_model();
                true
            }
            Some("optimization_strategy") => {
                self.parse_optimization_strategy();
                true
            }
            Some("component_knowledge") => {
                self.parse_component_knowledge();
                true
            }
            Some("simulation_requirements") => {
                self.parse_simulation_requirements();
                true
            }
            Some("test_sequences") => {
                self.parse_test_sequences();
                true
            }
            Some("model_selector") => {
                self.parse_model_selector();
                true
            }
            _ => false
        }
    }
    
    /// Parse @behavioral_model name { ... }
    fn parse_behavioral_model(&mut self) {
        self.builder.start_node(SyntaxKind::BEHAVIORAL_MODEL.into());
        
        // @ token
        self.expect(SyntaxKind::AT);
        
        // "behavioral_model" identifier
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.tokens[self.pos].1.clone();
            if text == "behavioral_model" {
                self.bump();
            } else {
                self.error("Expected 'behavioral_model' after @".to_string());
            }
        }
        
        // Model name
        self.expect(SyntaxKind::IDENT);
        
        // Model body
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_model_property();
            
            // Optional comma or semicolon
            if self.peek() == Some(SyntaxKind::COMMA) || self.peek() == Some(SyntaxKind::SEMI) {
                self.bump();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse a model property: key: value
    fn parse_model_property(&mut self) {
        self.builder.start_node(SyntaxKind::MODEL_PROPERTY.into());
        
        // Property name
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COLON);
        
        // Property value (could be string, number, array, or struct)
        self.parse_property_value();
        
        self.builder.finish_node();
    }
    
    /// Parse property value - could be various types
    fn parse_property_value(&mut self) {
        // For now, just parse as expression which handles most cases
        self.parse_expression();
    }
    
    /// Parse @optimization_strategy { ... }
    fn parse_optimization_strategy(&mut self) {
        self.builder.start_node(SyntaxKind::OPTIMIZATION_STRATEGY.into());
        
        // @ token
        self.expect(SyntaxKind::AT);
        
        // "optimization_strategy" identifier
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.tokens[self.pos].1.clone();
            if text == "optimization_strategy" {
                self.bump();
            } else {
                self.error("Expected 'optimization_strategy' after @".to_string());
            }
        }
        
        // Strategy body
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_optimization_phase();
            
            // Optional comma or semicolon
            if self.peek() == Some(SyntaxKind::COMMA) || self.peek() == Some(SyntaxKind::SEMI) {
                self.bump();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse optimization phase definition
    fn parse_optimization_phase(&mut self) {
        self.builder.start_node(SyntaxKind::OPTIMIZATION_PHASE.into());
        
        // Phase name (e.g., phase1, initial_sizing, etc.)
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COLON);
        
        // Phase definition - parse as expression
        self.parse_expression();
        
        self.builder.finish_node();
    }
    
    /// Parse @component_knowledge { ... }
    fn parse_component_knowledge(&mut self) {
        self.builder.start_node(SyntaxKind::COMPONENT_KNOWLEDGE.into());
        
        // @ token
        self.expect(SyntaxKind::AT);
        
        // "component_knowledge" identifier
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.tokens[self.pos].1.clone();
            if text == "component_knowledge" {
                self.bump();
            } else {
                self.error("Expected 'component_knowledge' after @".to_string());
            }
        }
        
        // Knowledge body
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_knowledge_item();
            
            // Optional comma or semicolon
            if self.peek() == Some(SyntaxKind::COMMA) || self.peek() == Some(SyntaxKind::SEMI) {
                self.bump();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse a knowledge item
    fn parse_knowledge_item(&mut self) {
        self.builder.start_node(SyntaxKind::KNOWLEDGE_ITEM.into());
        
        // Item name
        self.expect(SyntaxKind::IDENT);
        self.expect(SyntaxKind::COLON);
        
        // Item value
        self.parse_property_value();
        
        self.builder.finish_node();
    }
    
    /// Parse @simulation_requirements { ... }
    fn parse_simulation_requirements(&mut self) {
        self.builder.start_node(SyntaxKind::SIMULATION_REQUIREMENTS.into());
        
        // @ token
        self.expect(SyntaxKind::AT);
        
        // "simulation_requirements" identifier
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.tokens[self.pos].1.clone();
            if text == "simulation_requirements" {
                self.bump();
            } else {
                self.error("Expected 'simulation_requirements' after @".to_string());
            }
        }
        
        // Requirements body
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_model_property(); // Reuse for requirements
            
            // Optional comma or semicolon
            if self.peek() == Some(SyntaxKind::COMMA) || self.peek() == Some(SyntaxKind::SEMI) {
                self.bump();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse @test_sequences { ... }
    fn parse_test_sequences(&mut self) {
        self.builder.start_node(SyntaxKind::TEST_SEQUENCES.into());
        
        // @ token
        self.expect(SyntaxKind::AT);
        
        // "test_sequences" identifier
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.tokens[self.pos].1.clone();
            if text == "test_sequences" {
                self.bump();
            } else {
                self.error("Expected 'test_sequences' after @".to_string());
            }
        }
        
        // Test sequences body
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_model_property(); // Reuse for test sequences
            
            // Optional comma or semicolon
            if self.peek() == Some(SyntaxKind::COMMA) || self.peek() == Some(SyntaxKind::SEMI) {
                self.bump();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
    
    /// Parse @model_selector { ... }
    fn parse_model_selector(&mut self) {
        self.builder.start_node(SyntaxKind::MODEL_SELECTOR.into());
        
        // @ token
        self.expect(SyntaxKind::AT);
        
        // "model_selector" identifier
        if self.peek() == Some(SyntaxKind::IDENT) {
            let text = self.tokens[self.pos].1.clone();
            if text == "model_selector" {
                self.bump();
            } else {
                self.error("Expected 'model_selector' after @".to_string());
            }
        }
        
        // Model selector body
        self.expect(SyntaxKind::L_BRACE);
        
        while self.peek() != Some(SyntaxKind::R_BRACE) && self.peek().is_some() {
            self.parse_model_property(); // Reuse for selector logic
            
            // Optional comma or semicolon
            if self.peek() == Some(SyntaxKind::COMMA) || self.peek() == Some(SyntaxKind::SEMI) {
                self.bump();
            }
        }
        
        self.expect(SyntaxKind::R_BRACE);
        self.builder.finish_node();
    }
}