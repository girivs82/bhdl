/// Detailed comparison of transmission line wave propagation vs analytical RC behavior
/// 
/// This analyzes the fundamental differences between:
/// 1. Classical lumped RC circuit: V_C(t) = V_s * (1 - e^(-t/τ))
/// 2. Transmission line wave propagation with finite propagation delays

use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Transmission Line vs Analytical RC Analysis ===");
    
    // Circuit parameters
    let v_source = 5.0;      // 5V source
    let r_value = 1000.0;    // 1kΩ resistor  
    let c_value = 1e-6;      // 1μF capacitor
    let r_internal = 1.0;    // 1Ω source resistance
    
    // Transmission line parameters
    let tl_delay = 100e-12;  // 100ps propagation delay (resistor)
    let cap_delay = 10e-12;  // 10ps capacitor delay
    
    // Classical RC time constant
    let tau_classical = (r_value + r_internal) * c_value; // (1kΩ + 1Ω) * 1μF ≈ 1ms
    
    println!("Circuit Parameters:");
    println!("  V_source = {:.1}V", v_source);
    println!("  R_total = {:.1}Ω ({}Ω + {}Ω internal)", r_value + r_internal, r_value, r_internal);
    println!("  C = {:.1}μF", c_value * 1e6);
    println!("  τ_classical = {:.3}ms", tau_classical * 1000.0);
    println!("  TL_delay = {:.0}ps", tl_delay * 1e12);
    
    analyze_behavior_comparison(v_source, r_value, r_internal, c_value, tau_classical, tl_delay);
    
    println!("\nFor detailed analysis, see tests/outputs/tl_vs_analytical_comparison.csv");
}

fn analyze_behavior_comparison(v_source: f64, r_value: f64, r_internal: f64, c_value: f64, tau: f64, tl_delay: f64) {
    let mut file = File::create("tests/outputs/tl_vs_analytical_comparison.csv").expect("Could not create file");
    writeln!(file, "time_ps,v_analytical,v_tl_model,v_step,error_percent,phase").expect("Could not write header");
    
    println!("\n=== Behavior Analysis at Key Time Points ===");
    
    // Analyze different time phases
    let time_points = vec![
        0.0,           // t=0: Initial conditions
        50e-12,        // t=50ps: Before first wave arrives
        100e-12,       // t=100ps: First wave arrives (critical point)
        150e-12,       // t=150ps: After first wave
        500e-12,       // t=500ps: Multiple wave reflections
        1e-9,          // t=1ns: Early settling
        10e-9,         // t=10ns: Medium term
        100e-9,        // t=100ns: Approaching steady state
        1e-6,          // t=1μs: Near time constant
        5e-6,          // t=5μs: Well past time constant
    ];
    
    for &time in &time_points {
        analyze_at_time(time, v_source, r_value, r_internal, c_value, tau, tl_delay, &mut file);
    }
    
    // Detailed analysis for first 1ns with fine time resolution
    println!("\n=== Fine Time Resolution Analysis (First 1ns) ===");
    for i in 0..=1000 {
        let time = i as f64 * 1e-12; // 1ps steps
        analyze_at_time(time, v_source, r_value, r_internal, c_value, tau, tl_delay, &mut file);
    }
}

