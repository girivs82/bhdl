//! Generic GLACIER Solver - Pure Numerical Analysis
//! 
//! This solver has NO knowledge of circuits, components, or electrical concepts.
//! It only knows about:
//! - Variables (can be in linear or log space)
//! - Equations (residuals)
//! - Jacobians
//! - Numerical convergence

use nalgebra::{DMatrix, DVector};
use log::{info, debug};
use std::collections::HashMap;

use crate::errors::{SpiceError, Result};

/// Variable representation type
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum VariableSpace {
    /// Linear space variable
    Linear,
    /// Logarithmic space variable (x represents log(actual_value))
    Logarithmic,
}

/// A variable in the system
#[derive(Debug, Clone)]
pub struct Variable {
    /// Unique identifier
    pub id: usize,
    /// Variable name (for debugging)
    pub name: String,
    /// Space representation
    pub space: VariableSpace,
    /// Current value
    pub value: f64,
}

/// Equation residual and Jacobian contribution
pub trait EquationSystem: Send + Sync {
    /// Evaluate residuals for all equations given current variable values
    fn evaluate_residuals(&self, variables: &[Variable]) -> DVector<f64>;
    
    /// Build Jacobian matrix given current variable values
    fn build_jacobian(&self, variables: &[Variable]) -> DMatrix<f64>;
    
    /// Get number of equations
    fn num_equations(&self) -> usize;
    
    /// Get number of variables
    fn num_variables(&self) -> usize;
    
    /// Optional: provide scaling hints for better conditioning
    fn get_scaling_hints(&self) -> Option<Vec<f64>> {
        None
    }
}

/// Adaptive control parameters
pub struct AdaptiveControl {
    /// Base proportional gain
    base_kp: f64,
    /// Current proportional gain
    kp: f64,
    /// Integral gain
    ki: f64,
    /// Derivative gain
    kd: f64,
    /// Integral accumulator
    integral: f64,
    /// Last error
    last_error: f64,
    /// Gradient filter
    filtered_gradient: f64,
    /// Filter coefficient
    filter_alpha: f64,
}

impl AdaptiveControl {
    pub fn new() -> Self {
        Self {
            base_kp: 0.7,
            kp: 0.7,
            ki: 0.0,
            kd: 0.0,
            integral: 0.0,
            last_error: 0.0,
            filtered_gradient: 1.0,
            filter_alpha: 0.1,
        }
    }
    
    /// Adapt gains based on system gradient
    pub fn adapt(&mut self, gradient: f64) {
        // Simple gradient-based adaptation
        self.filtered_gradient = self.filter_alpha * gradient + 
                                (1.0 - self.filter_alpha) * self.filtered_gradient;
        
        if self.filtered_gradient < 1.0 {
            // Linear region - normal gains
            self.kp = self.base_kp;
        } else if self.filtered_gradient > 50.0 {
            // High sensitivity - reduce gain
            self.kp = self.base_kp * 0.1;
        } else {
            // Scale smoothly
            let scale = 1.0 / (1.0 + (self.filtered_gradient - 1.0) * 0.02);
            self.kp = self.base_kp * scale;
        }
    }
    
    /// Compute damping factor
    pub fn compute_damping(&mut self, error: f64, dt: f64) -> f64 {
        let p = self.kp * error;
        self.integral += error * dt;
        let i = self.ki * self.integral;
        let d = self.kd * (error - self.last_error) / dt;
        self.last_error = error;
        
        (p + i + d).min(1.0)  // Limit to prevent instability
    }
    
    pub fn reset(&mut self) {
        self.integral = 0.0;
        self.last_error = 0.0;
        self.filtered_gradient = 1.0;
    }
}

/// Generic GLACIER solver configuration
#[derive(Debug, Clone)]
pub struct SolverConfig {
    /// Maximum iterations
    pub max_iterations: usize,
    /// Convergence tolerance
    pub tolerance: f64,
    /// Enable adaptive damping
    pub use_adaptive_damping: bool,
    /// Minimum damping factor
    pub min_damping: f64,
    /// Maximum damping factor  
    pub max_damping: f64,
    /// Enable diagonal perturbation for singular matrices
    pub singular_perturbation: f64,
    /// Damping factor for Newton steps
    pub damping_factor: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-9,
            use_adaptive_damping: true,
            min_damping: 1e-6,
            max_damping: 1.0,
            singular_perturbation: 1e-10,
            damping_factor: 0.7,
        }
    }
}

/// Generic GLACIER solver
pub struct GenericGlacierSolver {
    config: SolverConfig,
    adaptive_control: AdaptiveControl,
    ramp_value: f64,
}

impl GenericGlacierSolver {
    pub fn new(config: SolverConfig) -> Self {
        Self {
            config,
            adaptive_control: AdaptiveControl::new(),
            ramp_value: 1.0,
        }
    }
    
    /// Set ramp value for voltage sources (0.0 to 1.0)
    pub fn set_ramp(&mut self, ramp: f64) {
        self.ramp_value = ramp.clamp(0.0, 1.0);
    }
    
