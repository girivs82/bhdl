/// Test filtered transmission line response vs classical model
/// 
/// This shows how applying a low-pass filter to the transmission line
/// response makes it converge to the classical lumped model behavior
/// at lower frequencies (DC to MHz range).

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

fn main() {
    println!("=== Filtered Transmission Line Response ===");
    println!("Applying low-pass filtering to see convergence with classical model\n");
    
    // Circuit parameters
    let v_source = 5.0;
    let r_value = 1000.0;
    let c_value = 1e-6;
    let r_internal = 1.0;
    
    // Transmission line parameters
    let tl_delay = 100e-12; // 100ps
    let z0 = 50.0; // Characteristic impedance
    
    // Classical time constant
    let tau = (r_value + r_internal) * c_value;
    
    // Filter parameters - cutoff at different frequencies
    let cutoff_frequencies = vec![
        1e9,   // 1 GHz - minimal filtering
        100e6, // 100 MHz - moderate filtering  
        10e6,  // 10 MHz - significant filtering
        1e6,   // 1 MHz - heavy filtering
    ];
    
    println!("Circuit Parameters:");
    println!("  R = {}Ω, C = {}μF", r_value, c_value * 1e6);
    println!("  τ = {:.3}ms", tau * 1000.0);
    println!("  TL delay = {:.0}ps", tl_delay * 1e12);
    println!("  Critical frequency = {:.1} MHz\n", 1.41e8 / (4.0 * 0.01) / 1e6);
    
    // Generate responses for each filter cutoff
    for &fc in &cutoff_frequencies {
        println!("Testing with {:.0} MHz low-pass filter:", fc / 1e6);
        generate_filtered_response(v_source, r_value, r_internal, c_value, tau, tl_delay, z0, fc);
    }
}

fn generate_filtered_response(v_source: f64, r_value: f64, r_internal: f64, 
                             c_value: f64, tau: f64, tl_delay: f64, z0: f64, fc: f64) {
    
    let filename = format!("tests/outputs/filtered_tl_{}MHz.csv", fc / 1e6);
    let mut file = File::create(&filename).expect("Could not create file");
    writeln!(file, "time_ps,v_tl_raw,v_tl_filtered,v_classical,error_percent").expect("Could not write header");
    
    // Simulation parameters
    let duration = 10e-9; // 10ns
    let dt = 0.1e-12; // 0.1ps time step for fine resolution
    let num_steps = (duration / dt) as usize;
    
    // Storage for raw TL response
    let mut v_tl_raw = vec![0.0; num_steps];
    
    // Generate raw transmission line response
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time < tl_delay {
            v_tl_raw[i] = 0.0;
        } else {
            // Simple model: voltage divider after wave arrival
            // In reality would have multiple reflections
            let v_incident = v_source * r_value / (r_internal + r_value);
            
            // Add reflection effects (simplified)
            let reflection_coeff = (r_value - z0) / (r_value + z0);
            let num_round_trips = ((time - tl_delay) / (2.0 * tl_delay)) as i32;
            
            // Geometric series of reflections
            let reflection_factor = if num_round_trips > 0 {
                1.0 + 0.1 * reflection_coeff.powi(num_round_trips)
            } else {
                1.0
            };
            
            v_tl_raw[i] = v_incident * reflection_factor;
        }
    }
    
    // Apply low-pass filter (Butterworth 2nd order)
    let v_tl_filtered = butterworth_filter(&v_tl_raw, dt, fc);
    
    // Calculate metrics
    let mut max_error: f64 = 0.0;
    let mut rms_error = 0.0;
    let mut count = 0;
    
    // Write results
    for i in 0..num_steps {
        let time = i as f64 * dt;
        let v_classical = v_source * (1.0 - (-time / tau).exp());
        
        let error = if v_classical > 0.01 {
            ((v_tl_filtered[i] - v_classical) / v_classical * 100.0).abs()
        } else {
            0.0
        };
        
        if time > tl_delay {
            max_error = max_error.max(error);
            rms_error += error * error;
            count += 1;
        }
        
        // Sample output every 10 steps
        if i % 10 == 0 {
            writeln!(file, "{:.1},{:.6},{:.6},{:.6},{:.2}", 
                     time * 1e12, v_tl_raw[i], v_tl_filtered[i], v_classical, error)
                .expect("Could not write data");
        }
    }
    
    rms_error = (rms_error / count as f64).sqrt();
    
    println!("  Max error: {:.1}%", max_error);
    println!("  RMS error: {:.1}%", rms_error);
    println!("  Results saved to {}\n", filename);
}

/// Butterworth 2nd order low-pass filter
fn butterworth_filter(input: &[f64], dt: f64, fc: f64) -> Vec<f64> {
    let wc = 2.0 * PI * fc;
    let k = wc * dt;
    
    // Bilinear transform coefficients for 2nd order Butterworth
    let a = k * k;
    let b = 2.0 * k * 1.414; // sqrt(2) for Butterworth
    let c = 4.0;
    
    let a0 = a;
    let a1 = 2.0 * a;
    let a2 = a;
    let b0 = a + b + c;
    let b1 = 2.0 * a - 2.0 * c;
    let b2 = a - b + c;
    
    let mut output = vec![0.0; input.len()];
    
    // Apply filter
    for i in 2..input.len() {
        output[i] = (a0 * input[i] + a1 * input[i-1] + a2 * input[i-2]
                    - b1 * output[i-1] - b2 * output[i-2]) / b0;
    }
    
    output
}

/// Analyze filter phase delay
fn analyze_filter_delay(fc: f64, signal_freq: f64) -> f64 {
    // Phase delay for Butterworth filter
    let omega = 2.0 * PI * signal_freq;
    let omega_c = 2.0 * PI * fc;
    let phase = -(omega / omega_c).atan();
    phase / omega // Convert phase to time delay
}