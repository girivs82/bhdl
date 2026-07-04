//! Simple synchronous test for GPU solver

use anyhow::Result;
use bhdl_spice::{
    circuit::Circuit,
    generic_glacier_solver::{GenericGlacierSolver, SolverConfig},
    spice_equation_system::SpiceEquationSystem,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{solver::solve_with_gpu, gpu_data::GpuCircuitConverter};

fn create_simple_led_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes
    let _vdd = circuit.add_node("VDD".to_string(), None);
    let _led_cathode = circuit.add_node("LED_CATHODE".to_string(), None);
    let _gnd = circuit.add_node("GND".to_string(), None);
    
    // VDD = 5V source from VDD to GND
    circuit.add_branch(
        "V1".to_string(),
        "VDD",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None
    );
    
    // R1 = 330Ω resistor from VDD to LED_CATHODE
    circuit.add_branch(
        "R1".to_string(),
        "VDD",
        "LED_CATHODE",
        "Resistor".to_string(),
        330.0,
        None
    );
    
    // LED from LED_CATHODE to GND
    circuit.add_branch(
        "D1".to_string(),
        "LED_CATHODE",
        "GND",
        "LED".to_string(),
        0.0,
        None
    );
    
    circuit
}

fn main() -> Result<()> {
    println!("\n=== Simple GPU Solver Test ===\n");
    
    // Create simple circuit
    let circuit = create_simple_led_circuit();
    
    println!("Circuit:");
    println!("  Nodes: {} (including ground)", circuit.nodes().count());
    for (idx, node) in circuit.nodes() {
        println!("    Node {}: {} (ground: {})", idx.index(), node.name, node.is_ground);
    }
    
    println!("\n  Components: {}", circuit.branches().count());
    for (idx, branch) in circuit.branches() {
        if let Some((n1, n2)) = circuit.branch_nodes(idx) {
            println!("    {} {}: node {} -> node {}, value: {}", 
                     branch.component_type, idx.index(), n1.index(), n2.index(), branch.value);
        }
    }
    
    let config = SolverConfig {
        max_iterations: 100,
        tolerance: 1e-12,
        use_adaptive_damping: true,
        min_damping: 1e-6,
        max_damping: 1.0,
        singular_perturbation: 1e-10,
        damping_factor: 0.7,
    };
    
    // First, solve with CPU solver for reference
    println!("\n--- CPU Solver Reference ---");
    let mut equation_system = SpiceEquationSystem::new(circuit.clone())?;
    equation_system.set_voltage_ramp(1.0); // Full voltage
    let mut variables = equation_system.create_variables();
    
    println!("\nInitial variables:");
    for var in &variables {
        println!("  {}: {} (space: {:?})", var.name, var.value, var.space);
    }
    
    let mut cpu_solver = GenericGlacierSolver::new(config.clone());
    match cpu_solver.solve(&mut variables, &equation_system) {
        Ok(stats) => {
            println!("\nCPU converged in {} iterations, error: {:.2e}", stats.iterations, stats.final_error);
            println!("Solution:");
            for var in &variables {
                let actual_val = match var.space {
                    bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => var.value.exp(),
                    _ => var.value,
                };
                println!("  {}: {} (actual: {})", var.name, var.value, actual_val);
            }
        }
        Err(e) => {
            println!("\nCPU failed to converge: {}", e);
        }
    }
    
    // Now try GPU solver using synchronous wrapper
    println!("\n--- GPU Solver Test ---");
    
    #[cfg(feature = "gpu")]
    {
        // First show what data would be sent to GPU
        println!("\nData conversion for GPU:");
        let mut converter = GpuCircuitConverter::new();
        let (circuit_data, components, gpu_variables) = converter.convert(&circuit);
        
        println!("\nGPU Circuit Data:");
        println!("  num_nodes: {}", circuit_data.num_nodes);
        println!("  num_components: {}", circuit_data.num_components);
        println!("  num_voltage_sources: {}", circuit_data.num_voltage_sources);
        println!("  ground_node: {}", circuit_data.ground_node);
        
        println!("\nGPU Components:");
        for (i, comp) in components.iter().enumerate() {
            println!("  [{}] type: {}, node1: {}, node2: {}, value: {}", 
                     i, comp.comp_type, comp.node1, comp.node2, comp.value);
            if comp.comp_type == 2 || comp.comp_type == 3 { // LED or Diode
                println!("      is_sat: {:.2e}, n: {}, vt: {}", 
                         comp.is_sat, comp.n_emission, comp.vt);
            }
        }
        
        println!("\nGPU Variables (initial):");
        for (i, var) in gpu_variables.iter().enumerate() {
            let var_type_str = match var.var_type {
                0 => "Voltage",
                1 => "Current",
                _ => "Unknown",
            };
            let space_str = match var.space {
                0 => "Linear",
                1 => "Log",
                _ => "Unknown",
            };
            println!("  [{}] {}{} ({}): {}", 
                     i, var_type_str, var.index, space_str, var.value);
        }
        
        // Now try to solve
        println!("\nAttempting GPU solve using synchronous wrapper...");
        match solve_with_gpu(circuit, config) {
            Ok(result) => {
                println!("\nGPU converged!");
                println!("  Iterations: {}", result.iterations);
                println!("  Final error: {:.2e}", result.final_error);
                println!("  Total power: {:.3}W", result.total_power);
                
                println!("\nNode voltages:");
                for (node, voltage) in &result.node_voltages {
                    println!("  Node {}: {:.3}V", node.index(), voltage);
                }
                
                println!("\nBranch currents:");
                for (branch, current) in &result.branch_currents {
                    println!("  Branch {}: {:.3}A", branch.index(), current);
                }
            }
            Err(e) => {
                println!("\nGPU failed: {}", e);
                println!("\nDetailed error: {:?}", e);
                
                println!("\nPossible reasons:");
                println!("  1. GPU not available (no WebGPU support)");
                println!("  2. Shader compilation error");
                println!("  3. Initial values issue");
                println!("  4. Matrix solving issue in GPU shader");
            }
        }
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU feature not enabled. Run with: cargo run --features gpu --bin test_gpu_simple");
    }
    
    Ok(())
}