    /// Solve the nonlinear system
    pub fn solve(
        &mut self,
        variables: &mut [Variable],
        system: &dyn EquationSystem,
    ) -> Result<SolverStats> {
        info!("Starting GLACIER numerical solver");
        debug!("System has {} variables and {} equations", 
               variables.len(), system.num_equations());
        
        // Validate dimensions
        if variables.len() != system.num_variables() {
            return Err(SpiceError::InvalidModel(
                format!("Variable count mismatch: {} vs {}", 
                        variables.len(), system.num_variables())
            ));
        }
        
        let mut iteration = 0;
        let mut stats = SolverStats::new();
        
        while iteration < self.config.max_iterations {
            // Evaluate system at current point
            let residual = system.evaluate_residuals(variables);
            let mut jacobian = system.build_jacobian(variables);
            
            // Check convergence
            let error = residual.norm();
            stats.residual_history.push(error);
            
            if error < self.config.tolerance {
                info!("Converged in {} iterations with error {:.2e}", iteration, error);
                stats.iterations = iteration;
                stats.converged = true;
                stats.final_error = error;
                return Ok(stats);
            }
            
            // Check for singular matrix
            if jacobian.determinant().abs() < self.config.singular_perturbation {
                debug!("Nearly singular Jacobian, adding perturbation");
                for i in 0..jacobian.nrows() {
                    jacobian[(i, i)] += self.config.singular_perturbation;
                }
            }
            
            // Solve for Newton update
            let lu = jacobian.lu();
            let delta = lu.solve(&(-residual))
                .ok_or(SpiceError::SingularMatrix)?;
            
            // Compute damping
            let damping = if self.config.use_adaptive_damping {
                let gradient = self.estimate_gradient(variables, &delta);
                self.adaptive_control.adapt(gradient);
                self.adaptive_control.compute_damping(error, 1.0)
                    .clamp(self.config.min_damping, self.config.max_damping)
            } else {
                1.0  // Full Newton step
            };
            
            stats.damping_history.push(damping);
            
            // Update variables
            self.apply_update(variables, &delta, damping);
            
            iteration += 1;
            
            if iteration % 10 == 0 {
                debug!("Iteration {}: error = {:.2e}, damping = {:.3}", 
                       iteration, error, damping);
            }
        }
        
        // Did not converge
        stats.iterations = iteration;
        stats.converged = false;
        
        // Get final residual
        let final_residual = system.evaluate_residuals(variables);
        stats.final_error = final_residual.norm();
        
        Err(SpiceError::ConvergenceFailed(iteration))
    }
    
    /// Estimate gradient for adaptive control
    fn estimate_gradient(&self, variables: &[Variable], delta: &DVector<f64>) -> f64 {
        let mut max_gradient = 0.0;
        
        for (i, var) in variables.iter().enumerate() {
            let grad = match var.space {
                VariableSpace::Logarithmic => {
                    // For log variables, gradient represents exponential change
                    (delta[i].abs() * var.value.exp()).max(delta[i].abs())
                }
                VariableSpace::Linear => {
                    delta[i].abs()
                }
            };
            max_gradient = f64::max(max_gradient, grad);
        }
        
        max_gradient
    }
    
    /// Apply update to variables with damping
    fn apply_update(&self, variables: &mut [Variable], delta: &DVector<f64>, damping: f64) {
        for (i, var) in variables.iter_mut().enumerate() {
            var.value += damping * delta[i];
            
            // Ensure log variables don't go too negative
            if var.space == VariableSpace::Logarithmic && var.value < -50.0 {
                var.value = -50.0;  // Limit to ~1e-22 in linear space
            }
        }
    }
    
    /// Get initial guess based on variable spaces
    pub fn get_initial_guess(variables: &mut [Variable]) {
        for var in variables {
            var.value = match var.space {
                VariableSpace::Linear => 0.0,  // Default linear value
                VariableSpace::Logarithmic => -5.0,  // log(~0.007) - reasonable starting point
            };
        }
    }
}

/// Solver statistics
#[derive(Debug, Clone)]
pub struct SolverStats {
    /// Number of iterations
    pub iterations: usize,
    /// Whether convergence was achieved
    pub converged: bool,
    /// Final error norm
    pub final_error: f64,
    /// History of residual norms
    pub residual_history: Vec<f64>,
    /// History of damping factors
    pub damping_history: Vec<f64>,
}

impl SolverStats {
    fn new() -> Self {
        Self {
            iterations: 0,
            converged: false,
            final_error: 0.0,
            residual_history: Vec::new(),
            damping_history: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    /// Simple test system: x^2 - 4 = 0
    struct QuadraticSystem;
    
    impl EquationSystem for QuadraticSystem {
        fn evaluate_residuals(&self, variables: &[Variable]) -> DVector<f64> {
            let x = variables[0].value;
            DVector::from_vec(vec![x * x - 4.0])
        }
        
        fn build_jacobian(&self, variables: &[Variable]) -> DMatrix<f64> {
            let x = variables[0].value;
            DMatrix::from_vec(1, 1, vec![2.0 * x])
        }
        
        fn num_equations(&self) -> usize { 1 }
        fn num_variables(&self) -> usize { 1 }
    }
    
    #[test]
    fn test_simple_quadratic() {
        let mut solver = GenericGlacierSolver::new(SolverConfig::default());
        let mut variables = vec![
            Variable {
                id: 0,
                name: "x".to_string(),
                space: VariableSpace::Linear,
                value: 1.0,  // Initial guess
            }
        ];
        
        let system = QuadraticSystem;
        let result = solver.solve(&mut variables, &system).unwrap();
        
        assert!(result.converged);
        assert!((variables[0].value - 2.0).abs() < 1e-9);
    }
}