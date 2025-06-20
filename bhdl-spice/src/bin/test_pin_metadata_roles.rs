//! Test pin metadata integration with component role detection
//! 
//! This test demonstrates how AST pin metadata improves component role inference

use bhdl_spice::{ComponentRoleDetector, ComponentRole, Circuit};
use bhdl_analyzer::AnalysisResult;
use bhdl_netlist::Netlist;
use std::collections::HashMap;

fn main() {
    println!("=== Pin Metadata Integration Test ===\n");
    
    // Create a simple buck converter circuit
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vin = circuit.add_node("VIN".to_string(), Some(12.0));
    let sw = circuit.add_node("SW".to_string(), None);
    let vout = circuit.add_node("VOUT".to_string(), Some(5.0));
    let fb = circuit.add_node("FB".to_string(), None);
    let comp_node = circuit.add_node("COMP".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), Some(0.0));
    
    // Add buck controller
    let controller = circuit.add_branch(
        "U1".to_string(),
        "VIN",
        "GND",
        "BuckController".to_string(),
        1.0,
        None
    );
    
    // Connect controller pins (simplified - in reality would use proper multi-pin model)
    
    // Add components connected to switch node
    let inductor = circuit.add_branch(
        "L1".to_string(),
        "SW",
        "VOUT",
        "Inductor".to_string(),
        10e-6, // 10µH
        None
    );
    
    let catch_diode = circuit.add_branch(
        "D1".to_string(),
        "GND",
        "SW",
        "SchottkyDiode".to_string(),
        0.3, // Forward voltage
        None
    );
    
    // Input capacitor
    let cin = circuit.add_branch(
        "C1".to_string(),
        "VIN",
        "GND",
        "Capacitor".to_string(),
        100e-6, // 100µF
        None
    );
    
    // Output capacitor
    let cout = circuit.add_branch(
        "C2".to_string(),
        "VOUT",
        "GND",
        "Capacitor".to_string(),
        220e-6, // 220µF
        None
    );
    
    // Feedback resistors
    let rfb1 = circuit.add_branch(
        "R1".to_string(),
        "VOUT",
        "FB",
        "Resistor".to_string(),
        10000.0, // 10k
        None
    );
    
    let rfb2 = circuit.add_branch(
        "R2".to_string(),
        "FB",
        "GND",
        "Resistor".to_string(),
        2200.0, // 2.2k
        None
    );
    
    // Compensation network
    let rcomp = circuit.add_branch(
        "R3".to_string(),
        "FB",
        "COMP",
        "Resistor".to_string(),
        15000.0, // 15k
        None
    );
    
    let ccomp = circuit.add_branch(
        "C3".to_string(),
        "COMP",
        "GND",
        "Capacitor".to_string(),
        10e-9, // 10nF
        None
    );
    
    // Load resistor
    let load = circuit.add_branch(
        "R4".to_string(),
        "VOUT",
        "GND",
        "Resistor".to_string(),
        10.0, // 10Ω load
        None
    );
    
    // Create mock analysis result with pin metadata
    let mut analysis_result = create_mock_analysis_result();
    
    // Create detector without metadata
    println!("1. Component role detection WITHOUT pin metadata:");
    let detector_no_metadata = ComponentRoleDetector::new(circuit.clone());
    let roles_no_metadata = detector_no_metadata.detect_all_roles();
    print_roles(&roles_no_metadata);
    
    // Create detector with metadata
    println!("\n2. Component role detection WITH pin metadata:");
    
    // Create mock netlist (simplified)
    let netlist = create_mock_netlist();
    let instance_to_component = create_instance_mapping();
    
    let detector_with_metadata = ComponentRoleDetector::with_ast_metadata(
        circuit.clone(),
        &netlist,
        instance_to_component,
        &analysis_result
    );
    
    let roles_with_metadata = detector_with_metadata.detect_all_roles();
    print_roles(&roles_with_metadata);
    
    // Compare results
    println!("\n3. Improvements from pin metadata:");
    compare_results(&roles_no_metadata, &roles_with_metadata);
}

