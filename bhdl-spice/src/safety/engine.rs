//! Safety Analysis Engine
//! 
//! Orchestrates safety rules and generates analysis results

use std::collections::{HashMap, HashSet};
use super::*;
use crate::circuit::Circuit;
use crate::analysis::AnalysisResult;

/// Configuration for safety analysis
#[derive(Debug, Clone)]
pub struct SafetyConfig {
    pub enabled: bool,
    pub auto_fix: bool,
    pub severity_threshold: Severity,
    pub derating_factors: DeratingFactors,
    pub excluded_rules: HashSet<String>,
    pub custom_limits: HashMap<String, f64>,
}

impl Default for SafetyConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            auto_fix: false,
            severity_threshold: Severity::Info,
            derating_factors: DeratingFactors::default(),
            excluded_rules: HashSet::new(),
            custom_limits: HashMap::new(),
        }
    }
}

/// Derating factors for conservative design
#[derive(Debug, Clone)]
pub struct DeratingFactors {
    pub voltage: f64,      // 0.8 = use 80% of max
    pub current: f64,      // 0.7 = use 70% of max  
    pub power: f64,        // 0.5 = use 50% of max
    pub temperature: f64,  // 0.8 = use 80% of max
}

impl Default for DeratingFactors {
    fn default() -> Self {
        Self {
            voltage: 0.8,      // 80% voltage derating
            current: 0.7,      // 70% current derating
            power: 0.5,        // 50% power derating
            temperature: 0.8,  // 80% temperature derating
        }
    }
}

/// Main safety analysis engine
pub struct SafetyAnalysisEngine {
    rules: Vec<Box<dyn SafetyRule>>,
    config: SafetyConfig,
    /// Component models indexed by name
    component_models: HashMap<String, crate::ComponentModel>,
}

impl SafetyAnalysisEngine {
    /// Create a new safety analysis engine with the given configuration
    pub fn new(config: SafetyConfig) -> Self {
        let mut rules: Vec<Box<dyn SafetyRule>> = vec![];
        
        // Add default rules
        rules.push(Box::new(rules::CurrentLimitingRule::new(
            config.derating_factors.current
        )));
        
        rules.push(Box::new(rules::OvervoltageRule::new(
            config.derating_factors.voltage
        )));
        
        rules.push(Box::new(rules::ShortCircuitRule::new()));
        
        // More rules will be added as implemented
        
        // Sort by priority (highest first)
        rules.sort_by_key(|r| std::cmp::Reverse(r.priority()));
        
        Self { 
            rules, 
            config,
            component_models: HashMap::new(),
        }
    }
    
    /// Add a custom safety rule
    pub fn add_rule(&mut self, rule: Box<dyn SafetyRule>) {
        self.rules.push(rule);
        self.rules.sort_by_key(|r| std::cmp::Reverse(r.priority()));
    }
    
    /// Check if DC analysis is needed for safety checks
    pub fn needs_dc_analysis(&self) -> bool {
        // Most safety rules need DC analysis results
        self.rules.iter().any(|rule| {
            !self.config.excluded_rules.contains(rule.name())
        })
    }
    
    /// Run safety analysis on the circuit
    pub fn analyze(
        &self, 
        circuit: &Circuit, 
        dc_result: Option<&AnalysisResult>
    ) -> SafetyAnalysisResult {
        if !self.config.enabled {
            return SafetyAnalysisResult {
                violations: Vec::new(),
                suggested_fixes: Vec::new(),
                summary: SafetySummary::from_violations(&[]),
            };
        }
        
        let mut all_violations = Vec::new();
        let mut suggested_fixes = Vec::new();
        
        // Run each rule
        for rule in &self.rules {
            if self.config.excluded_rules.contains(rule.name()) {
                log::debug!("Skipping excluded rule: {}", rule.name());
                continue;
            }
            
            log::debug!("Running safety rule: {}", rule.name());
            let violations = rule.check(circuit, dc_result);
            
            // Generate fixes for violations above threshold
            for violation in &violations {
                if violation.severity >= self.config.severity_threshold {
                    if rule.can_auto_fix() {
                        if let Some(fix) = rule.suggest_fix(violation, circuit) {
                            suggested_fixes.push((violation.clone(), fix));
                        }
                    }
                }
            }
            
            all_violations.extend(violations);
        }
        
        // Sort violations by severity (most severe first)
        all_violations.sort_by_key(|v| std::cmp::Reverse(v.severity));
        
        let summary = SafetySummary::from_violations(&all_violations);
        
        SafetyAnalysisResult {
            violations: all_violations,
            suggested_fixes,
            summary,
        }
    }
    
    /// Apply approved modifications to the circuit
    pub fn apply_modifications(
        &self,
        circuit: &mut Circuit,
        modifications: Vec<CircuitModification>
    ) -> Result<Vec<String>, String> {
        if !self.config.auto_fix {
            return Err("Auto-fix is disabled in configuration".to_string());
        }
        
        let mut applied_mods = Vec::new();
        
        for modification in modifications {
            match self.apply_single_modification(circuit, &modification) {
                Ok(description) => applied_mods.push(description),
                Err(e) => {
                    log::error!("Failed to apply modification: {}", e);
                    return Err(format!("Failed to apply modification: {}", e));
                }
            }
        }
        
        Ok(applied_mods)
    }
    
    /// Apply a single modification to the circuit
    fn apply_single_modification(
        &self,
        _circuit: &mut Circuit,
        modification: &CircuitModification
    ) -> Result<String, String> {
        match modification {
            CircuitModification::InsertComponent { 
                component_type, 
                value, 
                from_node, 
                to_node, 
                new_node: _, 
                reason 
            } => {
                // Implementation depends on Circuit API
                // This is a placeholder
                let description = format!(
                    "Inserted {} with value {} between {:?} and {:?}: {}",
                    component_type.to_string(),
                    value,
                    from_node,
                    to_node,
                    reason
                );
                Ok(description)
            }
            
            CircuitModification::ModifyComponentValue {
                instance,
                new_value,
                old_value,
                reason
            } => {
                let description = format!(
                    "Modified component {:?} from {} to {}: {}",
                    instance,
                    old_value,
                    new_value,
                    reason
                );
                Ok(description)
            }
            
            CircuitModification::AddProtectionCircuit {
                protection_type,
                target,
                specifications: _,
                reason
            } => {
                let description = format!(
                    "Added {:?} protection for {:?}: {}",
                    protection_type,
                    target,
                    reason
                );
                Ok(description)
            }
            
            CircuitModification::AddParallelComponent {
                component_type,
                value,
                node1,
                node2,
                reason
            } => {
                let description = format!(
                    "Added {} with value {} in parallel between {:?} and {:?}: {}",
                    component_type.to_string(),
                    value,
                    node1,
                    node2,
                    reason
                );
                Ok(description)
            }
        }
    }
}

impl ComponentType {
    fn to_string(&self) -> &str {
        match self {
            ComponentType::Resistor => "Resistor",
            ComponentType::Capacitor => "Capacitor",
            ComponentType::Inductor => "Inductor",
            ComponentType::Diode => "Diode",
            ComponentType::TVSDiode => "TVS Diode",
            ComponentType::Fuse => "Fuse",
            ComponentType::PTC => "PTC Resettable Fuse",
            ComponentType::GasDischargeTube => "Gas Discharge Tube",
            ComponentType::Zener => "Zener Diode",
            ComponentType::Custom(name) => name,
        }
    }
}