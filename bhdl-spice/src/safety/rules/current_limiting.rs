//! Current limiting safety rule
//! 
//! Detects components that need current limiting but don't have it

use crate::safety::*;
use crate::circuit::{Circuit, Component, ComponentId};
use crate::analysis::AnalysisResult;
use std::time::Duration;

/// Rule to check for proper current limiting
pub struct CurrentLimitingRule {
    derating_factor: f64,
}

impl CurrentLimitingRule {
    pub fn new(derating_factor: f64) -> Self {
        Self { derating_factor }
    }
    
    /// Calculate appropriate current limiting resistor value
    fn calculate_limiting_resistor(
        &self,
        component: &Component,
        supply_voltage: f64,
    ) -> f64 {
        let target_current = match component.component_type() {
            "LED" => {
                // Get LED forward voltage based on color
                let vf = component.get_parameter("forward_voltage")
                    .or_else(|| component.get_parameter("vf"))
                    .unwrap_or(2.0); // Default 2V for red LED
                
                let target = component.get_parameter("nominal_current")
                    .or_else(|| component.get_parameter("if"))
                    .unwrap_or(0.015); // Default 15mA
                
                (supply_voltage - vf) / target
            }
            "LaserDiode" => {
                let vf = component.get_parameter("forward_voltage").unwrap_or(1.8);
                let target = component.get_parameter("operating_current").unwrap_or(0.005); // 5mA
                (supply_voltage - vf) / target
            }
            _ => {
                // Generic calculation
                let max_current = component.max_current().unwrap_or(0.1);
                supply_voltage / (max_current * self.derating_factor)
            }
        };
        
        super::round_to_e12(target_current)
    }
}

impl SafetyRule for CurrentLimitingRule {
    fn name(&self) -> &str {
        "Current Limiting Check"
    }
    
    fn default_severity(&self) -> Severity {
        Severity::Critical
    }
    
    fn priority(&self) -> u32 {
        1000 // Very high priority - prevents immediate damage
    }
    
    fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        
        // Without DC analysis results, we can't check current safety
        if dc_result.is_none() {
            // Could add a warning that safety analysis needs DC results
            return violations;
        }
        
        let result = dc_result.unwrap();
        
        // Check each component
        for (component_id, component) in circuit.components() {
            if let Some(max_current) = component.max_current() {
                if let Some(&current) = result.get_component_current(component_id) {
                    let abs_current = current.abs();
                    let derated_max = max_current * self.derating_factor;
                    
                    if abs_current > max_current {
                        // Critical overcurrent
                        violations.push(SafetyViolation {
                            rule_name: self.name().to_string(),
                            severity: Severity::Critical,
                            location: CircuitLocation {
                                components: vec![component_id],
                                nodes: component.nodes().to_vec(),
                                nets: vec![],
                                description: format!("Component {}", component.name()),
                            },
                            message: format!(
                                "{} current {:.1}mA exceeds absolute maximum {:.1}mA",
                                component.name(),
                                abs_current * 1000.0,
                                max_current * 1000.0
                            ),
                            technical_details: format!(
                                "Measured: {:.3}A, Max: {:.3}A, Overcurrent ratio: {:.1}x",
                                abs_current,
                                max_current,
                                abs_current / max_current
                            ),
                            user_impact: "Component will fail immediately".to_string(),
                            estimated_damage: Some(DamageEstimate {
                                failure_mode: "Overcurrent failure".to_string(),
                                time_to_failure: Some(Duration::from_millis(10)),
                                affected_components: vec![component.name().to_string()],
                                estimated_cost: component.cost(),
                            }),
                        });
                    } else if abs_current > derated_max {
                        // Warning - exceeds derating
                        violations.push(SafetyViolation {
                            rule_name: self.name().to_string(),
                            severity: Severity::Warning,
                            location: CircuitLocation {
                                components: vec![component_id],
                                nodes: component.nodes().to_vec(),
                                nets: vec![],
                                description: format!("Component {}", component.name()),
                            },
                            message: format!(
                                "{} current {:.1}mA exceeds {:.0}% derating limit of {:.1}mA",
                                component.name(),
                                abs_current * 1000.0,
                                self.derating_factor * 100.0,
                                derated_max * 1000.0
                            ),
                            technical_details: format!(
                                "Measured: {:.3}A, Derated max: {:.3}A",
                                abs_current,
                                derated_max
                            ),
                            user_impact: "Reduced component lifetime and reliability".to_string(),
                            estimated_damage: None,
                        });
                    }
                }
            }
        }
        
        violations
    }
    
    fn can_auto_fix(&self) -> bool {
        true
    }
    
    fn suggest_fix(
        &self, 
        violation: &SafetyViolation, 
        circuit: &Circuit
    ) -> Option<CircuitModification> {
        // Only suggest fixes for overcurrent violations
        if !violation.message.contains("exceeds absolute maximum") {
            return None;
        }
        
        // Get the component that needs protection
        let component_id = *violation.location.components.first()?;
        let component = circuit.get_component(component_id)?;
        
        // Get supply voltage (simplified - would trace actual path)
        let supply_voltage = circuit.get_supply_voltage().unwrap_or(5.0);
        
        // Calculate resistor value
        let resistance = self.calculate_limiting_resistor(component, supply_voltage);
        
        // Get the nodes to insert between
        let nodes = component.nodes();
        if nodes.len() >= 2 {
            Some(CircuitModification::InsertComponent {
                component_type: ComponentType::Resistor,
                value: ComponentValue::Resistance(resistance),
                from_node: nodes[0], // Assuming first node is input
                to_node: nodes[0],   // Will create intermediate node
                new_node: Some(format!("{}_limited", component.name())),
                reason: format!(
                    "Add {}Ω current limiting resistor for {} protection",
                    super::format_engineering(resistance),
                    component.name()
                ),
            })
        } else {
            None
        }
    }
}