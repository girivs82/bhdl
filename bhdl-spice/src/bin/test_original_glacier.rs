//! Test using the original GLACIER solver backup

use anyhow::Result;

// Include the backup solver directly
#[path = "../glacier_solver_backup.rs"]
mod original_glacier;

use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};

fn main() -> Result<()> {
    println!("=== Testing ORIGINAL GLACIER Solver (from backup) ===\n");
    
    // Create test circuit: 5V -> 220Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    let mut solver = original_glacier::GlacierSolver::new(circuit);
    
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
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("✓ Analysis successful! Found {} solution(s)\n", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("Solution {} (region {:.1}%-{:.1}%):", 
                         i+1, start*100.0, end*100.0);
                
                // Find LED current
                let led_current = result.branch_currents.values()
                    .filter(|&&i| i > 0.001 && i < 0.050)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .copied()
                    .unwrap_or(0.0);
                
                println!("  LED current: {:.3}mA", led_current * 1000.0);
                println!("  Total power: {:.3}mW", result.total_power * 1000.0);
                
                // Expected values
                let expected_current = (5.0 - 2.0) / 220.0; // ~13.6mA
                let error_percent = ((led_current - expected_current) / expected_current * 100.0).abs();
                
                println!("  Expected: {:.3}mA (error: {:.1}%)", 
                         expected_current * 1000.0, error_percent);
                
                if error_percent < 10.0 {
                    println!("  ✓ This is the correct solution!");
                }
            }
        }
        Err(e) => {
            println!("✗ Analysis failed: {}", e);
        }
    }
    
    println!("\nIf this shows ~13.6mA, then the original solver is fine and");
    println!("something changed in the current implementation.");
    
    Ok(())
}