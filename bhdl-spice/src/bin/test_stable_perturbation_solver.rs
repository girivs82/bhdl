/// Test the Stable Perturbation Solver
/// 
/// This demonstrates the perturbation analysis approach where we work with
/// node perturbations and feedback between nodes at each time step

use std::fs::File;
use std::io::Write;
use bhdl_spice::perturbation::stable_solver::{StableCircuit, ComponentType};

fn run_tests() {
    println!("=== Stable Perturbation Solver Analysis ===\n");
    
    test_simple_capacitor();
    test_rc_circuit();
    test_rlc_circuit();
}

fn test_simple_capacitor() {
    println!("Test 1: Single Capacitor with Current Source\n");
    
    // Circuit: I_source -> C -> GND
    // This tests the fundamental capacitor behavior in the perturbation framework
    
    let mut circuit = StableCircuit::new(2);
    
    // For current source, we'll use a very large resistor with voltage source
    // V = I * R, so 0.1A through 1e6Ω gives 100kV (effectively current source)
    let i_source = 0.1;  // 100 mA
    let r_large = 1e6;   // 1 MΩ (large resistance for current source)
    let v_equiv = i_source * r_large;
    let c = 100e-6;      // 100 µF
    
    circuit.add_component(ComponentType::VoltageSource(v_equiv), 1, 0);
    circuit.add_component(ComponentType::Resistor(r_large), 1, 0);
    circuit.add_component(ComponentType::Capacitor(c), 1, 0);
    
    println!("Equivalent current source: {:.1} mA", i_source * 1000.0);
    println!("Capacitor: {:.0} µF", c * 1e6);
    
    let dt = 1e-6;  // 1 µs
    let duration = 10e-3;  // 10 ms
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/stable_capacitor_test.csv").unwrap();
    writeln!(file, "time_ms,vc_perturbation,vc_exact,error_%").unwrap();
    
    for i in 0..steps {
        circuit.step(dt);
        let time = (i + 1) as f64 * dt;
        
        let vc = circuit.node_voltages[1];
        let vc_exact = i_source * time / c;  // V = I*t/C
        
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 1000 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2}", 
                     time * 1000.0, vc, vc_exact, error).unwrap();
        }
    }
    
    let vc_final = circuit.node_voltages[1];
    let vc_expected = i_source * duration / c;
    println!("Final voltage: {:.3} V (expected: {:.3} V)", vc_final, vc_expected);
    println!("Error: {:.2}%\n", ((vc_final - vc_expected) / vc_expected * 100.0).abs());
}

fn test_rc_circuit() {
    println!("Test 2: RC Circuit Step Response\n");
    
    // Circuit: 5V -> R(50Ω) -> C(100µF) -> GND
    let mut circuit = StableCircuit::new(3);
    
    let v_source = 5.0;
    let r = 50.0;
    let c = 100e-6;
    
    // Add components: VCC -> R -> RC_node -> C -> GND
    circuit.add_component(ComponentType::VoltageSource(v_source), 1, 0);  // VCC to GND
    circuit.add_component(ComponentType::Resistor(r), 1, 2);              // VCC to RC node
    circuit.add_component(ComponentType::Capacitor(c), 2, 0);             // RC node to GND
    
    println!("Circuit: {}V -> {}Ω -> {}µF -> GND", v_source, r, c * 1e6);
    
    let tau = r * c;
    println!("Time constant τ = {:.1} ms", tau * 1000.0);
    
    let dt = 1e-6;
    let duration = 25e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/stable_rc_test.csv").unwrap();
    writeln!(file, "time_ms,vc_perturbation,vc_exact,error_%,ic_mA").unwrap();
    
    for i in 0..steps {
        circuit.step(dt);
        let time = (i + 1) as f64 * dt;
        
        // Capacitor voltage is at node 2
        let vc = circuit.node_voltages[2];
        
        // Current through capacitor
        let ic = circuit.get_component_current(2); // Component ID 2 is the capacitor
        
        // Exact solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.3}",
                     time * 1000.0, vc, vc_exact, error, ic * 1000.0).unwrap();
        }
    }
    
    let vc_final = circuit.node_voltages[2];
    let vc_expected = v_source * (1.0 - (-duration / tau).exp());
    println!("Final voltage: {:.3} V (expected: {:.3} V)", vc_final, vc_expected);
    println!("Error: {:.2}%\n", ((vc_final - vc_expected) / vc_expected * 100.0).abs());
}

