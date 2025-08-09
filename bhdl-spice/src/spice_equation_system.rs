//! SPICE Circuit Equation System
//! 
//! This module implements the EquationSystem trait for circuit analysis,
//! providing the circuit-specific intelligence while keeping GLACIER generic.

use nalgebra::{DMatrix, DVector};
use petgraph::graph::{NodeIndex, EdgeIndex};
use std::collections::HashMap;
use log::{debug, warn};

use crate::{
    circuit::{Circuit, Branch}, 
    components::ComponentModel,
    errors::{SpiceError, Result},
    generic_glacier_solver::{EquationSystem, Variable, VariableSpace},
    runtime_models::ModelExecutionContext,
};

/// Maps component types to their equation forms
#[derive(Debug, Clone)]
pub enum ComponentEquation {
    /// Linear resistor: i = (v1 - v2) / R
    Linear { conductance: f64 },
    
    /// Exponential device (LED/Diode): i = Is * (exp(v/nVt) - 1)
    Exponential { is: f64, n: f64, vt: f64 },
    
    /// Voltage source: v1 - v2 = V
    VoltageSource { voltage: f64 },
    
    /// Current source: i = I
    CurrentSource { current: f64 },
}

/// Circuit equation system for SPICE analysis
pub struct SpiceEquationSystem {
    /// The circuit being analyzed
    pub circuit: Circuit,
    
    /// Mapping from variable index to circuit element
    var_to_element: HashMap<usize, VariableElement>,
    
    /// Mapping from circuit element to variable index
    element_to_var: HashMap<VariableElement, usize>,
    
    /// Component equations
    component_equations: HashMap<EdgeIndex, ComponentEquation>,
    
    /// Ground node (if any)
    ground_node: Option<NodeIndex>,
    
    /// Number of variables
    num_vars: usize,
    
    /// Voltage source ramping factor (0.0 to 1.0)
    voltage_ramp: f64,
}

/// Element that a variable represents
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
enum VariableElement {
    NodeVoltage(NodeIndex),
    BranchCurrent(EdgeIndex),
}

impl SpiceEquationSystem {
    pub fn new(circuit: Circuit) -> Result<Self> {
        let mut system = Self {
            circuit,
            var_to_element: HashMap::new(),
            element_to_var: HashMap::new(),
            component_equations: HashMap::new(),
            ground_node: None,
            num_vars: 0,
            voltage_ramp: 1.0,
        };
        
        system.setup_variables()?;
        system.setup_equations()?;
        
        Ok(system)
    }
    
    /// Set up variables from circuit topology
    fn setup_variables(&mut self) -> Result<()> {
        let mut var_idx = 0;
        
        // Find ground node
        self.ground_node = self.circuit.nodes()
            .find(|(_, node)| node.is_ground || node.name == "gnd" || node.name == "0")
            .map(|(idx, _)| idx);
        
        // Voltage variables for non-ground nodes
        for (node_idx, _) in self.circuit.nodes() {
            if Some(node_idx) != self.ground_node {
                let element = VariableElement::NodeVoltage(node_idx);
                self.var_to_element.insert(var_idx, element.clone());
                self.element_to_var.insert(element, var_idx);
                var_idx += 1;
            }
        }
        
        // Current variables for branches that need them
        for (edge_idx, branch) in self.circuit.branches() {
            let needs_current = match branch.component_type.as_str() {
                "VoltageSource" => true,
                "LED" | "Diode" => true,
                _ => false,
            };
            
            if needs_current {
                let element = VariableElement::BranchCurrent(edge_idx);
                self.var_to_element.insert(var_idx, element.clone());
                self.element_to_var.insert(element, var_idx);
                var_idx += 1;
            }
        }
        
        self.num_vars = var_idx;
        Ok(())
    }
    
