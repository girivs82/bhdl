/// Dynamic 2-Port Wave Solver with Proper Bidirectional Propagation
/// 
/// Each connection point has forward and backward traveling waves
/// Components process these waves and generate new ones

use std::fs::File;
use std::io::Write;

#[derive(Debug, Clone, Copy, Default)]
struct PortWaves {
    forward: f64,   // Wave traveling right →
    backward: f64,  // Wave traveling left ←
}

impl PortWaves {
    fn voltage(&self) -> f64 {
        self.forward + self.backward
    }
    
    fn current(&self, z0: f64) -> f64 {
        (self.forward - self.backward) / z0
    }
}

/// RC circuit with wave propagation
struct WaveRC {
    // Circuit parameters
    v_source: f64,
    r: f64,
    c: f64,
    z0: f64,
    
    // Port waves (at each connection point)
    port1: PortWaves,  // Source output
    port2: PortWaves,  // Between R and C
    port3: PortWaves,  // Capacitor bottom (ground)
    
    // Component states
    vc: f64,
    
    // Simulation
    dt: f64,
}

impl WaveRC {
    fn new(r: f64, c: f64, z0: f64, dt: f64) -> Self {
        Self {
            v_source: 0.0,
            r,
            c,
            z0,
            port1: PortWaves::default(),
            port2: PortWaves::default(),
            port3: PortWaves::default(),
            vc: 0.0,
            dt,
        }
    }
    
    fn step(&mut self) {
        // Save old values for stability
        let old_port1 = self.port1;
        let old_port2 = self.port2;
        
        // Step 1: Voltage source generates waves
        // The source maintains voltage by adjusting its output wave
        let v_at_source = self.port1.voltage();
        let v_error = self.v_source - v_at_source;
        self.port1.forward = self.v_source / 2.0 + 0.5 * v_error;
        
        // Step 2: Resistor scattering
        // Input: port1.forward (from source), port2.backward (from capacitor)
        // The resistor sees waves from both sides
        let _gamma_r = (self.r - self.z0) / (self.r + self.z0);
        
        // Scattering at resistor
        // S-parameters for series resistor in 50Ω system:
        // S11 = S22 = (R - Z0)/(R + 2*Z0)
        // S21 = S12 = 2*Z0/(R + 2*Z0)
        let s11 = (self.r - self.z0) / (self.r + 2.0 * self.z0);
        let s21 = 2.0 * self.z0 / (self.r + 2.0 * self.z0);
        
        // New waves after resistor
        self.port1.backward = s11 * old_port1.forward + s21 * old_port2.backward;
        self.port2.forward = s21 * old_port1.forward + s11 * old_port2.backward;
        
        // Step 3: Capacitor processing
        // The capacitor integrates current
        let i_cap = self.port2.current(self.z0);
        self.vc += i_cap * self.dt / self.c;
        
        // Capacitor reflection
        // For a shunt capacitor, we need to consider it as load
        // The capacitor impedance at this instant
        let z_c = if i_cap.abs() > 1e-12 {
            self.vc / i_cap
        } else {
            1e6  // Very high impedance at start
        };
        
        let gamma_c = (z_c - self.z0) / (z_c + self.z0);
        
        // Reflected wave from capacitor
        self.port2.backward = gamma_c * self.port2.forward;
        
        // Ground is perfect absorber
        self.port3.forward = 0.0;
        self.port3.backward = 0.0;
    }
}

/// RLC circuit with wave propagation
struct WaveRLC {
    // Circuit parameters
    v_source: f64,
    r: f64,
    l: f64,
    c: f64,
    z0: f64,
    
    // Port waves
    port1: PortWaves,  // Source output
    port2: PortWaves,  // Between R and L
    port3: PortWaves,  // Between L and C
    port4: PortWaves,  // Ground
    
    // Component states
    il: f64,  // Inductor current
    vc: f64,  // Capacitor voltage
    
    // Simulation
    dt: f64,
}

impl WaveRLC {
    fn new(r: f64, l: f64, c: f64, z0: f64, dt: f64) -> Self {
        Self {
            v_source: 0.0,
            r, l, c, z0,
            port1: PortWaves::default(),
            port2: PortWaves::default(),
            port3: PortWaves::default(),
            port4: PortWaves::default(),
            il: 0.0,
            vc: 0.0,
            dt,
        }
    }
    