fn test_rlc_circuit() {
    println!("Test 3: RLC Circuit (Series Resonant)\n");
    
    // Circuit: 5V -> R(10Ω) -> L(1mH) -> C(100µF) -> GND
    let mut circuit = StableCircuit::new(4);
    
    let v_source = 5.0;
    let r = 10.0;
    let l = 1e-3;      // 1 mH
    let c = 100e-6;    // 100 µF
    
    // Add components
    circuit.add_component(ComponentType::VoltageSource(v_source), 1, 0);  // VCC to GND
    circuit.add_component(ComponentType::Resistor(r), 1, 2);              // VCC to R-L node
    circuit.add_component(ComponentType::Inductor(l), 2, 3);              // R-L to L-C node
    circuit.add_component(ComponentType::Capacitor(c), 3, 0);             // L-C to GND
    
    println!("Circuit: {}V -> {}Ω -> {:.1}mH -> {}µF -> GND", 
             v_source, r, l * 1000.0, c * 1e6);
    
    // Calculate characteristic parameters
    let omega0 = 1.0 / (l * c).sqrt();
    let f0 = omega0 / (2.0 * std::f64::consts::PI);
    let q_factor = (l / c).sqrt() / r;
    
    println!("Resonant frequency: {:.1} Hz", f0);
    println!("Q factor: {:.2}", q_factor);
    
    let dt = 1e-6;
    let duration = 20e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/stable_rlc_test.csv").unwrap();
    writeln!(file, "time_ms,vc,vl,ic_mA,il_mA,energy_mJ").unwrap();
    
    for i in 0..steps {
        circuit.step(dt);
        let time = (i + 1) as f64 * dt;
        
        // Node voltages
        let vc = circuit.node_voltages[3];  // Capacitor voltage
        let vl = circuit.node_voltages[2] - circuit.node_voltages[3];  // Inductor voltage
        
        // Currents
        let ic = circuit.get_component_current(3);  // Capacitor current
        let il = circuit.get_component_current(2);  // Inductor current
        
        // Energy calculation
        let energy_c = 0.5 * c * vc * vc;
        let energy_l = 0.5 * l * il * il;
        let total_energy = (energy_c + energy_l) * 1000.0;  // in mJ
        
        if i % 200 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.3},{:.3},{:.3}",
                     time * 1000.0, vc, vl, ic * 1000.0, il * 1000.0, total_energy).unwrap();
        }
    }
    
    println!("RLC simulation completed - check oscillatory behavior in CSV");
    println!("Final capacitor voltage: {:.3} V", circuit.node_voltages[3]);
}

fn main() {
    run_tests();
    
    println!("PERTURBATION ANALYSIS KEY INSIGHTS:\n");
    
    println!("1. NODE-BASED APPROACH:");
    println!("   - Each node has a voltage that evolves based on connected components");
    println!("   - No lumped element assumptions - pure nodal analysis");
    println!("   - Perturbations propagate between nodes at each time step\n");
    
    println!("2. MODIFIED NODAL ANALYSIS (MNA):");
    println!("   - Builds conductance matrix G and current vector b");
    println!("   - Solves Gx = b at each time step");
    println!("   - Backward Euler integration for stability\n");
    
    println!("3. COMPONENT COMPANION MODELS:");
    println!("   - Resistor: G = 1/R, I = 0");
    println!("   - Capacitor: G = C/Δt, I = C*V_prev/Δt");
    println!("   - Inductor: G = Δt/L, I = -I_prev + Δt*V_prev/L\n");
    
    println!("4. ADVANTAGES:");
    println!("   - Handles stiff systems (wide range of time constants)");
    println!("   - Naturally stable (backward Euler)");
    println!("   - Scales to large circuits");
    println!("   - No impedance matching issues\n");
    
    println!("This is the robust approach for general circuit simulation!");
    println!("Results saved to: tests/outputs/stable_*.csv");
}