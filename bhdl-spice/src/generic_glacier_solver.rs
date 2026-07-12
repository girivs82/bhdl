//! Generic GLACIER Solver - Pure Numerical Analysis
//! 
//! This solver has NO knowledge of circuits, components, or electrical concepts.
//! It only knows about:
//! - Variables (can be in linear or log space)
//! - Equations (residuals)
//! - Jacobians
//! - Numerical convergence

use nalgebra::{DMatrix, DVector};
use log::{info, debug, warn};
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
            use_adaptive_damping: false, // Full Newton steps; ramping provides robustness
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
            
            // Row + column equilibration scaling for numerical conditioning.
            // Circuit Jacobians mix entries spanning many orders of magnitude
            // (e.g. inductor 1e6 S vs capacitor 1e-12 S).  Without scaling the
            // LU factorization loses all precision.  This is standard practice
            // in production SPICE solvers.
            let n = jacobian.nrows();

            // Row scaling: divide each row so max |entry| = 1
            let mut row_scale: Vec<f64> = Vec::with_capacity(n);
            for i in 0..n {
                let max_val = (0..n)
                    .map(|j| jacobian[(i, j)].abs())
                    .fold(0.0_f64, f64::max);
                let s = if max_val > 1e-30 { 1.0 / max_val } else { 1.0 };
                row_scale.push(s);
                for j in 0..n {
                    jacobian[(i, j)] *= s;
                }
            }

            // Column scaling: divide each column so max |entry| = 1
            let mut col_scale: Vec<f64> = Vec::with_capacity(n);
            for j in 0..n {
                let max_val = (0..n)
                    .map(|i| jacobian[(i, j)].abs())
                    .fold(0.0_f64, f64::max);
                let s = if max_val > 1e-30 { 1.0 / max_val } else { 1.0 };
                col_scale.push(s);
                for i in 0..n {
                    jacobian[(i, j)] *= s;
                }
            }

            // Scale the right-hand side with row scaling
            let scaled_rhs = DVector::from_fn(n, |i, _| -residual[i] * row_scale[i]);

            // Add small diagonal perturbation for near-singular systems
            for i in 0..n {
                if jacobian[(i, i)].abs() < self.config.singular_perturbation {
                    jacobian[(i, i)] += self.config.singular_perturbation;
                }
            }

            // Solve the scaled system
            let lu = jacobian.lu();
            let scaled_delta = lu.solve(&scaled_rhs)
                .ok_or(SpiceError::SingularMatrix)?;

            // Unscale the solution (column scaling affects the solution)
            let delta = DVector::from_fn(n, |j, _| scaled_delta[j] * col_scale[j]);
            
            // Backtracking line search with steepest-descent fallback.
            //
            // Try the full Newton step first; if the residual increases,
            // halve the step.  If Newton direction is completely wrong
            // (all step sizes increase error), fall back to a steepest-
            // descent step using the negative gradient direction.
            let saved: Vec<f64> = variables.iter().map(|v| v.value).collect();
            let mut alpha = 1.0_f64;
            let min_alpha = 1.0 / 128.0; // 7 halvings
            let mut used_gradient_fallback = false;

            loop {
                for (i, var) in variables.iter_mut().enumerate() {
                    var.value = saved[i] + alpha * delta[i];
                    if var.space == VariableSpace::Logarithmic && var.value < -700.0 {
                        var.value = -700.0;
                    }
                }

                let new_residual = system.evaluate_residuals(variables);
                let new_error = new_residual.norm();

                if new_error < error {
                    break;
                }

                if alpha <= min_alpha {
                    // Newton direction is bad.  Try steepest-descent
                    // (negative gradient of 0.5*||r||^2 = J^T * r).
                    // Recompute with the original (unscaled) Jacobian.
                    let jac_orig = system.build_jacobian(variables);
                    let grad = jac_orig.transpose() * &residual;
                    let grad_norm = grad.norm();

                    if grad_norm > 1e-30 {
                        // Steepest-descent direction, normalized
                        let sd_dir = -&grad / grad_norm;

                        // Try a few step sizes along the gradient
                        let mut best_err = error;
                        let mut best_alpha_sd = 0.0;
                        for k in 0..10 {
                            let a = 0.1 / (1 << k) as f64; // 0.1, 0.05, 0.025, ...
                            for (i, var) in variables.iter_mut().enumerate() {
                                var.value = saved[i] + a * sd_dir[i];
                                if var.space == VariableSpace::Logarithmic && var.value < -700.0 {
                                    var.value = -700.0;
                                }
                            }
                            let sd_err = system.evaluate_residuals(variables).norm();
                            if sd_err < best_err {
                                best_err = sd_err;
                                best_alpha_sd = a;
                            }
                        }

                        if best_alpha_sd > 0.0 {
                            // Apply the best gradient step
                            for (i, var) in variables.iter_mut().enumerate() {
                                var.value = saved[i] + best_alpha_sd * sd_dir[i];
                                if var.space == VariableSpace::Logarithmic && var.value < -700.0 {
                                    var.value = -700.0;
                                }
                            }
                            used_gradient_fallback = true;
                            alpha = best_alpha_sd;
                            break;
                        }
                    }

                    // Even gradient didn't help — accept min-alpha Newton
                    for (i, var) in variables.iter_mut().enumerate() {
                        var.value = saved[i] + min_alpha * delta[i];
                        if var.space == VariableSpace::Logarithmic && var.value < -700.0 {
                            var.value = -700.0;
                        }
                    }
                    break;
                }

                alpha *= 0.5;
            }

            stats.damping_history.push(alpha);

            iteration += 1;

            if iteration % 10 == 0 {
                debug!("Iteration {}: error = {:.2e}, alpha = {:.3}{}",
                       iteration, error, alpha,
                       if used_gradient_fallback { " (gradient)" } else { "" });
            }
        }
        
        // Did not converge
        stats.iterations = iteration;
        stats.converged = false;
        
        // Get final residual
        let final_residual = system.evaluate_residuals(variables);
        stats.final_error = final_residual.norm();

        // Name the worst offenders — "convergence failed" without saying
        // WHERE costs a bisection session every time it fires.
        let mut worst: Vec<(usize, f64)> = final_residual
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.abs()))
            .collect();
        worst.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        for (i, r) in worst.iter().take(3) {
            let name = variables
                .get(*i)
                .map(|v| v.name.clone())
                .unwrap_or_else(|| format!("var#{i}"));
            let val = variables.get(*i).map(|v| v.value).unwrap_or(f64::NAN);
            warn!(
                "  non-converged: |residual| {:.3e} at '{}' (value {:.4})",
                r, name, val
            );
        }

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

            // Clamp log variables to prevent IEEE 754 issues
            // -700 corresponds to exp(-700) ≈ 0 (below f64 denormal range)
            // This is generous enough for deeply reverse-biased diodes
            if var.space == VariableSpace::Logarithmic && var.value < -700.0 {
                var.value = -700.0;
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