//! Test Component Role Detection using Simulation-Based Analysis

use std::error::Error;
use bhdl_spice::circuit::Circuit;
use bhdl_spice::extended_analysis::{ComponentRoleDetector, ComponentRole};

fn main() -> Result<(), Box<dyn Error>> {
    println!("Component Role Detection Test\n");
    println!("============================\n");
    
    // Create a realistic 7805 voltage regulator circuit
    let circuit = create_realistic_7805_circuit();
    
    // Initialize the component role detector
    let mut detector = ComponentRoleDetector::new(circuit);
    
    // Initialize the real simulation engine
    println!("Initializing simulation engine...");
    match detector.initialize_simulation() {
        Ok(()) => println!("✅ Simulation engine initialized successfully"),
        Err(e) => {
            println!("⚠️  Simulation engine failed to initialize: {}", e);
            println!("   Falling back to mock analysis");
        }
    }
    
    // Detect roles for all components
    println!("Detecting component roles using real simulation...\n");
    let roles = detector.detect_all_roles();
    
    // Display results
    println!("Component Role Analysis Results:");
    println!("-------------------------------");
    
    for (component_id, role) in roles {
        let component_name = detector.circuit.get_component(component_id)
            .map(|c| c.name())
            .unwrap_or("Unknown");
        let component_type = detector.circuit.get_component(component_id)
            .map(|c| c.component_type())
            .unwrap_or("Unknown");
            
        let role_description = get_role_description(&role);
        let icon = get_role_icon(&role);
        
        println!("{} {} ({}): {:?}", icon, component_name, component_type, role);
        println!("   {}", role_description);
    }
    
    println!("\n{}", "=".repeat(50));
    println!("Simulation-Based Analysis Summary:");
    println!("- Input filtering components reduce voltage ripple");
    println!("- Output stabilization components improve settling time");
    println!("- Protection components prevent circuit damage");
    println!("- Load components determine power requirements");
    
    Ok(())
}

fn create_realistic_7805_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Add nodes representing the power supply circuit
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("PROTECTED_VIN".to_string(), None);
    circuit.add_node("VCC".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    // Set realistic node voltages
    if let Some((vin_id, _)) = circuit.get_node("VIN") {
        circuit.set_node_voltage(vin_id, 12.0);
    }
    if let Some((protected_vin_id, _)) = circuit.get_node("PROTECTED_VIN") {
        circuit.set_node_voltage(protected_vin_id, 12.0);
    }
    if let Some((vcc_id, _)) = circuit.get_node("VCC") {
        circuit.set_node_voltage(vcc_id, 5.0);
    }
    if let Some((gnd_id, _)) = circuit.get_node("GND") {
        circuit.set_node_voltage(gnd_id, 0.0);
    }
    
    // Input protection: TVS Diode
    circuit.add_branch(
        "D1".to_string(),
        "VIN",
        "PROTECTED_VIN",
        "TVSDiode".to_string(),
        15.0, // 15V clamping voltage
        None,
    );
    
    // Input filtering: Large electrolytic capacitor
    let cin1_id = circuit.add_branch(
        "C1".to_string(),
        "PROTECTED_VIN",
        "GND",
        "Capacitor".to_string(),
        100e-6, // 100µF
        None,
    );
    circuit.set_branch_current(cin1_id, 0.001); // Small charging current
    
    // Input filtering: Small ceramic bypass capacitor
    circuit.add_branch(
        "C2".to_string(),
        "PROTECTED_VIN", 
        "GND",
        "Capacitor".to_string(),
        0.1e-6, // 0.1µF
        None,
    );
    
    // Main voltage regulator: 7805
    let reg_id = circuit.add_branch(
        "U1".to_string(),
        "PROTECTED_VIN",
        "VCC",
        "VoltageRegulator".to_string(),
        5.0, // 5V output
        None,
    );
    circuit.set_branch_current(reg_id, 0.2); // 200mA load
    
    // Output stabilization: Output capacitor for regulator stability
    circuit.add_branch(
        "C3".to_string(),
        "VCC",
        "GND",
        "Capacitor".to_string(),
        10e-6, // 10µF
        None,
    );
    
    // Output decoupling: High-frequency bypass
    circuit.add_branch(
        "C4".to_string(),
        "VCC",
        "GND",
        "Capacitor".to_string(),
        0.1e-6, // 0.1µF
        None,
    );
    
    // Load: Current-limiting resistor for LED
    let rled_id = circuit.add_branch(
        "R1".to_string(),
        "VCC",
        "VCC", // Will connect to LED anode in real circuit
        "Resistor".to_string(),
        330.0, // 330Ω
        None,
    );
    circuit.set_branch_current(rled_id, 0.015); // 15mA for LED
    
    // Load: Another load resistor simulating circuit load
    let rload_id = circuit.add_branch(
        "R2".to_string(),
        "VCC",
        "GND",
        "Resistor".to_string(),
        25.0, // 25Ω for 200mA at 5V
        None,
    );
    circuit.set_branch_current(rload_id, 0.2); // 200mA
    
    circuit
}

