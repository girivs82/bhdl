/// Adaptive transmission line model that transitions between wave propagation
/// and lumped behavior based on signal characteristics
///
/// This demonstrates how to adapt the TL model for low-frequency operation

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

fn main() {
    println!("=== Adaptive Transmission Line Model ===");
    println!("Demonstrating smooth transition from wave to lumped behavior\n");
    
    // Circuit parameters
    let v_source = 5.0;
    let r_load = 1000.0;
    let c_load = 1e-6;
    let r_source = 1.0;
    
    // Transmission line parameters
    let trace_length = 10e-3; // 10mm
    let z0 = 50.0;
    let velocity = 1.41e8; // m/s in FR4
    let tl_delay = trace_length / velocity;
    
    // Distributed parameters per meter
    let l_per_m = z0 / velocity; // H/m
    let c_per_m = 1.0 / (z0 * velocity); // F/m
    
    println!("Circuit Parameters:");
    println!("  Source: {}V with {}Ω internal resistance", v_source, r_source);
    println!("  Load: {}Ω || {}μF", r_load, c_load * 1e6);
    println!("  Trace: {}mm, Z₀={}Ω, delay={:.1}ps", trace_length * 1000.0, z0, tl_delay * 1e12);
    println!("  Distributed: L={:.1}nH/mm, C={:.1}pF/mm\n", l_per_m * 1e6, c_per_m * 1e15);
    
    // Test with different signal rise times
    let rise_times = vec![
        10e-12,   // 10ps - much faster than TL delay
        100e-12,  // 100ps - comparable to TL delay  
        1e-9,     // 1ns - slower than TL delay
        10e-9,    // 10ns - much slower than TL delay
        100e-9,   // 100ns - quasi-static
    ];
    
    for &t_rise in &rise_times {
        analyze_adaptive_response(v_source, r_source, r_load, c_load, z0, tl_delay, 
                                 l_per_m, c_per_m, trace_length, t_rise);
    }
}

fn analyze_adaptive_response(v_source: f64, r_source: f64, r_load: f64, c_load: f64,
                            z0: f64, tl_delay: f64, l_per_m: f64, c_per_m: f64, 
                            trace_length: f64, t_rise: f64) {
    
    println!("Rise time = {:.0}ps (delay/t_rise = {:.2}):", 
             t_rise * 1e12, tl_delay / t_rise);
    
    let filename = format!("tests/outputs/adaptive_tl_{}ps_rise.csv", t_rise * 1e12);
    let mut file = File::create(&filename).expect("Could not create file");
    writeln!(file, "time_ps,v_source,v_load_wave,v_load_lumped,v_load_adaptive,mode")
        .expect("Could not write header");
    
    // Simulation parameters
    let duration = t_rise * 20.0; // Simulate for 20 rise times
    let dt = t_rise / 100.0; // 100 points per rise time
    let num_steps = (duration / dt) as usize;
    
    // Lumped equivalent parameters
    let l_total = l_per_m * trace_length;
    let c_total = c_per_m * trace_length;
    
    println!("  Lumped equivalent: L={:.1}nH, C={:.1}pF", l_total * 1e9, c_total * 1e12);
    
    // State variables for lumped model
    let mut i_lumped = 0.0;
    let mut v_c_lumped = 0.0;
    
    // State for wave model
    let mut wave_queue: Vec<(f64, f64)> = Vec::new(); // (arrival_time, voltage)
    let mut v_load_wave = 0.0;
    
    for step in 0..num_steps {
        let time = step as f64 * dt;
        
        // Source voltage with finite rise time
        let v_in = if time < t_rise {
            v_source * (time / t_rise)
        } else {
            v_source
        };
        
        // 1. Pure wave propagation model
        if time >= tl_delay {
            // Simplified: just the initial wave arrival
            let t_wave = time - tl_delay;
            let v_wave_in = if t_wave < t_rise {
                v_source * (t_wave / t_rise)
            } else {
                v_source
            };
            v_load_wave = v_wave_in * r_load / (r_source + r_load);
        } else {
            v_load_wave = 0.0;
        }
        
        // 2. Pure lumped model (including trace L)
        // di/dt = (v_in - i*R_total - v_c) / L_total
        let r_total = r_source + r_load;
        let di_dt = (v_in - i_lumped * r_total - v_c_lumped) / l_total;
        i_lumped += di_dt * dt;
        
        // dv_c/dt = i / C_total
        let dvc_dt = i_lumped / (c_total + c_load);
        v_c_lumped += dvc_dt * dt;
        
        // 3. Adaptive model - blend based on frequency content
        let adaptation_factor = calculate_adaptation_factor(t_rise, tl_delay);
        let v_load_adaptive = v_load_wave * (1.0 - adaptation_factor) + v_c_lumped * adaptation_factor;
        
        // Determine mode
        let mode = if adaptation_factor < 0.2 {
            "wave"
        } else if adaptation_factor > 0.8 {
            "lumped"
        } else {
            "hybrid"
        };
        
        // Write data
        if step % 10 == 0 {
            writeln!(file, "{:.1},{:.6},{:.6},{:.6},{:.6},{}",
                     time * 1e12, v_in, v_load_wave, v_c_lumped, v_load_adaptive, mode)
                .expect("Could not write data");
        }
    }
    
    println!("  Adaptation factor: {:.2} (0=wave, 1=lumped)", 
             calculate_adaptation_factor(t_rise, tl_delay));
    println!("  Results saved to {}\n", filename);
}

/// Calculate adaptation factor based on signal characteristics
/// Returns 0 for pure wave behavior, 1 for pure lumped behavior
fn calculate_adaptation_factor(t_rise: f64, tl_delay: f64) -> f64 {
    let ratio = t_rise / tl_delay;
    
    if ratio < 0.1 {
        0.0 // Pure wave propagation
    } else if ratio > 10.0 {
        1.0 // Pure lumped model
    } else {
        // Smooth transition using tanh
        0.5 * (1.0 + ((ratio - 1.0) / 2.0).tanh())
    }
}

/// Frequency-aware adaptation for AC analysis
fn frequency_adaptation(frequency: f64, critical_freq: f64) -> f64 {
    if frequency < critical_freq / 100.0 {
        1.0 // Use lumped model
    } else if frequency > critical_freq {
        0.0 // Use wave model
    } else {
        // Logarithmic transition
        let log_ratio = (frequency / critical_freq).log10();
        0.5 * (1.0 - log_ratio)
    }
}

/// Smart averaging of reflections for low-frequency operation
fn average_reflections(reflections: &[(f64, f64)], time_window: f64) -> f64 {
    // For slow signals, multiple reflections within a time window
    // can be averaged to give effective DC behavior
    let total: f64 = reflections.iter()
        .filter(|(t, _)| *t < time_window)
        .map(|(_, v)| v)
        .sum();
    
    total / reflections.len().max(1) as f64
}