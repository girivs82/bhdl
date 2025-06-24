/// Wave 2-Port Model: Single Capacitor
/// 
/// Understanding the physics of a capacitor as a wave scattering device
/// We'll apply a voltage step and observe the transient response

use std::fs::File;
use std::io::Write;

/// Wave quantities at a port
#[derive(Debug, Clone, Copy, Default)]
struct Wave {
    voltage: f64,
    current: f64,
}

impl Wave {
    fn new(v: f64, i: f64) -> Self {
        Self { voltage: v, current: i }
    }
}

/// 2-Port Capacitor Model
struct WaveCapacitor {
    capacitance: f64,
    voltage: f64,  // Stored voltage (state)
    
    // Port impedances (key to wave behavior!)
    port1_impedance: f64,
    port2_impedance: f64,
    
    // Port waves
    port1_incident: Wave,
    port1_reflected: Wave,
    port2_incident: Wave,
    port2_reflected: Wave,
}

impl WaveCapacitor {
    fn new(capacitance: f64, dt: f64) -> Self {
        // Key insight: Capacitor impedance in discrete time
        // Z_c = Δt / (2C) from bilinear transform
        let z_port = dt / (2.0 * capacitance);
        
        Self {
            capacitance,
            voltage: 0.0,
            port1_impedance: z_port,
            port2_impedance: z_port,
            port1_incident: Wave::default(),
            port1_reflected: Wave::default(),
            port2_incident: Wave::default(),
            port2_reflected: Wave::default(),
        }
    }
    
    /// Core physics: How capacitor scatters waves
    fn scatter(&mut self) {
        // Capacitor physics in wave domain:
        // 1. The capacitor stores voltage
        // 2. It reflects waves based on its stored energy
        // 3. Current into capacitor changes stored voltage
        
        // Incident wave creates current into capacitor
        let i_incident = (self.port1_incident.voltage - self.port2_incident.voltage) / 
                        (self.port1_impedance + self.port2_impedance);
        
        // Capacitor reflects based on its voltage state
        // This is the KEY to making waves work!
        self.port1_reflected.voltage = -self.port1_incident.voltage + 2.0 * self.voltage;
        self.port1_reflected.current = -self.port1_reflected.voltage / self.port1_impedance;
        
        self.port2_reflected.voltage = -self.port2_incident.voltage - 2.0 * self.voltage;
        self.port2_reflected.current = -self.port2_reflected.voltage / self.port2_impedance;
    }
    
    /// Update capacitor state based on current flow
    fn update_state(&mut self, dt: f64) {
        // Calculate actual port voltages and currents
        let v1 = self.port1_incident.voltage + self.port1_reflected.voltage;
        let v2 = self.port2_incident.voltage + self.port2_reflected.voltage;
        let i1 = (self.port1_incident.voltage - self.port1_reflected.voltage) / self.port1_impedance;
        let i2 = (self.port2_incident.voltage - self.port2_reflected.voltage) / self.port2_impedance;
        
        // Current into capacitor (conservation)
        let i_cap = i1;  // What goes in port 1 must come out port 2
        
        // Update voltage: V = V + (I * dt) / C
        self.voltage += i_cap * dt / self.capacitance;
    }
    
    fn get_port_voltage(&self, port: usize) -> f64 {
        if port == 1 {
            self.port1_incident.voltage + self.port1_reflected.voltage
        } else {
            self.port2_incident.voltage + self.port2_reflected.voltage
        }
    }
    
    fn get_port_current(&self, port: usize) -> f64 {
        if port == 1 {
            (self.port1_incident.voltage - self.port1_reflected.voltage) / self.port1_impedance
        } else {
            (self.port2_incident.voltage - self.port2_reflected.voltage) / self.port2_impedance
        }
    }
}

fn main() {
    println!("=== Wave 2-Port Model: Single Capacitor ===\n");
    
    // Test setup
    let c = 100e-6;  // 100 µF
    let v_step = 5.0;  // 5V step
    let r_source = 50.0;  // Source impedance
    let dt = 1e-6;  // 1 µs timestep
    
    println!("Test: {}V step through {}Ω into {}µF capacitor", v_step, r_source, c * 1e6);
    
    // Create wave capacitor
    let mut cap = WaveCapacitor::new(c, dt);
    
    println!("Capacitor port impedance: {:.3} Ω", cap.port1_impedance);
    println!("  (This is Z = Δt/(2C) = {:.0}e-6 / (2 * {:.0}e-6) = {:.3} Ω)\n", 
             dt * 1e6, c * 1e6, cap.port1_impedance);
    
    // For comparison: traditional RC solution
    let tau = r_source * c;
    println!("Traditional RC time constant: τ = {:.3} ms\n", tau * 1000.0);
    
    // Output file
    let mut file = File::create("tests/outputs/wave_2port_capacitor.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,ic_wave_mA,vc_traditional,ic_traditional_mA,error_%").unwrap();
    
    // Simulation
    let duration = 5.0 * tau;  // 5 time constants
    let steps = (duration / dt) as usize;
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Apply voltage step through source impedance
        // This is like connecting: [V] --[Rs]-- [C] -- [GND]
        
        // Calculate incident wave from source
        // The source creates a wave based on voltage divider with port impedance
        let v_thevenin = v_step;
        let z_total = r_source + cap.port1_impedance;
        let i_initial = v_thevenin / z_total;
        
        // Incident wave at capacitor port 1
        cap.port1_incident = Wave::new(
            i_initial * cap.port1_impedance,
            i_initial
        );
        
        // Port 2 is grounded (no incident wave)
        cap.port2_incident = Wave::new(0.0, 0.0);
        
        // Scatter waves
        cap.scatter();
        
        // Update state
        cap.update_state(dt);
        
        // Get results
        let vc_wave = cap.voltage;
        let ic_wave = cap.get_port_current(1);
        
        // Traditional solution
        let vc_trad = v_step * (1.0 - (-time / tau).exp());
        let ic_trad = (v_step / r_source) * (-time / tau).exp();
        
        // Error calculation
        let error = if vc_trad > 0.01 {
            ((vc_wave - vc_trad) / vc_trad * 100.0).abs()
        } else {
            0.0
        };
        
        // Record every 10 steps
        if i % 10 == 0 {
            writeln!(file, "{:.3},{:.6},{:.3},{:.6},{:.3},{:.2}",
                     time * 1000.0,
                     vc_wave,
                     ic_wave * 1000.0,
                     vc_trad,
                     ic_trad * 1000.0,
                     error).unwrap();
        }
    }
    
    println!("Simulation complete. Results saved to: tests/outputs/wave_2port_capacitor.csv");
    
    println!("\nKEY PHYSICS INSIGHTS:");
    println!("1. Capacitor port impedance Z = Δt/(2C) comes from discretization");
    println!("2. Capacitor reflects waves based on stored voltage");
    println!("3. Reflection equation: b = -a + 2V_stored");
    println!("4. This creates the proper I = C*dV/dt behavior");
    println!("5. No global equations needed - just local wave scattering!");
    
    println!("\nNEXT STEPS:");
    println!("- Add series resistor to see wave interactions");
    println!("- Add inductor to create RLC circuit");
    println!("- Show how components connect through wave channels");
}