//! Test 7805 voltage regulator circuit with safety analysis

use std::error::Error;
use bhdl_spice::circuit::Circuit;
use bhdl_spice::safety::rules::check_voltage_regulator_safety;
use bhdl_spice::safety::Severity;

fn main() -> Result<(), Box<dyn Error>> {
    println!("7805 Voltage Regulator Safety Analysis\n");
    println!("======================================\n");
    
    // Test different scenarios
    test_normal_operation()?;
    test_low_input_voltage()?;
    test_high_input_voltage()?;
    test_overload_condition()?;
    test_high_power_dissipation()?;
    test_missing_capacitor()?;
    
    Ok(())
}

fn create_7805_circuit(
    vin: f64,
    load_resistance: f64,
    has_output_cap: bool,
) -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes
    let gnd = circuit.add_node("GND".to_string(), None);
    let vin_node = circuit.add_node("VIN".to_string(), None);
    let vout_node = circuit.add_node("VOUT".to_string(), None);
    
    // Set node voltages for analysis
    circuit.set_node_voltage(gnd, 0.0);
    circuit.set_node_voltage(vin_node, vin);
    circuit.set_node_voltage(vout_node, 5.0); // Assume regulated
    
    // Add voltage source
    circuit.add_branch(
        "V1".to_string(),
        "VIN",
        "GND",
        "VoltageSource".to_string(),
        vin,
        None,
    );
    
    // Add 7805 voltage regulator
    // Note: The Branch's get_parameter method is hardcoded to return 7805 values
    let vreg_id = circuit.add_branch(
        "U1".to_string(),
        "VIN",
        "VOUT",
        "VoltageRegulator".to_string(),
        5.0, // Output voltage
        None,
    );
    
    // Set the current through the regulator
    circuit.set_branch_current(vreg_id, 5.0 / load_resistance);
    
    // Note: In a real implementation, we'd need to handle 3-terminal devices properly
    // For now, we'll add a dummy connection for the ground pin
    
    // Add load resistor
    let rload_id = circuit.add_branch(
        "R1".to_string(),
        "VOUT",
        "GND",
        "Resistor".to_string(),
        load_resistance,
        None,
    );
    circuit.set_branch_current(rload_id, 5.0 / load_resistance);
    
    // Add output capacitor if specified
    if has_output_cap {
        circuit.add_branch(
            "C1".to_string(),
            "VOUT",
            "GND",
            "Capacitor".to_string(),
            10e-6, // 10µF
            None,
        );
    }
    
    circuit
}

fn test_normal_operation() -> Result<(), Box<dyn Error>> {
    println!("Test 1: Normal Operation (12V input, 100mA load)");
    println!("{}", "-".repeat(50));
    
    let circuit = create_7805_circuit(12.0, 50.0, true); // 50Ω = 100mA at 5V
    
    // Find the voltage regulator
    let vreg_id = circuit.branches()
        .find(|(_, b)| b.component_type() == "VoltageRegulator")
        .map(|(id, _)| id)
        .expect("Voltage regulator not found");
    
    let violations = check_voltage_regulator_safety(&circuit, vreg_id);
    
    if violations.is_empty() {
        println!("✅ No safety violations detected!");
        println!("   Input: 12V, Output: 5V, Load: 100mA");
        println!("   Power dissipation: {:.1}W", (12.0 - 5.0) * 0.1);
        println!("   Junction temp: ~{:.0}°C", 25.0 + (12.0 - 5.0) * 0.1 * 65.0);
    } else {
        println!("✅ Safety violations detected:");
        for v in &violations {
            let icon = match v.severity {
                Severity::Warning => "⚠️",
                Severity::Error => "❌",
                Severity::Critical => "🔥",
                Severity::Info => "ℹ️",
            };
            println!("{} {}: {}", icon, v.rule_name, v.message);
            println!("   Details: {}", v.technical_details);
        }
    }
    println!();
    
    Ok(())
}

fn test_low_input_voltage() -> Result<(), Box<dyn Error>> {
    println!("Test 2: Low Input Voltage (6V input)");
    println!("{}", "-".repeat(50));
    
    let circuit = create_7805_circuit(6.0, 50.0, true);
    
    let vreg_id = circuit.branches()
        .find(|(_, b)| b.component_type() == "VoltageRegulator")
        .map(|(id, _)| id)
        .expect("Voltage regulator not found");
    
    let violations = check_voltage_regulator_safety(&circuit, vreg_id);
    
    println!("Expected: WARNING - Input voltage too low for regulation");
    for v in &violations {
        let icon = match v.severity {
            Severity::Warning => "⚠️",
            Severity::Error => "❌",
            Severity::Critical => "🔥",
            Severity::Info => "ℹ️",
        };
        println!("{} {}: {}", icon, v.rule_name, v.message);
        println!("   Details: {}", v.technical_details);
        println!("   Impact: {}", v.user_impact);
    }
    println!();
    
    Ok(())
}