    /// Set up component equations
    fn setup_equations(&mut self) -> Result<()> {
        for (edge_idx, branch) in self.circuit.branches() {
            let equation = match branch.component_type.as_str() {
                "Resistor" => {
                    ComponentEquation::Linear { 
                        conductance: 1.0 / branch.value 
                    }
                }
                "LED" => {
                    // Red LED parameters - more realistic values
                    ComponentEquation::Exponential {
                        is: 1e-14,  // 10 femtoamps - typical for LED
                        n: 2.0,     // Higher emission coefficient for LED
                        vt: 0.026,  // 26mV at room temperature
                    }
                }
                "Diode" => {
                    // Silicon diode parameters
                    ComponentEquation::Exponential {
                        is: 1e-12,
                        n: 1.0,
                        vt: 0.026,
                    }
                }
                "VoltageSource" => {
                    ComponentEquation::VoltageSource {
                        voltage: branch.value,
                    }
                }
                "CurrentSource" => {
                    ComponentEquation::CurrentSource {
                        current: branch.value,
                    }
                }
                _ => {
                    warn!("Unknown component type: {}, treating as open circuit", 
                          branch.component_type);
                    ComponentEquation::Linear { conductance: 1e-12 }
                }
            };
            
            self.component_equations.insert(edge_idx, equation);
        }
        
        Ok(())
    }
    
    /// Set voltage ramping factor
    pub fn set_voltage_ramp(&mut self, ramp: f64) {
        self.voltage_ramp = ramp.clamp(0.0, 1.0);
    }
    
    /// Get variable index for a node voltage
    fn get_voltage_var(&self, node: NodeIndex) -> Option<usize> {
        self.element_to_var.get(&VariableElement::NodeVoltage(node)).copied()
    }
    
    /// Get variable index for a branch current
    fn get_current_var(&self, edge: EdgeIndex) -> Option<usize> {
        self.element_to_var.get(&VariableElement::BranchCurrent(edge)).copied()
    }
    
    /// Create variables for GLACIER solver
    pub fn create_variables(&self) -> Vec<Variable> {
        let mut variables = Vec::new();
        
        for i in 0..self.num_vars {
            if let Some(element) = self.var_to_element.get(&i) {
                let (name, space) = match element {
                    VariableElement::NodeVoltage(node_idx) => {
                        let node_name = self.circuit.get_node_by_id(*node_idx)
                            .map(|n| n.name.clone())
                            .unwrap_or_else(|| format!("n{}", node_idx.index()));
                        (format!("v_{}", node_name), VariableSpace::Linear)
                    }
                    VariableElement::BranchCurrent(edge_idx) => {
                        let branch = self.circuit.branches()
                            .find(|(idx, _)| *idx == *edge_idx)
                            .map(|(_, b)| b);
                        
                        let space = if let Some(branch) = branch {
                            match branch.component_type.as_str() {
                                "LED" | "Diode" => VariableSpace::Logarithmic,
                                _ => VariableSpace::Linear,
                            }
                        } else {
                            VariableSpace::Linear
                        };
                        
                        let branch_name = branch
                            .map(|b| b.name.clone())
                            .unwrap_or_else(|| format!("b{}", edge_idx.index()));
                        (format!("i_{}", branch_name), space)
                    }
                };
                
                // Set appropriate initial value based on variable space
                let initial_value = match space {
                    VariableSpace::Logarithmic => (1e-12_f64).ln(), // log(1e-12) ≈ -27.63
                    VariableSpace::Linear => 0.0,
                };
                
                variables.push(Variable {
                    id: i,
                    name,
                    space,
                    value: initial_value,
                });
            }
        }
        
        variables
    }
    
    /// Get initial guess for variables with ramping support
    pub fn get_initial_guess(&self, variables: &mut [Variable]) {
        self.get_initial_guess_with_ramp(variables, 1.0);
    }
    
