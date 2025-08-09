//! GLACIER Transient Analysis Extension
//! 
//! Implements time-domain logarithmic transformation for transient analysis
//! avoiding exponentials throughout the computation.

use nalgebra::{DMatrix, DVector};
use petgraph::graph::{NodeIndex, EdgeIndex};
use std::collections::HashMap;
use log::{info, debug, warn};

use crate::{
    Circuit, Branch, ComponentModel, SpiceError, Result,
    NodeVoltages, BranchCurrents, AnalysisResult,
    glacier_solver::{AdaptivePIDController, GlacierSolver},
    runtime_models::ModelExecutionContext,
};

/// Variable type in mixed formulation
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariableType {
    /// Linear voltage variable
    Voltage,
    /// Logarithmic current variable (w = log(i))
    LogCurrent,
}

/// Mixed variable representation for transient analysis
#[derive(Debug, Clone)]
pub struct MixedVariable {
    pub id: usize,
    pub var_type: VariableType,
    pub value: f64,
    pub node_id: Option<NodeIndex>,
    pub branch_id: Option<usize>,
}

/// Companion model in log space
#[derive(Debug, Clone)]
pub struct LogCompanionModel {
    /// log(G) for the companion conductance
    pub log_conductance: f64,
    /// log(I) for the companion current source
    pub log_current: f64,
    /// Reference value for scaling
    pub reference_value: f64,
}

/// Logarithmic time derivative handler
pub struct LogarithmicTimeDerivative {
    vt: f64,  // Thermal voltage
}

impl LogarithmicTimeDerivative {
    pub fn new(vt: f64) -> Self {
        Self { vt }
    }
    
    /// Compute log-space update for exponential device
    pub fn compute_log_space_update(&self, w_old: f64, v_old: f64, 
                                   v_new: f64, dt: f64) -> f64 {
        // w = log(i), update directly in log space
        // For exponential device: log(i) ≈ log(Is) + v/Vt
        
        if v_old > 4.0 * self.vt {
            // Strong forward bias - linear in log space
            w_old + (v_new - v_old) / self.vt
        } else {
            // Near threshold - use careful update
            let i_old = w_old.exp();
            let di_dv = i_old / self.vt;  // Simplified derivative
            let di = di_dv * (v_new - v_old);
            
            // Logarithmic update
            if (i_old + di * dt) > 0.0 {
                (i_old + di * dt).ln()
            } else {
                w_old - 10.0  // Large negative log for zero current
            }
        }
    }
}

/// Capacitor companion model in log space
pub struct LogCapacitorCompanion {
    capacitance: f64,
}

impl LogCapacitorCompanion {
    pub fn new(capacitance: f64) -> Self {
        Self { capacitance }
    }
    
    /// Build log-space companion model
    pub fn build_log_companion(&self, v_old: f64, w_old: f64, dt: f64) -> LogCompanionModel {
        // w = log(i) for the capacitor current
        // In normal space: i_new = C * (v_new - v_old) / dt
        // In log space: w_new = log(C * (v_new - v_old) / dt)
        
        // For the companion model G*v + I = 0:
        // We need to express this in terms of log currents
        
        // Key insight: For small dt, capacitor current can be large
        // So we work with log(G) and log(I) directly
        
        let log_g = (self.capacitance / dt).ln();
        let log_i_history = w_old;  // Previous log current
        
        LogCompanionModel {
            log_conductance: log_g,
            log_current: log_i_history,
            reference_value: v_old,
        }
    }
}

/// Inductor companion model in log space
pub struct LogInductorCompanion {
    inductance: f64,
}

impl LogInductorCompanion {
    pub fn new(inductance: f64) -> Self {
        Self { inductance }
    }
    
    /// Build log-space companion model
    pub fn build_log_companion(&self, v_old: f64, i_old: f64, dt: f64) -> LogCompanionModel {
        // v = L * di/dt
        // Rearranged: i = (1/L) * integral(v)
        
        let r_eq = self.inductance / dt;
        let v_eq = r_eq * i_old;
        
        // Transform for current in log space
        if i_old.abs() > 1e-20 {
            let r_eq_log = r_eq / i_old.abs();
            LogCompanionModel {
                log_conductance: (1.0 / r_eq_log).ln(),
                log_current: v_eq.ln(),
                reference_value: i_old,
            }
        } else {
            LogCompanionModel {
                log_conductance: (1.0 / r_eq).ln(),
                log_current: v_eq.ln(),
                reference_value: 0.0,
            }
        }
    }
}

