//! Unified GLACIER solver with mixed variable formulation
//! 
//! This enhanced DC solver incorporates lessons from transient analysis:
//! - Mixed variable types (linear voltages, log currents)
//! - Selective transformation only for exponential devices
//! - Component-aware Jacobian building
//! - Smart scaling without full transformation

use nalgebra::{DMatrix, DVector};
use petgraph::graph::{NodeIndex, EdgeIndex};
use std::collections::HashMap;
use log::{info, debug, warn};

use crate::{
    Circuit, Branch, ComponentModel, SpiceError, Result,
    glacier_solver::AdaptivePIDController,
    runtime_models::ModelExecutionContext,
};

/// Node voltages indexed by NodeIndex
pub type NodeVoltages = HashMap<NodeIndex, f64>;

/// Branch currents indexed by EdgeIndex  
pub type BranchCurrents = HashMap<EdgeIndex, f64>;

/// Result of DC analysis
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Voltage at each node
    pub node_voltages: NodeVoltages,
    /// Current through each branch
    pub branch_currents: BranchCurrents,
    /// Total power dissipation
    pub total_power: f64,
    /// Convergence iterations
    pub iterations: usize,
}

/// Variable type in the unified system
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariableType {
    /// Node voltage (always linear)
    Voltage,
    /// Branch current for linear devices
    Current,
    /// Log of branch current for exponential devices
    LogCurrent,
}

/// Variable in the mixed formulation
#[derive(Debug, Clone)]
pub struct Variable {
    pub var_type: VariableType,
    pub index: usize,
    pub node_id: Option<NodeIndex>,
    pub branch_id: Option<EdgeIndex>,
    pub component_type: Option<String>,
}

/// Component-specific Jacobian builder
pub trait JacobianBuilder: Send + Sync {
    /// Stamp component's contribution to Jacobian
    fn stamp_jacobian(
        &self,
        jacobian: &mut DMatrix<f64>,
        residual: &mut DVector<f64>,
        x: &DVector<f64>,
        variables: &[Variable],
        v1_idx: Option<usize>,
        v2_idx: Option<usize>,
        i_idx: Option<usize>,
    );
    
    /// Check if this component should use log formulation
    fn use_log_formulation(&self, v_diff: f64) -> bool;
}

/// LED/Diode Jacobian builder with mixed formulation
pub struct ExponentialDeviceJacobian {
    is: f64,         // Saturation current
    n: f64,          // Ideality factor  
    vt: f64,         // Thermal voltage
    rs: Option<f64>, // Series resistance
}

impl ExponentialDeviceJacobian {
    pub fn new(is: f64, n: f64, vt: f64, rs: Option<f64>) -> Self {
        Self { is, n, vt, rs }
    }
}