fn analyze_at_time(time: f64, v_source: f64, r_value: f64, r_internal: f64, c_value: f64, tau: f64, tl_delay: f64, file: &mut File) {
    // Classical analytical solution: V_C(t) = V_s * (1 - e^(-t/τ))
    let v_analytical = v_source * (1.0 - (-time / tau).exp());
    
    // Transmission line model behavior
    let v_tl_model = if time < tl_delay {
        // Before first wave arrives: capacitor voltage is 0
        0.0
    } else {
        // After first wave arrives: voltage divider with transmission line effects
        // The wave carries: V_incident = V_source * Z_load / (Z_source + Z_load)
        let z_source = r_internal;
        let z_load = r_value; // Simplified: resistor impedance
        let v_incident = v_source * z_load / (z_source + z_load);
        
        // For now, simplified to instantaneous charging after wave arrival
        // In reality, there would be multiple reflections and complex behavior
        v_incident
    };
    
    // Step response (ideal instantaneous response)
    let v_step = if time == 0.0 { 0.0 } else { v_source };
    
    // Calculate error percentage
    let error_percent = if v_analytical != 0.0 {
        ((v_tl_model - v_analytical) / v_analytical * 100.0).abs()
    } else if v_tl_model != 0.0 {
        f64::INFINITY
    } else {
        0.0
    };
    
    // Determine phase
    let phase = if time < tl_delay {
        "pre-wave"
    } else if time < 10.0 * tl_delay {
        "wave-arrival"
    } else if time < tau {
        "early-settling"
    } else if time < 5.0 * tau {
        "exponential-decay"
    } else {
        "steady-state"
    };
    
    // Write to CSV
    writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.2},{}", 
             time * 1e12, v_analytical, v_tl_model, v_step, error_percent, phase).expect("Could not write data");
    
    // Print analysis for key time points
    if time == 0.0 || time == 50e-12 || time == 100e-12 || time == 150e-12 || 
       time == 500e-12 || time == 1e-9 || time == 1e-6 {
        println!("\nTime t = {:.0}ps ({:.3}×τ):", time * 1e12, time / tau);
        println!("  Analytical:    {:.3}V ({:.1}%)", v_analytical, v_analytical / v_source * 100.0);
        println!("  TL Model:      {:.3}V ({:.1}%)", v_tl_model, v_tl_model / v_source * 100.0);
        println!("  Error:         {:.2}%", error_percent);
        println!("  Phase:         {}", phase);
        
        if time == 100e-12 {
            println!("  >>> CRITICAL: First wave arrives! TL model shows discontinuous jump");
            println!("  >>> Classical model shows smooth exponential curve");
        }
        
        if error_percent > 50.0 && error_percent < f64::INFINITY {
            println!("  ⚠️  Large error - fundamental difference in physics");
        }
    }
}

#[allow(dead_code)]
fn analyze_wave_physics() {
    println!("\n=== Wave Physics Analysis ===");
    
    // In transmission line model:
    // 1. Voltage source launches waves with finite speed
    // 2. Waves have characteristic impedance Z₀ 
    // 3. Reflections occur at impedance discontinuities
    // 4. Multiple round trips create complex behavior
    
    println!("Key differences from lumped circuit:");
    println!("1. Finite propagation speed:");
    println!("   - Classical: Instantaneous field coupling");
    println!("   - TL model: Wave propagation delays (100ps resistor, 10ps capacitor)");
    
    println!("2. Impedance matching:");
    println!("   - Classical: Pure resistance and reactance");
    println!("   - TL model: Characteristic impedance Z₀ and reflection coefficients");
    
    println!("3. Energy storage mechanism:");
    println!("   - Classical: Electric field energy ½CV²");
    println!("   - TL model: Electromagnetic wave energy propagating through medium");
    
    println!("4. Frequency response:");
    println!("   - Classical: Single pole at ω = 1/RC");
    println!("   - TL model: Multiple poles/zeros due to wave reflections");
    
    let z0_resistor = 1000.0_f64;
    let z0_capacitor = 10.0_f64;
    let reflection_coeff = (z0_capacitor - z0_resistor) / (z0_capacitor + z0_resistor);
    
    println!("\nReflection analysis:");
    println!("  Z₀_resistor = {:.0}Ω", z0_resistor);
    println!("  Z₀_capacitor = {:.0}Ω", z0_capacitor);
    println!("  Γ = (Z_L - Z₀)/(Z_L + Z₀) = {:.3}", reflection_coeff);
    println!("  >>> {:.1}% of wave energy reflects back", reflection_coeff.abs() * 100.0);
}