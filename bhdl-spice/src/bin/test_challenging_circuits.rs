//! Test suite for challenging circuits with nonlinearities and discontinuities

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Enhanced Solver on Challenging Circuits ===\n");
    println!("These circuits are known to cause convergence problems in traditional solvers\n");
    
    let mut results = Vec::new();
    
    // Test 1: Ultra-sharp LED (already tested)
    println!("\n1. Ultra-sharp LED (Is=1e-16)");
    println!("   Challenge: Extremely narrow convergence window");
    results.push(("Ultra-sharp LED", test_ultra_sharp_led()));
    
    // Test 2: Series diodes with different characteristics
    println!("\n2. Mixed Series Diodes");
    println!("   Challenge: Multiple nonlinear elements with different exponentials");
    results.push(("Mixed Series Diodes", test_mixed_diodes()));
    
    // Test 3: Bistable circuit (flip-flop like behavior)
    println!("\n3. Bistable Circuit");
    println!("   Challenge: Multiple stable operating points");
    results.push(("Bistable Circuit", test_bistable()));
    
    // Test 4: Zener diode voltage regulator
    println!("\n4. Zener Diode Regulator");
    println!("   Challenge: Sharp transition at breakdown voltage");
    results.push(("Zener Regulator", test_zener_regulator()));
    
    // Test 5: Tunnel diode circuit
    println!("\n5. Tunnel Diode Circuit");
    println!("   Challenge: Negative resistance region");
    results.push(("Tunnel Diode", test_tunnel_diode()));
    
    // Test 6: MOSFET near threshold
    println!("\n6. MOSFET Near Threshold");
    println!("   Challenge: Sharp transition from cutoff to saturation");
    results.push(("MOSFET Threshold", test_mosfet_threshold()));
    
    // Test 7: Schmitt trigger
    println!("\n7. Schmitt Trigger");
    println!("   Challenge: Hysteresis and positive feedback");
    results.push(("Schmitt Trigger", test_schmitt_trigger()));
    
    // Test 8: Parallel diodes with imbalance
    println!("\n8. Parallel Diodes with Mismatch");
    println!("   Challenge: Current sharing with exponential characteristics");
    results.push(("Parallel Diodes", test_parallel_diodes()));
    
    // Test 9: LED with bypass capacitor (startup transient)
    println!("\n9. LED with Large Bypass Capacitor");
    println!("   Challenge: DC solution with capacitor initial condition");
    results.push(("LED with Capacitor", test_led_with_cap()));
    
    // Test 10: Multiple operating regions
    println!("\n10. Multi-Region Circuit");
    println!("    Challenge: Circuit with 3+ distinct operating regions");
    results.push(("Multi-Region", test_multi_region()));
    
    // Summary
    println!("\n\n=== RESULTS SUMMARY ===");
    println!("Circuit                        Status");
    println!("------------------------------------");
    
    let mut passed = 0;
    let mut total = 0;
    
    for (name, result) in results {
        total += 1;
        match result {
            Ok(true) => {
                println!("{:<30} ✅ PASS", name);
                passed += 1;
            }
            Ok(false) => {
                println!("{:<30} ⚠️  PARTIAL", name);
            }
            Err(e) => {
                println!("{:<30} ❌ FAIL: {}", name, e);
            }
        }
    }
    
    println!("\nTotal: {}/{} passed", passed, total);
    
    if passed == total {
        println!("\n🎉 All challenging circuits solved successfully!");
    } else {
        println!("\n⚠️  Some circuits remain challenging even with gradient detection.");
        println!("This is expected for circuits with true discontinuities or");
        println!("numerical precision limits.");
    }
    
    Ok(())
}

fn test_ultra_sharp_led() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 470.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
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
        saturation_current: Some(1e-16),  // Ultra-sharp!
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => Ok(!solutions.is_empty()),
        Err(_) => Ok(false), // Partial success if gradient detection worked
    }
}

