//! Simple debug comparison of GPU vs CPU for LED circuit
//! 
//! This test focuses on understanding why GPU and CPU converge to different solutions.

use anyhow::Result;
use std::collections::HashMap;
use std::sync::Arc;

use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits,
    GlacierSolver,
};

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{
    gpu_context::GpuContext,
    full_solver::GlacierFullGpuSolver,
};

fn create_simple_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 5V -> 330Ω -> LED -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "led_anode", "Resistor".to_string(), 330.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 330.0,
        tolerance: 0.05,
        limits: ElectricalLimits {
            max_voltage: Some(50.0),
            max_current: Some(0.1),
            max_power: Some(0.25),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
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
        limits: ElectricalLimits {
            max_voltage: Some(5.0),
            max_current: Some(0.03),
            max_power: Some(0.1),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
    });
    
    (circuit, models)
}

async fn debug_simple_comparison() -> Result<()> {
    println!("Debug: GPU vs CPU Simple LED Circuit");
    println!("{}", "=".repeat(60));
    
    let (circuit, models) = create_simple_led_circuit();
    
    // CPU Reference Solution
    println!("\nCPU GLACIER Solution:");
    println!("{}", "-".repeat(30));
    
    let mut cpu_solver = GlacierSolver::new(circuit.clone());
    for (name, model) in models.clone() {
        cpu_solver.add_model(name, model);
    }
    
    let cpu_result = cpu_solver.analyze();
    match cpu_result {
        Ok(solutions) => {
            if let Some((_, _, _, result)) = solutions.first() {
                println!("Node voltages:");
                for (node, voltage) in &result.node_voltages {
                    println!("  {:?}: {:.6}V", node, voltage);
                }
                println!("Branch currents:");
                for (branch, current) in &result.branch_currents {
                    println!("  {:?}: {:.6}mA", branch, current * 1000.0);
                }
                
                // Extract voltages by position (assuming specific node order)
                let mut node_voltages: Vec<f64> = result.node_voltages.values().cloned().collect();
                node_voltages.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
                
                let mut branch_currents: Vec<f64> = result.branch_currents.values().cloned().collect();
                
                println!("Analysis (assuming node order):");
                if node_voltages.len() >= 2 && branch_currents.len() >= 1 {
                    let max_voltage = node_voltages.last().unwrap_or(&0.0);  // Should be VDD
                    let mid_voltage = if node_voltages.len() > 2 { node_voltages[node_voltages.len()-2] } else { 0.0 };  // LED anode
                    let led_current = branch_currents.iter().find(|&&c| c.abs() > 1e-6).unwrap_or(&0.0);
                    
                    println!("  Max voltage (VDD): {:.6}V", max_voltage);
                    println!("  Mid voltage (LED anode): {:.6}V", mid_voltage);
                    println!("  LED voltage drop: {:.6}V", mid_voltage);
                    println!("  LED current: {:.6}mA", led_current.abs() * 1000.0);
                    println!("  Resistor voltage: {:.6}V", max_voltage - mid_voltage);
                    println!("  Resistor current: {:.6}mA", ((max_voltage - mid_voltage) / 330.0) * 1000.0);
                }
            }
        }
        Err(e) => {
            println!("CPU failed: {:?}", e);
        }
    }
    
    // GPU Solution
    #[cfg(feature = "gpu")]
    {
        println!("\nGPU GLACIER Solution:");
        println!("{}", "-".repeat(30));
        
        let context = GpuContext::new().await?;
        let gpu_solver = GlacierFullGpuSolver::new(Arc::new(context), 100).await?;
        
        // Use the full multi-phase GLACIER algorithm just like CPU
        println!("Running full GPU GLACIER algorithm with Phase 0 -> Phase 2:");
        
        // Phase 0: Coarse scan to find good starting points
        match gpu_solver.phase0_coarse_scan(&circuit, 21).await {
            Ok(phase0_results) => {
                let converged_count = phase0_results.iter().filter(|r| r.converged != 0).count();
                println!("  Phase 0: {}/{} points converged", converged_count, phase0_results.len());
                
                // Now use the full GLACIER analysis (equivalent to CPU)
                // This should use the same algorithm as CPU GLACIER
                match gpu_solver.analyze_glacier(&circuit).await {
                    Ok(solutions) => {
                        if let Some((start, end, gradient, result)) = solutions.first() {
                            println!("  Phase 2: Found solution in region {:.0}%-{:.0}%", start * 100.0, end * 100.0);
                            
                            // Extract final solution
                            let led_current = result.branch_currents.values()
                                .map(|&c| c.abs())
                                .fold(0.0, f64::max);
                            
                            println!("Converged with full GLACIER algorithm");
                            println!("  LED current from multi-phase: {:.6}mA", led_current * 1000.0);
                        } else {
                            println!("  No solutions found with multi-phase algorithm");
                        }
                    }
                    Err(e) => {
                        println!("  Multi-phase algorithm failed: {:?}", e);
                        
                        // Fallback to single solve at 100%
                        println!("  Falling back to single solve at 100%");
                        match gpu_solver.solve_at_ramp(&circuit, 1.0, None).await {
                            Ok((solution, iters, error)) => {
                                println!("Converged in {} iterations, error: {:.2e}", iters, error);
                                
                                println!("Variable values:");
                                for (i, var) in solution.iter().enumerate() {
                                    let actual_value = match var.space {
                                        bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => var.value.exp(),
                                        _ => var.value,
                                    };
                                    println!("  {}: {} = {:.6} ({})", i, var.name, var.value, actual_value);
                                }
                
                // Extract specific values
                // Looking at the circuit: VDD -> R1 -> LED_anode -> LED -> GND
                // So we should have v_n0 (VDD) and v_n2 (LED anode), with ground being implicit
                
                let vdd_voltage = solution.iter()
                    .find(|v| v.name.contains("v_n0"))
                    .map(|v| v.value)
                    .unwrap_or(0.0);
                
                let led_anode_voltage = solution.iter()
                    .find(|v| v.name.contains("v_n2"))
                    .map(|v| v.value)
                    .unwrap_or(0.0);
                
                let led_current = solution.iter()
                    .find(|v| v.name.contains("i_b") && v.name.contains("b2"))
                    .map(|v| {
                        let current = match v.space {
                            bhdl_spice::generic_glacier_solver::VariableSpace::Logarithmic => v.value.exp(),
                            _ => v.value,
                        };
                        current
                    })
                    .unwrap_or(0.0);
                
                println!("Analysis:");
                println!("  VDD voltage: {:.6}V", vdd_voltage);
                println!("  LED anode voltage: {:.6}V", led_anode_voltage);
                println!("  LED voltage drop: {:.6}V", led_anode_voltage);
                println!("  LED current: {:.6}mA", led_current.abs() * 1000.0);
                println!("  Resistor voltage: {:.6}V", vdd_voltage - led_anode_voltage);
                println!("  Resistor current: {:.6}mA", ((vdd_voltage - led_anode_voltage) / 330.0) * 1000.0);
                
                // Check Shockley equation manually
                let is_sat = 1e-14;
                let n = 2.0;
                let vt = 0.026;
                let v_led = led_anode_voltage;
                let theoretical_current = is_sat * ((v_led / (n * vt)).exp() - 1.0);
                                println!("  Theoretical LED current (Shockley): {:.6}mA", theoretical_current * 1000.0);
                            }
                            Err(e) => {
                                println!("  Fallback solve failed: {:?}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("  Phase 0 failed: {:?}", e);
            }
        }
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("\nGPU GLACIER: Not available (compile with --features gpu)");
    }
    
    Ok(())
}

fn main() -> Result<()> {
    #[cfg(feature = "gpu")]
    {
        pollster::block_on(debug_simple_comparison())
    }
    
    #[cfg(not(feature = "gpu"))]
    {
        println!("GPU support not enabled. Run with: cargo run --features gpu");
        Ok(())
    }
}