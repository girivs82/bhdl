/// Test AC and transient analysis with the simple wave solver
/// 
/// This test demonstrates:
/// 1. RC circuit transient response (step input)
/// 2. RC circuit AC response (sinusoidal input)
/// 3. RLC circuit transient response (step input)

use bhdl_spice::perturbation::simple_wave::*;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== AC and Transient Analysis Tests ===");
    
    // Test 1: RC circuit step response
    test_rc_step_response();
    
    // Test 2: RC circuit AC response
    test_rc_ac_response();
    
    // Test 3: RLC circuit step response (if we had inductors)
    // test_rlc_step_response();
    
    println!("\n=== Analysis Complete ===");
}

fn test_rc_step_response() {
    println!("\n=== Test 1: RC Circuit Step Response ===");
    
    // Circuit: 5V step -> R(1kΩ) -> C(1μF) -> GND
    // Expected: V_C(t) = 5V * (1 - e^(-t/RC))
    // Time constant τ = RC = 1kΩ * 1μF = 1ms
    
    let mut circuit = SimpleWaveCircuit::new(0); // Ground = node 0
    circuit.set_time_step(10e-6); // 10μs time step
    
    // Add nodes
    circuit.add_node(1); // Source positive
    circuit.add_node(2); // Between R and C
    
    // Add components - DC step input
    circuit.add_component(ComponentType::VoltageSource { voltage: 5.0, internal_resistance: 1.0 }, 1, 0);
    circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 1, 2);
    circuit.add_component(ComponentType::Capacitor { capacitance: 1e-6 }, 2, 0);
    
    // Run transient analysis for 5ms (5 time constants)
    let results = circuit.solve_transient(5e-3);
    
    // Write results to CSV
    let mut file = File::create("tests/outputs/rc_step_response.csv").expect("Could not create file");
    writeln!(file, "time_ms,v_source,v_capacitor,v_theory").expect("Could not write header");
    
    let tau = 1e-3; // Time constant = 1ms
    
    println!("RC Step Response Results:");
    println!("Time Constant τ = {:.1}ms", tau * 1000.0);
    
    for (i, (time, voltages)) in results.iter().enumerate() {
        let v_source = voltages.get(&1).copied().unwrap_or(0.0);
        let v_capacitor = voltages.get(&2).copied().unwrap_or(0.0);
        
        // Theoretical response: V_C(t) = V_s * (1 - e^(-t/τ))
        let v_theory = 5.0 * (1.0 - (-time / tau).exp());
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6}", 
                 time * 1000.0, v_source, v_capacitor, v_theory).expect("Could not write data");
        
        // Print every 100 steps
        if i % 100 == 0 {
            println!("  t = {:.2}ms: V_C = {:.3}V (theory: {:.3}V), error = {:.1}mV", 
                     time * 1000.0, v_capacitor, v_theory, (v_capacitor - v_theory).abs() * 1000.0);
        }
    }
    
    println!("Results saved to tests/outputs/rc_step_response.csv");
}

