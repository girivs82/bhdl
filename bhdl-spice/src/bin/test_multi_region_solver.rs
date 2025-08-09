//! Test multi-region solver that finds solutions in different operating regions

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Multi-Region Solver Test ===\n");
    println!("This solver will attempt to find solutions in different operating regions");
    println!("For LED circuits, this might include:");
    println!("- Region 1: LEDs OFF (low voltage)");
    println!("- Region 2: LEDs ON (normal operation)");
    println!("- Transition regions are avoided\n");
    
    // Test circuit: parallel LEDs
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
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
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    // TODO: This would require modifying the solver to:
    // 1. Detect transitions during Phase 1 scan
    // 2. Identify stable regions
    // 3. Run Phase 2 from multiple starting points
    // 4. Return multiple solutions
    
    println!("Running standard solver for now...");
    match solver.analyze() {
        Ok(result) => {
            println!("\nFound solution:");
            println!("  LED voltage: {:.3} V", result.node_voltages.get("out").unwrap_or(&0.0));
            
            // Check which region we're in
            let led_voltage = *result.node_voltages.get("out").unwrap_or(&0.0);
            if led_voltage < 1.8 {
                println!("  Region: LEDs OFF");
            } else {
                println!("  Region: LEDs ON");
                for (branch, current) in &result.branch_currents {
                    if branch.starts_with("D") {
                        println!("  {}: {:.2} mA", branch, current * 1000.0);
                    }
                }
            }
        }
        Err(e) => {
            println!("\nSolver failed: {}", e);
        }
    }
    
    Ok(())
}