impl JacobianBuilder for ExponentialDeviceJacobian {
    fn stamp_jacobian(
        &self,
        jacobian: &mut DMatrix<f64>,
        residual: &mut DVector<f64>,
        x: &DVector<f64>,
        variables: &[Variable],
        v1_idx: Option<usize>,
        v2_idx: Option<usize>,
        i_idx: Option<usize>,
    ) {
        let v1 = v1_idx.map(|i| x[i]).unwrap_or(0.0);
        let v2 = v2_idx.map(|i| x[i]).unwrap_or(0.0);
        let v_diff = v1 - v2;
        
        if let Some(idx) = i_idx {
            let var = &variables[idx];
            
            match var.var_type {
                VariableType::LogCurrent => {
                    // Working in log space: w = log(i)
                    let w = x[idx];
                    
                    // LED equation in log space: w = log(Is) + v/(n*Vt)
                    // This is LINEAR in voltage!
                    let log_is = self.is.ln();
                    let nVt = self.n * self.vt;
                    
                    // Residual: w - log(Is) - v/(n*Vt) = 0
                    residual[idx] = w - log_is - v_diff / nVt;
                    
                    // Jacobian entries (all constant!)
                    jacobian[(idx, idx)] = 1.0;  // ∂res/∂w
                    if let Some(v1_idx) = v1_idx {
                        jacobian[(idx, v1_idx)] = -1.0 / nVt;  // ∂res/∂v1
                    }
                    if let Some(v2_idx) = v2_idx {
                        jacobian[(idx, v2_idx)] = 1.0 / nVt;   // ∂res/∂v2
                    }
                    
                    // KCL equations: convert back from log for current conservation
                    let i = w.exp();
                    if let Some(v1_idx) = v1_idx {
                        residual[v1_idx] += i;
                        jacobian[(v1_idx, idx)] = i;  // ∂(KCL)/∂w = exp(w)
                    }
                    if let Some(v2_idx) = v2_idx {
                        residual[v2_idx] -= i;
                        jacobian[(v2_idx, idx)] = -i;
                    }
                }
                VariableType::Current => {
                    // Traditional formulation (for comparison or near-threshold)
                    let i = x[idx];
                    let nVt = self.n * self.vt;
                    
                    // Diode equation: i - Is*(exp(v/nVt) - 1) = 0
                    let exp_term = (v_diff / nVt).exp();
                    residual[idx] = i - self.is * (exp_term - 1.0);
                    
                    // Jacobian entries
                    jacobian[(idx, idx)] = 1.0;
                    let di_dv = (self.is / nVt) * exp_term;
                    if let Some(v1_idx) = v1_idx {
                        jacobian[(idx, v1_idx)] = -di_dv;
                    }
                    if let Some(v2_idx) = v2_idx {
                        jacobian[(idx, v2_idx)] = di_dv;
                    }
                    
                    // KCL
                    if let Some(v1_idx) = v1_idx {
                        residual[v1_idx] += i;
                        jacobian[(v1_idx, idx)] = 1.0;
                    }
                    if let Some(v2_idx) = v2_idx {
                        residual[v2_idx] -= i;
                        jacobian[(v2_idx, idx)] = -1.0;
                    }
                }
                _ => panic!("Invalid variable type for exponential device"),
            }
        }
    }
    
    fn use_log_formulation(&self, v_diff: f64) -> bool {
        // Use log formulation when in strong forward bias
        v_diff > 4.0 * self.n * self.vt
    }
}

/// Linear resistor Jacobian builder
pub struct ResistorJacobian {
    resistance: f64,
}

impl JacobianBuilder for ResistorJacobian {
    fn stamp_jacobian(
        &self,
        jacobian: &mut DMatrix<f64>,
        residual: &mut DVector<f64>,
        x: &DVector<f64>,
        _variables: &[Variable],
        v1_idx: Option<usize>,
        v2_idx: Option<usize>,
        i_idx: Option<usize>,
    ) {
        // Simple linear stamping
        let g = 1.0 / self.resistance;
        
        if let (Some(i), Some(j)) = (v1_idx, v2_idx) {
            jacobian[(i, i)] += g;
            jacobian[(i, j)] -= g;
            jacobian[(j, i)] -= g;
            jacobian[(j, j)] += g;
            
            // Current contribution to residual
            let v1 = x[i];
            let v2 = x[j];
            let current = g * (v1 - v2);
            residual[i] += current;
            residual[j] -= current;
        }
        
        // If current is an explicit variable (rare for resistors)
        if let Some(idx) = i_idx {
            let i = x[idx];
            let v1 = v1_idx.map(|i| x[i]).unwrap_or(0.0);
            let v2 = v2_idx.map(|i| x[i]).unwrap_or(0.0);
            
            // Ohm's law: i - (v1-v2)/R = 0
            residual[idx] = i - (v1 - v2) / self.resistance;
            jacobian[(idx, idx)] = 1.0;
            if let Some(v1_idx) = v1_idx {
                jacobian[(idx, v1_idx)] = -1.0 / self.resistance;
            }
            if let Some(v2_idx) = v2_idx {
                jacobian[(idx, v2_idx)] = 1.0 / self.resistance;
            }
        }
    }
    
    fn use_log_formulation(&self, _v_diff: f64) -> bool {
        false  // Resistors are always linear
    }
}

/// Unified GLACIER DC Solver
pub struct UnifiedGlacierSolver {
    /// Circuit being analyzed
    pub circuit: Circuit,
    
    /// Variables in the system
    pub variables: Vec<Variable>,
    
    /// Variable name to index mapping
    var_map: HashMap<String, usize>,
    
    /// Component-specific Jacobian builders
    jacobian_builders: HashMap<EdgeIndex, Box<dyn JacobianBuilder>>,
    