/// Integration method for GLACIER transient
#[derive(Debug, Clone, Copy)]
pub enum GlacierIntegration {
    LogarithmicBackwardEuler,
    LogarithmicTrapezoidal,
    AdaptiveLogBDF,
}

impl GlacierIntegration {
    /// Select method based on circuit characteristics
    pub fn select_method(sharpness: f64, stiffness: f64) -> Self {
        if sharpness > 50.0 || stiffness > 1000.0 {
            // Ultra-stiff or sharp: maximum stability
            GlacierIntegration::LogarithmicBackwardEuler
        } else if stiffness > 100.0 {
            // Moderately stiff: adaptive order
            GlacierIntegration::AdaptiveLogBDF
        } else {
            // Normal: good accuracy
            GlacierIntegration::LogarithmicTrapezoidal
        }
    }
}

/// Transient state tracking
#[derive(Debug, Clone)]
pub struct TransientState {
    pub time: f64,
    pub variables: Vec<MixedVariable>,
    pub history: Vec<Vec<f64>>,  // Previous timestep values
    pub companion_models: HashMap<usize, LogCompanionModel>,
}

/// GLACIER Transient Solver
pub struct GlacierTransientSolver {
    /// Base GLACIER DC solver
    dc_solver: GlacierSolver,
    
    /// Transient-specific PID controller
    transient_pid: AdaptivePIDController,
    
    /// Integration method
    integration_method: GlacierIntegration,
    
    /// Timestep controller
    dt_min: f64,
    dt_max: f64,
    
    /// Thermal voltage
    vt: f64,
}

impl GlacierTransientSolver {
    pub fn new(circuit: Circuit) -> Self {
        Self {
            dc_solver: GlacierSolver::new(circuit),
            transient_pid: AdaptivePIDController::new(0.4, 0.2, 0.1),
            integration_method: GlacierIntegration::LogarithmicBackwardEuler,
            dt_min: 1e-12,
            dt_max: 1e-6,
            vt: 0.026,  // 26mV at room temperature
        }
    }
    
    /// Solve a single timestep
    pub fn solve_timestep(&mut self, circuit: &Circuit, state: &TransientState, 
                         dt: f64) -> Result<TransientState> {
        info!("Solving timestep at t={} with dt={}", state.time, dt);
        
        // Build companion models for reactive components
        let companion_models = self.build_companion_models(circuit, state, dt)?;
        
        // Create modified circuit with companion models
        let transient_circuit = self.apply_companion_models(circuit, &companion_models)?;
        
        // Set up mixed variable system
        let (matrix, rhs) = self.build_mixed_system(&transient_circuit, state, &companion_models)?;
        
        // Solve with logarithmic scaling
        let solution = self.solve_scaled_system(matrix, rhs)?;
        
        // Update state with new solution
        let new_state = self.update_state(state, solution, dt);
        
        Ok(new_state)
    }
    
    /// Build companion models for all reactive components
    fn build_companion_models(&self, circuit: &Circuit, state: &TransientState, 
                             dt: f64) -> Result<HashMap<usize, LogCompanionModel>> {
        let mut models = HashMap::new();
        
        for (idx, (_edge_idx, branch)) in circuit.branches().enumerate() {
            match branch.component_type.as_str() {
                "Capacitor" => {
                    let v_old = self.get_voltage_across_branch(state, branch)?;
                    let w_old = self.get_log_current_through_branch(state, idx)
                        .unwrap_or(-20.0);  // Very small current initially
                    
                    let capacitance = branch.value;  // Capacitance stored as value
                    let companion = LogCapacitorCompanion::new(capacitance);
                    models.insert(idx, companion.build_log_companion(v_old, w_old, dt));
                }
                "Inductor" => {
                    let v_old = self.get_voltage_across_branch(state, branch)?;
                    let i_old = self.get_current_through_branch(state, idx)?;
                    
                    let inductance = branch.value;  // Inductance stored as value
                    let companion = LogInductorCompanion::new(inductance);
                    models.insert(idx, companion.build_log_companion(v_old, i_old, dt));
                }
                _ => {} // Non-reactive components don't need companion models
            }
        }
        
        Ok(models)
    }
    
    /// Apply companion models to create transient circuit
    fn apply_companion_models(&self, circuit: &Circuit, 
                             models: &HashMap<usize, LogCompanionModel>) -> Result<Circuit> {
        // For now, return original circuit
        // In full implementation, would replace reactive components with companions
        Ok(circuit.clone())
    }
    
