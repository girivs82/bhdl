/// Proper Wave-Based Solver with Correct Physics
/// 
/// This implements true wave propagation with energy storage in L and C

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Wave amplitude (voltage and current)
#[derive(Debug, Clone, Copy, Default)]
struct Wave {
    voltage: f64,
    current: f64,
}

impl Wave {
    fn new(v: f64, i: f64) -> Self {
        Self { voltage: v, current: i }
    }
    
    /// Power carried by the wave
    fn power(&self) -> f64 {
        self.voltage * self.current
    }
}

/// Port connecting components
#[derive(Debug, Clone, Default)]
struct Port {
    forward: Wave,   // Wave going into the port
    backward: Wave,  // Wave coming out of the port
    voltage: f64,    // Total voltage at port
    current: f64,    // Total current at port
}

impl Port {
    /// Update voltage and current from waves
    fn update_from_waves(&mut self) {
        self.voltage = self.forward.voltage + self.backward.voltage;
        self.current = self.forward.current - self.backward.current;
    }
    
    /// Calculate waves from voltage and current
    fn update_waves(&mut self, z0: f64) {
        // V = V+ + V-
        // I = (V+ - V-) / Z0
        // Solving: V+ = (V + Z0*I) / 2
        //          V- = (V - Z0*I) / 2
        let v_plus = (self.voltage + z0 * self.current) / 2.0;
        let v_minus = (self.voltage - z0 * self.current) / 2.0;
        
        self.forward.voltage = v_plus;
        self.forward.current = v_plus / z0;
        self.backward.voltage = v_minus;
        self.backward.current = v_minus / z0;
    }
}

/// Transmission line connecting two ports
#[derive(Debug)]
struct TransmissionLine {
    z0: f64,              // Characteristic impedance
    delay: f64,           // Propagation delay
    history_size: usize,  // Number of delay steps
    forward_history: Vec<Wave>,  // History buffer for forward waves
    backward_history: Vec<Wave>, // History buffer for backward waves
    write_idx: usize,     // Current write position in circular buffer
}

impl TransmissionLine {
    fn new(z0: f64, delay: f64, dt: f64) -> Self {
        let history_size = ((delay / dt).ceil() as usize).max(1);
        Self {
            z0,
            delay,
            history_size,
            forward_history: vec![Wave::default(); history_size],
            backward_history: vec![Wave::default(); history_size],
            write_idx: 0,
        }
    }
    
    /// Propagate waves through the transmission line
    fn propagate(&mut self, port1: &mut Port, port2: &mut Port) {
        // Store current waves in history
        self.forward_history[self.write_idx] = port1.forward;
        self.backward_history[self.write_idx] = port2.backward;
        
        // Retrieve delayed waves
        let read_idx = (self.write_idx + 1) % self.history_size;
        port2.forward = self.forward_history[read_idx];
        port1.backward = self.backward_history[read_idx];
        
        // Update write index
        self.write_idx = (self.write_idx + 1) % self.history_size;
    }
}


/// Voltage source
struct VoltageSource {
    voltage: f64,
}

impl Component for VoltageSource {
    fn process(&mut self, port1: &mut Port, port2: &mut Port, z0: f64, _dt: f64) {
        // Source maintains voltage between ports
        port1.voltage = self.voltage;
        port2.voltage = 0.0; // Ground
        
        // Current determined by load
        let current = port1.forward.current - port1.backward.current;
        port1.current = current;
        port2.current = -current;
        
        // Update waves
        port1.update_waves(z0);
        port2.update_waves(z0);
    }
    
