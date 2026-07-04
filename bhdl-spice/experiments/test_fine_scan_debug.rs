//! Debug test for fine scan phase

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Fine Scan Debug Test ===\n");
    
    // Test with Ultra-sharp LED (Is=1e-16) which showed good convergence in scan
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-16),  // Ultra-sharp
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    println!("Testing Ultra-sharp LED (Is=1e-16)");
    println!("This should find converged points around ramp=0.48 with error ~3.38e-7\n");
    
    // Run the analysis
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ Analysis succeeded!");
            println!("Found {} solutions", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (region {:.1}%-{:.1}%):", i+1, start*100.0, end*100.0);
                
                if let (Some((_, v_in)), Some((_, v_out))) = (
                    result.node_voltages.iter().find(|(idx, _)| idx.index() == 0),
                    result.node_voltages.iter().find(|(idx, _)| idx.index() == 1)
                ) {
                    let i_circuit = (v_in - v_out) / 470.0;
                    let v_led = *v_out;
                    println!("  V_LED = {:.3}V, I = {:.2}mA", v_led, i_circuit * 1000.0);
                }
            }
        }
        Err(e) => {
            println!("\n❌ Analysis failed: {}", e);
            println!("\nThis suggests the fine scan is not successfully jumping to 100%");
            println!("or Phase 2 is still having issues converging from the fine scan point.");
        }
    }
    
    Ok(())
}