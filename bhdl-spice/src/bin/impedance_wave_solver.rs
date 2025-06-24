/// Impedance-Based Wave Solver
/// 
/// Each component creates reflections based on impedance mismatch
/// Waves propagate naturally through the network

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Transmission line segment connecting components
#[derive(Debug, Clone)]
struct TLine {
    z0: f64,           // Characteristic impedance
    delay: f64,        // Propagation delay
    v_forward: f64,    // Forward voltage wave
    v_backward: f64,   // Backward voltage wave
}

impl TLine {
    fn new(z0: f64) -> Self {
        Self {
            z0,
            delay: 10e-12,  // 10ps default
            v_forward: 0.0,
            v_backward: 0.0,
        }
    }
    
    fn voltage(&self) -> f64 {
        self.v_forward + self.v_backward
    }
    
    fn current(&self) -> f64 {
        (self.v_forward - self.v_backward) / self.z0
    }
}

/// Wave-based circuit solver
struct WaveCircuit {
    // Components
    v_source: f64,
    r: f64,
    l: f64,
    c: f64,
    
    // Transmission lines connecting components
    tl_source: TLine,  // Source to R
    tl_r_to_l: TLine,  // R to L
    tl_l_to_c: TLine,  // L to C
    
    // Component states
    il: f64,  // Inductor current
    vc: f64,  // Capacitor voltage
    
    // Simulation
    dt: f64,
    time: f64,
}

impl WaveCircuit {
    fn new(r: f64, l: f64, c: f64, z0: f64, dt: f64) -> Self {
        Self {
            v_source: 0.0,
            r, l, c,
            tl_source: TLine::new(z0),
            tl_r_to_l: TLine::new(z0),
            tl_l_to_c: TLine::new(z0),
            il: 0.0,
            vc: 0.0,
            dt,
            time: 0.0,
        }
    }
    
    fn step(&mut self) {
        // Source generates waves
        self.tl_source.v_forward = self.v_source / 2.0;
        
        // Wave hits resistor - calculate reflection and transmission
        let z_r = self.r;
        let gamma_r = (z_r - self.tl_source.z0) / (z_r + self.tl_source.z0);
        let tau_r = 1.0 + gamma_r;
        
        // Reflected back to source
        self.tl_source.v_backward = gamma_r * self.tl_source.v_forward;
        
        // Transmitted through resistor
        let v_after_r = tau_r * self.tl_source.v_forward;
        let i_after_r = v_after_r / (self.r + self.tl_source.z0);
        self.tl_r_to_l.v_forward = i_after_r * self.tl_r_to_l.z0;
        
        // Wave hits inductor
        // Inductor impedance (using incremental model)
        let z_l = if self.time > 0.0 {
            2.0 * self.l / self.dt  // Bilinear transform approximation
        } else {
            1e6  // Very high impedance initially
        };
        
        // Voltage across inductor drives current change
        let v_l = self.tl_r_to_l.voltage() - self.tl_l_to_c.voltage();
        self.il += v_l * self.dt / self.l;
        
        // Inductor creates reflections
        let gamma_l = (z_l - self.tl_r_to_l.z0) / (z_l + self.tl_r_to_l.z0);
        self.tl_r_to_l.v_backward = gamma_l * self.tl_r_to_l.v_forward;
        
        // Transmitted through inductor
        self.tl_l_to_c.v_forward = (1.0 - gamma_l.abs()) * self.tl_r_to_l.v_forward;
        
        // Wave hits capacitor
        // Capacitor impedance
        let z_c = if self.il.abs() > 1e-12 {
            self.vc / self.il.abs()
        } else {
            1e6
        };
        
        // Capacitor integrates current
        let i_c = self.tl_l_to_c.current();
        self.vc += i_c * self.dt / self.c;
        
        // Capacitor reflection (mostly reflective initially)
        let gamma_c = (z_c - self.tl_l_to_c.z0) / (z_c + self.tl_l_to_c.z0);
        self.tl_l_to_c.v_backward = gamma_c * self.tl_l_to_c.v_forward;
        
        self.time += self.dt;
    }
}

fn main() {
    println!("=== Impedance-Based Wave Solver ===\n");
    
    // Test parameters
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    let z0 = 50.0;
    let v_step = 5.0;
    
    // First test without L
    println!("Test 1: RC Circuit (L ≈ 0)");
    test_circuit(r, 1e-12, c, z0, v_step, "impedance_rc.csv");
    
    println!("\n{}\n", "=".repeat(50));
    
    // Second test with L
    println!("Test 2: RLC Circuit");
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    println!("  Natural frequency: {:.1} Hz", omega_0 / (2.0 * PI));
    println!("  Damping ratio: ζ = {:.3}", zeta);
    test_circuit(r, l, c, z0, v_step, "impedance_rlc.csv");
}

fn test_circuit(r: f64, l: f64, c: f64, z0: f64, v_step: f64, filename: &str) {
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut circuit = WaveCircuit::new(r, l, c, z0, dt);
    
    // Traditional solver for comparison
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    let mut file = File::create(format!("tests/outputs/{}", filename)).unwrap();
    writeln!(file, "time_ms,il_wave_mA,vc_wave,il_trad_mA,vc_trad,error_vc_%").unwrap();
    
    let mut max_vc = 0.0;
    let mut final_error = 0.0;
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time >= 1e-3 {
            circuit.v_source = v_step;
            
            // Traditional solver
            if l > 1e-9 {  // RLC
                let dvc_dt = il_trad / c;
                let dil_dt = (v_step - vc_trad - r * il_trad) / l;
                vc_trad += dvc_dt * dt;
                il_trad += dil_dt * dt;
            } else {  // RC
                let tau = r * c;
                vc_trad = v_step * (1.0 - (-(time - 1e-3) / tau).exp());
            }
        }
        
        circuit.step();
        
        if circuit.vc > max_vc {
            max_vc = circuit.vc;
        }
        
        if i % 100 == 0 {  // Record every 100 steps
            let error_vc = if vc_trad > 0.01 {
                ((circuit.vc - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            final_error = error_vc;
            
            writeln!(file, "{:.3},{:.3},{:.6},{:.3},{:.6},{:.2}",
                     time * 1000.0,
                     circuit.il * 1000.0,
                     circuit.vc,
                     il_trad * 1000.0,
                     vc_trad,
                     error_vc).unwrap();
        }
    }
    
    println!("  Max Vc: {:.3} V", max_vc);
    println!("  Final Vc: {:.3} V (wave), {:.3} V (traditional)", circuit.vc, vc_trad);
    println!("  Final error: {:.1}%", final_error);
    println!("  Results: tests/outputs/{}", filename);
}