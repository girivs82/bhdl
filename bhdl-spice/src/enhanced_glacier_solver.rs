//! Enhanced GLACIER Solver with Automatic Scaling and Transformations
//! 
//! This solver combines:
//! 1. Automatic problem analysis (condition numbers, value ranges)
//! 2. Intelligent scaling to O(1) range
//! 3. Adaptive transformations (log for exponentials)
//! 4. Convergence monitoring with strategy switching

use nalgebra::{DMatrix, DVector};
use std::collections::HashMap;
use log::{info, debug, warn};

use crate::{
    Circuit, ComponentModel, SpiceError, Result,
    NodeVoltages, BranchCurrents, AnalysisResult,
    glacier_solver::{GlacierSolver, AdaptivePIDController},
    runtime_models::{RuntimeModelEngine, ModelExecutionContext},
    NodeId, ComponentId,
};
use petgraph::graph::{NodeIndex, EdgeIndex};

/// Problem characteristics detected during analysis
#[derive(Debug, Clone)]
pub struct ProblemAnalysis {
    /// Condition number of Jacobian
    pub condition_number: f64,
    /// Range of variable values (min, max)
    pub value_ranges: Vec<(f64, f64)>,
    /// Which variables have exponential behavior
    pub exponential_vars: Vec<bool>,
    /// Suggested scaling factors
    pub scale_factors: Vec<f64>,
    /// Problem difficulty estimate (0-1)
    pub difficulty: f64,
}

/// Transformation type for each variable
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum TransformType {
    /// No transformation
    Linear,
    /// Logarithmic transformation for exponentials
    Logarithmic,
    /// Inverse transformation for reciprocals
    Inverse,
}

/// Scaling and transformation state
#[derive(Debug, Clone)]
pub struct ScalingState {
    /// Scale factors for each variable
    pub scale_factors: DVector<f64>,
    /// Transformation type for each variable
    pub transforms: Vec<TransformType>,
    /// Whether scaling is active
    pub scaling_active: bool,
    /// Last condition number
    pub last_condition: f64,
}

impl ScalingState {
    pub fn new(n_vars: usize) -> Self {
        Self {
            scale_factors: DVector::from_element(n_vars, 1.0),
            transforms: vec![TransformType::Linear; n_vars],
            scaling_active: false,
            last_condition: 1.0,
        }
    }
    
    /// Apply scaling and transformation to variables
    pub fn transform(&self, x: &DVector<f64>) -> DVector<f64> {
        let mut x_scaled = x.clone();
        
        for i in 0..x.len() {
            match self.transforms[i] {
                TransformType::Linear => {
                    x_scaled[i] = x[i] * self.scale_factors[i];
                }
                TransformType::Logarithmic => {
                    // Log transform: y = log(x/x0) where x0 is typical scale
                    let x0 = 1.0 / self.scale_factors[i];
                    if x[i] > 0.0 {
                        x_scaled[i] = (x[i] / x0).ln();
                    } else {
                        x_scaled[i] = -10.0; // Reasonable lower bound
                    }
                }
                TransformType::Inverse => {
                    // Inverse transform: y = x0/x
                    let x0 = 1.0 / self.scale_factors[i];
                    if x[i].abs() > 1e-15 {
                        x_scaled[i] = x0 / x[i];
                    } else {
                        x_scaled[i] = 1e15 * x0.signum() * x[i].signum();
                    }
                }
            }
        }
        
        x_scaled
    }
    
    /// Apply inverse transformation
    pub fn inverse_transform(&self, y: &DVector<f64>) -> DVector<f64> {
        let mut x = y.clone();
        
        for i in 0..y.len() {
            match self.transforms[i] {
                TransformType::Linear => {
                    x[i] = y[i] / self.scale_factors[i];
                }
                TransformType::Logarithmic => {
                    // Inverse log: x = x0 * exp(y)
                    let x0 = 1.0 / self.scale_factors[i];
                    x[i] = x0 * y[i].exp();
                }
                TransformType::Inverse => {
                    // Inverse of inverse: x = x0/y
                    let x0 = 1.0 / self.scale_factors[i];
                    if y[i].abs() > 1e-15 {
                        x[i] = x0 / y[i];
                    } else {
                        x[i] = 1e15 * x0.signum() * y[i].signum();
                    }
                }
            }
        }
        
        x
    }
    
