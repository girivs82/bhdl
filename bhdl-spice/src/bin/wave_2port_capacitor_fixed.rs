/// Wave 2-Port Model: Single Capacitor (Fixed)
/// 
/// Corrected physics implementation

use std::fs::File;
use std::io::Write;

/// 2-Port Capacitor with proper wave scattering
struct WaveCapacitor {
    capacitance: f64,
    voltage: f64,  // State variable
    port_impedance: f64,
    dt: f64,
}

impl WaveCapacitor {
    fn new(capacitance: f64, dt: f64) -> Self {
        // Port impedance for capacitor: Rp = Δt/(2C)
        let rp = dt / (2.0 * capacitance);
        
        Self {
            capacitance,
            voltage: 0.0,
            port_impedance: rp,
            dt,
        }
    }
    
    /// Apply waves and get capacitor response
    fn apply_wave(&mut self, v_incident: f64) -> f64 {
        // For a grounded capacitor with incident wave at port 1:
        // The reflected wave depends on the stored voltage
        
        // The key insight: Wave Digital Filter theory says:
        // For capacitor: b = -a + 2*V_c
        // where V_c is the capacitor voltage (state)
        
        let v_reflected = -v_incident + 2.0 * self.voltage;
        
        // Port voltage and current
        let v_port = v_incident + v_reflected;
        let i_port = (v_incident - v_reflected) / self.port_impedance;
        
        // Update capacitor voltage
        // Using trapezoidal integration
        self.voltage += i_port * self.dt / self.capacitance;
        
        v_reflected
    }
    
    fn get_voltage(&self) -> f64 {
        self.voltage
    }
    
    fn get_current(&self, v_incident: f64, v_reflected: f64) -> f64 {
        (v_incident - v_reflected) / self.port_impedance
    }
}

/// Source with series resistance feeding capacitor
struct SourceWithResistor {
    voltage: f64,
    resistance: f64,
    port_impedance: f64,
}

impl SourceWithResistor {
    fn new(voltage: f64, resistance: f64) -> Self {
        Self {
            voltage,
            resistance,
            port_impedance: resistance,
        }
    }
    
    /// Get wave from source
    fn get_wave(&self) -> f64 {
        // Thévenin equivalent creates wave
        self.voltage / 2.0
    }
}

fn main() {
    println!("=== Wave 2-Port Model: Single Capacitor (Fixed) ===\n");
    
    // Circuit parameters
    let v_source = 5.0;
    let r_source = 50.0;
    let c = 100e-6;
    let dt = 1e-6;
    
    println!("Circuit: {}V -> {}Ω -> {}µF -> GND", v_source, r_source, c * 1e6);
    
    // Create components
    let source = SourceWithResistor::new(v_source, r_source);
    let mut cap = WaveCapacitor::new(c, dt);
    
    println!("\nWave impedances:");
    println!("  Source impedance: {} Ω", source.port_impedance);
    println!("  Capacitor port impedance: {:.3} Ω", cap.port_impedance);
    println!("  Impedance ratio: {:.1}", source.port_impedance / cap.port_impedance);
    
    // This large impedance mismatch will cause reflections!
    
    // Traditional time constant
    let tau = r_source * c;
    println!("\nTraditional τ = RC = {:.1} ms", tau * 1000.0);
    
    // Simulation
    let mut file = File::create("tests/outputs/wave_2port_capacitor_fixed.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,ic_wave_mA,vc_traditional,ic_traditional_mA,error_%").unwrap();
    
    let duration = 20e-3;  // 20 ms
    let steps = (duration / dt) as usize;
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Wave from source
        let v_incident_source = source.get_wave();
        
        // Account for impedance mismatch at connection
        // This is key! The wave sees impedance division
        let z_total = source.port_impedance + cap.port_impedance;
        let v_incident_cap = v_incident_source * (2.0 * cap.port_impedance) / z_total;
        
        // Apply to capacitor
        let v_reflected_cap = cap.apply_wave(v_incident_cap);
        
        // Get capacitor state
        let vc_wave = cap.get_voltage();
        let ic_wave = cap.get_current(v_incident_cap, v_reflected_cap);
        
        // Traditional solution
        let vc_trad = v_source * (1.0 - (-time / tau).exp());
        let ic_trad = (v_source / r_source) * (-time / tau).exp();
        
        // Calculate error
        let error = if vc_trad > 0.01 {
            ((vc_wave - vc_trad) / vc_trad * 100.0).abs()
        } else {
            0.0
        };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.3},{:.6},{:.3},{:.2}",
                     time * 1000.0,
                     vc_wave,
                     ic_wave * 1000.0,
                     vc_trad,
                     ic_trad * 1000.0,
                     error).unwrap();
        }
    }
    
    println!("\nResults saved to: tests/outputs/wave_2port_capacitor_fixed.csv");
    
    // Final values
    let vc_final = cap.get_voltage();
    let vc_expected = v_source * (1.0 - (-duration / tau).exp());
    
    println!("\nFinal values:");
    println!("  Wave model: {:.3} V", vc_final);
    println!("  Traditional: {:.3} V", vc_expected);
    println!("  Error: {:.1}%", ((vc_final - vc_expected) / vc_expected * 100.0).abs());
    
    println!("\nPHYSICS INSIGHTS:");
    println!("1. Capacitor reflects waves: b = -a + 2*Vc");
    println!("2. Large impedance mismatch ({}Ω vs {:.3}Ω) causes reflections", 
             r_source, cap.port_impedance);
    println!("3. Wave amplitude adjusted by impedance division");
    println!("4. Capacitor integrates current naturally through wave scattering");
    println!("5. No matrix equations - just local wave physics!");
}