/// Clean Two-Port Superposition Wave Solver
/// 
/// Fresh implementation of 2-port wave propagation

use std::fs::File;
use std::io::Write;

/// Wave components at a port
#[derive(Debug, Clone, Copy, Default)]
struct PortWave {
    incident: f64,   // a (forward wave into port)
    reflected: f64,  // b (backward wave from port)
}

/// Two-port network with S-parameters
struct TwoPortNetwork {
    name: String,
    // S-matrix elements
    s11: f64,  // Input reflection
    s12: f64,  // Reverse transmission  
    s21: f64,  // Forward transmission
    s22: f64,  // Output reflection
    
    // Port states
    port1: PortWave,
    port2: PortWave,
}

impl TwoPortNetwork {
    fn new_resistor(name: &str, r: f64, z0: f64) -> Self {
        // Series resistor S-parameters
        let z = r;
        let gamma = (z - z0) / (z + z0);
        
        Self {
            name: name.to_string(),
            s11: gamma,
            s12: 1.0 - gamma,
            s21: 1.0 - gamma, 
            s22: gamma,
            port1: PortWave::default(),
            port2: PortWave::default(),
        }
    }
    
    fn new_transmission_line(name: &str) -> Self {
        // Ideal matched transmission line
        Self {
            name: name.to_string(),
            s11: 0.0,
            s12: 1.0,
            s21: 1.0,
            s22: 0.0,
            port1: PortWave::default(),
            port2: PortWave::default(),
        }
    }
    
    fn update(&mut self) {
        // Apply S-matrix: [b1, b2] = S * [a1, a2]
        let a1 = self.port1.incident;
        let a2 = self.port2.incident;
        
        self.port1.reflected = self.s11 * a1 + self.s12 * a2;
        self.port2.reflected = self.s21 * a1 + self.s22 * a2;
    }
    
    fn voltage_at_port1(&self, z0: f64) -> f64 {
        // V = sqrt(Z0) * (a + b)
        z0.sqrt() * (self.port1.incident + self.port1.reflected)
    }
    
    fn voltage_at_port2(&self, z0: f64) -> f64 {
        z0.sqrt() * (self.port2.incident + self.port2.reflected)
    }
}

/// Simple cascade solver for series elements
struct CascadeSolver {
    elements: Vec<TwoPortNetwork>,
    z0: f64,
    dt: f64,
    time: f64,
}

impl CascadeSolver {
    fn new(z0: f64, dt: f64) -> Self {
        Self {
            elements: Vec::new(),
            z0,
            dt,
            time: 0.0,
        }
    }
    
    fn add_element(&mut self, element: TwoPortNetwork) {
        self.elements.push(element);
    }
    
    fn step(&mut self, v_source: f64) -> f64 {
        // Inject source wave
        if !self.elements.is_empty() {
            // Convert voltage to wave amplitude
            let a_source = v_source / (2.0 * self.z0.sqrt());
            self.elements[0].port1.incident = a_source;
        }
        
        // Forward pass: propagate waves left to right
        for i in 0..self.elements.len() {
            self.elements[i].update();
            
            // Connect to next element
            if i + 1 < self.elements.len() {
                self.elements[i + 1].port1.incident = self.elements[i].port2.reflected;
            }
        }
        
        // Backward pass: propagate reflections right to left
        for i in (0..self.elements.len()).rev() {
            if i + 1 < self.elements.len() {
                self.elements[i].port2.incident = self.elements[i + 1].port1.reflected;
            }
            self.elements[i].update();
        }
        
        self.time += self.dt;
        
        // Return voltage at last element output
        if let Some(last) = self.elements.last() {
            last.voltage_at_port2(self.z0)
        } else {
            0.0
        }
    }
}

fn main() {
    println!("=== Clean Two-Port Wave Solver ===\n");
    
    // Test parameters
    let r_source = 10.0;
    let r_load = 50.0;
    let v_step = 5.0;
    let z0 = 50.0;
    
    println!("Test circuit: {}V source -> {}Ω -> {}Ω load", v_step, r_source, r_load);
    println!("Transmission line impedance: {}Ω", z0);
    
    // Create solver
    let dt = 1e-12;  // 1ps timestep
    let mut solver = CascadeSolver::new(z0, dt);
    
    // Build circuit
    solver.add_element(TwoPortNetwork::new_resistor("R_source", r_source, z0));
    solver.add_element(TwoPortNetwork::new_transmission_line("TL1"));
    solver.add_element(TwoPortNetwork::new_resistor("R_load", r_load, z0));
    
    // Expected steady-state from voltage divider
    let v_expected = v_step * r_load / (r_source + r_load);
    println!("Expected steady-state voltage: {:.3}V", v_expected);
    
    // Run simulation
    let duration = 1e-9;  // 1ns
    let steps = (duration / dt) as usize;
    let mut output = File::create("tests/outputs/clean_two_port.csv").unwrap();
    writeln!(output, "time_ps,v_out,v_expected").unwrap();
    
    for i in 0..steps {
        let time = i as f64 * dt;
        let v_out = solver.step(v_step);
        
        if i % 10 == 0 {
            writeln!(output, "{:.1},{:.6},{:.6}", 
                     time * 1e12, v_out, v_expected).unwrap();
        }
    }
    
    println!("\nSimulation complete. Results saved to: tests/outputs/clean_two_port.csv");
    
    // Check final value
    let v_final = solver.step(v_step);
    let error = ((v_final - v_expected) / v_expected * 100.0).abs();
    println!("Final voltage: {:.6}V (error: {:.2}%)", v_final, error);
    
    println!("\nNext: Add transmission line delays and energy storage elements");
}