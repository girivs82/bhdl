//! Component inference based on electrical constraints

use log::{info, warn};

use crate::{
    Circuit, ComponentModel,
    NonlinearDcAnalysis, AnalysisResult, SpiceError, Result,
};

/// Constraint violation detected during analysis
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    pub component: String,
    pub violation_type: ViolationType,
    pub actual_value: f64,
    pub limit_value: f64,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ViolationType {
    OverVoltage,
    UnderVoltage,
    OverCurrent,
    OverPower,
    MissingCurrentLimit,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    Warning,
    Error,
    Critical,
}

/// Inferred component to add to the circuit
#[derive(Debug, Clone)]
pub struct InferredComponent {
    pub name: String,
    pub component_type: String,
    pub value: f64,
    pub node1: String,
    pub node2: String,
    pub reason: String,
    pub confidence: f64,
}

/// Component inference engine
pub struct ComponentInference {
    circuit: Circuit,
    models: std::collections::HashMap<String, ComponentModel>,
    violations: Vec<ConstraintViolation>,
    inferred_components: Vec<InferredComponent>,
    analysis_result: Option<AnalysisResult>,
}

impl ComponentInference {
    /// Create a new inference engine
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: std::collections::HashMap::new(),
            violations: Vec::new(),
            inferred_components: Vec::new(),
            analysis_result: None,
        }
    }
    
    /// Add component model for analysis
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Run inference to identify missing components
    pub fn infer(&mut self) -> Result<Vec<InferredComponent>> {
        info!("Starting component inference");
        
        // Run initial DC analysis using nonlinear solver
        let mut analysis = NonlinearDcAnalysis::new(self.circuit.clone());
        for (name, model) in &self.models {
            analysis.add_model(name.clone(), model.clone());
        }
        let result = analysis.analyze()?;
        
        // Store the result for later use
        self.analysis_result = Some(result.clone());
        
        // Check for constraint violations
        self.check_constraints(&result)?;
        
        // Infer components to fix violations
        self.infer_fixes()?;
        
        Ok(self.inferred_components.clone())
    }
    
    /// Check all component constraints
    fn check_constraints(&mut self, result: &AnalysisResult) -> Result<()> {
        self.violations.clear();
        
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let v1 = result.node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = result.node_voltages.get(&n2).copied().unwrap_or(0.0);
                let voltage = (v1 - v2).abs();
                let current = result.branch_currents.get(&edge_idx).copied().unwrap_or(0.0).abs();
                let power = voltage * current;
                
                // Get component model and check limits
                if let Some(model) = self.models.get(&branch.name) {
                    let limits = model.limits();
                    
                    // Check voltage limits
                    if let Some(max_v) = limits.max_voltage {
                        if voltage > max_v {
                            self.violations.push(ConstraintViolation {
                                component: branch.name.clone(),
                                violation_type: ViolationType::OverVoltage,
                                actual_value: voltage,
                                limit_value: max_v,
                                severity: Severity::Error,
                            });
                        }
                    }
                    
                    // Check current limits
                    if let Some(max_i) = limits.max_current {
                        if current > max_i {
                            self.violations.push(ConstraintViolation {
                                component: branch.name.clone(),
                                violation_type: ViolationType::OverCurrent,
                                actual_value: current,
                                limit_value: max_i,
                                severity: Severity::Critical,
                            });
                        }
                    }
                    
                    // Check power limits
                    if let Some(max_p) = limits.max_power {
                        if power > max_p {
                            self.violations.push(ConstraintViolation {
                                component: branch.name.clone(),
                                violation_type: ViolationType::OverPower,
                                actual_value: power,
                                limit_value: max_p,
                                severity: Severity::Error,
                            });
                        }
                    }
                    
                    // Special check for LEDs without current limiting
                    if let ComponentModel::LED { forward_current, .. } = model {
                        if current > forward_current * 2.0 {  // 2x nominal is definitely too high
                            self.violations.push(ConstraintViolation {
                                component: branch.name.clone(),
                                violation_type: ViolationType::MissingCurrentLimit,
                                actual_value: current,
                                limit_value: *forward_current,
                                severity: Severity::Critical,
                            });
                        }
                    }
                }
            }
        }
        
        info!("Found {} constraint violations", self.violations.len());
        for violation in &self.violations {
            warn!("Violation: {:?}", violation);
        }
        
        Ok(())
    }
    
    /// Infer components to fix violations
    fn infer_fixes(&mut self) -> Result<()> {
        self.inferred_components.clear();
        
        for violation in &self.violations {
            match violation.violation_type {
                ViolationType::OverCurrent | ViolationType::MissingCurrentLimit => {
                    // Infer current limiting resistor
                    if let Some(resistor) = self.infer_current_limiter(violation)? {
                        self.inferred_components.push(resistor);
                    }
                }
                ViolationType::OverVoltage => {
                    // Might need voltage divider or regulator
                    if let Some(component) = self.infer_voltage_limiter(violation)? {
                        self.inferred_components.push(component);
                    }
                }
                ViolationType::OverPower => {
                    // Might need current limiting or better power rating
                    warn!("Power violation in {} - consider higher power rating", violation.component);
                }
                _ => {}
            }
        }
        
        info!("Inferred {} components", self.inferred_components.len());
        Ok(())
    }
    
    /// Infer current limiting resistor
    fn infer_current_limiter(&self, violation: &ConstraintViolation) -> Result<Option<InferredComponent>> {
        // Find the branch with violation
        let branch = self.circuit.get_branch(&violation.component)
            .ok_or_else(|| SpiceError::ComponentNotFound(violation.component.clone()))?;
        
        let (edge_idx, _) = branch;
        let (n1, n2) = self.circuit.branch_nodes(edge_idx)
            .ok_or_else(|| SpiceError::Other(anyhow::anyhow!("Invalid branch")))?;
        
        // Get node names
        let node1_name = self.circuit.nodes()
            .find(|(idx, _)| *idx == n1)
            .map(|(_, node)| node.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        
        let _node2_name = self.circuit.nodes()
            .find(|(idx, _)| *idx == n2)
            .map(|(_, node)| node.name.clone())
            .unwrap_or_else(|| "unknown".to_string());
        
        // Calculate required resistance
        let target_current = violation.limit_value * 0.8;  // 80% of limit for safety
        let actual_current = violation.actual_value;
        
        if actual_current <= 0.0 {
            return Ok(None);
        }
        
        // For LED current limiting, we need to know the supply voltage
        if let Some(ComponentModel::LED { forward_voltage, .. }) = self.models.get(&violation.component) {
            // Get voltages from analysis result
            let v1 = self.analysis_result.as_ref()
                .and_then(|r| r.node_voltages.get(&n1).copied())
                .unwrap_or(0.0);
            let v2 = self.analysis_result.as_ref()
                .and_then(|r| r.node_voltages.get(&n2).copied())
                .unwrap_or(0.0);
            
            let supply_voltage = v1.max(v2);
            let required_resistance = (supply_voltage - forward_voltage) / target_current;
            
            // Find nearest standard value
            let standard_value = find_standard_resistor_value(required_resistance);
            
            Ok(Some(InferredComponent {
                name: format!("R_{}", violation.component),
                component_type: "Resistor".to_string(),
                value: standard_value,
                node1: node1_name.clone(),
                node2: format!("{}_limited", node1_name),  // Insert in series
                reason: format!(
                    "Current limiting for {}: I={:.3}A exceeds limit {:.3}A. R=({:.1}V-{:.1}V)/{:.3}A={:.0}Ω",
                    violation.component, actual_current, violation.limit_value,
                    supply_voltage, forward_voltage, target_current, required_resistance
                ),
                confidence: 0.95,
            }))
        } else {
            // Generic current limiting
            let voltage_drop = 1.0;  // Assume 1V drop for now
            let required_resistance = voltage_drop / (actual_current - target_current);
            let standard_value = find_standard_resistor_value(required_resistance);
            
            Ok(Some(InferredComponent {
                name: format!("R_{}", violation.component),
                component_type: "Resistor".to_string(),
                value: standard_value,
                node1: node1_name.clone(),
                node2: format!("{}_limited", node1_name),
                reason: format!(
                    "Current limiting for {}: I={:.3}A exceeds limit {:.3}A",
                    violation.component, actual_current, violation.limit_value
                ),
                confidence: 0.85,
            }))
        }
    }
    
    /// Infer voltage limiting component
    fn infer_voltage_limiter(&self, violation: &ConstraintViolation) -> Result<Option<InferredComponent>> {
        // For now, just warn - could infer voltage divider or regulator
        warn!("Voltage violation in {} - consider voltage regulation", violation.component);
        Ok(None)
    }
}

/// Find nearest E12 standard resistor value
fn find_standard_resistor_value(value: f64) -> f64 {
    const E12_VALUES: [f64; 12] = [1.0, 1.2, 1.5, 1.8, 2.2, 2.7, 3.3, 3.9, 4.7, 5.6, 6.8, 8.2];
    
    if value <= 0.0 {
        return 10.0;  // Default minimum
    }
    
    // Find decade multiplier
    let decade = 10f64.powf((value.log10()).floor());
    let normalized = value / decade;
    
    // Find closest E12 value
    let mut closest = E12_VALUES[0];
    let mut min_diff = (normalized - E12_VALUES[0]).abs();
    
    for &e12_val in &E12_VALUES[1..] {
        let diff = (normalized - e12_val).abs();
        if diff < min_diff {
            min_diff = diff;
            closest = e12_val;
        }
    }
    
    closest * decade
}