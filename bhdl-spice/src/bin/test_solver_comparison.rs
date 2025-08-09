//! Direct comparison between NonlinearDcAnalysis and GlacierSolver

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, NonlinearDcAnalysis, GlacierSolver};

fn main() -> Result<()> {
    println!("=== SOLVER COMPARISON ===\n");
    
    // Create identical circuit for both solvers: 1V -> 100Ω -> Diode -> GND
    let circuit1 = create_test_circuit();
    let circuit2 = create_test_circuit();
    
    println!("Circuit: 1V -> 100Ω -> Diode -> GND");
    println!("Using identical diode parameters:");
    println!("  Is = 1e-12 A");
    println!("  n = 1.0 (emission coefficient)");
    println!("  Vt = 0.026V\n");
    
    // Test NonlinearDcAnalysis
    println!("1. Testing NonlinearDcAnalysis:");
    println!("--------------------------------");
    let mut nonlinear = NonlinearDcAnalysis::new(circuit1);
    
    // Add models
    nonlinear.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 1.0,
        internal_resistance: None,
    });
    
    nonlinear.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    nonlinear.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.0,  // No offset
        forward_resistance: 0.1,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    
    match nonlinear.analyze() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Find diode voltage and current
            if let Some(v_out) = result.node_voltages.iter().find(|(n, _)| n.index() == 1).map(|(_, v)| v) {
                println!("  Diode voltage: {:.3}V", v_out);
                
                // Calculate current
                let i_diode = (1.0 - v_out) / 100.0;
                println!("  Diode current: {:.6}A", i_diode);
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    // Test GlacierSolver
    println!("\n2. Testing GlacierSolver:");
    println!("--------------------------------");
    let mut two_phase = GlacierSolver::new(circuit2);
    
    // Add identical models
    two_phase.add_model("V0".to_string(), ComponentModel::VoltageSource { 
        voltage: 1.0,
        internal_resistance: None,
    });
    
    two_phase.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    two_phase.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.0,
        forward_resistance: 0.1,
        reverse_current: 1e-12,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    });
    
    // Enable detailed debug output by modifying tolerance
    // (Since we can't modify the solver's debug flags directly)
    
    match two_phase.analyze() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            
            // Find diode voltage and current
            if let Some(v_out) = result.node_voltages.iter().find(|(n, _)| n.index() == 1).map(|(_, v)| v) {
                println!("  Diode voltage: {:.3}V", v_out);
                
                // Calculate current
                let i_diode = (1.0 - v_out) / 100.0;
                println!("  Diode current: {:.6}A", i_diode);
            }
        }
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    println!("\n3. Analysis:");
    println!("-------------");
    println!("The key difference is likely in:");
    println!("- Matrix formulation details");
    println!("- Convergence criteria");
    println!("- Ramping strategy");
    println!("- Numerical conditioning");
    
    Ok(())
}

fn create_test_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Add components
    circuit.add_branch("V0".to_string(), "in", "GND", "VoltageSource".to_string(), 1.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "Diode".to_string(), 0.0, None);
    
    circuit
}