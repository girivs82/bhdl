//! DC Analysis engine using Modified Nodal Analysis (MNA)

use std::collections::HashMap;
use nalgebra::{DMatrix, DVector};
use petgraph::graph::{NodeIndex, EdgeIndex};
use log::{debug, info};

use crate::{Circuit, ComponentModel, SpiceError, Result};

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
    /// Convergence iterations (if iterative solver used)
    pub iterations: usize,
}

/// DC Analysis solver
pub struct DcAnalysis {
    circuit: Circuit,
    /// Component models indexed by branch name
    models: HashMap<String, ComponentModel>,
}

impl DcAnalysis {
    /// Create a new DC analysis for a circuit
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
        }
    }
    
    /// Add component model
    pub fn add_model(&mut self, component_name: String, model: ComponentModel) {
        self.models.insert(component_name, model);
    }
    
    /// Run DC analysis using Modified Nodal Analysis
    pub fn analyze(&mut self) -> Result<AnalysisResult> {
        info!("Starting DC analysis");
        
        // Check for ground node
        let ground_idx = self.circuit.ground_node()
            .ok_or(SpiceError::NoGroundNode)?
            .0;
        
        // Build node list (excluding ground)
        let mut node_list: Vec<NodeIndex> = self.circuit.nodes()
            .map(|(idx, _)| idx)
            .filter(|&idx| idx != ground_idx)
            .collect();
        node_list.sort_by_key(|n| n.index());
        
        let num_nodes = node_list.len();
        
        // Count voltage sources (they add extra unknowns)
        let voltage_sources: Vec<_> = self.circuit.branches()
            .filter(|(_, branch)| {
                self.models.get(&branch.name)
                    .map(|m| matches!(m, ComponentModel::VoltageSource { .. }))
                    .unwrap_or(false)
            })
            .map(|(idx, _)| idx)
            .collect();
        
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        
        // Build MNA matrices: A * x = b
        // where x = [v1, v2, ..., vn, i1, i2, ..., im]
        // v_i are node voltages, i_j are currents through voltage sources
        let mut a_matrix = DMatrix::<f64>::zeros(matrix_size, matrix_size);
        let mut b_vector = DVector::<f64>::zeros(matrix_size);
        
        // Build conductance submatrix (G)
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                if let Some(model) = self.models.get(&branch.name) {
                    match model {
                        ComponentModel::Resistor { resistance, .. } => {
                            let conductance = 1.0 / resistance;
                            self.stamp_conductance(&mut a_matrix, &node_list, ground_idx, n1, n2, conductance);
                        }
                        ComponentModel::VoltageSource { .. } => {
                            // Handled separately below
                        }
                        ComponentModel::CurrentSource { current, .. } => {
                            self.stamp_current(&mut b_vector, &node_list, ground_idx, n1, n2, *current);
                        }
                        _ => {
                            // For now, use DC resistance model
                            let resistance = model.dc_resistance();
                            if resistance.is_finite() && resistance > 0.0 {
                                let conductance = 1.0 / resistance;
                                self.stamp_conductance(&mut a_matrix, &node_list, ground_idx, n1, n2, conductance);
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
                        
                        // Voltage source stamps
                        if let Some(n1_idx) = node_list.iter().position(|&n| n == n1) {
                            a_matrix[(n1_idx, vsrc_row)] = 1.0;
                            a_matrix[(vsrc_row, n1_idx)] = 1.0;
                        }
                        if let Some(n2_idx) = node_list.iter().position(|&n| n == n2) {
                            a_matrix[(n2_idx, vsrc_row)] = -1.0;
                            a_matrix[(vsrc_row, n2_idx)] = -1.0;
                        }
                        
                        // Voltage constraint
                        b_vector[vsrc_row] = *voltage;
                    }
                }
            }
        }
        
        debug!("MNA matrix size: {}x{}", matrix_size, matrix_size);
        debug!("A matrix:\n{}", a_matrix);
        debug!("b vector:\n{}", b_vector);
        
        // Solve linear system
        let solution = a_matrix.lu().solve(&b_vector)
            .ok_or(SpiceError::SingularMatrix)?;
        
        // Extract node voltages
        let mut node_voltages = NodeVoltages::new();
        node_voltages.insert(ground_idx, 0.0);  // Ground is always 0V
        
        for (i, &node_idx) in node_list.iter().enumerate() {
            node_voltages.insert(node_idx, solution[i]);
            self.circuit.set_node_voltage(node_idx, solution[i]);
        }
        
        // Calculate branch currents using Ohm's law
        let mut branch_currents = BranchCurrents::new();
        let mut total_power = 0.0;
        
        // Collect branch data first to avoid borrowing issues
        let branches: Vec<_> = self.circuit.branches()
            .map(|(idx, branch)| (idx, branch.name.clone(), branch.clone()))
            .collect();
        
        for (edge_idx, branch_name, _branch) in branches {
            if let Some((n1, n2)) = self.circuit.branch_nodes(edge_idx) {
                let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                let voltage_diff = v1 - v2;
                
                let current = if let Some(model) = self.models.get(&branch_name) {
                    match model {
                        ComponentModel::Resistor { resistance, .. } => voltage_diff / resistance,
                        ComponentModel::VoltageSource { .. } => {
                            // Current from solution vector
                            if let Some(vsrc_idx) = voltage_sources.iter().position(|&e| e == edge_idx) {
                                solution[num_nodes + vsrc_idx]
                            } else {
                                0.0
                            }
                        }
                        ComponentModel::CurrentSource { current, .. } => *current,
                        _ => {
                            let resistance = model.dc_resistance();
                            if resistance.is_finite() && resistance > 0.0 {
                                voltage_diff / resistance
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
                
                // Calculate power
                let power = voltage_diff.abs() * current.abs();
                total_power += power;
            }
        }
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: 1,  // Direct solver
        })
    }
    
    /// Stamp conductance into MNA matrix
    fn stamp_conductance(
        &self,
        matrix: &mut DMatrix<f64>,
        node_list: &[NodeIndex],
        ground: NodeIndex,
        n1: NodeIndex,
        n2: NodeIndex,
        conductance: f64,
    ) {
        // Get matrix indices for nodes
        let n1_idx = if n1 == ground { None } else { node_list.iter().position(|&n| n == n1) };
        let n2_idx = if n2 == ground { None } else { node_list.iter().position(|&n| n == n2) };
        
        // Stamp conductance matrix
        if let Some(i) = n1_idx {
            matrix[(i, i)] += conductance;
            if let Some(j) = n2_idx {
                matrix[(i, j)] -= conductance;
            }
        }
        if let Some(j) = n2_idx {
            matrix[(j, j)] += conductance;
            if let Some(i) = n1_idx {
                matrix[(j, i)] -= conductance;
            }
        }
    }
    
    /// Stamp current source into RHS vector
    fn stamp_current(
        &self,
        vector: &mut DVector<f64>,
        node_list: &[NodeIndex],
        ground: NodeIndex,
        n1: NodeIndex,
        n2: NodeIndex,
        current: f64,
    ) {
        // Current flows from n1 to n2
        if n1 != ground {
            if let Some(i) = node_list.iter().position(|&n| n == n1) {
                vector[i] -= current;
            }
        }
        if n2 != ground {
            if let Some(j) = node_list.iter().position(|&n| n == n2) {
                vector[j] += current;
            }
        }
    }
}