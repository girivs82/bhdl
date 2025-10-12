// Intent Hint Processor
// Applies synthesis hints from intent resolution to guide component selection and optimization

use std::collections::HashMap;
use anyhow::{Result, Context};
use bhdl_analyzer::flow_tracking::FlowTracker;
use bhdl_common::{SynthesisHint, ValidationRule, IntentResult};
use bhdl_netlist::{Netlist, InstanceId};

/// Processes intent hints to guide synthesis decisions
pub struct IntentHintProcessor {
    /// Cache of processed hints by component
    component_hints: HashMap<String, Vec<SynthesisHint>>,
    /// Cache of validation rules by component
    component_validations: HashMap<String, Vec<ValidationRule>>,
    /// Component selection preferences based on hints
    selection_preferences: HashMap<String, ComponentPreference>,
}

/// Component selection preference derived from intent hints
#[derive(Debug, Clone)]
pub struct ComponentPreference {
    pub component_name: String,
    pub preferred_topology: Option<String>,
    pub required_characteristics: Vec<String>,
    pub optimization_priority: OptimizationPriority,
    pub value_constraints: Vec<ValueConstraint>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum OptimizationPriority {
    MinimizeNoise,
    MaximizePrecision,
    MinimizeDelay,
    MaximizeBandwidth,
    MinimizeCost,
    MaximizeReliability,
    Balanced,
}

#[derive(Debug, Clone)]
pub struct ValueConstraint {
    pub parameter: String,
    pub constraint_type: ConstraintType,
    pub value: f64,
    pub unit: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstraintType {
    Minimum,
    Maximum,
    Exactly,
    Range { min: f64, max: f64 },
}

/// Component recommendation based on intent hints
#[derive(Debug, Clone)]
pub struct ComponentRecommendation {
    pub component_type: String,
    pub suggested_value: Option<String>,
    pub rationale: String,
    pub confidence: f64, // 0.0 to 1.0
    pub alternative_options: Vec<String>,
}

impl IntentHintProcessor {
    pub fn new() -> Self {
        Self {
            component_hints: HashMap::new(),
            component_validations: HashMap::new(),
            selection_preferences: HashMap::new(),
        }
    }

    /// Process all intent hints from the flow tracker
    pub fn process_flow_hints(&mut self, flow_tracker: &FlowTracker) -> Result<()> {
        // Extract hints from all flow paths
        for flow_path in flow_tracker.get_flow_paths() {
            if let Some(ref intent_result) = flow_path.intent_result {
                // Process each component in the flow
                for component_name in &flow_path.components {
                    self.process_component_hints(
                        component_name,
                        intent_result,
                    )?;
                }
            }
        }
        Ok(())
    }

    /// Process hints for a specific component
    fn process_component_hints(
        &mut self,
        component_name: &str,
        intent_result: &IntentResult,
    ) -> Result<()> {
        // Store hints for this component
        let hints = intent_result.synthesis_hints.clone();
        self.component_hints.insert(component_name.to_string(), hints.clone());

        // Store validation rules
        let validations = intent_result.validation_rules.clone();
        self.component_validations.insert(component_name.to_string(), validations);

        // Generate component preferences from hints
        let preference = self.analyze_hints(&hints)?;
        self.selection_preferences.insert(component_name.to_string(), preference);

        Ok(())
    }

    /// Analyze synthesis hints to determine component preferences
    fn analyze_hints(&self, hints: &[SynthesisHint]) -> Result<ComponentPreference> {
        let mut preferred_topology = None;
        let mut required_characteristics = Vec::new();
        let mut optimization_priority = OptimizationPriority::Balanced;
        let mut value_constraints = Vec::new();

        for hint in hints {
            match hint {
                SynthesisHint::RCNetwork => {
                    preferred_topology = Some("RC Network".to_string());
                    required_characteristics.push("Time constant control".to_string());
                }
                SynthesisHint::AnalogFilter => {
                    preferred_topology = Some("Analog Filter".to_string());
                    required_characteristics.push("Frequency selective".to_string());
                    optimization_priority = OptimizationPriority::MaximizeBandwidth;
                }
                SynthesisHint::DigitalFilter => {
                    preferred_topology = Some("Digital Filter".to_string());
                    required_characteristics.push("Discrete-time filtering".to_string());
                    optimization_priority = OptimizationPriority::MinimizeDelay;
                }
                SynthesisHint::BufferChain => {
                    preferred_topology = Some("Buffer Chain".to_string());
                    required_characteristics.push("Low propagation delay".to_string());
                    optimization_priority = OptimizationPriority::MinimizeDelay;
                }
                SynthesisHint::ActiveDelay => {
                    preferred_topology = Some("Active Delay Element".to_string());
                    required_characteristics.push("Precise timing".to_string());
                }
                SynthesisHint::Custom(hint_text) => {
                    // Parse custom hints for specific requirements
                    self.parse_custom_hint(hint_text, &mut required_characteristics, &mut value_constraints);
                }
            }
        }

        Ok(ComponentPreference {
            component_name: String::new(), // Will be set by caller
            preferred_topology,
            required_characteristics,
            optimization_priority,
            value_constraints,
        })
    }

