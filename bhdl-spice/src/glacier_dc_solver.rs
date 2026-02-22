//! GLACIER DC Solver - Clean interface between generic solver and SPICE
//! 
//! This module provides a clean API for DC analysis using the generic
//! GLACIER solver with SPICE-specific equation systems.

use std::collections::HashMap;
use petgraph::graph::{NodeIndex, EdgeIndex};
use log::{info, debug, warn};

use crate::{
    circuit::Circuit,
    errors::{SpiceError, Result},
    generic_glacier_solver::{GenericGlacierSolver, SolverConfig, Variable},
    spice_equation_system::{SpiceEquationSystem, extract_solution},
};

/// DC analysis result
#[derive(Debug, Clone)]
pub struct DcAnalysisResult {
    /// Voltage at each node
    pub node_voltages: HashMap<NodeIndex, f64>,
    /// Current through each branch
    pub branch_currents: HashMap<EdgeIndex, f64>,
    /// Total power dissipation
    pub total_power: f64,
    /// Convergence iterations
    pub iterations: usize,
    /// Final residual error
    pub final_error: f64,
}

/// DC solver using GLACIER numerical engine
pub struct GlacierDcSolver {
    /// Solver configuration
    config: SolverConfig,
}

impl GlacierDcSolver {
    /// Create a new DC solver with default configuration
    pub fn new() -> Self {
        Self {
            config: SolverConfig::default(),
        }
    }
    
    /// Create with custom configuration
    pub fn with_config(config: SolverConfig) -> Self {
        Self { config }
    }
    
    /// Solve DC operating point
    pub fn solve(&self, circuit: Circuit) -> Result<DcAnalysisResult> {
        // Check if circuit has nonlinear elements
        let has_nonlinear = circuit.branches().any(|(_, b)| 
            b.component_type == "LED" || b.component_type == "Diode"
        );
        
        if has_nonlinear {
            // For nonlinear circuits, use ramping approach
            info!("Circuit has nonlinear elements, using ramping approach");
            self.solve_with_ramping(circuit)
        } else {
            // For linear circuits, try direct solve
            self.solve_direct(circuit)
        }
    }
    