fn create_mock_analysis_result() -> AnalysisResult {
    let mut result = AnalysisResult::default();
    
    // Add module definition for BuckController with pin metadata
    let mut module_defs = HashMap::new();
    let mut buck_module = bhdl_analyzer::ModuleDefinition::default();
    
    // Add pins with metadata
    let mut pins = HashMap::new();
    
    // SW pin with explicit SwitchNode function
    let mut sw_pin = HashMap::new();
    sw_pin.insert("type".to_string(), "power".to_string());
    sw_pin.insert("direction".to_string(), "out".to_string());
    sw_pin.insert("function".to_string(), "SwitchNode".to_string());
    sw_pin.insert("slew_rate".to_string(), "fast".to_string());
    pins.insert("SW".to_string(), sw_pin);
    
    // FB pin with Feedback function
    let mut fb_pin = HashMap::new();
    fb_pin.insert("type".to_string(), "signal".to_string());
    fb_pin.insert("direction".to_string(), "in".to_string());
    fb_pin.insert("function".to_string(), "Feedback".to_string());
    fb_pin.insert("impedance".to_string(), "high".to_string());
    pins.insert("FB".to_string(), fb_pin);
    
    // COMP pin with Compensation function
    let mut comp_pin = HashMap::new();
    comp_pin.insert("type".to_string(), "signal".to_string());
    comp_pin.insert("direction".to_string(), "out".to_string());
    comp_pin.insert("function".to_string(), "Compensation".to_string());
    pins.insert("COMP".to_string(), comp_pin);
    
    // VIN pin
    let mut vin_pin = HashMap::new();
    vin_pin.insert("type".to_string(), "power".to_string());
    vin_pin.insert("direction".to_string(), "in".to_string());
    vin_pin.insert("function".to_string(), "PowerIn".to_string());
    pins.insert("VIN".to_string(), vin_pin);
    
    // GND pin
    let mut gnd_pin = HashMap::new();
    gnd_pin.insert("type".to_string(), "ground".to_string());
    gnd_pin.insert("function".to_string(), "Ground".to_string());
    pins.insert("GND".to_string(), gnd_pin);
    
    buck_module.pins = Some(pins);
    module_defs.insert("BuckController".to_string(), buck_module);
    
    result.module_definitions = Some(module_defs);
    result
}

fn create_mock_netlist() -> Netlist {
    // Create a minimal netlist for testing
    // In a real scenario, this would come from the synthesizer
    Netlist::new("test_circuit".to_string())
}

fn create_instance_mapping() -> HashMap<bhdl_netlist::InstanceId, bhdl_spice::ComponentId> {
    // Create mapping between netlist instances and circuit components
    // In a real scenario, this would be built during circuit conversion
    HashMap::new()
}

fn print_roles(roles: &HashMap<bhdl_spice::ComponentId, ComponentRole>) {
    let component_names = vec![
        ("L1", bhdl_spice::ComponentId::new(1)),
        ("D1", bhdl_spice::ComponentId::new(2)),
        ("C1", bhdl_spice::ComponentId::new(3)),
        ("C2", bhdl_spice::ComponentId::new(4)),
        ("R1", bhdl_spice::ComponentId::new(5)),
        ("R2", bhdl_spice::ComponentId::new(6)),
        ("R3", bhdl_spice::ComponentId::new(7)),
        ("C3", bhdl_spice::ComponentId::new(8)),
        ("R4", bhdl_spice::ComponentId::new(9)),
    ];
    
    for (name, id) in component_names {
        if let Some(role) = roles.get(&id) {
            println!("  {} -> {:?}", name, role);
        }
    }
}

fn compare_results(
    without: &HashMap<bhdl_spice::ComponentId, ComponentRole>,
    with: &HashMap<bhdl_spice::ComponentId, ComponentRole>
) {
    let mut improvements = 0;
    let mut changes = Vec::new();
    
    for (id, role_with) in with {
        if let Some(role_without) = without.get(id) {
            if role_with != role_without {
                improvements += 1;
                changes.push((id, role_without, role_with));
            }
        }
    }
    
    println!("  Total improvements: {}", improvements);
    for (id, old_role, new_role) in changes {
        println!("  Component {:?}: {:?} -> {:?}", id, old_role, new_role);
    }
    
    println!("\nKey benefits of pin metadata:");
    println!("  - Switch node identification without relying on naming");
    println!("  - Accurate feedback/compensation network detection");
    println!("  - Distinguishes between different capacitor roles");
    println!("  - Enables detection of specialized pins (bootstrap, soft-start, etc.)");
}