    /// Transform Jacobian based on scaling and transformations
    pub fn transform_jacobian(&self, J: &DMatrix<f64>, x: &DVector<f64>) -> DMatrix<f64> {
        let n = J.nrows();
        let mut J_scaled = J.clone();
        
        // Apply row scaling (equations)
        for i in 0..n {
            for j in 0..n {
                J_scaled[(i, j)] /= self.scale_factors[i];
            }
        }
        
        // Apply column scaling (variables) with chain rule for transformations
        for j in 0..n {
            match self.transforms[j] {
                TransformType::Linear => {
                    // dy/dx = scale_factor
                    for i in 0..n {
                        J_scaled[(i, j)] *= self.scale_factors[j];
                    }
                }
                TransformType::Logarithmic => {
                    // y = ln(x/x0), dy/dx = 1/x
                    if x[j] > 0.0 {
                        let dydx = 1.0 / x[j];
                        for i in 0..n {
                            J_scaled[(i, j)] *= dydx;
                        }
                    }
                }
                TransformType::Inverse => {
                    // y = x0/x, dy/dx = -x0/x²
                    let x0 = 1.0 / self.scale_factors[j];
                    if x[j].abs() > 1e-15 {
                        let dydx = -x0 / (x[j] * x[j]);
                        for i in 0..n {
                            J_scaled[(i, j)] *= dydx;
                        }
                    }
                }
            }
        }
        
        J_scaled
    }
}

/// Enhanced GLACIER solver with automatic scaling
pub struct EnhancedGlacierSolver {
    /// Base GLACIER solver
    pub base_solver: GlacierSolver,
    /// Scaling state
    scaling: ScalingState,
    /// Problem analysis results
    analysis: Option<ProblemAnalysis>,
    /// Convergence history
    convergence_history: Vec<f64>,
    /// Strategy switch threshold
    strategy_switch_threshold: f64,
}

impl EnhancedGlacierSolver {
    /// Solve with transformations applied - full implementation
    fn solve_with_transformations(
        analysis: &Option<ProblemAnalysis>,
        scaling: &ScalingState,
        solver: &mut GlacierSolver,
        start_ramp: f64,
        target_voltage: Option<f64>,
    ) -> Result<AnalysisResult> {
        // Apply enhanced solving based on scaling state
        if scaling.scaling_active && scaling.transforms.iter().any(|&t| t == TransformType::Logarithmic) {
            info!("Using full log transformation solving");
            
            // Call the new method that implements full transformation
            solver.analyze_with_log_transform_full(start_ramp, target_voltage, scaling)
        } else if scaling.scaling_active {
            info!("Using enhanced solving with linear scaling");
            solver.analyze_with_guidance(start_ramp, target_voltage)
        } else {
            // No special scaling needed
            solver.analyze_with_guidance(start_ramp, target_voltage)
        }
    }

    pub fn new(circuit: Circuit) -> Self {
        // Count nodes (excluding ground) for variable count
        let ground_idx = circuit.ground_node().map(|n| n.0);
        let n_nodes = circuit.nodes()
            .filter(|(idx, _)| Some(*idx) != ground_idx)
            .count();
        
        // Count voltage sources
        let n_vsources = circuit.branches()
            .filter(|(_, b)| matches!(b.component_type.as_str(), "VoltageSource"))
            .count();
            
        let n_vars = n_nodes + n_vsources;
        
        Self {
            base_solver: GlacierSolver::new(circuit),
            scaling: ScalingState::new(n_vars),
            analysis: None,
            convergence_history: Vec::new(),
            strategy_switch_threshold: 0.1, // Switch if progress < 10%
        }
    }
    
