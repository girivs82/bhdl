//! Test if the convergence fix works for multiple voltages

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Convergence Fix ===\n");
    
    let voltages = vec![5.0, 6.0, 7.0, 8.0, 9.0];
    
    for voltage in voltages {
        println!("Testing {}V...", voltage);
        
        let mut circuit = Circuit::new();
        circuit.add_node("VCC".to_string(), None);
        circuit.add_node("n1".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), voltage, None);
        circuit.add_branch("R1".to_string(), "VCC", "n1", "Resistor".to_string(), 330.0, None);
        circuit.add_branch("D1".to_string(), "n1", "GND", "LED".to_string(), 0.0, None);
        
        let mut glacier = GlacierSolver::new(circuit);
        
        glacier.add_model("V1".to_string(), ComponentModel::VoltageSource { 
            voltage, internal_resistance: None 
        });
        glacier.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 330.0, tolerance: 5.0, limits: ElectricalLimits::default() 
        });
        glacier.add_model("D1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 20e-3,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-12),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: ElectricalLimits::default(),
        });
        
        match glacier.analyze() {
            Ok(solutions) => {
                println!("  ✅ {}V: SUCCESS ({} solutions)", voltage, solutions.len());
            }
            Err(e) => {
                println!("  ❌ {}V: FAILED - {}", voltage, e);
            }
        }
    }
    
    Ok(())
}