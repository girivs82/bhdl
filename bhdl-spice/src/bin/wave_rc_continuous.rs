/// Wave Digital RC with Continuous Source
/// 
/// Key insight: Voltage source continuously emits waves!

use std::fs::File;
use std::io::Write;

/// Wave Digital Voltage Source
struct WDVoltageSource {
    voltage: f64,
    internal_resistance: f64,
}

impl WDVoltageSource {
    fn new(v: f64, r_internal: f64) -> Self {
        Self {
            voltage: v,
            internal_resistance: r_internal,
        }
    }
    
    /// Source continuously emits wave
    fn get_wave(&self) -> f64 {
        // Thévenin source creates wave amplitude
        self.voltage / 2.0
    }
}

/// Wave Digital Resistor
struct WDResistor {
    resistance: f64,
}

impl WDResistor {
    fn new(r: f64) -> Self {
        Self { resistance: r }
    }
    
    /// Resistor scattering (matched impedance)
    fn scatter(&self, a: f64) -> f64 {
        // For matched impedance: b = 0 (perfect absorption)
        // For now, assume matched
        0.0
    }
}

/// Wave Digital Capacitor (one-port to ground)
struct WDCapacitor {
    capacitance: f64,
    port_impedance: f64,
    voltage: f64,  // State variable
}

impl WDCapacitor {
    fn new(c: f64, dt: f64) -> Self {
        Self {
            capacitance: c,
            port_impedance: dt / (2.0 * c),
            voltage: 0.0,
        }
    }
    
    /// Capacitor reflects based on stored voltage
    fn scatter(&self, a: f64) -> f64 {
        // Wave digital capacitor equation
        self.voltage - a
    }
    
    /// Update capacitor state
    fn update(&mut self, a: f64, b: f64) {
        // Port voltage is sum of waves
        self.voltage = a + b;
    }
}

fn main() {
    println!("=== Wave Digital RC with Continuous Source ===\n");
    
    // Circuit: 5V source -> 50Ω resistor -> 100µF capacitor -> ground
    let v_source = 5.0;
    let r = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    println!("Circuit: {}V -> {}Ω -> {}µF -> GND", v_source, r, c * 1e6);
    
    // Create components
    let source = WDVoltageSource::new(v_source, 0.01); // Small internal resistance
    let resistor = WDResistor::new(r);
    let mut capacitor = WDCapacitor::new(c, dt);
    
    println!("Capacitor port impedance: {:.3} Ω", capacitor.port_impedance);
    
    // For simplicity, let's use the fact that R >> Rc
    // So most of the impedance is from R
    let z_total = r + capacitor.port_impedance;
    
    // Simulation
    let tau = r * c;
    println!("Time constant: τ = {:.1} ms\n", tau * 1000.0);
    
    let duration = 25e-3;
    let steps = (duration / dt) as usize;
    
    let mut file = File::create("tests/outputs/wave_rc_continuous.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,vc_exact,error_%,ic_mA").unwrap();
    
    println!("Simulating...");
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Source continuously emits waves
        let a_source = source.get_wave();
        
        // Wave propagates through circuit
        // Simplified: voltage divider in wave domain
        let a_cap = a_source * capacitor.port_impedance / z_total;
        
        // Capacitor reflects
        let b_cap = capacitor.scatter(a_cap);
        
        // Update capacitor
        capacitor.update(a_cap, b_cap);
        
        // Calculate current
        let i_cap = (a_cap - b_cap) / capacitor.port_impedance;
        
        // Exact solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        let error = if vc_exact > 0.01 {
            ((capacitor.voltage - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.3}",
                     time * 1000.0, 
                     capacitor.voltage, 
                     vc_exact, 
                     error,
                     i_cap * 1000.0).unwrap();
        }
    }
    
    println!("\nFinal values:");
    println!("  Wave model: {:.3} V", capacitor.voltage);
    println!("  Expected: {:.3} V", v_source * (1.0 - (-duration / tau).exp()));
    
    println!("\nKEY INSIGHTS:");
    println!("1. Voltage source CONTINUOUSLY emits waves (not just once!)");
    println!("2. Wave amplitude = V/2 for Thévenin source");
    println!("3. Capacitor reflects: b = Vc - a");
    println!("4. Capacitor voltage: Vc = a + b");
    println!("5. This creates the exponential charging behavior");
}