/// Frequency domain analysis of transmission line vs lumped models
/// 
/// This shows how transmission line and lumped models converge at low frequencies
/// but diverge at high frequencies where wave effects dominate.

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;
use num_complex::Complex;

fn main() {
    println!("=== Frequency Domain Convergence Analysis ===");
    println!("Comparing transmission line vs lumped models across frequency\n");
    
    // Circuit parameters
    let r = 1000.0; // 1kΩ
    let c = 1e-6;   // 1μF
    let l_trace = 10e-3; // 10mm trace length
    
    // Transmission line parameters (typical PCB)
    let z0 = 50.0;  // Characteristic impedance
    let velocity = 1.41e8; // Wave velocity in FR4
    let delay = l_trace / velocity;
    
    println!("Circuit Parameters:");
    println!("  R = {}Ω, C = {}μF", r, c * 1e6);
    println!("  Trace length = {}mm", l_trace * 1000.0);
    println!("  Propagation delay = {:.1}ps", delay * 1e12);
    println!("  Critical frequency = {:.1}MHz\n", velocity / (4.0 * l_trace) / 1e6);
    
    analyze_frequency_response(r, c, z0, delay);
    analyze_time_domain_equivalence();
}

fn analyze_frequency_response(r: f64, c: f64, z0: f64, delay: f64) {
    let mut file = File::create("tests/outputs/frequency_domain_comparison.csv")
        .expect("Could not create file");
    writeln!(file, "freq_Hz,mag_lumped_dB,phase_lumped_deg,mag_tl_dB,phase_tl_deg,error_dB")
        .expect("Could not write header");
    
    // Frequency range: 1Hz to 10GHz
    let frequencies: Vec<f64> = (0..=10000)
        .map(|i| 10.0_f64.powf(i as f64 / 1000.0 - 1.0))
        .collect();
    
    println!("Frequency Response Analysis:");
    
    for &freq in &frequencies {
        let omega = 2.0 * PI * freq;
        let s = Complex::new(0.0, omega);
        
        // Lumped RC model transfer function: H(s) = 1/(1 + sRC)
        let h_lumped = Complex::new(1.0, 0.0) / (Complex::new(1.0, 0.0) + s * r * c);
        
        // Transmission line model (simplified - single delay)
        // H(s) = exp(-s*delay) * Z_load/(Z_0 + Z_load)
        // where Z_load = R || (1/sC) = R/(1 + sRC)
        let z_load = Complex::new(r, 0.0) / (Complex::new(1.0, 0.0) + s * r * c);
        let h_tl = (-s * delay).exp() * z_load / (Complex::new(z0, 0.0) + z_load);
        
        // Convert to dB and degrees
        let mag_lumped_db = 20.0 * h_lumped.norm().log10();
        let phase_lumped_deg = h_lumped.arg() * 180.0 / PI;
        let mag_tl_db = 20.0 * h_tl.norm().log10();
        let phase_tl_deg = h_tl.arg() * 180.0 / PI;
        let error_db = (mag_tl_db - mag_lumped_db).abs();
        
        writeln!(file, "{:.3e},{:.3},{:.3},{:.3},{:.3},{:.3}",
                 freq, mag_lumped_db, phase_lumped_deg, mag_tl_db, phase_tl_deg, error_db)
            .expect("Could not write data");
        
        // Print key frequencies
        if freq.abs() < 1.001 || (freq / 1e3).abs() < 1.001 || (freq / 1e6).abs() < 1.001 
           || (freq / 1e9).abs() < 1.001 {
            println!("  f = {:.0}Hz: Error = {:.1}dB", freq, error_db);
        }
    }
    
    println!("\nResults saved to tests/outputs/frequency_domain_comparison.csv");
}

fn analyze_time_domain_equivalence() {
    println!("\n=== Time Domain Equivalence Analysis ===");
    
    // Show how to make TL model equivalent to lumped at low frequencies
    println!("\nFor low-frequency equivalence (DC to MHz):");
    println!("1. Transmission line delays become negligible phase shifts");
    println!("2. Multiple reflections average out over slow rise times");
    println!("3. Distributed parameters can be lumped when λ >> circuit size");
    
    println!("\nEquivalence conditions:");
    println!("- Rise time >> propagation delay");
    println!("- Frequency << c/(4×length)");
    println!("- Impedance matching not critical (reflections settle quickly)");
    
    // Create comparison showing convergence
    create_convergence_demo();
}

fn create_convergence_demo() {
    let mut file = File::create("tests/outputs/time_domain_convergence.csv")
        .expect("Could not create file");
    writeln!(file, "rise_time_ns,max_error_percent,settling_time_ratio")
        .expect("Could not write header");
    
    // Test different rise times
    let rise_times = vec![0.01, 0.1, 1.0, 10.0, 100.0]; // ns
    
    for &tr_ns in &rise_times {
        let tr = tr_ns * 1e-9;
        let tl_delay = 100e-12; // 100ps
        
        // Error decreases as rise time increases relative to propagation delay
        let delay_ratio = tl_delay / tr;
        let max_error = 100.0 * delay_ratio; // Simplified model
        let settling_ratio = 1.0 + 5.0 * delay_ratio; // How much longer TL takes to settle
        
        writeln!(file, "{:.3},{:.1},{:.2}", tr_ns, max_error, settling_ratio)
            .expect("Could not write data");
        
        println!("  Rise time = {:.1}ns: Max error = {:.1}%, Settling time ratio = {:.2}x",
                 tr_ns, max_error, settling_ratio);
    }
    
    println!("\nResults saved to tests/outputs/time_domain_convergence.csv");
}

/// Calculate effective lumped parameters from distributed TL
fn distributed_to_lumped(l_per_m: f64, c_per_m: f64, length: f64) -> (f64, f64) {
    // For short lines at low frequencies:
    // L_total ≈ l_per_m × length
    // C_total ≈ c_per_m × length
    let l_total = l_per_m * length;
    let c_total = c_per_m * length;
    (l_total, c_total)
}

/// Show how to adapt TL parameters for low-frequency simulation
fn adaptation_strategy() {
    println!("\n=== Adaptation Strategy for Low Frequencies ===");
    println!("1. Replace transmission lines with π or T lumped equivalents");
    println!("2. Use frequency-dependent damping to smooth reflections");
    println!("3. Apply adaptive time stepping based on signal bandwidth");
    println!("4. Hybrid approach: TL for fast edges, lumped for slow signals");
}