    /// Get initial guess with voltage source ramping
    pub fn get_initial_guess_with_ramp(&self, variables: &mut [Variable], ramp: f64) {
        // Find voltage sources
        let mut voltage_sources = Vec::new();
        
        for (edge_idx, branch) in self.circuit.branches() {
            if branch.component_type == "VoltageSource" {
                voltage_sources.push((edge_idx, branch.value));
            }
        }
        
        // Smart initial guess based on circuit topology
        for var in variables {
            var.value = match var.space {
                VariableSpace::Linear => {
                    if var.name.starts_with("v_") {
                        // Voltage variable - use ramped value
                        if !voltage_sources.is_empty() {
                            // Start with fraction of source voltage
                            voltage_sources[0].1 * ramp * 0.5
                        } else {
                            0.0
                        }
                    } else {
                        // Current variable for voltage sources
                        if var.name.starts_with("i_V") {
                            // Voltage source current - start small
                            1e-3
                        } else {
                            // Other currents
                            10e-3
                        }
                    }
                }
                VariableSpace::Logarithmic => {
                    // For LEDs/Diodes in log space
                    // Better initial guess based on typical LED operation
                    let expected_current = if !voltage_sources.is_empty() {
                        // More realistic estimate based on circuit
                        let v_source = voltage_sources[0].1 * ramp;
                        
                        // If we're ramping up, start with smaller current
                        if ramp < 0.5 {
                            1e-6  // 1 microamp for low ramp values
                        } else {
                            // Estimate based on typical LED circuit
                            // Assume ~2V LED drop, find series resistance
                            let v_led = 2.0;
                            // Typical LED current range: 5-20mA
                            ((v_source - v_led) / 300.0).clamp(1e-6, 30e-3)
                        }
                    } else {
                        5e-3  // 5mA default
                    };
                    expected_current.ln()
                }
            };
        }
    }
}

impl EquationSystem for SpiceEquationSystem {
    fn evaluate_residuals(&self, variables: &[Variable]) -> DVector<f64> {
        let mut residual = DVector::zeros(self.num_vars);
        
        // Build variable value vector
        let values: Vec<f64> = variables.iter().map(|v| v.value).collect();
        let x = DVector::from_vec(values);
        
        // Clear KCL residuals
        for i in 0..self.num_vars {
            if let Some(VariableElement::NodeVoltage(_)) = self.var_to_element.get(&i) {
                residual[i] = 0.0;
            }
        }
        
        // Process each branch
        for (edge_idx, branch) in self.circuit.branches() {
            let (n1, n2) = self.circuit.branch_nodes(edge_idx).unwrap();
            let v1_idx = self.get_voltage_var(n1);
            let v2_idx = self.get_voltage_var(n2);
            let i_idx = self.get_current_var(edge_idx);
            
            let v1 = v1_idx.map(|i| x[i]).unwrap_or(0.0);
            let v2 = v2_idx.map(|i| x[i]).unwrap_or(0.0);
            let v_diff = v1 - v2;
            
            if let Some(equation) = self.component_equations.get(&edge_idx) {
                match equation {
                    ComponentEquation::Linear { conductance } => {
                        let current = conductance * v_diff;
                        
                        // KCL contributions
                        if let Some(idx) = v1_idx {
                            residual[idx] += current;
                        }
                        if let Some(idx) = v2_idx {
                            residual[idx] -= current;
                        }
                    }
                    
                    ComponentEquation::Exponential { is, n, vt } => {
                        if let Some(idx) = i_idx {
                            let var = &variables[idx];
                            
                            match var.space {
                                VariableSpace::Logarithmic => {
                                    // w = log(i)
                                    let w = x[idx];
                                    let log_is = is.ln();
                                    let nVt = n * vt;
                                    
                                    // LED equation in log space: w = log(Is) + v/(n*Vt)
                                    residual[idx] = w - log_is - v_diff / nVt;
                                    
                                    // KCL with current = exp(w)
                                    let current = w.exp();
                                    if let Some(v1_idx) = v1_idx {
                                        residual[v1_idx] += current;
                                    }
                                    if let Some(v2_idx) = v2_idx {
                                        residual[v2_idx] -= current;
                                    }
                                }
                                VariableSpace::Linear => {
                                    let i = x[idx];
                                    let nVt = n * vt;
                                    
                                    // Limit exponential to prevent overflow
                                    const MAX_EXP: f64 = 50.0;
                                    let v_norm = (v_diff / nVt).min(MAX_EXP);
                                    let exp_term = v_norm.exp();
                                    
                                    // Diode equation with limiting
                                    residual[idx] = if v_norm >= MAX_EXP {
                                        // Linear extrapolation beyond MAX_EXP
                                        let i_max = is * (MAX_EXP.exp() - 1.0);
                                        let slope = is * MAX_EXP.exp() / nVt;
                                        i - (i_max + slope * (v_diff - MAX_EXP * nVt))
                                    } else {
                                        i - is * (exp_term - 1.0)
                                    };
                                    
                                    // KCL
                                    if let Some(v1_idx) = v1_idx {
                                        residual[v1_idx] += i;
                                    }
                                    if let Some(v2_idx) = v2_idx {
                                        residual[v2_idx] -= i;
                                    }
                                }
                            }
                        }
                    }
                    
                    ComponentEquation::VoltageSource { voltage } => {
                        if let Some(idx) = i_idx {
                            // Voltage constraint with ramping
                            residual[idx] = v_diff - (voltage * self.voltage_ramp);
                            
                            // Current contribution
                            let current = x[idx];
                            if let Some(v1_idx) = v1_idx {
                                residual[v1_idx] -= current;
                            }
                            if let Some(v2_idx) = v2_idx {
                                residual[v2_idx] += current;
                            }
                        }
                    }
                    
                    ComponentEquation::CurrentSource { current } => {
                        // Direct KCL contribution
                        if let Some(idx) = v1_idx {
                            residual[idx] -= current;
                        }
                        if let Some(idx) = v2_idx {
                            residual[idx] += current;
                        }
                    }
                }
            }
        }
        
        residual
    }
    
