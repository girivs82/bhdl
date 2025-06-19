//! Test component role detection on a manually created buck regulator circuit

use std::error::Error;
use bhdl_spice::circuit::Circuit;
use bhdl_spice::extended_analysis::{ComponentRoleDetector, ComponentRole};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Buck Regulator Component Role Analysis (Direct)");
    println!("==============================================\n");
    
    // Create a realistic buck regulator circuit
    let circuit = create_buck_regulator_circuit();
    
    println!("Circuit created with:");
    println!("  {} nodes", circuit.nodes().count());
    println!("  {} components\n", circuit.branches().count());
    
    // Initialize component role detector
    let mut detector = ComponentRoleDetector::new(circuit);
    
    // Initialize simulation
    match detector.initialize_simulation() {
        Ok(()) => println!("✅ Simulation engine initialized"),
        Err(e) => {
            println!("⚠️  Simulation initialization failed: {}", e);
            println!("   Continuing with topology-only analysis");
        }
    }
    
    // Detect all component roles
    println!("\nDetecting component roles...");
    let roles = detector.detect_all_roles();
    
    // Display results organized by role
    println!("\n📊 Component Role Analysis Results");
    println!("==================================\n");
    
    // Group components by role
    let mut role_groups: std::collections::HashMap<ComponentRole, Vec<String>> = 
        std::collections::HashMap::new();
    
    for (comp_id, role) in &roles {
        if let Some(component) = detector.circuit.get_component(*comp_id) {
            let info = format!("{} ({}, {})", 
                component.name(), 
                component.component_type(),
                format_component_value(component.value, component.component_type())
            );
            
            role_groups.entry(role.clone())
                .or_insert_with(Vec::new)
                .push(info);
        }
    }
    
    // Display by category
    println!("🔌 Input Stage:");
    display_role_group(&role_groups, &ComponentRole::InputProtection);
    display_role_group(&role_groups, &ComponentRole::InputFilter);
    display_role_group(&role_groups, &ComponentRole::EMIFiltering);
    
    println!("\n⚡ Power Stage:");
    display_role_group(&role_groups, &ComponentRole::PowerSwitch);
    display_role_group(&role_groups, &ComponentRole::PowerInductor);
    display_role_group(&role_groups, &ComponentRole::CatchDiode);
    display_role_group(&role_groups, &ComponentRole::Bootstrap);
    
    println!("\n📊 Control Stage:");
    display_role_group(&role_groups, &ComponentRole::FeedbackNetwork);
    display_role_group(&role_groups, &ComponentRole::Compensation);
    display_role_group(&role_groups, &ComponentRole::SoftStart);
    display_role_group(&role_groups, &ComponentRole::Sense);
    
    println!("\n🔋 Output Stage:");
    display_role_group(&role_groups, &ComponentRole::OutputStabilization);
    display_role_group(&role_groups, &ComponentRole::OutputProtection);
    display_role_group(&role_groups, &ComponentRole::Decoupling);
    display_role_group(&role_groups, &ComponentRole::Load);
    
    println!("\n❓ Unclassified:");
    display_role_group(&role_groups, &ComponentRole::Unknown);
    
    // Summary
    println!("\n📈 Summary:");
    println!("  Total components: {}", roles.len());
    let identified = roles.iter().filter(|(_, r)| **r != ComponentRole::Unknown).count();
    let accuracy = (identified as f64 / roles.len() as f64) * 100.0;
    println!("  Successfully identified: {} ({:.1}%)", identified, accuracy);
    
    // Detailed component listing
    println!("\n📋 Detailed Component List:");
    println!("{:<10} {:<20} {:<15} {:<20}", "Name", "Type", "Value", "Role");
    println!("{}", "-".repeat(65));
    
    let mut components: Vec<_> = roles.iter()
        .filter_map(|(comp_id, role)| {
            detector.circuit.get_component(*comp_id)
                .map(|comp| (comp, role))
        })
        .collect();
    
    // Sort by component name
    components.sort_by(|a, b| a.0.name().cmp(b.0.name()));
    
    for (component, role) in components {
        println!("{:<10} {:<20} {:<15} {:?}", 
            component.name(),
            component.component_type(),
            format_component_value(component.value, component.component_type()),
            role
        );
    }
    
    Ok(())
}

