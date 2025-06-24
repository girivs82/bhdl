/// Test perturbation-based simulation with RLC circuit
/// 
/// This demonstrates the physics-based simulation approach where
/// perturbations propagate through the circuit naturally.

use bhdl_spice::perturbation::*;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Perturbation-Based RLC Circuit Simulation ===\n");
    
    // Create a series RLC circuit with step input
    let mut circuit = PerturbationCircuit::new();
    
    // Circuit: 5V -> R(100Ω) -> L(10mH) -> C(100µF) -> GND
    
    // Create nodes
    circuit.add_node(0); // Voltage source positive (and input to resistor)
    circuit.add_node(1); // Between resistor and inductor
    circuit.add_node(2); // Between inductor and capacitor
    circuit.add_node(3); // Ground
    
    // Add components
    circuit.add_component(0, Box::new(VoltageSourceModel::new(0.0))); // Start at 0V
    circuit.add_component(1, Box::new(ResistorModel::new(100.0)));    // 100Ω
    circuit.add_component(2, Box::new(InductorModel::new(10e-3)));    // 10mH
    circuit.add_component(3, Box::new(CapacitorModel::new(100e-6)));  // 100µF
    
    // Connect components in series
    circuit.connect(0, 0, 3); // Voltage source between node 0 and ground (node 3)
    circuit.connect(1, 0, 1); // Resistor between node 0 and node 1
    circuit.connect(2, 1, 2); // Inductor between node 1 and node 2
    circuit.connect(3, 2, 3); // Capacitor between node 2 and ground (node 3)
    
    // Prepare output file
    let mut output = File::create("tests/outputs/perturbation_rlc_response.csv").unwrap();
    writeln!(output, "time_ms,v_source,v_r,v_l,v_c,i_circuit").unwrap();
    
    // Simulation parameters
    let dt = 1e-6;        // 1 µs time step
    let total_time = 0.1; // 100 ms total
    let steps = (total_time / dt) as usize;
    let step_voltage_time = 0.001; // Apply step at 1ms
    
    println!("Circuit configuration:");
    println!("  R = 100 Ω");
    println!("  L = 10 mH");  
    println!("  C = 100 µF");
    println!("  Step input: 0V -> 5V at t=1ms\n");
    
    println!("Natural frequency: {:.1} Hz", 1.0 / (2.0 * std::f64::consts::PI * ((10e-3 * 100e-6) as f64).sqrt()));
    println!("Damping factor: {:.3}", 50.0 / ((10e-3 / 100e-6) as f64).sqrt());
    println!();
    
    // Run simulation
    println!("Running simulation...");
    let print_interval = steps / 20; // Print 20 updates
    
    for step in 0..steps {
        let time = step as f64 * dt;
        
        // Apply step voltage at 1ms
        if time >= step_voltage_time && step == (step_voltage_time / dt) as usize {
            // Update voltage source
            circuit.components.insert(0, Box::new(VoltageSourceModel::new(5.0)));
            println!("Applied 5V step at t={:.3}ms", time * 1000.0);
        }
        
        // Run one time step
        let converged = circuit.step(dt);
        
        // Record data every 100 steps (every 0.1ms)
        if step % 100 == 0 {
            let v_source = circuit.components.get(&0).unwrap().get_voltage();
            let i_circuit = circuit.components.get(&1).unwrap().get_current();
            let v_r = i_circuit * 100.0; // V = IR
            let v_l = circuit.components.get(&2).unwrap().get_voltage();
            let v_c = circuit.components.get(&3).unwrap().get_voltage();
            
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
    let v_c_final = circuit.components.get(&3).unwrap().get_voltage();
    let i_final = circuit.components.get(&1).unwrap().get_current();
    println!("  Capacitor voltage: {:.3} V", v_c_final);
    println!("  Circuit current: {:.6} A", i_final);
    println!("  Expected steady-state: 5.0 V, 0.0 A");
    
    println!("\nResults written to: tests/outputs/perturbation_rlc_response.csv");
    println!("\nTo visualize: python3 scripts/plot_rlc_response.py");
}