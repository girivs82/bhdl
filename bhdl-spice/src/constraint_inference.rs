//! Constraint-based component value inference
//! 
//! This module implements BHDL's dual-role syntax where component
//! parameters can be constraints that SPICE solves for.

use std::collections::HashMap;
use anyhow::Result;

use crate::{
    Circuit, NodeId, ComponentId,
    NonlinearDcAnalysis, AnalysisResult,
    SpiceError,
};

/// Component constraint types
#[derive(Debug, Clone)]
pub enum ComponentConstraint {
    /// Current through component
    Current { target: f64, tolerance: f64 },
    /// Voltage across component
    Voltage { target: f64, tolerance: f64 },
    /// Power dissipation
    Power { max: f64 },
    /// Temperature rise
    ThermalRise { max_delta: f64 },
    /// Voltage ratio (for dividers)
    VoltageRatio { ratio: f64 },
    /// Ripple voltage
    RippleVoltage { max: f64 },
    /// ESR constraint
    ESR { max: f64 },
    /// Efficiency constraint
    Efficiency { min: f64 },
}

/// Constraint solver for component values
pub struct ConstraintSolver {
    constraints: HashMap<String, Vec<ComponentConstraint>>,
    initial_values: HashMap<String, f64>,
}

impl ConstraintSolver {
    pub fn new() -> Self {
        Self {
            constraints: HashMap::new(),
            initial_values: HashMap::new(),
        }
    }
    
    /// Add a constraint for a component
    pub fn add_constraint(&mut self, component: &str, constraint: ComponentConstraint) {
        self.constraints
            .entry(component.to_string())
            .or_insert_with(Vec::new)
            .push(constraint);
    }
    
    /// Set initial guess for component value
    pub fn set_initial_value(&mut self, component: &str, value: f64) {
        self.initial_values.insert(component.to_string(), value);
    }
    
    /// Solve for component values that satisfy constraints
    pub fn solve(&self, circuit: &mut Circuit) -> Result<HashMap<String, f64>> {
        let mut solutions = HashMap::new();
        
        // Iterate through constrained components
        for (component, constraints) in &self.constraints {
            let value = self.solve_component(circuit, component, constraints)?;
            solutions.insert(component.clone(), value);
            
            // Update circuit with solved value
            if let Some((_, branch)) = circuit.get_branch_mut(component) {
                branch.value = value;
            }
        }
        
        Ok(solutions)
    }
    
    /// Solve for a single component value
    fn solve_component(
        &self, 
        circuit: &Circuit,
        component: &str,
        constraints: &[ComponentConstraint],
    ) -> Result<f64> {
        // Get initial guess
        let mut value = self.initial_values.get(component).copied()
            .unwrap_or_else(|| self.default_initial_value(component));
        
        // Iterative refinement
        const MAX_ITERATIONS: usize = 50;
        const TOLERANCE: f64 = 0.001;
        
        for iteration in 0..MAX_ITERATIONS {
            // Create test circuit with current value
            let mut test_circuit = circuit.clone();
            if let Some((_, branch)) = test_circuit.get_branch_mut(component) {
                branch.value = value;
            }
            
            // Run analysis
            let mut analysis = NonlinearDcAnalysis::new(test_circuit.clone());
            let result = analysis.analyze()?;
            
            // Check constraints and calculate adjustment
            let adjustment = self.calculate_adjustment(
                &test_circuit,
                &result,
                component,
                constraints,
                value
            )?;
            
            // Apply adjustment
            let new_value = value + adjustment;
            
            // Check convergence
            if adjustment.abs() / value < TOLERANCE {
                return Ok(new_value);
            }
            
            value = new_value;
            
            // Ensure reasonable bounds
            value = value.max(0.001).min(1e9);
        }
        
        Err(anyhow::anyhow!(
            "Failed to converge on value for {} after {} iterations", 
            component, MAX_ITERATIONS
        ))
    }
    
    /// Calculate value adjustment based on constraint errors
    fn calculate_adjustment(
        &self,
        circuit: &Circuit,
        result: &AnalysisResult,
        component: &str,
        constraints: &[ComponentConstraint],
        current_value: f64,
    ) -> Result<f64> {
        let mut total_adjustment = 0.0;
        let mut constraint_count = 0;
        
        for constraint in constraints {
            match constraint {
                ComponentConstraint::Current { target, tolerance } => {
                    let actual = circuit.branch_current(component, result)?;
                    let error = target - actual.abs();
                    
                    if error.abs() > *tolerance {
                        // For current constraint on resistor: I = V/R
                        // To increase current, decrease resistance
                        let adjustment = -current_value * (error / target);
                        total_adjustment += adjustment;
                        constraint_count += 1;
                    }
                }
                
                ComponentConstraint::Voltage { target, tolerance } => {
                    let voltage = self.get_component_voltage(circuit, result, component)?;
                    let error = target - voltage;
                    
                    if error.abs() > *tolerance {
                        // Component-specific adjustment logic
                        let adjustment = self.voltage_adjustment(component, current_value, error);
                        total_adjustment += adjustment;
                        constraint_count += 1;
                    }
                }
                
                ComponentConstraint::Power { max } => {
                    let current = circuit.branch_current(component, result)?;
                    let voltage = self.get_component_voltage(circuit, result, component)?;
                    let power = voltage * current.abs();
                    
                    if power > *max {
                        // Increase resistance to reduce power
                        let target_r = max / (current.abs() * current.abs());
                        let adjustment = target_r - current_value;
                        total_adjustment += adjustment * 0.5; // Conservative adjustment
                        constraint_count += 1;
                    }
                }
                
                ComponentConstraint::VoltageRatio { ratio } => {
                    // For voltage dividers
                    if component.ends_with("1") || component.ends_with("2") {
                        let adjustment = self.ratio_adjustment(circuit, result, component, *ratio)?;
                        total_adjustment += adjustment;
                        constraint_count += 1;
                    }
                }
                
                _ => {
                    // Other constraints not yet implemented
                }
            }
        }
        
        if constraint_count > 0 {
            Ok(total_adjustment / constraint_count as f64)
        } else {
            Ok(0.0)
        }
    }
    
