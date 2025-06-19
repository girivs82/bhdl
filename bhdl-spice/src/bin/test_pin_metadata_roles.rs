//! Test pin metadata improvements on component role detection

use std::error::Error;
use bhdl_spice::circuit::Circuit;
use bhdl_spice::extended_analysis::{ComponentRoleDetector, ComponentRole};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Pin Metadata Component Role Detection Test");
    println!("==========================================\n");
    
    // Create two identical circuits to compare detection with and without pin metadata
    let circuit1 = create_buck_regulator_circuit();
    let circuit2 = create_buck_regulator_circuit();
    
    // Test 1: Original detection (without pin metadata awareness)
    println!("Test 1: Original Role Detection (topology-based only)");
    println!("----------------------------------------------------");
    let mut detector1 = ComponentRoleDetector::new(circuit1);
    // Don't initialize simulation to see pure topology detection
    let roles1 = detector1.detect_all_roles();
    
    // Test 2: Enhanced detection (with pin metadata)
    println!("\nTest 2: Enhanced Role Detection (pin metadata + topology)");
    println!("--------------------------------------------------------");
    let mut detector2 = ComponentRoleDetector::new(circuit2);
    detector2.initialize_simulation().ok(); // Initialize to enable all features
    let roles2 = detector2.detect_all_roles();
    
    // Compare results
    println!("\n📊 Comparison of Detection Methods");
    println!("==================================\n");
    
    // Key components that should benefit from pin metadata
    let key_components = vec![
        ("C_BOOT", "Bootstrap capacitor connected to SW and BOOT pins"),
        ("C_SS", "Soft-start capacitor connected to SS pin"),
        ("C_COMP1", "Compensation capacitor connected to COMP pin"),
        ("R_FB1", "Feedback resistor connected to FB pin"),
        ("R_FB2", "Feedback resistor connected to FB pin"),
        ("C_IN1", "Input filter capacitor"),
        ("C_IN2", "Input filter capacitor"),
        ("C_IN3", "Input filter capacitor"),
    ];
    
    println!("{:<12} {:<25} {:<25} {:<10}", "Component", "Topology-Only", "With Pin Metadata", "Improved?");
    println!("{}", "-".repeat(75));
    
    for (comp_name, description) in &key_components {
        let role1 = find_component_role(&detector1, &roles1, comp_name);
        let role2 = find_component_role(&detector2, &roles2, comp_name);
        let improved = role1 != role2 && is_role_more_accurate(&role2, comp_name);
        
        println!("{:<12} {:<25} {:<25} {:<10}", 
            comp_name,
            format!("{:?}", role1),
            format!("{:?}", role2),
            if improved { "✓" } else { "" }
        );
    }
    
    // Summary statistics
    println!("\n📈 Detection Accuracy Summary");
    println!("=============================");
    
    let total1 = roles1.len();
    let identified1 = roles1.values().filter(|r| **r != ComponentRole::Unknown).count();
    let accuracy1 = (identified1 as f64 / total1 as f64) * 100.0;
    
    let total2 = roles2.len();
    let identified2 = roles2.values().filter(|r| **r != ComponentRole::Unknown).count();
    let accuracy2 = (identified2 as f64 / total2 as f64) * 100.0;
    
    println!("Topology-only detection: {}/{} ({:.1}%)", identified1, total1, accuracy1);
    println!("With pin metadata:       {}/{} ({:.1}%)", identified2, total2, accuracy2);
    
    // Count improvements
    let mut improvements = 0;
    let mut correct_classifications = 0;
    
    for (comp_name, _) in &key_components {
        let role1 = find_component_role(&detector1, &roles1, comp_name);
        let role2 = find_component_role(&detector2, &roles2, comp_name);
        
        if role1 != role2 {
            improvements += 1;
            if is_role_more_accurate(&role2, comp_name) {
                correct_classifications += 1;
            }
        }
    }
    
    println!("\nKey component improvements: {}/{}", correct_classifications, key_components.len());
    
    // Detailed analysis of pin metadata impact
    println!("\n🔍 Pin Metadata Impact Analysis");
    println!("================================");
    
    println!("\n1. Bootstrap Capacitor (C_BOOT):");
    println!("   - Connected between SW (switch node) and BOOT pins");
    println!("   - Pin metadata identifies BOOT as Bootstrap function");
    println!("   - Should be classified as Bootstrap, not generic capacitor");
    
    println!("\n2. Soft-Start Capacitor (C_SS):");
    println!("   - Connected to SS pin and ground");
    println!("   - Pin metadata identifies SS as SoftStart function");
    println!("   - Should be classified as SoftStart, not Decoupling");
    
    println!("\n3. Compensation Network (C_COMP1, R_COMP1):");
    println!("   - Connected to COMP pin");
    println!("   - Pin metadata identifies COMP as Compensation function");
    println!("   - Should be classified as Compensation");
    
    println!("\n4. Feedback Network (R_FB1, R_FB2):");
    println!("   - Connected to FB pin");
    println!("   - Pin metadata identifies FB as Feedback function");
    println!("   - Should be classified as FeedbackNetwork");
    
    println!("\n5. Input Capacitors (C_IN1, C_IN2, C_IN3):");
    println!("   - Connected to VIN power input");
    println!("   - Should be classified as InputFilter, not Bootstrap");
    
    Ok(())
}

