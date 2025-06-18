//! Overvoltage protection safety rule
//! 
//! Detects components exposed to voltages beyond their ratings

use crate::safety::*;
use crate::circuit::{Circuit, Component, ComponentId};
use crate::analysis::AnalysisResult;
use std::time::Duration;

/// Rule to check for overvoltage conditions
pub struct OvervoltageRule {
    derating_factor: f64,
}

impl OvervoltageRule {
    pub fn new(derating_factor: f64) -> Self {
        Self { derating_factor }
    }
    
    /// Get safe operating voltage for a component
    fn get_safe_voltage(&self, component: &Component) -> Option<f64> {
        component.max_voltage().map(|v| v * self.derating_factor)
    }
}

impl SafetyRule for OvervoltageRule {
    fn name(&self) -> &str {
        "Overvoltage Protection Check"
    }
    
    fn default_severity(&self) -> Severity {
        Severity::Critical
    }
    
    fn priority(&self) -> u32 {
        900 // High priority
    }
    
    fn check(&self, circuit: &Circuit, dc_result: Option<&AnalysisResult>) -> Vec<SafetyViolation> {
        let mut violations = Vec::new();
        
        // Without DC analysis results, we can't check voltage safety
        if dc_result.is_none() {
            return violations;
        }
        
        let result = dc_result.unwrap();
        
        // Check each component
        for (component_id, component) in circuit.components() {
            if let Some(max_voltage) = component.max_voltage() {
                let derated_max = max_voltage * self.derating_factor;
                // Get voltage across component
                if let Some(&voltage) = result.get_component_voltage(component_id) {
                    let abs_voltage = voltage.abs();
                    
                    if abs_voltage > max_voltage {
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
                                    "{} voltage {:.1}V exceeds absolute maximum {:.1}V",
                                    component.name(),
                                    abs_voltage,
                                    max_voltage
                                ),
                                technical_details: format!(
                                    "Applied: {:.2}V, Max: {:.2}V, Overvoltage: {:.0}%",
                                    abs_voltage,
                                    max_voltage,
                                    ((abs_voltage / max_voltage) - 1.0) * 100.0
                                ),
                                user_impact: self.get_failure_description(component, abs_voltage, max_voltage),
                                estimated_damage: Some(DamageEstimate {
                                    failure_mode: self.get_failure_mode(component),
                                    time_to_failure: self.estimate_failure_time(abs_voltage, max_voltage),
                                    affected_components: vec![component.name().to_string()],
                                    estimated_cost: component.cost(),
                                }),
                            });
                        } else if abs_voltage > derated_max {
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
                                    "{} voltage {:.1}V exceeds {:.0}% derating limit of {:.1}V",
                                    component.name(),
                                    abs_voltage,
                                    self.derating_factor * 100.0,
                                    derated_max
                                ),
                                technical_details: format!(
                                    "Applied: {:.2}V, Derated max: {:.2}V",
                                    abs_voltage,
                                    derated_max
                                ),
                                user_impact: "Reduced component lifetime and increased failure risk".to_string(),
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
        // Only fix critical overvoltage violations
        if violation.severity != Severity::Critical {
            return None;
        }
        
        // Get the component that needs protection
        let component_id = violation.location.components.first()?;
        let component = circuit.get_component(*component_id)?;
        
        if violation.message.contains("exceeds absolute maximum") {
            // For critical overvoltage, suggest adding voltage regulation
            let target_voltage = self.get_safe_voltage(component)?;
            
            Some(CircuitModification::AddProtectionCircuit {
                protection_type: ProtectionType::OvervoltageProtection,
                target: ProtectionTarget::Component(*component_id),
                specifications: {
                    let mut specs = HashMap::new();
                    specs.insert("output_voltage".to_string(), target_voltage);
                    specs.insert("current_rating".to_string(), 0.5); // 500mA
                    specs
                },
                reason: format!(
                    "Add voltage regulator to limit {} supply to {:.1}V",
                    component.name(),
                    target_voltage
                ),
            })
        } else {
            None
        }
    }
}

impl OvervoltageRule {
    fn get_failure_description(&self, component: &Component, voltage: f64, max_voltage: f64) -> String {
        let overvoltage_ratio = voltage / max_voltage;
        
        match component.component_type() {
            "MOSFET" => {
                if overvoltage_ratio > 1.5 {
                    "Gate oxide breakdown - MOSFET will be permanently damaged".to_string()
                } else {
                    "Gate oxide stress - reduced lifetime and possible failure".to_string()
                }
            }
            "IC" | "MCU" | "CPU" | "FPGA" => {
                if overvoltage_ratio > 1.2 {
                    "Semiconductor junction breakdown - IC will be destroyed".to_string()
                } else {
                    "Accelerated aging and possible latch-up".to_string()
                }
            }
            "LED" => "LED junction breakdown - permanent failure".to_string(),
            "Capacitor" => {
                if component.component_subtype() == Some("Electrolytic") {
                    "Electrolyte breakdown - possible explosion".to_string()
                } else {
                    "Dielectric breakdown - short circuit failure".to_string()
                }
            }
            _ => "Component breakdown due to overvoltage".to_string()
        }
    }
    
    fn get_failure_mode(&self, component: &Component) -> String {
        match component.component_type() {
            "MOSFET" => "Gate oxide breakdown".to_string(),
            "IC" | "MCU" => "Junction breakdown".to_string(),
            "LED" => "PN junction failure".to_string(),
            "Capacitor" => "Dielectric breakdown".to_string(),
            _ => "Overvoltage breakdown".to_string()
        }
    }
    
    fn estimate_failure_time(&self, voltage: f64, max_voltage: f64) -> Option<Duration> {
        let overvoltage_ratio = voltage / max_voltage;
        
        if overvoltage_ratio > 2.0 {
            Some(Duration::from_millis(1)) // Immediate
        } else if overvoltage_ratio > 1.5 {
            Some(Duration::from_secs(1)) // Seconds
        } else if overvoltage_ratio > 1.2 {
            Some(Duration::from_secs(60)) // Minutes
        } else {
            None // Gradual degradation
        }
    }
}