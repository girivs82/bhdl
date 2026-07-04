//! Test pseudo-transient method for DC analysis
//! This treats the DC problem as a transient with large time steps

use anyhow::Result;
use nalgebra::{DMatrix, DVector};
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, SpiceError};

/// Simple pseudo-transient solver
struct PseudoTransientSolver {
    circuit: Circuit,
    models: std::collections::HashMap<String, ComponentModel>,
}

impl PseudoTransientSolver {
    fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: std::collections::HashMap::new(),
        }
    }
    
    fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Solve using pseudo-transient method
    /// Add artificial capacitance to each node and solve as transient
    fn solve(&mut self) -> Result<DVector<f64>> {
        println!("Starting pseudo-transient analysis...\n");
        
        // Get circuit info
        let ground_idx = self.circuit.ground_node()
            .ok_or_else(|| SpiceError::NoGroundNode)?
            .0;
            
        let node_list: Vec<_> = self.circuit.nodes()
            .filter(|&(idx, _)| idx != ground_idx)
            .map(|(idx, _)| idx)
            .collect();
            
        let num_nodes = node_list.len();
        let voltage_sources: Vec<_> = self.circuit.branches()
            .filter_map(|(idx, branch)| {
                match self.models.get(&branch.name)? {
                    ComponentModel::VoltageSource { voltage, .. } => 
                        Some((idx, branch.name.clone(), *voltage)),
                    _ => None
                }
            })
            .collect();
        let num_vsources = voltage_sources.len();
        let matrix_size = num_nodes + num_vsources;
        
        // Initialize with small voltages (not zero to avoid singular derivatives)
        let mut x = DVector::from_element(matrix_size, 0.1);
        
        // Pseudo-transient parameters
        let mut pseudo_time = 0.0;
        let initial_dt = 1e-9;  // Start with small time step
        let mut dt = initial_dt;
        let max_dt = 1.0;      // Maximum time step
        let tol = 1e-9;        // Convergence tolerance
        let tau = 1e-6;        // Time constant (artificial capacitance)
        
        // Add artificial capacitance to each node (C = tau)
        // This makes dV/dt = (I - I_eq) / C
        
        let mut iteration = 0;
        let max_iterations = 1000;
        
        while iteration < max_iterations {
            iteration += 1;
            
            // Build system including capacitive terms
            let mut jacobian = DMatrix::zeros(matrix_size, matrix_size);
            let mut residual = DVector::zeros(matrix_size);
            
            // Standard MNA stamps
            self.stamp_linear_elements(&mut jacobian, &mut residual, &x, &node_list, ground_idx)?;
            self.stamp_nonlinear_elements(&mut jacobian, &mut residual, &x, &node_list, ground_idx)?;
            self.stamp_voltage_sources(&mut jacobian, &mut residual, &x, &voltage_sources, &node_list)?;
            
            // Add pseudo-transient terms: C * dV/dt to each node
            // Using backward Euler: V_new = V_old + dt * dV/dt
            // Rearranged: (1/dt) * C * (V_new - V_old) + I(V_new) = 0
            let capacitance_term = tau / dt;
            
            // Store old x for comparison
            let x_old = x.clone();
            
            // Add capacitive terms to diagonal of Jacobian
            for i in 0..num_nodes {
                jacobian[(i, i)] += capacitance_term;
                // Add C/dt * V_old to residual
                residual[i] -= capacitance_term * x_old[i];
            }
            
            // Solve the system
            let neg_residual = -residual.clone();
            match jacobian.lu().solve(&neg_residual) {
                Some(delta_x) => {
                    // Check convergence before updating
                    let max_change = delta_x.iter().map(|&v| v.abs()).fold(0.0, f64::max);
                    let max_residual = residual.iter().map(|&v| v.abs()).fold(0.0, f64::max);
                    
                    // Update solution
                    x += delta_x;
                    
                    if iteration % 10 == 0 || max_change < tol {
                        println!("Iter {}: t={:.2e}, dt={:.2e}, max_change={:.2e}, residual={:.2e}", 
                                 iteration, pseudo_time, dt, max_change, max_residual);
                    }
                    
                    // Check for steady state
                    if max_change < tol && max_residual < tol {
                        println!("\n✅ Converged to steady state!");
                        break;
                    }
                    
                    // Adaptive time stepping
                    if max_change < 0.1 {
                        // Good convergence, increase time step
                        dt = (dt * 1.5).min(max_dt);
                    } else if max_change > 0.5 {
                        // Poor convergence, decrease time step
                        dt = (dt * 0.5).max(initial_dt);
                    }
                    
                    pseudo_time += dt;
                }
                None => {
                    println!("Failed to solve at iteration {}", iteration);
                    // Reduce time step and retry
                    dt *= 0.1;
                    if dt < 1e-15 {
                        return Err(anyhow::anyhow!("Time step too small"));
                    }
                }
            }
        }
        
        if iteration >= max_iterations {
            println!("⚠️  Maximum iterations reached");
        }
        
        Ok(x)
    }
    
    // Simplified stamping functions (would be more complete in real implementation)
    fn stamp_linear_elements(&self, jacobian: &mut DMatrix<f64>, _residual: &mut DVector<f64>, 
                            _x: &DVector<f64>, node_list: &[petgraph::graph::NodeIndex], 
                            ground_idx: petgraph::graph::NodeIndex) -> Result<()> {
        // Stamp resistors
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some(ComponentModel::Resistor { resistance, .. }) = self.models.get(&branch.name) {
                let (n1, n2) = self.circuit.branch_nodes(edge_idx).unwrap();
                let g = 1.0 / resistance;
                
                // Get matrix indices
                let i1 = if n1 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n1)
                };
                let i2 = if n2 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n2)
                };
                
                // Stamp conductance
                if let Some(i) = i1 {
                    jacobian[(i, i)] += g;
                    if let Some(j) = i2 {
                        jacobian[(i, j)] -= g;
                    }
                }
                if let Some(i) = i2 {
                    jacobian[(i, i)] += g;
                    if let Some(j) = i1 {
                        jacobian[(i, j)] -= g;
                    }
                }
            }
        }
        Ok(())
    }
    
    fn stamp_nonlinear_elements(&self, jacobian: &mut DMatrix<f64>, residual: &mut DVector<f64>,
                               x: &DVector<f64>, node_list: &[petgraph::graph::NodeIndex],
                               ground_idx: petgraph::graph::NodeIndex) -> Result<()> {
        // Stamp LEDs using exponential model
        for (edge_idx, branch) in self.circuit.branches() {
            if let Some(ComponentModel::LED { saturation_current, emission_coefficient, 
                                             thermal_voltage, .. }) = self.models.get(&branch.name) {
                let is = saturation_current.unwrap_or(1e-12);
                let n = emission_coefficient.unwrap_or(2.0);
                let vt = thermal_voltage.unwrap_or(0.026);
                
                let (n1, n2) = self.circuit.branch_nodes(edge_idx).unwrap();
                
                // Get node voltages
                let v1 = if n1 == ground_idx { 0.0 } else {
                    node_list.iter().position(|&n| n == n1)
                        .map(|i| x[i]).unwrap_or(0.0)
                };
                let v2 = if n2 == ground_idx { 0.0 } else {
                    node_list.iter().position(|&n| n == n2)
                        .map(|i| x[i]).unwrap_or(0.0)
                };
                
                let v_diode = v1 - v2;
                
                // Exponential model with linearization
                let (i_diode, g_diode) = if v_diode > 0.0 {
                    let exp_term = (v_diode / (n * vt)).min(50.0).exp();
                    let i = is * (exp_term - 1.0);
                    let g = is * exp_term / (n * vt);
                    (i, g)
                } else {
                    // Reverse bias - small leakage
                    (is, 1e-12)
                };
                
                // Get matrix indices
                let i1 = if n1 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n1)
                };
                let i2 = if n2 == ground_idx { None } else {
                    node_list.iter().position(|&n| n == n2)
                };
                
                // Stamp linearized model: I = g*V + (I - g*V)
                let i_eq = i_diode - g_diode * v_diode;
                
                if let Some(i) = i1 {
                    jacobian[(i, i)] += g_diode;
                    residual[i] -= i_eq;
                    if let Some(j) = i2 {
                        jacobian[(i, j)] -= g_diode;
                    }
                }
                if let Some(i) = i2 {
                    jacobian[(i, i)] += g_diode;
                    residual[i] += i_eq;
                    if let Some(j) = i1 {
                        jacobian[(i, j)] -= g_diode;
                    }
                }
            }
        }
        Ok(())
    }
    
    fn stamp_voltage_sources(&self, jacobian: &mut DMatrix<f64>, residual: &mut DVector<f64>,
                            x: &DVector<f64>, voltage_sources: &[(petgraph::graph::EdgeIndex, String, f64)],
                            node_list: &[petgraph::graph::NodeIndex]) -> Result<()> {
        let num_nodes = node_list.len();
        
        for (idx, (edge_idx, _name, voltage)) in voltage_sources.iter().enumerate() {
            let (n1, n2) = self.circuit.branch_nodes(*edge_idx).unwrap();
            let ground_idx = self.circuit.ground_node().unwrap().0;
            
            let i1 = if n1 == ground_idx { None } else {
                node_list.iter().position(|&n| n == n1)
            };
            let i2 = if n2 == ground_idx { None } else {
                node_list.iter().position(|&n| n == n2)
            };
            
            let vsource_idx = num_nodes + idx;
            
            // KCL equations
            if let Some(i) = i1 {
                jacobian[(i, vsource_idx)] = 1.0;
                jacobian[(vsource_idx, i)] = 1.0;
            }
            if let Some(i) = i2 {
                jacobian[(i, vsource_idx)] = -1.0;
                jacobian[(vsource_idx, i)] = -1.0;
            }
            
            // Voltage constraint
            residual[vsource_idx] = -*voltage;
            if let Some(i) = i1 {
                residual[vsource_idx] += x[i];
            }
            if let Some(i) = i2 {
                residual[vsource_idx] -= x[i];
            }
        }
        Ok(())
    }
}

