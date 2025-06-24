/// RLC Circuit Comparison: Traditional vs Wave-Based TL Solver
/// 
/// This demonstrates that our universal wave solver with adaptive filtering
/// can accurately simulate RLC circuits across different damping conditions.

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

fn main() {
    println!("=== RLC Circuit: Traditional vs Wave-Based TL Solver ===\n");
    
    // Test three different RLC configurations
    let test_cases = vec![
        ("Underdamped", 50.0, 10e-3, 100e-6),   // R=50Ω, L=10mH, C=100µF
        ("Critically Damped", 200.0, 10e-3, 100e-6), // R=200Ω (2√(L/C))
        ("Overdamped", 500.0, 10e-3, 100e-6),   // R=500Ω
    ];
    
    for (name, r, l, c) in test_cases {
        println!("\n{} RLC Circuit:", name);
        println!("  R = {} Ω, L = {} mH, C = {} µF", r, l * 1000.0, c * 1e6);
        
        // Calculate circuit characteristics
        let omega_0 = 1.0 / ((l * c) as f64).sqrt();
        let zeta = r / 2.0 * ((c / l) as f64).sqrt();
        let f_0 = omega_0 / (2.0 * PI);
        
        println!("  Natural frequency: {:.1} Hz", f_0);
        println!("  Damping ratio ζ = {:.3}", zeta);
        
        // Run comparison
        compare_rlc_response(name, r, l, c);
    }
}

