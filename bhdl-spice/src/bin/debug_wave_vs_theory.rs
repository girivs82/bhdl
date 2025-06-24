/// Compare wave solver against exact analytical solutions
/// 
/// Starting with simple RC circuit where we know the exact answer:
/// V_source = 5V, R = 1kΩ, C = 1μF
/// Theoretical: V_c(t) = V_source * (1 - e^(-t/RC))
/// At t = τ (time constant): V_c = 5V * (1 - e^(-1)) = 3.161V
/// At t = 5τ: V_c = 5V * (1 - e^(-5)) = 4.967V

use bhdl_spice::perturbation::generic_wave::*;
use bhdl_spice::perturbation::stable_solver::{StableCircuit, ComponentType as StableComponentType};
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Wave Solver vs Analytical Theory Debug ===");
    
    // Circuit parameters
    let r = 1000.0;     // 1kΩ
    let c = 1e-6;       // 1μF  
    let v_source = 5.0; // 5V step
    let tau = r * c;    // Time constant = 1ms
    
    println!("Circuit: {}V -> {}Ω -> {}μF -> GND", v_source, r, c * 1e6);
    println!("Time constant τ = RC = {:.3} ms", tau * 1000.0);
    
    // Theoretical values at key time points
    let times = vec![0.0, tau, 2.0*tau, 3.0*tau, 5.0*tau];
    println!("\nTheoretical capacitor voltages:");
    for &t in &times {
        let v_theory = v_source * (1.0 - f64::exp(-t/tau));
        println!("  t = {:.1}τ ({:.2}ms): V_c = {:.3}V", t/tau, t*1000.0, v_theory);
    }
    
    // Create wave circuit
    let mut wave_circuit = GenericWaveCircuit::new(50.0);
    
    // Add nodes: 0=ground, 1=source+, 2=between R and C
    wave_circuit.add_node(0);
    wave_circuit.add_node(1); 
    wave_circuit.add_node(2);
    
    // Add components
    let vsource_id = wave_circuit.add_component(
        ComponentType::VoltageSource { voltage: v_source },
        vec![1, 0]
    );
    let resistor_id = wave_circuit.add_component(
        ComponentType::Resistor { resistance: r },
        vec![1, 2]
    );
    let cap_id = wave_circuit.add_component(
        ComponentType::Capacitor { capacitance: c },
        vec![2, 0]
    );
    
    // Create stable circuit for comparison
    let mut stable_circuit = StableCircuit::new(3);
    stable_circuit.add_component(StableComponentType::VoltageSource(v_source), 1, 0);
    stable_circuit.add_component(StableComponentType::Resistor(r), 1, 2);
    stable_circuit.add_component(StableComponentType::Capacitor(c), 2, 0);
    
    // Simulation parameters  
    let dt = tau / 1000.0;  // 1μs time steps (1000 steps per time constant)
    let total_time = 5.0 * tau;
    let steps = (total_time / dt) as usize;
    let record_interval = steps / 100; // Record 100 points
    
    println!("\nSimulation: dt = {:.1}μs, {} steps total", dt * 1e6, steps);
    
    // Output file
    let mut output = File::create("tests/outputs/wave_vs_theory_debug.csv").unwrap();
    writeln!(output, "time_ms,t_over_tau,v_cap_theory,v_cap_wave,v_cap_stable,error_wave_mV,error_stable_mV,wave_converged").unwrap();
    
    // Simulation loop
    for step in 0..steps {
        let time = step as f64 * dt;
        
        // Step both solvers
        let wave_converged = wave_circuit.step(dt);
        let stable_converged = stable_circuit.step(dt);
        
        // Record data at intervals
        if step % record_interval == 0 || step == steps - 1 {
            let t_ratio = time / tau;
            
            // Theoretical value
            let v_theory = v_source * (1.0 - f64::exp(-time/tau));
            
            // Solver values
            let v_wave = wave_circuit.get_node_voltage(2);
            let v_stable = stable_circuit.get_component_voltage(2);
            
            // Errors in mV
            let error_wave = (v_wave - v_theory).abs() * 1000.0;
            let error_stable = (v_stable - v_theory).abs() * 1000.0;
            
            writeln!(output, "{:.3},{:.3},{:.6},{:.6},{:.6},{:.3},{:.3},{}",
                time * 1000.0, t_ratio, v_theory, v_wave, v_stable, 
                error_wave, error_stable, wave_converged).unwrap();
            
            // Print key checkpoints
            if (t_ratio - 1.0).abs() < 0.01 || (t_ratio - 5.0).abs() < 0.01 || step == 0 {
                println!("\nt = {:.2}τ ({:.2}ms):", t_ratio, time * 1000.0);
                println!("  Theory:  {:.3}V", v_theory);
                println!("  Wave:    {:.3}V (error: {:.1}mV, converged: {})", v_wave, error_wave, wave_converged);
                println!("  Stable:  {:.3}V (error: {:.1}mV)", v_stable, error_stable);
            }
        }
    }
    
    println!("\nDetailed results written to: tests/outputs/wave_vs_theory_debug.csv");
    
    // Final summary
    let final_time = total_time;
    let v_theory_final = v_source * (1.0 - f64::exp(-final_time/tau));
    let v_wave_final = wave_circuit.get_node_voltage(2);
    let v_stable_final = stable_circuit.get_component_voltage(2);
    
    println!("\n=== Final Results at t = 5τ ===");
    println!("Theory:  {:.3}V (expected ~4.967V)", v_theory_final);
    println!("Wave:    {:.3}V (error: {:.1}mV)", v_wave_final, (v_wave_final - v_theory_final).abs() * 1000.0);
    println!("Stable:  {:.3}V (error: {:.1}mV)", v_stable_final, (v_stable_final - v_theory_final).abs() * 1000.0);
    
    // Check if wave solver is fundamentally broken
    if v_wave_final.is_nan() || v_wave_final.is_infinite() {
        println!("\n❌ CRITICAL: Wave solver produced NaN/infinite values!");
    } else if (v_wave_final - v_theory_final).abs() > 0.5 {
        println!("\n❌ MAJOR ERROR: Wave solver error > 500mV");
    } else if (v_wave_final - v_theory_final).abs() > 0.05 {
        println!("\n⚠️  SIGNIFICANT ERROR: Wave solver error > 50mV");
    } else {
        println!("\n✅ Wave solver appears to be working correctly");
    }
}