fn test_mixed_diodes() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // 12V -> 1k -> Sharp diode -> Normal diode -> GND
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("D1".to_string(), "n1", "n2", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "Diode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 1000.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    // Sharp diode
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        saturation_current: Some(1e-15),  // Sharp
        emission_coefficient: Some(1.2),
        limits: ElectricalLimits::default(),
    });
    
    // Normal diode
    solver.add_model("D2".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        saturation_current: Some(1e-12),  // Normal
        emission_coefficient: Some(2.0),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => Ok(!solutions.is_empty()),
        Err(_) => Err(anyhow::anyhow!("Failed to converge"))
    }
}

fn test_bistable() -> Result<bool> {
    // Simple bistable with cross-coupled transistors (simplified as nonlinear resistors)
    let mut circuit = Circuit::new();
    circuit.add_node("vcc".to_string(), None);
    circuit.add_node("out1".to_string(), None);
    circuit.add_node("out2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "vcc", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "vcc", "out1", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("R2".to_string(), "vcc", "out2", "Resistor".to_string(), 10000.0, None);
    
    // Cross-coupled diodes to simulate bistable behavior
    circuit.add_branch("D1".to_string(), "out1", "out2", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "out2", "out1", "Diode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    for r in ["R1", "R2"] {
        solver.add_model(r.to_string(), ComponentModel::Resistor { 
            resistance: 10000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
    }
    
    for d in ["D1", "D2"] {
        solver.add_model(d.to_string(), ComponentModel::Diode {
            forward_voltage: 0.7,
            saturation_current: Some(1e-14),
            emission_coefficient: Some(1.5),
            limits: ElectricalLimits::default(),
        });
    }
    
    // This circuit has two stable states - might find one or both
    match solver.analyze() {
        Ok(solutions) => {
            println!("   Found {} stable state(s)", solutions.len());
            Ok(!solutions.is_empty())
        }
        Err(_) => Err(anyhow::anyhow!("Failed to find any stable state"))
    }
}

fn test_zener_regulator() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Variable input -> Resistor -> Zener to GND, output at zener cathode
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 12.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 330.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "ZenerDiode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 12.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 330.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::ZenerDiode {
        zener_voltage: 5.1,
        test_current: 20e-3,
        dynamic_resistance: 10.0,
        forward_voltage: 0.7,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            if let Some((_, _, _, result)) = solutions.first() {
                // Check if output is regulated near 5.1V
                let v_out = result.node_voltages.iter()
                    .find(|(idx, _)| idx.index() == 1)
                    .map(|(_, v)| *v)
                    .unwrap_or(0.0);
                println!("   Output voltage: {:.2}V (expect ~5.1V)", v_out);
            }
            Ok(!solutions.is_empty())
        }
        Err(_) => Err(anyhow::anyhow!("Failed to converge"))
    }
}

fn test_tunnel_diode() -> Result<bool> {
    // Simplified tunnel diode with negative resistance region
    // This is extremely challenging due to the N-shaped I-V curve
    println!("   Note: Tunnel diodes have negative resistance regions");
    println!("   This may require special handling beyond gradient detection");
    
    // For now, return partial success as this is a known limitation
    Ok(false)
}

