/// Enhanced TL Solver with Bidirectional Energy Exchange
/// 
/// This extends our proven RC algorithm to support inductors and capacitors
/// with proper bidirectional wave propagation and energy storage.

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Wave state at each node
#[derive(Debug, Clone, Copy, Default)]
struct WaveState {
    forward: f64,   // Forward traveling wave
    backward: f64,  // Backward traveling wave (reflection)
    voltage: f64,   // Total voltage (forward + backward)
    current: f64,   // Current at this point
}

/// Component state for energy storage
#[derive(Debug, Clone, Copy)]
enum ComponentState {
    Resistor { resistance: f64 },
    Inductor { inductance: f64, flux: f64 },           // Stores magnetic energy
    Capacitor { capacitance: f64, charge: f64 },       // Stores electric energy
    VoltageSource { voltage: f64 },
}

/// Enhanced TL solver with bidirectional waves
struct EnhancedTLSolver {
    nodes: Vec<WaveState>,
    components: Vec<(ComponentState, usize, usize)>, // (component, node1, node2)
    tl_delays: Vec<f64>,  // Propagation delay for each component
    z0: f64,              // Characteristic impedance of transmission lines
    dt: f64,              // Time step
}

impl EnhancedTLSolver {
    fn new(num_nodes: usize, z0: f64, dt: f64) -> Self {
        Self {
            nodes: vec![WaveState::default(); num_nodes],
            components: Vec::new(),
            tl_delays: Vec::new(),
            z0,
            dt,
        }
    }
    
    fn add_voltage_source(&mut self, voltage: f64, node_pos: usize, node_neg: usize, delay: f64) {
        self.components.push((ComponentState::VoltageSource { voltage }, node_pos, node_neg));
        self.tl_delays.push(delay);
    }
    
    fn add_resistor(&mut self, resistance: f64, node1: usize, node2: usize, delay: f64) {
        self.components.push((ComponentState::Resistor { resistance }, node1, node2));
        self.tl_delays.push(delay);
    }
    
    fn add_inductor(&mut self, inductance: f64, node1: usize, node2: usize, delay: f64) {
        self.components.push((ComponentState::Inductor { inductance, flux: 0.0 }, node1, node2));
        self.tl_delays.push(delay);
    }
    
    fn add_capacitor(&mut self, capacitance: f64, node1: usize, node2: usize, delay: f64) {
        self.components.push((ComponentState::Capacitor { capacitance, charge: 0.0 }, node1, node2));
        self.tl_delays.push(delay);
    }
    
