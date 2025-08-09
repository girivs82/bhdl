//! Debug solver convergence issues

use anyhow::Result;
use bhdl_spice::{
    circuit::Circuit,
    generic_glacier_solver::{GenericGlacierSolver, SolverConfig, EquationSystem},
    spice_equation_system::SpiceEquationSystem,
};

fn create_simple_resistor_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Simple voltage divider - should converge easily
    let _vdd = circuit.add_node("VDD".to_string(), None);
    let _mid = circuit.add_node("MID".to_string(), None);
    let _gnd = circuit.add_node("GND".to_string(), None);
    
    // 5V source
    circuit.add_branch(
        "V1".to_string(),
        "VDD",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None
    );
    
    // R1 = 1kΩ from VDD to MID
    circuit.add_branch(
        "R1".to_string(),
        "VDD",
        "MID",
        "Resistor".to_string(),
        1000.0,
        None
    );
    
    // R2 = 1kΩ from MID to GND
    circuit.add_branch(
        "R2".to_string(),
        "MID",
        "GND",
        "Resistor".to_string(),
        1000.0,
        None
    );
    
    circuit
}

fn main() -> Result<()> {
    
    println!("\n=== Solver Debug Test ===\n");
    
    // Create simple circuit
    let circuit = create_simple_resistor_circuit();
    
    println!("Test circuit: Simple voltage divider");
    println!("Expected: VDD=5V, MID=2.5V, GND=0V, I=2.5mA\n");
    
    // Create equation system
    let mut equation_system = SpiceEquationSystem::new(circuit.clone())?;
    equation_system.set_voltage_ramp(1.0); // Full voltage
    let mut variables = equation_system.create_variables();
    
    println!("Variables:");
    for var in &variables {
        println!("  [{}] {}: {} (space: {:?})", var.id, var.name, var.value, var.space);
    }
    
    // Check equation evaluation at expected solution
    println!("\nChecking equations at expected solution:");
    variables[0].value = 5.0;  // v_VDD
    variables[1].value = 2.5;  // v_MID
    variables[2].value = 0.0025; // i_V1 (2.5mA)
    
    let residuals = equation_system.evaluate_residuals(&variables);
    println!("Residuals at expected solution:");
    for (i, r) in residuals.iter().enumerate() {
        println!("  [{}] {}: {:.2e}", i, variables[i].name, r);
    }
    
    // Reset and try to solve
    println!("\nResetting to initial values and solving...");
    variables[0].value = 0.0;
    variables[1].value = 0.0;
    variables[2].value = 0.0;
    
    let config = SolverConfig {
        max_iterations: 10,  // Just a few iterations to see what happens
        tolerance: 1e-9,
        use_adaptive_damping: true,
        min_damping: 1e-6,
        max_damping: 1.0,
        singular_perturbation: 1e-10,
        damping_factor: 0.7,
    };
    
    let mut solver = GenericGlacierSolver::new(config.clone());
    
    // Try to solve with limited iterations
    match solver.solve(&mut variables, &equation_system) {
        Ok(stats) => {
            println!("\nConverged in {} iterations, error: {:.2e}", stats.iterations, stats.final_error);
        }
        Err(e) => {
            println!("\nFailed after 10 iterations: {}", e);
        }
    }
    
    println!("\nFinal values:");
    for var in &variables {
        println!("  {}: {:.6}", var.name, var.value);
    }
    
    // Check final residuals
    let final_residuals = equation_system.evaluate_residuals(&variables);
    println!("\nFinal residuals:");
    for (i, r) in final_residuals.iter().enumerate() {
        println!("  [{}] {}: {:.2e}", i, variables[i].name, r);
    }
    
    // Try with more iterations
    println!("\n--- Trying with 100 iterations ---");
    variables[0].value = 0.0;
    variables[1].value = 0.0;
    variables[2].value = 0.0;
    
    let config2 = SolverConfig {
        max_iterations: 100,
        ..config
    };
    
    let mut solver2 = GenericGlacierSolver::new(config2);
    match solver2.solve(&mut variables, &equation_system) {
        Ok(stats) => {
            println!("Converged in {} iterations, error: {:.2e}", stats.iterations, stats.final_error);
            println!("\nFinal solution:");
            for var in &variables {
                println!("  {}: {:.6}", var.name, var.value);
            }
        }
        Err(e) => {
            println!("Still failed: {}", e);
            println!("\nValues after 100 iterations:");
            for var in &variables {
                println!("  {}: {:.6}", var.name, var.value);
            }
        }
    }
    
    Ok(())
}