    /// Parse custom hint text for additional requirements
    fn parse_custom_hint(
        &self,
        hint_text: &str,
        characteristics: &mut Vec<String>,
        constraints: &mut Vec<ValueConstraint>,
    ) {
        let hint_lower = hint_text.to_lowercase();

        // Detect specific patterns in custom hints
        if hint_lower.contains("low-noise") || hint_lower.contains("low noise") {
            characteristics.push("Low noise operation".to_string());
        }
        if hint_lower.contains("precision") || hint_lower.contains("accurate") {
            characteristics.push("High precision".to_string());
        }
        if hint_lower.contains("high speed") || hint_lower.contains("fast") {
            characteristics.push("High speed operation".to_string());
        }
        if hint_lower.contains("low power") || hint_lower.contains("efficient") {
            characteristics.push("Low power consumption".to_string());
        }

        // Extract numeric constraints (e.g., "max 20mA", "min 10kHz")
        if let Some(max_current) = self.extract_max_current(hint_text) {
            constraints.push(ValueConstraint {
                parameter: "current".to_string(),
                constraint_type: ConstraintType::Maximum,
                value: max_current,
                unit: Some("A".to_string()),
            });
        }
    }

    /// Extract maximum current from hint text
    fn extract_max_current(&self, text: &str) -> Option<f64> {
        // Look for patterns like "max 20mA", "limit: 50mA", etc.
        if text.contains("mA") {
            // Simple pattern matching - in production would use regex
            if let Some(pos) = text.find("mA") {
                let before = &text[..pos];
                if let Some(num_start) = before.rfind(|c: char| !c.is_numeric() && c != '.') {
                    let num_str = &before[num_start + 1..];
                    if let Ok(ma) = num_str.trim().parse::<f64>() {
                        return Some(ma / 1000.0); // Convert mA to A
                    }
                }
            }
        }
        None
    }

    /// Get component recommendations based on intent hints
    pub fn get_component_recommendation(
        &self,
        component_name: &str,
    ) -> Option<ComponentRecommendation> {
        let preference = self.selection_preferences.get(component_name)?;

        let rationale = self.build_rationale(preference);
        let suggested_value = self.suggest_value(preference);

        Some(ComponentRecommendation {
            component_type: self.infer_component_type(preference),
            suggested_value,
            rationale,
            confidence: 0.8, // Based on hint clarity
            alternative_options: self.generate_alternatives(preference),
        })
    }

    /// Infer component type from preferences
    fn infer_component_type(&self, preference: &ComponentPreference) -> String {
        if let Some(ref topology) = preference.preferred_topology {
            match topology.as_str() {
                "RC Network" => "Resistor+Capacitor".to_string(),
                "Analog Filter" => "RC Filter".to_string(),
                "Buffer Chain" => "Buffer IC".to_string(),
                _ => "Generic Component".to_string(),
            }
        } else {
            "Generic Component".to_string()
        }
    }

    /// Suggest component values based on constraints
    fn suggest_value(&self, preference: &ComponentPreference) -> Option<String> {
        // Look for current limiting constraints
        for constraint in &preference.value_constraints {
            if constraint.parameter == "current" {
                match constraint.constraint_type {
                    ConstraintType::Maximum => {
                        // Suggest resistor value for current limiting
                        // Assuming 5V supply: R = V / I
                        let resistance = 5.0 / constraint.value;
                        return Some(format!("{:.1}kΩ", resistance / 1000.0));
                    }
                    _ => {}
                }
            }
        }
        None
    }

    /// Build rationale for component selection
    fn build_rationale(&self, preference: &ComponentPreference) -> String {
        let mut rationale_parts = Vec::new();

        if let Some(ref topology) = preference.preferred_topology {
            rationale_parts.push(format!("Topology: {}", topology));
        }

        if !preference.required_characteristics.is_empty() {
            rationale_parts.push(format!(
                "Requirements: {}",
                preference.required_characteristics.join(", ")
            ));
        }

        match preference.optimization_priority {
            OptimizationPriority::MinimizeNoise => {
                rationale_parts.push("Optimized for low noise".to_string());
            }
            OptimizationPriority::MaximizePrecision => {
                rationale_parts.push("Optimized for precision".to_string());
            }
            OptimizationPriority::MinimizeDelay => {
                rationale_parts.push("Optimized for speed".to_string());
            }
            _ => {}
        }

        rationale_parts.join("; ")
    }

