/// Test stable solver with RLC circuit
/// 
/// This uses backward Euler integration for numerical stability
/// and should produce results comparable to traditional SPICE.

use bhdl_spice::perturbation::stable_solver::*;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Stable Solver RLC Circuit Simulation ===\n");
    
    // Create a series RLC circuit
    // Circuit: 5V -> R(50Ω) -> L(10mH) -> C(100µF) -> GND
    // Using 50Ω for critical damping: R = 2*sqrt(L/C)
    
    let mut circuit = StableCircuit::new(4); // 4 nodes including ground
    
    // Node 0: Ground
    // Node 1: Voltage source positive
    // Node 2: Between R and L
    // Node 3: Between L and C
    
    // Add components
    circuit.add_component(ComponentType::VoltageSource(0.0), 1, 0); // Start at 0V
    circuit.add_component(ComponentType::Resistor(50.0), 1, 2);     // 50Ω
    circuit.add_component(ComponentType::Inductor(10e-3), 2, 3);    // 10mH
    circuit.add_component(ComponentType::Capacitor(100e-6), 3, 0);  // 100µF
    
    // Calculate circuit characteristics
    let l: f64 = 10e-3;
    let c: f64 = 100e-6;
    let r: f64 = 50.0;
    let omega_0 = 1.0 / (l * c).sqrt();
    let zeta = r / 2.0 * (c / l).sqrt();
    
    println!("Circuit configuration:");
    println!("  R = 50 Ω");
    println!("  L = 10 mH");  
    println!("  C = 100 µF");
    println!("  Step input: 0V -> 5V at t=1ms\n");
    
    println!("Circuit characteristics:");
    println!("  Natural frequency: {:.1} Hz", omega_0 / (2.0 * std::f64::consts::PI));
    println!("  Damping ratio ζ = {:.3}", zeta);
    
    if zeta < 1.0 {
        println!("  System is UNDERDAMPED");
        let omega_d = omega_0 * (1.0 - zeta * zeta).sqrt();
        println!("  Damped frequency: {:.1} Hz", omega_d / (2.0 * std::f64::consts::PI));
    } else if (zeta - 1.0).abs() < 0.01 {
        println!("  System is CRITICALLY DAMPED");
    } else {
        println!("  System is OVERDAMPED");
    }
    println!();
    
    // Prepare output file
    let mut output = File::create("tests/outputs/stable_rlc_response.csv").unwrap();
    writeln!(output, "time_ms,v_source,v_r,v_l,v_c,i_circuit").unwrap();
    
    // Simulation parameters
    let dt = 10e-6;       // 10 µs time step
    let total_time = 0.05; // 50 ms total (enough to see settling)
    let steps = (total_time / dt) as usize;
    let step_voltage_time = 0.001; // Apply step at 1ms
    
    // Run simulation
    println!("Running simulation with backward Euler integration...");
    let print_interval = steps / 20; // Print 20 updates
    
    for step in 0..steps {
        let time = step as f64 * dt;
        
        // Apply step voltage at 1ms
        if time >= step_voltage_time && time < step_voltage_time + dt {
            circuit.set_voltage_source(0, 5.0);
            println!("Applied 5V step at t={:.3}ms", time * 1000.0);
        }
        
        // Run one time step
        if !circuit.step(dt) {
            eprintln!("Simulation failed at step {}", step);
            break;
        }
        
        // Record data every 100 µs (every 10 steps)
        if step % 10 == 0 {
            let v_source = circuit.get_component_voltage(0);
            let v_r = circuit.get_component_voltage(1);
            let v_l = circuit.get_component_voltage(2);
            let v_c = circuit.get_component_voltage(3);
            let i_circuit = circuit.get_component_current(1); // Current through resistor
            
            writeln!(output, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.6}",
                time * 1000.0, v_source, v_r, v_l, v_c, i_circuit).unwrap();
        }
        
        // Progress update
        if step % print_interval == 0 {
            let progress = (step as f64 / steps as f64) * 100.0;
            print!("\rProgress: {:.0}%", progress);
            std::io::stdout().flush().unwrap();
        }
    }
    
    println!("\rProgress: 100%");
    
    // Final state
    println!("\nFinal state at t={}ms:", total_time * 1000.0);
    let v_c_final = circuit.get_component_voltage(3);
    let i_final = circuit.get_component_current(1);
    println!("  Capacitor voltage: {:.3} V", v_c_final);
    println!("  Circuit current: {:.6} A", i_final);
    println!("  Expected steady-state: 5.0 V, 0.0 A");
    
    // Check if we reached steady state
    let steady_state_error = (v_c_final - 5.0).abs();
    if steady_state_error < 0.01 {
        println!("  ✓ Reached steady state successfully!");
    } else {
        println!("  ⚠ Not fully settled (error: {:.3}V)", steady_state_error);
    }
    
    println!("\nResults written to: tests/outputs/stable_rlc_response.csv");
    println!("\nTo compare with traditional SPICE:");
    println!("  python3 scripts/compare_spice_results.py");
}