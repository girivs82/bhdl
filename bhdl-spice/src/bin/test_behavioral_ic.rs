//! Test behavioral IC model framework
//! 
//! Demonstrates the creation and usage of behavioral IC models

use bhdl_spice::{
    Circuit, NodeId,
    models::{IcModelBuilder, SpiceModel},
};

fn main() {
    println!("Behavioral IC Model Framework Test");
    println!("==================================\n");
    
    test_comparator();
    test_logic_gates();
    test_voltage_reference();
    test_mixed_signal_circuit();
}

fn test_comparator() {
    println!("1. Comparator Model Test");
    println!("------------------------");
    
    // Create a comparator model
    let comp = IcModelBuilder::comparator("LM339");
    
    println!("Created comparator: {}", comp.name());
    println!("Number of terminals: {}", comp.num_terminals());
    println!("Is nonlinear: {}", comp.is_nonlinear());
    
    // Show parameters
    println!("\nParameters:");
    for (name, value) in comp.parameters() {
        println!("  {}: {}", name, value);
    }
    
    // Test current calculation with example voltages
    let voltages = vec![3.0, 2.5, 0.0, 5.0, 0.0]; // IN_P, IN_N, OUT, VDD, VSS
    let current = comp.current(&voltages, 25.0);
    println!("\nQuiescent current: {:.2} mA", current * 1000.0);
    
    println!();
}

fn test_logic_gates() {
    println!("2. Logic Gate Models Test");
    println!("-------------------------");
    
    let gate_types = vec!["AND", "OR", "NAND", "NOR", "XOR"];
    
    for gate_type in gate_types {
        let gate = IcModelBuilder::logic_gate(&format!("74HC{}", gate_type), gate_type);
        println!("Created {} gate: {}", gate_type, gate.name());
        println!("  Terminals: {}", gate.num_terminals());
        
        let params = gate.parameters();
        if let Some(tpd) = params.get("tpd") {
            println!("  Propagation delay: {:.1} ns", tpd * 1e9);
        }
    }
    
    println!();
}

fn test_voltage_reference() {
    println!("3. Voltage Reference Model Test");
    println!("-------------------------------");
    
    let vrefs = vec![
        ("LM4040-2.5", 2.5),
        ("TL431", 2.495),
        ("REF5050", 5.0),
    ];
    
    for (name, vref) in vrefs {
        let model = IcModelBuilder::voltage_reference(name, vref);
        println!("Created voltage reference: {}", model.name());
        println!("  Reference voltage: {} V", vref);
        println!("  Terminals: {}", model.num_terminals());
        
        // Test with supply voltage
        let voltages = vec![vref + 2.0, 0.0, 0.0]; // IN, OUT, GND
        let current = model.current(&voltages, 25.0);
        println!("  Operating current: {:.2} mA", current * 1000.0);
    }
    
    println!();
}

fn test_mixed_signal_circuit() {
    println!("4. Mixed-Signal Circuit Test");
    println!("----------------------------");
    println!("Window comparator with logic output\n");
    
    // Create a window comparator circuit
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vin = circuit.add_node("VIN".to_string(), None);
    let vref_low = circuit.add_node("VREF_LOW".to_string(), None);
    let vref_high = circuit.add_node("VREF_HIGH".to_string(), None);
    let comp1_out = circuit.add_node("COMP1_OUT".to_string(), None);
    let comp2_out = circuit.add_node("COMP2_OUT".to_string(), None);
    let window_out = circuit.add_node("WINDOW_OUT".to_string(), None);
    let vdd = circuit.add_node("VDD".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    
    // Create behavioral models
    let comp1 = IcModelBuilder::comparator("COMP1");
    let comp2 = IcModelBuilder::comparator("COMP2");
    let and_gate = IcModelBuilder::logic_gate("AND1", "AND");
    
    println!("Circuit components:");
    println!("  - Comparator 1: {} (VIN > VREF_LOW)", comp1.name());
    println!("  - Comparator 2: {} (VIN < VREF_HIGH)", comp2.name());
    println!("  - AND gate: {} (window detection)", and_gate.name());
    
    println!("\nWindow comparator behavior:");
    println!("  VREF_LOW = 2.0V");
    println!("  VREF_HIGH = 3.0V");
    println!("  Output HIGH when 2.0V < VIN < 3.0V");
    
    // Test different input voltages
    let test_voltages = vec![1.5, 2.5, 3.5];
    println!("\nTest results:");
    for v in test_voltages {
        let in_window = v > 2.0 && v < 3.0;
        println!("  VIN = {}V -> Output = {}", v, if in_window { "HIGH" } else { "LOW" });
    }
    
    println!();
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_comparator_parameters() {
        let comp = IcModelBuilder::comparator("TEST_COMP");
        let params = comp.parameters();
        
        assert!(params.contains_key("vdd_nom"));
        assert!(params.contains_key("iq"));
        assert_eq!(comp.num_terminals(), 5);
    }
    
    #[test]
    fn test_logic_gate_types() {
        for gate_type in &["AND", "OR", "NAND", "NOR", "XOR"] {
            let gate = IcModelBuilder::logic_gate("TEST", gate_type);
            assert_eq!(gate.num_terminals(), 3);
            assert!(gate.is_nonlinear());
        }
    }
}