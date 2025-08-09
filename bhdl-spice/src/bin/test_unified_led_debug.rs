//! Debug test for unified GLACIER solver with LED

use bhdl_spice::{
    Circuit,
    unified_glacier_solver::UnifiedGlacierSolver,
};

fn main() {
    println!("=== LED Circuit Debug Test ===\n");
    
    // Create simple LED circuit: 5V -> 220Ω -> LED -> GND
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
    
    println!("Circuit created with {} nodes and {} branches",
             circuit.nodes().count(), circuit.branches().count());
    
    // Print circuit details
    println!("\nBranches:");
    for (idx, branch) in circuit.branches() {
        println!("  {:?}: {} ({}) = {}", idx, branch.name, branch.component_type, branch.value);
    }
    
    // Create solver
    let mut solver = UnifiedGlacierSolver::new(circuit);
    
    match solver.solve() {
        Ok(result) => {
            println!("\n✓ Converged in {} iterations", result.iterations);
            
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
                        
                        // For LED, print operating point details
                        if branch.component_type == "LED" {
                            // Get node voltages
                            if let Some((n1, n2)) = solver.circuit.branch_nodes(idx) {
                                let v1 = result.node_voltages.get(&n1).copied().unwrap_or(0.0);
                                let v2 = result.node_voltages.get(&n2).copied().unwrap_or(0.0);
                                let v_led = v1 - v2;
                                println!("    LED voltage drop = {:.3}V", v_led);
                                println!("    LED power = {:.3}mW", v_led * current * 1000.0);
                            }
                        }
                    }
                }
            }
            
            // Verify KCL
            println!("\nKCL verification:");
            let i_source = result.branch_currents.values()
                .find(|&&i| (i - 0.005).abs() < 0.01)
                .copied()
                .unwrap_or(0.0);
            println!("  Source current = {:.3}mA", i_source * 1000.0);
            
            // Calculate expected LED current
            if let Some(n1_idx) = solver.circuit.get_node("n1").map(|(idx, _)| idx) {
                if let Some(&v_n1) = result.node_voltages.get(&n1_idx) {
                    let i_expected = (5.0 - v_n1) / 220.0;
                    println!("  Expected current through R1 = {:.3}mA", i_expected * 1000.0);
                }
            }
        }
        Err(e) => {
            println!("\n✗ Failed to converge: {}", e);
            
            // Print debug info
            println!("\nVariable info:");
            for var in &solver.variables {
                println!("  Var {}: {:?}", var.index, var.var_type);
            }
        }
    }
}