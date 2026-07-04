//! Debug the integrated GPU flow to see why it's not using Phase 0 results

use std::collections::HashMap;
use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
};

fn main() {
    std::env::set_var("RUST_LOG", "info");
    
    println!("\n=== INTEGRATED GPU FLOW DEBUG ===\n");
    
    // Create a simple series 2 LED circuit
    let (circuit, models) = create_series_2_leds();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            println!("Testing integrated GPU flow...\n");
            
            let config = IntegratedSolverConfig {
                mode: SolverMode::Gpu,
                phase0_ramp_points: 10,  // Fewer points for easier debugging
                max_iterations: 300,
                tolerance: 1e-6,
            };
            
            let mut solver = IntegratedGlacierSolver::with_config(circuit, config);
            for (name, model) in &models {
                solver.add_model(name.clone(), model.clone());
            }
            
            println!("Calling analyze_async()...");
            match solver.analyze_async().await {
                Ok(solutions) => {
                    println!("\n✅ Analysis completed!");
                    println!("Number of solutions: {}", solutions.len());
                    for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                        println!("\nSolution {}: region [{:.1}%-{:.1}%]", i+1, start * 100.0, end * 100.0);
                        println!("  Iterations: {}", result.iterations);
                        println!("  Gradient: {:.2}", gradient);
                    }
                }
                Err(e) => {
                    println!("\n❌ Analysis failed: {}", e);
                }
            }
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU feature not enabled. Compile with --features gpu");
    }
}

fn create_series_2_leds() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("N_RES".to_string(), None);
    circuit.add_node("N_LED1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Voltage source - 7.4V for 2 LEDs
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 7.4, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 7.4,
        internal_resistance: Some(0.0),
    });
    
    // Current limiting resistor
    circuit.add_branch("R1".to_string(), "VCC", "N_RES", "Resistor".to_string(), 470.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 470.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    // LED1 with Is=1e-12
    circuit.add_branch("LED1".to_string(), "N_RES", "N_LED1", "LED".to_string(), 0.0, None);
    models.insert("LED1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // LED2 with Is=1e-15
    circuit.add_branch("LED2".to_string(), "N_LED1", "GND", "LED".to_string(), 0.0, None);
    models.insert("LED2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}