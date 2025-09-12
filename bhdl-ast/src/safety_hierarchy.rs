//! Hierarchical safety requirements AST nodes for ISO 26262 compliance
//! 
//! This module provides AST nodes for representing hierarchical safety requirements
//! including safety goals, functional requirements, technical requirements, and
//! their decomposition relationships.

use crate::{AstNode, BhdlLanguage, SyntaxKind, SyntaxNode, SyntaxToken};
use std::collections::HashMap;

/// Requirement level in the V-model hierarchy
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum RequirementLevel {
    /// Safety Goal - highest level from hazard analysis
    SafetyGoal,
    /// Functional Safety Requirement - what the system must do
    Functional,
    /// Technical Safety Requirement - how it will be implemented
    Technical,
    /// Hardware/Software requirement - specific implementation
    Implementation,
}

/// ASIL levels per ISO 26262
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ASILLevel {
    QM,     // Quality Management (no safety relevance)
    ASIL_A, // Lowest safety level
    ASIL_B,
    ASIL_C,
    ASIL_D, // Highest safety level
}

impl ASILLevel {
    pub fn from_string(s: &str) -> Option<Self> {
        match s {
            "QM" => Some(Self::QM),
            "ASIL_A" | "ASIL-A" => Some(Self::ASIL_A),
            "ASIL_B" | "ASIL-B" => Some(Self::ASIL_B),
            "ASIL_C" | "ASIL-C" => Some(Self::ASIL_C),
            "ASIL_D" | "ASIL-D" => Some(Self::ASIL_D),
            _ => None,
        }
    }
}

/// Represents a requirement definition (safety_goal, functional_requirement, etc.)
#[derive(Debug, Clone)]
pub struct RequirementDef {
    pub(crate) syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for RequirementDef {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        matches!(kind, 
            SyntaxKind::SAFETY_GOAL_DEF |
            SyntaxKind::FUNCTIONAL_REQ_DEF |
            SyntaxKind::TECHNICAL_REQ_DEF
        )
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl RequirementDef {
    /// Get the requirement ID
    pub fn id(&self) -> Option<SyntaxToken<BhdlLanguage>> {
        self.syntax
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
    }
    
    /// Get the requirement level
    pub fn level(&self) -> RequirementLevel {
        match self.syntax.kind() {
            SyntaxKind::SAFETY_GOAL_DEF => RequirementLevel::SafetyGoal,
            SyntaxKind::FUNCTIONAL_REQ_DEF => RequirementLevel::Functional,
            SyntaxKind::TECHNICAL_REQ_DEF => RequirementLevel::Technical,
            _ => RequirementLevel::Implementation,
        }
    }
    
    /// Get requirement properties (description, asil, etc.)
    pub fn properties(&self) -> RequirementProperties {
        let mut props = RequirementProperties::default();
        
        // Parse the requirement body for properties
        for child in self.syntax.children() {
            if child.kind() == SyntaxKind::REQ_PROPERTY {
                if let Some(prop) = RequirementProperty::cast(child) {
                    match prop.name().as_deref() {
                        Some("description") => props.description = prop.value(),
                        Some("asil") => props.asil = prop.value().and_then(|s| ASILLevel::from_string(&s)),
                        Some("derived_from") => props.derived_from = prop.list_value(),
                        Some("decomposes_to") => props.decomposes_to = prop.list_value(),
                        Some("hazard") => props.hazard = prop.value(),
                        _ => {}
                    }
                }
            }
        }
        
        props
    }
}

/// Properties of a requirement
#[derive(Debug, Clone, Default)]
pub struct RequirementProperties {
    pub description: Option<String>,
    pub asil: Option<ASILLevel>,
    pub derived_from: Vec<String>,
    pub decomposes_to: Vec<String>,
    pub hazard: Option<String>,
}

/// A single property in a requirement definition
#[derive(Debug, Clone)]
pub struct RequirementProperty {
    pub(crate) syntax: SyntaxNode<BhdlLanguage>,
}

impl AstNode for RequirementProperty {
    type Language = BhdlLanguage;

    fn can_cast(kind: SyntaxKind) -> bool {
        kind == SyntaxKind::REQ_PROPERTY
    }

    fn cast(syntax: SyntaxNode<BhdlLanguage>) -> Option<Self> {
        if Self::can_cast(syntax.kind()) {
            Some(Self { syntax })
        } else {
            None
        }
    }

    fn syntax(&self) -> &SyntaxNode<BhdlLanguage> {
        &self.syntax
    }
}

impl RequirementProperty {
    pub fn name(&self) -> Option<String> {
        self.syntax
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| token.kind() == SyntaxKind::IDENT)
            .map(|t| t.text().to_string())
    }
    
    pub fn value(&self) -> Option<String> {
        self.syntax
            .children_with_tokens()
            .filter_map(|element| element.into_token())
            .find(|token| matches!(token.kind(), SyntaxKind::STRING | SyntaxKind::IDENT))
            .map(|t| t.text().trim_matches('"').to_string())
    }
    
    pub fn list_value(&self) -> Vec<String> {
        // Parse array values like [REQ_001, REQ_002]
        let mut values = Vec::new();
        let mut in_list = false;
        
        for element in self.syntax.children_with_tokens() {
            if let Some(token) = element.as_token() {
                match token.kind() {
                    SyntaxKind::L_BRACKET => in_list = true,
                    SyntaxKind::R_BRACKET => in_list = false,
                    SyntaxKind::IDENT if in_list => {
                        values.push(token.text().to_string());
                    }
                    _ => {}
                }
            }
        }
        
        values
    }
}

/// Enhanced satisfies specification for hierarchical requirements
#[derive(Debug, Clone)]
pub enum HierarchicalSatisfiesSpec {
    /// Direct implementation by component(s)
    ViaComponent {
        components: Vec<String>,
        coverage: Option<f64>,
        verification: Option<String>,
    },
    
