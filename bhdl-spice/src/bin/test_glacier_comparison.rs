//! Compare standard GLACIER solver with enhanced version
//! This helps verify that the enhancements don't break existing functionality

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== GLACIER Solver Comparison Test ===\n");
    
    // Create the same LED circuit for both tests
    let circuit = create_led_circuit();
    
    // Test 1: Standard GLACIER solver
    println!("Test 1: Standard GLACIER Solver");
    test_standard_solver(circuit.clone())?;
    
    // Test 2: Enhanced GLACIER solver
    println!("\nTest 2: Enhanced GLACIER Solver (with log-space)");
    test_enhanced_solver(circuit)?;
    
    Ok(())
}

fn create_led_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    circuit
}

fn add_models(solver: &mut GlacierSolver) {
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
}

fn test_standard_solver(circuit: Circuit) -> Result<()> {
    let mut solver = GlacierSolver::new(circuit);
    add_models(&mut solver);
    
    println!("\nRunning standard GLACIER analysis...");
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✓ Found {} solution(s)", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.0}%-{:.0}%):", 
                         i+1, start*100.0, end*100.0);
                println!("  Average gradient: {:.2}", gradient);
                println!("  Iterations: {}", result.iterations);
                
                // Find LED current
                let led_current = result.branch_currents.values()
                    .filter(|&&i| i > 0.001 && i < 0.050)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                
                println!("  LED current: {:.3}mA", led_current * 1000.0);
                println!("  Total power: {:.3}mW", result.total_power * 1000.0);
                
                // Check if this is a reasonable solution
                if led_current > 0.005 && led_current < 0.020 {
                    println!("  ✓ This looks like a valid LED operating point");
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
    
    Ok(())
}

fn test_enhanced_solver(circuit: Circuit) -> Result<()> {
    let mut solver = GlacierSolver::new(circuit);
    add_models(&mut solver);
    
    println!("\nRunning enhanced GLACIER analysis...");
    
    match solver.analyze_with_enhanced_dc() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Find LED current
            let led_current = result.branch_currents.values()
                .filter(|&&i| i > 0.001 && i < 0.050)
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .copied()
                .unwrap_or(0.0);
            
            println!("  LED current: {:.3}mA", led_current * 1000.0);
            println!("  Total power: {:.3}mW", result.total_power * 1000.0);
            
            // Check if this is a reasonable solution
            if led_current > 0.005 && led_current < 0.020 {
                println!("  ✓ Valid LED operating point found");
            } else {
                println!("  ✗ LED current out of expected range");
            }
            
            // Compare voltages
            println!("\n  Node voltages:");
            for (_, voltage) in result.node_voltages.iter().take(3) {
                println!("    {:.3}V", voltage);
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
    
    Ok(())
}