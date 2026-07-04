//! Compare GPU vs CPU multi-region discovery
//! 
//! This test shows why GPU and CPU find different solution regions

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    glacier_solver::GlacierSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{
    gpu_context::GpuContext,
    full_solver::GlacierFullGpuSolver,
};

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 5V -> 1kΩ -> LED -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "led_anode", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 0.05,
        limits: ElectricalLimits::default(),
    });
    
    circuit.add_branch("D1".to_string(), "led_anode", "gnd", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    (circuit, models)
}

async fn test_region_discovery() -> Result<()> {
    println!("GPU vs CPU Region Discovery Comparison");
    println!("{}", "=".repeat(80));
    
    let (circuit, models) = create_simple_led_circuit();
    
    // Test CPU solver
    println!("\nCPU GLACIER Solver - Multi-region Analysis:");
    println!("Region\tStart%\tEnd%\tMid%\tV_LED@Mid\tI_LED@Mid");
    println!("{}", "-".repeat(80));
    
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        cpu_solver.add_model(name.clone(), model.clone());
    }
    
    // Run full GLACIER analysis
    let cpu_regions = match cpu_solver.analyze_all_regions() {
        Ok(regions) => regions,
        Err(e) => {
            println!("CPU analysis failed: {:?}", e);
            return Ok(());
        }
    };
    
    // Display CPU regions
    for (i, (start, end, gradient, result)) in cpu_regions.iter().enumerate() {
        // Find LED branch
        let led_edge = circuit.branches()
            .find(|(_, branch)| branch.component_type == "LED")
            .map(|(edge, _)| edge);
            
        // Get LED current
        let led_current = led_edge
            .and_then(|edge| result.branch_currents.get(&edge))
            .map(|current| current.abs())
            .unwrap_or(0.0);
            
        // Find LED voltage
        let led_voltage = led_edge
            .and_then(|edge| circuit.branch_nodes(edge))
            .and_then(|(n1, _)| result.node_voltages.get(&n1))
            .copied()
            .unwrap_or(0.0);
        
        println!("{}\t{:.1}\t{:.1}\t{:.1}\t{:.3}V\t\t{:.3}mA",
                i+1, start*100.0, end*100.0, (start+end)*50.0, 
                led_voltage, led_current * 1000.0);
    }
    
    println!("\nFound {} distinct operating regions", cpu_regions.len());
    
    // Test GPU solver
    #[cfg(feature = "gpu")]
    {
        println!("\n\nGPU GLACIER Solver - Multi-region Analysis:");
        println!("Region\tStart%\tEnd%\tGradient\tNotes");
        println!("{}", "-".repeat(80));
        
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        // Run full GLACIER analysis on GPU
        let gpu_regions = match gpu_solver.analyze_glacier(&circuit).await {
            Ok(regions) => regions,
            Err(e) => {
                println!("GPU analysis failed: {:?}", e);
                return Ok(());
            }
        };
        
        // Display GPU regions
        for (i, (start, end, gradient, result)) in gpu_regions.iter().enumerate() {
            // Check if this region matches any CPU region
            let matches_cpu = cpu_regions.iter().any(|(cpu_start, cpu_end, _, _)| {
                (start - cpu_start).abs() < 0.1 && (end - cpu_end).abs() < 0.1
            });
            
            let notes = if matches_cpu {
                "✓ Matches CPU region"
            } else {
                "⚠️  No CPU equivalent"
            };
            
            println!("{}\t{:.1}\t{:.1}\t{:.1}\t{}",
                    i+1, start*100.0, end*100.0, gradient, notes);
        }
        
        println!("\nFound {} distinct operating regions", gpu_regions.len());
        
        println!("\n\nAnalysis:");
        println!("- CPU found {} regions, GPU found {} regions", 
                cpu_regions.len(), gpu_regions.len());
        
        if cpu_regions.len() != gpu_regions.len() {
            println!("- Region count mismatch suggests different convergence behavior");
            println!("- GPU may be finding additional solution branches");
        }
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(test_region_discovery())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}