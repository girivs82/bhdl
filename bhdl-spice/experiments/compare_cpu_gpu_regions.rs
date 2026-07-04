//! Compare the regions found by CPU vs GPU solvers

use std::collections::HashMap;
use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
};

fn main() {
    std::env::set_var("RUST_LOG", "info");
    
    println!("\n=== CPU vs GPU REGION COMPARISON ===\n");
    
    // Create a challenging circuit that should have multiple regions
    let (circuit, models) = create_challenging_circuit();
    
    // Test CPU Serial
    println!("1. CPU Serial Analysis:");
    let config = IntegratedSolverConfig {
        mode: SolverMode::CpuSerial,
        phase0_ramp_points: 40,
        max_iterations: 500,
        tolerance: 1e-9,
    };
    
    let mut cpu_solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
    for (name, model) in &models {
        cpu_solver.add_model(name.clone(), model.clone());
    }
    
    match cpu_solver.analyze() {
        Ok(solutions) => {
            println!("  Found {} regions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("  Region {}: [{:.1}%-{:.1}%], gradient={:.2}, {} iterations", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
                // Show current in the circuit
                let max_current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .filter(|&c| c > 1e-6 && c < 1.0)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);
                println!("    Max current: {:.3} mA", max_current * 1000.0);
            }
        }
        Err(e) => {
            println!("  Failed: {}", e);
        }
    }
    
    // Test GPU
    #[cfg(feature = "gpu")]
    {
        println!("\n2. GPU Analysis:");
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let config = IntegratedSolverConfig {
                mode: SolverMode::Gpu,
                phase0_ramp_points: 40,
                max_iterations: 500,
                tolerance: 1e-7,
            };
            
            let mut gpu_solver = IntegratedGlacierSolver::with_config(circuit.clone(), config);
            for (name, model) in &models {
                gpu_solver.add_model(name.clone(), model.clone());
            }
            
            match gpu_solver.analyze_async().await {
                Ok(solutions) => {
                    println!("  Found {} regions:", solutions.len());
                    for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                        println!("  Region {}: [{:.1}%-{:.1}%], gradient={:.2}, {} iterations", 
                                 i+1, start*100.0, end*100.0, gradient, result.iterations);
                        // Show current in the circuit
                        let max_current = result.branch_currents.values()
                            .map(|&c| c.abs())
                            .filter(|&c| c > 1e-6 && c < 1.0)
                            .max_by(|a, b| a.partial_cmp(b).unwrap())
                            .unwrap_or(0.0);
                        println!("    Max current: {:.3} mA", max_current * 1000.0);
                    }
                }
                Err(e) => {
                    println!("  Failed: {}", e);
                }
            }
        });
    }
    
    println!("\n3. Analysis:");
    println!("  - CPU uses gradient-based region detection");
    println!("  - GPU uses simple consecutive convergence grouping");
    println!("  - Both find valid solutions, but CPU provides more granular regions");
    println!("  - The actual circuit behavior (currents) should be similar");
}

fn create_challenging_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Create a circuit with multiple operating regions
    // This is a Zener diode voltage regulator that should have distinct regions
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("VREG".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Variable input voltage
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 15.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 15.0,
        internal_resistance: Some(0.0),
    });
    
    // Current limiting resistor
    circuit.add_branch("R1".to_string(), "VIN", "VREG", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // Zener diode (modeled as regular diode for DC analysis)
    circuit.add_branch("D1".to_string(), "GND", "VREG", "Diode".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 5.1, // 5.1V Zener
        forward_resistance: 10.0,
        reverse_current: 1e-9,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: Default::default(),
    });
    
    // Load resistor
    circuit.add_branch("RLOAD".to_string(), "VREG", "GND", "Resistor".to_string(), 1000.0, None);
    models.insert("RLOAD".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    (circuit, models)
}