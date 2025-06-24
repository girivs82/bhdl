//! Adaptive Logarithmic Gradient Circuit Solver
//! 
//! Universal solver using Two-Phase Adaptive PID control with logarithmic gradient tracking.
//! Works for both linear and nonlinear circuits by adapting convergence strategy based on
//! circuit sensitivity and device characteristics.

use nalgebra::{DMatrix, DVector};
use petgraph::graph::{NodeIndex, EdgeIndex};
use std::collections::HashMap;
use log::{info, debug};

use crate::{
    Circuit, ComponentModel, SpiceError, Result,
    NodeVoltages, BranchCurrents, AnalysisResult,
    runtime_models::{RuntimeModelEngine, ModelExecutionContext},
};

/// Two-Phase Adaptive PID Controller for convergence control
#[derive(Debug, Clone)]
pub struct AdaptivePIDController {
    // Base parameters (Phase 1: Aggressive)
    base_kp: f64,
    base_ki: f64, 
    base_kd: f64,
    
    // Adaptive parameters (adjusted based on log gradient)
    kp: f64,
    ki: f64,
    kd: f64,
    
    // PID state
    integral: f64,
    last_error: f64,
}

impl AdaptivePIDController {
    pub fn new(kp: f64, ki: f64, kd: f64) -> Self {
        Self {
            base_kp: kp,
            base_ki: ki,
            base_kd: kd,
            kp,
            ki,
            kd,
            integral: 0.0,
            last_error: 0.0,
        }
    }
    
    /// Compute PID control signal with adaptive gains
    pub fn compute(&mut self, error: f64, log_gradient: f64) -> f64 {
        // Adapt gains based on logarithmic gradient (device sensitivity)
        if log_gradient < 2.0 {
            // Low sensitivity (high Vt case) - more aggressive
            self.kp = self.base_kp * 2.0;
            self.ki = self.base_ki * 3.0;
            self.kd = self.base_kd * 0.5;
        } else if log_gradient > 10.0 {
            // High sensitivity - more conservative
            self.kp = self.base_kp * 0.5;
            self.ki = self.base_ki * 0.3;
            self.kd = self.base_kd * 2.0;
        } else {
            // Normal sensitivity - standard gains
            self.kp = self.base_kp;
            self.ki = self.base_ki;
            self.kd = self.base_kd;
        }
        
        // PID computation
        self.integral += error;
        let derivative = error - self.last_error;
        let output = self.kp * error + self.ki * self.integral + self.kd * derivative;
        self.last_error = error;
        
        output
    }
    
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.last_error = 0.0;
    }
}

/// Unified adaptive circuit solver for linear and nonlinear analysis
pub struct AdaptiveCircuitSolver {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    
    // Convergence parameters
    max_iterations: usize,
    tolerance: f64,
    
    // Adaptive control
    phase1_pid: AdaptivePIDController,
    phase2_pid: AdaptivePIDController,
    current_phase: u8,
    
    // Performance tracking
    phase_switch_threshold: f64,
    convergence_history: Vec<f64>,
    
    // Runtime model engine for stdlib-driven models
    model_engine: RuntimeModelEngine,
}

impl AdaptiveCircuitSolver {
    /// Create new adaptive solver
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
            max_iterations: 1000,
            tolerance: 1e-9,
            
            // Phase 1: Aggressive convergence for rapid initial progress
            phase1_pid: AdaptivePIDController::new(10.0, 2.0, 0.1),
            
            // Phase 2: Precision convergence for final accuracy
            phase2_pid: AdaptivePIDController::new(1.0, 0.2, 0.02),
            
            current_phase: 1,
            phase_switch_threshold: 0.9,
            convergence_history: Vec::new(),
            
