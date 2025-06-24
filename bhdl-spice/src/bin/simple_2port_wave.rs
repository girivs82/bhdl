/// Simplified 2-Port Wave Solver
/// 
/// Start simple: just RC circuit with proper wave propagation

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

#[derive(Debug, Clone, Copy, Default)]
struct Wave {
    v: f64,  // Voltage wave
    i: f64,  // Current wave  
}

/// Simple 2-port circuit: Source -> TL -> R -> TL -> C -> Ground
struct Simple2PortCircuit {
    // Component values
    v_source: f64,
    r: f64,
    c: f64,
    z0: f64,
    
    // Wave states at each interface
    waves: Vec<Wave>,  // Forward and backward waves at each point
    
    // Component states
    vc: f64,  // Capacitor voltage
    
    // Time
    dt: f64,
}

impl Simple2PortCircuit {
    fn new(r: f64, c: f64, z0: f64, dt: f64) -> Self {
        Self {
            v_source: 0.0,
            r,
            c,
            z0,
            waves: vec![Wave::default(); 6], // 3 interfaces x 2 directions
            vc: 0.0,
            dt,
        }
    }
    
    fn step(&mut self) {
        // Port 0: Source output
        // Port 1: Resistor input  
        // Port 2: Capacitor input
        
        // Step 1: Source generates forward wave
        let v_fwd_0 = self.v_source / 2.0;  // Half voltage goes forward
        let i_fwd_0 = v_fwd_0 / self.z0;
        self.waves[0] = Wave { v: v_fwd_0, i: i_fwd_0 };
        
        // Step 2: Forward wave hits resistor
        // Reflection coefficient: Γ = (R - Z0) / (R + Z0)
        let gamma_r = (self.r - self.z0) / (self.r + self.z0);
        let trans_r = 1.0 + gamma_r;  // Transmission = 1 + Γ
        
        // Reflected and transmitted at resistor
        let v_back_1 = self.waves[0].v * gamma_r;
        let i_back_1 = -v_back_1 / self.z0;  // Current reverses for backward wave
        self.waves[1] = Wave { v: v_back_1, i: i_back_1 };
        
        let v_fwd_2 = self.waves[0].v * trans_r * self.z0 / (self.r + self.z0);
        let i_fwd_2 = v_fwd_2 / self.z0;
        self.waves[2] = Wave { v: v_fwd_2, i: i_fwd_2 };
        
        // Step 3: Forward wave hits capacitor
        // Capacitor impedance (incremental)
        let zc = self.dt / (2.0 * self.c);
        let gamma_c = (zc - self.z0) / (zc + self.z0);
        
        // Current into capacitor updates voltage
        let ic = self.waves[2].i;
        self.vc += ic * self.dt / self.c;
        
        // Capacitor reflects based on its current state
        let v_back_3 = -self.waves[2].v * gamma_c.abs();  // Capacitive reflection
        let i_back_3 = -v_back_3 / self.z0;
        self.waves[3] = Wave { v: v_back_3, i: i_back_3 };
        
        // Step 4: Backward wave from capacitor hits resistor from right
        let v_back_4 = self.waves[3].v * trans_r * self.z0 / (self.r + self.z0);
        let i_back_4 = -v_back_4 / self.z0;
        self.waves[4] = Wave { v: v_back_4, i: i_back_4 };
        
        // Step 5: Source sees reflected wave and adjusts
        let v_reflected = self.waves[1].v + self.waves[4].v;
        // Source maintains voltage by injecting compensating wave
        let v_error = self.v_source - 2.0 * (self.waves[0].v + v_reflected);
        self.waves[0].v += 0.1 * v_error;  // Feedback gain
    }
}

fn main() {
    println!("=== Simplified 2-Port Wave Solver ===\n");
    
    // Circuit parameters
    let r = 50.0;
    let c = 100e-6;
    let z0 = 50.0;
    let v_step = 5.0;
    
    println!("RC Circuit: R={}Ω, C={}µF, Z0={}Ω", r, c * 1e6, z0);
    
    // Simulation
    let dt = 1e-6;
    let duration = 20e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut circuit = Simple2PortCircuit::new(r, c, z0, dt);
    
    // Traditional solver
    let mut vc_trad = 0.0;
    
    // Output
    let mut file = File::create("tests/outputs/simple_2port_wave.csv").unwrap();
    writeln!(file, "time_ms,v_fwd,v_back,v_capacitor,v_traditional,error_percent").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        // Apply step
        if time >= 1e-3 {
            circuit.v_source = v_step;
            
            // Traditional
            let tau = r * c;
            vc_trad = v_step * (1.0 - (-(time - 1e-3) / tau).exp());
        }
        
        // Step simulation
        circuit.step();
        
        // Record every 10 steps
        if i % 10 == 0 {
            let error = if vc_trad > 0.01 {
                ((circuit.vc - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.2}",
                     time * 1000.0, 
                     circuit.waves[0].v,
                     circuit.waves[1].v + circuit.waves[4].v,
                     circuit.vc,
                     vc_trad,
                     error).unwrap();
        }
    }
    
    println!("\nResults saved to: tests/outputs/simple_2port_wave.csv");
    
    // Final values
    println!("\nFinal values:");
    println!("  Wave solver: Vc = {:.3} V", circuit.vc);
    println!("  Traditional: Vc = {:.3} V", vc_trad);
    println!("  Error: {:.1}%", ((circuit.vc - vc_trad) / vc_trad * 100.0).abs());
}