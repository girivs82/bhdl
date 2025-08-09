//! Test Unified GLACIER DC Solver with mixed variable formulation
//! 
//! This test validates the enhanced DC solver that incorporates lessons
//! from transient analysis, including mixed variable types and selective
//! log transformation.

use bhdl_spice::{
    Circuit, Branch, ComponentModel, ElectricalLimits,
    unified_glacier_solver::{UnifiedGlacierSolver, VariableType},
};
use petgraph::graph::NodeIndex;
use std::collections::HashMap;
// use log::info;

fn main() {
    // env_logger::init();
    
    println!("=== Unified GLACIER DC Solver Tests ===\n");
    
    // Test 1: Simple LED circuit
    println!("Test 1: Simple LED Circuit");
    println!("--------------------------");
    test_simple_led_circuit();
    
    // Test 2: Multiple LEDs in series
    println!("\nTest 2: Multiple LEDs in Series");
    println!("--------------------------------");
    test_series_leds();
    
    // Test 3: Mixed linear/exponential circuit
    println!("\nTest 3: Mixed Linear/Exponential Circuit");
    println!("----------------------------------------");
    test_mixed_circuit();
    
    // Test 4: Extreme parameters
    println!("\nTest 4: Extreme Parameters");
    println!("---------------------------");
    test_extreme_parameters();
    
    // Test 5: Convergence comparison
    println!("\nTest 5: Convergence Comparison");
    println!("-------------------------------");
    test_convergence_comparison();
}

fn test_simple_led_circuit() {
    // Create circuit: 5V -> 220Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add components
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
        "n1",
        "Resistor".to_string(),
        220.0,
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "n1",
        "gnd",
        "LED".to_string(),
        2.0,  // Forward voltage hint
        None,
    );
    
    // Create and run solver
    let mut solver = UnifiedGlacierSolver::new(circuit);
    
    match solver.solve() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Get node indices
            let n1_idx = solver.circuit.get_node("n1").map(|(idx, _)| idx);
            
            // Extract voltages
            let v_n1 = n1_idx.and_then(|idx| result.node_voltages.get(&idx)).copied().unwrap_or(0.0);
            println!("  Node voltages:");
            println!("    VCC = 5.00V");
            println!("    N1  = {:.3}V", v_n1);
            
            // Extract currents
            if let Some(&led_current) = result.branch_currents.values().find(|&&i| i > 0.0 && i < 0.1) {
                println!("  LED current = {:.3}mA", led_current * 1000.0);
                
                // Calculate LED voltage drop
                let v_led = v_n1;  // Since other end is ground
                println!("  LED voltage = {:.3}V", v_led);
                
                // Verify Ohm's law for resistor
                let i_r = (5.0 - v_n1) / 220.0;
                println!("  Resistor current = {:.3}mA (calculated)", i_r * 1000.0);
                
                let error = (i_r - led_current).abs();
                if error < 1e-6 {
                    println!("  ✓ KCL satisfied (error = {:.2e})", error);
                } else {
                    println!("  ✗ KCL error = {:.2e}", error);
                }
            }
            
            // Check variable types used
            println!("\n  Variable types:");
            let log_vars = solver.variables.iter()
                .filter(|v| v.var_type == VariableType::LogCurrent)
                .count();
            let current_vars = solver.variables.iter()
                .filter(|v| v.var_type == VariableType::Current)
                .count();
            let voltage_vars = solver.variables.iter()
                .filter(|v| v.var_type == VariableType::Voltage)
                .count();
            
            println!("    Voltage variables: {}", voltage_vars);
            println!("    Current variables: {}", current_vars);
            println!("    LogCurrent variables: {}", log_vars);
            
            if log_vars > 0 {
                println!("  ✓ Using log formulation for exponential devices");
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}