            // Initialize runtime model engine
            model_engine: RuntimeModelEngine::new()
                .expect("Failed to initialize runtime model engine"),
        }
    }
    
    /// Add component model
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Set convergence parameters
    pub fn set_convergence(&mut self, max_iterations: usize, tolerance: f64) {
        self.max_iterations = max_iterations;
        self.tolerance = tolerance;
    }
    
    /// Configure PID parameters for specific circuit characteristics
    pub fn configure_for_circuit_type(&mut self, circuit_type: CircuitType) {
        match circuit_type {
            CircuitType::Linear => {
                // Linear circuits: moderate gains, fast convergence expected
                self.phase1_pid = AdaptivePIDController::new(5.0, 1.0, 0.05);
                self.phase2_pid = AdaptivePIDController::new(1.0, 0.1, 0.01);
            },
            CircuitType::Nonlinear => {
                // Nonlinear circuits: higher gains, more careful convergence
                self.phase1_pid = AdaptivePIDController::new(10.0, 2.0, 0.1);
                self.phase2_pid = AdaptivePIDController::new(1.0, 0.2, 0.02);
            },
            CircuitType::Mixed => {
                // Mixed circuits: balanced approach
                self.phase1_pid = AdaptivePIDController::new(8.0, 1.5, 0.08);
                self.phase2_pid = AdaptivePIDController::new(1.2, 0.15, 0.015);
            },
        }
    }
    
    /// Perform adaptive DC analysis
    pub fn analyze(&mut self) -> Result<AnalysisResult> {
        info!("Starting adaptive logarithmic gradient DC analysis");
        
        // Detect circuit type for optimal configuration
        let circuit_type = self.detect_circuit_type();
        self.configure_for_circuit_type(circuit_type);
        
        // Get ground node
        let ground_idx = self.circuit.ground_node()
            .ok_or_else(|| SpiceError::NoGroundNode)?
            .0;
        
        // Build node list (excluding ground)
        let node_list: Vec<NodeIndex> = self.circuit.nodes()
            .filter(|(idx, _)| *idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
        
        let num_nodes = node_list.len();
        
        // Find voltage sources
        let voltage_sources: Vec<EdgeIndex> = self.circuit.branches()
            .filter(|(_, branch)| {
                self.models.get(&branch.name)
                    .map(|m| matches!(m, ComponentModel::VoltageSource { .. }))
                    .unwrap_or(false)
            })
            .map(|(idx, _)| idx)
            .collect();
        
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        
        if matrix_size == 0 {
            return Err(SpiceError::EmptyCircuit);
        }
        
        // Initial guess - small perturbation to help convergence
        let mut x = DVector::zeros(matrix_size);
        for i in 0..num_nodes {
            x[i] = 0.01; // Small initial voltage to avoid singularities
        }
        
        // Reset PID controllers
        self.phase1_pid.reset();
        self.phase2_pid.reset();
        self.current_phase = 1;
        self.convergence_history.clear();
        
        // Two-Phase Adaptive Convergence Loop
        let mut iteration = 0;
        let mut converged = false;
        let mut max_change = f64::INFINITY;
        
        while iteration < self.max_iterations && !converged {
            let old_x = x.clone();
            
            // Build Jacobian matrix and residual vector
            let (jacobian, residual) = self.build_system_matrices(
                &x, &node_list, ground_idx, &voltage_sources
            )?;
            
            // Calculate convergence error
            max_change = residual.norm();
            self.convergence_history.push(max_change);
            
            // Debug output for first few iterations
            if iteration < 5 {
                println!("Iteration {}: residual_norm = {:.6e}", iteration, max_change);
                if matrix_size <= 4 {  // Only for small matrices
                    println!("  Jacobian:\n{}", jacobian);
                    println!("  Residual: {}", residual.transpose());
                    println!("  Solution x: {}", x.transpose());
                }
            }
            
            // Calculate logarithmic gradient for adaptive control
            let log_gradient = self.calculate_logarithmic_gradient(&x, &node_list);
            
            // Calculate ramp factor for phase switching
            let ramp_factor = if max_change > 0.0 && self.convergence_history.len() > 5 {
                let initial_error = self.convergence_history[0];
                1.0 - (max_change / initial_error)
            } else {
                0.0
            };
            
            // Phase switching logic
            if self.current_phase == 1 && ramp_factor > self.phase_switch_threshold && max_change < 1e-10 {
                info!("Switching to Phase 2 (precision) at iteration {}", iteration);
                self.current_phase = 2;
                self.phase2_pid.reset();
            }
            
            // Check convergence
            if max_change < self.tolerance {
                converged = true;
                break;
            }
            
            // Solve system: J * dx = -residual
            let dx = jacobian.lu().solve(&(-&residual))
                .ok_or_else(|| SpiceError::SingularMatrix)?;
            
            // Adaptive PID control for step size
            let current_pid = if self.current_phase == 1 {
                &mut self.phase1_pid
            } else {
                &mut self.phase2_pid
            };
            
            let control_signal = current_pid.compute(max_change, log_gradient);
            
            // Improved step size calculation
            let step_size = if max_change > 1.0 {
                // Large errors: use controlled step
                0.1 + 0.9 * (1.0 / (1.0 + control_signal.abs()))
            } else {
                // Small errors: near full Newton step
                0.8 + 0.2 * (1.0 / (1.0 + control_signal.abs()))
            };
            
            // Apply controlled update
            x = &old_x + &(step_size * dx);
            
            // Apply voltage limiting to prevent numerical issues
            for i in 0..num_nodes {
                x[i] = x[i].clamp(-100.0, 100.0);
            }
            
            // Debug output
            if iteration % 50 == 0 || iteration < 10 {
                debug!("Iteration {}: Phase {}, Error = {:.3e}, LogGrad = {:.2}, StepSize = {:.3}", 
                       iteration, self.current_phase, max_change, log_gradient, step_size);
            }
            
            iteration += 1;
        }
        
        if !converged {
            return Err(SpiceError::ConvergenceFailed(iteration));
        }
        
        info!("Converged in {} iterations (Phase 1→2 transition)", iteration);
        
        // Extract results
        let mut node_voltages = NodeVoltages::new();
        node_voltages.insert(ground_idx, 0.0);
        
        for (i, &node_idx) in node_list.iter().enumerate() {
            node_voltages.insert(node_idx, x[i]);
            self.circuit.set_node_voltage(node_idx, x[i]);
        }
        
        // Calculate branch currents
        let vsource_currents: Vec<f64> = (num_nodes..matrix_size)
            .map(|i| x[i])
            .collect();
        let branch_currents = self.calculate_branch_currents(
            &node_voltages, &voltage_sources, &vsource_currents
        )?;
        
        // Calculate total power
        let total_power = self.calculate_total_power(&node_voltages, &branch_currents);
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: iteration,
        })
    }
    
    /// Detect circuit type for optimal solver configuration
    fn detect_circuit_type(&self) -> CircuitType {
        let mut has_nonlinear = false;
        let mut has_linear = false;
        
        for model in self.models.values() {
            match model {
                ComponentModel::Resistor { .. } | 
                ComponentModel::VoltageSource { .. } | 
                ComponentModel::CurrentSource { .. } => {
                    has_linear = true;
                },
                ComponentModel::LED { .. } | 
                ComponentModel::Diode { .. } => {
                    has_nonlinear = true;
                },
                _ => {
                    has_linear = true; // Default to linear
                }
            }
        }
        
        match (has_linear, has_nonlinear) {
            (true, true) => CircuitType::Mixed,
            (false, true) => CircuitType::Nonlinear,
            _ => CircuitType::Linear,
        }
    }
    
    /// Calculate logarithmic gradient for adaptive control
    fn calculate_logarithmic_gradient(&self, x: &DVector<f64>, node_list: &[NodeIndex]) -> f64 {
        let mut total_gradient = 0.0;
        let mut count = 0;
        
        // Examine each component for sensitivity calculation
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                if let Some(model) = self.models.get(&branch.name) {
                    // Get node voltages
                    let v1 = if let Some(idx) = node_list.iter().position(|&n| n == n1) {
                        x[idx]
                    } else {
                        0.0 // Ground node
                    };
                    let v2 = if let Some(idx) = node_list.iter().position(|&n| n == n2) {
                        x[idx] 
                    } else {
                        0.0 // Ground node
                    };
                    let v_diff = v1 - v2;
                    
                    let local_gradient = match model {
                        ComponentModel::LED { dynamic_resistance, .. } => {
                            // For LED: d(ln(I))/dV ≈ 1/(Vt + V/R_d)
                            let vt = 0.026; // Thermal voltage
                            let rd = *dynamic_resistance;
                            1.0 / (vt + v_diff.abs() / rd)
                        },
                        ComponentModel::Diode { emission_coefficient, .. } => {
                            // For diode: d(ln(I))/dV = 1/(n*Vt)
                            let vt = 0.026;
                            let n = emission_coefficient.unwrap_or(1.0);
                            1.0 / (n * vt)
                        },
                        ComponentModel::Resistor { .. } => {
                            // For resistor: d(ln(I))/dV = d(ln(V/R))/dV = 1/V
                            1.0 / v_diff.abs().max(0.001)
                        },
                        _ => {
                            // Default: assume linear behavior
                            1.0
                        }
                    };
                    
                    total_gradient += local_gradient;
                    count += 1;
                }
            }
        }
        
        if count > 0 {
            total_gradient / count as f64
        } else {
            1.0 // Default gradient
        }
    }
    
    /// Build system matrices (Jacobian and residual)
    fn build_system_matrices(
        &mut self,
        x: &DVector<f64>,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
    ) -> Result<(DMatrix<f64>, DVector<f64>)> {
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        let size = num_nodes + num_vsources;
        
        let mut jacobian = DMatrix::zeros(size, size);
        let mut residual = DVector::zeros(size);
        
        // Collect branch information first to avoid borrow conflicts
        let branch_info: Vec<_> = self.circuit.branches()
            .filter_map(|(edge_idx, branch)| {
                self.circuit.branch_nodes(edge_idx).map(|(n1, n2)| {
                    (edge_idx, branch.name.clone(), n1, n2)
                })
            })
            .collect();
        
        // Process each branch - works for both linear and nonlinear
        for (edge_idx, branch_name, n1, n2) in branch_info {
            // Get node indices in matrix
            let n1_idx = if n1 == ground_idx {
                None
            } else {
                node_list.iter().position(|&n| n == n1)
            };
            
            let n2_idx = if n2 == ground_idx {
                None
            } else {
                node_list.iter().position(|&n| n == n2)
            };
            
            // Get node voltages
            let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
            let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
            let v_diff = v1 - v2;
            
            // Try stdlib model first, fallback to hardcoded models if needed
            self.stamp_component(&branch_name, &mut jacobian, &mut residual, 
                               n1_idx, n2_idx, v_diff, x)?;
        }
        
        // Add very small conductance to ground for numerical stability
        // This prevents singular matrices when nodes are only connected through voltage sources
        let gmin = 1e-12; // Very small conductance (1 pS)
        for i in 0..num_nodes {
            jacobian[(i, i)] += gmin;
            // No residual current for gmin since V_ground = 0
        }
        
        // Handle voltage sources
        for (vsrc_num, &edge_idx) in voltage_sources.iter().enumerate() {
            self.stamp_voltage_source(&mut jacobian, &mut residual, x, 
                                    edge_idx, vsrc_num, node_list, ground_idx)?;
        }
        
        Ok((jacobian, residual))
    }
    
    /// Stamp component into system matrices using runtime model engine
    fn stamp_component(
        &mut self,
        component_name: &str,
        jacobian: &mut DMatrix<f64>,
        residual: &mut DVector<f64>,
        n1_idx: Option<usize>,
        n2_idx: Option<usize>,
        v_diff: f64,
        x: &DVector<f64>,
    ) -> Result<()> {
        // Create model execution context
        let mut ctx = ModelExecutionContext {
            jacobian,
            residual,
            x,
            n1_idx,
            n2_idx,
            v_diff,
        };
        
        // Execute model through runtime engine
        self.model_engine.execute_component_model(component_name, &mut ctx)
            .map_err(|e| SpiceError::AnalysisFailed(format!("Model execution failed: {}", e)))
    }
    
    /// Stamp linear element (conductance) into matrices
    fn stamp_linear_element(
        &self,
        jacobian: &mut DMatrix<f64>,
        residual: &mut DVector<f64>,
        n1_idx: Option<usize>,
        n2_idx: Option<usize>,
        conductance: f64,
        current: f64,
    ) {
        // Stamp conductance in Jacobian
        if let Some(i1) = n1_idx {
            jacobian[(i1, i1)] += conductance;
            residual[i1] += current;
        }
        if let Some(i2) = n2_idx {
            jacobian[(i2, i2)] += conductance;
            residual[i2] -= current;
        }
        if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
            jacobian[(i1, i2)] -= conductance;
            jacobian[(i2, i1)] -= conductance;
        }
    }
    
    /// Stamp voltage source into system matrices
    fn stamp_voltage_source(
        &self,
        jacobian: &mut DMatrix<f64>,
        residual: &mut DVector<f64>,
        x: &DVector<f64>,
        edge_idx: EdgeIndex,
        vsrc_num: usize,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
    ) -> Result<()> {
        if let Some(branch) = self.circuit.branches().find(|(idx, _)| *idx == edge_idx) {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                if let Some(ComponentModel::VoltageSource { voltage, internal_resistance, .. }) = self.models.get(&branch.1.name) {
                    let vsrc_row = node_list.len() + vsrc_num;
                    let vsrc_current = x[vsrc_row];
                    
                    let n1_idx = if n1 == ground_idx { None } else { node_list.iter().position(|&n| n == n1) };
                    let n2_idx = if n2 == ground_idx { None } else { node_list.iter().position(|&n| n == n2) };
                    
                    // Handle internal resistance if present
                    if let Some(r_int) = internal_resistance {
                        if *r_int > 0.0 {
                            // For voltage source with internal resistance:
                            // Use auxiliary variable method but include resistance in residual
                            
                            // KCL equations with current variable
                            if let Some(i1) = n1_idx {
                                jacobian[(i1, vsrc_row)] = 1.0;
                                jacobian[(vsrc_row, i1)] = 1.0;
                                residual[i1] += vsrc_current;
                            }
                            if let Some(i2) = n2_idx {
                                jacobian[(i2, vsrc_row)] = -1.0;
                                jacobian[(vsrc_row, i2)] = -1.0;
                                residual[i2] -= vsrc_current;
                            }
                            
                            // Voltage constraint with internal resistance: V1 - V2 = Voltage - I*R_int
                            let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                            let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                            residual[vsrc_row] = v1 - v2 - (voltage - vsrc_current * r_int);
                            
                            // Add derivative for internal resistance: d(residual)/dI = R_int
                            jacobian[(vsrc_row, vsrc_row)] = *r_int;
                        } else {
                            // Ideal voltage source (R_int = 0) - use auxiliary variable method
                            self.stamp_ideal_voltage_source(jacobian, residual, x, voltage, vsrc_row, n1_idx, n2_idx)?;
                        }
                    } else {
                        // No internal resistance specified - treat as ideal
                        self.stamp_ideal_voltage_source(jacobian, residual, x, voltage, vsrc_row, n1_idx, n2_idx)?;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// Stamp ideal voltage source using auxiliary variable method
    fn stamp_ideal_voltage_source(
        &self,
        jacobian: &mut DMatrix<f64>,
        residual: &mut DVector<f64>,
        x: &DVector<f64>,
        voltage: &f64,
        vsrc_row: usize,
        n1_idx: Option<usize>,
        n2_idx: Option<usize>,
    ) -> Result<()> {
        // For ideal voltage source: we need entries in Jacobian for voltage constraint
        // KCL equations: I flows from node1 to node2
        if let Some(i1) = n1_idx {
            jacobian[(i1, vsrc_row)] = 1.0;      // dKCL1/dI = 1
            jacobian[(vsrc_row, i1)] = 1.0;      // dVconstraint/dV1 = 1
        }
        if let Some(i2) = n2_idx {
            jacobian[(i2, vsrc_row)] = -1.0;     // dKCL2/dI = -1  
            jacobian[(vsrc_row, i2)] = -1.0;     // dVconstraint/dV2 = -1
        }
        
        // Current terms in residual (KCL)
        let vsrc_current = x[vsrc_row];
        if let Some(i1) = n1_idx {
            residual[i1] += vsrc_current;         // Current leaves node 1
        }
        if let Some(i2) = n2_idx {
            residual[i2] -= vsrc_current;         // Current enters node 2  
        }
        
        // Voltage constraint residual: V1 - V2 - Vsource = 0
        let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
        let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
        residual[vsrc_row] = v1 - v2 - voltage;
        
        Ok(())
    }
    
    /// Calculate branch currents from solution
    fn calculate_branch_currents(
        &mut self,
        node_voltages: &NodeVoltages,
        voltage_sources: &[EdgeIndex],
        vsource_currents: &[f64],
    ) -> Result<BranchCurrents> {
        let mut branch_currents = BranchCurrents::new();
        
        let branches: Vec<_> = self.circuit.branches()
            .map(|(idx, branch)| (idx, branch.name.clone()))
            .collect();
        
        for (edge_idx, branch_name) in branches {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                let v_diff = v1 - v2;
                
                let current = if let Some(model) = self.models.get(&branch_name) {
                    match model {
                        ComponentModel::VoltageSource { .. } => {
                            if let Some(vsrc_idx) = voltage_sources.iter().position(|&e| e == edge_idx) {
                                vsource_currents[vsrc_idx]
                            } else {
                                0.0
                            }
                        },
                        ComponentModel::LED { forward_voltage, dynamic_resistance, .. } => {
                            if v_diff > forward_voltage * 0.7 {
                                (v_diff - forward_voltage) / dynamic_resistance
                            } else {
                                v_diff * 1e-12
                            }
                        },
                        ComponentModel::Diode { saturation_current, emission_coefficient, .. } => {
                            let vt = 0.026;
                            let n = emission_coefficient.unwrap_or(1.0);
                            let is = saturation_current.unwrap_or(1e-12);
                            
                            if v_diff > 0.0 {
                                is * ((v_diff / (n * vt)).min(40.0).exp() - 1.0)
                            } else {
                                -is
                            }
                        },
                        _ => {
                            let resistance = model.dc_resistance();
                            if resistance.is_finite() && resistance > 0.0 {
                                v_diff / resistance
                            } else {
                                0.0
                            }
                        }
                    }
                } else {
                    0.0
                };
                
                branch_currents.insert(edge_idx, current);
                self.circuit.set_branch_current(edge_idx, current);
            }
        }
        
        Ok(branch_currents)
    }
    
    /// Calculate total power dissipation
    fn calculate_total_power(
        &self,
        node_voltages: &NodeVoltages,
        branch_currents: &BranchCurrents,
    ) -> f64 {
        let mut total_power = 0.0;
        
        for (edge_idx, &current) in branch_currents {
            if let Some((n1, n2)) = self.circuit.branch_nodes(*edge_idx) {
                let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                let voltage_diff = (v1 - v2).abs();
                let power = voltage_diff * current.abs();
                total_power += power;
            }
        }
        
        total_power
    }
}

/// Circuit type classification for optimal solver configuration
#[derive(Debug, Clone, Copy)]
pub enum CircuitType {
    Linear,    // Only resistors, voltage/current sources
    Nonlinear, // Diodes, LEDs, transistors
    Mixed,     // Combination of linear and nonlinear
}

/// Custom error types
impl From<()> for SpiceError {
    fn from(_: ()) -> Self {
        SpiceError::AnalysisFailed("Unknown error in adaptive solver".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Circuit;
    
    #[test]
    fn test_adaptive_pid_controller() {
        let mut pid = AdaptivePIDController::new(1.0, 0.1, 0.01);
        
        // Test normal sensitivity
        let output1 = pid.compute(0.1, 5.0);
        assert!(output1 > 0.0, "PID should produce positive output for positive error");
        
        // Test low sensitivity adaptation
        let output2 = pid.compute(0.1, 1.0);
        assert!(output2 > output1, "PID should be more aggressive for low sensitivity");
        
        // Test high sensitivity adaptation  
        let output3 = pid.compute(0.1, 15.0);
        assert!(output3 < output1, "PID should be more conservative for high sensitivity");
    }
    
    #[test]
    fn test_circuit_type_detection() {
        let circuit = Circuit::new();
        let mut solver = AdaptiveCircuitSolver::new(circuit);
        
        // Add linear components
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 1000.0, 
            limits: None 
        });
        
        let circuit_type = solver.detect_circuit_type();
        assert!(matches!(circuit_type, CircuitType::Linear));
        
        // Add nonlinear component
        solver.add_model("D1".to_string(), ComponentModel::LED { 
            forward_voltage: 2.0,
            dynamic_resistance: 10.0,
            limits: None 
        });
        
        let circuit_type = solver.detect_circuit_type();
        assert!(matches!(circuit_type, CircuitType::Mixed));
    }
}