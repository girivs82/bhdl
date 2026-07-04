//! Demonstrate the DC operating point selection issue in transient solver

use anyhow::Result;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    println!("\n=== TRANSIENT SOLVER DC SELECTION ISSUE ===\n");
    
    // Create a circuit with multiple LEDs that will have multiple solutions
    println!("1. Creating test circuit with 3 LEDs in series...");
    let mut circuit = Circuit::new();
    
    // Nodes
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    circuit.add_node("N4".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Components
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "N3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "N3", "N4", "LED".to_string(), 0.0, None);
    circuit.add_branch("R2".to_string(), "N4", "GND", "Resistor".to_string(), 10.0, None);
    
    // Load models with different Is values to create multiple solutions
    let mut models = std::collections::HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 5.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    models.insert("R2".to_string(), StdlibModelLoader::create_resistor_model("R2", 10.0, None));
    
    // LEDs with different Is values
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),  // Moderate Is
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-20),  // Sharp Is
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    models.insert("D3".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-25),  // Very sharp Is
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Create solver and add models
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in models {
        solver.add_model(name, model);
    }
    
    // Step 1: Get all DC solutions
    println!("\n2. Finding all DC solutions with GLACIER...");
    let all_solutions = solver.analyze()?;
    
    println!("\nFound {} solutions:", all_solutions.len());
    for (i, (ramp_start, ramp_end, gradient, result)) in all_solutions.iter().enumerate() {
        println!("\nSolution {}: ramp=[{:.1}%-{:.1}%], gradient={:.1}", 
                 i+1, ramp_start*100.0, ramp_end*100.0, gradient);
        println!("  Total power: {:.3}W", result.total_power);
        
        // Check LED states
        let i_d1 = result.branch_currents.get(&circuit.get_branch("D1").unwrap().0)
            .copied().unwrap_or(0.0);
        let i_d2 = result.branch_currents.get(&circuit.get_branch("D2").unwrap().0)
            .copied().unwrap_or(0.0);
        let i_d3 = result.branch_currents.get(&circuit.get_branch("D3").unwrap().0)
            .copied().unwrap_or(0.0);
        
        let d1_on = i_d1 > 0.001;  // > 1mA
        let d2_on = i_d2 > 0.001;
        let d3_on = i_d3 > 0.001;
        
        println!("  LED states: D1={}, D2={}, D3={}", 
                 if d1_on { "ON" } else { "OFF" },
                 if d2_on { "ON" } else { "OFF" },
                 if d3_on { "ON" } else { "OFF" });
        println!("  Currents: I(D1)={:.3}mA, I(D2)={:.3}mA, I(D3)={:.3}mA",
                 i_d1*1000.0, i_d2*1000.0, i_d3*1000.0);
                 
        // Calculate voltage drops
        let v_n1 = result.node_voltages.get(&circuit.get_node("N1").unwrap().0)
            .copied().unwrap_or(0.0);
        let v_n2 = result.node_voltages.get(&circuit.get_node("N2").unwrap().0)
            .copied().unwrap_or(0.0);
        let v_n3 = result.node_voltages.get(&circuit.get_node("N3").unwrap().0)
            .copied().unwrap_or(0.0);
        let v_n4 = result.node_voltages.get(&circuit.get_node("N4").unwrap().0)
            .copied().unwrap_or(0.0);
        
        println!("  Voltages: V(N1)={:.3}V, V(N2)={:.3}V, V(N3)={:.3}V, V(N4)={:.3}V",
                 v_n1, v_n2, v_n3, v_n4);
    }
    
    // Step 2: Show which one would be selected by transient solver
    println!("\n3. Transient solver DC selection:");
    let selected = all_solutions.iter()
        .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap());
        
    if let Some((ramp_start, ramp_end, gradient, result)) = selected {
        println!("\nTransient solver would select: ramp=[{:.1}%-{:.1}%]", 
                 ramp_start*100.0, ramp_end*100.0);
        println!("  Reason: Maximum power = {:.3}W", result.total_power);
        
        // Analyze if this is physically reasonable
        let i_total = result.branch_currents.get(&circuit.get_branch("R1").unwrap().0)
            .copied().unwrap_or(0.0);
        println!("  Total current: {:.3}mA", i_total*1000.0);
        
        // Check for issues
        println!("\n4. Physical validity analysis:");
        
        if result.total_power > 0.5 {
            println!("  ⚠️  WARNING: Very high power dissipation!");
            println!("     This might be an unstable or non-physical solution");
        }
        
        if i_total > 0.050 {  // > 50mA
            println!("  ⚠️  WARNING: High current through LEDs!");
            println!("     This could damage the components");
        }
        
        // Check if solution makes sense
        let v_across_leds = 5.0 - result.node_voltages.get(&circuit.get_node("N4").unwrap().0)
            .copied().unwrap_or(0.0);
        let expected_drop_per_led = v_across_leds / 3.0;
        
        println!("  Total voltage across LEDs: {:.3}V", v_across_leds);
        println!("  Average drop per LED: {:.3}V", expected_drop_per_led);
        
        if expected_drop_per_led > 3.0 || expected_drop_per_led < 1.5 {
            println!("  ⚠️  WARNING: Unusual voltage distribution!");
        }
    }
    
    // Step 3: Compare with what MAESTRO would select
    println!("\n5. Better selection criteria:");
    println!("\nMAESTRO would consider:");
    println!("  - Physical LED operating points (1.8-2.2V for red LEDs)");
    println!("  - Reasonable current levels (5-30mA typical)");
    println!("  - Stable operating regions");
    println!("  - Component safety limits");
    
    // Find the most physically reasonable solution
    let mut best_solution = None;
    let mut best_score = f64::NEG_INFINITY;
    
    for (i, (ramp_start, ramp_end, gradient, result)) in all_solutions.iter().enumerate() {
        let i_avg = result.branch_currents.values()
            .filter(|&&i| i > 0.0)
            .sum::<f64>() / 3.0;
            
        // Score based on:
        // - Current in reasonable range (penalty for too high/low)
        // - Lower power is better
        // - Stable gradient
        let current_score = if i_avg > 0.005 && i_avg < 0.030 {
            1.0 - ((i_avg - 0.015).abs() / 0.015)  // Best at 15mA
        } else {
            -1.0  // Penalty for out of range
        };
        
        let power_score = -result.total_power * 10.0;  // Lower power is better
        let gradient_score = -gradient.ln().max(0.0) / 10.0;  // Lower gradient is better
        
        let total_score = current_score + power_score + gradient_score;
        
        println!("\nSolution {} score: {:.3} (current: {:.3}, power: {:.3}, gradient: {:.3})",
                 i+1, total_score, current_score, power_score, gradient_score);
                 
        if total_score > best_score {
            best_score = total_score;
            best_solution = Some(i);
        }
    }
    
    if let Some(idx) = best_solution {
        println!("\n✓ Physically optimal solution: #{}", idx + 1);
        println!("  This has reasonable current and power characteristics");
    }
    
    println!("\n=== CONCLUSION ===");
    println!("The transient solver's 'max power' selection can lead to:");
    println!("- Non-physical high-power states");
    println!("- Component damage from overcurrent");
    println!("- Unstable operating points");
    println!("\nBetter approach would be to:");
    println!("1. Use MAESTRO for intelligent selection");
    println!("2. Consider physical constraints");
    println!("3. Prefer stable, low-power solutions");
    println!("4. Allow user to specify selection criteria");
    
    Ok(())
}