    /// Build mixed linear/logarithmic system
    fn build_mixed_system(&self, circuit: &Circuit, state: &TransientState,
                         companions: &HashMap<usize, LogCompanionModel>) 
                         -> Result<(DMatrix<f64>, DVector<f64>)> {
        let n_vars = state.variables.len();
        let mut matrix = DMatrix::zeros(n_vars, n_vars);
        let mut rhs = DVector::zeros(n_vars);
        
        // Placeholder - would build full MNA system with log variables
        for i in 0..n_vars {
            matrix[(i, i)] = 1.0;
            rhs[i] = state.variables[i].value;
        }
        
        Ok((matrix, rhs))
    }
    
    /// Solve system with logarithmic scaling
    fn solve_scaled_system(&self, mut matrix: DMatrix<f64>, mut rhs: DVector<f64>) 
                          -> Result<DVector<f64>> {
        // Find maximum values for scaling
        let max_element = matrix.iter().map(|x| x.abs()).fold(0.0, f64::max);
        let max_rhs = rhs.iter().map(|x| x.abs()).fold(0.0, f64::max);
        
        // Scale to prevent overflow
        if max_element > 1e10 {
            let scale = 1e10 / max_element;
            matrix *= scale;
            rhs *= scale;
        }
        
        // Solve scaled system
        let lu = matrix.lu();
        let solution = lu.solve(&rhs)
            .ok_or(SpiceError::SingularMatrix)?;
        
        Ok(solution)
    }
    
    /// Update state with new solution
    fn update_state(&self, state: &TransientState, solution: DVector<f64>, dt: f64) 
                   -> TransientState {
        let mut new_state = state.clone();
        new_state.time += dt;
        
        // Update history
        let current_values: Vec<f64> = state.variables.iter()
            .map(|v| v.value)
            .collect();
        new_state.history.push(current_values);
        
        // Keep only recent history
        if new_state.history.len() > 5 {
            new_state.history.remove(0);
        }
        
        // Update variables
        for (i, var) in new_state.variables.iter_mut().enumerate() {
            var.value = solution[i];
        }
        
        new_state
    }
    
    /// Compute adaptive timestep based on gradient
    pub fn compute_adaptive_timestep(&self, state: &TransientState) -> f64 {
        // Compute temporal gradient from history
        let gradient = if state.history.len() >= 2 {
            let mut max_gradient = 0.0;
            let last_idx = state.history.len() - 1;
            
            for i in 0..state.variables.len() {
                let v_curr = state.variables[i].value;
                let v_prev = state.history[last_idx][i];
                let v_prev2 = if last_idx > 0 { 
                    state.history[last_idx - 1][i] 
                } else { 
                    v_prev 
                };
                
                // Second-order gradient approximation
                let grad = ((v_curr - v_prev).abs() + (v_prev - v_prev2).abs()) / 2.0;
                max_gradient = f64::max(max_gradient, grad);
            }
            
            max_gradient
        } else {
            1.0  // Default moderate gradient
        };
        
        // Logarithmic timestep scaling based on gradient
        if gradient > 100.0 {
            self.dt_min
        } else if gradient > 10.0 {
            let log_factor = (f64::log10(gradient) - 1.0) / 2.0;
            self.dt_min * (10.0_f64).powf(1.0 - log_factor)
        } else if gradient > 1.0 {
            let factor = (gradient - 1.0) / 9.0;
            self.dt_min * (self.dt_max / self.dt_min).powf(1.0 - factor)
        } else {
            self.dt_max
        }
    }
    
    // Helper methods
    fn get_voltage_across_branch(&self, _state: &TransientState, 
                                _branch: &crate::Branch) -> Result<f64> {
        // Placeholder - would compute voltage from state
        Ok(0.0)
    }
    
    fn get_current_through_branch(&self, _state: &TransientState,
                                 _branch_idx: usize) -> Result<f64> {
        // Placeholder - would compute current from state
        Ok(0.0)
    }
    
    fn get_log_current_through_branch(&self, _state: &TransientState,
                                     _branch_idx: usize) -> Option<f64> {
        // Placeholder - would get log current if available
        None
    }
}

/// Transient analysis result
#[derive(Debug, Clone)]
pub struct TransientAnalysisResult {
    /// Time points
    pub time_points: Vec<f64>,
    
    /// Node voltages at each time point
    pub voltages: Vec<NodeVoltages>,
    
    /// Branch currents at each time point
    pub currents: Vec<BranchCurrents>,
    
    /// Number of iterations per timestep
    pub iterations: Vec<usize>,
    
    /// Timesteps used
    pub timesteps: Vec<f64>,
}

