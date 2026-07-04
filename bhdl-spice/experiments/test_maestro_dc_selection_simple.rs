//! Simple demonstration of MAESTRO DC selection vs max power heuristic

use anyhow::Result;
use std::collections::HashMap;
use bhdl_spice::{
    Circuit, ComponentModel,
    GlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};

fn main() -> Result<()> {
    println!("\n=== MAESTRO vs MAX POWER DC SELECTION DEMO ===\n");
    
    // Create test circuit with multiple possible operating points
    println!("1. Creating circuit with 3 LEDs in series...");
    let mut circuit = Circuit::new();
    
    // Nodes
    circuit.add_node("VDD".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("N2".to_string(), None);
    circuit.add_node("N3".to_string(), None);
    circuit.add_node("N4".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Components
    circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "N1", "N2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "N2", "N3", "LED".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "N3", "N4", "LED".to_string(), 0.0, None);
    circuit.add_branch("R2".to_string(), "N4", "GND", "Resistor".to_string(), 10.0, None);
    
    // Models
    let mut models = HashMap::new();
    models.insert("V1".to_string(), StdlibModelLoader::create_voltage_source_model("V1", 9.0));
    models.insert("R1".to_string(), StdlibModelLoader::create_resistor_model("R1", 100.0, None));
    models.insert("R2".to_string(), StdlibModelLoader::create_resistor_model("R2", 10.0, None));
    
    // LEDs with different Is values to create multiple solutions
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),  // Moderate Is
        emission_coefficient: Some(1.8),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    models.insert("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-15),  // Sharper Is
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    models.insert("D3".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.020,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-18),  // Very sharp Is
        emission_coefficient: Some(2.2),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    // Step 1: Find all DC solutions with GLACIER
    println!("\n2. Finding all DC solutions with GLACIER...");
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    let all_solutions = solver.analyze()?;
    println!("   Found {} solutions", all_solutions.len());
    
    // Display all solutions
    println!("\n3. Analysis of all solutions:");
    for (i, (ramp_start, ramp_end, gradient, result)) in all_solutions.iter().enumerate() {
        println!("\n   Solution {}: ramp=[{:.1}%-{:.1}%], gradient={:.1}", 
                 i+1, ramp_start*100.0, ramp_end*100.0, gradient);
        println!("     Total power: {:.3}W", result.total_power);
        
        // Get currents
        let i_r1 = result.branch_currents.get(&circuit.get_branch("R1").unwrap().0)
            .copied().unwrap_or(0.0);
        let i_d1 = result.branch_currents.get(&circuit.get_branch("D1").unwrap().0)
            .copied().unwrap_or(0.0);
        let i_d2 = result.branch_currents.get(&circuit.get_branch("D2").unwrap().0)
            .copied().unwrap_or(0.0);
        let i_d3 = result.branch_currents.get(&circuit.get_branch("D3").unwrap().0)
            .copied().unwrap_or(0.0);
        
        println!("     Total current: {:.1}mA", i_r1 * 1000.0);
        println!("     LED currents: D1={:.1}mA, D2={:.1}mA, D3={:.1}mA",
                 i_d1*1000.0, i_d2*1000.0, i_d3*1000.0);
        
        // Check LED states
        let d1_on = i_d1 > 0.001;
        let d2_on = i_d2 > 0.001;
        let d3_on = i_d3 > 0.001;
        let leds_on = [d1_on, d2_on, d3_on].iter().filter(|&&x| x).count();
        
        println!("     LED states: {} LEDs ON", leds_on);
        
        // Physical validity check
        let physical_score = evaluate_physical_validity(i_r1, result.total_power);
        println!("     Physical score: {:.2}", physical_score);
    }
    
    // Step 2: Show transient solver's max power selection
    println!("\n4. Current transient solver selection (MAX POWER):");
    let max_power_solution = all_solutions.iter()
        .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap());
        
    if let Some((_, _, _, result)) = max_power_solution {
        println!("   Selected: Power = {:.3}W", result.total_power);
        let i_total = result.branch_currents.get(&circuit.get_branch("R1").unwrap().0)
            .copied().unwrap_or(0.0);
        println!("   Total current: {:.1}mA", i_total * 1000.0);
        
        if result.total_power > 0.5 || i_total > 0.050 {
            println!("   ⚠️  WARNING: This selection may damage components!");
        }
    }
    
    // Step 3: Show what MAESTRO would select
    println!("\n5. MAESTRO-style intelligent selection:");
    
    // Find best solution based on physical criteria
    let best_solution = all_solutions.iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| {
            let i_a = a.3.branch_currents.get(&circuit.get_branch("R1").unwrap().0)
                .copied().unwrap_or(0.0);
            let i_b = b.3.branch_currents.get(&circuit.get_branch("R1").unwrap().0)
                .copied().unwrap_or(0.0);
            
            let score_a = evaluate_physical_validity(i_a, a.3.total_power);
            let score_b = evaluate_physical_validity(i_b, b.3.total_power);
            
            score_a.partial_cmp(&score_b).unwrap()
        });
        
    if let Some((idx, (_, _, _, result))) = best_solution {
        println!("   Selected: Solution {} with Power = {:.3}W", idx + 1, result.total_power);
        let i_total = result.branch_currents.get(&circuit.get_branch("R1").unwrap().0)
            .copied().unwrap_or(0.0);
        println!("   Total current: {:.1}mA", i_total * 1000.0);
        println!("   ✓ This solution optimizes for component safety and efficiency");
    }
    
    // Step 4: Demonstrate the problem
    println!("\n6. Why MAX POWER selection is problematic:");
    println!("   - May select high-power dissipation states");
    println!("   - Can exceed component ratings");
    println!("   - Often non-physical or unstable");
    println!("   - Doesn't consider efficiency or thermal limits");
    
    println!("\n7. MAESTRO selection criteria:");
    println!("   - Reasonable current levels (5-30mA for LEDs)");
    println!("   - Minimal power dissipation");
    println!("   - Component safety margins");
    println!("   - Stable operating regions");
    
    // Save analysis
    save_analysis(&all_solutions, &circuit)?;
    
    println!("\n=== CONCLUSION ===");
    println!("The transient solver should use MAESTRO's intelligent selection");
    println!("instead of blindly choosing the maximum power solution.");
    
    Ok(())
}