    fn build_jacobian(&self, variables: &[Variable]) -> DMatrix<f64> {
        let n = self.num_vars;
        let mut jacobian = DMatrix::zeros(n, n);
        
        // Build variable value vector
        let values: Vec<f64> = variables.iter().map(|v| v.value).collect();
        let x = DVector::from_vec(values);
        
        // Process each branch
        for (edge_idx, _) in self.circuit.branches() {
            let (n1, n2) = self.circuit.branch_nodes(edge_idx).unwrap();
            let v1_idx = self.get_voltage_var(n1);
            let v2_idx = self.get_voltage_var(n2);
            let i_idx = self.get_current_var(edge_idx);
            
            let v1 = v1_idx.map(|i| x[i]).unwrap_or(0.0);
            let v2 = v2_idx.map(|i| x[i]).unwrap_or(0.0);
            let v_diff = v1 - v2;
            
            if let Some(equation) = self.component_equations.get(&edge_idx) {
                match equation {
                    ComponentEquation::Linear { conductance } => {
                        // Stamp conductance matrix
                        if let (Some(i), Some(j)) = (v1_idx, v2_idx) {
                            jacobian[(i, i)] += conductance;
                            jacobian[(i, j)] -= conductance;
                            jacobian[(j, i)] -= conductance;
                            jacobian[(j, j)] += conductance;
                        }
                    }
                    
                    ComponentEquation::Exponential { is, n, vt } => {
                        if let Some(idx) = i_idx {
                            let var = &variables[idx];
                            
                            match var.space {
                                VariableSpace::Logarithmic => {
                                    let w = x[idx];
                                    let nVt = n * vt;
                                    
                                    // LED equation Jacobian (constant!)
                                    jacobian[(idx, idx)] = 1.0;
                                    if let Some(v1_idx) = v1_idx {
                                        jacobian[(idx, v1_idx)] = -1.0 / nVt;
                                    }
                                    if let Some(v2_idx) = v2_idx {
                                        jacobian[(idx, v2_idx)] = 1.0 / nVt;
                                    }
                                    
                                    // KCL Jacobian
                                    let i = w.exp();
                                    if let Some(v1_idx) = v1_idx {
                                        jacobian[(v1_idx, idx)] = i;
                                    }
                                    if let Some(v2_idx) = v2_idx {
                                        jacobian[(v2_idx, idx)] = -i;
                                    }
                                }
                                VariableSpace::Linear => {
                                    let nVt = n * vt;
                                    
                                    // Limit exponential to prevent overflow
                                    const MAX_EXP: f64 = 50.0;
                                    let v_norm = (v_diff / nVt).min(MAX_EXP);
                                    
                                    let di_dv = if v_norm >= MAX_EXP {
                                        // Constant derivative in linear region
                                        (is / nVt) * MAX_EXP.exp()
                                    } else {
                                        (is / nVt) * v_norm.exp()
                                    };
                                    
                                    // Diode equation Jacobian
                                    jacobian[(idx, idx)] = 1.0;
                                    if let Some(v1_idx) = v1_idx {
                                        jacobian[(idx, v1_idx)] = -di_dv;
                                    }
                                    if let Some(v2_idx) = v2_idx {
                                        jacobian[(idx, v2_idx)] = di_dv;
                                    }
                                    
                                    // KCL Jacobian
                                    if let Some(v1_idx) = v1_idx {
                                        jacobian[(v1_idx, idx)] = 1.0;
                                    }
                                    if let Some(v2_idx) = v2_idx {
                                        jacobian[(v2_idx, idx)] = -1.0;
                                    }
                                }
                            }
                        }
                    }
                    
                    ComponentEquation::VoltageSource { .. } => {
                        if let Some(idx) = i_idx {
                            // Voltage constraint Jacobian
                            if let Some(v1_idx) = v1_idx {
                                jacobian[(idx, v1_idx)] = 1.0;
                            }
                            if let Some(v2_idx) = v2_idx {
                                jacobian[(idx, v2_idx)] = -1.0;
                            }
                            
                            // Current contribution Jacobian
                            if let Some(v1_idx) = v1_idx {
                                jacobian[(v1_idx, idx)] = -1.0;
                            }
                            if let Some(v2_idx) = v2_idx {
                                jacobian[(v2_idx, idx)] = 1.0;
                            }
                        }
                    }
                    
                    ComponentEquation::CurrentSource { .. } => {
                        // Current source has no Jacobian contributions
                        // (constant current independent of voltages)
                    }
                }
            }
        }
        
        jacobian
    }
    