    fn step(&mut self) {
        // Save old values
        let old_port1 = self.port1;
        let old_port2 = self.port2;
        let old_port3 = self.port3;
        
        // Voltage source
        let v_at_source = self.port1.voltage();
        let v_error = self.v_source - v_at_source;
        self.port1.forward = self.v_source / 2.0 + 0.5 * v_error;
        
        // Resistor scattering
        let s11_r = (self.r - self.z0) / (self.r + 2.0 * self.z0);
        let s21_r = 2.0 * self.z0 / (self.r + 2.0 * self.z0);
        
        self.port1.backward = s11_r * old_port1.forward + s21_r * old_port2.backward;
        self.port2.forward = s21_r * old_port1.forward + s11_r * old_port2.backward;
        
        // Inductor processing
        // Voltage across inductor
        let v_l = self.port2.voltage() - self.port3.voltage();
        // Update inductor current
        self.il += v_l * self.dt / self.l;
        
        // Inductor creates waves based on its impedance
        let z_l = 2.0 * self.l / self.dt;  // Incremental impedance
        let _gamma_l = (z_l - self.z0) / (z_l + self.z0);
        
        // For series inductor, use 2-port S-parameters
        let s11_l = (z_l - self.z0) / (z_l + 2.0 * self.z0);
        let s21_l = 2.0 * self.z0 / (z_l + 2.0 * self.z0);
        
        self.port2.backward = s11_l * old_port2.forward + s21_l * old_port3.backward;
        self.port3.forward = s21_l * old_port2.forward + s11_l * old_port3.backward;
        
        // Capacitor processing
        let i_cap = self.port3.current(self.z0);
        self.vc += i_cap * self.dt / self.c;
        
        // Capacitor reflection
        let z_c = if i_cap.abs() > 1e-12 {
            self.vc / i_cap
        } else {
            1e6
        };
        
        let gamma_c = (z_c - self.z0) / (z_c + self.z0);
        self.port3.backward = gamma_c * self.port3.forward;
        
        // Ground
        self.port4.forward = 0.0;
        self.port4.backward = 0.0;
    }
}

fn main() {
    println!("=== Dynamic 2-Port Wave Solver ===\n");
    
    // Test both RC and RLC
    test_rc();
    println!("\n{}\n", "=".repeat(50));
    test_rlc();
}

fn test_rc() {
    println!("RC Circuit Test:");
    
    let r = 50.0;
    let c = 100e-6;
    let z0 = 50.0;
    let v_step = 5.0;
    
    let dt = 1e-6;
    let duration = 20e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut circuit = WaveRC::new(r, c, z0, dt);
    let mut vc_trad = 0.0;
    
    let mut file = File::create("tests/outputs/dynamic_2port_rc.csv").unwrap();
    writeln!(file, "time_ms,v_fwd,v_back,v_cap_wave,v_cap_trad,error_percent").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time >= 1e-3 {
            circuit.v_source = v_step;
            let tau = r * c;
            vc_trad = v_step * (1.0 - (-(time - 1e-3) / tau).exp());
        }
        
        circuit.step();
        
        if i % 10 == 0 {
            let error = if vc_trad > 0.01 {
                ((circuit.vc - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.2}",
                     time * 1000.0,
                     circuit.port1.forward,
                     circuit.port1.backward,
                     circuit.vc,
                     vc_trad,
                     error).unwrap();
        }
    }
    
    println!("  Final Vc: {:.3}V (wave), {:.3}V (traditional)", circuit.vc, vc_trad);
    println!("  Error: {:.1}%", ((circuit.vc - vc_trad) / vc_trad * 100.0).abs());
    println!("  Results: tests/outputs/dynamic_2port_rc.csv");
}

fn test_rlc() {
    println!("RLC Circuit Test:");
    
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    let z0 = 50.0;
    let v_step = 5.0;
    
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut circuit = WaveRLC::new(r, l, c, z0, dt);
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    let mut file = File::create("tests/outputs/dynamic_2port_rlc.csv").unwrap();
    writeln!(file, "time_ms,il_wave,vc_wave,vc_trad,error_percent").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time >= 1e-3 {
            circuit.v_source = v_step;
            
            // Traditional RLC solver
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt;
            il_trad += dil_dt * dt;
        }
        
        circuit.step();
        
        if i % 10 == 0 {
            let error = if vc_trad > 0.01 {
                ((circuit.vc - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.2}",
                     time * 1000.0,
                     circuit.il * 1000.0,  // mA
                     circuit.vc,
                     vc_trad,
                     error).unwrap();
        }
    }
    
    println!("  Final Vc: {:.3}V (wave), {:.3}V (traditional)", circuit.vc, vc_trad);
    println!("  Error: {:.1}%", ((circuit.vc - vc_trad) / vc_trad * 100.0).abs());
    println!("  Results: tests/outputs/dynamic_2port_rlc.csv");
}