fn get_role_description(role: &ComponentRole) -> &'static str {
    match role {
        ComponentRole::InputFilter => "Reduces input voltage ripple and noise",
        ComponentRole::OutputStabilization => "Provides feedback loop stability for regulator",
        ComponentRole::Decoupling => "Suppresses high-frequency noise and transients",
        ComponentRole::InputProtection => "Protects against overvoltage and reverse polarity",
        ComponentRole::OutputProtection => "Protects against short circuits and overcurrent",
        ComponentRole::FeedbackNetwork => "Sets output voltage in adjustable regulators",
        ComponentRole::EMIFiltering => "Reduces electromagnetic interference",
        ComponentRole::ThermalProtection => "Provides temperature sensing and limiting",
        ComponentRole::Load => "Represents the circuit being powered",
        ComponentRole::Sense => "Provides voltage or current sensing for control",
        ComponentRole::PowerInductor => "Stores energy in switch-mode power supply",
        ComponentRole::CatchDiode => "Provides current path during switch-off time",
        ComponentRole::RectifierDiode => "Converts AC to DC or rectifies output",
        ComponentRole::Snubber => "Suppresses voltage spikes during switching",
        ComponentRole::Compensation => "Controls feedback loop stability",
        ComponentRole::Bootstrap => "Provides power for high-side gate drive",
        ComponentRole::SoftStart => "Controls startup ramp rate",
        ComponentRole::Transformer => "Provides isolation and voltage conversion",
        ComponentRole::PowerSwitch => "Main switching element in SMPS",
        ComponentRole::Unknown => "Role could not be determined from simulation",
    }
}

fn get_role_icon(role: &ComponentRole) -> &'static str {
    match role {
        ComponentRole::InputFilter => "🔵",
        ComponentRole::OutputStabilization => "🟢", 
        ComponentRole::Decoupling => "⚪",
        ComponentRole::InputProtection => "🛡️",
        ComponentRole::OutputProtection => "🔒",
        ComponentRole::FeedbackNetwork => "🔄",
        ComponentRole::EMIFiltering => "📡",
        ComponentRole::ThermalProtection => "🌡️",
        ComponentRole::Load => "⚡",
        ComponentRole::Sense => "👁️",
        ComponentRole::PowerInductor => "🔷",
        ComponentRole::CatchDiode => "⬇️",
        ComponentRole::RectifierDiode => "➡️",
        ComponentRole::Snubber => "💢",
        ComponentRole::Compensation => "📊",
        ComponentRole::Bootstrap => "🔺",
        ComponentRole::SoftStart => "📈",
        ComponentRole::Transformer => "🔀",
        ComponentRole::PowerSwitch => "🔲",
        ComponentRole::Unknown => "❓",
    }
}