//! Nonlinear DC analysis using Newton-Raphson method

use nalgebra::{DMatrix, DVector};
use petgraph::graph::{NodeIndex, EdgeIndex};
use std::collections::HashMap;
use log::info;

use crate::{
    Circuit, ComponentModel, SpiceError, Result,
    NodeVoltages, BranchCurrents, AnalysisResult,
};

/// Nonlinear DC Analysis solver using Newton-Raphson iteration
pub struct NonlinearDcAnalysis {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    max_iterations: usize,
    tolerance: f64,
    damping_factor: f64,
}

impl NonlinearDcAnalysis {
    /// Create new nonlinear analysis
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
            max_iterations: 100,
            tolerance: 1e-6,
            damping_factor: 1.0,
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
    
    /// Set damping factor for convergence (0.0 to 1.0)
    pub fn set_damping(&mut self, factor: f64) {
        self.damping_factor = factor.clamp(0.1, 1.0);
    }
    
    /// Perform nonlinear DC analysis
    pub fn analyze(&mut self) -> Result<AnalysisResult> {
        info!("Starting nonlinear DC analysis");
        
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
        
        // Initial guess - use linear DC analysis result
        let mut x = DVector::zeros(matrix_size);
        
        // Initialize node voltages with small values to help convergence
        for i in 0..num_nodes {
            x[i] = 0.1; // Small initial voltage
        }
        
        // Newton-Raphson iteration
        let mut iteration = 0;
        let mut converged = false;
        
        while iteration < self.max_iterations && !converged {
            // Build Jacobian matrix and residual vector
            let (jacobian, residual) = self.build_jacobian_and_residual(
                &x, &node_list, ground_idx, &voltage_sources
            )?;
            
            // Check convergence
            let residual_norm = residual.norm();
            info!("Iteration {}: residual norm = {:.6e}", iteration, residual_norm);
            
            if residual_norm < self.tolerance {
                converged = true;
                break;
            }
            
            // Solve J * dx = -F
            let dx = jacobian.lu().solve(&(-&residual))
                .ok_or_else(|| SpiceError::SingularMatrix)?;
            
            // Update solution with damping
            x += &(self.damping_factor * dx);
            
            // Apply voltage limiting to prevent numerical issues
            for i in 0..num_nodes {
                x[i] = x[i].clamp(-100.0, 100.0);
            }
            
            iteration += 1;
        }
        
        if !converged {
            return Err(SpiceError::ConvergenceFailed(iteration));
        }
        
        info!("Converged in {} iterations", iteration);
        
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
    
    /// Build Jacobian matrix and residual vector for Newton-Raphson
    fn build_jacobian_and_residual(
        &self,
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
        
        // Process each branch
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                // Get node indices in our matrix
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
                
                if let Some(model) = self.models.get(&branch.name) {
                    match model {
                        ComponentModel::Resistor { resistance, .. } => {
                            let g = 1.0 / resistance;
                            let i = g * v_diff;
                            
                            // Stamp conductance in Jacobian
                            if let Some(i1) = n1_idx {
                                jacobian[(i1, i1)] += g;
                                residual[i1] += i;
                            }
                            if let Some(i2) = n2_idx {
                                jacobian[(i2, i2)] += g;
                                residual[i2] -= i;
                            }
                            if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                jacobian[(i1, i2)] -= g;
                                jacobian[(i2, i1)] -= g;
                            }
                        }
                        
                        ComponentModel::LED { forward_voltage, dynamic_resistance, .. } => {
                            // LED model: I = Is * (exp(V/Vt) - 1) approximated as piecewise linear
                            let _vt = 0.026; // Thermal voltage at room temperature
                            
                            if v_diff > forward_voltage * 0.7 {
                                // Forward biased - use exponential model linearized
                                let v_d = v_diff - forward_voltage;
                                let i = v_d / dynamic_resistance;
                                let g = 1.0 / dynamic_resistance;
                                
                                // For Newton-Raphson, we need di/dv
                                let di_dv = g; // Simplified - should use exponential derivative
                                
                                // Stamp in Jacobian
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += di_dv;
                                    residual[i1] += i;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += di_dv;
                                    residual[i2] -= i;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= di_dv;
                                    jacobian[(i2, i1)] -= di_dv;
                                }
                            } else {
                                // Reverse biased - very high resistance
                                let g = 1e-9;
                                let i = g * v_diff;
                                
                                // Stamp small conductance
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += g;
                                    residual[i1] += i;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += g;
                                    residual[i2] -= i;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= g;
                                    jacobian[(i2, i1)] -= g;
                                }
                            }
                        }
                        
                        ComponentModel::Diode { saturation_current, emission_coefficient, .. } => {
                            // Full diode model: I = Is * (exp(V/(n*Vt)) - 1)
                            let vt = 0.026;
                            let n = emission_coefficient.unwrap_or(1.0);
                            let is = saturation_current.unwrap_or(1e-12);
                            
                            if v_diff > 0.0 {
                                // Forward biased
                                let exp_term = (v_diff / (n * vt)).min(40.0).exp();
                                let i = is * (exp_term - 1.0);
                                let di_dv = (is / (n * vt)) * exp_term;
                                
                                // Stamp in Jacobian
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += di_dv;
                                    residual[i1] += i;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += di_dv;
                                    residual[i2] -= i;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= di_dv;
                                    jacobian[(i2, i1)] -= di_dv;
                                }
                            } else {
                                // Reverse biased
                                let i = -is;
                                let di_dv = 1e-12; // Very small conductance
                                
                                // Stamp in Jacobian
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += di_dv;
                                    residual[i1] += i;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += di_dv;
                                    residual[i2] -= i;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= di_dv;
                                    jacobian[(i2, i1)] -= di_dv;
                                }
                            }
                        }
                        
                        ComponentModel::VoltageSource { .. } => {
                            // Handled separately below
                        }
                        
                        ComponentModel::CurrentSource { current, .. } => {
                            // Current source - constant current
                            if let Some(i1) = n1_idx {
                                residual[i1] -= current;
                            }
                            if let Some(i2) = n2_idx {
                                residual[i2] += current;
                            }
                        }
                        
                        _ => {
                            // Other components - use linear resistance model
                            let resistance = model.dc_resistance();
                            if resistance.is_finite() && resistance > 0.0 {
                                let g = 1.0 / resistance;
                                let i = g * v_diff;
                                
                                // Stamp conductance
                                if let Some(i1) = n1_idx {
                                    jacobian[(i1, i1)] += g;
                                    residual[i1] += i;
                                }
                                if let Some(i2) = n2_idx {
                                    jacobian[(i2, i2)] += g;
                                    residual[i2] -= i;
                                }
                                if let (Some(i1), Some(i2)) = (n1_idx, n2_idx) {
                                    jacobian[(i1, i2)] -= g;
                                    jacobian[(i2, i1)] -= g;
                                }
                            }
                        }
                    }
                }
            }
        }
        
        // Handle voltage sources
        for (vsrc_num, &edge_idx) in voltage_sources.iter().enumerate() {
            if let Some(branch) = self.circuit.branches().find(|(idx, _)| *idx == edge_idx) {
                if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                    if let Some(ComponentModel::VoltageSource { voltage, .. }) = self.models.get(&branch.1.name) {
                        let vsrc_row = num_nodes + vsrc_num;
                        let vsrc_current = x[vsrc_row];
                        
                        // Voltage source equations
                        let n1_idx = if n1 == ground_idx { None } else { node_list.iter().position(|&n| n == n1) };
                        let n2_idx = if n2 == ground_idx { None } else { node_list.iter().position(|&n| n == n2) };
                        
                        // KCL equations
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
                        
                        // Voltage constraint
                        let v1 = n1_idx.map(|i| x[i]).unwrap_or(0.0);
                        let v2 = n2_idx.map(|i| x[i]).unwrap_or(0.0);
                        residual[vsrc_row] = v1 - v2 - voltage;
                    }
                }
            }
        }
        
        Ok((jacobian, residual))
    }
    
    /// Calculate branch currents from node voltages
    fn calculate_branch_currents(
        &mut self,
        node_voltages: &NodeVoltages,
        voltage_sources: &[EdgeIndex],
        vsource_currents: &[f64],
    ) -> Result<BranchCurrents> {
        let mut branch_currents = BranchCurrents::new();
        
        // Collect branches to avoid borrowing issues
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
                            // Get current from solution vector
                            if let Some(vsrc_idx) = voltage_sources.iter().position(|&e| e == edge_idx) {
                                vsource_currents[vsrc_idx]
                            } else {
                                0.0
                            }
                        }
                        
                        ComponentModel::LED { forward_voltage, dynamic_resistance, .. } => {
                            if v_diff > forward_voltage * 0.7 {
                                // Forward biased
                                (v_diff - forward_voltage) / dynamic_resistance
                            } else {
                                // Reverse biased
                                v_diff * 1e-9
                            }
                        }
                        
                        ComponentModel::Diode { saturation_current, emission_coefficient, .. } => {
                            let vt = 0.026;
                            let n = emission_coefficient.unwrap_or(1.0);
                            let is = saturation_current.unwrap_or(1e-12);
                            
                            if v_diff > 0.0 {
                                is * ((v_diff / (n * vt)).min(40.0).exp() - 1.0)
                            } else {
                                -is
                            }
                        }
                        
                        _ => {
                            // Linear components
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