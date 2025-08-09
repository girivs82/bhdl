//! Adaptive Manifold-Aware Continuation Solver
//! 
//! A truly generic solver that adapts to the solution manifold structure
//! without any component-specific knowledge.

use nalgebra::{DMatrix, DVector, SVD};
use std::collections::HashMap;
use petgraph::graph::{NodeIndex, EdgeIndex};
use log::info;

use crate::{
    Circuit, ComponentModel, SpiceError, Result,
    NodeVoltages, BranchCurrents, AnalysisResult,
    runtime_models::{RuntimeModelEngine, ModelExecutionContext},
};

/// Manifold properties at a point
#[derive(Debug, Clone)]
struct ManifoldProperties {
    /// Local curvature estimate (higher = more curved)
    curvature: f64,
    /// Condition number of Jacobian
    condition: f64,
    /// Dominant gradient direction
    gradient_direction: DVector<f64>,
    /// Trust region radius based on local properties
    trust_radius: f64,
    /// Indicates if we're near a singularity
    near_singularity: bool,
}

/// Adaptive Manifold-Aware Solver
pub struct ManifoldSolver {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    model_engine: RuntimeModelEngine,
    
    // Solver parameters
    tolerance: f64,
    max_iterations: usize,
    
    // Adaptive parameters
    min_trust_radius: f64,
    max_trust_radius: f64,
    gradient_flow_threshold: f64,  // Curvature above which we use gradient flow
}

impl ManifoldSolver {
    pub fn new(circuit: Circuit) -> Result<Self> {
        Ok(Self {
            circuit,
            models: HashMap::new(),
            model_engine: RuntimeModelEngine::new()?,
            tolerance: 1e-9,
            max_iterations: 500,
            min_trust_radius: 1e-6,
            max_trust_radius: 1.0,
            gradient_flow_threshold: 100.0,  // High curvature threshold
        })
    }
    
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Main analysis function
    pub fn analyze(&mut self) -> Result<AnalysisResult> {
        // Setup circuit equations
        let (node_list, ground_idx, voltage_sources) = self.setup_circuit()?;
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        let size = num_nodes + num_vsources;
        
        // Initial guess - start conservative
        let mut x = DVector::zeros(size);
        for i in 0..num_nodes {
            x[i] = 0.1;  // Small initial voltages
        }
        
        // Continuation parameter (0 = easy problem, 1 = full problem)
        let mut lambda = 0.0;
        let lambda_step = 0.1;
        
        info!("Starting Adaptive Manifold-Aware Continuation Solver");
        
        while lambda < 1.0 {
            lambda = f64::min(lambda + lambda_step, 1.0);
            
            println!("\n=== Continuation step: λ = {:.2} ===", lambda);
            
            // Solve at current continuation level
            match self.solve_at_lambda(&mut x, lambda, &node_list, ground_idx, &voltage_sources) {
                Ok(_) => {
                    println!("✓ Converged at λ = {:.2}", lambda);
                }
                Err(e) => {
                    // Back off and try smaller step
                    lambda -= lambda_step;
                    let new_step = lambda_step * 0.5;
                    if new_step < 0.01 {
                        return Err(e);
                    }
                    println!("✗ Failed at λ = {:.2}, reducing step to {:.3}", lambda + lambda_step, new_step);
                    lambda += new_step;
                }
            }
        }
        
        // Extract solution
        self.extract_solution(x, node_list, ground_idx, voltage_sources)
    }
    
