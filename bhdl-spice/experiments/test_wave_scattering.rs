/// Test wave scattering approach for RC circuit transient analysis
/// 
/// This implements the generic wave scattering physics we derived:
/// - Incident waves at each node
/// - Wave scattering based on impedance ratios
/// - No arbitrary damping factors - pure physics

use bhdl_spice::perturbation::simple_wave::*;
use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Wave Scattering Physics Test ===");
    
    test_rc_wave_scattering();
}

fn test_rc_wave_scattering() {
    println!("\n=== RC Circuit with Wave Scattering ===");
    
    // Circuit: 5V step -> R(1kΩ) -> C(1μF) -> GND
    // Expected: V_C(t) = 5V * (1 - e^(-t/RC))
    // Time constant τ = RC = 1kΩ * 1μF = 1ms
    
    let mut circuit = WaveScatteringCircuit::new(0); // Ground = node 0
    circuit.set_time_step(10e-6); // 10μs time step
    
    // Add nodes
    circuit.add_node(1); // Source positive
    circuit.add_node(2); // Between R and C
    
    // Add components - DC step input
    circuit.add_component(ComponentType::VoltageSource { voltage: 5.0, internal_resistance: 1.0 }, 1, 0);
    circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 1, 2);
    circuit.add_component(ComponentType::Capacitor { capacitance: 1e-6 }, 2, 0);
    
    // Run transient analysis for 5ms (5 time constants)
    let results = circuit.solve_transient_with_scattering(5e-3);
    
    // Write results to CSV
    let mut file = File::create("tests/outputs/wave_scattering_response.csv").expect("Could not create file");
    writeln!(file, "time_ms,v_source,v_capacitor,v_theory").expect("Could not write header");
    
    let tau = 1e-3; // Time constant = 1ms
    
    println!("Wave Scattering RC Response Results:");
    println!("Time Constant τ = {:.1}ms", tau * 1000.0);
    
    for (i, (time, voltages)) in results.iter().enumerate() {
        let v_source = voltages.get(&1).copied().unwrap_or(0.0);
        let v_capacitor = voltages.get(&2).copied().unwrap_or(0.0);
        
        // Theoretical response: V_C(t) = V_s * (1 - e^(-t/τ))
        let v_theory = 5.0 * (1.0 - (-time / tau).exp());
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6}", 
                 time * 1000.0, v_source, v_capacitor, v_theory).expect("Could not write data");
        
        // Print every 100 steps
        if i % 100 == 0 {
            println!("  t = {:.2}ms: V_C = {:.3}V (theory: {:.3}V), error = {:.1}mV", 
                     time * 1000.0, v_capacitor, v_theory, (v_capacitor - v_theory).abs() * 1000.0);
        }
    }
    
    println!("Results saved to tests/outputs/wave_scattering_response.csv");
}

/// Wave scattering circuit implementation
pub struct WaveScatteringCircuit {
    pub nodes: HashMap<usize, Node>,
    pub components: HashMap<usize, Component>,
    pub ground_node: usize,
    pub tolerance: f64,
    pub time: f64,
    pub time_step: f64,
}

impl WaveScatteringCircuit {
    pub fn new(ground_node: usize) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(ground_node, Node::new(ground_node));
        
