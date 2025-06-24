/// Test simple wave solver against analytical RC circuit solution
/// 
/// This test validates that the simple wave solver can correctly handle
/// DC circuits by comparing against known analytical solutions

use bhdl_spice::perturbation::simple_wave::*;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Simple Wave Solver vs Analytical Theory ===");
    
    // Test 1: Simple voltage divider (DC analysis)
    test_voltage_divider();
    
    // Test 2: RC circuit with time evolution (quasi-static analysis)  
    test_rc_circuit();
    
    println!("\n=== Summary ===");
    println!("✅ Simple wave solver correctly handles DC analysis");
    println!("✅ Results match analytical solutions within tolerance");
    println!("✅ Foundation established for extending to AC/transient analysis");
}

fn test_voltage_divider() {
    println!("\n=== Test 1: Voltage Divider ===");
    
    // Circuit: 10V -> R1(1k) -> R2(3k) -> GND
    // Expected: V_middle = 10V * 3k/(1k+3k) = 7.5V
    
    let mut circuit = SimpleWaveCircuit::new(0); // Ground = node 0
    
    // Add nodes
    circuit.add_node(1); // Source positive  
    circuit.add_node(2); // Middle node
    
    // Add components - realistic internal resistance for a 9V battery (~1-2 ohms)
    circuit.add_component(ComponentType::VoltageSource { voltage: 10.0, internal_resistance: 1.0 }, 1, 0);
    circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 1, 2);
    circuit.add_component(ComponentType::Resistor { resistance: 3000.0 }, 2, 0);
    
    // Solve DC
    let converged = circuit.solve_dc(100);
    assert!(converged, "Circuit should converge");
    
    // Check results
    let v_source = circuit.get_node_voltage(1);
    let v_middle = circuit.get_node_voltage(2);
    let v_expected = 10.0 * 3000.0 / (1000.0 + 3000.0); // 7.5V
    
    println!("Voltage divider (R1=1kΩ, R2=3kΩ):");
    println!("  V_source: {:.3}V (expected 10.0V)", v_source);
    println!("  V_middle: {:.3}V (expected {:.3}V)", v_middle, v_expected);
    println!("  Error:    {:.3}mV", (v_middle - v_expected).abs() * 1000.0);
    
    // Verify accuracy
    assert!((v_source - 10.0).abs() < 0.01, "Source voltage error");
    assert!((v_middle - v_expected).abs() < 0.01, "Divider voltage error");
    
    circuit.print_state();
}

fn test_rc_circuit() {
    println!("\n=== Test 2: RC Circuit Analysis ===");
    
    // RC circuit: 5V -> R(1k) -> C(1μF) -> GND
    // In DC steady state: V_C = 5V (capacitor acts as open circuit)
    // Current in DC: I = 0A (no current through capacitor)
    
    let mut circuit = SimpleWaveCircuit::new(0); // Ground = node 0
    
    // Add nodes
    circuit.add_node(1); // Source positive
    circuit.add_node(2); // Between R and C
    
    // Add components - realistic internal resistance for a 5V regulator (~0.1-1 ohm)
    circuit.add_component(ComponentType::VoltageSource { voltage: 5.0, internal_resistance: 0.5 }, 1, 0);
    circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 1, 2);
    circuit.add_component(ComponentType::Capacitor { capacitance: 1e-6 }, 2, 0);
    
    // Solve DC
    let converged = circuit.solve_dc(100);
    assert!(converged, "RC circuit should converge");
    
    // Check results
    let v_source = circuit.get_node_voltage(1);
    let v_cap = circuit.get_node_voltage(2);
    let i_resistor = circuit.get_component_current(1);
    let i_capacitor = circuit.get_component_current(2);
    
    println!("RC Circuit (R=1kΩ, C=1μF) - DC Analysis:");
    println!("  V_source:   {:.3}V (expected 5.0V)", v_source);
    println!("  V_capacitor: {:.3}V (expected 5.0V)", v_cap);
    println!("  I_resistor:  {:.6}A (expected ~0A)", i_resistor);
    println!("  I_capacitor: {:.6}A (expected 0A)", i_capacitor);
    
    // In DC steady state:
    // - Capacitor voltage should equal source voltage (no voltage drop across R)
    // - Current should be zero (capacitor is open circuit)
    assert!((v_source - 5.0).abs() < 0.01, "Source voltage error");
    assert!((v_cap - 5.0).abs() < 0.01, "Capacitor voltage should equal source in DC");
    assert!(i_capacitor.abs() < 1e-6, "Capacitor current should be zero in DC");
    assert!(i_resistor.abs() < 1e-6, "Resistor current should be zero in DC (no current flow)");
    
    // Write detailed component analysis
    println!("\nComponent Analysis:");
    for (&id, component) in &circuit.components {
        println!("  Component {}: V={:.6}V, I={:.6}A, R_dc={:.1}Ω", 
                 id, component.voltage, component.current, component.dc_resistance());
        match component.comp_type {
            ComponentType::VoltageSource { voltage, internal_resistance } => {
                println!("    Type: Voltage Source ({}V, {}Ω internal)", voltage, internal_resistance);
            },
            ComponentType::Resistor { resistance } => {
                println!("    Type: Resistor ({}Ω)", resistance);
            },
            ComponentType::Capacitor { capacitance } => {
                println!("    Type: Capacitor ({}F) - Open circuit in DC", capacitance);
            },
        }
    }
    
    circuit.print_state();
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_wave_accuracy() {
        // Test multiple circuit configurations
        test_voltage_divider();
        test_rc_circuit();
        
        // Additional test: Current divider
        let mut circuit = SimpleWaveCircuit::new(0);
        circuit.add_node(1);
        
        // Voltage source with parallel resistors - realistic internal resistance
        circuit.add_component(ComponentType::VoltageSource { voltage: 10.0, internal_resistance: 1.0 }, 1, 0);
        circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 1, 0);
        circuit.add_component(ComponentType::Resistor { resistance: 2000.0 }, 1, 0);
        
        let converged = circuit.solve_dc(100);
        assert!(converged);
        
        // Parallel resistance = 1/(1/1000 + 1/2000) = 666.67Ω
        let r_parallel = 1.0 / (1.0/1000.0 + 1.0/2000.0);
        let i_total = 10.0 / r_parallel;
        let i1_expected = 10.0 / 1000.0; // Current through R1
        let i2_expected = 10.0 / 2000.0; // Current through R2
        
        let i1_actual = circuit.get_component_current(1);
        let i2_actual = circuit.get_component_current(2);
        
        println!("\nCurrent divider test:");
        println!("  I1: {:.6}A (expected {:.6}A)", i1_actual, i1_expected);
        println!("  I2: {:.6}A (expected {:.6}A)", i2_actual, i2_expected);
        
        // Verify current divider rule
        assert!((i1_actual - i1_expected).abs() < 1e-6);
        assert!((i2_actual - i2_expected).abs() < 1e-6);
    }
}