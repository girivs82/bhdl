//! Direct test of GPU solver kernel bypassing multi-phase approach

use anyhow::Result;
use bhdl_spice::{
    circuit::Circuit,
    generic_glacier_solver::{GenericGlacierSolver, SolverConfig},
    spice_equation_system::SpiceEquationSystem,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, full_solver::GlacierFullGpuSolver};

fn create_simple_resistor_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Simple voltage divider - should converge easily
    let _vdd = circuit.add_node("VDD".to_string(), None);
    let _mid = circuit.add_node("MID".to_string(), None);
    let _gnd = circuit.add_node("GND".to_string(), None);
    
    // 5V source
    circuit.add_branch(
        "V1".to_string(),
        "VDD",
        "GND",
        "VoltageSource".to_string(),
        5.0,
        None
    );
    
    // R1 = 1kΩ from VDD to MID
    circuit.add_branch(
        "R1".to_string(),
        "VDD",
        "MID",
        "Resistor".to_string(),
        1000.0,
        None
    );
    
    // R2 = 1kΩ from MID to GND
    circuit.add_branch(
        "R2".to_string(),
        "MID",
        "GND",
        "Resistor".to_string(),
        1000.0,
        None
    );
    
    circuit
}

#[cfg(feature = "gpu")]
async fn test_gpu_direct() -> Result<()> {
    println!("\n=== Direct GPU Solver Test ===\n");
    
    // Create simple circuit
    let circuit = create_simple_resistor_circuit();
    
    println!("Test circuit: Simple voltage divider");
    println!("Expected: VDD=5V, MID=2.5V, GND=0V\n");
    
    // Test CPU solver first
    println!("--- CPU Solver ---");
    let mut equation_system = SpiceEquationSystem::new(circuit.clone())?;
    equation_system.set_voltage_ramp(1.0); // Full voltage
    let mut variables = equation_system.create_variables();
    
    let config = SolverConfig {
        tolerance: 1e-9,  // Relax tolerance slightly
        ..Default::default()
    };
    let mut cpu_solver = GenericGlacierSolver::new(config);
    
    match cpu_solver.solve(&mut variables, &equation_system) {
        Ok(stats) => {
            println!("CPU converged in {} iterations", stats.iterations);
            for var in &variables {
                println!("  {}: {:.3}", var.name, var.value);
            }
        }
        Err(e) => {
            println!("CPU failed: {}", e);
        }
    }
    
    // Now test GPU solver directly
    println!("\n--- Direct GPU Solver Test ---");
    
    match GpuContext::new().await {
        Ok(context) => {
            println!("GPU context created successfully");
            
            // Create full GPU solver
            let gpu_solver = GlacierFullGpuSolver::new(
                std::sync::Arc::new(context),
                10 // max circuit size
            ).await?;
            
            println!("GPU solver created, attempting solve at ramp=1.0...");
            
            // Try direct solve at full voltage
            match gpu_solver.solve_at_ramp(&circuit, 1.0, None).await {
                Ok(vars) => {
                    println!("GPU converged!");
                    for var in &vars {
                        let actual = match var.space {
                            bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => var.value.exp(),
                            _ => var.value,
                        };
                        println!("  {}: {:.3} (actual: {:.3})", var.name, var.value, actual);
                    }
                }
                Err(e) => {
                    println!("GPU solve failed: {}", e);
                    println!("\nPossible issues:");
                    println!("  1. Shader compilation error");
                    println!("  2. Matrix solving error in shader");
                    println!("  3. Buffer transfer issues");
                    println!("  4. Numerical issues in GPU computation");
                }
            }
        }
        Err(e) => {
            println!("Failed to create GPU context: {}", e);
        }
    }
    
    Ok(())
}

#[cfg(not(feature = "gpu"))]
async fn test_gpu_direct() -> Result<()> {
    println!("GPU feature not enabled");
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_gpu_direct())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU feature not enabled. Run with: cargo run --features gpu --bin test_gpu_direct");
        Ok(())
    }
}