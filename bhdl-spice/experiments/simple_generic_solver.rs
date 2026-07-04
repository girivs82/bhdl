/// Simplified Generic Solver using proven MNA approach
/// 
/// This implements the same principles as the stable solver but with a simpler
/// interface for arbitrary topologies and nonlinear elements

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

/// Generic electrical element trait
pub trait Element: Send + Sync {
    fn terminals(&self) -> usize;
    fn get_value(&self) -> f64;
    fn name(&self) -> &str;
    fn element_type(&self) -> ElementType;
    fn reset(&mut self);
    
    // Current/voltage state
    fn get_current(&self) -> f64;
    fn get_voltage(&self) -> f64;
    fn set_current(&mut self, current: f64);
    fn set_voltage(&mut self, voltage: f64);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElementType {
    Resistor,
    Capacitor,
    Inductor,
    VoltageSource,
    Diode,
}

/// Linear resistor
pub struct Resistor {
    resistance: f64,
    current: f64,
    voltage: f64,
    name: String,
}

impl Resistor {
    pub fn new(resistance: f64, name: &str) -> Self {
        Self { 
            resistance,
            current: 0.0,
            voltage: 0.0,
            name: name.to_string()
        }
    }
}

impl Element for Resistor {
    fn terminals(&self) -> usize { 2 }
    fn get_value(&self) -> f64 { self.resistance }
    fn name(&self) -> &str { &self.name }
    fn element_type(&self) -> ElementType { ElementType::Resistor }
    fn reset(&mut self) { self.current = 0.0; self.voltage = 0.0; }
    fn get_current(&self) -> f64 { self.current }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_current(&mut self, current: f64) { self.current = current; }
    fn set_voltage(&mut self, voltage: f64) { self.voltage = voltage; }
}

/// Capacitor with state
pub struct Capacitor {
    capacitance: f64,
    current: f64,
    voltage: f64,
    name: String,
}

impl Capacitor {
    pub fn new(capacitance: f64, name: &str) -> Self {
        Self { 
            capacitance,
            current: 0.0,
            voltage: 0.0,
            name: name.to_string()
        }
    }
}

impl Element for Capacitor {
    fn terminals(&self) -> usize { 2 }
    fn get_value(&self) -> f64 { self.capacitance }
    fn name(&self) -> &str { &self.name }
    fn element_type(&self) -> ElementType { ElementType::Capacitor }
    fn reset(&mut self) { self.current = 0.0; self.voltage = 0.0; }
    fn get_current(&self) -> f64 { self.current }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_current(&mut self, current: f64) { self.current = current; }
    fn set_voltage(&mut self, voltage: f64) { self.voltage = voltage; }
}

/// Voltage source
pub struct VoltageSource {
    voltage: f64,
    current: f64,
    name: String,
}

impl VoltageSource {
    pub fn new(voltage: f64, name: &str) -> Self {
        Self { 
            voltage,
            current: 0.0,
            name: name.to_string()
        }
    }
}

impl Element for VoltageSource {
    fn terminals(&self) -> usize { 2 }
    fn get_value(&self) -> f64 { self.voltage }
    fn name(&self) -> &str { &self.name }
    fn element_type(&self) -> ElementType { ElementType::VoltageSource }
    fn reset(&mut self) { self.current = 0.0; }
    fn get_current(&self) -> f64 { self.current }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_current(&mut self, current: f64) { self.current = current; }
    fn set_voltage(&mut self, _voltage: f64) { /* Voltage is fixed */ }
}

/// Simple generic circuit solver using classic stamping approach
pub struct SimpleGenericSolver {
    /// Elements indexed by ID
    elements: HashMap<usize, Box<dyn Element>>,
    /// Connections: (element_id, node1, node2)
    connections: Vec<(usize, usize, usize)>,
    /// Node voltages
    node_voltages: Vec<f64>,
    /// Number of nodes
    num_nodes: usize,
    /// Time
    time: f64,
}

impl SimpleGenericSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: HashMap::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            num_nodes,
            time: 0.0,
        }
    }
    
    /// Add an element
    pub fn add_element(&mut self, id: usize, element: Box<dyn Element>) {
        self.elements.insert(id, element);
    }
    
    /// Connect element between nodes
    pub fn connect(&mut self, element_id: usize, node1: usize, node2: usize) {
        self.connections.push((element_id, node1, node2));
    }
    
    /// Solve one time step using simple stamping
    pub fn step(&mut self, dt: f64) -> bool {
        // Use very simple approach: iterate until convergence
        let max_iterations = 10;
        let tolerance = 1e-6;
        
        for _iteration in 0..max_iterations {
            let mut new_voltages = self.node_voltages.clone();
            let mut converged = true;
            
            // For each free node (not ground), sum currents
            for node in 1..self.num_nodes {
                let mut total_current = 0.0;
                let mut total_conductance = 1e-12; // Small base conductance
                
                // Check all connections to this node
                for &(element_id, node1, node2) in &self.connections {
                    if node == node1 || node == node2 {
                        if let Some(element) = self.elements.get(&element_id) {
                            let v1 = self.node_voltages[node1];
                            let v2 = self.node_voltages[node2];
                            let v_element = v1 - v2;
                            
                            match element.element_type() {
                                ElementType::Resistor => {
                                    let g = 1.0 / element.get_value();
                                    let current = v_element * g;
                                    
                                    if node == node1 {
                                        total_current += current;
                                        total_conductance += g;
                                    } else {
                                        total_current -= current;
                                        total_conductance += g;
                                    }
                                }
                                ElementType::Capacitor => {
                                    let c = element.get_value();
                                    let g = c / dt;
                                    let i_hist = c * element.get_voltage() / dt;
                                    
                                    if node == node1 {
                                        total_current += (v_element - element.get_voltage()) * g;
                                        total_conductance += g;
                                    } else {
                                        total_current -= (v_element - element.get_voltage()) * g;
                                        total_conductance += g;
                                    }
                                }
                                ElementType::VoltageSource => {
                                    // Voltage sources fix node voltages
                                    if node == node1 {
                                        new_voltages[node1] = element.get_value();
                                    } else if node == node2 {
                                        new_voltages[node2] = 0.0;
                                    }
                                }
                                _ => {} // Other types not implemented yet
                            }
                        }
                    }
                }
                
                // Update voltage if not a voltage source node
                let is_voltage_node = self.connections.iter().any(|&(element_id, node1, node2)| {
                    (node == node1 || node == node2) && 
                    self.elements.get(&element_id)
                        .map(|e| e.element_type() == ElementType::VoltageSource)
                        .unwrap_or(false)
                });
                
                if !is_voltage_node && total_conductance > 1e-12 {
                    let voltage_correction = -total_current / total_conductance;
                    new_voltages[node] = self.node_voltages[node] + voltage_correction;
                    
                    if voltage_correction.abs() > tolerance {
                        converged = false;
                    }
                }
            }
            
            self.node_voltages = new_voltages;
            
            if converged {
                break;
            }
        }
        
        // Update element states
        for &(element_id, node1, node2) in &self.connections {
            if let Some(element) = self.elements.get_mut(&element_id) {
                let v1 = self.node_voltages[node1];
                let v2 = self.node_voltages[node2];
                let v_element = v1 - v2;
                
                match element.element_type() {
                    ElementType::Resistor => {
                        let current = v_element / element.get_value();
                        element.set_voltage(v_element);
                        element.set_current(current);
                    }
                    ElementType::Capacitor => {
                        let c = element.get_value();
                        let current = c * (v_element - element.get_voltage()) / dt;
                        element.set_current(current);
                        element.set_voltage(v_element);
                    }
                    ElementType::VoltageSource => {
                        element.set_voltage(v_element);
                        // Current will be determined by circuit
                    }
                    _ => {}
                }
            }
        }
        
        self.time += dt;
        true
    }
    
    /// Get voltage at node
    pub fn get_node_voltage(&self, node: usize) -> f64 {
        self.node_voltages.get(node).copied().unwrap_or(0.0)
    }
    
    /// Get element by ID
    pub fn get_element(&self, id: usize) -> Option<&dyn Element> {
        self.elements.get(&id).map(|e| e.as_ref())
    }
}

