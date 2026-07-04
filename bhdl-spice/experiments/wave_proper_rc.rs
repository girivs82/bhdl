/// Proper Wave Digital RC Circuit
/// 
/// Implementing the correct wave digital filter theory

use std::fs::File;
use std::io::Write;

/// Wave Digital Resistor
struct WDResistor {
    resistance: f64,
    port_impedance: f64,
}

impl WDResistor {
    fn new(r: f64) -> Self {
        Self {
            resistance: r,
            port_impedance: r, // For resistor, Rp = R is optimal
        }
    }
    
    /// Scatter: b1 = 0, b2 = a1 (for matched impedance)
    fn scatter(&self, a1: f64, a2: f64) -> (f64, f64) {
        // For resistor with port impedance = resistance
        // Acts as perfect absorber on port 1
        let b1 = 0.0;
        let b2 = a1;
        (b1, b2)
    }
}

/// Wave Digital Capacitor
struct WDCapacitor {
    capacitance: f64,
    port_impedance: f64,
    
    // State variable (key!)
    prev_port_voltage: f64,
}

impl WDCapacitor {
    fn new(c: f64, dt: f64) -> Self {
        Self {
            capacitance: c,
            port_impedance: dt / (2.0 * c),
            prev_port_voltage: 0.0,
        }
    }
    
    /// One-port capacitor to ground
    fn scatter_one_port(&mut self, a: f64) -> f64 {
        // Wave digital one-port capacitor:
        // b(n) = v(n-1) - a(n)
        // v(n) = a(n) + b(n) = v(n-1)
        
        let b = self.prev_port_voltage - a;
        let v = a + b;
        self.prev_port_voltage = v;
        
        b
    }
}

/// Series adaptor (connects resistor to capacitor)
struct SeriesAdaptor {
    port1_impedance: f64,
    port2_impedance: f64,
}

impl SeriesAdaptor {
    fn new(z1: f64, z2: f64) -> Self {
        Self {
            port1_impedance: z1,
            port2_impedance: z2,
        }
    }
    
    /// Scatter for series connection
    fn scatter(&self, a1: f64, a2: f64) -> (f64, f64) {
        // Series scattering equations
        let p = 2.0 * self.port1_impedance / (self.port1_impedance + self.port2_impedance);
        let q = 2.0 * self.port2_impedance / (self.port1_impedance + self.port2_impedance);
        
        let b1 = a2 + (p - 1.0) * a1;
        let b2 = a1 + (q - 1.0) * a2;
        
        (b1, b2)
    }
}

fn main() {
    println!("=== Proper Wave Digital RC Circuit ===\n");
    
    // Circuit parameters
    let v_source = 5.0;
    let r = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    println!("Circuit: {}V -> {}Ω -> {}µF -> GND", v_source, r, c * 1e6);
    
    // Create components
    let resistor = WDResistor::new(r);
    let mut capacitor = WDCapacitor::new(c, dt);
    
    println!("\nComponent impedances:");
    println!("  Resistor: {} Ω", resistor.port_impedance);
    println!("  Capacitor: {:.3} Ω", capacitor.port_impedance);
    
    // Create series adaptor
    let adaptor = SeriesAdaptor::new(resistor.port_impedance, capacitor.port_impedance);
    
    // Simulation
    let tau = r * c;
    println!("\nTime constant τ = {:.1} ms", tau * 1000.0);
    
    let duration = 25e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wave_proper_rc.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,vc_exact,error_%,i_mA").unwrap();
    
    // Wave state
    let mut a_r1 = 0.0;  // Wave into resistor port 1
    let mut b_r1 = 0.0;  // Wave out of resistor port 1
    let mut a_r2 = 0.0;  // Wave into resistor port 2
    let mut b_r2 = 0.0;  // Wave out of resistor port 2
    let mut a_c = 0.0;   // Wave into capacitor
    let mut b_c = 0.0;   // Wave out of capacitor
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Source creates incident wave into resistor
        a_r1 = v_source / 2.0;
        
        // Resistor scattering
        (b_r1, b_r2) = resistor.scatter(a_r1, a_r2);
        
        // Series adaptor connects resistor port 2 to capacitor
        a_c = b_r2;  // What comes out of resistor goes into capacitor
        b_c = capacitor.scatter_one_port(a_c);
        a_r2 = b_c;  // What reflects from capacitor goes back to resistor
        
        // Apply series adaptor scattering
        let (b_adapt1, b_adapt2) = adaptor.scatter(b_r2, b_c);
        a_r2 = b_adapt1;
        a_c = b_adapt2;
        
        // Calculate capacitor voltage
        let vc = capacitor.prev_port_voltage;
        
        // Calculate current
        let i_circuit = (a_c - b_c) / capacitor.port_impedance;
        
        // Exact solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.3}",
                     time * 1000.0, vc, vc_exact, error, i_circuit * 1000.0).unwrap();
        }
    }
    
    let vc_final = capacitor.prev_port_voltage;
    let vc_expected = v_source * (1.0 - (-duration / tau).exp());
    
    println!("\nFinal results:");
    println!("  Wave model: {:.3} V", vc_final);
    println!("  Expected: {:.3} V", vc_expected);
    println!("  Error: {:.1}%", ((vc_final - vc_expected) / vc_expected * 100.0).abs());
    
    println!("\nResults saved to: tests/outputs/wave_proper_rc.csv");
    
    println!("\nKEY INSIGHTS:");
    println!("1. Wave digital capacitor: b(n) = v(n-1) - a(n)");
    println!("2. This naturally implements capacitor memory");
    println!("3. Series adaptor properly connects components");
    println!("4. No matrices, just local scattering!");
}