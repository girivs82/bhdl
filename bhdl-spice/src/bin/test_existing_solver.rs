/// Test existing solvers to understand the correct setup
/// This will help us debug the adaptive solver integration

use bhdl_spice::{
    Circuit, ComponentModel, DcAnalysis, NonlinearDcAnalysis, ElectricalLimits,
    Result,
};

fn main() -> Result<()> {
    println!("=== Testing Existing Solvers ===");
    
    // Test with existing linear solver first
    test_linear_solver()?;
    
    // Test with existing nonlinear solver
    test_nonlinear_solver()?;
    
    Ok(())
}

fn test_linear_solver() -> Result<()> {
    println!("\n--- Testing Existing Linear Solver ---");
    
    // Create simple circuit: 5V -> R(1kΩ) -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut analysis = DcAnalysis::new(circuit);
    
    analysis.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    analysis.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    match analysis.analyze() {
        Ok(result) => {
            println!("✓ Linear solver succeeded:");
            println!("  Iterations: {}", result.iterations);
            println!("  Total Power: {:.3}W", result.total_power);
            
            // Print all node voltages
            for (node_idx, voltage) in &result.node_voltages {
                if let Some(node) = analysis.circuit().get_node_by_id(*node_idx) {
                    println!("  {}: {:.3}V", node.name, voltage);
                }
            }
        },
        Err(e) => {
            println!("✗ Linear solver failed: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}

fn test_nonlinear_solver() -> Result<()> {
    println!("\n--- Testing Existing Nonlinear Solver ---");
    
    // Create LED circuit: 5V -> R(330Ω) -> LED -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_NODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "LED_NODE", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "LED_NODE", "GND", "LED".to_string(), 2.0, None);
    
    let mut analysis = NonlinearDcAnalysis::new(circuit);
    
    analysis.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(1.0),
    });
    analysis.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    analysis.add_model("LED1".to_string(), ComponentModel::LED { 
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    match analysis.analyze() {
        Ok(result) => {
            println!("✓ Nonlinear solver succeeded:");
            println!("  Iterations: {}", result.iterations);
            println!("  Total Power: {:.3}W", result.total_power);
            
            // Print all node voltages
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.3}V", node_idx.index(), voltage);
            }
        },
        Err(e) => {
            println!("✗ Nonlinear solver failed: {}", e);
            return Err(e);
        }
    }
    
    Ok(())
}