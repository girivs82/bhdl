/// Test the adaptive solver on existing BHDL circuits
/// This demonstrates the solver working on real-world circuit examples

use bhdl_spice::{
    Circuit, ComponentModel, AdaptiveCircuitSolver, ElectricalLimits,
    Result,
};

fn main() -> Result<()> {
    println!("=== Testing Adaptive Solver on BHDL Circuits ===\n");
    
    // Test 1: Simple LED circuit 
    test_simple_led_circuit()?;
    
    // Test 2: RC circuit
    test_rc_circuit()?;
    
    // Test 3: Voltage divider
    test_voltage_divider()?;
    
    // Test 4: LED with resistor (nonlinear)
    test_led_with_resistor()?;
    
    Ok(())
}

fn test_simple_led_circuit() -> Result<()> {
    println!("--- Test 1: Simple LED Circuit ---");
    println!("Circuit: 5V -> 330Ω -> Red LED -> GND");
    
    // Create circuit: VCC -> R(330Ω) -> LED(red) -> GND
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_NODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "LED_NODE", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("LED1".to_string(), "LED_NODE", "GND", "LED".to_string(), 2.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(0.1),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    solver.add_model("LED1".to_string(), ComponentModel::LED { 
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.set_convergence(100, 1e-6);
    
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Total Power: {:.3}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.3}V", node_idx.index(), voltage);
            }
            
            // Expected: VCC ≈ 5V, LED_NODE ≈ 2V (LED forward voltage), GND = 0V
            // Current ≈ (5V - 2V) / 330Ω ≈ 9.1mA
            println!("  Expected: LED current ≈ 9.1mA, LED voltage ≈ 2V");
        },
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    println!();
    Ok(())
}

fn test_rc_circuit() -> Result<()> {
    println!("--- Test 2: RC Circuit ---");
    println!("Circuit: 5V -> 1kΩ -> [no capacitor for DC analysis] -> GND");
    
    // Create simple RC circuit (DC analysis, so capacitor acts as open circuit)
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("RC_NODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "RC_NODE", "Resistor".to_string(), 1000.0, None);
    // Capacitor omitted for DC analysis (acts as open circuit)
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0, 
        internal_resistance: Some(0.1),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.set_convergence(100, 1e-6);
    
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Total Power: {:.6}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.3}V", node_idx.index(), voltage);
            }
            
            // Expected: RC_NODE ≈ 5V (no current flow through open capacitor)
            println!("  Expected: RC_NODE ≈ 5V (open circuit at DC)");
        },
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    println!();
    Ok(())
}

fn test_voltage_divider() -> Result<()> {
    println!("--- Test 3: Voltage Divider ---");
    println!("Circuit: 12V -> 2kΩ -> [mid] -> 1kΩ -> GND");
    
    // Create voltage divider: 12V across 2kΩ and 1kΩ
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("MID".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "MID", "Resistor".to_string(), 2000.0, None);
    circuit.add_branch("R2".to_string(), "MID", "GND", "Resistor".to_string(), 1000.0, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0, 
        internal_resistance: Some(0.1),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 2000.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    solver.add_model("R2".to_string(), ComponentModel::Resistor { 
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
                println!("  Node {}: {:.3}V", node_idx.index(), voltage);
            }
            
            // Expected: MID = 12V * 1kΩ/(2kΩ + 1kΩ) = 4V
            // Current = 12V / 3kΩ = 4mA
            // Power = 12V * 4mA = 48mW
            println!("  Expected: MID = 4V, Current = 4mA, Power = 48mW");
        },
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    println!();
    Ok(())
}

fn test_led_with_resistor() -> Result<()> {
    println!("--- Test 4: LED with Current Limiting Resistor ---");
    println!("Circuit: 9V -> 220Ω -> Blue LED -> GND");
    
    // Create LED circuit with higher voltage and blue LED
    let mut circuit = Circuit::new();
    
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("LED_NODE".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("VS1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 9.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "LED_NODE", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("LED1".to_string(), "LED_NODE", "GND", "LED".to_string(), 3.2, None);
    
    let mut solver = AdaptiveCircuitSolver::new(circuit);
    
    solver.add_model("VS1".to_string(), ComponentModel::VoltageSource { 
        voltage: 9.0, 
        internal_resistance: Some(0.1),
    });
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0, 
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    solver.add_model("LED1".to_string(), ComponentModel::LED { 
        color: "blue".to_string(),
        forward_voltage: 3.2,
        forward_current: 0.02,
        dynamic_resistance: 8.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.set_convergence(100, 1e-6);
    
    match solver.analyze() {
        Ok(result) => {
            println!("✓ Converged in {} iterations", result.iterations);
            println!("  Total Power: {:.3}W", result.total_power);
            
            for (node_idx, voltage) in &result.node_voltages {
                println!("  Node {}: {:.3}V", node_idx.index(), voltage);
            }
            
            // Expected: LED_NODE ≈ 3.2V (blue LED forward voltage)
            // Current ≈ (9V - 3.2V) / 220Ω ≈ 26.4mA
            // Power ≈ 9V * 26.4mA ≈ 238mW
            println!("  Expected: LED current ≈ 26.4mA, LED voltage ≈ 3.2V, Power ≈ 238mW");
        },
        Err(e) => {
            println!("✗ Failed: {}", e);
        }
    }
    
    println!();
    Ok(())
}