fn test_high_input_voltage() -> Result<(), Box<dyn Error>> {
    println!("Test 3: High Input Voltage (40V input)");
    println!("{}", "-".repeat(50));
    
    let circuit = create_7805_circuit(40.0, 50.0, true);
    
    let vreg_id = circuit.branches()
        .find(|(_, b)| b.component_type() == "VoltageRegulator")
        .map(|(id, _)| id)
        .expect("Voltage regulator not found");
    
    let violations = check_voltage_regulator_safety(&circuit, vreg_id);
    
    println!("Expected: ERROR - Input voltage exceeds maximum");
    for v in &violations {
        let icon = match v.severity {
            Severity::Warning => "⚠️",
            Severity::Error => "❌",
            Severity::Critical => "🔥",
            Severity::Info => "ℹ️",
        };
        println!("{} {}: {}", icon, v.rule_name, v.message);
        println!("   Details: {}", v.technical_details);
        println!("   Impact: {}", v.user_impact);
    }
    println!();
    
    Ok(())
}

fn test_overload_condition() -> Result<(), Box<dyn Error>> {
    println!("Test 4: Overload Condition (1.5A load)");
    println!("{}", "-".repeat(50));
    
    let circuit = create_7805_circuit(12.0, 3.33, true); // 3.33Ω = 1.5A at 5V
    
    let vreg_id = circuit.branches()
        .find(|(_, b)| b.component_type() == "VoltageRegulator")
        .map(|(id, _)| id)
        .expect("Voltage regulator not found");
    
    let violations = check_voltage_regulator_safety(&circuit, vreg_id);
    
    println!("Expected: ERROR - Output current exceeds rating");
    for v in &violations {
        let icon = match v.severity {
            Severity::Warning => "⚠️",
            Severity::Error => "❌",
            Severity::Critical => "🔥",
            Severity::Info => "ℹ️",
        };
        println!("{} {}: {}", icon, v.rule_name, v.message);
        println!("   Details: {}", v.technical_details);
        println!("   Impact: {}", v.user_impact);
    }
    println!();
    
    Ok(())
}

fn test_high_power_dissipation() -> Result<(), Box<dyn Error>> {
    println!("Test 5: High Power Dissipation (24V input, 800mA load)");
    println!("{}", "-".repeat(50));
    
    let circuit = create_7805_circuit(24.0, 6.25, true); // 6.25Ω = 800mA at 5V
    
    let vreg_id = circuit.branches()
        .find(|(_, b)| b.component_type() == "VoltageRegulator")
        .map(|(id, _)| id)
        .expect("Voltage regulator not found");
    
    let violations = check_voltage_regulator_safety(&circuit, vreg_id);
    
    println!("Power dissipation: {:.1}W", (24.0 - 5.0) * 0.8);
    println!("Junction temp estimate: {:.0}°C", 25.0 + (24.0 - 5.0) * 0.8 * 65.0);
    println!("\nExpected: ERROR - Junction temperature exceeded");
    
    for v in &violations {
        let icon = match v.severity {
            Severity::Warning => "⚠️",
            Severity::Error => "❌",
            Severity::Critical => "🔥",
            Severity::Info => "ℹ️",
        };
        println!("{} {}: {}", icon, v.rule_name, v.message);
        println!("   Details: {}", v.technical_details);
        println!("   Impact: {}", v.user_impact);
    }
    println!();
    
    Ok(())
}

fn test_missing_capacitor() -> Result<(), Box<dyn Error>> {
    println!("Test 6: Missing Output Capacitor");
    println!("{}", "-".repeat(50));
    
    let circuit = create_7805_circuit(12.0, 50.0, false); // No output cap
    
    let vreg_id = circuit.branches()
        .find(|(_, b)| b.component_type() == "VoltageRegulator")
        .map(|(id, _)| id)
        .expect("Voltage regulator not found");
    
    let violations = check_voltage_regulator_safety(&circuit, vreg_id);
    
    println!("Expected: WARNING - Missing output capacitor");
    for v in &violations {
        let icon = match v.severity {
            Severity::Warning => "⚠️",
            Severity::Error => "❌",
            Severity::Critical => "🔥",
            Severity::Info => "ℹ️",
        };
        println!("{} {}: {}", icon, v.rule_name, v.message);
        println!("   Details: {}", v.technical_details);
        println!("   Impact: {}", v.user_impact);
    }
    
    println!("\n{}", "=".repeat(50));
    println!("Safety Analysis Summary:");
    println!("- Input voltage range: 7V - 35V");
    println!("- Max output current: 1A");
    println!("- Max power without heatsink: ~2W");
    println!("- Output capacitor required for stability");
    
    Ok(())
}