    /// Direct solve without ramping
    fn solve_direct(&self, circuit: Circuit) -> Result<DcAnalysisResult> {
        info!("Starting GLACIER DC analysis");
        debug!("Circuit has {} nodes and {} branches", 
               circuit.nodes().count(), circuit.branches().count());
        
        // Create SPICE equation system
        let equation_system = SpiceEquationSystem::new(circuit)?;
        
        // Create variables
        let mut variables = equation_system.create_variables();
        debug!("Created {} variables", variables.len());
        
        // Set initial guess
        equation_system.get_initial_guess(&mut variables);
        debug!("Initial guess set");
        
        // Create numerical solver
        let mut solver = GenericGlacierSolver::new(self.config.clone());
        
        // Solve the system
        let stats = solver.solve(&mut variables, &equation_system)?;
        
        info!("Converged in {} iterations with error {:.2e}", 
              stats.iterations, stats.final_error);
        
        // Extract solution
        let (node_voltages, branch_currents) = extract_solution(&equation_system, &variables);
        
        // Calculate total power
        let total_power = calculate_total_power(&equation_system.circuit, 
                                               &node_voltages, 
                                               &branch_currents);
        
        Ok(DcAnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: stats.iterations,
            final_error: stats.final_error,
        })
    }
    
    /// Solve with ramping approach for difficult circuits
    fn solve_with_ramping(&self, circuit: Circuit) -> Result<DcAnalysisResult> {
        info!("Starting ramped GLACIER DC analysis");
        
        // Ramp points to try
        let ramp_points = vec![0.1, 0.2, 0.3, 0.5, 0.7, 0.9, 1.0];
        let mut best_result = None;
        let mut best_error = f64::INFINITY;
        
        for &ramp in &ramp_points {
            debug!("Trying ramp = {:.1}", ramp);
            
            // Create equation system
            let mut equation_system = SpiceEquationSystem::new(circuit.clone())?;
            
            // Set voltage ramp
            equation_system.set_voltage_ramp(ramp);
            
            // Create variables
            let mut variables = equation_system.create_variables();
            
            // Set initial guess with ramp
            equation_system.get_initial_guess_with_ramp(&mut variables, ramp);
            
            // Create numerical solver with relaxed tolerance for intermediate steps
            let mut config = self.config.clone();
            if ramp < 1.0 {
                config.tolerance *= 10.0; // Relax tolerance for intermediate steps
            }
            let mut solver = GenericGlacierSolver::new(config);
            
            // Try to solve
            match solver.solve(&mut variables, &equation_system) {
                Ok(stats) => {
                    info!("Converged at ramp = {:.1} with error {:.2e}", ramp, stats.final_error);
                    
                    // Only store as best result if it's at full voltage
                    if ramp == 1.0 && stats.final_error < best_error {
                        best_error = stats.final_error;
                        
                        // Extract solution
                        let (node_voltages, branch_currents) = extract_solution(&equation_system, &variables);
                        let total_power = calculate_total_power(&equation_system.circuit, 
                                                               &node_voltages, 
                                                               &branch_currents);
                        
                        best_result = Some(DcAnalysisResult {
                            node_voltages,
                            branch_currents,
                            total_power,
                            iterations: stats.iterations,
                            final_error: stats.final_error,
                        });
                    }
                    
                    // If we found a good solution at ramp < 1.0, use it as initial guess for full solve
                    if ramp < 1.0 && stats.final_error < 1e-6 {
                        debug!("Using ramp = {:.1} solution as initial guess for full solve", ramp);
                        
                        // Set full voltage ramp
                        equation_system.set_voltage_ramp(1.0);
                        
                        // Create new variables for full solve
                        let mut full_variables = equation_system.create_variables();
                        
                        // Copy converged values as initial guess
                        for (i, var) in full_variables.iter_mut().enumerate() {
                            var.value = variables[i].value;
                        }
                        
                        // Solve at full voltage with tighter tolerance
                        let mut full_solver = GenericGlacierSolver::new(self.config.clone());
                        
                        match full_solver.solve(&mut full_variables, &equation_system) {
                            Ok(full_stats) => {
                                info!("Full solve converged with error {:.2e}", full_stats.final_error);
                                
                                let (node_voltages, branch_currents) = extract_solution(&equation_system, &full_variables);
                                let total_power = calculate_total_power(&equation_system.circuit, 
                                                                       &node_voltages, 
                                                                       &branch_currents);
                                
                                return Ok(DcAnalysisResult {
                                    node_voltages,
                                    branch_currents,
                                    total_power,
                                    iterations: full_stats.iterations,
                                    final_error: full_stats.final_error,
                                });
                            }
                            Err(e) => {
                                debug!("Full solve failed: {}", e);
                            }
                        }
                    }
                }
                Err(e) => {
                    debug!("Failed at ramp = {:.1}: {}", ramp, e);
                    
                    // If we fail at ramp=1.0 but had success at lower ramps, that's concerning
                    if ramp == 1.0 && best_error < 1e-3 {
                        warn!("Failed at full voltage despite converging at lower ramps");
                    }
                }
            }
        }
        
        // Return best result found, but only if it's at full voltage
        if let Some(result) = best_result {
            // Check if we have the full solution
            if best_error < 1e-6 {
                Ok(result)
            } else {
                Err(SpiceError::ConvergenceFailed(self.config.max_iterations))
            }
        } else {
            Err(SpiceError::ConvergenceFailed(self.config.max_iterations))
        }
    }
}

/// Calculate total power dissipation
fn calculate_total_power(
    circuit: &Circuit,
    node_voltages: &HashMap<NodeIndex, f64>,
    branch_currents: &HashMap<EdgeIndex, f64>,
) -> f64 {
    let mut total_power = 0.0;
    
    for (edge_idx, branch) in circuit.branches() {
        if let Some(&current) = branch_currents.get(&edge_idx) {
            let (n1, n2) = circuit.branch_nodes(edge_idx).unwrap();
            let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
            let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
            let voltage = (v1 - v2).abs();
            
            // Power = V * I
            let power = voltage * current.abs();
            
            // Only count dissipated power (not supplied)
            match branch.component_type.as_str() {
                "VoltageSource" | "CurrentSource" => {
                    // Sources supply power, don't count as dissipation
                }
                _ => {
                    total_power += power;
                }
            }
        }
    }
    
    total_power
}

/// Builder for configuring DC analysis
pub struct DcAnalysisBuilder {
    config: SolverConfig,
}

impl DcAnalysisBuilder {
    pub fn new() -> Self {
        Self {
            config: SolverConfig::default(),
        }
    }
    
    pub fn max_iterations(mut self, iterations: usize) -> Self {
        self.config.max_iterations = iterations;
        self
    }
    
    pub fn tolerance(mut self, tol: f64) -> Self {
        self.config.tolerance = tol;
        self
    }
    
    pub fn enable_adaptive_damping(mut self, enable: bool) -> Self {
        self.config.use_adaptive_damping = enable;
        self
    }
    