fn create_buck_regulator_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Create nodes with realistic voltages
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("VIN_FUSED".to_string(), None);
    circuit.add_node("SW".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("FB".to_string(), None);
    circuit.add_node("COMP".to_string(), None);
    circuit.add_node("SS".to_string(), None);
    circuit.add_node("BOOT".to_string(), None);
    circuit.add_node("EN".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    circuit.add_node("COMP_RC".to_string(), None);
    circuit.add_node("VOUT_SENSED".to_string(), None);
    
    // Set node voltages
    circuit.set_node_voltage(circuit.get_node("VIN").unwrap().0, 12.0);
    circuit.set_node_voltage(circuit.get_node("VIN_FUSED").unwrap().0, 12.0);
    circuit.set_node_voltage(circuit.get_node("SW").unwrap().0, 6.0);
    circuit.set_node_voltage(circuit.get_node("VOUT").unwrap().0, 5.0);
    circuit.set_node_voltage(circuit.get_node("FB").unwrap().0, 0.8);
    circuit.set_node_voltage(circuit.get_node("COMP").unwrap().0, 1.5);
    circuit.set_node_voltage(circuit.get_node("SS").unwrap().0, 0.8);
    circuit.set_node_voltage(circuit.get_node("BOOT").unwrap().0, 12.0);
    circuit.set_node_voltage(circuit.get_node("EN").unwrap().0, 1.2);
    circuit.set_node_voltage(circuit.get_node("GND").unwrap().0, 0.0);
    
    // Input EMI filter
    circuit.add_branch("L_EMI".to_string(), "VIN", "VIN_FUSED", "Ferrite".to_string(), 600e-6, None);
    
    // Input protection
    circuit.add_branch("F1".to_string(), "VIN_FUSED", "VIN_FUSED", "Fuse".to_string(), 5.0, None);
    circuit.add_branch("D_TVS1".to_string(), "VIN_FUSED", "GND", "TVSDiode".to_string(), 15.0, None);
    
    // Input capacitors
    circuit.add_branch("C_IN1".to_string(), "VIN_FUSED", "GND", "Capacitor".to_string(), 220e-6, None);
    circuit.add_branch("C_IN2".to_string(), "VIN_FUSED", "GND", "Capacitor".to_string(), 10e-6, None);
    circuit.add_branch("C_IN3".to_string(), "VIN_FUSED", "GND", "Capacitor".to_string(), 0.1e-6, None);
    
    // Buck controller TPS54360
    circuit.add_branch("U1".to_string(), "VIN_FUSED", "SW", "BuckController".to_string(), 5.0, None);
    
    // Enable divider
    circuit.add_branch("R_EN1".to_string(), "VIN_FUSED", "EN", "Resistor".to_string(), 100000.0, None);
    circuit.add_branch("R_EN2".to_string(), "EN", "GND", "Resistor".to_string(), 10000.0, None);
    
    // Bootstrap capacitor - KEY COMPONENT
    circuit.add_branch("C_BOOT".to_string(), "SW", "BOOT", "Capacitor".to_string(), 0.1e-6, None);
    
    // Catch diode
    circuit.add_branch("D_CATCH".to_string(), "GND", "SW", "SchottkyDiode".to_string(), 0.3, None);
    
    // Power inductor
    circuit.add_branch("L_OUT".to_string(), "SW", "VOUT", "Inductor".to_string(), 15e-6, None);
    
    // Output capacitors
    circuit.add_branch("C_OUT1".to_string(), "VOUT", "GND", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("C_OUT2".to_string(), "VOUT", "GND", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("C_OUT3".to_string(), "VOUT", "GND", "Capacitor".to_string(), 22e-6, None);
    circuit.add_branch("C_OUT4".to_string(), "VOUT", "GND", "Capacitor".to_string(), 10e-6, None);
    circuit.add_branch("C_OUT5".to_string(), "VOUT", "GND", "Capacitor".to_string(), 0.1e-6, None);
    
    // Feedback network - KEY COMPONENTS
    circuit.add_branch("R_FB1".to_string(), "VOUT", "FB", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("R_FB2".to_string(), "FB", "GND", "Resistor".to_string(), 1870.0, None);
    
    // Compensation network - KEY COMPONENTS
    circuit.add_branch("R_COMP1".to_string(), "FB", "COMP", "Resistor".to_string(), 13300.0, None);
    circuit.add_branch("C_COMP1".to_string(), "COMP", "GND", "Capacitor".to_string(), 3.3e-9, None);
    circuit.add_branch("R_COMP2".to_string(), "COMP", "COMP_RC", "Resistor".to_string(), 2200.0, None);
    circuit.add_branch("C_COMP2".to_string(), "COMP_RC", "GND", "Capacitor".to_string(), 22e-9, None);
    
    // Soft-start capacitor - KEY COMPONENT
    circuit.add_branch("C_SS".to_string(), "SS", "GND", "Capacitor".to_string(), 10e-9, None);
    
    // Current sense and load
    circuit.add_branch("R_SENSE".to_string(), "VOUT", "VOUT_SENSED", "Resistor".to_string(), 0.01, None);
    circuit.add_branch("D_TVS2".to_string(), "VOUT_SENSED", "GND", "TVSDiode".to_string(), 5.0, None);
    circuit.add_branch("R_LOAD".to_string(), "VOUT_SENSED", "GND", "Resistor".to_string(), 1.67, None);
    
    circuit
}

fn find_component_role(
    detector: &ComponentRoleDetector, 
    roles: &std::collections::HashMap<bhdl_spice::circuit::ComponentId, ComponentRole>,
    name: &str
) -> ComponentRole {
    for (comp_id, role) in roles {
        if let Some(component) = detector.circuit.get_component(*comp_id) {
            if component.name() == name {
                return role.clone();
            }
        }
    }
    ComponentRole::Unknown
}

fn is_role_more_accurate(role: &ComponentRole, component_name: &str) -> bool {
    match component_name {
        "C_BOOT" => matches!(role, ComponentRole::Bootstrap),
        "C_SS" => matches!(role, ComponentRole::SoftStart),
        "C_COMP1" | "C_COMP2" => matches!(role, ComponentRole::Compensation),
        "R_COMP1" | "R_COMP2" => matches!(role, ComponentRole::Compensation),
        "R_FB1" | "R_FB2" => matches!(role, ComponentRole::FeedbackNetwork),
        "C_IN1" | "C_IN2" | "C_IN3" => matches!(role, ComponentRole::InputFilter),
        _ => true,
    }
}