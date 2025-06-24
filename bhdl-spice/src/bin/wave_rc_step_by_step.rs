/// Step-by-Step Wave Model: RC Circuit
/// 
/// Build understanding by carefully implementing each component

use std::fs::File;
use std::io::Write;

/// First, let's understand a wave digital resistor
fn test_resistor() {
    println!("=== Test 1: Wave Digital Resistor ===\n");
    
    // A resistor with wave port impedance Rp
    let r = 50.0;  // Actual resistance
    let rp = 50.0; // Port impedance (matched!)
    
    // Apply incident wave
    let a_incident = 2.5;  // 5V source creates 2.5V wave
    
    // Scattering: For resistor with matched impedance
    // Reflection coefficient Γ = (R - Rp)/(R + Rp) = 0
    let gamma = (r - rp) / (r + rp);
    let b_reflected = gamma * a_incident;
    
    println!("Resistor R = {} Ω, Port impedance Rp = {} Ω", r, rp);
    println!("Incident wave: a = {} V", a_incident);
    println!("Reflection coefficient: Γ = {}", gamma);
    println!("Reflected wave: b = {} V", b_reflected);
    
    // Port voltage and current
    let v_port = a_incident + b_reflected;
    let i_port = (a_incident - b_reflected) / rp;
    
    println!("Port voltage: V = a + b = {} V", v_port);
    println!("Port current: I = (a - b)/Rp = {} A", i_port);
    println!("Ohm's law check: V/I = {} Ω ✓", v_port / i_port);
}

/// Now let's properly implement a wave digital capacitor
struct WaveCapacitor {
    capacitance: f64,
    port_impedance: f64,
    
    // State (key for capacitor!)
    voltage: f64,
}

impl WaveCapacitor {
    fn new(c: f64, dt: f64) -> Self {
        // Bilinear transform gives Rp = Δt/(2C)
        let rp = dt / (2.0 * c);
        
        Self {
            capacitance: c,
            port_impedance: rp,
            voltage: 0.0,
        }
    }
    
    /// Scatter incident wave
    fn scatter(&mut self, a: f64) -> f64 {
        // Wave digital capacitor formula:
        // b = 2*Vc - a
        // where Vc is the stored voltage
        
        2.0 * self.voltage - a
    }
    
    /// Update state based on port current
    fn update_voltage(&mut self, a: f64, b: f64, dt: f64) {
        // Port current
        let i = (a - b) / self.port_impedance;
        
        // Integrate: Vc = Vc + I*dt/C
        self.voltage += i * dt / self.capacitance;
    }
}

/// Series adaptor connects two wave ports
struct SeriesAdaptor {
    z1: f64,  // Port 1 impedance
    z2: f64,  // Port 2 impedance
}

impl SeriesAdaptor {
    fn new(z1: f64, z2: f64) -> Self {
        Self { z1, z2 }
    }
    
    /// Scatter waves at series connection
    fn scatter(&self, a1: f64, a2: f64) -> (f64, f64) {
        // Series adaptor enforces:
        // 1. Currents are equal: i1 = i2
        // 2. Voltages add: v1 + v2 = v_total
        
        // Total impedance
        let z_total = self.z1 + self.z2;
        
        // Common current (from incident waves)
        let i_common = (a1 + a2) / z_total;
        
        // Reflected waves
        let b1 = self.z1 * i_common - a1;
        let b2 = self.z2 * i_common - a2;
        
        (b1, b2)
    }
}

fn test_rc_circuit() {
    println!("\n\n=== Test 2: Complete RC Circuit ===\n");
    
    // Circuit: 5V source -> 50Ω resistor -> 100µF capacitor -> ground
    let v_source = 5.0;
    let r = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    println!("Circuit: {}V -> {}Ω -> {}µF -> GND", v_source, r, c * 1e6);
    
    // Create wave digital capacitor
    let mut cap = WaveCapacitor::new(c, dt);
    println!("Capacitor port impedance: {:.3} Ω", cap.port_impedance);
    
    // The source + resistor can be modeled as Thévenin equivalent
    // with impedance R and creating incident wave
    let source_impedance = r;
    
    // Series adaptor connects source/R to capacitor
    let adaptor = SeriesAdaptor::new(source_impedance, cap.port_impedance);
    
    // Output file
    let mut file = File::create("tests/outputs/wave_rc_step_by_step.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,vc_exact,error_%,a1,b1,a2,b2").unwrap();
    
    // Simulation
    let tau = r * c;
    let duration = 20e-3;
    let steps = (duration / dt) as usize;
    
    println!("\nSimulating for {} ms ({} time constants)...", 
             duration * 1000.0, duration / tau);
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Source creates incident wave
        let a1 = v_source / 2.0;  // Thévenin source wave
        
        // Capacitor's incident wave (initially 0)
        let a2 = cap.scatter(0.0);  // Previous reflected becomes incident
        
        // Scatter at series connection
        let (b1, b2) = adaptor.scatter(a1, a2);
        
        // Update capacitor state
        let b2_reflected = cap.scatter(b2);
        cap.update_voltage(b2, b2_reflected, dt);
        
        // Traditional solution
        let vc_exact = v_source * (1.0 - (-time / tau).exp());
        
        // Error
        let error = if vc_exact > 0.01 {
            ((cap.voltage - vc_exact) / vc_exact * 100.0).abs()
        } else {
            0.0
        };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2},{:.3},{:.3},{:.3},{:.3}",
                     time * 1000.0, cap.voltage, vc_exact, error,
                     a1, b1, a2, b2).unwrap();
        }
    }
    
    println!("\nFinal capacitor voltage:");
    println!("  Wave model: {:.3} V", cap.voltage);
    println!("  Exact: {:.3} V", v_source * (1.0 - (-duration / tau).exp()));
    
    println!("\nResults saved to: tests/outputs/wave_rc_step_by_step.csv");
}

fn main() {
    println!("=== Step-by-Step Wave Digital Model ===\n");
    
    test_resistor();
    test_rc_circuit();
    
    println!("\n\nKEY LEARNINGS:");
    println!("1. Each component has a port impedance Rp");
    println!("2. Components scatter waves: b = f(a, state)");
    println!("3. Series adaptor enforces i1 = i2");
    println!("4. Capacitor reflects based on stored voltage");
    println!("5. System naturally handles impedance mismatch");
    println!("\nNext: Add inductor to create RLC circuit");
}