    pub fn build(self) -> GlacierDcSolver {
        GlacierDcSolver::with_config(self.config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Circuit;
    use crate::circuit::{META_PARENT_INSTANCE, META_DECOMPOSITION_ROLE};

    /// Helper: build metadata for a decomposed regulator branch
    fn reg_meta(parent: &str, role: &str) -> HashMap<String, String> {
        let mut m = HashMap::new();
        m.insert(META_PARENT_INSTANCE.to_string(), parent.to_string());
        m.insert(META_DECOMPOSITION_ROLE.to_string(), role.to_string());
        m
    }
    
    #[test]
    fn test_regulator_circuit_with_diodes() {
        // Replicates the exact topology from IntentLayoutDemo:
        // VIN (12V) → regulated → reg_dropout (4Ω) → VOUT
        // reg_vout: VoltageSource 5V on VOUT→GND
        // TVS diode: GND→regulated (reverse biased at 12V)
        // c1: regulated→GND (cap, DC open)
        // c2: VOUT→GND (cap, DC open)
        // r1: VOUT→sensed (330Ω)
        // sense: sensed→net_sense (0.1Ω)
        // LED: net_sense→GND (forward biased)
        let _ = env_logger::builder().is_test(true).try_init();

        let mut circuit = Circuit::new();

        // Voltage sources
        circuit.add_branch("VIN".into(), "regulated", "GND", "VoltageSource".into(), 12.0, None);
        circuit.add_branch_with_metadata("reg_vout".into(), "VOUT", "GND", "VoltageSource".into(), 5.0, None, reg_meta("reg", "vout"));

        // Regulator dropout path
        circuit.add_branch_with_metadata("reg_dropout".into(), "regulated", "VOUT", "Resistor".into(), 4.0, None, reg_meta("reg", "dropout"));

        // TVS diode: anode=GND, cathode=regulated (reverse biased)
        circuit.add_branch("tvs".into(), "GND", "regulated", "Diode".into(), 1.0, None);

        // Capacitors (DC: open circuit)
        circuit.add_branch("c1".into(), "regulated", "GND", "Capacitor".into(), 100.0, None);
        circuit.add_branch("c2".into(), "VOUT", "GND", "Capacitor".into(), 10.0, None);

        // Load path
        circuit.add_branch("r1".into(), "VOUT", "sensed", "Resistor".into(), 330.0, None);
        circuit.add_branch("sense".into(), "sensed", "net_sense", "Resistor".into(), 0.1, None);
        circuit.add_branch("led".into(), "net_sense", "GND", "LED".into(), 1.0, None);

        let solver = GlacierDcSolver::new();
        let result = solver.solve(circuit);

        match &result {
            Ok(r) => {
                println!("Converged in {} iterations, error={:.2e}", r.iterations, r.final_error);
                for (node_idx, v) in &r.node_voltages {
                    println!("  Node {:?} = {:.4}V", node_idx, v);
                }
                for (edge_idx, i) in &r.branch_currents {
                    println!("  Branch {:?} = {:.6}A ({:.4}mA)", edge_idx, i, i * 1000.0);
                }
            }
            Err(e) => {
                println!("FAILED: {}", e);
            }
        }

        assert!(result.is_ok(), "Circuit should converge");
    }

    #[test]
    fn test_regulator_no_diodes() {
        // Same circuit but WITHOUT diodes to test if diodes cause convergence failure
        let _ = env_logger::builder().is_test(true).try_init();
        let mut circuit = Circuit::new();

        circuit.add_branch("VIN".into(), "regulated", "GND", "VoltageSource".into(), 12.0, None);
        circuit.add_branch_with_metadata("reg_vout".into(), "VOUT", "GND", "VoltageSource".into(), 5.0, None, reg_meta("reg", "vout"));
        circuit.add_branch_with_metadata("reg_dropout".into(), "regulated", "VOUT", "Resistor".into(), 4.0, None, reg_meta("reg", "dropout"));
        circuit.add_branch("r1".into(), "VOUT", "sensed", "Resistor".into(), 330.0, None);
        circuit.add_branch("sense".into(), "sensed", "net_sense", "Resistor".into(), 0.1, None);
        // Replace LED with a resistor to get equivalent load
        circuit.add_branch("rled".into(), "net_sense", "GND", "Resistor".into(), 200.0, None);

        let solver = GlacierDcSolver::new();
        let result = solver.solve(circuit);
        match &result {
            Ok(r) => println!("No-diode: Converged {} iters, err={:.2e}", r.iterations, r.final_error),
            Err(e) => println!("No-diode FAILED: {}", e),
        }
        assert!(result.is_ok(), "Linear circuit should converge");
    }

    #[test]
    fn test_regulator_led_only() {
        // Two voltage sources + dropout + LED load, no TVS diode
        let _ = env_logger::builder().is_test(true).try_init();
        let mut circuit = Circuit::new();

        circuit.add_branch("VIN".into(), "regulated", "GND", "VoltageSource".into(), 12.0, None);
        circuit.add_branch_with_metadata("reg_vout".into(), "VOUT", "GND", "VoltageSource".into(), 5.0, None, reg_meta("reg", "vout"));
        circuit.add_branch_with_metadata("reg_dropout".into(), "regulated", "VOUT", "Resistor".into(), 4.0, None, reg_meta("reg", "dropout"));
        circuit.add_branch("r1".into(), "VOUT", "sensed", "Resistor".into(), 330.0, None);
        circuit.add_branch("sense".into(), "sensed", "net_sense", "Resistor".into(), 0.1, None);
        circuit.add_branch("led".into(), "net_sense", "GND", "LED".into(), 1.0, None);

        let solver = GlacierDcSolver::new();
        let result = solver.solve(circuit);
        match &result {
            Ok(r) => {
                println!("LED-only: Converged {} iters, err={:.2e}", r.iterations, r.final_error);
                for (ni, v) in &r.node_voltages { println!("  {:?} = {:.4}V", ni, v); }
                for (ei, i) in &r.branch_currents { println!("  {:?} = {:.4}mA", ei, i*1000.0); }
            }
            Err(e) => println!("LED-only FAILED: {}", e),
        }
        assert!(result.is_ok(), "LED-only circuit should converge");
    }

    #[test]
    fn test_regulator_high_dropout() {
        // Full circuit with 1000Ω dropout (previously worked)
        let _ = env_logger::builder().is_test(true).try_init();
        let mut circuit = Circuit::new();

        circuit.add_branch("VIN".into(), "regulated", "GND", "VoltageSource".into(), 12.0, None);
        circuit.add_branch_with_metadata("reg_vout".into(), "VOUT", "GND", "VoltageSource".into(), 5.0, None, reg_meta("reg", "vout"));
        circuit.add_branch_with_metadata("reg_dropout".into(), "regulated", "VOUT", "Resistor".into(), 1000.0, None, reg_meta("reg", "dropout"));
        circuit.add_branch("tvs".into(), "GND", "regulated", "Diode".into(), 1.0, None);
        circuit.add_branch("c1".into(), "regulated", "GND", "Capacitor".into(), 100.0, None);
        circuit.add_branch("c2".into(), "VOUT", "GND", "Capacitor".into(), 10.0, None);
        circuit.add_branch("r1".into(), "VOUT", "sensed", "Resistor".into(), 330.0, None);
        circuit.add_branch("sense".into(), "sensed", "net_sense", "Resistor".into(), 0.1, None);
        circuit.add_branch("led".into(), "net_sense", "GND", "LED".into(), 1.0, None);

        let solver = GlacierDcSolver::new();
        let result = solver.solve(circuit);
        match &result {
            Ok(r) => println!("High-dropout: Converged {} iters, err={:.2e}", r.iterations, r.final_error),
            Err(e) => println!("High-dropout FAILED: {}", e),
        }
        assert!(result.is_ok(), "High dropout circuit should converge");
    }

    #[test]
    fn test_simple_resistor_divider() {
        // Create voltage divider: 5V -> 1k -> 1k -> GND
        let mut circuit = Circuit::new();
        
        circuit.add_branch(
            "V1".to_string(),
            "vcc",
            "gnd",
            "VoltageSource".to_string(),
            5.0,
            None,
        );
        
        circuit.add_branch(
            "R1".to_string(),
            "vcc",
            "mid",
            "Resistor".to_string(),
            1000.0,
            None,
        );
        
        circuit.add_branch(
            "R2".to_string(),
            "mid",
            "gnd",
            "Resistor".to_string(),
            1000.0,
            None,
        );
        
        let solver = GlacierDcSolver::new();
        let result = solver.solve(circuit).unwrap();
        
        // Check midpoint voltage (should be 2.5V)
        let mid_node = result.node_voltages.iter()
            .find(|(_, &v)| (v - 2.5).abs() < 0.01)
            .expect("Should find midpoint voltage");
        
        assert!((mid_node.1 - 2.5).abs() < 0.01);
        assert!(result.iterations < 10);  // Should converge quickly
    }
}