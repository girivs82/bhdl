//! Test multi-region solving capability

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Multi-Region Solving Test ===\n");
    println!("Testing the ability to find solutions in different operating regions");
    println!("For parallel LEDs, we expect:");
    println!("- Region 1: Low voltage (LEDs OFF)");
    println!("- Region 2: High voltage (LEDs ON)\n");
    
    // Create parallel LED circuit
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
    
    // Try to find solutions in all regions
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n=== FOUND {} SOLUTIONS ===", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\n--- Region {} ---", i + 1);
                println!("  Ramp range: {:.3} to {:.3}", start, end);
                println!("  Average gradient: {:.3}", gradient);
                
                // Get LED voltage - need to find the node index for "out" node
                // For now, just show all node voltages
                println!("  Node voltages:");
                for (node_idx, voltage) in &result.node_voltages {
                    println!("    Node {:?}: {:.3} V", node_idx, voltage);
                }
                println!("  Total power: {:.2} mW", result.total_power * 1000.0);
                
                // Calling logic would interpret these properties
                println!("\n  Mathematical properties:");
                println!("    - Ramp value center: {:.3}", (start + end) / 2.0);
                println!("    - Region width: {:.3}", end - start);
                println!("    - Gradient indicates: {}", 
                    if *gradient > 10.0 { "steep transition" }
                    else if *gradient > 1.0 { "moderate transition" }
                    else { "gradual transition" }
                );
            }
            
            println!("\n=== ANALYSIS COMPLETE ===");
            println!("The SPICE engine can now choose which solution to use based on:");
            println!("- Design intent (e.g., 'I want the LEDs to be ON')");
            println!("- Initial conditions");
            println!("- Previous simulation state");
            println!("\nNote: The solver provides only mathematical properties.");
            println!("Domain interpretation (e.g., 'LED ON/OFF') is done by calling logic.");
        }
        Err(e) => {
            println!("\nMulti-region analysis failed: {}", e);
        }
    }
    
    Ok(())
}