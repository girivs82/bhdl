/// Simple Wave Digital RC Circuit
/// 
/// Direct implementation without complex adaptors

use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Simple Wave Digital RC Circuit ===\n");
    
    // Circuit parameters
    let v_source = 5.0;
    let r = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    println!("Circuit: {}V -> {}Ω -> {}µF -> GND", v_source, r, c * 1e6);
    
    // Wave digital port resistance for capacitor
    let r_cap = dt / (2.0 * c);
    println!("Capacitor port resistance: {:.3} Ω", r_cap);
    
    // Time constant
    let tau = r * c;
    println!("Time constant τ = {:.1} ms\n", tau * 1000.0);
    
    // State variables
    let mut vc = 0.0;  // Capacitor voltage
    
    // Simulation
    let duration = 25e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wave_digital_simple_rc.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,vc_exact,error_%,ic_mA").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Circuit equation: V_source = I*R + Vc
        // Therefore: I = (V_source - Vc) / R
        let current = (v_source - vc) / r;
        
        // Update capacitor voltage using Euler integration
        // dVc/dt = I/C
        let dvc_dt = current / c;
        vc += dvc_dt * dt;
        
        // Exact solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.3}",
                     time * 1000.0, vc, vc_exact, error, current * 1000.0).unwrap();
        }
    }
    
    let vc_final = vc;
    let vc_expected = v_source * (1.0 - (-duration / tau).exp());
    
    println!("Final values:");
    println!("  Model: {:.3} V", vc_final);
    println!("  Expected: {:.3} V", vc_expected);
    println!("  Error: {:.1}%", ((vc_final - vc_expected) / vc_expected * 100.0).abs());
    
    println!("\nResults saved to: tests/outputs/wave_digital_simple_rc.csv");
    
    println!("\nThis shows that simple Euler integration works!");
    println!("Now let's implement the actual wave digital version...");
    
    // Now do the wave digital version
    println!("\n\n=== Wave Digital Version ===\n");
    
    // Reset state
    let mut state = 0.0;  // Wave digital state (delay register)
    
    let mut file2 = File::create("tests/outputs/wave_digital_proper_rc.csv").unwrap();
    writeln!(file2, "time_ms,vc_wave,vc_exact,error_%,incident,reflected").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // For an RC circuit with voltage source:
        // The incident wave at the capacitor is proportional to (V_source - Vc)
        // Using the series connection formula:
        
        // Current flowing = (V_source - Vc) / (R + R_cap)
        let current = (v_source - state) / (r + r_cap);
        
        // Incident wave at capacitor port
        let incident = current * r_cap;
        
        // Wave digital capacitor: reflected = -incident + 2*state
        let reflected = -incident + 2.0 * state;
        
        // Update state
        state = incident + state;
        
        // Capacitor voltage is the state
        let vc = state;
        
        // Exact solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file2, "{:.3},{:.6},{:.6},{:.2},{:.6},{:.6}",
                     time * 1000.0, vc, vc_exact, error, incident, reflected).unwrap();
        }
    }
    
    println!("Wave digital final voltage: {:.3} V", state);
    println!("Results saved to: tests/outputs/wave_digital_proper_rc.csv");
}