    /// Generate alternative component options
    fn generate_alternatives(&self, preference: &ComponentPreference) -> Vec<String> {
        let mut alternatives = Vec::new();

        if let Some(ref topology) = preference.preferred_topology {
            match topology.as_str() {
                "RC Network" => {
                    alternatives.push("Active RC filter".to_string());
                    alternatives.push("Switched capacitor filter".to_string());
                }
                "Analog Filter" => {
                    alternatives.push("Sallen-Key topology".to_string());
                    alternatives.push("Multiple feedback topology".to_string());
                }
                "Buffer Chain" => {
                    alternatives.push("Single high-drive buffer".to_string());
                    alternatives.push("Inverter chain".to_string());
                }
                _ => {}
            }
        }

        alternatives
    }

    /// Get validation rules for a component
    pub fn get_validation_rules(&self, component_name: &str) -> Vec<ValidationRule> {
        self.component_validations
            .get(component_name)
            .cloned()
            .unwrap_or_default()
    }

    /// Check if component selection satisfies intent hints
    pub fn validate_component_selection(
        &self,
        component_name: &str,
        selected_type: &str,
        selected_value: Option<&str>,
    ) -> Result<ValidationResult> {
        let mut warnings = Vec::new();
        let mut suggestions = Vec::new();

        // Get preferences for this component
        if let Some(preference) = self.selection_preferences.get(component_name) {
            // Check topology match
            if let Some(ref preferred_topology) = preference.preferred_topology {
                if !selected_type.contains(preferred_topology) {
                    warnings.push(format!(
                        "Selected type '{}' may not match preferred topology '{}'",
                        selected_type, preferred_topology
                    ));
                }
            }

            // Check value constraints
            if let Some(value_str) = selected_value {
                for constraint in &preference.value_constraints {
                    if !self.check_constraint(value_str, constraint) {
                        warnings.push(format!(
                            "Value '{}' may not satisfy constraint: {:?}",
                            value_str, constraint
                        ));
                    }
                }
            }

            // Generate suggestions based on optimization priority
            match preference.optimization_priority {
                OptimizationPriority::MinimizeNoise => {
                    suggestions.push("Consider using components with low noise figures".to_string());
                }
                OptimizationPriority::MaximizePrecision => {
                    suggestions.push("Consider using 1% tolerance components".to_string());
                }
                _ => {}
            }
        }

        Ok(ValidationResult {
            is_valid: warnings.is_empty(),
            warnings,
            suggestions,
        })
    }

    /// Check if a value satisfies a constraint
    fn check_constraint(&self, value_str: &str, constraint: &ValueConstraint) -> bool {
        // Simple validation - in production would parse units properly
        // For now, just check if we can extract a number
        if let Some(num) = self.extract_number(value_str) {
            match constraint.constraint_type {
                ConstraintType::Maximum => num <= constraint.value,
                ConstraintType::Minimum => num >= constraint.value,
                ConstraintType::Exactly => (num - constraint.value).abs() < 0.01,
                ConstraintType::Range { min, max } => num >= min && num <= max,
            }
        } else {
            true // Can't validate, assume OK
        }
    }

    /// Extract numeric value from string (simple implementation)
    fn extract_number(&self, text: &str) -> Option<f64> {
        // Remove common unit suffixes and parse
        let cleaned = text
            .replace("kΩ", "e3")
            .replace("MΩ", "e6")
            .replace("mA", "e-3")
            .replace("µF", "e-6")
            .replace("nF", "e-9")
            .replace("pF", "e-12");

        cleaned.chars()
            .take_while(|c| c.is_numeric() || *c == '.' || *c == 'e' || *c == '-')
            .collect::<String>()
            .parse::<f64>()
            .ok()
    }
}

/// Result of component validation against intent hints
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub warnings: Vec<String>,
    pub suggestions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_rc_network_hint() {
        let processor = IntentHintProcessor::new();
        let hints = vec![SynthesisHint::RCNetwork];

        let preference = processor.analyze_hints(&hints).unwrap();

        assert_eq!(preference.preferred_topology, Some("RC Network".to_string()));
        assert!(preference.required_characteristics.contains(&"Time constant control".to_string()));
    }

    #[test]
    fn test_parse_custom_hint_low_noise() {
        let processor = IntentHintProcessor::new();
        let mut characteristics = Vec::new();
        let mut constraints = Vec::new();

        processor.parse_custom_hint("Use low-noise components", &mut characteristics, &mut constraints);

        assert!(characteristics.contains(&"Low noise operation".to_string()));
    }

    #[test]
    fn test_extract_max_current() {
        let processor = IntentHintProcessor::new();

        assert_eq!(processor.extract_max_current("max 20mA"), Some(0.02));
        assert_eq!(processor.extract_max_current("limit: 50mA"), Some(0.05));
    }
}
