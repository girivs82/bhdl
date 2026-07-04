/// Two-Port Wave Solver with Transmission Line Delays
/// 
/// Extends the basic solver with proper wave propagation delays

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Wave components at a port
#[derive(Debug, Clone, Copy, Default)]
struct PortWave {
    incident: f64,   // a (forward wave into port)
    reflected: f64,  // b (backward wave from port)
}

/// Transmission line with delay
struct DelayLine {
    delay: f64,           // Propagation delay in seconds
    samples: usize,       // Number of delay samples
    forward_buffer: Vec<f64>,  // Forward wave delay buffer
    backward_buffer: Vec<f64>, // Backward wave delay buffer
    index: usize,         // Current position in circular buffer
}

impl DelayLine {
    fn new(delay: f64, dt: f64) -> Self {
        let samples = ((delay / dt).ceil() as usize).max(1);
        Self {
            delay,
            samples,
            forward_buffer: vec![0.0; samples],
            backward_buffer: vec![0.0; samples],
            index: 0,
        }
    }
    
    fn propagate(&mut self, forward_in: f64, backward_in: f64) -> (f64, f64) {
        // Store new values
        self.forward_buffer[self.index] = forward_in;
        self.backward_buffer[self.index] = backward_in;
        
        // Get delayed values
        let delay_index = (self.index + 1) % self.samples;
        let forward_out = self.forward_buffer[delay_index];
        let backward_out = self.backward_buffer[delay_index];
        
        // Update index
        self.index = (self.index + 1) % self.samples;
        
        (forward_out, backward_out)
    }
}

/// Two-port element types
enum ElementType {
    Resistor { r: f64 },
    Inductor { l: f64, current: f64 },
    Capacitor { c: f64, voltage: f64 },
    VoltageSource { v: f64 },
}

/// Two-port network element
struct Element {
    name: String,
    element_type: ElementType,
    port1: PortWave,
    port2: PortWave,
    z0: f64,  // Reference impedance
}

impl Element {
    fn new(name: &str, element_type: ElementType, z0: f64) -> Self {
        Self {
            name: name.to_string(),
            element_type,
            port1: PortWave::default(),
            port2: PortWave::default(),
            z0,
        }
    }
    
    fn update(&mut self, dt: f64) {
        match &mut self.element_type {
            ElementType::Resistor { r } => {
                // S-parameters for series resistor
                let gamma = (*r - self.z0) / (*r + self.z0);
                
                self.port1.reflected = gamma * self.port1.incident + 
                                      (1.0 - gamma) * self.port2.incident;
                self.port2.reflected = (1.0 - gamma) * self.port1.incident + 
                                      gamma * self.port2.incident;
            }
            
            ElementType::Inductor { l, current } => {
                // Discrete inductor model
                // v = L * di/dt => i(n+1) = i(n) + v*dt/L
                
                // Port voltages from waves
                let v1 = self.z0.sqrt() * (self.port1.incident + self.port1.reflected);
                let v2 = self.z0.sqrt() * (self.port2.incident + self.port2.reflected);
                let v_l = v1 - v2;
                
                // Update current
                *current += v_l * dt / *l;
                
                // Impedance at current frequency (approximation)
                let z_l = 2.0 * *l / dt;
                let gamma = (z_l - self.z0) / (z_l + self.z0);
                
                // Reflections based on impedance mismatch
                self.port1.reflected = gamma * self.port1.incident;
                self.port2.reflected = gamma * self.port2.incident;
                
                // Add current contribution
                let i_wave = *current / self.z0.sqrt();
                self.port1.reflected -= i_wave * 0.5;
                self.port2.reflected += i_wave * 0.5;
            }
            
            ElementType::Capacitor { c, voltage } => {
                // Discrete capacitor model
                // i = C * dv/dt => v(n+1) = v(n) + i*dt/C
                
                // Port currents from waves
                let i1 = (self.port1.incident - self.port1.reflected) / self.z0.sqrt();
                let i2 = (self.port2.incident - self.port2.reflected) / self.z0.sqrt();
                let i_c = i1; // Current through capacitor
                
                // Update voltage
                *voltage += i_c * dt / *c;
                
                // Impedance at current frequency
                let z_c = dt / (2.0 * *c);
                let gamma = (z_c - self.z0) / (z_c + self.z0);
                
                // Reflections (capacitor inverts phase)
                self.port1.reflected = -gamma.abs() * self.port1.incident;
                self.port2.reflected = -gamma.abs() * self.port2.incident;
                
                // Add voltage contribution
                let v_wave = *voltage / self.z0.sqrt();
                self.port1.reflected += v_wave * 0.5;
                self.port2.reflected += v_wave * 0.5;
            }
            
            ElementType::VoltageSource { v } => {
                // Voltage source with zero internal impedance
                let v_wave = *v / self.z0.sqrt();
                self.port1.reflected = v_wave * 0.5 - self.port1.incident;
                self.port2.reflected = -v_wave * 0.5 - self.port2.incident;
            }
        }
    }
    
