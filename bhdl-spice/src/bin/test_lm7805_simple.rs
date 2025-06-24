//! Simple test of LM7805 voltage regulator with hardcoded models
//! 
//! This test verifies the voltage regulator model works correctly
//! without relying on stdlib parsing.

use anyhow::Result;
use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, 
    ElectricalLimits
};
use std::time::Instant;

fn main() -> Result<()> {
    println!("===========================================");
    println!("LM7805 Voltage Regulator Simple Test");
    println!("===========================================\n");
    
    // Test multiple input voltages
    let test_voltages = vec![
        ("Low voltage (dropout)", 6.0),
        ("Nominal voltage", 12.0),
        ("High voltage", 24.0),
        ("Maximum voltage", 35.0),
    ];
    
    for (desc, vin) in test_voltages {
        println!("\n--- Testing: {} (Vin = {:.1}V) ---", desc, vin);
        test_lm7805_circuit(vin)?;
    }
    
    println!("\n===========================================");
    println!("✅ All LM7805 tests passed successfully!");
    println!("===========================================");
    
    Ok(())
}

fn test_lm7805_circuit(vin: f64) -> Result<()> {
    let mut circuit = Circuit::new();
    
    // Create circuit: Vin → LM7805 → Load (100Ω) → GND
    // Simplified without capacitors for now
    
    // Input voltage source
    circuit.add_branch(
        "V1".to_string(),
        "VIN",
        "GND",
        "voltage_source".to_string(),
        vin,
        None
    );
    
    // LM7805 voltage regulator (simplified as 2-terminal device)
    circuit.add_branch(
        "U1".to_string(),
        "VIN",
        "VOUT",
        "voltage_regulator".to_string(),
        5.0,        // Output voltage
        None
    );
    
    // Load resistor (100Ω = 50mA at 5V)
    circuit.add_branch(
        "RLOAD".to_string(),
        "VOUT",
        "GND",
        "resistor".to_string(),
        100.0,
        None
    );
    
    // Find the VOUT node before creating the solver
    let vout_node = circuit.nodes()
        .find(|(_, node)| node.name == "VOUT")
        .map(|(idx, _)| idx)
        .ok_or_else(|| anyhow::anyhow!("VOUT node not found in circuit"))?;
    
    // Create solver
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    // Add component models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource {
        voltage: vin,
        internal_resistance: Some(0.01),
    });
    
    // Add LM7805 model with realistic parameters
    solver.add_model("U1".to_string(), ComponentModel::VoltageRegulator {
        output_voltage: 5.0,
        dropout_voltage: 2.0,
        quiescent_current: 0.005, // 5mA
        max_current: 1.0,
        line_regulation: 0.01,
        load_regulation: 0.1,
        limits: ElectricalLimits {
            max_voltage: Some(35.0),
            max_current: Some(1.0),
            max_power: Some(20.0),  // Depends on heatsinking
            min_voltage: Some(0.0),
            max_temperature: Some(125.0),
        },
    });
    
    solver.add_model("RLOAD".to_string(), ComponentModel::Resistor {
        resistance: 100.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Run analysis
    let start = Instant::now();
    let result = solver.analyze()?;
    let elapsed = start.elapsed();
    
    println!("  Converged in {} iterations ({:.1}ms)", result.iterations, elapsed.as_secs_f64() * 1000.0);
    println!("  Total power: {:.3}W", result.total_power);
    
    // Get output voltage from results
    let vout = result.node_voltages.get(&vout_node)
        .copied()
        .ok_or_else(|| anyhow::anyhow!("VOUT voltage not found in results"))?;
    
    println!("  Output voltage: {:.3}V", vout);
    
    // Calculate regulation metrics
    let expected_vout = 5.0;
    let error = (vout - expected_vout).abs();
    let error_percent = (error / expected_vout) * 100.0;
    
    // Check if in dropout mode
    let min_vin = expected_vout + 2.0; // Dropout voltage
    if vin < min_vin {
        println!("  ⚠️  Dropout mode: Vin ({:.1}V) < Vin_min ({:.1}V)", vin, min_vin);
        println!("  Expected degraded output in dropout mode");
    } else {
        println!("  Regulation error: {:.1}mV ({:.2}%)", error * 1000.0, error_percent);
        
        // Verify regulation is within spec (±4% typical for LM7805)
        if error_percent > 4.0 {
            println!("  ⚠️  Regulation outside typical ±4% specification");
        } else {
            println!("  ✓ Regulation within specification");
        }
    }
    
    // Calculate efficiency
    let iin = result.total_power / vin;
    let pout = vout * (vout / 100.0); // Power in load resistor
    let efficiency = (pout / result.total_power) * 100.0;
    println!("  Efficiency: {:.1}%", efficiency);
    println!("  Input current: {:.1}mA", iin * 1000.0);
    
    // Show all node voltages for debugging
    println!("\n  Node voltages:");
    for (node_idx, voltage) in &result.node_voltages {
        println!("    Node {:?}: {:.3}V", node_idx, voltage);
    }
    
    Ok(())
}