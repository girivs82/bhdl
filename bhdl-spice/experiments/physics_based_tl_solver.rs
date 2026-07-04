/// Physics-Based TL Solver with Proper Bidirectional Propagation
/// 
/// Simple implementation: forward waves, component physics, backward waves, repeat until stable

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Node state with forward and backward waves
#[derive(Debug, Clone, Copy, Default)]
struct NodeState {
    v_forward: f64,   // Forward traveling voltage wave
    v_backward: f64,  // Backward traveling voltage wave
    voltage: f64,     // Total voltage (forward + backward)
    current: f64,     // Node current
}

/// Component with state
#[derive(Debug, Clone)]
enum Component {
    VoltageSource { voltage: f64 },
    Resistor { resistance: f64 },
    Inductor { inductance: f64, current: f64 },      // Stores current (magnetic energy)
    Capacitor { capacitance: f64, voltage: f64 },    // Stores voltage (electric energy)
}

/// Physics-based TL solver
struct PhysicsTLSolver {
    nodes: Vec<NodeState>,
    components: Vec<(Component, usize, usize)>, // (component, node1, node2)
    z0: f64,      // Characteristic impedance of transmission lines
    dt: f64,      // Time step
    delays: Vec<f64>, // Propagation delays
}

impl PhysicsTLSolver {
    fn new(num_nodes: usize, z0: f64, dt: f64) -> Self {
        Self {
            nodes: vec![NodeState::default(); num_nodes],
            components: Vec::new(),
            z0,
            dt,
            delays: Vec::new(),
        }
    }
    
    fn add_voltage_source(&mut self, v: f64, n1: usize, n2: usize, delay: f64) {
        self.components.push((Component::VoltageSource { voltage: v }, n1, n2));
        self.delays.push(delay);
    }
    
    fn add_resistor(&mut self, r: f64, n1: usize, n2: usize, delay: f64) {
        self.components.push((Component::Resistor { resistance: r }, n1, n2));
        self.delays.push(delay);
    }
    
    fn add_inductor(&mut self, l: f64, n1: usize, n2: usize, delay: f64) {
        self.components.push((Component::Inductor { inductance: l, current: 0.0 }, n1, n2));
        self.delays.push(delay);
    }
    
    fn add_capacitor(&mut self, c: f64, n1: usize, n2: usize, delay: f64) {
        self.components.push((Component::Capacitor { capacitance: c, voltage: 0.0 }, n1, n2));
        self.delays.push(delay);
    }
    
    fn step(&mut self, time: f64) -> bool {
        let mut converged = true;
        let tolerance = 1e-6;
        
        // Store old values for convergence check
        let old_voltages: Vec<f64> = self.nodes.iter().map(|n| n.voltage).collect();
        
        // Forward propagation: Update forward waves based on previous backward waves
        for i in 0..self.nodes.len() {
            // Forward wave is influenced by reflections from downstream
            if i > 0 {
                self.nodes[i].v_forward = self.nodes[i-1].v_backward * 0.5;
            }
        }
        
        // Process each component
        for (idx, (component, n1, n2)) in self.components.iter_mut().enumerate() {
            let delay = self.delays[idx];
            if time < delay { continue; }
            
            // Get node voltages and currents
            let v1 = self.nodes[*n1].voltage;
            let v2 = self.nodes[*n2].voltage;
            
            match component {
                Component::VoltageSource { voltage } => {
                    // Source maintains voltage
                    self.nodes[*n1].voltage = *voltage;
                    self.nodes[*n2].voltage = 0.0; // Ground
                    
                    // Forward wave from source
                    self.nodes[*n1].v_forward = *voltage;
                    self.nodes[*n1].v_backward = 0.0;
                }
                
                Component::Resistor { resistance } => {
                    // Ohm's law: I = V/R
                    let current = (v1 - v2) / *resistance;
                    
                    // Reflection coefficient
                    let gamma = (*resistance - self.z0) / (*resistance + self.z0);
                    
                    // Update waves
                    self.nodes[*n1].v_backward = self.nodes[*n1].v_forward * gamma;
                    self.nodes[*n2].v_forward = self.nodes[*n1].v_forward * (1.0 - gamma);
                    
                    // Update currents
                    self.nodes[*n1].current = current;
                    self.nodes[*n2].current = -current;
                }
                
                Component::Inductor { inductance, current: il } => {
                    // Inductor equation: V = L * di/dt
                    let v_inductor = v1 - v2;
                    let di_dt = v_inductor / *inductance;
                    *il += di_dt * self.dt;
                    
                    // Back-EMF opposes current change
                    let back_emf = *inductance * di_dt;
                    
                    // Inductor impedance for wave calculations
                    let z_l = 2.0 * *inductance / self.dt; // Discrete approximation
                    let gamma = (z_l - self.z0) / (z_l + self.z0);
                    
                    // Reflected wave includes back-EMF effect
                    self.nodes[*n1].v_backward = self.nodes[*n1].v_forward * gamma + back_emf * 0.1;
                    self.nodes[*n2].v_forward = self.nodes[*n1].v_forward * (1.0 - gamma);
                    
                    // Update currents
                    self.nodes[*n1].current = *il;
                    self.nodes[*n2].current = -*il;
                }
                
                Component::Capacitor { capacitance, voltage: vc } => {
                    // Capacitor equation: I = C * dv/dt
                    let current = (v1 - *vc) / self.z0; // Current into capacitor
                    let dv_dt = current / *capacitance;
                    *vc += dv_dt * self.dt;
                    
                    // Capacitor impedance for wave calculations
                    let z_c = self.dt / (2.0 * *capacitance); // Discrete approximation
                    let gamma = (z_c - self.z0) / (z_c + self.z0);
                    
                    // Capacitor reflects with opposite sign (stores energy)
                    self.nodes[*n1].v_backward = -self.nodes[*n1].v_forward * gamma.abs();
                    self.nodes[*n2].v_forward = 0.0; // Capacitor bottom is ground
                    self.nodes[*n2].voltage = *vc;
                    
                    // Update currents
                    self.nodes[*n1].current = current;
                    self.nodes[*n2].current = -current;
                }
            }
        }
        
        // Backward propagation: Update backward waves based on reflections
        for i in (0..self.nodes.len()-1).rev() {
            // Backward wave is influenced by reflections from upstream
            self.nodes[i].v_backward += self.nodes[i+1].v_forward * 0.5;
        }
        
        // Update total voltages
        for node in &mut self.nodes {
            node.voltage = node.v_forward + node.v_backward;
        }
        
        // Check convergence
        for i in 0..self.nodes.len() {
            let change = (self.nodes[i].voltage - old_voltages[i]).abs();
            if change > tolerance {
                converged = false;
            }
        }
        
        converged
    }
    
