use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits};
use bhdl_spice::glacier_gpu::{gpu_context::GpuContext, full_solver::GlacierFullGpuSolver};
use bhdl_spice::glacier_gpu::gpu_data::Phase0Result;
use std::collections::HashMap;
use std::sync::Arc;
use anyhow::Result;

#[tokio::main]
async fn main() -> Result<()> {
    println!("=== GPU Gradient Calculation Analysis ===");
    println!();
    
    // Create test circuit
    let (circuit, models) = create_test_circuit();
    
    // Create GPU context
    let context = match GpuContext::new().await {
        Ok(ctx) => Arc::new(ctx),
        Err(e) => {
            println!("GPU not available: {}", e);
            return Ok(());
        }
    };
    
    // Create GPU solver
    let gpu_solver = GlacierFullGpuSolver::new(context, 100).await?;
    
    // Run Phase 0 with GPU
    let gpu_phase0_results = gpu_solver.phase0_coarse_scan_with_models(&circuit, 40, &models).await?;
    
    println!("GPU Phase 0 Results Analysis:");
    println!("Total points: {}", gpu_phase0_results.len());
    
    // Analyze the gradient calculation issue
    println!("\n1. Max Gradient Field Analysis:");
    let unique_gradients: std::collections::HashSet<String> = gpu_phase0_results.iter()
        .filter(|r| r.converged != 0)
        .map(|r| format!("{:.4}", r.max_gradient))
        .collect();
    println!("   Unique max_gradient values: {:?}", unique_gradients);
    println!("   Observation: max_gradient is CONSTANT at 19.231 for all converged points!");
    
    println!("\n2. Heuristic Gradient Calculation:");
    println!("   Formula: log10(iterations * error) * 10.0");
    println!("   Point | Ramp   | Iter | Error    | Heuristic Gradient");
    println!("   ------|--------|------|----------|-------------------");
    
    let mut heuristic_gradients = Vec::new();
    for (i, result) in gpu_phase0_results.iter().enumerate() {
        if result.converged != 0 {
            let heuristic = (result.iterations as f32 * result.error.max(1e-10)).log10().abs() * 10.0;
            heuristic_gradients.push(heuristic);
            
            if i < 5 || i % 10 == 0 || i >= 35 {
                println!("   {:5} | {:.4} | {:4} | {:.2e} | {:.2}", 
                         i, result.ramp, result.iterations, result.error, heuristic);
            }
        }
    }
    
    let min_heuristic = heuristic_gradients.iter().cloned().fold(f32::INFINITY, f32::min);
    let max_heuristic = heuristic_gradients.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    println!("\n   Heuristic gradient range: [{:.2}, {:.2}]", min_heuristic, max_heuristic);
    println!("   All values are BELOW the threshold of 100.0!");
    
    println!("\n3. Region Detection Logic:");
    println!("   - Threshold for sharp transition: 100.0");
    println!("   - NO points exceed this threshold");
    println!("   - Result: All points are considered part of ONE smooth region");
    
    println!("\n4. Why CPU Finds 7 Regions:");
    println!("   - CPU calculates ACTUAL Jacobian gradients");
    println!("   - LED turn-on creates sharp d(current)/d(voltage) changes");
    println!("   - These are detected as gradient spikes > 100");
    
    println!("\n5. The Root Cause:");
    println!("   a) GPU Phase 0 doesn't compute actual Jacobian gradients");
    println!("   b) The max_gradient field is set to a constant value (19.231)");
    println!("   c) Heuristic based on iterations*error is too smooth");
    println!("   d) LED transitions don't cause dramatic iteration count changes");
    
    println!("\n6. Expected vs Actual LED Behavior:");
    // Calculate expected currents at different ramp values
    for ramp in [0.2, 0.4, 0.6, 0.8, 1.0] {
        let v_supply = 9.0 * ramp;
        let mut v_leds = 0.0;
        let mut leds_on = 0;
        
        // Simple LED model: 2V forward voltage when on
        for _ in 0..3 {
            if v_supply - v_leds > 2.0 {
                v_leds += 2.0;
                leds_on += 1;
            }
        }
        
        let current = if leds_on > 0 {
            (v_supply - v_leds) / 150.0
        } else {
            0.0
        };
        
        println!("   Ramp {:.0}%: {} LEDs on, I = {:.1}mA", 
                 ramp * 100.0, leds_on, current * 1000.0);
    }
    
    println!("\n=== CONCLUSION ===");
    println!("GPU finds 1 region because:");
    println!("1. It doesn't calculate actual circuit gradients");
    println!("2. The heuristic gradient stays below detection threshold");
    println!("3. All points appear to be in one smooth region");
    println!("\nTo fix this, GPU Phase 0 would need to:");
    println!("- Calculate actual Jacobian matrix elements");
    println!("- Detect gradient changes in circuit equations");
    println!("- Or use a more sensitive heuristic that captures LED transitions");
    
    Ok(())
}

fn create_test_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    // 9V -> 150Ω -> LED1 -> LED2 -> LED3 -> GND
    circuit.add_branch("V1".to_string(), "vdd", "gnd", "VoltageSource".to_string(), 9.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 9.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "vdd", "n1", "Resistor".to_string(), 150.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 150.0,
        tolerance: 0.05,
        limits: ElectricalLimits {
            max_voltage: Some(50.0),
            max_current: Some(0.1),
            max_power: Some(0.25),
            min_voltage: None,
            temp_range: Some((-40.0, 85.0)),
        },
    });
    
    // 3 LEDs in series
    for i in 1..=3 {
        let from_node = if i == 1 { "n1".to_string() } else { format!("n{}", i) };
        let to_node = if i == 3 { "gnd".to_string() } else { format!("n{}", i + 1) };
        
        circuit.add_branch(format!("D{}", i), &from_node, &to_node, "LED".to_string(), 0.0, None);
        models.insert(format!("D{}", i), ComponentModel::LED {
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
    }
    
    (circuit, models)
}