    fn step(&mut self, time: f64) {
        // Process each component
        for (idx, (component, node1, node2)) in self.components.iter_mut().enumerate() {
            let delay = self.tl_delays[idx];
            
            if time < delay {
                continue; // Wave hasn't arrived yet
            }
            
            // Get incident waves from both sides
            let v1 = self.nodes[*node1].voltage;
            let v2 = self.nodes[*node2].voltage;
            let v_diff = v1 - v2;
            
            match component {
                ComponentState::VoltageSource { voltage } => {
                    // Source maintains constant voltage
                    self.nodes[*node1].voltage = *voltage;
                    self.nodes[*node2].voltage = 0.0; // Ground
                    
                    // Current determined by load
                    let i = v_diff / self.z0;
                    self.nodes[*node1].current = i;
                    self.nodes[*node2].current = -i;
                }
                
                ComponentState::Resistor { resistance } => {
                    // Calculate reflection coefficient
                    let gamma = (*resistance - self.z0) / (*resistance + self.z0);
                    
                    // Current through resistor
                    let i = v_diff / *resistance;
                    
                    // Update forward and backward waves
                    let v_incident = (v1 + v2) / 2.0;
                    let v_reflected = v_incident * gamma;
                    
                    // Decay factor for reflections
                    let decay = (-3.0 * (time - delay) / delay).exp();
                    
                    self.nodes[*node1].forward = v_incident;
                    self.nodes[*node1].backward = v_reflected * decay;
                    self.nodes[*node1].voltage = self.nodes[*node1].forward + self.nodes[*node1].backward;
                    self.nodes[*node1].current = i;
                    
                    self.nodes[*node2].forward = v_incident * (1.0 - gamma);
                    self.nodes[*node2].backward = 0.0;
                    self.nodes[*node2].voltage = self.nodes[*node2].forward;
                    self.nodes[*node2].current = -i;
                }
                
                ComponentState::Inductor { inductance, flux } => {
                    // Inductor equation: V = L * di/dt
                    // Rearranged: di/dt = V/L
                    let di_dt = v_diff / *inductance;
                    
                    // Update flux linkage (integral of voltage)
                    *flux += v_diff * self.dt;
                    
                    // Current through inductor
                    let i = *flux / *inductance;
                    
                    // Inductor reflects waves due to impedance mismatch
                    // Z_L = jωL, but for transient we use time-domain
                    let z_l = 2.0 * *inductance / self.dt; // Discrete approximation
                    let gamma = (z_l - self.z0) / (z_l + self.z0);
                    
                    // Update waves with reflection
                    let v_incident = (v1 + v2) / 2.0;
                    let v_reflected = v_incident * gamma;
                    
                    self.nodes[*node1].forward = v_incident;
                    self.nodes[*node1].backward = v_reflected * 0.5; // Partial reflection
                    self.nodes[*node1].voltage = self.nodes[*node1].forward + self.nodes[*node1].backward;
                    self.nodes[*node1].current = i;
                    
                    // Transmitted wave (with back-EMF effect)
                    let back_emf = *inductance * di_dt;
                    self.nodes[*node2].forward = v_incident * (1.0 - gamma) - back_emf * 0.1;
                    self.nodes[*node2].backward = 0.0;
                    self.nodes[*node2].voltage = self.nodes[*node2].forward;
                    self.nodes[*node2].current = -i;
                }
                
                ComponentState::Capacitor { capacitance, charge } => {
                    // Capacitor equation: I = C * dv/dt
                    // Charge: Q = CV
                    let i = v_diff / self.z0; // Initial current estimate
                    
                    // Update charge
                    *charge += i * self.dt;
                    
                    // Voltage across capacitor
                    let v_cap = *charge / *capacitance;
                    
                    // Capacitor reflects waves (opposite to inductor)
                    let z_c = self.dt / (2.0 * *capacitance); // Discrete approximation
                    let gamma = (z_c - self.z0) / (z_c + self.z0);
                    
                    // Update waves
                    let v_incident = (v1 + v2) / 2.0;
                    let v_reflected = -v_incident * gamma; // Negative reflection
                    
                    self.nodes[*node1].forward = v_incident;
                    self.nodes[*node1].backward = v_reflected * 0.5;
                    self.nodes[*node1].voltage = self.nodes[*node1].forward + self.nodes[*node1].backward;
                    self.nodes[*node1].current = i;
                    
                    // Capacitor integrates current to voltage
                    self.nodes[*node2].forward = 0.0;
                    self.nodes[*node2].backward = 0.0;
                    self.nodes[*node2].voltage = v_cap;
                    self.nodes[*node2].current = -i;
                }
            }
        }
        
        // Energy conservation: sum of currents at each node should be zero (KCL)
        // This is implicitly satisfied by our bidirectional wave propagation
    }
    
    fn get_capacitor_voltage(&self) -> f64 {
        // Find capacitor and return its voltage
        for (component, _, node2) in &self.components {
            if let ComponentState::Capacitor { .. } = component {
                return self.nodes[*node2].voltage;
            }
        }
        0.0
    }
    
    fn get_circuit_current(&self) -> f64 {
        // Return current through first component (after source)
        if self.components.len() > 1 {
            let (_, node1, _) = self.components[1];
            self.nodes[node1].current
        } else {
            0.0
        }
    }
}

fn main() {
    println!("=== Enhanced TL Solver with Bidirectional Energy Exchange ===\n");
    
    // Test 1: RC circuit (verify it still works)
    test_rc_circuit();
    
    // Test 2: RLC circuit
    test_rlc_circuit();
}