    fn get_capacitor_voltage(&self) -> f64 {
        for (component, _, _) in &self.components {
            if let Component::Capacitor { voltage, .. } = component {
                return *voltage;
            }
        }
        0.0
    }
}

fn main() {
    println!("=== Physics-Based TL Solver with Bidirectional Propagation ===\n");
    
    // Test RLC circuit
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    let v_step = 5.0;
    
    // Circuit characteristics
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("RLC Circuit: R={}Ω, L={}mH, C={}µF", r, l * 1000.0, c * 1e6);
    println!("Natural frequency: {:.1} Hz", f_0);
    println!("Damping ratio ζ = {:.3}", zeta);
    
    // Simulation parameters
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    // Create solver with 4 nodes: Source+, Source-, R-L junction, L-C junction
    let mut solver = PhysicsTLSolver::new(4, 50.0, dt);
    
    // Build circuit: Source -> R -> L -> C -> Ground
    solver.add_voltage_source(0.0, 0, 1, 0.0);        // Initially 0V
    solver.add_resistor(r, 0, 2, 10e-12);             // 10ps delay
    solver.add_inductor(l, 2, 3, 50e-12);             // 50ps delay
    solver.add_capacitor(c, 3, 1, 20e-12);            // 20ps delay
    
    // Traditional solver for comparison
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    // Storage for results
    let mut v_tl_raw = vec![0.0; num_steps];
    let mut v_traditional = vec![0.0; num_steps];
    
    // Run simulation
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        // Apply step at 1ms
        if time >= 1e-3 && time < 1e-3 + dt {
            solver.components[0] = (Component::VoltageSource { voltage: v_step }, 0, 1);
            println!("Applied {}V step at t={:.1}ms", v_step, time * 1000.0);
        }
        
        // Run multiple iterations per time step for convergence
        for _ in 0..5 {
            if solver.step(time) {
                break; // Converged
            }
        }
        
        // Traditional solver
        if time >= 1e-3 {
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt;
            il_trad += dil_dt * dt;
        }
        
        v_tl_raw[i] = solver.get_capacitor_voltage();
        v_traditional[i] = vc_trad;
    }
    
    // Apply adaptive filtering
    let bandwidth = if zeta < 1.0 {
        f_0 / (0.5 / zeta)
    } else {
        1.0 / (2.0 * PI * r * c)
    };
    let filter_cutoff = bandwidth * 100.0;
    let v_tl_filtered = apply_rc_filter(&v_tl_raw, dt, filter_cutoff);
    
    println!("\nFilter cutoff: {:.1} kHz", filter_cutoff / 1000.0);
    
    // Save results
    let mut file = File::create("tests/outputs/physics_based_tl.csv").unwrap();
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
    println!("\nResults saved to: tests/outputs/physics_based_tl.csv");
}

/// RC filter from proven implementation
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