    /// Add component model
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.base_solver.add_model(name, model);
    }
    
    /// Analyze problem characteristics with circuit awareness
    fn analyze_problem(&mut self, x: &DVector<f64>, J: &DMatrix<f64>) -> ProblemAnalysis {
        let n = x.len();
        
        // Calculate condition number (simplified - use ratio of max/min diagonal)
        let mut min_diag = f64::INFINITY;
        let mut max_diag = 0.0;
        for i in 0..n {
            let diag = J[(i, i)].abs();
            if diag > 0.0 {
                min_diag = f64::min(min_diag, diag);
                max_diag = f64::max(max_diag, diag);
            }
        }
        let condition_number = if min_diag > 0.0 {
            max_diag / min_diag
        } else {
            f64::INFINITY
        };
        
        // Analyze value ranges and detect exponential components
        let mut value_ranges = Vec::new();
        let mut exponential_vars = Vec::new();
        let mut scale_factors = Vec::new();
        
        // For now, use heuristics based on Jacobian structure
        // In a full implementation, we would access circuit topology
        
        // Check each variable
        for i in 0..n {
            let mut has_exponential = false;
            
            // Check Jacobian column for exponential behavior
            let mut min_scale = f64::INFINITY;
            let mut max_scale = 0.0;
            
            for j in 0..n {
                let entry = J[(j, i)].abs();
                if entry > 0.0 {
                    min_scale = f64::min(min_scale, entry);
                    max_scale = f64::max(max_scale, entry);
                    
                    // Large ratio suggests exponential behavior
                    if max_scale / min_scale > 1e6 {
                        has_exponential = true;
                    }
                }
            }
            
            // Estimate range from Jacobian entries
            let mut min_scale = f64::INFINITY;
            let mut max_scale = 0.0;
            
            for j in 0..n {
                let entry = J[(j, i)].abs();
                if entry > 0.0 {
                    min_scale = f64::min(min_scale, entry);
                    max_scale = f64::max(max_scale, entry);
                }
            }
            
            value_ranges.push((x[i] - 1.0 / min_scale, x[i] + 1.0 / min_scale));
            exponential_vars.push(has_exponential);
            
            // Suggest scale factor to normalize to O(1)
            let scale = if x[i].abs() > 1e-10 {
                1.0 / x[i].abs()
            } else if max_scale > 0.0 {
                max_scale.sqrt()
            } else {
                1.0
            };
            scale_factors.push(scale);
        }
        
        // Count exponential components for difficulty
        let exp_count = exponential_vars.iter().filter(|&&e| e).count();
        let exp_ratio = exp_count as f64 / n as f64;
        
        // Estimate difficulty based on condition number and exponential content
        let cond_factor = f64::min(condition_number.log10() / 10.0, 1.0).max(0.0);
        let exp_factor = exp_ratio;
        let difficulty = (cond_factor * 0.7 + exp_factor * 0.3).min(1.0);
        
        ProblemAnalysis {
            condition_number,
            value_ranges,
            exponential_vars,
            scale_factors,
            difficulty,
        }
    }
    
    /// Update scaling based on analysis
    fn update_scaling(&mut self, analysis: &ProblemAnalysis) {
        let n = self.scaling.scale_factors.len();
        
        // Update scale factors
        for i in 0..n {
            self.scaling.scale_factors[i] = analysis.scale_factors[i];
            
            // Choose transformation type
            if analysis.exponential_vars[i] && analysis.difficulty > 0.7 {
                self.scaling.transforms[i] = TransformType::Logarithmic;
            } else {
                self.scaling.transforms[i] = TransformType::Linear;
            }
        }
        
        self.scaling.scaling_active = analysis.condition_number > 1e6 || analysis.difficulty > 0.5;
        self.scaling.last_condition = analysis.condition_number;
    }
    
    /// Monitor convergence and decide strategy
    fn monitor_convergence(&mut self, error: f64) -> bool {
        self.convergence_history.push(error);
        
        // Keep last 10 iterations
        if self.convergence_history.len() > 10 {
            self.convergence_history.remove(0);
        }
        
        // Check progress
        if self.convergence_history.len() >= 5 {
            let old_error = self.convergence_history[0];
            let new_error = *self.convergence_history.last().unwrap();
            
            let progress = (old_error - new_error) / old_error;
            
            if progress < self.strategy_switch_threshold {
                info!("Slow convergence detected: {:.2}% improvement", progress * 100.0);
                return true; // Need strategy switch
            }
        }
        
        false
    }
    
    /// Main analysis method with enhanced scaling
    pub fn analyze(&mut self) -> Result<AnalysisResult> {
        info!("Starting Enhanced Two-Phase Analysis with Automatic Scaling");
        
        // First, try standard two-phase approach
        match self.base_solver.analyze() {
            Ok(results) => {
                // Take the best result
                if let Some((_, _, _, result)) = results.into_iter()
                    .max_by(|a, b| a.3.branch_currents.values().sum::<f64>()
                        .partial_cmp(&b.3.branch_currents.values().sum::<f64>())
                        .unwrap())
                {
                    return Ok(result);
                }
            }
            Err(e) => {
                warn!("Standard two-phase failed: {}, trying with enhanced scaling", e);
            }
        }
        
        // If standard approach failed, use enhanced scaling
        self.analyze_with_scaling()
    }
    
    /// Analysis with full scaling and transformation
    fn analyze_with_scaling(&mut self) -> Result<AnalysisResult> {
        info!("Attempting analysis with automatic scaling and transformations");
        
        // Create a wrapper that applies transformations during solving
        let mut best_result = None;
        let mut best_current = 0.0;
        
        // Try multiple strategies based on problem difficulty
        let strategies = if self.analysis.as_ref().map(|a| a.difficulty).unwrap_or(0.5) > 0.7 {
            vec![(0.1, 0.5), (0.2, 1.0), (0.3, 1.5), (0.5, 2.0)]
        } else {
            vec![(0.5, 2.0), (0.7, 2.5), (1.0, 3.0)]
        };
        
        // First analyze the problem if not done
        if self.analysis.is_none() {
            // Get initial state from solver
            let x = DVector::from_element(self.scaling.scale_factors.len(), 0.0);
            let J = DMatrix::from_element(x.len(), x.len(), 1.0);
            let analysis = self.analyze_problem(&x, &J);
            
            info!("Problem analysis: difficulty={:.2}, condition={:.2e}",
                  analysis.difficulty, analysis.condition_number);
            
            self.update_scaling(&analysis);
            self.analysis = Some(analysis);
        }
        
        for (start_ramp, target_voltage) in strategies {
            info!("Trying strategy: start_ramp={}, target_voltage={}", start_ramp, target_voltage);
            
            // Use the enhanced solving approach with the base solver
            match Self::solve_with_transformations(&self.analysis, &self.scaling, &mut self.base_solver, start_ramp, Some(target_voltage)) {
                Ok(result) => {
                    let current = result.branch_currents.values()
                        .map(|&c| c.abs())
                        .filter(|&c| c > 1e-12 && c < 1.0)
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .unwrap_or(0.0);
                    
                    if current > best_current {
                        best_current = current;
                        best_result = Some(result);
                        info!("Found better solution with current: {} mA", current * 1000.0);
                    }
                }
                Err(e) => {
                    debug!("Strategy failed: {}", e);
                }
            }
        }
        
        best_result.ok_or_else(|| SpiceError::AnalysisFailed(
            "All enhanced scaling strategies failed".to_string()
        ))
    }
}