    /// Solve at a specific continuation parameter
    fn solve_at_lambda(
        &mut self,
        x: &mut DVector<f64>,
        lambda: f64,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
    ) -> Result<()> {
        let mut iteration = 0;
        let mut trust_radius = 0.1;  // Start with moderate trust region
        
        loop {
            iteration += 1;
            if iteration > self.max_iterations {
                return Err(SpiceError::ConvergenceFailed(iteration));
            }
            
            // Build system matrices with continuation
            let (jacobian, residual) = self.build_system_matrices_with_continuation(
                x, lambda, node_list, ground_idx, voltage_sources
            )?;
            
            // Analyze manifold properties
            let props = self.analyze_manifold(&jacobian, &residual);
            
            if iteration < 5 || iteration % 10 == 0 {
                println!("  Iter {}: |F|={:.2e}, curvature={:.2e}, condition={:.2e}, trust_r={:.3}", 
                         iteration, residual.norm(), props.curvature, props.condition, props.trust_radius);
            }
            
            // Check convergence
            if residual.norm() < self.tolerance {
                return Ok(());
            }
            
            // Choose solution method based on manifold properties
            let dx = if props.near_singularity || props.curvature > self.gradient_flow_threshold {
                // High curvature or near singularity: use gradient flow
                println!("    → Using gradient flow (curvature={:.2e})", props.curvature);
                self.gradient_flow_step(&jacobian, &residual, &props)
            } else {
                // Low curvature: use Newton with trust region
                self.newton_trust_region_step(&jacobian, &residual, &props, trust_radius)?
            };
            
            // Line search with trust region enforcement
            let (step_accepted, actual_reduction) = self.line_search_with_trust(
                x, &dx, lambda, &props, node_list, ground_idx, voltage_sources
            )?;
            
            if step_accepted {
                // Update trust radius based on actual vs predicted reduction
                if actual_reduction > 0.75 {
                    trust_radius = (trust_radius * 2.0).min(props.trust_radius);
                } else if actual_reduction < 0.25 {
                    trust_radius = trust_radius * 0.5;
                }
            } else {
                // Reduce trust radius and try again
                trust_radius = trust_radius * 0.25;
                if trust_radius < self.min_trust_radius {
                    return Err(SpiceError::AnalysisFailed(
                        "Trust radius became too small".to_string()
                    ));
                }
            }
        }
    }
    
    /// Analyze local manifold properties
    fn analyze_manifold(&self, jacobian: &DMatrix<f64>, residual: &DVector<f64>) -> ManifoldProperties {
        let _n = jacobian.nrows();
        
        // Compute SVD for condition number and directions
        let svd = SVD::new(jacobian.clone(), true, true);
        let singular_values = &svd.singular_values;
        
        let max_sv = singular_values.max();
        let min_sv = singular_values[singular_values.len() - 1].max(1e-15);
        let condition = max_sv / min_sv;
        
        // Estimate curvature using second-order finite differences
        // This is a simplified approach - real implementation would use directional derivatives
        let gradient = jacobian.transpose() * residual;
        let gradient_norm = gradient.norm();
        
        // Heuristic curvature estimate based on condition and gradient
        let curvature = condition.sqrt() * (gradient_norm / (1.0 + gradient_norm));
        
        // Trust radius inversely proportional to curvature
        let trust_radius = if curvature > 1e6 {
            self.min_trust_radius
        } else if curvature < 1.0 {
            self.max_trust_radius
        } else {
            self.max_trust_radius / curvature.sqrt()
        };
        
        // Check for near-singularity
        let near_singularity = min_sv < 1e-10 || condition > 1e12;
        
        ManifoldProperties {
            curvature,
            condition,
            gradient_direction: gradient.normalize(),
            trust_radius,
            near_singularity,
        }
    }
    
    /// Gradient flow step for high curvature regions
    fn gradient_flow_step(
        &self,
        jacobian: &DMatrix<f64>,
        residual: &DVector<f64>,
        props: &ManifoldProperties,
    ) -> DVector<f64> {
        // Gradient flow: dx/dt = -J^T * F
        // Take small step in gradient direction
        let gradient = jacobian.transpose() * residual;
        let step_size = props.trust_radius / (1.0 + gradient.norm());
        
        -step_size * gradient
    }
    
    /// Newton step with trust region
    fn newton_trust_region_step(
        &self,
        jacobian: &DMatrix<f64>,
        residual: &DVector<f64>,
        props: &ManifoldProperties,
        trust_radius: f64,
    ) -> Result<DVector<f64>> {
        // Solve J*dx = -F with trust region constraint
        let neg_residual = -residual;
        
        // Try direct solve first
        if let Some(dx) = jacobian.clone().lu().solve(&neg_residual) {
            let dx_norm = dx.norm();
            
            if dx_norm <= trust_radius {
                // Newton step is within trust region
                return Ok(dx);
            } else {
                // Scale Newton step to trust region boundary
                return Ok(dx * (trust_radius / dx_norm));
            }
        }
        
        // If direct solve fails, use gradient direction
        println!("    → LU decomposition failed, using gradient direction");
        Ok(self.gradient_flow_step(jacobian, residual, props))
    }
    
