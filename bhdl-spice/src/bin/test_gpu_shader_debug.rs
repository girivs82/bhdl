//! Debug GPU shader execution with detailed logging

use std::sync::Arc;
use std::collections::HashMap;

use bhdl_spice::{
    circuit::Circuit,
    ComponentModel,
    glacier_gpu::{GpuContext, GlacierFullGpuSolver, gpu_data::*},
};
fn main() {
    std::env::set_var("RUST_LOG", "info");
    
    println!("\n=== GPU SHADER DEBUG TEST ===\n");
    
    // Create simple LED circuit
    let (circuit, models) = create_simple_led_circuit();
    
    // Initialize GPU
    let result = tokio::runtime::Runtime::new().unwrap().block_on(async {
        let context = Arc::new(GpuContext::new().await?);
        let gpu_solver = GlacierFullGpuSolver::new(context, 100).await?;
        
        // Convert circuit to GPU format
        let mut converter = GpuCircuitConverter::new();
        let (circuit_data, components, variables) = converter.convert_with_models(&circuit, &models);
        
        println!("Circuit data:");
        println!("  Nodes: {}", circuit_data.num_nodes);
        println!("  Components: {}", circuit_data.num_components);
        println!("  Voltage sources: {}", circuit_data.num_voltage_sources);
        println!("  Ground node: {}", circuit_data.ground_node);
        
        println!("\nComponents:");
        for (i, comp) in components.iter().enumerate() {
            println!("  [{}] Type: {}, nodes: {}->{}, value: {}", 
                i, comp.comp_type, comp.node1, comp.node2, comp.value);
            if comp.comp_type == 2 || comp.comp_type == 3 { // LED or Diode
                println!("      Is={}, n={}, Vt={}", comp.is_sat, comp.n_emission, comp.vt);
            }
        }
        
        println!("\nInitial variables:");
        for (i, var) in variables.iter().enumerate() {
            let var_type = match var.var_type {
                0 => "Voltage",
                1 => "Current",
                _ => "Unknown",
            };
            let space = match var.space {
                0 => "Linear",
                1 => "Log",
                _ => "Unknown",
            };
            println!("  [{}] {} {}, index={}, value={}, scale_exp={}, scale_factor={}", 
                i, var_type, space, var.index, var.value, var.scale_exponent, var.scale_factor);
        }
        
        // Run Phase 0 scan with just a few points
        println!("\nRunning Phase 0 scan with 5 ramp points...");
        let results = gpu_solver.phase0_coarse_scan(&circuit, 5).await?;
        
        println!("\nPhase 0 Results:");
        for (i, result) in results.iter().enumerate() {
            println!("  [{}] Ramp: {:.1}%, converged: {}, iterations: {}, error: {:.6}, damping: {:.3}", 
                i, result.ramp * 100.0, result.converged, result.iterations, result.error, result.damping);
        }
        
        Ok::<(), anyhow::Error>(())
    });
    
    match result {
        Ok(_) => println!("\nGPU test completed successfully"),
        Err(e) => println!("\nGPU test failed: {}", e),
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