    fn num_equations(&self) -> usize {
        self.num_vars
    }
    
    fn num_variables(&self) -> usize {
        self.num_vars
    }
    
    fn get_scaling_hints(&self) -> Option<Vec<f64>> {
        // Could provide scaling based on expected voltage/current ranges
        None
    }
}

/// Extract solution into node voltages and branch currents
pub fn extract_solution(
    system: &SpiceEquationSystem,
    variables: &[Variable],
) -> (HashMap<NodeIndex, f64>, HashMap<EdgeIndex, f64>) {
    let mut node_voltages = HashMap::new();
    let mut branch_currents = HashMap::new();
    
    // Extract voltages
    for (i, var) in variables.iter().enumerate() {
        if let Some(element) = system.var_to_element.get(&i) {
            match element {
                VariableElement::NodeVoltage(node_idx) => {
                    node_voltages.insert(*node_idx, var.value);
                }
                VariableElement::BranchCurrent(edge_idx) => {
                    let current = match var.space {
                        VariableSpace::Logarithmic => var.value.exp(),
                        VariableSpace::Linear => var.value,
                    };
                    branch_currents.insert(*edge_idx, current);
                }
            }
        }
    }
    
    // Calculate currents for branches without explicit variables
    for (edge_idx, branch) in system.circuit.branches() {
        if !branch_currents.contains_key(&edge_idx) {
            if let Some(equation) = system.component_equations.get(&edge_idx) {
                if let ComponentEquation::Linear { conductance } = equation {
                    let (n1, n2) = system.circuit.branch_nodes(edge_idx).unwrap();
                    let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                    let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                    let current = conductance * (v1 - v2);
                    branch_currents.insert(edge_idx, current);
                }
            }
        }
    }
    
    (node_voltages, branch_currents)
}