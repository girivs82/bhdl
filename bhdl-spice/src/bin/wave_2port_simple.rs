/// Simple 2-Port Wave Model - Start from First Principles
/// 
/// Let's understand exactly how waves work

use std::fs::File;
use std::io::Write;

/// First, let's model just a capacitor connected to a constant current source
fn test_capacitor_current_source() {
    println!("=== Test 1: Capacitor with Current Source ===\n");
    
    let c = 100e-6;  // 100 µF
    let i_source = 0.1;  // 100 mA
    let dt = 1e-6;  // 1 µs
    
    // Wave port impedance for capacitor
    let rp = dt / (2.0 * c);
    println!("Capacitor: {} µF", c * 1e6);
    println!("Port impedance: Rp = {:.3} Ω", rp);
    println!("Current source: {} mA\n", i_source * 1000.0);
    
    // For a current source, the incident wave is:
    // a = I * Rp / 2
    let a_incident = i_source * rp / 2.0;
    
    // Capacitor state
    let mut vc = 0.0;
    
    // Simulate
    let duration = 10e-3;  // 10 ms
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wave_cap_isource.csv").unwrap();
    writeln!(file, "time_ms,vc,vc_exact").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Capacitor reflects based on its voltage
        // For one-port capacitor: b = 2*Vc - a
        let b_reflected = 2.0 * vc - a_incident;
        
        // Port voltage and current
        let v_port = a_incident + b_reflected;
        let i_port = (a_incident - b_reflected) / rp;
        
        // Update capacitor voltage
        vc = v_port;
        
        // Exact solution: V = I*t/C
        let vc_exact = i_source * time / c;
        
        if i % 1000 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6}", 
                     time * 1000.0, vc, vc_exact).unwrap();
        }
    }
    
    println!("Final voltage: {:.3} V (expected: {:.3} V)", 
             vc, i_source * duration / c);
}

/// Now test RC circuit with proper wave implementation
fn test_rc_proper() {
    println!("\n\n=== Test 2: Proper RC Circuit ===\n");
    
    let v_source = 5.0;
    let r = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    // Component impedances
    let r_source = 0.01;  // Small source impedance
    let rp_cap = dt / (2.0 * c);
    
    println!("Circuit: {}V -> {}Ω -> {}µF", v_source, r, c * 1e6);
    println!("Capacitor Rp = {:.3} Ω\n", rp_cap);
    
    // State
    let mut vc = 0.0;
    
    // Output
    let tau = r * c;
    let duration = 20e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wave_2port_simple.csv").unwrap();
    writeln!(file, "time_ms,vc,vc_exact,error_%,ic_mA").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Step 1: Source creates Thévenin equivalent
        // V_th = V_source, R_th = R + R_source ≈ R
        
        // Step 2: Calculate steady-state current
        // In steady state: I = (V_source - Vc) / R
        let i_steady = (v_source - vc) / r;
        
        // Step 3: This current creates incident wave at capacitor
        // For current into port: a = I * Rp / 2
        let a_cap = i_steady * rp_cap / 2.0;
        
        // Step 4: Capacitor reflects based on voltage
        let b_cap = 2.0 * vc - a_cap;
        
        // Step 5: Update capacitor voltage
        vc = a_cap + b_cap;
        
        // Actual current (for verification)
        let i_actual = (a_cap - b_cap) / rp_cap;
        
        // Exact solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.3}",
                     time * 1000.0, vc, vc_exact, error, i_actual * 1000.0).unwrap();
        }
    }
    
    println!("Final voltage: {:.3} V (expected: {:.3} V)",
             vc, v_source * (1.0 - (-duration / tau).exp()));
    
    println!("\nResults saved to CSV files");
}

fn main() {
    println!("=== Simple 2-Port Wave Model ===\n");
    
    println!("Starting from first principles...\n");
    
    test_capacitor_current_source();
    test_rc_proper();
    
    println!("\n\nKEY UNDERSTANDING:");
    println!("1. For capacitor: b = 2*Vc - a");
    println!("2. Port voltage: V = a + b");
    println!("3. Port current: I = (a - b) / Rp");
    println!("4. The circuit dynamics come from the feedback loop:");
    println!("   - Current depends on (V_source - Vc)");
    println!("   - This creates incident wave");
    println!("   - Capacitor reflects based on Vc");
    println!("   - New Vc = a + b");
    println!("5. This is exactly the same as I = C*dV/dt!");
}