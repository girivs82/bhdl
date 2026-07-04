//! Visualize component roles in a buck regulator with ASCII art

use std::error::Error;
use bhdl_spice::circuit::Circuit;
use bhdl_spice::extended_analysis::{ComponentRoleDetector, ComponentRole};

fn main() -> Result<(), Box<dyn Error>> {
    println!("\n🔋 Buck Regulator Component Role Visualization");
    println!("{}", "=".repeat(80));
    
    // Create the circuit
    let circuit = create_buck_regulator_circuit();
    
    // Detect roles
    let mut detector = ComponentRoleDetector::new(circuit);
    detector.initialize_simulation().ok();
    let roles = detector.detect_all_roles();
    
    // ASCII art representation
    println!(r#"
    VIN (12V)                                                    VOUT (5V)
        │                                                            │
        ├──[L_EMI]──[F1]──┬──[D_TVS1]                              │
        │    EMI    Fuse  │    15V                                 │
        │                 │                                         │
        │          ┌──────┴──────┬───────┬──────┐                 │
        │          │             │       │      │                 │
        │      [C_IN1]      [C_IN2]  [C_IN3]   │                 │
        │       220µF        10µF    0.1µF     │                 │
        │                                      │                 │
        │                   TPS54360           │                 │
        │              ┌───────────────┐       │                 │
        └──────────────┤ VIN       SW ├───────┼────[L_OUT]──────┼───┐
                       │              │       │     15µH         │   │
                  ┌────┤ BOOT         │    [D_CATCH]             │   │
              [C_BOOT] │              │     0.3V                 │   │
                0.1µF  │   FB  COMP   │       │                  │   │
                  └────┤              ├───┐   │                  │   │
                       └──────────────┘   │   │                  │   │
                                         │   │                  │   │
        Output Capacitors:               │   │                  │   │
        ┌────┬────┬────┬────┬────┐      │   │                  │   │
    [C_OUT1][C_OUT2][C_OUT3][C_OUT4][C_OUT5]│   │              │   │
      47µF   47µF   22µF   10µF  0.1µF  │   │                  │   │
        └────┴────┴────┴────┴────┘      │   │                  │   │
                                         │   │                  │   │
                          ┌──[R_FB1]─────┴───┴──────────────────┤   │
                          │   10kΩ                              │   │
                          │                                     │   │
                          ├──[R_FB2]────┐                       │   │
                          │   1.87kΩ    │                       │   │
                          │             │                       │   │
                          ├──[R_COMP1]──┼──[C_COMP1]           │   │
                          │   13.3kΩ    │    3.3nF             │   │
                          │             │                       │   │
                          │        [R_COMP2]──[C_COMP2]        │   │
                          │         2.2kΩ      22nF            │   │
                          │                                     │   │
                          └─────────────────────────────────────┴───┴──[R_SENSE]──[D_TVS2]──[R_LOAD]
                                                                         10mΩ       5V       1.67Ω
    "#);
    
    // Role legend
    println!("\n📊 Component Role Legend:");
    println!("{}", "─".repeat(40));
    
    let role_colors = [
        (ComponentRole::InputProtection, "🛡️", "Protection against overvoltage/overcurrent"),
        (ComponentRole::InputFilter, "🔵", "Reduces input voltage ripple"),
        (ComponentRole::PowerInductor, "🔷", "Main energy storage element"),
        (ComponentRole::CatchDiode, "⬇️", "Freewheeling current path"),
        (ComponentRole::OutputStabilization, "🟢", "Output voltage stability"),
        (ComponentRole::Decoupling, "⚪", "High-frequency noise suppression"),
        (ComponentRole::FeedbackNetwork, "🔄", "Voltage regulation feedback"),
        (ComponentRole::Compensation, "📊", "Loop stability control"),
        (ComponentRole::Sense, "👁️", "Current/voltage sensing"),
        (ComponentRole::Load, "⚡", "Power consumption"),
        (ComponentRole::EMIFiltering, "📡", "EMI/RFI suppression"),
        (ComponentRole::SoftStart, "📈", "Controlled startup"),
        (ComponentRole::Bootstrap, "🔺", "High-side gate drive"),
    ];
    
    for (role, icon, description) in &role_colors {
        let count = roles.values().filter(|r| *r == role).count();
        if count > 0 {
            println!("{} {:?} ({}): {}", icon, role, count, description);
        }
    }
    
    // Performance summary
    println!("\n📈 Detection Performance:");
    println!("{}", "─".repeat(40));
    let total = roles.len();
    let identified = roles.values().filter(|r| **r != ComponentRole::Unknown).count();
    let accuracy = (identified as f64 / total as f64) * 100.0;
    
    println!("Total components: {}", total);
    println!("Successfully identified: {} ({:.1}%)", identified, accuracy);
    
    // Unidentified components
    let unknown: Vec<_> = roles.iter()
        .filter(|(_, role)| **role == ComponentRole::Unknown)
        .filter_map(|(id, _)| detector.circuit.get_component(*id))
        .collect();
    
    if !unknown.is_empty() {
        println!("\n⚠️  Unidentified components:");
        for comp in unknown {
            println!("  - {} ({})", comp.name(), comp.component_type());
        }
    }
    
    Ok(())
}

fn create_buck_regulator_circuit() -> Circuit {
    let mut circuit = Circuit::new();
    
    // Create nodes
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
    circuit.add_node("COMP_RC".to_string(), None);
    circuit.add_node("VOUT_SENSED".to_string(), None);
    
    // Set voltages
    circuit.set_node_voltage(vin, 12.0);
    circuit.set_node_voltage(vin_fused, 12.0);
    circuit.set_node_voltage(sw, 6.0);
    circuit.set_node_voltage(vout, 5.0);
    circuit.set_node_voltage(fb, 0.8);
    circuit.set_node_voltage(comp, 1.5);
    circuit.set_node_voltage(ss, 0.8);
    circuit.set_node_voltage(boot, 12.0);
    circuit.set_node_voltage(en, 1.2);
    circuit.set_node_voltage(gnd, 0.0);
    
    // Build circuit (same as before)
    circuit.add_branch("L_EMI".to_string(), "VIN", "VIN_FUSED", "Ferrite".to_string(), 600e-6, None);
    circuit.add_branch("F1".to_string(), "VIN_FUSED", "VIN_FUSED", "Fuse".to_string(), 5.0, None);
    circuit.add_branch("D_TVS1".to_string(), "VIN_FUSED", "GND", "TVSDiode".to_string(), 15.0, None);
    circuit.add_branch("C_IN1".to_string(), "VIN_FUSED", "GND", "Capacitor".to_string(), 220e-6, None);
    circuit.add_branch("C_IN2".to_string(), "VIN_FUSED", "GND", "Capacitor".to_string(), 10e-6, None);
    circuit.add_branch("C_IN3".to_string(), "VIN_FUSED", "GND", "Capacitor".to_string(), 0.1e-6, None);
    circuit.add_branch("U1".to_string(), "VIN_FUSED", "SW", "BuckController".to_string(), 5.0, None);
    circuit.add_branch("R_EN1".to_string(), "VIN_FUSED", "EN", "Resistor".to_string(), 100000.0, None);
    circuit.add_branch("R_EN2".to_string(), "EN", "GND", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("C_BOOT".to_string(), "SW", "BOOT", "Capacitor".to_string(), 0.1e-6, None);
    circuit.add_branch("D_CATCH".to_string(), "GND", "SW", "SchottkyDiode".to_string(), 0.3, None);
    circuit.add_branch("L_OUT".to_string(), "SW", "VOUT", "Inductor".to_string(), 15e-6, None);
    circuit.add_branch("C_OUT1".to_string(), "VOUT", "GND", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("C_OUT2".to_string(), "VOUT", "GND", "Capacitor".to_string(), 47e-6, None);
    circuit.add_branch("C_OUT3".to_string(), "VOUT", "GND", "Capacitor".to_string(), 22e-6, None);
    circuit.add_branch("C_OUT4".to_string(), "VOUT", "GND", "Capacitor".to_string(), 10e-6, None);
    circuit.add_branch("C_OUT5".to_string(), "VOUT", "GND", "Capacitor".to_string(), 0.1e-6, None);
    circuit.add_branch("R_FB1".to_string(), "VOUT", "FB", "Resistor".to_string(), 10000.0, None);
    circuit.add_branch("R_FB2".to_string(), "FB", "GND", "Resistor".to_string(), 1870.0, None);
    circuit.add_branch("R_COMP1".to_string(), "FB", "COMP", "Resistor".to_string(), 13300.0, None);
    circuit.add_branch("C_COMP1".to_string(), "COMP", "GND", "Capacitor".to_string(), 3.3e-9, None);
    circuit.add_branch("R_COMP2".to_string(), "COMP", "COMP_RC", "Resistor".to_string(), 2200.0, None);
    circuit.add_branch("C_COMP2".to_string(), "COMP_RC", "GND", "Capacitor".to_string(), 22e-9, None);
    circuit.add_branch("C_SS".to_string(), "SS", "GND", "Capacitor".to_string(), 10e-9, None);
    circuit.add_branch("R_SENSE".to_string(), "VOUT", "VOUT_SENSED", "Resistor".to_string(), 0.01, None);
    circuit.add_branch("D_TVS2".to_string(), "VOUT_SENSED", "GND", "TVSDiode".to_string(), 5.0, None);
    circuit.add_branch("R_LOAD".to_string(), "VOUT_SENSED", "GND", "Resistor".to_string(), 1.67, None);
    
    circuit
}