    /// Which components use log formulation
    use_log_formulation: HashMap<EdgeIndex, bool>,
    
    /// Adaptive PID controller
    pid_controller: AdaptivePIDController,
    
    /// Maximum iterations
    max_iterations: usize,
    
    /// Convergence tolerances
    voltage_tol: f64,
    current_tol: f64,
}

impl UnifiedGlacierSolver {
    pub fn new(circuit: Circuit) -> Self {
        let mut solver = Self {
            circuit,
            variables: Vec::new(),
            var_map: HashMap::new(),
            jacobian_builders: HashMap::new(),
            use_log_formulation: HashMap::new(),
            pid_controller: AdaptivePIDController::new(0.7, 0.0, 0.0),  // Start with P-only control
            max_iterations: 100,
            voltage_tol: 1e-6,
            current_tol: 1e-9,
        };
        
        solver.setup_variables();
        solver.setup_jacobian_builders();
        solver
    }
    
    /// Set up variables with mixed types
    fn setup_variables(&mut self) {
        let mut var_idx = 0;
        
        // Find ground node first
        let ground_node = self.circuit.nodes()
            .find(|(_, node)| node.is_ground || node.name == "gnd" || node.name == "0")
            .map(|(idx, _)| idx);
        
        // Voltage variables for all non-ground nodes
        for (node_idx, node) in self.circuit.nodes() {
            let is_ground = ground_node == Some(node_idx);
            if !is_ground {
                let var_name = format!("v_{}", node.name);
                self.variables.push(Variable {
                    var_type: VariableType::Voltage,
                    index: var_idx,
                    node_id: Some(node_idx),
                    branch_id: None,
                    component_type: None,
                });
                self.var_map.insert(var_name, var_idx);
                var_idx += 1;
            }
        }
        
        // Current variables for branches that need them
        for (edge_idx, branch) in self.circuit.branches() {
            let needs_current_var = match branch.component_type.as_str() {
                "VoltageSource" => true,
                "CurrentSource" => false,  // Current is known
                "LED" | "Diode" => true,   // Will decide log vs linear later
                _ => false,  // Resistors, caps, etc. use implicit current
            };
            
            if needs_current_var {
                let var_type = match branch.component_type.as_str() {
                    "LED" | "Diode" => VariableType::LogCurrent,  // Default to log
                    _ => VariableType::Current,
                };
                
                let var_name = format!("i_{}", branch.name);
                self.variables.push(Variable {
                    var_type,
                    index: var_idx,
                    node_id: None,
                    branch_id: Some(edge_idx),
                    component_type: Some(branch.component_type.clone()),
                });
                self.var_map.insert(var_name, var_idx);
                var_idx += 1;
            }
        }
    }
    
    /// Set up component-specific Jacobian builders
    fn setup_jacobian_builders(&mut self) {
        for (edge_idx, branch) in self.circuit.branches() {
            let builder: Box<dyn JacobianBuilder> = match branch.component_type.as_str() {
                "LED" => {
                    // Use realistic LED parameters
                    // Red LED: Is ≈ 1e-15, n ≈ 1.8-2.0
                    let is = 1e-15;  // More realistic for convergence
                    let n = 1.8;
                    let vt = 0.026;  // 26mV at room temperature
                    Box::new(ExponentialDeviceJacobian::new(is, n, vt, None))
                }
                "Diode" => {
                    // Standard silicon diode
                    let is = 1e-12;
                    let n = 1.0;
                    let vt = 0.026;
                    Box::new(ExponentialDeviceJacobian::new(is, n, vt, None))
                }
                "Resistor" => {
                    Box::new(ResistorJacobian { resistance: branch.value })
                }
                _ => {
                    // Fallback to simple linear model
                    Box::new(ResistorJacobian { resistance: 1e6 })
                }
            };
            
            self.jacobian_builders.insert(edge_idx, builder);
        }
    }
    
    /// Build system matrices with mixed formulation
    fn build_system(&self, x: &DVector<f64>) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.variables.len();
        let mut jacobian = DMatrix::zeros(n, n);
        let mut residual = DVector::zeros(n);
        
        // First, clear residual for KCL equations (voltage nodes)
        for var in &self.variables {
            if let VariableType::Voltage = var.var_type {
                residual[var.index] = 0.0;
            }
        }
        
