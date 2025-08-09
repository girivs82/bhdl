//! Compare the backup GLACIER solver with the current one
//! This will help identify if the DC solver behavior has changed

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};

// We'll need to temporarily rename and use the backup
fn main() -> Result<()> {
    println!("=== GLACIER Solver Backup vs Current Comparison ===\n");
    
    // Create test circuit: 5V -> 220Ω -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("gnd".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "gnd", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "n1", "gnd", "LED".to_string(), 2.0, None);
    
    // Test with current solver
    println!("Testing with CURRENT GlacierSolver:");
    {
        let mut solver = bhdl_spice::GlacierSolver::new(circuit.clone());
        
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
                println!("  Found {} solutions", solutions.len());
                for (_, _, _, result) in solutions {
                    let led_current = result.branch_currents.values()
                        .filter(|&&i| i > 0.001 && i < 0.050)
                        .max_by(|a, b| a.partial_cmp(b).unwrap())
                        .copied()
                        .unwrap_or(0.0);
                    println!("  LED current: {:.3}mA", led_current * 1000.0);
                }
            }
            Err(e) => {
                println!("  Failed: {}", e);
            }
        }
    }
    
    println!("\n🔍 To test with backup solver:");
    println!("   1. Temporarily rename glacier_solver.rs to glacier_solver_current.rs");
    println!("   2. Rename glacier_solver_backup.rs to glacier_solver.rs");
    println!("   3. Update lib.rs if needed");
    println!("   4. Run this test again");
    println!("   5. Compare the results");
    
    println!("\nExpected correct LED current: ~13.6mA");
    
    Ok(())
}