    fn get_port_voltage(&self, port: usize) -> f64 {
        let wave = if port == 1 { self.port1 } else { self.port2 };
        self.z0.sqrt() * (wave.incident + wave.reflected)
    }
}

/// Wave solver with transmission line sections
struct WaveCircuit {
    elements: Vec<Element>,
    delay_lines: Vec<DelayLine>,
    z0: f64,
    dt: f64,
    time: f64,
}

impl WaveCircuit {
    fn new(z0: f64, dt: f64) -> Self {
        Self {
            elements: Vec::new(),
            delay_lines: Vec::new(),
            z0,
            dt,
            time: 0.0,
        }
    }
    
    fn add_element(&mut self, element: Element, tl_delay: f64) {
        self.elements.push(element);
        if tl_delay > 0.0 {
            self.delay_lines.push(DelayLine::new(tl_delay, self.dt));
        }
    }
    
    fn step(&mut self) -> Vec<f64> {
        let n = self.elements.len();
        
        // Forward propagation
        for i in 0..n {
            // Update element
            self.elements[i].update(self.dt);
            
            // Propagate to next element through delay line
            if i + 1 < n && i < self.delay_lines.len() {
                let (fwd, bwd) = self.delay_lines[i].propagate(
                    self.elements[i].port2.reflected,
                    self.elements[i + 1].port1.reflected
                );
                self.elements[i + 1].port1.incident = fwd;
                self.elements[i].port2.incident = bwd;
            }
        }
        
        // Backward propagation
        for i in (0..n).rev() {
            self.elements[i].update(self.dt);
        }
        
        self.time += self.dt;
        
        // Return voltages at each element output
        self.elements.iter().map(|e| e.get_port_voltage(2)).collect()
    }
}

fn main() {
    println!("=== Two-Port Wave Solver with Delays ===\n");
    
    // Test RLC circuit
    let r = 50.0;
    let l = 10e-3;  // 10mH
    let c = 100e-6; // 100µF
    let v_step = 5.0;
    let z0 = 50.0;
    
    // Circuit characteristics
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("RLC Circuit: R={}Ω, L={}mH, C={}µF", r, l * 1000.0, c * 1e6);
    println!("Natural frequency: {:.1} Hz", f_0);
    println!("Damping ratio ζ = {:.3}", zeta);
    
    // Create circuit
    let dt = 1e-6;  // 1µs timestep
    let tl_delay = 10e-12;  // 10ps between elements
    let mut circuit = WaveCircuit::new(z0, dt);
    
    // Build circuit: Source -> R -> L -> C
    circuit.add_element(
        Element::new("Source", ElementType::VoltageSource { v: 0.0 }, z0),
        tl_delay
    );
    circuit.add_element(
        Element::new("R", ElementType::Resistor { r }, z0),
        tl_delay
    );
    circuit.add_element(
        Element::new("L", ElementType::Inductor { l, current: 0.0 }, z0),
        tl_delay
    );
    circuit.add_element(
        Element::new("C", ElementType::Capacitor { c, voltage: 0.0 }, z0),
        0.0  // No delay after last element
    );
    
    // Traditional RLC solver for comparison
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    // Run simulation
    let duration = 50e-3;  // 50ms
    let steps = (duration / dt) as usize;
    let mut output = File::create("tests/outputs/two_port_rlc_delays.csv").unwrap();
    writeln!(output, "time_ms,v_wave,v_traditional,error_percent").unwrap();
    
    let mut max_error = 0.0_f64;
    let mut rms_error = 0.0_f64;
    let mut count = 0;
    
    println!("\nRunning simulation...");
    
    for i in 0..steps {
        let time = i as f64 * dt;
        
        // Apply step at 1ms
        if time >= 1e-3 && time < 1e-3 + dt {
            if let ElementType::VoltageSource { ref mut v } = circuit.elements[0].element_type {
                *v = v_step;
            }
            println!("Applied {}V step at t={:.1}ms", v_step, time * 1000.0);
        }
        
        // Wave solver step
        let voltages = circuit.step();
        let v_cap = voltages.last().copied().unwrap_or(0.0);
        
        // Traditional solver
        if time >= 1e-3 {
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt;
            il_trad += dil_dt * dt;
        }
        
        // Record every 10 steps
        if i % 10 == 0 {
            let error = if vc_trad.abs() > 0.01 {
                ((v_cap - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            if time > 2e-3 {  // After transients
                max_error = max_error.max(error);
                rms_error += error * error;
                count += 1;
            }
            
            writeln!(output, "{:.3},{:.6},{:.6},{:.2}", 
                     time * 1000.0, v_cap, vc_trad, error).unwrap();
        }
    }
    
    if count > 0 {
        rms_error = (rms_error / count as f64).sqrt();
    }
    
    println!("\nResults:");
    println!("Max error: {:.2}%", max_error);
    println!("RMS error: {:.2}%", rms_error);
    println!("Results saved to: tests/outputs/two_port_rlc_delays.csv");
    
    if max_error > 10.0 {
        println!("\nNote: High error suggests we need:");
        println!("- Better energy storage modeling");
        println!("- Iterative convergence per timestep");
        println!("- Adaptive filtering based on circuit bandwidth");
    }
}