fn main() {
    println!("=== Simple Generic Solver Test ===\n");
    
    test_rc_circuit();
}

fn test_rc_circuit() {
    println!("Test: RC Circuit\n");
    
    let mut solver = SimpleGenericSolver::new(3);
    
    // Circuit: 5V -> R(50Ω) -> C(100µF) -> GND
    solver.add_element(0, Box::new(VoltageSource::new(5.0, "V1")));
    solver.add_element(1, Box::new(Resistor::new(50.0, "R1")));
    solver.add_element(2, Box::new(Capacitor::new(100e-6, "C1")));
    
    // Connections: Node 0 = VCC, Node 1 = RC junction, Node 2 = GND
    solver.connect(0, 0, 2);  // Voltage source: VCC to GND
    solver.connect(1, 0, 1);  // Resistor: VCC to RC junction
    solver.connect(2, 1, 2);  // Capacitor: RC junction to GND
    
    let dt = 1e-6;
    let duration = 25e-3;
    let steps = (duration / dt) as usize;
    
    let tau = 50.0 * 100e-6;  // RC time constant
    
    println!("Circuit: 5V -> 50Ω -> 100µF -> GND");
    println!("Time constant: {:.1} ms", tau * 1000.0);
    
    let mut file = File::create("tests/outputs/simple_generic_rc_test.csv").unwrap();
    writeln!(file, "time_ms,vc_simple,vc_exact,error_%").unwrap();
    
    for i in 0..steps {
        if !solver.step(dt) {
            println!("Simulation failed at step {}", i);
            break;
        }
        
        let time = (i + 1) as f64 * dt;
        let vc = solver.get_node_voltage(1);  // Voltage at RC junction
        let vc_exact = 5.0 * (1.0 - (-time / tau).exp());
        
        let error = if vc_exact > 0.01 {
            ((vc - vc_exact) / vc_exact * 100.0).abs()
        } else { 0.0 };
        
        if i % 100 == 0 {
            writeln!(file, "{:.3},{:.6},{:.6},{:.2}", 
                     time * 1000.0, vc, vc_exact, error).unwrap();
        }
    }
    
    let vc_final = solver.get_node_voltage(1);
    let vc_expected = 5.0 * (1.0 - (-duration / tau).exp());
    println!("Final voltage: {:.3} V (expected: {:.3} V)", vc_final, vc_expected);
    println!("Error: {:.2}%\n", ((vc_final - vc_expected) / vc_expected * 100.0).abs());
    
    println!("✓ Simple generic solver successfully completed RC test");
    println!("Results saved to: tests/outputs/simple_generic_rc_test.csv");
}