//! Test voltage shift transformation for LED convergence

use anyhow::Result;
use bhdl_spice::{Circuit, ComponentModel, ElectricalLimits, GlacierSolver};

fn main() -> Result<()> {
    println!("=== Testing Voltage Shift Transformation ===\n");
    
    // First, let's understand the transformation mathematically
    println!("Mathematical Analysis:");
    println!("---------------------");
    println!("Standard Shockley: I = Is * (exp(V/nVt) - 1)");
    println!("Shifted model:     I = Is' * (exp((V-Vf)/Vt') - 1)");
    println!("");
    println!("To match behavior at V=Vf+ΔV:");
    println!("  Is * exp(Vf/nVt) * exp(ΔV/nVt) = Is' * exp(ΔV/Vt')");
    println!("  Therefore: Is' = Is * exp(Vf/nVt) * (Vt'/nVt)");
    println!("");
    
    // Calculate transformed parameters
    let is_original = 3.96e-19;  // Realistic LED Is
    let vf = 2.0;
    let n = 2.0;
    let vt = 0.026;
    
    // For the shifted model to work like reference, we need n'=1 in shifted space
    let is_shifted = is_original * ((vf / (n * vt)).exp());
    
    println!("Original parameters:");
    println!("  Is = {:e} A", is_original);
    println!("  n = {}", n);
    println!("  Vt = {} V", vt);
    println!("  Vf = {} V", vf);
    println!("");
    println!("Transformed parameters:");
    println!("  Is' = {:e} A", is_shifted);
    println!("  n' = 1 (in shifted coordinates)");
    println!("  Vt' = {} V", vt);
    println!("");
    
    // Now let's implement this transformation in practice
    println!("\nPractical Implementation Strategy:");
    println!("---------------------------------");
    println!("1. During matrix building, detect LED/diode branches");
    println!("2. Add Vf offset to the residual for those branches");
    println!("3. This shifts the operating point numerically");
    println!("4. After solving, the physical voltage is correct");
    println!("");
    
    // Test both approaches
    test_standard_led()?;
    test_shifted_led()?;
    
    Ok(())
}

fn test_standard_led() -> Result<()> {
    println!("\nTest 1: Standard LED Model (Should Fail)");
    println!("========================================");
    
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
    
    // Standard LED with realistic Is
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(3.96e-19),  // Realistic but problematic
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(_) => println!("✅ Converged (unexpected!)"),
        Err(e) => println!("❌ Failed as expected: {}", e),
    }
    
    Ok(())
}

fn test_shifted_led() -> Result<()> {
    println!("\nTest 2: Shifted LED Model (Should Work)");
    println!("======================================");
    
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
    
    // Calculate shifted Is
    let is_original = 3.96e-19;
    let vf = 2.0;
    let n = 2.0;
    let vt = 0.026;
    let is_shifted = is_original * ((vf / (n * vt)).exp());
    
    println!("Using shifted Is = {:e} A", is_shifted);
    
    // LED with shifted parameters
    // Note: We'd need to modify the LED model to use voltage shifting
    // For now, we'll use the large Is to demonstrate
    solver.add_model("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 20e-3,
        dynamic_resistance: 10.0,
        saturation_current: Some(is_shifted),  // Much larger, numerically stable
        emission_coefficient: Some(1.0),  // Effective n=1 in shifted space
        thermal_voltage: Some(0.026),
        limits: ElectricalLimits::default(),
    });
    
    match solver.analyze() {
        Ok(_) => println!("✅ Converged with shifted model!"),
        Err(e) => println!("❌ Failed: {}", e),
    }
    
    Ok(())
}

// The proper implementation would be to modify the LED stamping in runtime_models.rs
// to automatically apply the voltage shift transformation:
//
// 1. In the LED model execution:
//    - Use v_effective = v - Vf instead of v
//    - Use transformed Is' = Is * exp(Vf/nVt)
//    - Use n' = 1 for the shifted exponential
//
// 2. This is mathematically equivalent but numerically stable
//
// 3. The solution voltages remain correct (no back-transformation needed)
//    because we're just shifting the coordinate system for the exponential