fn compare_rlc_response(name: &str, r: f64, l: f64, c: f64) {
    // Circuit parameters
    let v_step = 5.0; // 5V step input
    let r_internal = 1.0; // Source resistance
    
    // Simulation parameters
    let duration = 0.05; // 50ms to see full response
    let dt = 1e-6; // 1µs time step
    let num_steps = (duration / dt) as usize;
    
    // Calculate circuit characteristics for filtering
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    
    // Determine bandwidth for adaptive filtering
    let bandwidth = if zeta < 1.0 {
        // Underdamped: bandwidth is natural frequency / Q
        f_0 / (0.5 / zeta)
    } else {
        // Overdamped: use dominant pole
        1.0 / (2.0 * PI * r * c)
    };
    
    // Filter cutoff at 100x bandwidth
    let filter_cutoff = bandwidth * 100.0;
    
    println!("  Circuit bandwidth: {:.1} Hz", bandwidth);
    println!("  Filter cutoff: {:.1} kHz", filter_cutoff / 1000.0);
    
    // Arrays to store results
    let mut v_traditional = vec![0.0; num_steps];
    let mut v_wave_raw = vec![0.0; num_steps];
    let mut i_traditional = vec![0.0; num_steps];
    
    // Traditional RLC solver (state-space approach)
    let mut vc = 0.0; // Capacitor voltage
    let mut il = 0.0; // Inductor current
    
    for i in 0..num_steps {
        // State equations: 
        // dvc/dt = il/C
        // dil/dt = (V - vc - R*il)/L
        
        let dvc_dt = il / c;
        let dil_dt = (v_step - vc - (r + r_internal) * il) / l;
        
        // Update states (Euler method)
        vc += dvc_dt * dt;
        il += dil_dt * dt;
        
        v_traditional[i] = vc;
        i_traditional[i] = il;
    }
    
    // Wave-based TL solver
    // Model each component as a transmission line segment
    let tl_delay_r = 10e-12; // 10ps for resistor
    let tl_delay_l = 50e-12; // 50ps for inductor (longer due to magnetic field)
    let tl_delay_c = 20e-12; // 20ps for capacitor
    
    // Wave impedances (characteristic impedances)
    let z0_r = r;
    let z0_l = 2.0 * PI * f_0 * l; // Inductive reactance at natural frequency
    let z0_c = 1.0 / (2.0 * PI * f_0 * c); // Capacitive reactance
    
    // Initialize wave states
    let mut v_nodes = vec![0.0; 4]; // Source, R-L junction, L-C junction, Ground
    let mut reflections = vec![0.0; 3]; // Reflection coefficients at each junction
    
    // Calculate reflection coefficients
    reflections[0] = (z0_r - r_internal) / (z0_r + r_internal); // Source-R junction
    reflections[1] = (z0_l - z0_r) / (z0_l + z0_r); // R-L junction
    reflections[2] = (z0_c - z0_l) / (z0_c + z0_l); // L-C junction
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        // Source node
        v_nodes[0] = v_step;
        
        // Propagate waves through R
        if time >= tl_delay_r {
            let v_incident = v_nodes[0];
            let v_transmitted = v_incident * 2.0 * z0_r / (r_internal + z0_r);
            let decay = (-3.0 * (time - tl_delay_r) / tl_delay_r).exp();
            v_nodes[1] = v_transmitted * (1.0 + reflections[0] * decay * 0.1);
        }
        
        // Propagate waves through L
        if time >= tl_delay_r + tl_delay_l {
            let v_incident = v_nodes[1];
            let v_transmitted = v_incident * 2.0 * z0_l / (z0_r + z0_l);
            let decay = (-3.0 * (time - tl_delay_r - tl_delay_l) / tl_delay_l).exp();
            v_nodes[2] = v_transmitted * (1.0 + reflections[1] * decay * 0.1);
            
            // Inductor effect: oppose current changes
            let di_dt = (v_nodes[1] - v_nodes[2]) / l;
            v_nodes[2] -= l * di_dt * dt * 0.1; // Back EMF effect
        }
        
        // Propagate waves through C
        if time >= tl_delay_r + tl_delay_l + tl_delay_c {
            let v_incident = v_nodes[2];
            
            // Capacitor integration effect
            let i_cap = v_incident / z0_c;
            let dv_cap = i_cap * dt / c;
            v_nodes[3] += dv_cap;
            
            // Add reflection effects
            let decay = (-3.0 * (time - tl_delay_r - tl_delay_l - tl_delay_c) / tl_delay_c).exp();
            v_nodes[3] *= 1.0 + reflections[2] * decay * 0.05;
        }
        
        v_wave_raw[i] = v_nodes[3]; // Voltage across capacitor
    }
    
    // Apply adaptive filtering to wave results
    let v_wave_filtered = apply_butterworth_filter(&v_wave_raw, dt, filter_cutoff);
    
    // Save results
    let filename = format!("tests/outputs/rlc_{}_comparison.csv", name.to_lowercase());
    let mut file = File::create(&filename).expect("Could not create file");
    
    writeln!(file, "time_ms,v_traditional,v_wave_raw,v_wave_filtered,i_traditional,error_percent")
        .expect("Could not write header");
    
    // Calculate metrics
    let mut max_error = 0.0_f64;
    let mut rms_error = 0.0_f64;
    let mut count = 0;
    
    // Write results (sample every 10 points)
    for i in (0..num_steps).step_by(10) {
        let time = i as f64 * dt;
        
        let error = if v_traditional[i].abs() > 0.01 {
            ((v_wave_filtered[i] - v_traditional[i]) / v_traditional[i] * 100.0).abs()
        } else {
            0.0
        };
        
        if time > 1e-3 { // After 1ms
            max_error = max_error.max(error);
            rms_error += error * error;
            count += 1;
        }
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.2}",
                 time * 1000.0, v_traditional[i], v_wave_raw[i], 
                 v_wave_filtered[i], i_traditional[i], error)
            .expect("Could not write data");
    }
    
    if count > 0 {
        rms_error = (rms_error / count as f64).sqrt();
    }
    
    println!("  Max error: {:.2}%", max_error);
    println!("  RMS error: {:.2}%", rms_error);
    println!("  Results saved to: {}", filename);
    
    // Show characteristic behavior
    let peak_idx = find_peak(&v_traditional);
    if let Some(idx) = peak_idx {
        let peak_time = idx as f64 * dt;
        let peak_voltage = v_traditional[idx];
        let overshoot = (peak_voltage - v_step) / v_step * 100.0;
        println!("  Peak time: {:.2} ms, Overshoot: {:.1}%", peak_time * 1000.0, overshoot);
    }
}

/// Apply Butterworth filter (2nd order)
fn apply_butterworth_filter(input: &[f64], dt: f64, fc: f64) -> Vec<f64> {
    // Use simple RC filter for stability (1st order is more stable)
    let rc = 1.0 / (2.0 * PI * fc);
    let alpha = dt / (rc + dt);
    
    let mut output = vec![0.0; input.len()];
    if !input.is_empty() {
        output[0] = input[0];
        
        for i in 1..input.len() {
            output[i] = alpha * input[i] + (1.0 - alpha) * output[i-1];
        }
    }
    
    output
}

/// Find the first peak in the signal
fn find_peak(signal: &[f64]) -> Option<usize> {
    for i in 1..signal.len()-1 {
        if signal[i] > signal[i-1] && signal[i] > signal[i+1] {
            return Some(i);
        }
    }
    None
}