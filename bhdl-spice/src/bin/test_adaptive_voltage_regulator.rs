/// Test the adaptive solver on a voltage regulator circuit
/// This should work with the existing VoltageRegulator ComponentModel

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, ElectricalLimits,
    Result,
};

fn main() -> Result<()> {
    println!("=== Testing Adaptive Solver on Voltage Regulator Circuit ===\n");
    
    test_7805_regulator()?;
    
    Ok(())
}

fn test_7805_regulator() -> Result<()> {
    println!("--- 7805 Linear Voltage Regulator Circuit ---");
    println!("Circuit: 12V -> 7805(IN-OUT) -> Load, GND common");
    
    // Create circuit: VIN(12V) -> 7805.IN, 7805.OUT -> VOUT, GND common
    let mut circuit = Circuit::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Input voltage source (12V supply)
    circuit.add_branch("VS1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 12.0, None);
    
    // 7805 voltage regulator (input to output)
    circuit.add_branch("REG1".to_string(), "VIN", "VOUT", "VoltageRegulator".to_string(), 0.0, None);
    
    // Load resistor (1kΩ load for 5mA output current at 5V)
    circuit.add_branch("RLOAD".to_string(), "VOUT", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add component models
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0, 
        internal_resistance: Some(0.1),
    });
    
    solver.add_model("REG1".to_string(), ComponentModel::VoltageRegulator { 
        output_voltage: 5.0,
        dropout_voltage: 2.0,
        quiescent_current: 0.005,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("RLOAD".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.set_convergence(100, 1e-6);
    
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Total Power: {:.3}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                let node_name = match node_idx.index() {
                    0 => "VIN",
                    1 => "VOUT", 
                    2 => "GND",
                    _ => "Unknown"
                };
                println!("  {}: {:.3}V", node_name, voltage);
            }
            
            // Calculate load current
            if let Some((_, vout)) = result.node_voltages.iter().find(|(idx, _)| idx.index() == 1) {
                let load_current = vout / 1000.0; // I = V/R
                println!("  Load current: {:.1}mA", load_current * 1000.0);
                println!("  Expected: VOUT ≈ 5V, Load current ≈ 5mA");
            }
        },
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    println!();
    Ok(())
}