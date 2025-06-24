/// Two-Port Wave Solver with Superposition
/// 
/// Each component is treated as a 2-port network with:
/// - Port 1: forward wave a1, backward wave b1
/// - Port 2: forward wave a2, backward wave b2
/// 
/// Transmission lines connect the ports with bidirectional wave propagation

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;
use std::collections::VecDeque;

/// Wave at a port (complex in general, but we use real for now)
#[derive(Debug, Clone, Copy, Default)]
struct Wave {
    amplitude: f64,
}

/// 2-Port network interface
trait TwoPort: AsAny {
    /// Get S-parameters (scattering matrix)
    /// [b1]   [S11 S12] [a1]
    /// [b2] = [S21 S22] [a2]
    fn s_parameters(&self, freq: f64) -> [[f64; 2]; 2];
    
    /// Process waves through the 2-port
    fn scatter(&mut self, a1: Wave, a2: Wave, dt: f64) -> (Wave, Wave);
    
    /// Get internal state (for components with memory)
    fn get_state(&self) -> f64;
    
    /// Update internal state
    fn update_state(&mut self, dt: f64);
}

// Helper trait for downcasting
trait AsAny {
    fn as_any(&self) -> &dyn std::any::Any;
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;
}

impl<T: 'static> AsAny for T {
    fn as_any(&self) -> &dyn std::any::Any { self }
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any { self }
}

/// Resistor as 2-port
struct ResistorTwoPort {
    resistance: f64,
    z0: f64, // Reference impedance
}

impl ResistorTwoPort {
    fn new(resistance: f64, z0: f64) -> Self {
        Self { resistance, z0 }
    }
}

impl TwoPort for ResistorTwoPort {
    fn s_parameters(&self, _freq: f64) -> [[f64; 2]; 2] {
        // For a series resistor:
        // S11 = S22 = (R - Z0) / (R + 2*Z0)
        // S12 = S21 = 2*Z0 / (R + 2*Z0)
        let s11 = (self.resistance - self.z0) / (self.resistance + 2.0 * self.z0);
        let s21 = 2.0 * self.z0 / (self.resistance + 2.0 * self.z0);
        
        [[s11, s21],
         [s21, s11]]
    }
    
    fn scatter(&mut self, a1: Wave, a2: Wave, _dt: f64) -> (Wave, Wave) {
        let s = self.s_parameters(0.0);
        let b1 = Wave { amplitude: s[0][0] * a1.amplitude + s[0][1] * a2.amplitude };
        let b2 = Wave { amplitude: s[1][0] * a1.amplitude + s[1][1] * a2.amplitude };
        (b1, b2)
    }
    
    fn get_state(&self) -> f64 { 0.0 }
    fn update_state(&mut self, _dt: f64) {}
}

/// Inductor as 2-port with memory
struct InductorTwoPort {
    inductance: f64,
    current: f64,
    z0: f64,
}

impl InductorTwoPort {
    fn new(inductance: f64, z0: f64) -> Self {
        Self { inductance, current: 0.0, z0 }
    }
}

impl TwoPort for InductorTwoPort {
    fn s_parameters(&self, _freq: f64) -> [[f64; 2]; 2] {
        // Inductor impedance: jωL
        // For time domain, use incremental impedance
        let z_l = 2.0 * self.inductance / 1e-6; // dt = 1µs approximation
        let s11 = (z_l - self.z0) / (z_l + 2.0 * self.z0);
        let s21 = 2.0 * self.z0 / (z_l + 2.0 * self.z0);
        
        [[s11, s21],
         [s21, s11]]
    }
    
    fn scatter(&mut self, a1: Wave, a2: Wave, dt: f64) -> (Wave, Wave) {
        // Voltage across inductor from waves
        let v1 = self.z0 * (a1.amplitude - a2.amplitude);
        
        // Update inductor current: i(t+dt) = i(t) + v*dt/L
        self.current += v1 * dt / self.inductance;
        
        // Reflect based on current impedance mismatch
        let z_l = 2.0 * self.inductance / dt;
        let gamma = (z_l - self.z0) / (z_l + self.z0);
        
        // Include effect of changing current (back-EMF)
        let back_emf_factor = 0.1; // Tuning parameter
        
        let b1 = Wave { amplitude: gamma * a1.amplitude - back_emf_factor * self.current };
        let b2 = Wave { amplitude: (1.0 + gamma) * a1.amplitude - gamma * a2.amplitude };
        
        (b1, b2)
    }
    
    fn get_state(&self) -> f64 { self.current }
    
    fn update_state(&mut self, _dt: f64) {
        // State already updated in scatter()
    }
}

