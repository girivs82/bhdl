/// Compare matrix setup between adaptive solver and working nonlinear solver
/// This will help identify where the convergence issue lies.

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, NonlinearDcAnalysis, ElectricalLimits,
    Result,
};

fn main() -> Result<()> {
    println!("=== Matrix Setup Comparison ===");
    
    // Test 1: Working nonlinear solver
    test_working_solver()?;
    
    // Test 2: Our adaptive solver with debug output
    test_adaptive_solver_debug()?;
    
    Ok(())
}

fn test_working_solver() -> Result<()> {
    println!("\n--- Working NonlinearDcAnalysis ---");
    
    // Create identical circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    let mut analysis = NonlinearDcAnalysis::new(circuit);
    analysis.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    
    // Set same tolerance as our adaptive solver
    analysis.set_convergence(50, 1e-6);
    
    match analysis.analyze() {
        Ok(result) => {
            println!("✓ Working solver succeeded:");
            println!("  Iterations: {}", result.iterations);
            println!("  Total Power: {:.6}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.6}V", node_idx.index(), voltage);
            }
        },
        Err(e) => {
            println!("✗ Working solver failed: {}", e);
        }
    }
    
    Ok(())
}

fn test_adaptive_solver_debug() -> Result<()> {
    println!("\n--- Adaptive Solver Debug ---");
    
    // Create identical circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    
    // Check circuit structure
    println!("Circuit structure:");
    if let Some((ground_idx, ground_node)) = circuit.ground_node() {
        println!("  Ground node: {} ({})", ground_idx.index(), ground_node.name);
    } else {
        println!("  ❌ No ground node found!");
    }
    
    println!("  Nodes:");
    for (idx, node) in circuit.nodes() {
        println!("    {}: {} (ground: {})", idx.index(), node.name, node.is_ground);
    }
    
    println!("  Branches:");
    for (idx, branch) in circuit.branches() {
        if let Some((n1, n2)) = circuit.branch_nodes(idx) {
            println!("    {}: {} ({} -> {}) = {}", 
                     idx.index(), branch.name, n1.index(), n2.index(), branch.value);
        }
    }
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    
    solver.set_convergence(50, 1e-3);  // More relaxed for first success
    
    // Add debug output to understand what's happening
    println!("\nAttempting analysis...");
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Adaptive solver succeeded:");
            println!("  Iterations: {}", result.iterations);
            println!("  Total Power: {:.6}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.6}V", node_idx.index(), voltage);
            }
        },
        Err(e) => {
            println!("✗ Adaptive solver failed: {}", e);
            
            // Debug: what should the solution be?
            println!("\n📋 Expected solution:");
            println!("  For 5V source with 1Ω internal resistance, no external load:");
            println!("  VCC = 5V (no current flows, so no voltage drop across internal resistance)");
            println!("  Current = 0A (no path for current)");
            println!("  Power = 0W");
        }
    }
    
    Ok(())
}