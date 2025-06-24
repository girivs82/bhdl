/// Test if our TL solver can handle inductors
/// 
/// This extends the proven RC algorithm to include inductor effects

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

fn main() {
    println!("=== Testing TL Solver with Inductors ===\n");
    
    // First test: Simple RL circuit (no capacitor)
    test_rl_circuit();
    
    // Second test: LC circuit (no resistor - should oscillate)
    test_lc_circuit();
    
    // Third test: RLC circuit (all components)
    test_rlc_circuit();
}

fn test_rl_circuit() {
    println!("Test 1: RL Circuit (R=50Ω, L=10mH)");
    println!("─────────────────────────────────────");
    
    let v_source = 5.0;
    let r_value = 50.0;
    let l_value = 10e-3;
    let r_internal = 1.0;
    
    // Time constant for RL circuit
    let tau_rl = l_value / (r_value + r_internal);
    println!("  RL time constant τ = L/R = {:.3} ms", tau_rl * 1000.0);
    
    // Simulation parameters
    let duration = 5e-3; // 5ms
    let dt = 1e-6; // 1µs
    let num_steps = (duration / dt) as usize;
    
    // Traditional RL solution
    let mut i_traditional = vec![0.0; num_steps];
    let mut v_l_traditional = vec![0.0; num_steps];
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        // Current in RL circuit: I(t) = (V/R)(1 - e^(-t/τ))
        i_traditional[i] = (v_source / (r_value + r_internal)) * (1.0 - (-time / tau_rl).exp());
        // Voltage across inductor: V_L = V * e^(-t/τ)
        v_l_traditional[i] = v_source * (-time / tau_rl).exp();
    }
    
    // TL model for RL circuit
    let tl_delay = 50e-12; // 50ps for inductor
    let mut v_tl_raw = vec![0.0; num_steps];
    let mut i_tl_raw = vec![0.0; num_steps];
    
    // Current state for inductor
    let mut inductor_current = 0.0;
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time < tl_delay {
            v_tl_raw[i] = 0.0;
            i_tl_raw[i] = 0.0;
        } else {
            // After wave arrives at inductor
            // Inductor opposes current change: V = L * di/dt
            let v_applied = v_source - inductor_current * r_value;
            let di_dt = v_applied / l_value;
            inductor_current += di_dt * dt;
            
            // Add wave effects
            let reflection_decay = (-3.0 * (time - tl_delay) / tl_delay).exp();
            i_tl_raw[i] = inductor_current * (1.0 + 0.05 * reflection_decay);
            v_tl_raw[i] = v_applied * (1.0 + 0.05 * reflection_decay);
        }
    }
    
    // Apply filtering
    let fc = 1.0 / (2.0 * PI * tau_rl) * 100.0; // 100x bandwidth
    let i_filtered = apply_rc_filter(&i_tl_raw, dt, fc);
    
    // Save results
    let mut file = File::create("tests/outputs/rl_tl_test.csv").unwrap();
    writeln!(file, "time_ms,i_traditional,i_tl_raw,i_tl_filtered,error_percent").unwrap();
    
    let mut max_error: f64 = 0.0;
    for i in (0..num_steps).step_by(10) {
        let time = i as f64 * dt;
        let error = if i_traditional[i] > 0.001 {
            ((i_filtered[i] - i_traditional[i]) / i_traditional[i] * 100.0).abs()
        } else { 0.0 };
        
        if time > 10.0 * tl_delay {
            max_error = max_error.max(error);
        }
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.2}",
                 time * 1000.0, i_traditional[i], i_tl_raw[i], i_filtered[i], error).unwrap();
    }
    
    println!("  Filter cutoff: {:.1} kHz", fc / 1000.0);
    println!("  Max error: {:.2}%", max_error);
    println!("  Results saved to: tests/outputs/rl_tl_test.csv\n");
}

fn test_lc_circuit() {
    println!("Test 2: LC Circuit (L=10mH, C=100µF)");
    println!("─────────────────────────────────────");
    
    let _v_source = 5.0;
    let l_value = 10e-3;
    let c_value = 100e-6;
    
    // Natural frequency
    let omega_0 = 1.0 / ((l_value * c_value) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    println!("  Natural frequency f₀ = {:.1} Hz", f_0);
    println!("  Period T = {:.2} ms", 1000.0 / f_0);
    
    // This circuit should oscillate - our current TL model can't handle this
    println!("  Note: LC oscillation requires bidirectional energy transfer");
    println!("  Current TL model is unidirectional - needs enhancement\n");
}

fn test_rlc_circuit() {
    println!("Test 3: RLC Circuit");
    println!("───────────────────");
    println!("  Conclusion: Need bidirectional wave propagation for RLC\n");
}

/// Simple first-order RC filter (from proven implementation)
fn apply_rc_filter(input: &[f64], dt: f64, cutoff_freq: f64) -> Vec<f64> {
    let rc = 1.0 / (2.0 * PI * cutoff_freq);
    let alpha = dt / (rc + dt);
    
    let mut output = vec![0.0; input.len()];
    output[0] = input[0];
    
    for i in 1..input.len() {
        output[i] = alpha * input[i] + (1.0 - alpha) * output[i-1];
    }
    
    output
}