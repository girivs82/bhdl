/// Correct Wave Digital Capacitor Model
/// 
/// Based on proper WDF theory

use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Correct Wave Digital Capacitor Model ===\n");
    
    // Test 1: Capacitor with current source
    test_current_source();
    
    // Test 2: RC circuit
    test_rc_circuit();
}

fn test_current_source() {
    println!("Test 1: Capacitor with Current Source\n");
    
    let c = 100e-6;  // 100 µF
    let dt = 1e-6;   // 1 µs
    let i_source = 0.1; // 100 mA
    
    // Port resistance from bilinear transform
    let rp = dt / (2.0 * c);
    
    println!("Capacitor: {} µF", c * 1e6);
    println!("Current: {} mA", i_source * 1000.0);
    println!("Port resistance: {:.3} Ω", rp);
    
    // Wave digital state (initialized to 0)
    let mut z_prev = 0.0;  // Previous state (delay element)
    
    let duration = 10e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wdf_capacitor_isource.csv").unwrap();
    writeln!(file, "time_ms,vc,vc_exact,error_%").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // For current source: incident wave a = I * Rp
        let a = i_source * rp;
        
        // Wave digital capacitor equation
        // b[n] = z[n-1] - a[n]  (reflection)
        let b = z_prev - a;
        
        // Update delay element
        // z[n] = a[n] + z[n-1]
        z_prev = a + z_prev;
        
        // Capacitor voltage = (a + b)
        let vc = a + b;
        
        // Exact solution
        let vc_exact = i_source * time / c;
        
        if i % 1000 == 0 {
            let error = if vc_exact > 0.01 {
                ((vc - vc_exact) / vc_exact * 100.0).abs()
            } else { 0.0 };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.2}", 
                     time * 1000.0, vc, vc_exact, error).unwrap();
        }
    }
    
    println!("Final voltage: {:.3} V (expected: {:.3} V)\n", 
             z_prev, i_source * duration / c);
}

fn test_rc_circuit() {
    println!("Test 2: RC Circuit Step Response\n");
    
    let v_step = 5.0;
    let r = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    // Port resistances
    let rp = dt / (2.0 * c);
    
    println!("Circuit: {}V -> {}Ω -> {}µF", v_step, r, c * 1e6);
    println!("Capacitor port resistance: {:.3} Ω", rp);
    println!("Time constant: {:.1} ms", r * c * 1000.0);
    
    // States
    let mut z_cap = 0.0;  // Capacitor delay element
    
    let duration = 25e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wdf_rc_step.csv").unwrap();
    writeln!(file, "time_ms,vc,vc_exact,error_%,current_mA").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Series connection of source resistance and capacitor
        // Need to use series adaptor
        
        // Port 1: Source with series R
        // Port 2: Capacitor
        
        // Incident wave from source (through R)
        let a1 = v_step / 2.0;  // Voltage source wave
        
        // Reflected wave from capacitor (from previous state)
        let a2 = z_cap;
        
        // Series adaptor equations
        // For series connection: currents equal, voltages add
        let gamma = rp / (r + rp);  // Scattering parameter
        
        // Wave into capacitor port
        let a_cap = (1.0 - gamma) * a1 + gamma * a2;
        
        // Capacitor reflection
        let b_cap = z_cap - a_cap;
        
        // Update capacitor state
        z_cap = a_cap + z_cap;
        
        // Capacitor voltage
        let vc = a_cap + b_cap;
        
        // Current
        let i_circuit = (a_cap - b_cap) / rp;
        
        // Exact solution
        let tau = r * c;
        let vc_exact = v_step * (1.0 - (-time / tau).exp());
        
        if i % 100 == 0 {
            let error = if vc_exact > 0.01 {
                ((vc - vc_exact) / vc_exact * 100.0).abs()
            } else { 0.0 };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.3}", 
                     time * 1000.0, vc, vc_exact, error, i_circuit * 1000.0).unwrap();
        }
    }
    
    let vc_final = z_cap;
    let vc_expected = v_step * (1.0 - (-duration / (r * c)).exp());
    
    println!("\nFinal voltage: {:.3} V", vc_final);
    println!("Expected: {:.3} V", vc_expected);
    println!("Error: {:.1}%", ((vc_final - vc_expected) / vc_expected * 100.0).abs());
    
    println!("\nResults saved to CSV files");
    
    println!("\nKEY EQUATIONS:");
    println!("1. Capacitor: b = z - a (reflection)");
    println!("2. State update: z_new = a + z_old");
    println!("3. Voltage: V = a + b");
    println!("4. Port resistance: Rp = Δt/(2C)");
    println!("5. This implements the bilinear transform exactly!");
}