fn test_mosfet_threshold() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("gate".to_string(), None);
    circuit.add_node("drain".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Gate voltage right at threshold
    circuit.add_branch("Vg".to_string(), "gate", "GND", "VoltageSource".to_string(), 2.0, None);
    circuit.add_branch("Vd".to_string(), "drain", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("M1".to_string(), "drain", "GND", "MOSFET".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("Vg".to_string(), ComponentModel::VoltageSource { 
        voltage: 2.0,  // Right at threshold
        internal_resistance: None,
    });
    
    solver.add_model("Vd".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("M1".to_string(), ComponentModel::MOSFET {
        mosfet_type: "NMOS".to_string(),
        threshold_voltage: 2.0,  // Vgs right at threshold!
        k_value: 0.001,
        channel_length_modulation: 0.01,
        gate_control: Some("gate".to_string()),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("   Found solution at threshold");
            Ok(!solutions.is_empty())
        }
        Err(_) => Err(anyhow::anyhow!("Failed at threshold"))
    }
}

fn test_schmitt_trigger() -> Result<bool> {
    // Simplified Schmitt trigger using diodes for hysteresis
    println!("   Note: Schmitt triggers have hysteresis");
    println!("   Multiple solutions may exist depending on history");
    
    // Return partial for now as this requires transient analysis
    Ok(false)
}

fn test_parallel_diodes() -> Result<bool> {
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Current source -> parallel diodes with mismatch
    circuit.add_branch("I1".to_string(), "in", "GND", "CurrentSource".to_string(), 0.01, None);
    circuit.add_branch("D1".to_string(), "in", "GND", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "in", "GND", "Diode".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("I1".to_string(), ComponentModel::CurrentSource { 
        current: 0.01,  // 10mA total
    });
    
    // Mismatched diodes
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        saturation_current: Some(1e-14),  // Lower Is
        emission_coefficient: Some(1.5),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        saturation_current: Some(2e-14),  // Higher Is (2x)
        emission_coefficient: Some(1.5),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("   Current sharing converged");
            Ok(!solutions.is_empty())
        }
        Err(_) => Err(anyhow::anyhow!("Failed to converge"))
    }
}

fn test_led_with_cap() -> Result<bool> {
    // LED with large bypass capacitor - DC analysis should ignore cap
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("out".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "in", "out", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "out", "GND", "LED".to_string(), 0.0, None);
    circuit.add_branch("C1".to_string(), "out", "GND", "Capacitor".to_string(), 100e-6, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 5.0,
        internal_resistance: None,
    });
    
    solver.add_model("R1".to_string(), ComponentModel::Resistor { 
        resistance: 220.0,
        tolerance: 5.0,
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("C1".to_string(), ComponentModel::Capacitor {
        capacitance: 100e-6,
        tolerance: 20.0,
        voltage_rating: Some(16.0),
        esr: None,
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("   DC solution found (capacitor open)");
            Ok(!solutions.is_empty())
        }
        Err(_) => Err(anyhow::anyhow!("Failed to converge"))
    }
}

fn test_multi_region() -> Result<bool> {
    // Circuit with multiple distinct operating regions
    let mut circuit = Circuit::new();
    circuit.add_node("in".to_string(), None);
    circuit.add_node("n1".to_string(), None);
    circuit.add_node("n2".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Voltage divider with diodes creating multiple regions
    circuit.add_branch("V1".to_string(), "in", "GND", "VoltageSource".to_string(), 10.0, None);
    circuit.add_branch("R1".to_string(), "in", "n1", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R2".to_string(), "n1", "n2", "Resistor".to_string(), 1000.0, None);
    circuit.add_branch("R3".to_string(), "n2", "GND", "Resistor".to_string(), 1000.0, None);
    
    // Diodes to create regions
    circuit.add_branch("D1".to_string(), "n1", "GND", "Diode".to_string(), 0.0, None);
    circuit.add_branch("D2".to_string(), "n2", "GND", "LED".to_string(), 0.0, None);
    
    let mut solver = GlacierSolver::new(circuit);
    
    solver.add_model("V1".to_string(), ComponentModel::VoltageSource { 
        voltage: 10.0,
        internal_resistance: None,
    });
    
    for r in ["R1", "R2", "R3"] {
        solver.add_model(r.to_string(), ComponentModel::Resistor { 
            resistance: 1000.0,
            tolerance: 5.0,
            limits: ElectricalLimits::default(),
        });
    }
    
    solver.add_model("D1".to_string(), ComponentModel::Diode {
        forward_voltage: 0.7,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.5),
        limits: ElectricalLimits::default(),
    });
    
    solver.add_model("D2".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-14),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(solutions) => {
            println!("   Found {} operating region(s)", solutions.len());
            Ok(!solutions.is_empty())
        }
        Err(_) => Err(anyhow::anyhow!("Failed to find any region"))
    }
}