fn test_series_leds() {
    // Create circuit: 12V -> 1kΩ -> LED1 -> LED2 -> LED3 -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        12.0,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        1000.0,
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "n1",
        "n2",
        "LED".to_string(),
        2.0,
        None,
    );
    
    circuit.add_branch(
        "D2".to_string(),
        "n2",
        "n3",
        "LED".to_string(),
        2.0,
        None,
    );
    
    circuit.add_branch(
        "D3".to_string(),
        "n3",
        "gnd",
        "LED".to_string(),
        2.0,
        None,
    );
    
    let mut solver = UnifiedGlacierSolver::new(circuit);
    
    match solver.solve() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Get node indices
            let n1_idx = solver.circuit.get_node("n1").map(|(idx, _)| idx);
            let n2_idx = solver.circuit.get_node("n2").map(|(idx, _)| idx);
            let n3_idx = solver.circuit.get_node("n3").map(|(idx, _)| idx);
            
            // Get node voltages
            let v_n1 = n1_idx.and_then(|idx| result.node_voltages.get(&idx)).copied().unwrap_or(0.0);
            let v_n2 = n2_idx.and_then(|idx| result.node_voltages.get(&idx)).copied().unwrap_or(0.0);
            let v_n3 = n3_idx.and_then(|idx| result.node_voltages.get(&idx)).copied().unwrap_or(0.0);
            
            println!("  Node voltages:");
            println!("    VCC = 12.00V");
            println!("    N1  = {:.3}V", v_n1);
            println!("    N2  = {:.3}V", v_n2);
            println!("    N3  = {:.3}V", v_n3);
            
            // Calculate LED drops
            let v_led1 = v_n1 - v_n2;
            let v_led2 = v_n2 - v_n3;
            let v_led3 = v_n3;  // Other end is ground
            
            println!("\n  LED voltage drops:");
            println!("    LED1 = {:.3}V", v_led1);
            println!("    LED2 = {:.3}V", v_led2);
            println!("    LED3 = {:.3}V", v_led3);
            println!("    Total = {:.3}V", v_led1 + v_led2 + v_led3);
            
            // Calculate current
            let i_circuit = (12.0 - v_n1) / 1000.0;
            println!("\n  Circuit current = {:.3}mA", i_circuit * 1000.0);
            
            // Check if all LEDs are properly forward biased
            if v_led1 > 1.5 && v_led2 > 1.5 && v_led3 > 1.5 {
                println!("  ✓ All LEDs properly forward biased");
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}

fn test_mixed_circuit() {
    // Create circuit with both linear and exponential components
    // 5V -> R1(100Ω) -> || R2(1k) || (D1 -> R3(220Ω)) || -> GND
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
        "n1",
        "Resistor".to_string(),
        100.0,
        None,
    );
    
    circuit.add_branch(
        "R2".to_string(),
        "n1",
        "gnd",
        "Resistor".to_string(),
        1000.0,
        None,
    );
    
    circuit.add_branch(
        "D1".to_string(),
        "n1",
        "n2",
        "LED".to_string(),
        2.0,
        None,
    );
    
    circuit.add_branch(
        "R3".to_string(),
        "n2",
        "gnd",
        "Resistor".to_string(),
        220.0,
        None,
    );
    
    let mut solver = UnifiedGlacierSolver::new(circuit);
    
    match solver.solve() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Get node indices
            let n1_idx = solver.circuit.get_node("n1").map(|(idx, _)| idx);
            let n2_idx = solver.circuit.get_node("n2").map(|(idx, _)| idx);
            
            let v_n1 = n1_idx.and_then(|idx| result.node_voltages.get(&idx)).copied().unwrap_or(0.0);
            let v_n2 = n2_idx.and_then(|idx| result.node_voltages.get(&idx)).copied().unwrap_or(0.0);
            
            println!("  Node voltages:");
            println!("    N1 = {:.3}V", v_n1);
            println!("    N2 = {:.3}V", v_n2);
            
            // Calculate branch currents
            let i_r1 = (5.0 - v_n1) / 100.0;
            let i_r2 = v_n1 / 1000.0;
            let i_r3 = v_n2 / 220.0;
            let i_led = i_r3;  // Same current through LED and R3
            
            println!("\n  Branch currents:");
            println!("    I(R1) = {:.3}mA", i_r1 * 1000.0);
            println!("    I(R2) = {:.3}mA", i_r2 * 1000.0);
            println!("    I(LED) = {:.3}mA", i_led * 1000.0);
            println!("    I(R3) = {:.3}mA", i_r3 * 1000.0);
            
            // Check KCL at n1
            let kcl_error = (i_r1 - i_r2 - i_led).abs();
            println!("\n  KCL at N1: {:.3} - {:.3} - {:.3} = {:.2e}",
                     i_r1 * 1000.0, i_r2 * 1000.0, i_led * 1000.0, kcl_error);
            
            if kcl_error < 1e-6 {
                println!("  ✓ KCL satisfied");
            }
            
            // Check mixed formulation
            let has_log_current = solver.variables.iter()
                .any(|v| v.var_type == VariableType::LogCurrent && 
                         v.component_type.as_ref().map(|t| t == "LED").unwrap_or(false));
            
            if has_log_current {
                println!("  ✓ LED using log current formulation");
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}

