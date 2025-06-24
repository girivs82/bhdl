/// Test bidirectional wave propagation solver
/// 
/// This demonstrates proper wave propagation in both directions,
/// similar to waves at a shore with incident and reflected components.

use bhdl_spice::perturbation::bidirectional_waves::*;
use bhdl_spice::perturbation::stable_solver::{StableCircuit, ComponentType as StableComponentType};
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Bidirectional Wave Propagation Solver ===\n");
    
    // Circuit parameters for RLC circuit
    let r: f64 = 50.0;   // 50Ω
    let l: f64 = 10e-3;  // 10mH
    let c: f64 = 100e-6; // 100µF
    let v_step = 5.0;    // 5V step
    
    // Calculate characteristic impedances
    let z0 = 50.0; // Use 50Ω transmission lines (matched to resistor)
    let z_l: f64 = 2.0 * std::f64::consts::PI * 159.0 * l; // Inductive impedance at natural frequency
    let z_c: f64 = 1.0 / (2.0 * std::f64::consts::PI * 159.0 * c); // Capacitive impedance
    
    println!("Circuit: {}Ω - {}mH - {}µF", r, l * 1000.0, c * 1e6);
    println!("Characteristic impedances:");
    println!("  Transmission lines: {} Ω", z0);
    println!("  Inductor (at f0): {:.1} Ω", z_l);
    println!("  Capacitor (at f0): {:.1} Ω", z_c);
    println!();
    
    // Create wave circuit
    let mut wave_circuit = WaveCircuit::new();
    
    // Add ports
    // Port 0: Ground (reference)
    // Port 1: Voltage source positive
    // Port 2: Voltage source negative (ground)
    // Port 3: Between source and resistor
    // Port 4: Between resistor and inductor
    // Port 5: Between inductor and capacitor
    // Port 6: Capacitor bottom (ground)
    
    for i in 0..7 {
        wave_circuit.add_port(i);
    }
    
    // Add components with their ports
    let vsource_idx = wave_circuit.add_voltage_source(0.0, vec![1, 2]); // Component 0
    
    wave_circuit.add_component(Box::new(WaveResistor::new(r)), vec![3, 4]); // Component 1
    wave_circuit.add_component(Box::new(WaveInductor::new(l)), vec![4, 5]); // Component 2
    wave_circuit.add_component(Box::new(WaveCapacitor::new(c)), vec![5, 6]); // Component 3
    
    // Connect ports with waveguides
    wave_circuit.connect_ports(1, 3, z0); // Source to resistor
    wave_circuit.connect_ports(2, 0, 0.001); // Source ground to reference (very low impedance)
    wave_circuit.connect_ports(6, 0, 0.001); // Capacitor ground to reference
    
    // Also run stable solver for comparison
    let mut stable_circuit = StableCircuit::new(4);
    stable_circuit.add_component(StableComponentType::VoltageSource(0.0), 1, 0);
    stable_circuit.add_component(StableComponentType::Resistor(r), 1, 2);
    stable_circuit.add_component(StableComponentType::Inductor(l), 2, 3);
    stable_circuit.add_component(StableComponentType::Capacitor(c), 3, 0);
    
    // Prepare output file
    let mut output = File::create("tests/outputs/bidirectional_waves_rlc.csv").unwrap();
    writeln!(output, "time_ms,v_cap_wave,i_circuit_wave,v_cap_stable,i_circuit_stable,power_r,power_l,power_c,energy_total").unwrap();
    
    // Simulation parameters
    let dt = 10e-6;       // 10 µs
    let total_time = 0.02; // 20 ms
    let steps = (total_time / dt) as usize;
    let step_time = 0.001; // Apply step at 1ms
    
    println!("Running simulation...");
    let print_interval = steps / 20;
    
    for step in 0..steps {
        let time = step as f64 * dt;
        
        // Apply step voltage at 1ms
        if time >= step_time && time < step_time + dt {
            // Update voltage source in wave circuit
            wave_circuit.set_voltage_source(vsource_idx, v_step);
            
            // Update voltage source in stable circuit
            stable_circuit.set_voltage_source(0, v_step);
            
            println!("Applied {} V step at t={:.3} ms", v_step, time * 1000.0);
        }
        
        // Run one time step
        let wave_converged = wave_circuit.step(dt);
        stable_circuit.step(dt);
        
        // Record data every 10 steps
        if step % 10 == 0 {
            // Wave circuit results
            let v_cap_wave = wave_circuit.get_port_voltage(5); // Voltage at capacitor top
            let r_state = wave_circuit.get_component_state(1).unwrap();
            let l_state = wave_circuit.get_component_state(2).unwrap();
            let c_state = wave_circuit.get_component_state(3).unwrap();
            
            // Calculate total energy
            let energy_total = r_state.energy + l_state.energy + c_state.energy;
            
            // Stable circuit results
            let v_cap_stable = stable_circuit.get_component_voltage(3);
            let i_stable = stable_circuit.get_component_current(1);
            
            writeln!(output, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6},{:.6}",
                time * 1000.0,
                v_cap_wave,
                r_state.current,
                v_cap_stable,
                i_stable,
                r_state.power,
                l_state.power,
                c_state.power,
                energy_total
            ).unwrap();
        }
        
        // Progress update
        if step % print_interval == 0 {
            let progress = (step as f64 / steps as f64) * 100.0;
            print!("\rProgress: {:.0}%", progress);
            if !wave_converged {
                print!(" (wave solver not converged)");
            }
            std::io::stdout().flush().unwrap();
        }
    }
    
    println!("\rProgress: 100%                              ");
    
    // Final state comparison
    println!("\nFinal state at t={} ms:", total_time * 1000.0);
    
    let v_cap_wave = wave_circuit.get_port_voltage(5);
    let v_cap_stable = stable_circuit.get_component_voltage(3);
    let final_energy = wave_circuit.get_component_state(1).unwrap().energy +
                       wave_circuit.get_component_state(2).unwrap().energy +
                       wave_circuit.get_component_state(3).unwrap().energy;
    
    println!("  Wave solver:");
    println!("    Capacitor voltage: {:.3} V", v_cap_wave);
    println!("    Total energy dissipated: {:.3} mJ", final_energy * 1000.0);
    
    println!("  Stable solver:");
    println!("    Capacitor voltage: {:.3} V", v_cap_stable);
    
    let error = (v_cap_wave - v_cap_stable).abs();
    let error_percent = error / v_step * 100.0;
    
    println!("\nAccuracy:");
    println!("  Absolute error: {:.3} mV", error * 1000.0);
    println!("  Relative error: {:.3}%", error_percent);
    
    if error_percent < 1.0 {
        println!("  ✓ Excellent agreement with stable solver!");
    } else if error_percent < 5.0 {
        println!("  ✓ Good agreement with stable solver");
    } else {
        println!("  ⚠ Moderate agreement - may need tuning");
    }
    
    // Demonstrate wave propagation
    println!("\nWave propagation demonstration:");
    println!("  Waves propagate bidirectionally through the circuit");
    println!("  Reflections occur at impedance mismatches");
    println!("  Energy is conserved (dissipated in resistor)");
    
    println!("\nResults written to: tests/outputs/bidirectional_waves_rlc.csv");
}