    /// Composed from child requirements
    ComposedOf {
        requirements: Vec<String>,
        rationale: Option<String>,
    },
    
    /// Allocated to subsystem/module
    AllocatedTo {
        subsystem: String,
        requirements: Vec<String>,
    },
    
    /// Multiple components with strategy
    ViaMultiple {
        components: Vec<String>,
        strategy: String, // "redundant", "diverse", "voting"
        coverage: Option<f64>,
    },
}

/// Parse enhanced satisfies item that supports hierarchical relationships
pub fn parse_hierarchical_satisfies_item(item: &crate::safety::SatisfiesItem) -> Option<(String, HierarchicalSatisfiesSpec)> {
    let req_id = item.requirement_id()?.text().to_string();
    
    // Check for composed_of keyword
    let has_composed = item.syntax()
        .children_with_tokens()
        .any(|e| e.as_token()
            .map(|t| t.text() == "composed_of")
            .unwrap_or(false));
    
    if has_composed {
        // Parse composed_of [REQ1, REQ2, ...]
        let requirements = parse_requirement_list(item.syntax());
        let rationale = parse_property(item.syntax(), "rationale");
        
        return Some((req_id, HierarchicalSatisfiesSpec::ComposedOf {
            requirements,
            rationale,
        }));
    }
    
    // Check for allocated_to keyword
    let has_allocated = item.syntax()
        .children_with_tokens()
        .any(|e| e.as_token()
            .map(|t| t.text() == "allocated_to")
            .unwrap_or(false));
    
    if has_allocated {
        let subsystem = parse_next_ident_after(item.syntax(), "allocated_to")?;
        let requirements = parse_requirement_list(item.syntax());
        
        return Some((req_id, HierarchicalSatisfiesSpec::AllocatedTo {
            subsystem,
            requirements,
        }));
    }
    
    // Check for via with multiple components
    if let Some(spec) = item.satisfaction() {
        match spec {
            crate::safety::SatisfiesSpec::Via(via) => {
                let components = via.component_paths();
                let coverage = parse_coverage(item.syntax());
                let verification = parse_property(item.syntax(), "verification");
                
                return Some((req_id, HierarchicalSatisfiesSpec::ViaComponent {
                    components,
                    coverage,
                    verification,
                }));
            }
            crate::safety::SatisfiesSpec::Details(details) => {
                let fields = details.fields();
                
                // Check if this is a multi-component satisfaction
                let fields_map: HashMap<String, String> = fields.into_iter().collect();
                if let Some(strategy) = fields_map.get("strategy") {
                    let components = parse_component_list(item.syntax());
                    let coverage = fields_map.get("coverage")
                        .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok());
                    
                    return Some((req_id, HierarchicalSatisfiesSpec::ViaMultiple {
                        components,
                        strategy: strategy.clone(),
                        coverage,
                    }));
                }
            }
        }
    }
    
    None
}

// Helper functions for parsing
fn parse_requirement_list(node: &SyntaxNode<BhdlLanguage>) -> Vec<String> {
    let mut requirements = Vec::new();
    let mut in_list = false;
    
    for element in node.children_with_tokens() {
        if let Some(token) = element.as_token() {
            match token.kind() {
                SyntaxKind::L_BRACKET => in_list = true,
                SyntaxKind::R_BRACKET => in_list = false,
                SyntaxKind::IDENT if in_list => {
                    requirements.push(token.text().to_string());
                }
                _ => {}
            }
        }
    }
    
    requirements
}

fn parse_component_list(node: &SyntaxNode<BhdlLanguage>) -> Vec<String> {
    // Similar to parse_requirement_list but for component names
    parse_requirement_list(node)
}

fn parse_property(node: &SyntaxNode<BhdlLanguage>, property: &str) -> Option<String> {
    let mut found_property = false;
    
    for element in node.children_with_tokens() {
        if let Some(token) = element.as_token() {
            if found_property && token.kind() == SyntaxKind::STRING {
                return Some(token.text().trim_matches('"').to_string());
            }
            if token.text() == property {
                found_property = true;
            }
        }
    }
    
    None
}

fn parse_coverage(node: &SyntaxNode<BhdlLanguage>) -> Option<f64> {
    parse_property(node, "coverage")
        .and_then(|s| s.trim_end_matches('%').parse::<f64>().ok())
}

fn parse_next_ident_after(node: &SyntaxNode<BhdlLanguage>, keyword: &str) -> Option<String> {
    let mut found_keyword = false;
    
    for element in node.children_with_tokens() {
        if let Some(token) = element.as_token() {
            if found_keyword && token.kind() == SyntaxKind::IDENT {
                return Some(token.text().to_string());
            }
            if token.text() == keyword {
                found_keyword = true;
            }
        }
    }
    
    None
}