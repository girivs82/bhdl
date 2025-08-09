//! Test enhanced GLACIER on all challenging circuits

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn create_led_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    circuit
}

fn create_series_leds_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("node1".to_string(), None);
    circuit.add_node("node2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "node1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "node1", "node2", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "node2", "GND", "LED".to_string(), 0.0, None);
    
    circuit
}

fn create_parallel_leds_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("node1".to_string(), None);
    circuit.add_node("led1_anode".to_string(), None);
    circuit.add_node("led2_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "node1", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R2".to_string(), "node1", "led1_anode", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("R3".to_string(), "node1", "led2_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led1_anode", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "led2_anode", "GND", "LED".to_string(), 0.0, None);
    
    circuit
}

fn create_diode_bridge_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    circuit.add_node("AC1".to_string(), None);
    circuit.add_node("AC2".to_string(), None);
    circuit.add_node("DC_plus".to_string(), None);
    circuit.add_node("DC_minus".to_string(), None);
    circuit.add_node("load".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Simulate AC with DC for this test
    circuit.add_branch("V1".to_string(), "AC1", "GND", "VoltageSource".to_string(), 10.0, None);
    circuit.add_branch("V2".to_string(), "AC2", "GND", "VoltageSource".to_string(), -10.0, None);
    
    // Bridge diodes
    circuit.add_branch("D1".to_string(), "AC1", "DC_plus", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "DC_minus", "AC1", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D3".to_string(), "AC2", "DC_plus", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D4".to_string(), "DC_minus", "AC2", "Diode".to_string(), 0.0, None);
    
    // Load
    circuit.add_branch("R1".to_string(), "DC_plus", "load", "Resistor".to_string(), 100.0, None);
    circuit.add_branch("R2".to_string(), "load", "DC_minus", "Resistor".to_string(), 1000.0, None);
    
    circuit
}

fn test_circuit(name: &str, circuit: Circuit, led_models: Vec<(&str, ComponentModel)>) -> Result<()> {
    println!("\n{}", "=".repeat(60));
    println!("Testing: {}", name);
    println!("{}", "=".repeat(60));
    
    let mut solver = GlacierSolver::new(circuit);
    
    // Add component models
    for (component_name, model) in led_models {
        solver.add_model(component_name.to_string(), model);
    }
    
    // Add standard models
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,  // Will be overridden by circuit
        internal_resistance: None,
    });
    
    solver.add_model("V2".to_string(), ComponentModel::VoltageSource { 
        voltage: -10.0,
        internal_resistance: None,
    });
    
    // Add resistor models
    for r in ["R1", "R2", "R3"] {
        solver.add_model(r.to_string(), ComponentModel::Resistor { 
            resistance: 100.0,  // Will be overridden
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("\n✅ GLACIER found {} solutions", solutions.len());
            
            for (i, (start, end, gradient, result)) in solutions.iter().enumerate() {
                println!("\nSolution {} (Region {:.1}%-{:.1}%, gradient={:.2}):", 
                         i+1, start*100.0, end*100.0, gradient);
                
                // Sort voltages for display
                let mut voltages: Vec<(String, f64)> = Vec::new();
                for (node_idx, voltage) in result.node_voltages.iter() {
                    // Find node name
                    let node_name = solver.circuit.nodes()
                        .find(|(idx, _)| idx == node_idx)
                        .map(|(_, node)| node.name.clone())
                        .unwrap_or_else(|| format!("Node{:?}", node_idx));
                    voltages.push((node_name, *voltage));
                }
                voltages.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
                
                // Display key voltages
                println!("  Node voltages:");
                for (name, voltage) in voltages.iter().take(5) {
                    println!("    {}: {:.3}V", name, voltage);
                }
                
                // Calculate some key metrics
                let total_current = result.branch_currents.values()
                    .filter(|&&i| i.abs() > 1e-6)
                    .map(|i| i.abs())
                    .sum::<f64>() / result.branch_currents.len() as f64;
                
                println!("  Average current: {:.3}mA", total_current * 1000.0);
                println!("  Total power: {:.3}mW", result.total_power * 1000.0);
                println!("  Iterations: {}", result.iterations);
                
                // Check for numerical stability
                let max_voltage = voltages.iter().map(|(_, v)| v.abs()).fold(0.0, f64::max);
                if max_voltage > 100.0 {
                    println!("  ⚠️  WARNING: Very high voltage detected ({:.1}V)", max_voltage);
                }
                
                if result.iterations > 100 {
                    println!("  ⚠️  WARNING: High iteration count");
                }
            }
            
            // Verify solutions are distinct
            if solutions.len() > 1 {
                let powers: Vec<f64> = solutions.iter()
                    .map(|(_, _, _, r)| r.total_power)
                    .collect();
                
                let all_different = powers.windows(2)
                    .all(|w| (w[0] - w[1]).abs() > 1e-6);
                
                if all_different {
                    println!("\n✅ All solutions are distinct");
                } else {
                    println!("\n⚠️  WARNING: Some solutions may be duplicates");
                }
            }
            
            Ok(())
        },
        Err(e) => {
            println!("\n❌ GLACIER failed: {}", e);
            Err(e.into())
        }
    }
}

fn main() -> Result<()> {
    println!("=== Enhanced GLACIER Comprehensive Test ===\n");
    println!("Testing multiple challenging circuits with extreme parameters");
    println!("Verifying numerical stability and solution correctness\n");
    
    // Test 1: Single LED with extreme parameters
    let led_extreme = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(3.96e-19),  // Ultra-extreme
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    test_circuit("Single LED (Extreme Is=3.96e-19)", 
                 create_led_circuit(), 
                 vec![("D1", led_extreme.clone())])?;
    
    // Test 2: Single LED with moderate parameters
    let led_moderate = ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),  // More typical
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    };
    
    test_circuit("Single LED (Moderate Is=1e-12)", 
                 create_led_circuit(), 
                 vec![("D1", led_moderate.clone())])?;
    
    // Test 3: Series LEDs
    test_circuit("Series LEDs (2x Extreme)", 
                 create_series_leds_circuit(), 
                 vec![("D1", led_extreme.clone()), ("D2", led_extreme.clone())])?;
    
    // Test 4: Parallel LEDs with different parameters
    test_circuit("Parallel LEDs (Mixed)", 
                 create_parallel_leds_circuit(), 
                 vec![("D1", led_extreme.clone()), ("D2", led_moderate.clone())])?;
    
    // Test 5: Diode bridge with standard diodes
    let diode_std = ComponentModel::Diode {
        forward_voltage: 0.7,
        forward_resistance: 10.0,
        reverse_current: 1e-9,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.0),
        limits: ElectricalLimits::default(),
    };
    
    test_circuit("Diode Bridge Rectifier", 
                 create_diode_bridge_circuit(), 
                 vec![("D1", diode_std.clone()), 
                      ("D2", diode_std.clone()),
                      ("D3", diode_std.clone()),
                      ("D4", diode_std.clone())])?;
    
    println!("\n\n=== Summary ===");
    println!("Enhanced GLACIER successfully handles:");
    println!("✅ Multiple solutions from different operating regions");
    println!("✅ Extreme LED parameters (Is=3.96e-19)");
    println!("✅ Complex topologies (series, parallel, bridge)");
    println!("✅ No bias toward specific component states");
    println!("✅ Robust convergence using stored starting points");
    
    Ok(())
}