fn main() -> Result<()> {
    println!("=== Pseudo-Transient Method Test ===");
    println!("\nTesting 2 LEDs in series using pseudo-transient method");
    println!("This should naturally find the symmetric solution\n");
    
    // Create 2 LEDs in series circuit
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = PseudoTransientSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    let led_model = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-13),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    solver.add_model("D1".to_string(), led_model.clone());
    solver.add_model("D2".to_string(), led_model);
    
    match solver.solve() {
        Ok(x) => {
            println!("\nResults:");
            println!("  V_in = {:.3}V", x[0]);
            println!("  V_n1 = {:.3}V", x[1]);
            println!("  V_n2 = {:.3}V", x[2]);
            println!("  I_source = {:.3}mA", x[3] * 1000.0);
            
            let v_r = x[0] - x[1];
            let v_led1 = x[1] - x[2];
            let v_led2 = x[2];
            let i_circuit = v_r / 330.0;
            
            println!("\nComponent analysis:");
            println!("  V_R = {:.3}V", v_r);
            println!("  V_LED1 = {:.3}V", v_led1);
            println!("  V_LED2 = {:.3}V", v_led2);
            println!("  Circuit current = {:.3}mA", i_circuit * 1000.0);
            
            // Check symmetry
            let symmetry_error = (v_led1 - v_led2).abs();
            println!("\nSymmetry check:");
            println!("  LED voltage difference: {:.2e}V", symmetry_error);
            if symmetry_error < 0.01 {
                println!("  ✅ Symmetric solution found!");
            } else {
                println!("  ⚠️  Asymmetric solution");
            }
        }
        Err(e) => {
            println!("❌ Failed: {}", e);
        }
    }
    
    Ok(())
}