//! Simple test for unified GLACIER solver debugging

use bhdl_spice::{
    Circuit,
    unified_glacier_solver::UnifiedGlacierSolver,
};

fn main() {
    println!("=== Simple Unified GLACIER Test ===\n");
    
    // Create very simple circuit: 5V source with 1k resistor
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
        "gnd",
        "Resistor".to_string(),
        1000.0,
        None,
    );
    
    println!("Circuit created with {} nodes and {} branches",
             circuit.nodes().count(), circuit.branches().count());
    
    // Create solver
    let mut solver = UnifiedGlacierSolver::new(circuit);
    
    match solver.solve() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Print results
            println!("\nNode voltages:");
            for (node_idx, voltage) in &result.node_voltages {
                if let Some(node) = solver.circuit.get_node_by_id(*node_idx) {
                    println!("  {} = {:.3}V", node.name, voltage);
                }
            }
            
            println!("\nBranch currents:");
            for (edge_idx, current) in &result.branch_currents {
                for (idx, branch) in solver.circuit.branches() {
                    if idx == *edge_idx {
                        println!("  {} = {:.3}mA", branch.name, current * 1000.0);
                    }
                }
            }
        }
        Err(e) => {
            println!("✗ Failed to converge: {}", e);
        }
    }
}