/// Run transient analysis
pub fn run_transient_analysis(circuit: Circuit, t_start: f64, t_end: f64,
                             initial_state: Option<TransientState>) -> Result<TransientAnalysisResult> {
    let mut solver = GlacierTransientSolver::new(circuit.clone());
    let circuit_ref = &circuit;
    
    // Initialize state
    let mut state = initial_state.unwrap_or_else(|| {
        // Create default initial state
        let n_nodes = circuit.graph.node_count();
        let n_branches = circuit.branches().count();
        
        let mut variables = Vec::new();
        
        // Add voltage variables
        for i in 0..n_nodes {
            variables.push(MixedVariable {
                id: i,
                var_type: VariableType::Voltage,
                value: 0.0,
                node_id: Some(NodeIndex::new(i)),
                branch_id: None,
            });
        }
        
        // Add log current variables for nonlinear branches
        // (Would identify which branches need log currents)
        
        TransientState {
            time: t_start,
            variables,
            history: Vec::new(),
            companion_models: HashMap::new(),
        }
    });
    
    // Run transient simulation
    let mut time_points = vec![t_start];
    let mut voltages = Vec::new();
    let mut currents = Vec::new();
    let mut iterations = Vec::new();
    let mut timesteps = Vec::new();
    
    let mut time = t_start;
    while time < t_end {
        // Compute adaptive timestep
        let dt = solver.compute_adaptive_timestep(&state);
        let dt = dt.min(t_end - time);  // Don't overshoot end time
        
        // Solve timestep
        let new_state = solver.solve_timestep(circuit_ref, &state, dt)?;
        
        // Extract results
        let node_voltages = extract_node_voltages(&new_state);
        let branch_currents = extract_branch_currents(&new_state);
        
        time_points.push(new_state.time);
        voltages.push(node_voltages);
        currents.push(branch_currents);
        iterations.push(1);  // Placeholder
        timesteps.push(dt);
        
        // Update state
        state = new_state;
        time = state.time;
    }
    
    Ok(TransientAnalysisResult {
        time_points,
        voltages,
        currents,
        iterations,
        timesteps,
    })
}

fn extract_node_voltages(state: &TransientState) -> crate::NodeVoltages {
    let mut voltages = HashMap::new();
    
    for var in &state.variables {
        if let (VariableType::Voltage, Some(node_id)) = (var.var_type, var.node_id) {
            voltages.insert(node_id, var.value);
        }
    }
    
    voltages
}

fn extract_branch_currents(state: &TransientState) -> crate::BranchCurrents {
    let mut currents = HashMap::new();
    
    for var in &state.variables {
        if let Some(branch_id) = var.branch_id {
            let current = match var.var_type {
                VariableType::LogCurrent => var.value.exp(),  // Convert from log
                _ => var.value,
            };
            currents.insert(EdgeIndex::new(branch_id), current);
        }
    }
    
    currents
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_log_space_update() {
        let deriv = LogarithmicTimeDerivative::new(0.026);
        
        // Test strong forward bias
        let w_old = -5.0;  // log(i) = -5
        let v_old = 0.7;   // 700mV
        let v_new = 0.72;  // 720mV
        let dt = 1e-9;
        
        let w_new = deriv.compute_log_space_update(w_old, v_old, v_new, dt);
        
        // Should increase linearly with voltage
        let expected = w_old + (v_new - v_old) / 0.026;
        assert!((w_new - expected).abs() < 1e-10);
    }
    
    #[test]
    fn test_capacitor_companion() {
        let cap = LogCapacitorCompanion::new(1e-6);  // 1uF
        let companion = cap.build_log_companion(5.0, -10.0, 1e-9);
        
        // Check that log conductance is reasonable
        assert!(companion.log_conductance > 0.0);  // Should be positive (large G)
    }
    
    #[test]
    fn test_adaptive_timestep() {
        let circuit = Circuit::new();
        let solver = GlacierTransientSolver::new(&circuit);
        
        // Create state with some history
        let mut state = TransientState {
            time: 0.0,
            variables: vec![
                MixedVariable {
                    id: 0,
                    var_type: VariableType::Voltage,
                    value: 5.0,
                    node_id: Some(NodeIndex::new(0)),
                    branch_id: None,
                }
            ],
            history: vec![vec![4.0], vec![4.5]],  // Shows increasing voltage
            companion_models: HashMap::new(),
        };
        
        let dt = solver.compute_adaptive_timestep(&state);
        
        // Should give moderate timestep for moderate gradient
        assert!(dt > solver.dt_min);
        assert!(dt < solver.dt_max);
    }
}