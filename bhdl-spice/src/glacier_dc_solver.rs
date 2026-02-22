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
    
    /// Solve with source-stepping continuation for nonlinear circuits.
    ///
    /// Uses adaptive source ramping where each converged solution becomes the
    /// initial guess for the next ramp level.  When a step fails, the ramp
    /// increment is halved and retried from the last converged point — this
    /// is the standard SPICE "source stepping" homotopy with adaptive step
    /// size control.
    fn solve_with_ramping(&self, circuit: Circuit) -> Result<DcAnalysisResult> {
        info!("Starting ramped GLACIER DC analysis (adaptive continuation)");

        let mut equation_system = SpiceEquationSystem::new(circuit.clone())?;
        let mut variables = equation_system.create_variables();

        // Seed with a small initial ramp
        let initial_ramp = 0.05;
        equation_system.set_voltage_ramp(initial_ramp);
        equation_system.get_initial_guess_with_ramp(&mut variables, initial_ramp);

        let mut current_ramp = 0.0_f64;
        let mut step_size = 0.05_f64; // Start with 5% steps
        let min_step = 0.001; // Minimum 0.1% step
        let mut total_iters: usize = 0;

        while current_ramp < 1.0 {
            let target_ramp = (current_ramp + step_size).min(1.0);
            debug!("Continuation step: ramp = {:.4} (step = {:.4})", target_ramp, step_size);

            equation_system.set_voltage_ramp(target_ramp);

            // Save variable state so we can revert on failure
            let saved_values: Vec<f64> = variables.iter().map(|v| v.value).collect();

            let mut step_config = self.config.clone();
            if target_ramp < 1.0 {
                step_config.tolerance = (self.config.tolerance * 100.0).max(1e-6);
                step_config.max_iterations = step_config.max_iterations.max(200);
            }
            let mut solver = GenericGlacierSolver::new(step_config);

            match solver.solve(&mut variables, &equation_system) {
                Ok(stats) => {
                    info!(
                        "Converged at ramp = {:.4} in {} iters (error {:.2e})",
                        target_ramp, stats.iterations, stats.final_error
                    );
                    total_iters += stats.iterations;
                    current_ramp = target_ramp;

                    // At full voltage — done!
                    if current_ramp >= 1.0 {
                        let (node_voltages, branch_currents) =
                            extract_solution(&equation_system, &variables);
                        let total_power = calculate_total_power(
                            &equation_system.circuit,
                            &node_voltages,
                            &branch_currents,
                        );
                        return Ok(DcAnalysisResult {
                            node_voltages,
                            branch_currents,
                            total_power,
                            iterations: total_iters,
                            final_error: stats.final_error,
                        });
                    }

                    // Success — try increasing step size for next iteration
                    if stats.iterations <= 5 {
                        step_size = (step_size * 1.5).min(0.10);
                    }
                }
                Err(_e) => {
                    // Revert to last converged state
                    for (i, var) in variables.iter_mut().enumerate() {
                        var.value = saved_values[i];
                    }

                    if step_size > min_step {
                        // Halve the step and retry
                        step_size *= 0.5;
                        debug!(
                            "Step failed at ramp = {:.4}, reducing step to {:.4}",
                            target_ramp, step_size
                        );
                        continue;
                    } else {
                        warn!(
                            "Failed at ramp = {:.4} with minimum step size {:.4}",
                            target_ramp, min_step
                        );
                        break;
                    }
                }
            }
        }

        Err(SpiceError::ConvergenceFailed(self.config.max_iterations))
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