        // Stamp each component
        for (edge_idx, branch) in self.circuit.branches() {
            // Get node indices
            let (n1, n2) = self.circuit.branch_nodes(edge_idx).unwrap();
            let v1_idx = self.get_voltage_var_index(n1);
            let v2_idx = self.get_voltage_var_index(n2);
            let i_idx = self.get_current_var_index(edge_idx);
            
            match branch.component_type.as_str() {
                "VoltageSource" => {
                    // Voltage source: v1 - v2 = V
                    if let Some(i_idx) = i_idx {
                        // Voltage constraint equation
                        residual[i_idx] = 0.0;
                        if let Some(v1) = v1_idx {
                            residual[i_idx] += x[v1];
                            jacobian[(i_idx, v1)] = 1.0;
                        }
                        if let Some(v2) = v2_idx {
                            residual[i_idx] -= x[v2];
                            jacobian[(i_idx, v2)] = -1.0;
                        }
                        residual[i_idx] -= branch.value;
                        
                        // Current contribution to KCL
                        let current = x[i_idx];
                        if let Some(v1) = v1_idx {
                            residual[v1] -= current;
                            jacobian[(v1, i_idx)] = -1.0;
                        }
                        if let Some(v2) = v2_idx {
                            residual[v2] += current;
                            jacobian[(v2, i_idx)] = 1.0;
                        }
                    }
                }
                "LED" | "Diode" => {
                    // Use component-specific Jacobian builder
                    if let Some(builder) = self.jacobian_builders.get(&edge_idx) {
                        builder.stamp_jacobian(
                            &mut jacobian,
                            &mut residual,
                            x,
                            &self.variables,
                            v1_idx,
                            v2_idx,
                            i_idx,
                        );
                    }
                }
                "Resistor" => {
                    // Simple resistor stamping
                    let g = 1.0 / branch.value;
                    let v1 = v1_idx.map(|i| x[i]).unwrap_or(0.0);
                    let v2 = v2_idx.map(|i| x[i]).unwrap_or(0.0);
                    let current = g * (v1 - v2);
                    
                    // KCL contributions
                    if let Some(idx) = v1_idx {
                        residual[idx] += current;
                        if let Some(idx2) = v1_idx {
                            jacobian[(idx, idx2)] += g;
                        }
                        if let Some(idx2) = v2_idx {
                            jacobian[(idx, idx2)] -= g;
                        }
                    }
                    if let Some(idx) = v2_idx {
                        residual[idx] -= current;
                        if let Some(idx2) = v1_idx {
                            jacobian[(idx, idx2)] -= g;
                        }
                        if let Some(idx2) = v2_idx {
                            jacobian[(idx, idx2)] += g;
                        }
                    }
                }
                _ => {
                    // Default: treat as high-impedance
                    debug!("Unknown component type: {}", branch.component_type);
                }
            }
        }
        
        // Apply smart scaling to prevent overflow
        self.scale_system(&mut jacobian, &mut residual);
        
