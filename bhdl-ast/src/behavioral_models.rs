// AST nodes for behavioral models and optimization strategies
// These represent @behavioral_model and @optimization_strategy annotations

use crate::{SyntaxKind, BhdlLanguage, SyntaxNode};
use rowan::ast::AstNode;
use std::collections::HashMap;

/// Represents a @behavioral_model annotation block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct BehavioralModel(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for BehavioralModel {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::BEHAVIORAL_MODEL
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self(syntax))
        } else {
            None
        }
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl BehavioralModel {
    /// Get the model name (e.g., "analytical", "state_space", "switching")
    pub fn model_name(&self) -> Option<String> {
        // Look for the name after @behavioral_model
        self.0.children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
            .map(|token| token.text().to_string())
    }
    
    /// Get all properties defined in the model
    pub fn properties(&self) -> HashMap<String, String> {
        let mut props = HashMap::new();
        
        // Parse key-value pairs from the block
        let text = self.0.text().to_string();
        for line in text.lines() {
            if let Some(colon_pos) = line.find(':') {
                let key = line[..colon_pos].trim().to_string();
                let value = line[colon_pos + 1..]
                    .trim()
                    .trim_end_matches(',')
                    .to_string();
                
                // Skip the @behavioral_model line itself
                if !key.starts_with('@') && !key.is_empty() {
                    props.insert(key, value);
                }
            }
        }
        
        props
    }
}

/// Represents an @optimization_strategy annotation block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OptimizationStrategy(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for OptimizationStrategy {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::OPTIMIZATION_STRATEGY
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self(syntax))
        } else {
            None
        }
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl OptimizationStrategy {
    /// Get target efficiency if specified
    pub fn target_efficiency(&self) -> Option<f64> {
        self.get_numeric_property("target_efficiency")
    }
    
    /// Get minimum phase margin if specified
    pub fn min_phase_margin(&self) -> Option<f64> {
        self.get_numeric_property("min_phase_margin")
    }
    
    /// Get maximum crossover frequency if specified
    pub fn max_crossover_freq(&self) -> Option<String> {
        self.get_property("max_crossover_freq")
    }
    
    /// Get optimization objectives
    pub fn objectives(&self) -> Vec<String> {
        self.get_list_property("objectives")
    }
    
    /// Get optimization constraints
    pub fn constraints(&self) -> Vec<String> {
        self.get_list_property("constraints")
    }
    
    /// Get search method
    pub fn search_method(&self) -> Option<String> {
        self.get_property("search_method")
    }
    
    // Helper methods
    fn get_property(&self, key: &str) -> Option<String> {
        let text = self.0.text().to_string();
        if let Some(pos) = text.find(&format!("{}: ", key)) {
            let start = pos + key.len() + 2;
            let after_key = &text[start..];
            let value = after_key
                .split(&[',', '\n', '}'][..])
                .next()?
                .trim()
                .trim_matches('"');
            Some(value.to_string())
        } else {
            None
        }
    }
    
    fn get_numeric_property(&self, key: &str) -> Option<f64> {
        self.get_property(key)?
            .parse()
            .ok()
    }
    
    fn get_list_property(&self, key: &str) -> Vec<String> {
        let text = self.0.text().to_string();
        if let Some(pos) = text.find(&format!("{}: [", key)) {
            let start = pos + key.len() + 3;
            let after_key = &text[start..];
            if let Some(end) = after_key.find(']') {
                let list_content = &after_key[..end];
                return list_content
                    .split(',')
                    .map(|s| s.trim().trim_matches('"').to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
            }
        }
        Vec::new()
    }
}

/// Represents a @component_knowledge annotation block
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ComponentKnowledge(pub(crate) SyntaxNode<BhdlLanguage>);

impl AstNode for ComponentKnowledge {
    type Language = BhdlLanguage;
    
    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::COMPONENT_KNOWLEDGE
    }
    
    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self(syntax))
        } else {
            None
        }
    }
    
    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.0
    }
}

impl ComponentKnowledge {
    /// Get all knowledge sections (e.g., inductor_selection, capacitor_selection)
    pub fn sections(&self) -> HashMap<String, HashMap<String, String>> {
        let mut sections = HashMap::new();
        
        // Parse the component knowledge block
        // This is a simplified parser - real implementation would use proper AST
        let text = self.0.text().to_string();
        let mut current_section = String::new();
        let mut current_props = HashMap::new();
        
        for line in text.lines() {
            let trimmed = line.trim();
            
            // Check for section start
            if trimmed.ends_with(": {") {
                // Save previous section if any
                if !current_section.is_empty() {
                    sections.insert(current_section.clone(), current_props.clone());
                    current_props.clear();
                }
                
                // Start new section
                current_section = trimmed.trim_end_matches(": {").to_string();
            }
            // Check for property in section
            else if trimmed.contains(':') && !trimmed.starts_with('@') {
                if let Some(colon_pos) = trimmed.find(':') {
                    let key = trimmed[..colon_pos].trim().to_string();
                    let value = trimmed[colon_pos + 1..]
                        .trim()
                        .trim_end_matches(',')
                        .to_string();
                    current_props.insert(key, value);
                }
            }
        }
        
        // Save last section
        if !current_section.is_empty() {
            sections.insert(current_section, current_props);
        }
        
        sections
    }
}