//! Short circuit detection safety rule
//! 
//! Detects direct or near-short circuit conditions

use crate::safety::*;
use crate::circuit::{Circuit, NodeId};
use crate::analysis::AnalysisResult;
use std::time::Duration;

/// Rule to detect short circuits
pub struct ShortCircuitRule {
    min_resistance_threshold: f64, // Ohms
}

impl ShortCircuitRule {
    pub fn new() -> Self {
        Self {
            min_resistance_threshold: 0.1, // 100 milliohms
        }
    }
}

impl Default for ShortCircuitRule {
    fn default() -> Self {
        Self::new()
    }
}

impl SafetyRule for ShortCircuitRule {
    fn name(&self) -> &str {
        "Short Circuit Detection"
    }
    
    fn default_severity(&self) -> Severity {
        Severity::Critical
    }
    
    fn priority(&self) -> u32 {
        1100 // Highest priority - prevents fires
    }
    
    fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        
        // Check for direct power to ground connections
        let power_nodes = circuit.get_power_nodes();
        let ground_nodes = circuit.get_ground_nodes();
        
        for power_node in &power_nodes {
            for ground_node in &ground_nodes {
                // Check if there's a low-resistance path
                if let Some(resistance) = self.measure_path_resistance(circuit, *power_node, *ground_node) {
                    if resistance < self.min_resistance_threshold {
                        violations.push(self.create_short_circuit_violation(
                            circuit,
                            *power_node,
                            *ground_node,
                            resistance,
                        ));
                    } else if resistance < 1.0 {
                        // Near-short condition
                        violations.push(self.create_near_short_violation(
                            circuit,
                            *power_node,
                            *ground_node,
                            resistance,
                        ));
                    }
                }
            }
        }
        
