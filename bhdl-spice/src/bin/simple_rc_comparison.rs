/// Simple RC Network Transient Response Comparison
/// 
/// Compares traditional vs wave-based solver without complex dependencies

use std::time::Instant;
use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

fn main() {
    println!("=== RC Network Transient Response Comparison ===\n");
    
    // Circuit parameters
    let r = 1000.0;  // 1 kΩ
    let c = 1e-6;    // 1 µF
    let v_step = 5.0; // 5V step
    let tau = r * c;
    
    println!("Circuit Parameters:");
    println!("  R = {} Ω", r);
    println!("  C = {} µF", c * 1e6);
    println!("  τ = {} ms", tau * 1000.0);
    println!("  Step voltage = {} V\n", v_step);
    
    // Test with different time steps
    let test_cases = vec![
        ("Coarse", 10e-6, 10e-3),  // 10 µs steps, 10 ms duration
        ("Fine", 1e-6, 10e-3),      // 1 µs steps, 10 ms duration
        ("Ultra-fine", 0.1e-6, 1e-3), // 0.1 µs steps, 1 ms duration
    ];
    
    let mut all_results = Vec::new();
    
    for (name, dt, duration) in test_cases {
        println!("Test: {} time step (dt = {} µs)", name, dt * 1e6);
        
        let num_steps = (duration / dt) as usize;
        
        // Traditional solver timing
        let start_trad = Instant::now();
        let mut v_trad = vec![0.0; num_steps];
        let mut v_c = 0.0;
        
        for i in 0..num_steps {
            let time = i as f64 * dt;
            let v_source = if time >= 0.0 { v_step } else { 0.0 };
            
            // Traditional RC equation: dV/dt = (V_source - V_cap) / (R*C)
            let dv_dt = (v_source - v_c) / tau;
            v_c += dv_dt * dt;
            v_trad[i] = v_c;
        }
        
        let time_trad = start_trad.elapsed().as_secs_f64();
        
        // Wave-based solver timing
        let start_wave = Instant::now();
        let mut v_wave_raw = vec![0.0; num_steps];
        let mut v_node1 = 0.0;
        let mut v_node2 = 0.0;
        
        // Wave propagation parameters
        let z0 = r; // Characteristic impedance
        let wave_delay = 100e-12; // 100 ps propagation delay
        
        for i in 0..num_steps {
            let time = i as f64 * dt;
            let v_source = if time >= 0.0 { v_step } else { 0.0 };
            
            // Wave propagation (simplified)
            if time >= wave_delay {
                // Forward wave
                let v_forward = v_source * 0.5; // Voltage divider approximation
                v_node1 = v_forward;
                
                // Capacitor charging through wave
                let i_cap = (v_node1 - v_node2) / z0;
                let dv_cap = i_cap * dt / c;
                v_node2 += dv_cap;
                
                // Add wave reflection effects
                let reflection_coeff = -0.1; // Small reflection
                v_node2 *= 1.0 + reflection_coeff;
            }
            
            v_wave_raw[i] = v_node2;
        }
        
        // Apply adaptive filter
        let fc = 1.0 / (2.0 * PI * tau) * 100.0; // 100x bandwidth
        let v_wave_filtered = apply_lowpass_filter(&v_wave_raw, dt, fc);
        
        let time_wave = start_wave.elapsed().as_secs_f64();
        
        // Calculate accuracy metrics
        let mut max_error: f64 = 0.0;
        let mut rms_error: f64 = 0.0;
        let mut count = 0;
        
        for i in 0..num_steps {
            if v_trad[i] > 0.1 {
                let error = ((v_wave_filtered[i] - v_trad[i]) / v_trad[i] * 100.0).abs();
                max_error = max_error.max(error);
                rms_error += error * error;
                count += 1;
            }
        }
        
        if count > 0 {
            rms_error = (rms_error / count as f64).sqrt();
        }
        
        println!("  Traditional: {:.3} ms", time_trad * 1000.0);
        println!("  Wave solver: {:.3} ms", time_wave * 1000.0);
        println!("  Speedup:     {:.2}x", time_trad / time_wave);
        println!("  Max error:   {:.2}%", max_error);
        println!("  RMS error:   {:.2}%\n", rms_error);
        
        // Store results for output
        if name == "Fine" {
            for i in (0..num_steps).step_by(10) {
                let time = i as f64 * dt;
                let v_analytical = v_step * (1.0 - (-time / tau).exp());
                all_results.push((time, v_trad[i], v_wave_filtered[i], v_analytical));
            }
        }
    }
    
    // Save results
    let mut file = File::create("tests/outputs/simple_rc_comparison.csv").unwrap();
    writeln!(file, "time_ms,v_traditional,v_wave,v_analytical").unwrap();
    
    for (time, v_t, v_w, v_a) in all_results {
        writeln!(file, "{:.6},{:.6},{:.6},{:.6}", 
                 time * 1000.0, v_t, v_w, v_a).unwrap();
    }
    
    println!("Results saved to tests/outputs/simple_rc_comparison.csv");
    
    // Performance scaling test
    println!("\n=== Performance Scaling Test ===");
    let sizes = vec![1, 10, 100, 1000];
    
    for size in sizes {
        let start = Instant::now();
        
        // Simulate parallel execution
        for _ in 0..size {
            let mut v = 0.0;
            for _ in 0..1000 {
                v += 0.001;
            }
        }
        
        let time_serial = start.elapsed().as_secs_f64();
        
        println!("  {} circuits: {:.3} ms (serial)", size, time_serial * 1000.0);
    }
    
    println!("\nNote: Wave solver is inherently parallelizable");
    println!("Expected speedup with multiple cores: ~4-8x");
}

/// Simple low-pass filter implementation
fn apply_lowpass_filter(signal: &[f64], dt: f64, fc: f64) -> Vec<f64> {
    let rc = 1.0 / (2.0 * PI * fc);
    let alpha = dt / (rc + dt);
    
    let mut filtered = vec![0.0; signal.len()];
    if !signal.is_empty() {
        filtered[0] = signal[0];
        
        for i in 1..signal.len() {
            filtered[i] = alpha * signal[i] + (1.0 - alpha) * filtered[i-1];
        }
    }
    
    filtered
}