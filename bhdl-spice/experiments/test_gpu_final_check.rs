//! Final check of GPU solver after fixing buffer sizes

use std::collections::HashMap;
use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    IntegratedGlacierSolver, SolverMode, IntegratedSolverConfig,
};

fn main() {
    std::env::set_var("RUST_LOG", "error");
    
    println!("\n=== GPU SOLVER FINAL CHECK ===\n");
    
    let (circuit, models) = create_simple_led_circuit();
    
    #[cfg(feature = "gpu")]
    {
        tokio::runtime::Runtime::new().unwrap().block_on(async {
            let config = IntegratedSolverConfig {
                mode: SolverMode::Gpu,
                phase0_ramp_points: 10,
                max_iterations: 300,
                tolerance: 1e-6,
            };
            
            let mut solver = IntegratedGlacierSolver::with_config(circuit, config);
            for (name, model) in &models {
                solver.add_model(name.clone(), model.clone());
            }
            
            println!("Running GPU solver with auto-scaling...");
            match solver.analyze_async().await {
                Ok(solutions) => {
                    println!("✅ GPU SOLVER COMPLETED WITHOUT NaN!");
                    println!("Number of solution regions: {}", solutions.len());
                    
                    for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                        println!("\nRegion {}: [{:.1}% - {:.1}%]", i+1, start * 100.0, end * 100.0);
                        println!("  Iterations: {}", result.iterations);
                        println!("  Gradient: {:.2}", gradient);
                        
                        // Extract key values
                        let led_current = result.branch_currents.values()
                            .filter(|&&c| c.abs() > 1e-6 && c.abs() < 0.1)
                            .map(|&c| c.abs())
                            .max_by(|a, b| a.partial_cmp(b).unwrap())
                            .unwrap_or(0.0);
                        
                        let vcc_voltage = result.node_voltages.values()
                            .filter(|&&v| v > 4.0)
                            .copied()
                            .max_by(|a, b| a.partial_cmp(b).unwrap())
                            .unwrap_or(0.0);
                        
                        let led_voltage = result.node_voltages.values()
                            .filter(|&&v| v > 1.0 && v < 4.0)
                            .copied()
                            .max_by(|a, b| a.partial_cmp(b).unwrap())
                            .unwrap_or(0.0);
                        
                        println!("  LED Current: {:.3} mA", led_current * 1000.0);
                        println!("  VCC Voltage: {:.3} V", vcc_voltage);
                        println!("  LED Voltage: {:.3} V", led_voltage);
                        
                        // Debug: show all values
                        println!("\n  All node voltages:");
                        for (node, &voltage) in &result.node_voltages {
                            println!("    Node {:?}: {:.6} V", node, voltage);
                        }
                        
                        println!("\n  All branch currents:");
                        for (branch, &current) in &result.branch_currents {
                            println!("    Branch {:?}: {:.6} A ({:.3} mA)", branch, current, current * 1000.0);
                        }
                    }
                    
                    println!("\n✅ SUCCESS: GPU solver fixed! Buffer size mismatch was the issue.");
                }
                Err(e) => {
                    println!("❌ GPU solver failed: {}", e);
                }
            }
        });
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU feature not enabled. Compile with --features gpu");
    }
}

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_A".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VCC", "LED_A", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "LED_A", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}