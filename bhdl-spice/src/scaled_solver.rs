//! Numerically scaled solver that handles extreme value ranges

use nalgebra::{DMatrix, DVector};
use crate::{Result, SpiceError};
use std::collections::HashMap;

/// Automatic scaling detector and transformer for numerical stability
pub struct AutoScaler {
    /// Scaling factors for each variable
    scale_factors: Vec<f64>,
    
    /// Variable types for intelligent scaling
    variable_types: Vec<VariableType>,
    
    /// Threshold for detecting small values
    small_threshold: f64,
    
    /// Threshold for detecting large values
    large_threshold: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum VariableType {
    Voltage,      // Typically 0-10V range
    Current,      // Can vary from pA to A (1e-12 to 1)
    Resistance,   // Typically 1Ω to 1MΩ
    Unknown,      // Generic variable
}

impl AutoScaler {
    pub fn new(num_variables: usize) -> Self {
        Self {
            scale_factors: vec![1.0; num_variables],
            variable_types: vec![VariableType::Unknown; num_variables],
            small_threshold: 1e-9,
            large_threshold: 1e9,
        }
    }
    
    /// Analyze Jacobian and residual to determine optimal scaling
    pub fn compute_scaling(&mut self, jacobian: &DMatrix<f64>, residual: &DVector<f64>) {
        let n = jacobian.nrows();
        let m = jacobian.ncols();
        
        // Row scaling based on residual magnitudes
        let mut row_scales = vec![1.0; n];
        for i in 0..n {
            let res_mag = residual[i].abs();
            if res_mag > 0.0 {
                // Scale to bring residual to O(1)
                row_scales[i] = 1.0 / res_mag.max(1e-15);
            }
        }
        
        // Column scaling based on Jacobian column norms
        let mut col_scales = vec![1.0; m];
        for j in 0..m {
            let mut col_norm = 0.0;
            for i in 0..n {
                col_norm += (jacobian[(i, j)] * row_scales[i]).powi(2);
            }
            col_norm = col_norm.sqrt();
            
            if col_norm > 0.0 {
                // Scale to bring column norm to O(1)
                col_scales[j] = 1.0 / col_norm.max(1e-15);
            }
        }
        
        // Update scale factors
        for j in 0..m {
            self.scale_factors[j] = col_scales[j];
        }
        
        println!("\nAutoScaler: Computed scaling factors:");
        for (i, &scale) in self.scale_factors.iter().enumerate() {
            if (scale - 1.0).abs() > 1e-6 {
                println!("  Variable {}: scale = {:e}", i, scale);
            }
        }
    }
    
    /// Detect extreme values in solution vector
    pub fn detect_extreme_scaling(&mut self, x: &DVector<f64>) {
        println!("\nAutoScaler: Detecting extreme values...");
        
        for (i, &value) in x.iter().enumerate() {
            let abs_val = value.abs();
            
            if abs_val < self.small_threshold && abs_val > 0.0 {
                // Very small value - needs upscaling
                let suggested_scale = 1.0 / abs_val;
                println!("  Variable {}: {:e} (very small, suggest scale {:e})", i, value, suggested_scale);
                self.scale_factors[i] = suggested_scale;
            } else if abs_val > self.large_threshold {
                // Very large value - needs downscaling
                let suggested_scale = 1.0 / abs_val;
                println!("  Variable {}: {:e} (very large, suggest scale {:e})", i, value, suggested_scale);
                self.scale_factors[i] = suggested_scale;
            }
        }
    }
    
    /// Transform variables to scaled space
    pub fn scale_variables(&self, x: &DVector<f64>) -> DVector<f64> {
        let mut x_scaled = x.clone();
        for i in 0..x.len() {
            x_scaled[i] *= self.scale_factors[i];
        }
        x_scaled
    }
    
    /// Transform back from scaled space
    pub fn unscale_variables(&self, x_scaled: &DVector<f64>) -> DVector<f64> {
        let mut x = x_scaled.clone();
        for i in 0..x_scaled.len() {
            x[i] /= self.scale_factors[i];
        }
        x
    }
    
    /// Scale Jacobian matrix: J_scaled = D_r * J * D_c^(-1)
    /// where D_r is row scaling and D_c is column scaling
    pub fn scale_jacobian(&self, j: &DMatrix<f64>) -> DMatrix<f64> {
        let mut j_scaled = j.clone();
        let (n, m) = (j.nrows(), j.ncols());
        
        // Apply column scaling (divide by scale factors)
        for i in 0..n {
            for j in 0..m {
                j_scaled[(i, j)] /= self.scale_factors[j];
            }
        }
        
        j_scaled
    }
    
