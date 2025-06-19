//! Test pin metadata functionality for component role detection

use bhdl_spice::circuit::Circuit;
use bhdl_spice::pin_metadata::{ComponentPinDatabase, PinFunction};

fn main() {
    println!("Pin Metadata Demonstration");
    println!("==========================\n");
    
    // Create a simple circuit
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vin = circuit.add_node("VIN".to_string(), None);
    let sw = circuit.add_node("SW".to_string(), None);
    let boot = circuit.add_node("BOOT".to_string(), None);
    let fb = circuit.add_node("FB".to_string(), None);
    let comp = circuit.add_node("COMP".to_string(), None);
    let ss = circuit.add_node("SS".to_string(), None);
    let gnd = circuit.add_node("GND".to_string(), None);
    let vout = circuit.add_node("VOUT".to_string(), None);
    
    // Add a buck controller
    let buck_id = circuit.add_branch(
        "U1".to_string(), 
        "VIN", 
        "SW", 
        "BuckController".to_string(), 
        5.0, 
        None
    );
    
    // Create pin database
    let pin_db = ComponentPinDatabase::new_with_defaults();
    
    // Test pin metadata retrieval
    println!("Buck Controller Pin Metadata:");
    println!("-----------------------------");
    
    let pins = ["VIN", "SW", "BOOT", "FB", "COMP", "SS", "EN", "GND"];
    
    for pin in &pins {
        if let Some(metadata) = pin_db.get_pin_metadata("BuckController", pin) {
            println!("Pin {}: {:?}", pin, metadata.function);
            if let Some(desc) = &metadata.description {
                println!("  Description: {}", desc);
            }
            if let Some((vmin, vmax)) = metadata.electrical.voltage_range {
                println!("  Voltage range: {:.1}V to {:.1}V", vmin, vmax);
            }
            if let Some(dv_dt) = metadata.electrical.dv_dt_rating {
                println!("  dV/dt rating: {:.0}V/µs", dv_dt);
            }
            println!();
        }
    }
    
    // Test function checking
    println!("\nPin Function Tests:");
    println!("-------------------");
    
    println!("Is SW a switch node? {}", 
        pin_db.pin_has_function("BuckController", "SW", &PinFunction::SwitchNode));
    println!("Is BOOT a bootstrap pin? {}", 
        pin_db.pin_has_function("BuckController", "BOOT", &PinFunction::Bootstrap));
    println!("Is FB a feedback pin? {}", 
        pin_db.pin_has_function("BuckController", "FB", &PinFunction::Feedback));
    println!("Is COMP a compensation pin? {}", 
        pin_db.pin_has_function("BuckController", "COMP", &PinFunction::Compensation));
    println!("Is SS a soft-start pin? {}", 
        pin_db.pin_has_function("BuckController", "SS", &PinFunction::SoftStart));
    
    // Test circuit integration
    println!("\nCircuit Integration Test:");
    println!("------------------------");
    
    let component_pins = circuit.get_component_pin_metadata(buck_id, &pin_db);
    println!("Retrieved {} pin metadata entries for buck controller", component_pins.len());
    
    for (pin_name, metadata) in &component_pins {
        println!("  {}: {:?}", pin_name, metadata.function);
    }
    
    // Add components connected to these pins and show how they would be identified
    println!("\nComponent Role Identification Based on Pin Connections:");
    println!("------------------------------------------------------");
    
    // Add bootstrap capacitor
    circuit.add_branch("C_BOOT".to_string(), "SW", "BOOT", "Capacitor".to_string(), 0.1e-6, None);
    println!("C_BOOT connected between SW and BOOT pins → Should be identified as Bootstrap capacitor");
    
    // Add soft-start capacitor
    circuit.add_branch("C_SS".to_string(), "SS", "GND", "Capacitor".to_string(), 10e-9, None);
    println!("C_SS connected to SS pin → Should be identified as SoftStart capacitor");
    
    // Add feedback resistors
    circuit.add_branch("R_FB1".to_string(), "VOUT", "FB", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("R_FB2".to_string(), "FB", "GND", "Resistor".to_string(), 1870.0, None);
    println!("R_FB1/R_FB2 connected to FB pin → Should be identified as FeedbackNetwork");
    
    // Add compensation network
    circuit.add_branch("R_COMP".to_string(), "FB", "COMP", "Resistor".to_string(), 13300.0, None);
    circuit.add_branch("C_COMP".to_string(), "COMP", "GND", "Capacitor".to_string(), 3.3e-9, None);
    println!("R_COMP/C_COMP connected to COMP pin → Should be identified as Compensation");
}