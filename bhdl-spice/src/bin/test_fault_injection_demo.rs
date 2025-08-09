//! Demonstration of fault injection with R1 short circuit and overcurrent detection

use anyhow::Result;
use bhdl_spice::{
    Circuit, ComponentModel, ElectricalLimits, 
    GlacierSolver, FaultInjector, FaultSpec, FaultType,
    detect_overcurrent
};

fn main() -> Result<()> {
    println!("=== Fault Injection Demo: R1 Short Circuit ===\n");
    
    // Create a simple LED circuit: 5V -> R1(470Ω) -> LED -> GND
    let mut circuit = Circuit::new();
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("led_anode".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VCC", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VCC", "led_anode", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "led_anode", "GND", "LED".to_string(), 0.0, None);
    
    // Set up component models with current limits
    let mut models = std::collections::HashMap::new();
    
    models.insert("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: Some(0.1), // Small internal resistance
    });
    
    models.insert("R1".to_string(), ComponentModel::Resistor { 
        resistance: 470.0,
        tolerance: 5.0,
        limits: ElectricalLimits {
            max_voltage: Some(50.0),
            max_current: Some(0.1), // 100mA max for 1/4W resistor
            max_power: Some(0.25),  // 1/4 watt
            ..Default::default()
        },
    });
    
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3, // 20mA nominal
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits {
            max_current: Some(30e-3), // 30mA absolute max
            max_power: Some(0.1),     // 100mW max
            ..Default::default()
        },
    });
    
    // First, analyze normal circuit
    println!("1. Normal Circuit Analysis:");
    println!("   --------------------------");
    let mut solver = GlacierSolver::new(circuit.clone());
    for (name, model) in &models {
        solver.add_model(name.clone(), model.clone());
    }
    
    match solver.analyze() {
        Ok(solutions) => {
            if let Some((_, _, _, result)) = solutions.first() {
                // Extract currents and voltages
                let v_vcc = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_led = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                let i_circuit = (v_vcc - v_led) / 470.0;
                
                println!("   V_VCC = {:.2}V", v_vcc);
                println!("   V_LED_anode = {:.2}V", v_led);
                println!("   LED voltage drop = {:.2}V", v_led);
                println!("   Circuit current = {:.2}mA", i_circuit * 1000.0);
                println!("   R1 power = {:.3}W", i_circuit * i_circuit * 470.0);
                
                // Update circuit with calculated currents for overcurrent detection
                circuit.set_branch_current("R1", i_circuit);
                circuit.set_branch_current("D1", i_circuit);
            }
        }
        Err(e) => println!("   Analysis failed: {}", e),
    }
    
    // Check for overcurrents in normal operation
    let overcurrents = detect_overcurrent(&circuit, &models, 1.0);
    if overcurrents.is_empty() {
        println!("   ✓ No overcurrent conditions detected");
    }
    
    // Now inject R1 short circuit fault
    println!("\n2. Injecting R1 Short Circuit Fault:");
    println!("   ----------------------------------");
    
    let mut fault_injector = FaultInjector::new();
    fault_injector.add_fault(FaultSpec {
        component_name: "R1".to_string(),
        fault_type: FaultType::ShortCircuit { 
            resistance: 0.01 // 10 milliohm short
        },
        description: Some("R1 develops internal short circuit".to_string()),
    });
    
    // Apply faults to circuit and models
    let mut fault_circuit = circuit.clone();
    let mut fault_models = models.clone();
    fault_injector.apply_faults(&mut fault_circuit, &mut fault_models)?;
    
    // Analyze faulted circuit
    println!("\n3. Fault Circuit Analysis:");
    println!("   ------------------------");
    let mut fault_solver = GlacierSolver::new(fault_circuit.clone());
    for (name, model) in &fault_models {
        fault_solver.add_model(name.clone(), model.clone());
    }
    
    match fault_solver.analyze() {
        Ok(solutions) => {
            if let Some((_, _, _, result)) = solutions.first() {
                // Extract currents and voltages
                let v_vcc = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 0)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                let v_led = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                
                // With R1 shorted (0.01Ω), current is limited mainly by LED and source resistance
                let i_fault = (v_vcc - v_led) / 0.01;
                
                println!("   V_VCC = {:.2}V", v_vcc);
                println!("   V_LED_anode = {:.2}V", v_led);
                println!("   LED voltage drop = {:.2}V", v_led);
                println!("   FAULT CURRENT = {:.2}mA (was ~10mA normal)", i_fault * 1000.0);
                println!("   R1 power = {:.3}W", i_fault * i_fault * 0.01);
                
                // Update circuit with fault currents
                fault_circuit.set_branch_current("R1", i_fault);
                fault_circuit.set_branch_current("D1", i_fault);
                
                // Detect overcurrent conditions
                println!("\n4. Overcurrent Detection:");
                println!("   -----------------------");
                let overcurrents = detect_overcurrent(&fault_circuit, &fault_models, 1.0);
                
                if overcurrents.is_empty() {
                    println!("   No overcurrent detected (components may be within limits)");
                } else {
                    println!("   ⚠️  OVERCURRENT CONDITIONS DETECTED:");
                    for (component, current, limit) in &overcurrents {
                        println!("      {} : {:.1}mA exceeds limit of {:.1}mA ({:.0}%)", 
                                 component, 
                                 current * 1000.0, 
                                 limit * 1000.0,
                                 (current / limit) * 100.0);
                    }
                }
                
                // Additional safety checks
                println!("\n5. Safety Analysis:");
                println!("   ----------------");
                
                // Check if LED is in danger
                if i_fault > 0.030 {
                    println!("   🔥 WARNING: LED current ({:.1}mA) exceeds absolute maximum (30mA)!", 
                             i_fault * 1000.0);
                    println!("      LED will likely be damaged or destroyed!");
                }
                
                // Check power dissipation
                let led_power = i_fault * v_led;
                if led_power > 0.1 {
                    println!("   🔥 WARNING: LED power ({:.1}mW) exceeds maximum (100mW)!", 
                             led_power * 1000.0);
                }
                
                // Suggest protection
                println!("\n6. Recommended Protection:");
                println!("   ------------------------");
                println!("   • Add a current limiting resistor in series (minimum 100Ω)");
                println!("   • Use a fuse rated at 50mA to protect the LED");
                println!("   • Consider a PTC thermistor for self-resetting protection");
                println!("   • Implement active current limiting with a transistor circuit");
            }
        }
        Err(e) => {
            println!("   Fault analysis failed: {}", e);
            println!("   This might indicate the circuit cannot reach a stable operating point");
            println!("   with the fault present (e.g., voltage source overloaded).");
        }
    }
    
    // Demonstrate restoration
    println!("\n7. Restoring Original Circuit:");
    println!("   ----------------------------");
    fault_injector.restore(&mut fault_circuit, &mut fault_models)?;
    println!("   ✓ Circuit restored to original state");
    
    Ok(())
}

// Helper extension trait for Circuit
trait CircuitExt {
    fn set_branch_current(&mut self, name: &str, current: f64);
}

impl CircuitExt for Circuit {
    fn set_branch_current(&mut self, name: &str, current: f64) {
        self.set_branch_current_by_name(name, current);
    }
}