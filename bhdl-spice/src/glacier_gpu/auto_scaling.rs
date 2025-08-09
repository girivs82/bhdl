//! Automatic scaling for f32 GPU solver
//! 
//! Provides generic auto-scaling without any component knowledge

use bytemuck::{Pod, Zeroable};

/// Auto-scaling state for each variable
#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct VariableScale {
    pub scale_factor: f32,
    pub scale_exponent: i32,  // 10^scale_exponent
}

impl VariableScale {
    /// Create scale factor for a value
    pub fn from_value(x: f64) -> Self {
        if x.abs() < 1e-30 {
            // Very small or zero - use unit scale
            VariableScale {
                scale_factor: 1.0,
                scale_exponent: 0,
            }
        } else {
            // Compute scale as 10^floor(log10(|x|))
            let exponent = x.abs().log10().floor() as i32;
            VariableScale {
                scale_factor: 10_f32.powi(exponent),
                scale_exponent: exponent,
            }
        }
    }
    
    /// Normalize a value using this scale
    pub fn normalize(&self, x: f64) -> f32 {
        (x / self.scale_factor as f64) as f32
    }
    
    /// Denormalize a value
    pub fn denormalize(&self, x_norm: f32) -> f64 {
        x_norm as f64 * self.scale_factor as f64
    }
}

/// Auto-scaling tracker for the solver
pub struct AutoScaler {
    scales: Vec<VariableScale>,
    iteration: usize,
}

impl AutoScaler {
    pub fn new(num_vars: usize) -> Self {
        Self {
            scales: vec![VariableScale { scale_factor: 1.0, scale_exponent: 0 }; num_vars],
            iteration: 0,
        }
    }
    
    /// Update scales based on current variable values
    pub fn update_scales(&mut self, values: &[f64]) {
        // Only update scales every few iterations to maintain stability
        if self.iteration % 5 == 0 {
            for (i, &value) in values.iter().enumerate() {
                let new_scale = VariableScale::from_value(value);
                
                // Only change scale if it differs significantly
                if (new_scale.scale_exponent - self.scales[i].scale_exponent).abs() > 2 {
                    self.scales[i] = new_scale;
                }
            }
        }
        self.iteration += 1;
    }
    
    /// Normalize a vector of values
    pub fn normalize_vector(&self, values: &[f64]) -> Vec<f32> {
        values.iter()
            .zip(&self.scales)
            .map(|(&val, scale)| scale.normalize(val))
            .collect()
    }
    
    /// Denormalize a vector
    pub fn denormalize_vector(&self, normalized: &[f32]) -> Vec<f64> {
        normalized.iter()
            .zip(&self.scales)
            .map(|(&val, scale)| scale.denormalize(val))
            .collect()
    }
    
    /// Scale Jacobian for better conditioning
    pub fn scale_jacobian(&self, jacobian: &mut [f64], n: usize) {
        // Apply row and column scaling based on variable scales
        for i in 0..n {
            for j in 0..n {
                let idx = i * n + j;
                // J_scaled[i,j] = J[i,j] * scale[j] / scale[i]
                jacobian[idx] *= self.scales[j].scale_factor as f64 / self.scales[i].scale_factor as f64;
            }
        }
    }
    
    /// Get condition number estimate from scaled Jacobian
    pub fn estimate_condition(&self, jacobian: &[f32], n: usize) -> f32 {
        let mut max_elem = 0.0f32;
        let mut min_elem = f32::MAX;
        
        for i in 0..n {
            for j in 0..n {
                let elem = jacobian[i * n + j].abs();
                if elem > 1e-30 {
                    max_elem = max_elem.max(elem);
                    min_elem = min_elem.min(elem);
                }
            }
        }
        
        if min_elem > 0.0 {
            max_elem / min_elem
        } else {
            f32::INFINITY
        }
    }
}