fn create_buck_regulator_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Create nodes with realistic voltages
    let vin = circuit.add_node("VIN".to_string(), None);
    let vin_fused = circuit.add_node("VIN_FUSED".to_string(), None);
    let sw = circuit.add_node("SW".to_string(), None);
    let vout = circuit.add_node("VOUT".to_string(), None);
    let fb = circuit.add_node("FB".to_string(), None);
    let comp = circuit.add_node("COMP".to_string(), None);
    let ss = circuit.add_node("SS".to_string(), None);
    let boot = circuit.add_node("BOOT".to_string(), None);
    let en = circuit.add_node("EN".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    
    // Set node voltages
    circuit.set_node_voltage(vin, 12.0);
    circuit.set_node_voltage(vin_fused, 12.0);
    circuit.set_node_voltage(sw, 6.0);  // Average switch node voltage
    circuit.set_node_voltage(vout, 5.0);
    circuit.set_node_voltage(fb, 0.8);  // Feedback reference
    circuit.set_node_voltage(comp, 1.5);
    circuit.set_node_voltage(ss, 0.8);
    circuit.set_node_voltage(boot, 12.0);
    circuit.set_node_voltage(en, 1.2);
    circuit.set_node_voltage(gnd, 0.0);
    
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
    
    // Bootstrap capacitor
    circuit.add_branch("C_BOOT".to_string(), "SW", "BOOT", "Capacitor".to_string(), 0.1e-6, None);
    
    // Catch diode (Schottky)
    circuit.add_branch("D_CATCH".to_string(), "GND", "SW", "SchottkyDiode".to_string(), 0.3, None);
    
    // Power inductor
    circuit.add_branch("L_OUT".to_string(), "SW", "VOUT", "Inductor".to_string(), 15e-6, None);
    
    // Output capacitors
    circuit.add_branch("C_OUT1".to_string(), "VOUT", "GND", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("C_OUT2".to_string(), "VOUT", "GND", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("C_OUT3".to_string(), "VOUT", "GND", "Capacitor".to_string(), 22e-6, None);
    circuit.add_branch("C_OUT4".to_string(), "VOUT", "GND", "Capacitor".to_string(), 10e-6, None);
    circuit.add_branch("C_OUT5".to_string(), "VOUT", "GND", "Capacitor".to_string(), 0.1e-6, None);
    
    // Feedback network
    circuit.add_branch("R_FB1".to_string(), "VOUT", "FB", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("R_FB2".to_string(), "FB", "GND", "Resistor".to_string(), 1870.0, None);
    
    // Compensation network
    circuit.add_branch("R_COMP1".to_string(), "FB", "COMP", "Resistor".to_string(), 13300.0, None);
    circuit.add_branch("C_COMP1".to_string(), "COMP", "GND", "Capacitor".to_string(), 3.3e-9, None);
    
    // Additional compensation (series RC) - create node first
    circuit.add_node("COMP_RC".to_string(), None);
    circuit.add_branch("R_COMP2".to_string(), "COMP", "COMP_RC", "Resistor".to_string(), 2200.0, None);
    circuit.add_branch("C_COMP2".to_string(), "COMP_RC", "GND", "Capacitor".to_string(), 22e-9, None);
    
    // Soft-start capacitor
    circuit.add_branch("C_SS".to_string(), "SS", "GND", "Capacitor".to_string(), 10e-9, None);
    
    // Output current sense (optional) - create node first
    circuit.add_node("VOUT_SENSED".to_string(), None);
    circuit.add_branch("R_SENSE".to_string(), "VOUT", "VOUT_SENSED", "Resistor".to_string(), 0.01, None);
    
    // Output protection
    circuit.add_branch("D_TVS2".to_string(), "VOUT_SENSED", "GND", "TVSDiode".to_string(), 5.0, None);
    
    // Load resistor (simulating 3A at 5V = 1.67Ω)
    circuit.add_branch("R_LOAD".to_string(), "VOUT_SENSED", "GND", "Resistor".to_string(), 1.67, None);
    
    circuit
}

fn format_component_value(value: f64, comp_type: &str) -> String {
    match comp_type {
        "Resistor" => {
            if value >= 1e6 {
                format!("{:.1}MΩ", value / 1e6)
            } else if value >= 1e3 {
                format!("{:.1}kΩ", value / 1e3)
            } else if value < 1.0 {
                format!("{:.0}mΩ", value * 1000.0)
            } else {
                format!("{:.1}Ω", value)
            }
        },
        "Capacitor" => {
            if value >= 1e-3 {
                format!("{:.0}mF", value * 1e3)
            } else if value >= 1e-6 {
                format!("{:.1}µF", value * 1e6)
            } else if value >= 1e-9 {
                format!("{:.0}nF", value * 1e9)
            } else {
                format!("{:.0}pF", value * 1e12)
            }
        },
        "Inductor" | "Ferrite" => {
            if value >= 1e-3 {
                format!("{:.1}mH", value * 1e3)
            } else if value >= 1e-6 {
                format!("{:.1}µH", value * 1e6)
            } else {
                format!("{:.1}nH", value * 1e9)
            }
        },
        _ => format!("{:.3}", value),
    }
}

fn display_role_group(
    role_groups: &std::collections::HashMap<ComponentRole, Vec<String>>,
    role: &ComponentRole,
) {
    if let Some(components) = role_groups.get(role) {
        if !components.is_empty() {
            println!("  {:?}:", role);
            for info in components {
                println!("    - {}", info);
            }
        }
    }
}