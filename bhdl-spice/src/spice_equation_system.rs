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

    /// Piecewise-linear tabulated I-V element (an IBIS buffer's composed
    /// DC curve): i = interp(points, v1 - v2), clamped to the end currents
    /// outside the table. Stamped like a voltage-dependent conductance —
    /// KCL contribution direct, Jacobian = local segment slope.
    TableIV { points: Vec<(f64, f64)> },

    /// Optocoupler phototransistor: a current-controlled current source
    /// with smooth saturation — i = ctr · i(ctrl_edge) · tanh(v_diff/v_knee).
    /// `ctrl_edge` is the IRED branch whose solved current controls this
    /// one (it must own a current variable — the converter always emits it
    /// as an LED-class branch, which does). The tanh knee sits at half the
    /// part's cited VCE(sat), so the collector current rolls off exactly
    /// where the datasheet says the transistor saturates; load-limited
    /// operation collapses V_CE toward saturation instead of fighting the
    /// source. The coupling is control-only — no galvanic path is stamped
    /// across the isolation barrier.
    PhotoCoupled { ctr: f64, v_knee: f64, ctrl_edge: EdgeIndex },
}

/// PWL interpolation shared by the TableIV residual and Jacobian.
/// Extrapolation conductance floor beyond a table's characterized range.
///
/// Flat (zero-slope) clamping outside the table is a NEWTON TRAP: once
/// an iterate overshoots past the last breakpoint, the Jacobian reports
/// zero local conductance and the only restoring force is GSHUNT (1e-9 S)
/// — floating Hi-Z IBIS pins were observed parked at ±16 MV with the
/// solver unable to walk back (Uno @ min corner, whose tables end 0.15V
/// lower than typ). Beyond the characterized range ANY model is an
/// extrapolation policy; we extend the end segment's own slope, floored
/// at 1 µS so the extension is always restoring (a µS adds only µA/V —
/// operating points INSIDE the table are untouched).
const TABLE_EXTRAP_G_MIN: f64 = 1e-6;

fn table_iv_interp(points: &[(f64, f64)], v: f64) -> f64 {
    if points.is_empty() {
        return 0.0;
    }
    let end_slope = |a: (f64, f64), b: (f64, f64)| -> f64 {
        let g = if b.0 > a.0 { (b.1 - a.1) / (b.0 - a.0) } else { 0.0 };
        g.max(TABLE_EXTRAP_G_MIN)
    };
    let first = points[0];
    let last = points[points.len() - 1];
    if v <= first.0 {
        let g = if points.len() >= 2 { end_slope(first, points[1]) } else { TABLE_EXTRAP_G_MIN };
        return first.1 + g * (v - first.0);
    }
    if v >= last.0 {
        let g = if points.len() >= 2 { end_slope(points[points.len() - 2], last) } else { TABLE_EXTRAP_G_MIN };
        return last.1 + g * (v - last.0);
    }
    for w in points.windows(2) {
        let (v0, i0) = w[0];
        let (v1, i1) = w[1];
        if v <= v1 {
            return i0 + (v - v0) / (v1 - v0) * (i1 - i0);
        }
    }
    last.1
}

/// Local dI/dV — central difference over the interpolant (smooths the
/// breakpoint kinks the way GLACIER's IbisTable gradient does).
fn table_iv_slope(points: &[(f64, f64)], v: f64) -> f64 {
    let d = 1e-6;
    (table_iv_interp(points, v + d) - table_iv_interp(points, v - d)) / (2.0 * d)
}