fn test_rc_ac_response() {
    println!("\n=== Test 2: RC Circuit AC Response ===");
    
    // Circuit: 5V sine @ 100Hz -> R(1kΩ) -> C(1μF) -> GND
    // Expected: Low-pass filter behavior
    // Cutoff frequency fc = 1/(2πRC) = 159.2 Hz
    // At 100Hz: |H(jω)| = 1/√(1 + (f/fc)²) ≈ 0.85
    
    let mut circuit = SimpleWaveCircuit::new(0); // Ground = node 0
    circuit.set_time_step(10e-6); // 10μs time step
    
    // Add nodes
    circuit.add_node(1); // Source positive
    circuit.add_node(2); // Between R and C
    
    // Add components - AC source at 100Hz
    let frequency = 100.0; // Hz
    let amplitude = 5.0; // V
    let phase = 0.0; // radians
    
    circuit.add_component(ComponentType::AcVoltageSource { 
        amplitude, 
        frequency, 
        phase, 
        internal_resistance: 1.0 
    }, 1, 0);
    circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 1, 2);
    circuit.add_component(ComponentType::Capacitor { capacitance: 1e-6 }, 2, 0);
    
    // Run for 5 periods to reach steady state
    let period = 1.0 / frequency;
    let duration = 5.0 * period;
    let results = circuit.solve_transient(duration);
    
    // Write results to CSV
    let mut file = File::create("tests/outputs/rc_ac_response.csv").expect("Could not create file");
    writeln!(file, "time_ms,v_source,v_capacitor,phase_deg").expect("Could not write header");
    
    // Calculate theoretical response
    let tau = 1e-3; // RC time constant
    let omega = 2.0 * std::f64::consts::PI * frequency;
    let cutoff_freq = 1.0 / (2.0 * std::f64::consts::PI * tau);
    let magnitude = 1.0 / (1.0 + (frequency / cutoff_freq).powi(2)).sqrt();
    let phase_shift = -(frequency / cutoff_freq).atan();
    
    println!("RC AC Response Results:");
    println!("  Input: {:.1}V @ {:.1}Hz", amplitude, frequency);
    println!("  Cutoff frequency: {:.1}Hz", cutoff_freq);
    println!("  Expected magnitude: {:.3}", magnitude);
    println!("  Expected phase shift: {:.1}°", phase_shift * 180.0 / std::f64::consts::PI);
    
    for (time, voltages) in results.iter() {
        let v_source = voltages.get(&1).copied().unwrap_or(0.0);
        let v_capacitor = voltages.get(&2).copied().unwrap_or(0.0);
        
        // Calculate phase (simplified)
        let source_phase = (omega * time).sin().atan2((omega * time).cos());
        let output_phase = if v_capacitor.abs() > 1e-6 {
            let input_normalized = amplitude * (omega * time).sin();
            (v_capacitor / input_normalized).atan() * 180.0 / std::f64::consts::PI
        } else {
            0.0
        };
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.2}", 
                 time * 1000.0, v_source, v_capacitor, output_phase).expect("Could not write data");
    }
    
    // Analyze steady-state response (last period)
    let steady_start_idx = (results.len() as f64 * 0.8) as usize;
    let steady_results = &results[steady_start_idx..];
    
    let mut max_output: f64 = 0.0;
    let mut min_output: f64 = 0.0;
    
    for (_, voltages) in steady_results {
        let v_cap = voltages.get(&2).copied().unwrap_or(0.0);
        max_output = max_output.max(v_cap);
        min_output = min_output.min(v_cap);
    }
    
    let actual_amplitude = (max_output - min_output) / 2.0;
    let actual_magnitude = actual_amplitude / amplitude;
    
    println!("  Actual amplitude: {:.3}V", actual_amplitude);
    println!("  Actual magnitude: {:.3} (error: {:.1}%)", 
             actual_magnitude, (actual_magnitude - magnitude).abs() / magnitude * 100.0);
    
    println!("Results saved to tests/outputs/rc_ac_response.csv");
}

#[allow(dead_code)]
fn test_rlc_step_response() {
    println!("\n=== Test 3: RLC Circuit Step Response ===");
    
    // Circuit: 5V step -> R(100Ω) -> L(1mH) -> C(1μF) -> GND
    // Expected: Damped oscillation or overdamped response
    
    let mut circuit = SimpleWaveCircuit::new(0); // Ground = node 0
    circuit.set_time_step(1e-6); // 1μs time step for higher frequency oscillations
    
    // Add nodes
    circuit.add_node(1); // Source positive
    circuit.add_node(2); // Between R and L
    circuit.add_node(3); // Between L and C
    
    // Add components
    circuit.add_component(ComponentType::VoltageSource { voltage: 5.0, internal_resistance: 1.0 }, 1, 0);
    circuit.add_component(ComponentType::Resistor { resistance: 100.0 }, 1, 2);
    circuit.add_component(ComponentType::Inductor { inductance: 1e-3 }, 2, 3);
    circuit.add_component(ComponentType::Capacitor { capacitance: 1e-6 }, 3, 0);
    
    // Calculate characteristic parameters
    let l: f64 = 1e-3; // H
    let c: f64 = 1e-6; // F
    let r: f64 = 100.0; // Ω
    
    let omega_0 = 1.0 / (l * c).sqrt(); // Natural frequency
    let zeta = r / 2.0 * (c / l).sqrt(); // Damping ratio
    
    println!("RLC Circuit Parameters:");
    println!("  Natural frequency: {:.1} rad/s ({:.1} Hz)", omega_0, omega_0 / (2.0 * std::f64::consts::PI));
    println!("  Damping ratio: {:.3}", zeta);
    
    if zeta < 1.0 {
        println!("  Response: Underdamped (oscillatory)");
    } else if zeta == 1.0 {
        println!("  Response: Critically damped");
    } else {
        println!("  Response: Overdamped");
    }
    
    // Run transient analysis
    let duration = 10e-3; // 10ms
    let results = circuit.solve_transient(duration);
    
    // Write results to CSV
    let mut file = File::create("tests/outputs/rlc_step_response.csv").expect("Could not create file");
    writeln!(file, "time_ms,v_source,v_inductor,v_capacitor").expect("Could not write header");
    
    for (time, voltages) in results.iter() {
        let v_source = voltages.get(&1).copied().unwrap_or(0.0);
        let v_inductor = voltages.get(&2).copied().unwrap_or(0.0);
        let v_capacitor = voltages.get(&3).copied().unwrap_or(0.0);
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6}", 
                 time * 1000.0, v_source, v_inductor, v_capacitor).expect("Could not write data");
    }
    
    println!("Results saved to tests/outputs/rlc_step_response.csv");
}