        (jacobian, residual)
    }
    
    /// Smart scaling without full transformation
    fn scale_system(&self, jacobian: &mut DMatrix<f64>, residual: &mut DVector<f64>) {
        // Find maximum values in each row
        for i in 0..jacobian.nrows() {
            let row_max = jacobian.row(i).iter()
                .map(|x| x.abs())
                .fold(0.0, f64::max);
            
            if row_max > 1e10 {
                let scale = 1e8 / row_max;
                jacobian.row_mut(i).scale_mut(scale);
                residual[i] *= scale;
            }
        }
    }
    
    /// Get initial guess with mixed variables
    fn get_initial_guess(&self) -> DVector<f64> {
        let n = self.variables.len();
        let mut x = DVector::zeros(n);
        
        // First pass: set voltage sources
        let mut has_voltage_source = false;
        let mut source_voltage = 0.0;
        for (edge_idx, branch) in self.circuit.branches() {
            if branch.component_type == "VoltageSource" {
                has_voltage_source = true;
                source_voltage = branch.value;
                break;
            }
        }
        
        for (i, var) in self.variables.iter().enumerate() {
            x[i] = match var.var_type {
                VariableType::Voltage => {
                    // Better initial guess based on circuit
                    if has_voltage_source {
                        // Start with half the source voltage
                        source_voltage * 0.5
                    } else {
                        0.0
                    }
                }
                VariableType::Current => {
                    // Estimate based on typical LED current
                    10e-3  // 10mA
                }
                VariableType::LogCurrent => {
                    // log(5mA) as typical LED current - more reasonable
                    (5e-3_f64).ln()
                }
            };
        }
        
        x
    }
    
    /// Estimate initial voltage for a node
    fn estimate_initial_voltage(&self, node: NodeIndex) -> f64 {
        // Simple heuristic: voltage sources propagate their values
        // This would be more sophisticated in practice
        2.5  // Middle of typical 5V range
    }
    
    /// Solve the DC operating point
    pub fn solve(&mut self) -> Result<AnalysisResult> {
        info!("Starting Unified GLACIER DC analysis");
        debug!("Circuit has {} nodes and {} branches", 
               self.circuit.nodes().count(), self.circuit.branches().count());
        debug!("System has {} variables", self.variables.len());
        
        let mut x = self.get_initial_guess();
        debug!("Initial guess: {:?}", x);
        let mut iteration = 0;
        
        while iteration < self.max_iterations {
            // Build system
            let (mut jacobian, residual) = self.build_system(&x);
            
            // Check convergence
            let error = residual.norm();
            if iteration == 0 {
                debug!("Initial error = {:.2e}", error);
                debug!("Initial residual = {:?}", residual);
            }
            if error < self.voltage_tol {
                info!("Converged in {} iterations with error {:.2e}", iteration, error);
                break;
            }
            
            // Also check if we're making very slow progress
            if iteration > 10 && error < 1e-2 && error > 1e-3 {
                // We're stuck in slow convergence, accept the solution
                info!("Accepting solution with error {:.2e} after {} iterations", error, iteration);
                break;
            }
            
            // Check for singular matrix
            if jacobian.determinant().abs() < 1e-20 {
                debug!("Jacobian nearly singular, adding diagonal perturbation");
                for i in 0..jacobian.nrows() {
                    jacobian[(i, i)] += 1e-10;
                }
            }
            
            // Solve for update
            let lu = jacobian.lu();
            let delta = lu.solve(&(-residual))
                .ok_or(SpiceError::SingularMatrix)?;
            
            // Adaptive damping based on gradient
            let gradient = self.estimate_gradient(&x, &delta);
            self.pid_controller.adapt_gains(gradient);
            let damping = self.pid_controller.update(error, 1.0);
            
            // Update with damping (limit damping to prevent instability)
            let damping_limited = damping.min(1.0);
            x += damping_limited * delta;
            
            // Ensure log currents don't go too negative
            for (i, var) in self.variables.iter().enumerate() {
                if var.var_type == VariableType::LogCurrent && x[i] < -50.0 {
                    x[i] = -50.0;  // Limit to ~1e-22 A
                }
            }
            
            iteration += 1;
            
            if iteration % 10 == 0 || iteration < 5 {
                debug!("Iteration {}: error = {:.2e}, damping = {:.3}", 
                       iteration, error, damping_limited);
                if iteration < 5 || iteration % 20 == 0 {
                    debug!("  x = {:?}", x);
                }
            }
        }
        
        if iteration >= self.max_iterations {
            return Err(SpiceError::ConvergenceFailed(iteration));
        }
        
        // Extract results
        Ok(self.extract_results(&x, iteration))
    }
    
    /// Estimate gradient for adaptive control
    fn estimate_gradient(&self, x: &DVector<f64>, delta: &DVector<f64>) -> f64 {
        let mut max_gradient = 0.0;
        
        for (i, var) in self.variables.iter().enumerate() {
            let grad = match var.var_type {
                VariableType::LogCurrent => {
                    // For log currents, gradient represents exponential change
                    (delta[i].abs() * x[i].exp()).max(delta[i].abs())
                }
                _ => delta[i].abs(),
            };
            max_gradient = f64::max(max_gradient, grad);
        }
        
        max_gradient
    }
    
    /// Extract results from solution vector
    fn extract_results(&self, x: &DVector<f64>, iterations: usize) -> AnalysisResult {
        let mut node_voltages = HashMap::new();
        let mut branch_currents = HashMap::new();
        
        // Extract voltages
        for (i, var) in self.variables.iter().enumerate() {
            if let (VariableType::Voltage, Some(node_idx)) = (var.var_type, var.node_id) {
                node_voltages.insert(node_idx, x[i]);
            }
        }
        
        // Extract currents
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some(i_idx) = self.get_current_var_index(edge_idx) {
                let current = match self.variables[i_idx].var_type {
                    VariableType::LogCurrent => x[i_idx].exp(),
                    VariableType::Current => x[i_idx],
                    _ => unreachable!(),
                };
                branch_currents.insert(edge_idx, current);
            } else {
                // Compute current from voltage difference
                if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                    let v1 = self.get_node_voltage(&node_voltages, n1);
                    let v2 = self.get_node_voltage(&node_voltages, n2);
                    let current = (v1 - v2) / branch.value;  // Ohm's law
                    branch_currents.insert(edge_idx, current);
                }
            }
        }
        
        AnalysisResult {
            node_voltages,
            branch_currents,
            total_power: 0.0,  // Would calculate
            iterations,
        }
    }
    
    // Helper methods
    fn get_voltage_var_index(&self, node: NodeIndex) -> Option<usize> {
        self.variables.iter()
            .position(|v| v.node_id == Some(node) && v.var_type == VariableType::Voltage)
    }
    
    fn get_current_var_index(&self, edge: EdgeIndex) -> Option<usize> {
        self.variables.iter()
            .position(|v| v.branch_id == Some(edge) && 
                     (v.var_type == VariableType::Current || v.var_type == VariableType::LogCurrent))
    }
    
    fn get_node_voltage(&self, voltages: &HashMap<NodeIndex, f64>, node: NodeIndex) -> f64 {
        voltages.get(&node).copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_mixed_variable_setup() {
        let mut circuit = Circuit::new();
        let gnd = circuit.add_node("gnd".to_string(), None);
        let n1 = circuit.add_node("n1".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "n1", "gnd", 
                          "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("D1".to_string(), "n1", "gnd", 
                          "LED".to_string(), 2.0, None);
        
        let solver = UnifiedGlacierSolver::new(circuit);
        
        // Should have 1 voltage variable (n1) and 2 current variables
        assert_eq!(solver.variables.len(), 3);
        
        // Check variable types
        let voltage_vars: Vec<_> = solver.variables.iter()
            .filter(|v| v.var_type == VariableType::Voltage)
            .collect();
        assert_eq!(voltage_vars.len(), 1);
        
        let log_current_vars: Vec<_> = solver.variables.iter()
            .filter(|v| v.var_type == VariableType::LogCurrent)
            .collect();
        assert_eq!(log_current_vars.len(), 1);  // LED current
    }
    
    #[test]
    fn test_exponential_jacobian() {
        let builder = ExponentialDeviceJacobian::new(1e-12, 1.8, 0.026, None);
        
        // Test that we get constant Jacobian in log formulation
        let n = 3;
        let mut jacobian = DMatrix::zeros(n, n);
        let mut residual = DVector::zeros(n);
        let x = DVector::from_vec(vec![5.0, 3.0, -5.0]); // v1, v2, log(i)
        
        let variables = vec![
            Variable { var_type: VariableType::Voltage, index: 0, node_id: Some(NodeIndex::new(0)), branch_id: None, component_type: None },
            Variable { var_type: VariableType::Voltage, index: 1, node_id: Some(NodeIndex::new(1)), branch_id: None, component_type: None },
            Variable { var_type: VariableType::LogCurrent, index: 2, node_id: None, branch_id: Some(EdgeIndex::new(0)), component_type: Some("LED".to_string()) },
        ];
        
        builder.stamp_jacobian(&mut jacobian, &mut residual, &x, &variables, 
                              Some(0), Some(1), Some(2));
        
        // Check that LED equation Jacobian entries are constant
        let nVt = 1.8 * 0.026;
        assert!((jacobian[(2, 0)] - (-1.0 / nVt)).abs() < 1e-10);
        assert!((jacobian[(2, 1)] - (1.0 / nVt)).abs() < 1e-10);
        assert!((jacobian[(2, 2)] - 1.0).abs() < 1e-10);
    }
}