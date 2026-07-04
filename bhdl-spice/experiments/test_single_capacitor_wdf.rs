/// Test Single Capacitor with Wave Digital Filter Theory
/// 
/// This tests the fundamental wave digital capacitor model

use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Single Capacitor Wave Digital Test ===\n");
    
    // Test parameters
    let c = 100e-6;  // 100 µF
    let dt = 1e-6;   // 1 µs timestep
    let v_initial = 0.0;  // Initial voltage
    let i_constant = 0.1; // 100mA constant current
    
    // Wave digital port resistance
    let rp = dt / (2.0 * c);
    
    println!("Capacitor: {} µF", c * 1e6);
    println!("Time step: {} µs", dt * 1e6);
    println!("Port resistance: {:.3} Ω", rp);
    println!("Current source: {} mA", i_constant * 1000.0);
    println!("Expected dV/dt = I/C = {} V/s\n", i_constant / c);
    
    // State variable (capacitor voltage)
    let mut vc = v_initial;
    
    // Simulation
    let duration = 10e-3;  // 10ms
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/single_capacitor_wdf.csv").unwrap();
    writeln!(file, "time_ms,vc,vc_exact,error_%,method").unwrap();
    
    // Method 1: Direct integration (reference)
    let mut vc_euler = v_initial;
    
    // Method 2: Wave Digital Filter
    let mut state_wdf = v_initial;
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Exact solution: V = V0 + I*t/C
        let vc_exact = v_initial + i_constant * time / c;
        
        // Method 1: Euler integration
        vc_euler += i_constant / c * dt;
        
        // Method 2: Wave Digital Filter
        // For a current source feeding a capacitor:
        // The incident wave is: a = I * Rp
        let a = i_constant * rp;
        
        // Capacitor scattering: b = 2*V - a
        let b = 2.0 * state_wdf - a;
        
        // Update voltage: V_new = (a + b) / 2
        state_wdf = (a + b) / 2.0;
        
        // Log every 100 steps
        if i % 100 == 0 {
            let error_euler = ((vc_euler - vc_exact) / vc_exact * 100.0).abs();
            let error_wdf = ((state_wdf - vc_exact) / vc_exact * 100.0).abs();
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},euler", 
                     time * 1000.0, vc_euler, vc_exact, error_euler).unwrap();
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},wdf", 
                     time * 1000.0, state_wdf, vc_exact, error_wdf).unwrap();
        }
    }
    
    println!("Final voltages:");
    println!("  Euler: {:.3} V", vc_euler);
    println!("  WDF: {:.3} V", state_wdf);
    println!("  Exact: {:.3} V", v_initial + i_constant * duration / c);
    
    println!("\nResults saved to: tests/outputs/single_capacitor_wdf.csv");
    
    // Now test with voltage step through resistor
    println!("\n\n=== Voltage Step Through Resistor ===\n");
    
    let v_step = 5.0;
    let r = 50.0;
    
    println!("Step voltage: {} V", v_step);
    println!("Series resistor: {} Ω", r);
    println!("Time constant τ = RC = {:.1} ms", r * c * 1000.0);
    
    // Reset states
    vc_euler = 0.0;
    state_wdf = 0.0;
    
    let mut file2 = File::create("tests/outputs/rc_step_wdf.csv").unwrap();
    writeln!(file2, "time_ms,vc_euler,vc_wdf,vc_exact,error_euler_%,error_wdf_%").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Exact solution for RC step response
        let tau = r * c;
        let vc_exact = v_step * (1.0 - (-time / tau).exp());
        
        // Method 1: Euler integration
        let i_euler = (v_step - vc_euler) / r;
        vc_euler += i_euler / c * dt;
        
        // Method 2: Wave Digital Filter approach
        // Current flowing = (V_step - V_cap) / R_total
        let i_wdf = (v_step - state_wdf) / (r + rp);
        
        // Incident wave at capacitor
        let a = i_wdf * rp;
        
        // Reflected wave
        let b = 2.0 * state_wdf - a;
        
        // Update voltage
        state_wdf = (a + b) / 2.0;
        
        if i % 100 == 0 {
            let error_euler = if vc_exact > 0.01 {
                ((vc_euler - vc_exact) / vc_exact * 100.0).abs()
            } else { 0.0 };
            
            let error_wdf = if vc_exact > 0.01 {
                ((state_wdf - vc_exact) / vc_exact * 100.0).abs()
            } else { 0.0 };
            
            writeln!(file2, "{:.3},{:.6},{:.6},{:.6},{:.2},{:.2}",
                     time * 1000.0, vc_euler, state_wdf, vc_exact, 
                     error_euler, error_wdf).unwrap();
        }
    }
    
    let tau = r * c;
    println!("\nRC Step Response Final Values:");
    println!("  Euler: {:.3} V", vc_euler);
    println!("  WDF: {:.3} V", state_wdf);
    println!("  Exact: {:.3} V", v_step * (1.0 - (-duration / tau).exp()));
    
    println!("\nKEY INSIGHTS:");
    println!("1. For capacitor: b = 2V - a (reflection coefficient = +1)");
    println!("2. Voltage update: V = (a + b) / 2");
    println!("3. This is equivalent to trapezoidal integration");
    println!("4. The port resistance Rp = Δt/(2C) comes from bilinear transform");
}