    /// Get voltage across a component
    pub fn get_component_voltage(
        &self,
        circuit: &Circuit,
        result: &AnalysisResult,
        component: &str,
    ) -> Result<f64> {
        let (edge_idx, _) = circuit.get_branch(component)
            .ok_or_else(|| anyhow::anyhow!("Component {} not found", component))?;
        
        let (n1, n2) = circuit.branch_nodes(edge_idx)
            .ok_or_else(|| anyhow::anyhow!("Invalid branch nodes"))?;
        
        let v1 = result.node_voltages.get(&n1).copied().unwrap_or(0.0);
        let v2 = result.node_voltages.get(&n2).copied().unwrap_or(0.0);
        
        Ok((v1 - v2).abs())
    }
    
    /// Calculate adjustment for voltage constraint
    fn voltage_adjustment(&self, component: &str, current_value: f64, error: f64) -> f64 {
        if component.starts_with("R") {
            // For resistors in voltage dividers
            current_value * (error / 10.0) // Conservative 10% adjustment
        } else if component.starts_with("C") {
            // For capacitors (ripple reduction)
            current_value * (error / 5.0)
        } else {
            0.0
        }
    }
    
    /// Calculate adjustment for ratio constraint
    fn ratio_adjustment(
        &self,
        circuit: &Circuit,
        result: &AnalysisResult,
        component: &str,
        target_ratio: f64,
    ) -> Result<f64> {
        // Assuming R1 and R2 form a voltage divider
        let r1_current = circuit.branch_current("R1", result)?;
        let r2_current = circuit.branch_current("R2", result)?;
        
        // In a divider, currents should be equal
        if (r1_current - r2_current).abs() > 0.001 {
            return Ok(0.0); // Not a simple divider
        }
        
        let r1_value = circuit.get_branch("R1")
            .map(|(_, b)| b.value).unwrap_or(10000.0);
        let r2_value = circuit.get_branch("R2")
            .map(|(_, b)| b.value).unwrap_or(10000.0);
        
        let current_ratio = r1_value / r2_value;
        let error = target_ratio - current_ratio;
        
        if component == "R1" {
            Ok(r2_value * error) // Adjust R1 to achieve ratio
        } else {
            Ok(-r1_value * error / (target_ratio * target_ratio)) // Adjust R2
        }
    }
    
    /// Default initial values based on component type
    fn default_initial_value(&self, component: &str) -> f64 {
        if component.starts_with("R") {
            1000.0  // 1kΩ default
        } else if component.starts_with("C") {
            1e-6    // 1µF default
        } else if component.starts_with("L") {
            1e-3    // 1mH default
        } else {
            1.0
        }
    }
}

/// Inferred component enumeration
#[derive(Debug, Clone)]
pub enum InferredComponent {
    Resistor { value: f64, power: f64 },
    Capacitor { value: f64, voltage: f64 },
    Inductor { value: f64, current: f64 },
}

/// Enhanced component inference with constraint solving
pub struct ComponentInference {
    solver: ConstraintSolver,
}

impl ComponentInference {
    pub fn new() -> Self {
        Self {
            solver: ConstraintSolver::new(),
        }
    }
    
    /// Add current constraint for a component
    pub fn add_current_constraint(&mut self, component: &str, target: f64, tolerance: f64) {
        self.solver.add_constraint(
            component,
            ComponentConstraint::Current { target, tolerance }
        );
    }
    
    /// Add voltage constraint for a node/component
    pub fn add_voltage_constraint(&mut self, node_or_component: &str, target: f64, tolerance: f64) {
        self.solver.add_constraint(
            node_or_component,
            ComponentConstraint::Voltage { target, tolerance }
        );
    }
    
    /// Add power constraint for a component
    pub fn add_power_constraint(&mut self, component: &str, max_power: f64) {
        self.solver.add_constraint(
            component,
            ComponentConstraint::Power { max: max_power }
        );
    }
    
    /// Infer component values based on constraints
    pub fn infer_component_values(&self, circuit: &mut Circuit) -> Result<HashMap<String, InferredComponent>> {
        let solutions = self.solver.solve(circuit)?;
        let mut inferred = HashMap::new();
        
        // Analyze final circuit to get power/ratings
        let mut analysis = NonlinearDcAnalysis::new(circuit.clone());
        let result = analysis.analyze()?;
        
        for (component, value) in solutions {
            let current = circuit.branch_current(&component, &result)?;
            let voltage = self.solver.get_component_voltage(circuit, &result, &component)?;
            let power = voltage * current.abs();
            
            let inferred_comp = if component.starts_with("R") {
                InferredComponent::Resistor { value, power }
            } else if component.starts_with("C") {
                InferredComponent::Capacitor { value, voltage }
            } else if component.starts_with("L") {
                InferredComponent::Inductor { value, current: current.abs() }
            } else {
                continue;
            };
            
            inferred.insert(component, inferred_comp);
        }
        
        Ok(inferred)
    }
}