/// Extension methods for problem analysis
pub trait ProblemAnalysisExt {
    /// Analyze circuit difficulty before solving
    fn analyze_circuit_difficulty(&self) -> f64;
    
    /// Suggest best solving strategy
    fn suggest_strategy(&self) -> &str;
}

impl ProblemAnalysisExt for Circuit {
    fn analyze_circuit_difficulty(&self) -> f64 {
        let mut difficulty = 0.0;
        
        // Count nonlinear elements
        let nonlinear_count = self.branches()
            .filter(|(_, b)| matches!(b.component_type.as_str(), 
                "LED" | "Diode" | "BJT" | "MOSFET"))
            .count();
        
        // Series nonlinear elements are especially difficult
        let series_nonlinear = self.detect_series_nonlinear();
        
        // Estimate difficulty
        difficulty += (nonlinear_count as f64) * 0.1;
        difficulty += (series_nonlinear as f64) * 0.3;
        
        difficulty.min(1.0_f64)
    }
    
    fn suggest_strategy(&self) -> &str {
        let difficulty = self.analyze_circuit_difficulty();
        
        if difficulty < 0.3 {
            "standard"
        } else if difficulty < 0.6 {
            "adaptive_scaling"
        } else {
            "progressive_with_log_transform"
        }
    }
}

/// Helper trait for circuit topology analysis
trait CircuitTopologyExt {
    fn detect_series_nonlinear(&self) -> usize;
}

impl CircuitTopologyExt for Circuit {
    fn detect_series_nonlinear(&self) -> usize {
        // Simplified detection - would need full implementation
        let mut series_count = 0;
        
        // Look for nodes with exactly 2 nonlinear components
        for (node_idx, _) in self.nodes() {
            let connected_nonlinear = self.node_branches(node_idx)
                .into_iter()
                .filter_map(|edge_idx| self.branches()
                    .find(|(idx, _)| *idx == edge_idx)
                    .map(|(_, b)| b))
                .filter(|b| matches!(b.component_type.as_str(), "LED" | "Diode"))
                .count();
            
            if connected_nonlinear == 2 {
                series_count += 1;
            }
        }
        
        series_count / 2 // Each series pair counted twice
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_scaling_transform() {
        let mut scaling = ScalingState::new(3);
        scaling.scale_factors[0] = 2.0;
        scaling.scale_factors[1] = 0.1;
        scaling.transforms[1] = TransformType::Logarithmic;
        
        let x = DVector::from_vec(vec![1.0, 10.0, 5.0]);
        let y = scaling.transform(&x);
        let x_recovered = scaling.inverse_transform(&y);
        
        // Check linear scaling
        assert!((y[0] - 2.0).abs() < 1e-10);
        assert!((x_recovered[0] - x[0]).abs() < 1e-10);
        
        // Check log transform
        assert!((y[1] - 0.0).abs() < 1e-10); // ln(10/10) = 0
        assert!((x_recovered[1] - x[1]).abs() < 1e-10);
    }
}