/// Capacitor as 2-port with memory
struct CapacitorTwoPort {
    capacitance: f64,
    voltage: f64,
    z0: f64,
}

impl CapacitorTwoPort {
    fn new(capacitance: f64, z0: f64) -> Self {
        Self { capacitance, voltage: 0.0, z0 }
    }
}

impl TwoPort for CapacitorTwoPort {
    fn s_parameters(&self, _freq: f64) -> [[f64; 2]; 2] {
        // Capacitor impedance: 1/(jωC)
        // For time domain, use incremental impedance
        let z_c = 1e-6 / (2.0 * self.capacitance); // dt = 1µs approximation
        let s11 = (z_c - self.z0) / (z_c + 2.0 * self.z0);
        let s21 = 2.0 * self.z0 / (z_c + 2.0 * self.z0);
        
        [[s11, s21],
         [s21, s11]]
    }
    
    fn scatter(&mut self, a1: Wave, a2: Wave, dt: f64) -> (Wave, Wave) {
        // Current through capacitor from waves
        let i1 = (a1.amplitude - a2.amplitude) / self.z0;
        
        // Update capacitor voltage: v(t+dt) = v(t) + i*dt/C
        self.voltage += i1 * dt / self.capacitance;
        
        // Reflect based on impedance mismatch
        let z_c = dt / (2.0 * self.capacitance);
        let gamma = (z_c - self.z0) / (z_c + self.z0);
        
        // Capacitor stores energy, so phase is important
        let b1 = Wave { amplitude: -gamma * a1.amplitude }; // Negative for capacitive
        let b2 = Wave { amplitude: (1.0 - gamma) * a1.amplitude + gamma * a2.amplitude };
        
        (b1, b2)
    }
    
    fn get_state(&self) -> f64 { self.voltage }
    
    fn update_state(&mut self, _dt: f64) {
        // State already updated in scatter()
    }
}

/// Voltage source as 2-port
struct VSourceTwoPort {
    voltage: f64,
    z0: f64,
}

impl VSourceTwoPort {
    fn new(voltage: f64, z0: f64) -> Self {
        Self { voltage, z0 }
    }
    
    fn set_voltage(&mut self, v: f64) {
        self.voltage = v;
    }
}

impl TwoPort for VSourceTwoPort {
    fn s_parameters(&self, _freq: f64) -> [[f64; 2]; 2] {
        // Ideal voltage source has zero impedance
        // But we use small internal resistance for stability
        let r_int = 0.01; // 10mΩ
        let s11 = (r_int - self.z0) / (r_int + 2.0 * self.z0);
        let s21 = 2.0 * self.z0 / (r_int + 2.0 * self.z0);
        
        [[s11, s21],
         [s21, s11]]
    }
    
    fn scatter(&mut self, a1: Wave, a2: Wave, _dt: f64) -> (Wave, Wave) {
        // Voltage source injects wave to maintain voltage
        // b1 = reflected wave = -a1 + V/Z0
        // b2 = transmitted wave includes source voltage
        
        let b1 = Wave { amplitude: -0.99 * a1.amplitude + self.voltage / self.z0 };
        let b2 = Wave { amplitude: 0.01 * a2.amplitude }; // Small reflection from ground
        
        (b1, b2)
    }
    
    fn get_state(&self) -> f64 { self.voltage }
    fn update_state(&mut self, _dt: f64) {}
}

/// Transmission line segment with delay
struct TransmissionLine {
    z0: f64,
    delay_samples: usize,
    forward_buffer: VecDeque<Wave>,
    backward_buffer: VecDeque<Wave>,
}

impl TransmissionLine {
    fn new(z0: f64, delay: f64, dt: f64) -> Self {
        let delay_samples = ((delay / dt).ceil() as usize).max(1);
        let mut forward_buffer = VecDeque::with_capacity(delay_samples);
        let mut backward_buffer = VecDeque::with_capacity(delay_samples);
        
        // Initialize with zeros
        for _ in 0..delay_samples {
            forward_buffer.push_back(Wave::default());
            backward_buffer.push_back(Wave::default());
        }
        
        Self { z0, delay_samples, forward_buffer, backward_buffer }
    }
    
    fn propagate(&mut self, forward_in: Wave, backward_in: Wave) -> (Wave, Wave) {
        // Store new waves
        self.forward_buffer.push_back(forward_in);
        self.backward_buffer.push_front(backward_in);
        
        // Get delayed waves
        let forward_out = self.forward_buffer.pop_front().unwrap_or_default();
        let backward_out = self.backward_buffer.pop_back().unwrap_or_default();
        
        (backward_out, forward_out)
    }
}