fn evaluate_physical_validity(current: f64, power: f64) -> f64 {
    let mut score = 1.0;
    
    // Current in reasonable range (5-30mA)
    if current > 0.005 && current < 0.030 {
        score *= 1.0 - ((current - 0.015).abs() / 0.015);  // Best at 15mA
    } else if current < 0.001 {
        score *= 0.1;  // Very low current
    } else {
        score *= 0.2;  // Too high current
    }
    
    // Power dissipation penalty
    if power < 0.2 {
        score *= 1.0;
    } else if power < 0.5 {
        score *= 0.8;
    } else {
        score *= 0.3;  // Too high power
    }
    
    score
}

fn save_analysis(
    solutions: &[(f64, f64, f64, bhdl_spice::AnalysisResult)],
    circuit: &Circuit
) -> Result<()> {
    use std::io::Write;
    
    let path = "tests/outputs/simulation/maestro_vs_max_power_analysis.txt";
    std::fs::create_dir_all(std::path::Path::new(path).parent().unwrap())?;
    let mut file = std::fs::File::create(path)?;
    
    writeln!(file, "=== MAESTRO vs MAX POWER SELECTION ANALYSIS ===\n")?;
    
    writeln!(file, "Circuit: 3 LEDs in series with 9V supply\n")?;
    
    writeln!(file, "All Solutions:")?;
    for (i, (_, _, _, result)) in solutions.iter().enumerate() {
        let i_total = result.branch_currents.get(&circuit.get_branch("R1").unwrap().0)
            .copied().unwrap_or(0.0);
        let score = evaluate_physical_validity(i_total, result.total_power);
        
        writeln!(file, "  Solution {}: Power={:.3}W, Current={:.1}mA, Score={:.2}", 
                 i+1, result.total_power, i_total * 1000.0, score)?;
    }
    
    println!("\n   Analysis saved to: {}", path);
    
    Ok(())
}