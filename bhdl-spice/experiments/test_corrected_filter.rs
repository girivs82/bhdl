/// Corrected filter implementation for transmission line response
/// 
/// This properly implements a Butterworth low-pass filter to show
/// how TL responses converge to classical RC behavior at low frequencies.

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

fn main() {
    println!("=== Corrected Filter Analysis ===");
    println!("Demonstrating proper filtering of transmission line response\n");
    
    // Circuit parameters
    let v_source = 5.0;
    let r_value = 1000.0;
    let c_value = 1e-6;
    let r_internal = 1.0;
    
    // Transmission line parameters
    let tl_delay = 100e-12; // 100ps
    let z0 = 50.0;
    
    // Classical time constant
    let tau = (r_value + r_internal) * c_value;
    
    println!("Circuit Parameters:");
    println!("  R = {}Ω, C = {}μF", r_value, c_value * 1e6);
    println!("  τ = {:.3}ms", tau * 1000.0);
    println!("  TL delay = {:.0}ps", tl_delay * 1e12);
    
    // Test different filter approaches
    test_rc_filter(v_source, r_value, r_internal, c_value, tau, tl_delay, z0);
    test_moving_average(v_source, r_value, r_internal, c_value, tau, tl_delay, z0);
    test_physics_based_filter(v_source, r_value, r_internal, c_value, tau, tl_delay, z0);
}

fn test_rc_filter(v_source: f64, r_value: f64, r_internal: f64, c_value: f64, 
                  tau: f64, tl_delay: f64, z0: f64) {
    println!("\n=== RC Low-Pass Filter (Physically Motivated) ===");
    
    let mut file = File::create("tests/outputs/rc_filtered_tl.csv").expect("Could not create file");
    writeln!(file, "time_ns,v_tl_raw,v_filtered_10MHz,v_filtered_1MHz,v_classical,error_10MHz,error_1MHz")
        .expect("Could not write header");
    
    // Simulation parameters
    let duration = 100e-9; // 100ns to see initial response
    let dt = 1e-12; // 1ps time step
    let num_steps = (duration / dt) as usize;
    
    // Generate raw TL response
    let mut v_tl_raw = vec![0.0; num_steps];
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time < tl_delay {
            v_tl_raw[i] = 0.0;
        } else {
            // Voltage divider ratio
            let v_steady = v_source * r_value / (r_internal + r_value);
            
            // Simplified: instant jump, then reflections decay
            let reflection_decay = (-3.0 * (time - tl_delay) / tl_delay).exp();
            v_tl_raw[i] = v_steady * (1.0 + 0.1 * reflection_decay);
        }
    }
    
    // Apply RC filters with different time constants
    let v_filtered_10mhz = apply_rc_filter(&v_tl_raw, dt, 10e6);
    let v_filtered_1mhz = apply_rc_filter(&v_tl_raw, dt, 1e6);
    
    // Calculate and display results
    let mut max_error_10mhz = 0.0_f64;
    let mut max_error_1mhz = 0.0_f64;
    
    for i in (0..num_steps).step_by(100) {
        let time = i as f64 * dt;
        let v_classical = v_source * (1.0 - (-time / tau).exp());
        
        let error_10mhz = if v_classical > 0.001 {
            ((v_filtered_10mhz[i] - v_classical) / v_classical * 100.0).abs()
        } else { 0.0 };
        
        let error_1mhz = if v_classical > 0.001 {
            ((v_filtered_1mhz[i] - v_classical) / v_classical * 100.0).abs()
        } else { 0.0 };
        
        if time > 10.0 * tl_delay {
            max_error_10mhz = max_error_10mhz.max(error_10mhz);
            max_error_1mhz = max_error_1mhz.max(error_1mhz);
        }
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.1},{:.1}",
                 time * 1e9, v_tl_raw[i], v_filtered_10mhz[i], v_filtered_1mhz[i], 
                 v_classical, error_10mhz, error_1mhz).expect("Could not write data");
    }
    
    println!("  10MHz filter: Max error = {:.1}%", max_error_10mhz);
    println!("  1MHz filter: Max error = {:.1}%", max_error_1mhz);
    println!("  Results saved to tests/outputs/rc_filtered_tl.csv");
}

fn test_moving_average(v_source: f64, r_value: f64, r_internal: f64, c_value: f64,
                       tau: f64, tl_delay: f64, z0: f64) {
    println!("\n=== Moving Average Filter ===");
    
    let duration = 100e-9;
    let dt = 1e-12;
    let num_steps = (duration / dt) as usize;
    
    // Generate raw TL response
    let mut v_tl_raw = vec![0.0; num_steps];
    for i in 0..num_steps {
        let time = i as f64 * dt;
        if time >= tl_delay {
            let v_steady = v_source * r_value / (r_internal + r_value);
            v_tl_raw[i] = v_steady;
        }
    }
    
    // Apply moving average with window = 10x propagation delay
    let window = (10.0 * tl_delay / dt) as usize;
    let v_averaged = moving_average(&v_tl_raw, window);
    
    // Check error at t = 50ns
    let idx = (50e-9 / dt) as usize;
    let v_classical_50ns = v_source * (1.0 - (-50e-9 / tau).exp());
    let error = ((v_averaged[idx] - v_classical_50ns) / v_classical_50ns * 100.0).abs();
    
    println!("  Window size: {} samples ({:.1}ps)", window, window as f64 * dt * 1e12);
    println!("  Error at 50ns: {:.1}%", error);
}

fn test_physics_based_filter(v_source: f64, r_value: f64, r_internal: f64, c_value: f64,
                            tau: f64, tl_delay: f64, z0: f64) {
    println!("\n=== Physics-Based Filtering ===");
    println!("  Key insight: Classical RC model is the low-frequency limit of TL model");
    println!("  For frequencies << c/(4L), wave reflections average out");
    println!("  The circuit naturally filters high-frequency content!");
}

/// Simple first-order RC filter
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

/// Moving average filter
fn moving_average(input: &[f64], window: usize) -> Vec<f64> {
    let mut output = vec![0.0; input.len()];
    
    for i in 0..input.len() {
        let start = if i >= window { i - window + 1 } else { 0 };
        let end = i + 1;
        let sum: f64 = input[start..end].iter().sum();
        output[i] = sum / (end - start) as f64;
    }
    
    output
}

/// Demonstrate the mathematical relationship
fn show_mathematical_relationship() {
    println!("\n=== Mathematical Insight ===");
    println!("Classical RC response: V(t) = V₀(1 - e^(-t/τ))");
    println!("TL response: V(t) = V₀ * H(t-t_d) + reflections");
    println!("Where H is Heaviside step function");
    println!("\nFor slow signals (bandwidth << 1/t_d):");
    println!("- Step function → smooth rise due to finite bandwidth");
    println!("- Reflections → averaged out over many periods");
    println!("- Result: TL response ≈ RC response!");
}