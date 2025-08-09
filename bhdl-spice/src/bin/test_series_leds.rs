//! Test series LEDs convergence with multi-region approach

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Series LEDs Test ===\n");
    
    // Create series LED circuit: 12V -> 1kΩ -> LED -> LED -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("n3".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "n3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "n3", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Add LED models
    for i in 1..=3 {
        solver.add_model(format!("D{}", i), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            limits: ElectricalLimits::default(),
        });
    }
    
    // Try standard analysis first
    println!("Attempting standard analysis...");
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Converged successfully!");
            println!("  Node voltages:");
            for (node_idx, voltage) in &result.node_voltages {
                println!("    Node {:?}: {:.3} V", node_idx, voltage);
            }
            
            // Calculate LED current through R1
            let r1_current = result.branch_currents.values()
                .nth(1)  // R1 is the second branch
                .map(|c| c.abs())
                .unwrap_or(0.0);
            println!("  LED current: {:.2} mA", r1_current * 1000.0);
            println!("  Expected: ~6 mA (12V - 6V) / 1kΩ");
            
            // Check if this is the OFF or ON state
            let vcc_voltage = result.node_voltages.values()
                .max_by(|a, b| a.partial_cmp(b).unwrap())
                .copied()
                .unwrap_or(0.0);
            println!("\n  VCC voltage: {:.3} V", vcc_voltage);
            if vcc_voltage < 1.0 {
                println!("  ⚠️  This appears to be the 'LEDs OFF' solution");
                println!("  The solver found a valid mathematical solution where LEDs are not conducting");
                println!("  For the 'LEDs ON' solution, multi-region analysis would be needed");
            }
        }
        Err(e) => {
            println!("✗ Standard analysis failed: {}", e);
            println!("\nTrying multi-region approach would help here...");
        }
    }
    
    Ok(())
}