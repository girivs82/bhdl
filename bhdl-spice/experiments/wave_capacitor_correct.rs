/// Correct Wave Digital Capacitor Implementation
/// 
/// Using the actual WDF equations from literature

use std::fs::File;
use std::io::Write;

/// Wave Digital Capacitor (one-port)
struct WDCapacitor {
    capacitance: f64,
    port_impedance: f64,
    
    // State: previous port voltage
    v_prev: f64,
}

impl WDCapacitor {
    fn new(c: f64, dt: f64) -> Self {
        Self {
            capacitance: c,
            port_impedance: dt / (2.0 * c),
            v_prev: 0.0,
        }
    }
    
    /// Wave digital capacitor equation
    /// b[n] = v[n-1] - a[n]
    /// where v[n-1] is the previous port voltage
    fn scatter(&mut self, a: f64) -> f64 {
        let b = self.v_prev - a;
        
        // Update port voltage for next iteration
        let v_new = a + b;
        self.v_prev = v_new;
        
        b
    }
    
    fn get_voltage(&self) -> f64 {
        self.v_prev
    }
}

/// Test standalone capacitor
fn test_capacitor_alone() {
    println!("=== Test 1: Capacitor Step Response ===\n");
    
    let c = 100e-6;
    let dt = 1e-6;
    let mut cap = WDCapacitor::new(c, dt);
    
    println!("Capacitor: {} µF", c * 1e6);
    println!("Port impedance: {:.3} Ω", cap.port_impedance);
    
    // Apply step through 50Ω resistance
    let v_step = 5.0;
    let r = 50.0;
    
    // Calculate incident wave amplitude
    // For voltage source with series R feeding capacitor:
    // a = V * Rp / (R + Rp)
    let a_steady = v_step * cap.port_impedance / (r + cap.port_impedance);
    
    println!("Step voltage: {} V through {} Ω", v_step, r);
    println!("Incident wave: {:.6} V\n", a_steady);
    
    // Simulate
    let tau = r * c;
    let duration = 5.0 * tau;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wave_capacitor_correct.csv").unwrap();
    writeln!(file, "time_ms,vc,vc_exact,error_%").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Apply incident wave
        let _b = cap.scatter(a_steady);
        
        // Get capacitor voltage
        let vc = cap.get_voltage();
        
        // Exact solution
        let vc_exact = v_step * (1.0 - (-time / tau).exp());
        
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 1000 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2}",
                     time * 1000.0, vc, vc_exact, error).unwrap();
        }
    }
    
    println!("Results: Constant incident wave a = {:.6} V", a_steady);
    println!("Final voltage: {:.3} V", cap.get_voltage());
    println!("(This is wrong! Shows we need the full circuit model)");
}

/// Full RC circuit with series connection
fn test_rc_circuit() {
    println!("\n\n=== Test 2: Complete RC Circuit ===\n");
    
    let v_source = 5.0;
    let r = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    let mut cap = WDCapacitor::new(c, dt);
    
    println!("Circuit: {}V -> {}Ω -> {}µF", v_source, r, c * 1e6);
    
    // The key insight: we need to model the series connection properly
    // Using series adaptor equations
    
    let tau = r * c;
    let duration = 20e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wave_rc_correct.csv").unwrap();
    writeln!(file, "time_ms,vc,vc_exact,error_%,a_cap,b_cap").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Series adaptor between source/resistor and capacitor
        // Port 1: R (impedance = R)
        // Port 2: C (impedance = Rp)
        
        // Incident from source
        let a1 = v_source / 2.0;
        
        // Get reflected from capacitor (from previous iteration)
        let vc_prev = cap.get_voltage();
        let a2 = vc_prev;  // Capacitor's "incident" is its previous voltage
        
        // Series adaptor scattering
        let z1 = r;
        let z2 = cap.port_impedance;
        let p = 2.0 * z1 / (z1 + z2);
        let q = 2.0 * z2 / (z1 + z2);
        
        // Calculate waves into capacitor
        let a_cap = a1 * q / 2.0;  // Simplified for one-way
        
        // Capacitor scattering
        let b_cap = cap.scatter(a_cap);
        
        // Get voltage
        let vc = cap.get_voltage();
        
        // Exact solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.6},{:.6}",
                     time * 1000.0, vc, vc_exact, error, a_cap, b_cap).unwrap();
        }
    }
    
    println!("Final voltage: {:.3} V", cap.get_voltage());
    println!("(Still not right - need proper bidirectional waves!)");
}

fn main() {
    println!("=== Correct Wave Digital Capacitor ===\n");
    
    test_capacitor_alone();
    test_rc_circuit();
    
    println!("\n\nCONCLUSIONS:");
    println!("1. Wave digital capacitor: b[n] = v[n-1] - a[n]");
    println!("2. A constant incident wave gives constant voltage (wrong!)");
    println!("3. We need bidirectional wave propagation");
    println!("4. The series adaptor must properly connect components");
    println!("5. This shows why we need the full 2-port network approach!");
    
    println!("\nNext: Implement proper bidirectional wave propagation");
}