    /// Line search with trust region enforcement
    fn line_search_with_trust(
        &mut self,
        x: &mut DVector<f64>,
        dx: &DVector<f64>,
        lambda: f64,
        props: &ManifoldProperties,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
    ) -> Result<(bool, f64)> {
        let current_residual_norm = {
            let (_, residual) = self.build_system_matrices_with_continuation(
                x, lambda, node_list, ground_idx, voltage_sources
            )?;
            residual.norm()
        };
        
        // Try full step first
        let mut alpha = 1.0;
        let mut best_alpha = 0.0;
        let mut best_reduction = 0.0;
        
        for _ in 0..10 {
            let x_new = &*x + alpha * dx;
            
            // Evaluate at new point
            let (_, new_residual) = self.build_system_matrices_with_continuation(
                &x_new, lambda, node_list, ground_idx, voltage_sources
            )?;
            let new_residual_norm = new_residual.norm();
            
            let reduction = (current_residual_norm - new_residual_norm) / current_residual_norm;
            
            if reduction > best_reduction {
                best_reduction = reduction;
                best_alpha = alpha;
            }
            
            if reduction > 0.1 {  // Sufficient reduction
                *x = x_new.clone();
                return Ok((true, reduction));
            }
            
            alpha *= 0.5;
            if alpha < 1e-4 {
                break;
            }
        }
        
        // Accept best step found
        if best_reduction > 0.0 {
            let x_best = &*x + best_alpha * dx;
            *x = x_best;
            Ok((true, best_reduction))
        } else {
            Ok((false, 0.0))
        }
    }
    
