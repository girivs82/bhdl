//! Test NonlinearDcAnalysis with the same simple circuit

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, NonlinearDcAnalysis};

fn main() -> Result<()> {
    println!("=== Testing NonlinearDcAnalysis ===\n");
    
    // Create same circuit: 1V -> 100Ω -> Diode -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "in", "GND", "VoltageSource".to_string(), 1.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
    println!("Circuit: 1V -> 100Ω -> Diode -> GND\n");
    
    // Create analyzer
    let mut analyzer = NonlinearDcAnalysis::new(circuit);
    
    // Add models
    analyzer.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 1.0,
        internal_resistance: None,
    });
    
    analyzer.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    analyzer.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.0,  // Standard diode, no offset like TwoPhase
        forward_resistance: 0.1,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),  // n=1 like TwoPhase  
        limits: ElectricalLimits::default(),
    });
    
    println!("Running NonlinearDcAnalysis...");
    
    // Analyze
    match analyzer.analyze() {
        Ok(result) => {
            println!("\n✓ Analysis completed successfully!");
            println!("  Iterations: {}", result.iterations);
            
            // Show voltages
            println!("\nNode voltages:");
            if let Some(v_in) = result.node_voltages.iter().find(|(n, _)| n.index() == 0).map(|(_, v)| v) {
                println!("  in:  {:.3}V", v_in);
            }
            if let Some(v_out) = result.node_voltages.iter().find(|(n, _)| n.index() == 1).map(|(_, v)| v) {
                println!("  out: {:.3}V", v_out);
            }
            println!("  GND: 0.000V");
            
            // Show currents
            println!("\nBranch currents:");
            let mut d1_current = None;
            for (edge_idx, current) in &result.branch_currents {
                // We need to match by index, not name
                println!("  Branch {}: {:.6}A", edge_idx.index(), current);
                if edge_idx.index() == 2 { // D1 is the third branch
                    d1_current = Some(*current);
                }
            }
            
            // Calculate diode voltage drop
            if let (Some(v_out), Some(i_diode)) = (
                result.node_voltages.iter().find(|(n, _)| n.index() == 1).map(|(_, v)| v),
                d1_current
            ) {
                println!("\nDiode analysis:");
                println!("  Voltage: {:.3}V", v_out);
                println!("  Current: {:.6}A", i_diode);
                
                // Verify using diode equation
                let is = 1e-12;
                let vt = 0.026;
                let expected_i = is * ((v_out / vt).exp() - 1.0);
                println!("  Expected current from equation: {:.6}A", expected_i);
                println!("  Error: {:.2}%", ((i_diode - expected_i) / expected_i * 100.0).abs());
            }
        }
        Err(e) => {
            println!("\n✗ Analysis failed: {}", e);
        }
    }
    
    Ok(())
}