/// Simple 2-port circuit solver
struct TwoPortCircuit {
    components: Vec<Box<dyn TwoPort>>,
    transmission_lines: Vec<TransmissionLine>,
    z0: f64,
    dt: f64,
}

impl TwoPortCircuit {
    fn new(z0: f64, dt: f64) -> Self {
        Self {
            components: Vec::new(),
            transmission_lines: Vec::new(),
            z0,
            dt,
        }
    }
    
    fn step(&mut self) -> Vec<f64> {
        let n = self.components.len();
        let mut port_waves = vec![(Wave::default(), Wave::default()); n + 1];
        
        // Multiple iterations for convergence
        for iter in 0..5 {
            // Forward pass: propagate through components
            for i in 0..n {
                let (a1, a2) = if i == 0 {
                    (port_waves[i].0, port_waves[i+1].0)  // Source sees incoming from left and right
                } else {
                    (port_waves[i].1, port_waves[i+1].0)  // Component sees reflected from left, incoming from right
                };
                
                let (b1, b2) = self.components[i].scatter(a1, a2, self.dt);
                port_waves[i].1 = b1;  // Update reflected wave going left
                port_waves[i+1].0 = b2; // Update transmitted wave going right
                
                // Debug first iteration
                if iter == 0 && i == 0 {
                    let state = self.components[0].get_state();
                    if state != 0.0 {
                        println!("  Source: a1={:.3}, a2={:.3}, b1={:.3}, b2={:.3}, V={:.3}", 
                                 a1.amplitude, a2.amplitude, b1.amplitude, b2.amplitude, state);
                    }
                }
            }
            
            // Backward pass: propagate through transmission lines
            for i in 0..self.transmission_lines.len() {
                let forward = port_waves[i].1;
                let backward = port_waves[i+1].0;
                
                let (back_out, fwd_out) = self.transmission_lines[i].propagate(forward, backward);
                port_waves[i].0 = back_out;
                port_waves[i+1].1 = fwd_out;
            }
        }
        
        // Update component states
        for comp in &mut self.components {
            comp.update_state(self.dt);
        }
        
        // Return component states
        self.components.iter().map(|c| c.get_state()).collect()
    }
}

fn main() {
    println!("=== Two-Port Wave Solver with Superposition ===\n");
    
    // RLC circuit parameters
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    let v_step = 5.0;
    let z0 = 50.0; // Characteristic impedance
    
    // Calculate circuit characteristics
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("RLC Circuit: R={}Ω, L={}mH, C={}µF", r, l * 1000.0, c * 1e6);
    println!("Natural frequency: {:.1} Hz", f_0);
    println!("Damping ratio ζ = {:.3}", zeta);
    println!("Transmission line Z0 = {} Ω\n", z0);
    
    // Simulation parameters
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    // Create circuit
    let mut circuit = TwoPortCircuit::new(z0, dt);
    
    // Add components
    circuit.components.push(Box::new(VSourceTwoPort::new(0.0, z0)));
    circuit.components.push(Box::new(ResistorTwoPort::new(r, z0)));
    circuit.components.push(Box::new(InductorTwoPort::new(l, z0)));
    circuit.components.push(Box::new(CapacitorTwoPort::new(c, z0)));
    
    // Add transmission lines between components
    for _ in 0..4 {
        circuit.transmission_lines.push(TransmissionLine::new(z0, 10e-12, dt)); // 10ps delay
    }
    
    // Traditional solver for comparison
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    // Output file
    let mut file = File::create("tests/outputs/two_port_wave_solver.csv").unwrap();
    writeln!(file, "time_ms,v_source,i_inductor,v_capacitor,v_trad,error_percent").unwrap();
    
    println!("Running simulation...");
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        // Apply step at 1ms
        if time >= 1e-3 && time < 1e-3 + dt {
            if let Some(src) = circuit.components[0].as_any_mut().downcast_mut::<VSourceTwoPort>() {
                src.set_voltage(v_step);
            }
            println!("Applied {}V step at t={:.1}ms", v_step, time * 1000.0);
        }
        
        // Step wave solver
        let states = circuit.step();
        
        // Traditional solver step
        if time >= 1e-3 {
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt;
            il_trad += dil_dt * dt;
        }
        
        // Record data every 10 steps
        if i % 10 == 0 {
            let v_source = states[0];
            let i_inductor = states[2];
            let v_capacitor = states[3];
            
            let error = if vc_trad.abs() > 0.01 {
                ((v_capacitor - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.2}",
                     time * 1000.0, v_source, i_inductor, v_capacitor, vc_trad, error).unwrap();
        }
    }
    
    println!("\nResults saved to: tests/outputs/two_port_wave_solver.csv");
}