    fn get_state(&self) -> (f64, f64) {
        (self.voltage, 0.0)
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Resistor
struct Resistor {
    resistance: f64,
}

impl Component for Resistor {
    fn process(&mut self, port1: &mut Port, port2: &mut Port, z0: f64, _dt: f64) {
        // Update port voltages/currents from waves
        port1.update_from_waves();
        port2.update_from_waves();
        
        // Ohm's law
        let voltage = port1.voltage - port2.voltage;
        let current = voltage / self.resistance;
        
        // Set port currents
        port1.current = current;
        port2.current = -current;
        
        // Calculate reflection coefficient
        let gamma = (self.resistance - z0) / (self.resistance + z0);
        
        // Scattering: reflected = gamma * incident
        port1.backward.voltage = port1.forward.voltage * gamma;
        port1.backward.current = port1.backward.voltage / z0;
        
        // Transmitted wave
        let transmission = 1.0 + gamma; // 2Z2/(Z1+Z2)
        port2.forward.voltage = port1.forward.voltage * transmission;
        port2.forward.current = port2.forward.voltage / z0;
        
        // Update port values
        port1.update_from_waves();
        port2.update_from_waves();
    }
    
    fn get_state(&self) -> (f64, f64) {
        (0.0, 0.0)
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Inductor with energy storage
struct Inductor {
    inductance: f64,
    current: f64,  // Stored magnetic energy as current
}

impl Component for Inductor {
    fn process(&mut self, port1: &mut Port, port2: &mut Port, z0: f64, dt: f64) {
        // Update from waves
        port1.update_from_waves();
        port2.update_from_waves();
        
        // Voltage across inductor
        let v_inductor = port1.voltage - port2.voltage;
        
        // Inductor equation: V = L * di/dt
        // Discrete form: i(n+1) = i(n) + V * dt / L
        let di = v_inductor * dt / self.inductance;
        self.current += di;
        
        // Inductor impedance (frequency-dependent, use discrete approximation)
        let z_l = 2.0 * self.inductance / dt;
        
        // Reflection coefficient
        let gamma = (z_l - z0) / (z_l + z0);
        
        // The inductor reflects waves based on its impedance
        // But also generates back-EMF opposing current change
        let back_emf = -self.inductance * di / dt;
        
        // Reflected wave includes back-EMF effect
        port1.backward.voltage = port1.forward.voltage * gamma + back_emf * 0.5;
        port1.backward.current = port1.backward.voltage / z0;
        
        // Current through inductor
        port1.current = self.current;
        port2.current = -self.current;
        
        // Voltage at port2 (after inductor)
        port2.voltage = port1.voltage - v_inductor;
        
        // Update waves
        port1.update_waves(z0);
        port2.update_waves(z0);
    }
    
    fn get_state(&self) -> (f64, f64) {
        (0.0, self.current)
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Capacitor with energy storage
struct Capacitor {
    capacitance: f64,
    voltage: f64,  // Stored electric energy as voltage
}

impl Component for Capacitor {
    fn process(&mut self, port1: &mut Port, port2: &mut Port, z0: f64, dt: f64) {
        // Update from waves
        port1.update_from_waves();
        port2.update_from_waves();
        
        // Current into capacitor
        let i_cap = port1.current;
        
        // Capacitor equation: I = C * dv/dt
        // Discrete form: v(n+1) = v(n) + I * dt / C
        let dv = i_cap * dt / self.capacitance;
        self.voltage += dv;
        
        // Capacitor impedance (discrete approximation)
        let z_c = dt / (2.0 * self.capacitance);
        
        // Reflection coefficient (negative for capacitor)
        let gamma = (z_c - z0) / (z_c + z0);
        
        // Capacitor reflects with inverted phase (stores energy)
        port1.backward.voltage = -port1.forward.voltage * gamma.abs();
        port1.backward.current = port1.backward.voltage / z0;
        
        // Set port voltages
        port2.voltage = self.voltage;
        port2.current = -i_cap;
        
        // Update waves
        port1.update_waves(z0);
        port2.update_waves(z0);
    }
    
    fn get_state(&self) -> (f64, f64) {
        (self.voltage, 0.0)
    }
    
    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Component trait that works with downcasting
trait Component: std::any::Any {
    /// Process waves at ports and update internal state
    fn process(&mut self, port1: &mut Port, port2: &mut Port, z0: f64, dt: f64);
    
    /// Get component state for monitoring
    fn get_state(&self) -> (f64, f64); // (voltage, current)
    
    /// For downcasting
    fn as_any(&self) -> &dyn std::any::Any;
}

/// Complete wave solver
struct WaveSolver {
    ports: Vec<Port>,
    components: Vec<Box<dyn Component>>,
    component_ports: Vec<(usize, usize)>, // Which ports each component connects
    transmission_lines: Vec<TransmissionLine>,
    tl_ports: Vec<(usize, usize)>, // Which ports each TL connects
    z0: f64,
    dt: f64,
}

impl WaveSolver {
    fn new(num_ports: usize, z0: f64, dt: f64) -> Self {
        Self {
            ports: vec![Port::default(); num_ports],
            components: Vec::new(),
            component_ports: Vec::new(),
            transmission_lines: Vec::new(),
            tl_ports: Vec::new(),
            z0,
            dt,
        }
    }
    
    fn add_voltage_source(&mut self, voltage: f64, port1: usize, port2: usize) {
        self.components.push(Box::new(VoltageSource { voltage }));
        self.component_ports.push((port1, port2));
    }
    
    fn add_resistor(&mut self, resistance: f64, port1: usize, port2: usize) {
        self.components.push(Box::new(Resistor { resistance }));
        self.component_ports.push((port1, port2));
    }
    
    fn add_inductor(&mut self, inductance: f64, port1: usize, port2: usize) {
        self.components.push(Box::new(Inductor { inductance, current: 0.0 }));
        self.component_ports.push((port1, port2));
    }
    
    fn add_capacitor(&mut self, capacitance: f64, port1: usize, port2: usize) {
        self.components.push(Box::new(Capacitor { capacitance, voltage: 0.0 }));
        self.component_ports.push((port1, port2));
    }
    
    fn add_transmission_line(&mut self, port1: usize, port2: usize, delay: f64) {
        self.transmission_lines.push(TransmissionLine::new(self.z0, delay, self.dt));
        self.tl_ports.push((port1, port2));
    }
    
    fn step(&mut self) -> bool {
        let mut converged = true;
        let tolerance = 1e-6;
        
        // Multiple iterations for convergence
        for _iter in 0..10 {
            let old_voltages: Vec<f64> = self.ports.iter().map(|p| p.voltage).collect();
            
            // Process all components
            for (comp, &(p1, p2)) in self.components.iter_mut().zip(&self.component_ports) {
                if p1 < p2 {
                    let (left, right) = self.ports.split_at_mut(p2);
                    comp.process(&mut left[p1], &mut right[0], self.z0, self.dt);
                } else {
                    let (left, right) = self.ports.split_at_mut(p1);
                    comp.process(&mut right[0], &mut left[p2], self.z0, self.dt);
                }
            }
            
            // Propagate through transmission lines
            for (tl, &(p1, p2)) in self.transmission_lines.iter_mut().zip(&self.tl_ports) {
                if p1 < p2 {
                    let (left, right) = self.ports.split_at_mut(p2);
                    tl.propagate(&mut left[p1], &mut right[0]);
                } else {
                    let (left, right) = self.ports.split_at_mut(p1);
                    tl.propagate(&mut right[0], &mut left[p2]);
                }
            }
            
            // Check convergence
            let mut max_change = 0.0_f64;
            for (i, &old_v) in old_voltages.iter().enumerate() {
                let change = (self.ports[i].voltage - old_v).abs();
                max_change = max_change.max(change);
            }
            
            if max_change < tolerance {
                converged = true;
                break;
            }
        }
        
        converged
    }
    
    fn get_capacitor_voltage(&self) -> f64 {
        // Find capacitor component
        for comp in &self.components {
            if let Some(cap) = comp.as_any().downcast_ref::<Capacitor>() {
                return cap.voltage;
            }
        }
        0.0
    }
}


fn main() {
    println!("=== Proper Wave-Based Solver ===\n");
    
    // Test RLC circuit
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    let v_step = 5.0;
    
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("RLC Circuit: R={}Ω, L={}mH, C={}µF", r, l * 1000.0, c * 1e6);
    println!("Natural frequency: {:.1} Hz", f_0);
    println!("Damping ratio ζ = {:.3}", zeta);
    
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    // Create solver with ports:
    // 0: Source+, 1: Source-, 2: After source TL, 3: After R,
    // 4: After R TL, 5: After L, 6: After L TL, 7: Top of C
    let mut solver = WaveSolver::new(8, 50.0, dt);
    
    // Build circuit with transmission lines between components
    solver.add_voltage_source(0.0, 0, 1);
    solver.add_transmission_line(0, 2, 10e-12); // 10ps from source
    
    solver.add_resistor(r, 2, 3);
    solver.add_transmission_line(3, 4, 10e-12); // 10ps through R
    
    solver.add_inductor(l, 4, 5);
    solver.add_transmission_line(5, 6, 50e-12); // 50ps through L
    
    solver.add_capacitor(c, 6, 7);
    solver.add_transmission_line(7, 1, 20e-12); // 20ps to ground
    
    // Traditional solver for comparison
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    let mut v_tl_raw = vec![0.0; num_steps];
    let mut v_traditional = vec![0.0; num_steps];
    
    // Run simulation
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        // Apply step at 1ms
        if time >= 1e-3 && time < 1e-3 + dt {
            solver.components[0] = Box::new(VoltageSource { voltage: v_step });
            println!("Applied {}V step at t={:.1}ms", v_step, time * 1000.0);
        }
        
        // Wave solver step
        solver.step();
        v_tl_raw[i] = solver.get_capacitor_voltage();
        
        // Traditional solver
        if time >= 1e-3 {
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt;
            il_trad += dil_dt * dt;
        }
        v_traditional[i] = vc_trad;
    }
    
    // Apply filtering
    let bandwidth = if zeta < 1.0 {
        f_0 / (0.5 / zeta)
    } else {
        1.0 / (2.0 * PI * r * c)
    };
    let filter_cutoff = bandwidth * 100.0;
    let v_tl_filtered = apply_rc_filter(&v_tl_raw, dt, filter_cutoff);
    
    println!("\nFilter cutoff: {:.1} kHz", filter_cutoff / 1000.0);
    
    // Save results
    let mut file = File::create("tests/outputs/proper_wave_solver.csv").unwrap();
    writeln!(file, "time_ms,v_tl_raw,v_tl_filtered,v_traditional,error_percent").unwrap();
    
    let mut max_error = 0.0_f64;
    let mut rms_error = 0.0_f64;
    let mut count = 0;
    
    for i in (0..num_steps).step_by(10) {
        let time = i as f64 * dt;
        let error = if v_traditional[i] > 0.01 {
            ((v_tl_filtered[i] - v_traditional[i]) / v_traditional[i] * 100.0).abs()
        } else {
            0.0
        };
        
        if time > 2e-3 {
            max_error = max_error.max(error);
            rms_error += error * error;
            count += 1;
        }
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.2}",
                 time * 1000.0, v_tl_raw[i], v_tl_filtered[i], v_traditional[i], error).unwrap();
    }
    
    if count > 0 {
        rms_error = (rms_error / count as f64).sqrt();
    }
    
    println!("Max error: {:.2}%", max_error);
    println!("RMS error: {:.2}%", rms_error);
    println!("\nResults saved to: tests/outputs/proper_wave_solver.csv");
}

fn apply_rc_filter(input: &[f64], dt: f64, cutoff_freq: f64) -> Vec<f64> {
    let rc = 1.0 / (2.0 * PI * cutoff_freq);
    let alpha = dt / (rc + dt);
    
    let mut output = vec![0.0; input.len()];
    output[0] = input[0];
    
    for i in 1..input.len() {
        output[i] = alpha * input[i] + (1.0 - alpha) * output[i-1];
    }
    
    output
}