/// Numeric branch-metadata read (the loader's `meta_f64`, local copy).
fn meta_f64(branch: &Branch, key: &str) -> Option<f64> {
    branch.metadata.get(key).and_then(|s| s.parse::<f64>().ok())
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
                    // Shockley parameters from branch metadata when the part
                    // declared them (an optocoupler IRED's Is is derived from
                    // its CITED Vf@IF point); class-typical red-LED values
                    // otherwise.
                    ComponentEquation::Exponential {
                        is: meta_f64(branch, crate::circuit::META_SATURATION_CURRENT)
                            .unwrap_or(1e-14),
                        n: meta_f64(branch, crate::circuit::META_EMISSION_COEFFICIENT)
                            .unwrap_or(2.0),
                        vt: 0.026,  // 26mV at room temperature
                    }
                }
                "Diode" => {
                    // Silicon diode parameters (metadata overrides, as LED)
                    ComponentEquation::Exponential {
                        is: meta_f64(branch, crate::circuit::META_SATURATION_CURRENT)
                            .unwrap_or(1e-12),
                        n: meta_f64(branch, crate::circuit::META_EMISSION_COEFFICIENT)
                            .unwrap_or(1.0),
                        vt: 0.026,
                    }
                }
                // Optocoupler phototransistor — resolved to its controlling
                // IRED branch here so the residual/Jacobian passes need no
                // name lookups. An unresolvable control edge is a converter
                // bug: warn loudly, stamp an open circuit.
                "PhotoCoupled" => {
                    let ctr = meta_f64(branch, crate::circuit::META_CTR);
                    let v_knee = meta_f64(branch, crate::circuit::META_CTR_VKNEE);
                    let ctrl = branch
                        .metadata
                        .get(crate::circuit::META_CTRL_BRANCH)
                        .and_then(|name| self.circuit.get_branch(name))
                        .map(|(idx, _)| idx);
                    match (ctr, v_knee, ctrl) {
                        (Some(ctr), Some(v_knee), Some(ctrl_edge)) if v_knee > 0.0 => {
                            ComponentEquation::PhotoCoupled { ctr, v_knee, ctrl_edge }
                        }
                        _ => {
                            warn!(
                                "PhotoCoupled branch {} missing ctr/v_knee/ctrl_branch metadata — open circuit",
                                branch.name
                            );
                            ComponentEquation::Linear { conductance: 1e-9 }
                        }
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
                // IBIS buffer (or any tabulated element): the composed PWL
                // rides branch metadata. A missing/malformed table is NOT
                // silently opened — the converter only stamps this type
                // with a valid table, so failure here is a programming
                // error worth a loud warning + open-circuit fallback.
                "IbisBuffer" => {
                    match branch
                        .metadata
                        .get(crate::circuit::META_IV_TABLE)
                        .and_then(|s| crate::circuit::decode_iv_table(s))
                    {
                        Some(points) => ComponentEquation::TableIV { points },
                        None => {
                            warn!(
                                "IbisBuffer branch {} has no decodable iv_table — open circuit",
                                branch.name
                            );
                            ComponentEquation::Linear { conductance: 1e-9 }
                        }
                    }
                }
                // Capacitor: DC open circuit modeled as large leakage resistance.
                // 1e-9 S (1 GΩ) — negligible current but avoids the extreme
                // 10^18 conductance ratio that makes the Jacobian ill-conditioned.
                "Capacitor" => {
                    debug!("Capacitor {} modeled as DC open circuit (1e-9 S)", branch.name);
                    ComponentEquation::Linear { conductance: 1e-9 }
                }
                // Inductor: DC short circuit modeled as small DCR (series resistance).
                // 100 S (10 mΩ) — realistic DCR for power inductors, drops only
                // 10 mV at 1 A.  Avoids the 1e6 S value that dominated Jacobians.
                "Inductor" => {
                    debug!("Inductor {} modeled as DC short circuit (100 S / 10 mΩ DCR)", branch.name);
                    ComponentEquation::Linear { conductance: 100.0 }
                }
                _ => {
                    warn!("Unknown component type: {}, treating as open circuit",
                          branch.component_type);
                    ComponentEquation::Linear { conductance: 1e-9 }
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
                                // LEDs are always forward-biased by design, so
                                // log-space works well and avoids exponential
                                // overflow.  Diodes (catch, TVS) can be reverse-
                                // biased, where log(i) is undefined for i < 0;
                                // use linear space for diodes.
                                "LED" => VariableSpace::Logarithmic,
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

/// Shunt conductance from every node to ground (SPICE `gshunt`). Real
/// boards are full of DC-floating islands — two unmodeled MCU pins joined
/// by a series resistor make a 2×2 KCL block that is EXACTLY singular
/// (healthy diagonals, so the near-zero-diagonal perturbation never fires;
/// first exposed by the Arduino Uno R3's crossed UART). 1 nS anchors such
/// islands at 0 V while perturbing driven nodes by ~nV. Stamped in BOTH the
/// residual and the Jacobian so Newton sees a consistent system.
const GSHUNT: f64 = 1e-9;

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

                    ComponentEquation::TableIV { points } => {
                        let current = table_iv_interp(points, v_diff);
                        if let Some(idx) = v1_idx {
                            residual[idx] += current;
                        }
                        if let Some(idx) = v2_idx {
                            residual[idx] -= current;
                        }
                    }

                    // i = ctr · i_ctrl · tanh(v/v_knee): KCL only — the
                    // branch owns no current variable; the controlling
                    // (IRED) branch's current variable is read directly.
                    ComponentEquation::PhotoCoupled { ctr, v_knee, ctrl_edge } => {
                        let i_ctrl = self
                            .get_current_var(*ctrl_edge)
                            .map(|ci| match variables[ci].space {
                                VariableSpace::Logarithmic => x[ci].exp(),
                                VariableSpace::Linear => x[ci],
                            })
                            .unwrap_or(0.0);
                        let current = ctr * i_ctrl * (v_diff / v_knee).tanh();
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

        // gshunt: every node leaks to ground (see GSHUNT doc).
        for i in 0..self.num_vars {
            if let Some(VariableElement::NodeVoltage(_)) = self.var_to_element.get(&i) {
                residual[i] += GSHUNT * x[i];
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
                        // Stamp conductance matrix (MNA standard stamp)
                        // Must handle cases where one node is ground (idx = None)
                        if let Some(i) = v1_idx {
                            jacobian[(i, i)] += conductance;
                            if let Some(j) = v2_idx {
                                jacobian[(i, j)] -= conductance;
                            }
                        }
                        if let Some(j) = v2_idx {
                            jacobian[(j, j)] += conductance;
                            if let Some(i) = v1_idx {
                                jacobian[(j, i)] -= conductance;
                            }
                        }
                    }

                    // Tabulated element: identical stamp with the LOCAL
                    // slope as the conductance (linearization at the
                    // current operating point).
                    ComponentEquation::TableIV { points } => {
                        let g = table_iv_slope(points, v_diff);
                        if let Some(i) = v1_idx {
                            jacobian[(i, i)] += g;
                            if let Some(j) = v2_idx {
                                jacobian[(i, j)] -= g;
                            }
                        }
                        if let Some(j) = v2_idx {
                            jacobian[(j, j)] += g;
                            if let Some(i) = v1_idx {
                                jacobian[(j, i)] -= g;
                            }
                        }
                    }

                    // ∂i/∂v = ctr·i_ctrl·sech²(u)/v_knee on the C/E rows;
                    // ∂i/∂(ctrl var) couples the collector KCL to the IRED
                    // current variable (exp(w)·… in the IRED's log space).
                    ComponentEquation::PhotoCoupled { ctr, v_knee, ctrl_edge } => {
                        let ci = self.get_current_var(*ctrl_edge);
                        let (i_ctrl, di_dw) = ci
                            .map(|ci| match variables[ci].space {
                                VariableSpace::Logarithmic => {
                                    let i = x[ci].exp();
                                    (i, i) // d(exp w)/dw = exp w
                                }
                                VariableSpace::Linear => (x[ci], 1.0),
                            })
                            .unwrap_or((0.0, 0.0));
                        let t = (v_diff / v_knee).tanh();
                        let g = ctr * i_ctrl * (1.0 - t * t) / v_knee;
                        if let Some(i) = v1_idx {
                            jacobian[(i, i)] += g;
                            if let Some(j) = v2_idx {
                                jacobian[(i, j)] -= g;
                            }
                            if let Some(ci) = ci {
                                jacobian[(i, ci)] += ctr * di_dw * t;
                            }
                        }
                        if let Some(j) = v2_idx {
                            jacobian[(j, j)] += g;
                            if let Some(i) = v1_idx {
                                jacobian[(j, i)] -= g;
                            }
                            if let Some(ci) = ci {
                                jacobian[(j, ci)] -= ctr * di_dw * t;
                            }
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

        // gshunt diagonal — must match the residual stamp exactly.
        for i in 0..self.num_vars {
            if let Some(VariableElement::NodeVoltage(_)) = self.var_to_element.get(&i) {
                jacobian[(i, i)] += GSHUNT;
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
                } else if let ComponentEquation::TableIV { points } = equation {
                    let (n1, n2) = system.circuit.branch_nodes(edge_idx).unwrap();
                    let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                    let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                    branch_currents.insert(edge_idx, table_iv_interp(points, v1 - v2));
                } else if let ComponentEquation::PhotoCoupled { ctr, v_knee, ctrl_edge } = equation {
                    // Controlling IRED currents were extracted in the first
                    // pass (LED branches own current variables).
                    let i_ctrl = branch_currents.get(ctrl_edge).copied().unwrap_or(0.0);
                    let (n1, n2) = system.circuit.branch_nodes(edge_idx).unwrap();
                    let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                    let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                    branch_currents.insert(edge_idx, ctr * i_ctrl * ((v1 - v2) / v_knee).tanh());
                }
            }
        }
    }
    
    (node_voltages, branch_currents)
}