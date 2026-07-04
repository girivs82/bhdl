/// Debug the adaptive solver to understand convergence issues

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, ElectricalLimits,
    Result,
};

fn main() -> Result<()> {
    println!("=== Debug Adaptive Solver ===");
    
    // Create simple circuit: 5V -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    
    solver.set_convergence(50, 1e-6);
    
    println!("Running analysis...");
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Succeeded!");
            println!("  Iterations: {}", result.iterations);
            println!("  Total Power: {:.6}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.6}V", node_idx.index(), voltage);
            }
        },
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    Ok(())
}