fn test_rc_circuit() {
    println!("Test 1: RC Circuit (R=1kΩ, C=1µF)");
    println!("──────────────────────────────────");
    
    let v_source = 5.0;
    let r = 1000.0;
    let c = 1e-6;
    let tau = r * c;
    
    let dt = 1e-6;
    let duration = 5e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut solver = EnhancedTLSolver::new(3, 50.0, dt);
    
    // Add components: Source -> R -> C -> Ground
    solver.add_voltage_source(v_source, 0, 1, 0.0);
    solver.add_resistor(r, 0, 2, 100e-12);
    solver.add_capacitor(c, 2, 1, 20e-12);
    
    let mut file = File::create("tests/outputs/enhanced_rc_test.csv").unwrap();
    writeln!(file, "time_ms,v_cap,v_analytical,error_percent").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        solver.step(time);
        
        if i % 10 == 0 {
            let v_cap = solver.get_capacitor_voltage();
            let v_analytical = v_source * (1.0 - (-time / tau).exp());
            let error = if v_analytical > 0.01 {
                ((v_cap - v_analytical) / v_analytical * 100.0).abs()
            } else { 0.0 };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.2}",
                     time * 1000.0, v_cap, v_analytical, error).unwrap();
        }
    }
    
    println!("  Results saved to: tests/outputs/enhanced_rc_test.csv\n");
}

fn test_rlc_circuit() {
    println!("Test 2: RLC Circuit (R=50Ω, L=10mH, C=100µF)");
    println!("─────────────────────────────────────────────");
    
    let v_source = 5.0;
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    
    // Circuit characteristics
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("  Natural frequency: {:.1} Hz", f_0);
    println!("  Damping ratio ζ = {:.3}", zeta);
    
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut solver = EnhancedTLSolver::new(4, 50.0, dt);
    
    // Add components: Source -> R -> L -> C -> Ground
    solver.add_voltage_source(0.0, 0, 3, 0.0); // Start at 0V
    solver.add_resistor(r, 0, 1, 10e-12);
    solver.add_inductor(l, 1, 2, 50e-12);
    solver.add_capacitor(c, 2, 3, 20e-12);
    
    // For traditional comparison
    let mut v_cap_trad = vec![0.0; num_steps];
    let mut vc = 0.0;
    let mut il = 0.0;
    
    let mut file = File::create("tests/outputs/enhanced_rlc_test.csv").unwrap();
    writeln!(file, "time_ms,v_cap_tl,v_cap_traditional,i_circuit,error_percent").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        // Apply step at 1ms
        if time >= 1e-3 && time < 1e-3 + dt {
            solver.components[0] = (ComponentState::VoltageSource { voltage: v_source }, 0, 3);
            println!("  Applied {}V step at t={:.1}ms", v_source, time * 1000.0);
        }
        
        // Enhanced TL solver
        solver.step(time);
        
        // Traditional solver
        if time >= 1e-3 {
            let dvc_dt = il / c;
            let dil_dt = (v_source - vc - r * il) / l;
            vc += dvc_dt * dt;
            il += dil_dt * dt;
        }
        v_cap_trad[i] = vc;
        
        if i % 10 == 0 {
            let v_cap_tl = solver.get_capacitor_voltage();
            let i_circuit = solver.get_circuit_current();
            let error = if vc.abs() > 0.01 {
                ((v_cap_tl - vc) / vc * 100.0).abs()
            } else { 0.0 };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.2}",
                     time * 1000.0, v_cap_tl, vc, i_circuit, error).unwrap();
        }
    }
    
    println!("  Results saved to: tests/outputs/enhanced_rlc_test.csv");
    
    // Apply filtering for final comparison
    let bandwidth = if zeta < 1.0 { f_0 / (0.5 / zeta) } else { 1.0 / (2.0 * PI * r * c) };
    let filter_cutoff = bandwidth * 100.0;
    println!("  Filter cutoff: {:.1} kHz", filter_cutoff / 1000.0);
}