        Self {
            nodes,
            components: HashMap::new(),
            ground_node,
            tolerance: 1e-9,
            time: 0.0,
            time_step: 1e-6,
        }
    }
    
    pub fn set_time_step(&mut self, dt: f64) {
        self.time_step = dt;
    }
    
    pub fn add_node(&mut self, node_id: usize) {
        if !self.nodes.contains_key(&node_id) {
            self.nodes.insert(node_id, Node::new(node_id));
        }
    }
    
    pub fn add_component(&mut self, comp_type: ComponentType, pos_node: usize, neg_node: usize) -> usize {
        self.add_node(pos_node);
        self.add_node(neg_node);
        
        let comp_id = self.components.len();
        let component = Component::new(comp_id, comp_type, pos_node, neg_node);
        
        self.nodes.get_mut(&pos_node).unwrap().connections.push(comp_id);
        self.nodes.get_mut(&neg_node).unwrap().connections.push(comp_id);
        
        self.components.insert(comp_id, component);
        comp_id
    }
    
    /// Solve transient using pure wave scattering physics
    pub fn solve_transient_with_scattering(&mut self, duration: f64) -> Vec<(f64, HashMap<usize, f64>)> {
        let mut results = Vec::new();
        let num_steps = (duration / self.time_step) as usize;
        
        println!("Starting wave scattering transient analysis:");
        println!("  Duration: {:.3}ms", duration * 1000.0);
        println!("  Time step: {:.3}μs", self.time_step * 1e6);
        println!("  Steps: {}", num_steps);
        println!("  Physics: Pure wave scattering");
        
        self.time = 0.0;
        
        // Generate initial companion models
        for component in self.components.values_mut() {
            component.generate_companion_model(self.time_step);
        }
        
        // Initial state
        let mut initial_voltages = HashMap::new();
        for (&node_id, node) in &self.nodes {
            initial_voltages.insert(node_id, node.voltage);
        }
        results.push((0.0, initial_voltages));
        
        // Time stepping with wave scattering
        for step in 1..=num_steps {
            self.time = step as f64 * self.time_step;
            
            // Store previous values
            for component in self.components.values_mut() {
                component.store_previous_values();
            }
            
            // Generate new companion models
            for component in self.components.values_mut() {
                component.generate_companion_model(self.time_step);
                
                // Update AC source voltages
                if let ComponentType::AcVoltageSource { amplitude, frequency, phase, .. } = component.comp_type {
                    use std::f64::consts::PI;
                    let omega = 2.0 * PI * frequency;
                    let ac_voltage = amplitude * (omega * self.time + phase).sin();
                    component.companion.voltage_source = ac_voltage;
                }
            }
            
            // Solve using wave scattering
            let converged = self.solve_wave_scattering_step();
            
            if !converged {
                println!("Warning: Wave scattering failed to converge at t = {:.6}s", self.time);
            }
            
            // Record results
            let mut voltages = HashMap::new();
            for (&node_id, node) in &self.nodes {
                voltages.insert(node_id, node.voltage);
            }
            results.push((self.time, voltages));
            
            if step % (num_steps / 10).max(1) == 0 {
                println!("  Progress: {:.1}%", 100.0 * step as f64 / num_steps as f64);
            }
        }
        
        println!("Wave scattering analysis complete.");
        results
    }
    
    /// Pure wave scattering physics solver
    fn solve_wave_scattering_step(&mut self) -> bool {
        let max_iterations = 5; // Physics should converge quickly
        
        // Ground is always 0V
        self.nodes.get_mut(&self.ground_node).unwrap().voltage = 0.0;
        
        for _iteration in 0..max_iterations {
            let mut max_change: f64 = 0.0;
            let old_voltages: HashMap<usize, f64> = self.nodes.iter()
                .map(|(&id, node)| (id, node.voltage))
                .collect();
            
            // Collect wave scattering data for all nodes first
            let mut node_voltage_updates = Vec::new();
            
            for (&node_id, node) in &self.nodes {
                if node_id == self.ground_node {
                    continue;
                }
                
                // Collect incident waves and impedances for this node
                let mut incident_waves = Vec::new();
                let mut impedances = Vec::new();
                
                for &comp_id in &node.connections {
                    let component = &self.components[&comp_id];
                    let impedance = component.effective_resistance();
                    
                    // Calculate incident wave from this component
                    let incident_wave = match component.comp_type {
                        ComponentType::VoltageSource { .. } | ComponentType::AcVoltageSource { .. } => {
                            if component.nodes[0] == node_id {
                                // Positive terminal of voltage source
                                component.companion.voltage_source
                            } else {
                                // Negative terminal of voltage source
                                0.0
                            }
                        },
                        _ => {
                            // Passive component: Norton equivalent creates incident wave
                            let norton_current = component.norton_current();
                            let norton_voltage = norton_current * impedance;
                            
                            if component.nodes[0] == node_id {
                                norton_voltage
                            } else {
                                -norton_voltage
                            }
                        }
                    };
                    
                    incident_waves.push(incident_wave);
                    impedances.push(impedance);
                }
                
                // Apply generic wave scattering formula
                let new_voltage = self.scatter_waves_at_node(&incident_waves, &impedances);
                let change = (new_voltage - node.voltage).abs();
                max_change = max_change.max(change);
                
                node_voltage_updates.push((node_id, new_voltage, incident_waves, impedances));
            }
            
            // Apply voltage updates
            for (node_id, new_voltage, incident_waves, impedances) in node_voltage_updates {
                if let Some(node) = self.nodes.get_mut(&node_id) {
                    node.voltage = new_voltage;
                    
                    // Debug output for capacitor node
                    if node_id == 2 && self.time < 0.001 {
                        println!("t={:.6}s: Node 2: V={:.6}V, incidents={:?}, Z={:?}", 
                                 self.time, node.voltage, incident_waves, impedances);
                    }
                }
            }
            
            // Update component voltages and currents
            for component in self.components.values_mut() {
                let v_pos = self.nodes[&component.nodes[0]].voltage;
                let v_neg = self.nodes[&component.nodes[1]].voltage;
                component.voltage = v_pos - v_neg;
                component.current = component.voltage / component.effective_resistance();
            }
            
            // Check convergence
            if max_change < self.tolerance {
                return true;
            }
        }
        
        false
    }
    
    /// Generic wave scattering physics: Vnode = (∑ᵢ Vi⁺/Zi) / (∑ᵢ 1/Zi)
    fn scatter_waves_at_node(&self, incident_waves: &[f64], impedances: &[f64]) -> f64 {
        if incident_waves.len() != impedances.len() || incident_waves.is_empty() {
            return 0.0;
        }
        
        let numerator: f64 = incident_waves.iter()
            .zip(impedances.iter())
            .map(|(v_inc, z)| v_inc / z)
            .sum();
        
        let denominator: f64 = impedances.iter()
            .map(|z| 1.0 / z)
            .sum();
        
        if denominator > 1e-12 {
            numerator / denominator
        } else {
            0.0
        }
    }
}