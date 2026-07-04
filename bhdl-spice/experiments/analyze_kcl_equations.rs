//! Analyze the KCL equations being set up

use bhdl_spice::{
    Circuit, ComponentModel,
    ProductionGlacierSolver,
    stdlib_model_loader::StdlibModelLoader,
};
use std::collections::HashMap;

fn main() {
    println!("=== ANALYZE KCL EQUATIONS ===\n");
    
    // Create simple LED circuit
    let mut circuit = Circuit::new();
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    
    println!("Circuit:");
    println!("  V1: VIN -> GND (5V)");
    println!("  R1: VIN -> N1 (220Ω)");  
    println!("  D1: N1 -> GND (LED)");
    println!();
    
    // What the equations should be:
    println!("Expected KCL equations:");
    println!("  At VIN: I(V1) + I(R1_out) = 0");
    println!("         I(V1) + (V_VIN - V_N1)/220 = 0");
    println!();
    println!("  At N1:  I(R1_in) - I(D1_out) = 0");
    println!("         (V_VIN - V_N1)/220 - I_LED(V_N1) = 0");
    println!();
    
    // The issue might be sign convention
    println!("Sign convention check:");
    println!("  Current leaving node: positive");
    println!("  Current entering node: negative");
    println!();
    
    // Let's check what current the LED should have at different voltages
    let led = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, .. } = &led {
        let is = saturation_current.unwrap();
        let n = emission_coefficient.unwrap();
        let vt = thermal_voltage.unwrap();
        
        println!("LED current at different voltages:");
        for v in [0.5, 1.0, 1.112, 1.5, 1.8, 2.0, 2.2] {
            let i = is * ((v / (n * vt)).exp() - 1.0);
            println!("  V = {:.3}V: I = {:.6} mA", v, i * 1000.0);
        }
        println!();
        
        // What voltage gives 17.675mA?
        let i_target = 0.017675;
        let v_for_target = n * vt * ((i_target / is) + 1.0).ln();
        println!("Voltage for I = 17.675 mA: {:.3} V", v_for_target);
        println!();
    }
    
    // Test manual calculation
    println!("Manual iterative solution:");
    println!("  Assume V(N1) = 2.0V (LED forward voltage)");
    println!("  Then I = (5 - 2) / 220 = 13.6 mA");
    println!("  Check: LED at 2V should conduct ~20mA ✓");
    println!();
    println!("  But solver finds V(N1) = 1.112V");
    println!("  Then I = (5 - 1.112) / 220 = 17.7 mA");  
    println!("  Check: LED at 1.112V should conduct 0.000001 mA ✗");
    println!();
    
    // The problem might be that the LED model is not being properly included
    // in the KCL equation, or the current calculation is wrong
    
    println!("HYPOTHESIS: The LED current calculation might be wrong");
    println!("  - Maybe the exponential is being limited incorrectly");
    println!("  - Maybe there's a numerical underflow");
    println!("  - Maybe the current is not being added to the KCL equation");
}