    /// Scale residual vector
    pub fn scale_residual(&self, r: &DVector<f64>) -> DVector<f64> {
        // For now, just return as-is
        // Could implement row scaling if needed
        r.clone()
    }
}

/// Solver wrapper that automatically handles scaling
pub struct ScaledSolver<S> {
    inner_solver: S,
    scaler: AutoScaler,
    use_auto_scaling: bool,
    iteration: usize,
}

impl<S> ScaledSolver<S> {
    pub fn new(solver: S, num_variables: usize) -> Self {
        Self {
            inner_solver: solver,
            scaler: AutoScaler::new(num_variables),
            use_auto_scaling: true,
            iteration: 0,
        }
    }
    
    /// Solve with automatic scaling
    pub fn solve_scaled(
        &mut self,
        mut x: DVector<f64>,
        compute_residual: impl Fn(&DVector<f64>) -> DVector<f64>,
        compute_jacobian: impl Fn(&DVector<f64>) -> DMatrix<f64>,
        max_iterations: usize,
        tolerance: f64,
    ) -> Result<DVector<f64>> {
        println!("\nScaledSolver: Starting scaled Newton-Raphson");
        
        // Initial scaling detection
        self.scaler.detect_extreme_scaling(&x);
        
        for iter in 0..max_iterations {
            self.iteration = iter;
            
            // Transform to scaled space
            let x_scaled = self.scaler.scale_variables(&x);
            
            // Compute residual and Jacobian in original space
            let residual = compute_residual(&x);
            let jacobian = compute_jacobian(&x);
            
            // Check convergence
            let error = residual.norm();
            if error < tolerance {
                println!("Converged in {} iterations", iter);
                return Ok(x);
            }
            
            // Auto-scaling: recompute scale factors based on current Jacobian
            if self.use_auto_scaling && iter % 10 == 0 {
                self.scaler.compute_scaling(&jacobian, &residual);
            }
            
            // Scale the system
            let j_scaled = self.scaler.scale_jacobian(&jacobian);
            let r_scaled = self.scaler.scale_residual(&residual);
            
            // Solve in scaled space
            let delta_scaled = match j_scaled.lu().solve(&(-r_scaled)) {
                Some(d) => d,
                None => {
                    println!("Singular matrix even after scaling!");
                    return Err(SpiceError::SingularMatrix);
                }
            };
            
            // Transform back to original space
            let delta = self.scaler.unscale_variables(&delta_scaled);
            
            // Adaptive damping for large steps
            let step_size = delta.norm();
            let damping = if step_size > 1.0 {
                0.5 / step_size.sqrt()
            } else {
                1.0
            };
            
            // Update solution
            x += damping * delta;
            
            if iter % 10 == 0 {
                println!("  Iter {}: error = {:e}, step = {:e}, damping = {:.3}", 
                         iter, error, step_size, damping);
            }
        }
        
        Err(SpiceError::ConvergenceFailed(max_iterations))
    }
}

/// Alternative: Logarithmic transformation for extreme values
pub struct LogTransformSolver {
    /// Indices of variables to log-transform
    log_indices: Vec<usize>,
    
    /// Threshold below which to use log transform
    log_threshold: f64,
}

impl LogTransformSolver {
    pub fn new() -> Self {
        Self {
            log_indices: Vec::new(),
            log_threshold: 1e-6,
        }
    }
    
    /// Detect which variables should be log-transformed
    pub fn detect_log_variables(&mut self, x: &DVector<f64>, variable_types: &[VariableType]) {
        self.log_indices.clear();
        
        for (i, (&value, &var_type)) in x.iter().zip(variable_types.iter()).enumerate() {
            match var_type {
                VariableType::Current => {
                    // Currents often benefit from log transform
                    if value.abs() < self.log_threshold && value.abs() > 0.0 {
                        println!("LogTransform: Variable {} (current = {:e}) will use log space", i, value);
                        self.log_indices.push(i);
                    }
                }
                _ => {} // Other types use linear space
            }
        }
    }
    
    /// Transform to log space for selected variables
    pub fn transform(&self, x: &DVector<f64>) -> DVector<f64> {
        let mut x_transformed = x.clone();
        for &i in &self.log_indices {
            if x[i] > 0.0 {
                x_transformed[i] = x[i].ln();
            } else {
                x_transformed[i] = -50.0; // ln(~1e-22)
            }
        }
        x_transformed
    }
    
    /// Transform back from log space
    pub fn inverse_transform(&self, x_log: &DVector<f64>) -> DVector<f64> {
        let mut x = x_log.clone();
        for &i in &self.log_indices {
            x[i] = x_log[i].exp();
        }
        x
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extreme_scaling_detection() {
        let mut scaler = AutoScaler::new(3);
        let x = DVector::from_vec(vec![1.0, 1e-15, 1e12]);
        
        scaler.detect_extreme_scaling(&x);
        
        // Should detect variable 1 as very small, variable 2 as very large
        assert!(scaler.scale_factors[1] > 1e10);
        assert!(scaler.scale_factors[2] < 1e-10);
    }
}