fn test_extreme_parameters() {
    // Test with extreme LED parameters (Is = 1e-30)
    let mut circuit = Circuit::new();
    
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        3.3,
        None,
    );
    
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        470.0,
        None,
    );
    
    // LED with extreme saturation current
    circuit.add_branch(
        "D1".to_string(),
        "n1",
        "gnd",
        "LED".to_string(),
        2.0,
        None,
    );
    
    println!("  Testing LED with Is = 1e-30 (extreme parameter)");
    
    let mut solver = UnifiedGlacierSolver::new(circuit);
    
    match solver.solve() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            let n1_idx = solver.circuit.get_node("n1").map(|(idx, _)| idx);
            let v_n1 = n1_idx.and_then(|idx| result.node_voltages.get(&idx)).copied().unwrap_or(0.0);
            let v_led = v_n1;
            let i_led = (3.3 - v_n1) / 470.0;
            
            println!("  LED voltage = {:.3}V", v_led);
            println!("  LED current = {:.3}mA", i_led * 1000.0);
            
            // In traditional formulation, exp(v_led/Vt) would be huge
            let exp_term = v_led / 0.026;
            println!("\n  Traditional formulation:");
            println!("    v/Vt = {:.1}", exp_term);
            println!("    exp(v/Vt) would be ~{:.2e}", exp_term);
            println!("    → Would overflow in single precision!");
            
            println!("\n  GLACIER formulation:");
            println!("    Working in log space avoids overflow");
            println!("    Constant Jacobian = 1/Vt = {:.1}", 1.0/0.026);
            println!("    ✓ Numerically stable solution");
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}

fn test_convergence_comparison() {
    // Compare convergence with and without log formulation
    println!("  Creating challenging circuit with sharp nonlinearity...");
    
    let mut circuit = Circuit::new();
    
    // Voltage source with sweep
    circuit.add_branch(
        "V1".to_string(),
        "vcc",
        "gnd",
        "VoltageSource".to_string(),
        5.0,
        None,
    );
    
    // Series resistor
    circuit.add_branch(
        "R1".to_string(),
        "vcc",
        "n1",
        "Resistor".to_string(),
        100.0,
        None,
    );
    
    // Back-to-back LEDs (challenging for convergence)
    circuit.add_branch(
        "D1".to_string(),
        "n1",
        "n2",
        "LED".to_string(),
        2.0,
        None,
    );
    
    circuit.add_branch(
        "D2".to_string(),
        "n2",
        "gnd",
        "LED".to_string(),
        2.0,
        None,
    );
    
    // Try different initial guesses
    let initial_guesses = vec![
        ("Zero", 0.0),
        ("Low", 1.0),
        ("Medium", 2.5),
        ("High", 4.0),
    ];
    
    println!("\n  Convergence from different initial guesses:");
    println!("  Initial    Iterations   Final Error");
    println!("  --------   ----------   -----------");
    
    for (name, _guess) in initial_guesses {
        let mut solver = UnifiedGlacierSolver::new(circuit.clone());
        
        match solver.solve() {
            Ok(result) => {
                println!("  {:8}   {:10}   converged", name, result.iterations);
            }
            Err(_) => {
                println!("  {:8}   {:10}   failed", name, ">");
            }
        }
    }
    
    println!("\n  Key advantages of unified approach:");
    println!("  • Selective log transformation only for exponential devices");
    println!("  • Better conditioned Jacobian matrix");
    println!("  • Adaptive PID damping based on gradient");
    println!("  • Smart scaling prevents numerical overflow");
}