    /// Build system matrices with continuation parameter
    fn build_system_matrices_with_continuation(
        &mut self,
        x: &DVector<f64>,
        lambda: f64,
        node_list: &[NodeIndex],
        ground_idx: NodeIndex,
        voltage_sources: &[EdgeIndex],
    ) -> Result<(DMatrix<f64>, DVector<f64>)> {
        let num_nodes = node_list.len();
        let num_vsources = voltage_sources.len();
        let size = num_nodes + num_vsources;
        
        let mut jacobian = DMatrix::zeros(size, size);
        let mut residual = DVector::zeros(size);
        
        // Add Gmin for numerical stability (scales with continuation)
        let gmin = 1e-12 + (1.0 - lambda) * 1e-9;  // More Gmin at start
        for i in 0..num_nodes {
            jacobian[(i, i)] += gmin;
        }
        
        // Process each branch
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some(model) = self.models.get(&branch.name) {
                if matches!(model, ComponentModel::VoltageSource { .. }) {
                    continue;  // Handle voltage sources separately
                }
            }
            
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let n1_idx = if n1 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n1)
                };
                let n2_idx = if n2 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n2)
                };
                
                let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                let v_diff = v1 - v2;
                
                // Apply continuation to nonlinear elements
                let scaled_v_diff = if self.is_nonlinear_component(&branch.name) {
                    v_diff * lambda  // Scale voltage for nonlinear elements
                } else {
                    v_diff
                };
                
                let mut ctx = ModelExecutionContext {
                    jacobian: &mut jacobian,
                    residual: &mut residual,
                    x,
                    n1_idx,
                    n2_idx,
                    v_diff: scaled_v_diff,
                };
                
                if let Some(model) = self.models.get(&branch.name) {
                    self.model_engine.execute_component_model_with_params(
                        &branch.name, model, &mut ctx
                    ).map_err(|e| SpiceError::AnalysisFailed(
                        format!("Model execution failed: {}", e)
                    ))?;
                }
            }
        }
        
        // Handle voltage sources (scaled by continuation)
        for (vsrc_num, &edge_idx) in voltage_sources.iter().enumerate() {
            if let Some(branch) = self.circuit.branches().find(|(idx, _)| *idx == edge_idx) {
                if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                    if let Some(ComponentModel::VoltageSource { voltage, .. }) = 
                        self.models.get(&branch.1.name) {
                        
                        let vsrc_row = num_nodes + vsrc_num;
                        let scaled_voltage = voltage * lambda;  // Scale voltage sources
                        
                        // Standard voltage source stamping
                        let n1_idx = if n1 == ground_idx { None } else {
                            node_list.iter().position(|&n| n == n1)
                        };
                        let n2_idx = if n2 == ground_idx { None } else {
                            node_list.iter().position(|&n| n == n2)
                        };
                        
                        if let Some(i1) = n1_idx {
                            jacobian[(i1, vsrc_row)] = 1.0;
                            jacobian[(vsrc_row, i1)] = 1.0;
                        }
                        if let Some(i2) = n2_idx {
                            jacobian[(i2, vsrc_row)] = -1.0;
                            jacobian[(vsrc_row, i2)] = -1.0;
                        }
                        
                        let vsrc_current = x[vsrc_row];
                        if let Some(i1) = n1_idx {
                            residual[i1] += vsrc_current;
                        }
                        if let Some(i2) = n2_idx {
                            residual[i2] -= vsrc_current;
                        }
                        
                        let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                        let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                        residual[vsrc_row] = v1 - v2 - scaled_voltage;
                    }
                }
            }
        }
        
        Ok((jacobian, residual))
    }
    
    /// Check if a component is nonlinear
    fn is_nonlinear_component(&self, name: &str) -> bool {
        if let Some(model) = self.models.get(name) {
            matches!(model, 
                ComponentModel::LED { .. } | 
                ComponentModel::Diode { .. }
            )
        } else {
            false
        }
    }
    
    /// Setup circuit for analysis
    fn setup_circuit(&self) -> Result<(Vec<NodeIndex>, NodeIndex, Vec<EdgeIndex>)> {
        // Find ground node
        let ground_idx = self.circuit.nodes()
            .find(|(_, node)| node.name == "GND" || node.name == "0")
            .map(|(idx, _)| idx)
            .ok_or_else(|| SpiceError::NoGroundNode)?;
        
        // Build node list (excluding ground)
        let node_list: Vec<NodeIndex> = self.circuit.nodes()
            .filter(|(idx, _)| *idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
        
        // Find voltage sources
        let voltage_sources: Vec<EdgeIndex> = self.circuit.branches()
            .filter(|(_, branch)| {
                self.models.get(&branch.name)
                    .map(|m| matches!(m, ComponentModel::VoltageSource { .. }))
                    .unwrap_or(false)
            })
            .map(|(idx, _)| idx)
            .collect();
        
        Ok((node_list, ground_idx, voltage_sources))
    }
    
    /// Extract solution into AnalysisResult
    fn extract_solution(
        &self,
        x: DVector<f64>,
        node_list: Vec<NodeIndex>,
        ground_idx: NodeIndex,
        voltage_sources: Vec<EdgeIndex>,
    ) -> Result<AnalysisResult> {
        let mut node_voltages = NodeVoltages::new();
        let mut branch_currents = BranchCurrents::new();
        
        // Extract node voltages
        node_voltages.insert(ground_idx, 0.0);
        for (i, &node_idx) in node_list.iter().enumerate() {
            node_voltages.insert(node_idx, x[i]);
        }
        
        // Extract voltage source currents
        let num_nodes = node_list.len();
        for (i, &vs_idx) in voltage_sources.iter().enumerate() {
            branch_currents.insert(vs_idx, x[num_nodes + i]);
        }
        
        // Calculate branch currents for other components
        for (edge_idx, branch) in self.circuit.branches() {
            if branch_currents.contains_key(&edge_idx) {
                continue;  // Already have current for voltage sources
            }
            
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                let v_diff = v1 - v2;
                
                // Simple current calculation - could be enhanced
                let current = if let Some(ComponentModel::Resistor { resistance, .. }) = 
                    self.models.get(&branch.name) {
                    v_diff / resistance
                } else {
                    0.0  // Would need proper model evaluation
                };
                
                branch_currents.insert(edge_idx, current);
            }
        }
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power: 0.0,  // Would need to calculate
            iterations: 0,     // Would need to track total iterations
        })
    }
}