        // If we have DC analysis results, check for excessive currents
        if let Some(result) = dc_result {
            // Check supply current
            if let Some(&supply_current) = result.get_supply_current() {
                let supply_voltage = circuit.get_supply_voltage().unwrap_or(5.0);
                let apparent_resistance = supply_voltage / supply_current.abs();
                
                if apparent_resistance < self.min_resistance_threshold {
                    violations.push(SafetyViolation {
                        rule_name: self.name().to_string(),
                        severity: Severity::Critical,
                        location: CircuitLocation {
                            components: vec![],
                            nodes: power_nodes.clone(),
                            nets: vec!["Power Supply".to_string()],
                            description: "Power supply output".to_string(),
                        },
                        message: format!(
                            "Excessive supply current {:.1}A indicates short circuit (apparent resistance {:.3}Ω)",
                            supply_current.abs(),
                            apparent_resistance
                        ),
                        technical_details: format!(
                            "Supply: {:.1}V, Current: {:.2}A, Apparent R: {:.3}Ω",
                            supply_voltage,
                            supply_current.abs(),
                            apparent_resistance
                        ),
                        user_impact: "Power supply overload - fire hazard!".to_string(),
                        estimated_damage: Some(DamageEstimate {
                            failure_mode: "Thermal runaway and possible fire".to_string(),
                            time_to_failure: Some(Duration::from_secs(1)),
                            affected_components: vec!["Power Supply".to_string(), "PCB Traces".to_string()],
                            estimated_cost: Some(1000.0), // High cost due to potential fire damage
                        }),
                    });
                }
            }
            
            // Check individual component currents for abnormal values
            for (component_id, component) in circuit.components() {
                if let Some(&current) = result.get_component_current(component_id) {
                    // Check if current is way beyond normal for component type
                    if let Some(typical_current) = self.get_typical_current(component) {
                        if current.abs() > typical_current * 10.0 {
                            violations.push(SafetyViolation {
                                rule_name: self.name().to_string(),
                                severity: Severity::Error,
                                location: CircuitLocation {
                                    components: vec![component_id],
                                    nodes: component.nodes().to_vec(),
                                    nets: vec![],
                                    description: format!("Component {}", component.name()),
                                },
                                message: format!(
                                    "{} carrying abnormal current {:.1}A (typical: {:.1}mA)",
                                    component.name(),
                                    current.abs(),
                                    typical_current * 1000.0
                                ),
                                technical_details: format!(
                                    "Measured: {:.3}A, Expected: <{:.3}A",
                                    current.abs(),
                                    typical_current
                                ),
                                user_impact: "Possible short circuit through component".to_string(),
                                estimated_damage: None,
                            });
                        }
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
        _circuit: &Circuit
    ) -> Option<CircuitModification> {
        if violation.message.contains("short circuit") {
            // For shorts, we can suggest adding a fuse or PTC
            let current_limit = if violation.message.contains("Excessive supply current") {
                // Extract current from message
                1.0 // Default 1A fuse
            } else {
                0.5 // Default 500mA for other cases
            };
            
            Some(CircuitModification::AddProtectionCircuit {
                protection_type: ProtectionType::OvercurrentProtection,
                target: ProtectionTarget::PowerInput,
                specifications: {
                    let mut specs = HashMap::new();
                    specs.insert("current_rating".to_string(), current_limit);
                    specs.insert("type".to_string(), 1.0); // 1 = fuse, 2 = PTC
                    specs
                },
                reason: format!(
                    "Add {}A fuse to protect against short circuit",
                    current_limit
                ),
            })
        } else {
            None
        }
    }
}

impl ShortCircuitRule {
    fn create_short_circuit_violation(
        &self,
        circuit: &Circuit,
        power_node: NodeId,
        ground_node: NodeId,
        resistance: f64,
    ) -> SafetyViolation {
        let power_name = circuit.get_node_name(power_node).unwrap_or("Power");
        let ground_name = circuit.get_node_name(ground_node).unwrap_or("Ground");
        
        SafetyViolation {
            rule_name: self.name().to_string(),
            severity: Severity::Critical,
            location: CircuitLocation {
                components: vec![],
                nodes: vec![power_node, ground_node],
                nets: vec![power_name.to_string(), ground_name.to_string()],
                description: format!("{} to {} path", power_name, ground_name),
            },
            message: format!(
                "SHORT CIRCUIT: Direct path from {} to {} with only {:.3}Ω resistance",
                power_name,
                ground_name,
                resistance
            ),
            technical_details: format!(
                "Path resistance: {:.3}Ω, Threshold: {:.3}Ω",
                resistance,
                self.min_resistance_threshold
            ),
            user_impact: "FIRE HAZARD - Circuit will draw excessive current and overheat".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Thermal damage, possible fire".to_string(),
                time_to_failure: Some(Duration::from_millis(100)),
                affected_components: vec!["Power Supply".to_string(), "PCB".to_string(), "All components".to_string()],
                estimated_cost: Some(10000.0), // Very high due to fire risk
            }),
        }
    }
    
    fn create_near_short_violation(
        &self,
        circuit: &Circuit,
        power_node: NodeId,
        ground_node: NodeId,
        resistance: f64,
    ) -> SafetyViolation {
        let power_name = circuit.get_node_name(power_node).unwrap_or("Power");
        let ground_name = circuit.get_node_name(ground_node).unwrap_or("Ground");
        
        SafetyViolation {
            rule_name: self.name().to_string(),
            severity: Severity::Error,
            location: CircuitLocation {
                components: vec![],
                nodes: vec![power_node, ground_node],
                nets: vec![power_name.to_string(), ground_name.to_string()],
                description: format!("{} to {} path", power_name, ground_name),
            },
            message: format!(
                "Near-short condition: Low resistance path ({:.2}Ω) from {} to {}",
                resistance,
                power_name,
                ground_name
            ),
            technical_details: format!(
                "Path resistance: {:.3}Ω (very low)",
                resistance
            ),
            user_impact: "High current draw will cause overheating and component stress".to_string(),
            estimated_damage: Some(DamageEstimate {
                failure_mode: "Overheating and component degradation".to_string(),
                time_to_failure: Some(Duration::from_secs(60)),
                affected_components: vec!["Components in path".to_string()],
                estimated_cost: Some(100.0),
            }),
        }
    }
    
    fn measure_path_resistance(&self, circuit: &Circuit, from: NodeId, to: NodeId) -> Option<f64> {
        // Simplified resistance calculation
        // In a real implementation, this would solve the resistance network
        
        // Quick check for direct connection
        let components_at_from = circuit.get_components_at_node(from);
        let components_at_to = circuit.get_components_at_node(to);
        
        // Find components connected to both nodes
        let mut min_resistance = f64::INFINITY;
        
        for &comp_id in &components_at_from {
            if components_at_to.contains(&comp_id) {
                // This component connects both nodes
                if let Some(component) = circuit.get_component(comp_id) {
                    if let Some(resistance) = component.resistance() {
                        min_resistance = min_resistance.min(resistance);
                    } else if matches!(component.component_type(), "Wire" | "Trace") {
                        min_resistance = 0.001; // milliohm for wire
                    }
                }
            }
        }
        
        if min_resistance.is_finite() {
            Some(min_resistance)
        } else {
            None
        }
    }
    
    fn get_typical_current(&self, component: &crate::circuit::Component) -> Option<f64> {
        // Return typical operating current for component type
        match component.component_type() {
            "LED" => Some(0.02), // 20mA
            "Resistor" => {
                // Calculate based on typical power
                if let Some(resistance) = component.resistance() {
                    let typical_power = 0.1; // 100mW typical
                    Some((typical_power / resistance).sqrt())
                } else {
                    None
                }
            }
            "IC" | "MCU" => Some(0.1), // 100mA typical
            "OpAmp" => Some(0.005), // 5mA
            "Capacitor" => Some(0.001), // Should have very low DC current
            _ => None,
        }
    }
}