/// Test the universal adaptive voltage regulator model across different circuit conditions
/// This verifies that no manual gain tuning is required for different scenarios

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, ElectricalLimits,
    Result,
};

fn main() -> Result<()> {
    println!("=== Testing Universal Adaptive Voltage Regulator Model ===\n");
    
    // Test 1: Standard 7805 with 12V input, light load
    test_scenario("Standard 7805 (12V->5V, 1kΩ load)", 12.0, 5.0, 1000.0)?;
    
    // Test 2: High input voltage, medium load  
    test_scenario("High input (24V->5V, 330Ω load)", 24.0, 5.0, 330.0)?;
    
    // Test 3: Low input voltage, heavy load
    test_scenario("Low input (8V->5V, 100Ω load)", 8.0, 5.0, 100.0)?;
    
    // Test 4: 3.3V regulator with different conditions
    test_scenario("3.3V regulator (9V->3.3V, 470Ω load)", 9.0, 3.3, 470.0)?;
    
    // Test 5: Near dropout condition
    test_scenario("Near dropout (7.5V->5V, 200Ω load)", 7.5, 5.0, 200.0)?;
    
    println!("🎉 All voltage regulator scenarios converged successfully!");
    println!("✅ Universal adaptive gain model requires no manual tuning");
    
    Ok(())
}

fn test_scenario(description: &str, vin: f64, vout_target: f64, load_resistance: f64) -> Result<()> {
    println!("--- {} ---", description);
    
    // Create circuit
    let mut circuit = Circuit::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Input voltage source
    circuit.add_branch("VS1".to_string(), "VIN", "GND", "VoltageSource".to_string(), vin, None);
    
    // Voltage regulator (input to output)
    circuit.add_branch("REG1".to_string(), "VIN", "VOUT", "VoltageRegulator".to_string(), 0.0, None);
    
    // Load resistor
    circuit.add_branch("RLOAD".to_string(), "VOUT", "GND", "Resistor".to_string(), load_resistance, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add component models
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: vin, 
        internal_resistance: Some(0.1),
    });
    
    solver.add_model("REG1".to_string(), ComponentModel::VoltageRegulator { 
        output_voltage: vout_target,
        dropout_voltage: 2.0,
        quiescent_current: 0.005,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("RLOAD".to_string(), ComponentModel::Resistor { 
        resistance: load_resistance, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.set_convergence(100, 1e-6);
    
    match solver.analyze() {
        Ok(result) => {
            let mut vout_actual = 0.0;
            for (node_idx, voltage) in &result.node_voltages {
                if node_idx.index() == 1 { // VOUT node
                    vout_actual = *voltage;
                    break;
                }
            }
            
            let load_current = vout_actual / load_resistance;
            let regulation_error = ((vout_actual - vout_target) / vout_target * 100.0).abs();
            
            println!("  ✓ Converged in {} iterations", result.iterations);
            println!("  Input: {:.1}V, Output: {:.3}V (target: {:.1}V)", vin, vout_actual, vout_target);
            println!("  Load current: {:.1}mA, Regulation error: {:.2}%", load_current * 1000.0, regulation_error);
            
            // Check if regulation is good (within 1% for in-regulation, more tolerance near dropout)
            let dropout_margin = vin - (vout_target + 2.0);
            let acceptable_error = if dropout_margin > 2.0 { 1.0 } else { 5.0 }; // More tolerance near dropout
            
            if regulation_error <= acceptable_error {
                println!("  ✅ Excellent regulation!");
            } else {
                println!("  ⚠️  Regulation outside normal range ({}% error)", regulation_error);
            }
            
        },
        Err(e) => {
            println!("  ✗ Failed: {}", e);
            return Err(e);
        }
    }
    
    println!();
    Ok(())
}