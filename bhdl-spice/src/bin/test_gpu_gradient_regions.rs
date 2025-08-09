//! Test GPU gradient-based region detection

use bhdl_spice::{
    Circuit, ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
};
use std::collections::HashMap;

fn main() {
    std::env::set_var("RUST_LOG", "info");
    
    println!("\n=== GPU GRADIENT-BASED REGION DETECTION TEST ===\n");
    
    // Create the challenging circuit with multiple regions
    let (circuit, models) = create_multi_region_circuit();
    
    // Test CPU Serial first to get reference
    println!("1. CPU Serial Analysis (Reference):");
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
            println!("  CPU found {} regions:", solutions.len());
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("  Region {}: [{:.1}%-{:.1}%], gradient={:.2}, {} iterations", 
                         i+1, start*100.0, end*100.0, gradient, result.iterations);
                let max_current = result.branch_currents.values()
                    .map(|&c| c.abs())
                    .filter(|&c| c > 1e-6 && c < 1.0)
                    .max_by(|a, b| a.partial_cmp(b).unwrap())
                    .unwrap_or(0.0);
                println!("    Max current: {:.3} mA", max_current * 1000.0);
            }
        }
        Err(e) => {
            println!("  CPU Failed: {}", e);
        }
    }
    
    // Test GPU with gradient detection
    #[cfg(feature = "gpu")]
    {
        println!("\n2. GPU Analysis with Gradient Detection:");
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
                    println!("  GPU found {} regions:", solutions.len());
                    for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                        println!("  Region {}: [{:.1}%-{:.1}%], gradient={:.2}, {} iterations", 
                                 i+1, start*100.0, end*100.0, gradient, result.iterations);
                        let max_current = result.branch_currents.values()
                            .map(|&c| c.abs())
                            .filter(|&c| c > 1e-6 && c < 1.0)
                            .max_by(|a, b| a.partial_cmp(b).unwrap())
                            .unwrap_or(0.0);
                        println!("    Max current: {:.3} mA", max_current * 1000.0);
                    }
                }
                Err(e) => {
                    println!("  GPU Failed: {}", e);
                }
            }
        });
    }
    
    println!("\n3. Analysis Summary:");
    println!("  - CPU uses f64 precision and detects sharp transitions precisely");
    println!("  - GPU uses f32 with auto-scaling, may merge adjacent regions");
    println!("  - Both should find valid DC operating points in each region");
    println!("  - The gradient values indicate circuit sensitivity");
}

fn create_multi_region_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // Create a circuit with multiple operating regions
    // This uses two LEDs with different forward voltages to create distinct regions
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Variable input voltage
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    // Current limiting resistor
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 100.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // First LED (red, Vf=1.8V)
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 1.8,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Second LED (blue, Vf=3.2V) 
    circuit.add_branch("D2".to_string(), "N2", "GND", "LED".to_string(), 0.0, None);
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.2,
        forward_current: 0.02,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15), // Ultra-sharp
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}