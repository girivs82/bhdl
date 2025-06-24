/// Perturbation-based circuit simulation engine
/// 
/// This module implements a physics-based simulation approach where changes
/// propagate through the circuit as perturbations, similar to how actual
/// electrical signals propagate. This approach is highly parallelizable
/// and maps well to GPU architectures.

pub mod physics_based;
pub mod stable_solver;
pub mod gpu_ready;
pub mod bidirectional_waves;
pub mod wave_rlc;
pub mod generic_wave;
pub mod simple_wave;
pub mod unified_wave_solver;

pub use physics_based::{ElectricalState, WaveComponent};
pub use unified_wave_solver::{UnifiedWaveSolver, SolverConfig, SolverBuilder};

use std::collections::HashMap;

/// Represents a change in electrical state at a node
#[derive(Debug, Clone, Copy, Default)]
pub struct Perturbation {
    /// Change in voltage at this time step
    pub delta_voltage: f64,
    /// Change in current at this time step
    pub delta_current: f64,
    /// Accumulated voltage (for integration)
    pub voltage: f64,
    /// Accumulated current (for integration)
    pub current: f64,
}

impl Perturbation {
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Check if perturbation is below threshold
    pub fn is_settled(&self, voltage_threshold: f64, current_threshold: f64) -> bool {
        self.delta_voltage.abs() < voltage_threshold && 
        self.delta_current.abs() < current_threshold
    }
}

/// Node in the circuit that accumulates perturbations
#[derive(Debug, Clone)]
pub struct Node {
    pub id: usize,
    pub voltage: f64,
    pub perturbation: Perturbation,
    /// Connected components (component_id, pin_index)
    pub connections: Vec<(usize, usize)>,
}

impl Node {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            voltage: 0.0,
            perturbation: Perturbation::new(),
            connections: Vec::new(),
        }
    }
    
    /// Apply Kirchhoff's Current Law
    pub fn apply_kcl(&mut self, incoming_currents: &[f64]) {
        let total_current: f64 = incoming_currents.iter().sum();
        // In steady state, sum of currents must be zero
        // Any imbalance creates a voltage perturbation
        // ΔV = ΔQ/C where C is parasitic capacitance
        // For now, use a small virtual capacitance
        const VIRTUAL_CAPACITANCE: f64 = 1e-12; // 1pF
        self.perturbation.delta_voltage = total_current * 1e-9 / VIRTUAL_CAPACITANCE;
    }
}

/// Trait for components that respond to perturbations
pub trait PerturbationModel: Send + Sync {
    /// Component's response to input perturbation
    fn forward_perturb(&mut self, input: &Perturbation, dt: f64) -> Perturbation;
    
    /// Component's response to output perturbation (reflection)
    fn backward_perturb(&mut self, output: &Perturbation, dt: f64) -> Perturbation;
    
    /// Update internal state after perturbation
    fn update_state(&mut self, dt: f64);
    
    /// Get current flowing through component
    fn get_current(&self) -> f64;
    
    /// Get voltage across component
    fn get_voltage(&self) -> f64;
    
    /// Reset component state
    fn reset(&mut self);
}

/// Resistor perturbation model
pub struct ResistorModel {
    resistance: f64,
    current: f64,
    voltage: f64,
}

impl ResistorModel {
    pub fn new(resistance: f64) -> Self {
        Self {
            resistance,
            current: 0.0,
            voltage: 0.0,
        }
    }
}

impl PerturbationModel for ResistorModel {
    fn forward_perturb(&mut self, input: &Perturbation, _dt: f64) -> Perturbation {
        // Ohm's law: V = IR, so ΔI = ΔV/R
        let delta_current = input.delta_voltage / self.resistance;
        self.current += delta_current;
        self.voltage = input.voltage;
        
        Perturbation {
            delta_voltage: 0.0, // Resistor doesn't change voltage
            delta_current,
            voltage: input.voltage,
            current: self.current,
        }
    }
    
    fn backward_perturb(&mut self, output: &Perturbation, _dt: f64) -> Perturbation {
        // If output current changes, voltage drop changes
        let delta_voltage = output.delta_current * self.resistance;
        
        Perturbation {
            delta_voltage,
            delta_current: 0.0,
            voltage: self.voltage + delta_voltage,
            current: output.current,
        }
    }
    
    fn update_state(&mut self, _dt: f64) {
        self.voltage = self.current * self.resistance;
    }
    
    fn get_current(&self) -> f64 { self.current }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn reset(&mut self) {
        self.current = 0.0;
        self.voltage = 0.0;
    }
}

/// Inductor perturbation model
pub struct InductorModel {
    inductance: f64,
    current: f64,
    voltage: f64,
    flux: f64, // Magnetic flux linkage
}

impl InductorModel {
    pub fn new(inductance: f64) -> Self {
        Self {
            inductance,
            current: 0.0,
            voltage: 0.0,
            flux: 0.0,
        }
    }
}

impl PerturbationModel for InductorModel {
    fn forward_perturb(&mut self, input: &Perturbation, dt: f64) -> Perturbation {
        // V = L * di/dt, so di = V * dt / L
        let delta_current = input.delta_voltage * dt / self.inductance;
        self.current += delta_current;
        self.voltage = input.voltage;
        
        Perturbation {
            delta_voltage: 0.0,
            delta_current,
            voltage: input.voltage,
            current: self.current,
        }
    }
    
    fn backward_perturb(&mut self, output: &Perturbation, dt: f64) -> Perturbation {
        // Back-EMF opposes current change: V = -L * di/dt
        let back_emf = -self.inductance * output.delta_current / dt;
        
        Perturbation {
            delta_voltage: back_emf,
            delta_current: 0.0,
            voltage: self.voltage + back_emf,
            current: output.current,
        }
    }
    
    fn update_state(&mut self, dt: f64) {
        self.flux += self.voltage * dt;
        self.current = self.flux / self.inductance;
    }
    
    fn get_current(&self) -> f64 { self.current }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn reset(&mut self) {
        self.current = 0.0;
        self.voltage = 0.0;
        self.flux = 0.0;
    }
}

/// Capacitor perturbation model
pub struct CapacitorModel {
    capacitance: f64,
    voltage: f64,
    current: f64,
    charge: f64,
}

impl CapacitorModel {
    pub fn new(capacitance: f64) -> Self {
        Self {
            capacitance,
            voltage: 0.0,
            current: 0.0,
            charge: 0.0,
        }
    }
}

impl PerturbationModel for CapacitorModel {
    fn forward_perturb(&mut self, input: &Perturbation, dt: f64) -> Perturbation {
        // I = C * dv/dt
        self.current = input.delta_current;
        let delta_voltage = self.current * dt / self.capacitance;
        self.voltage += delta_voltage;
        
        Perturbation {
            delta_voltage,
            delta_current: 0.0,
            voltage: self.voltage,
            current: self.current,
        }
    }
    
    fn backward_perturb(&mut self, output: &Perturbation, dt: f64) -> Perturbation {
        // Current through capacitor changes with voltage
        let delta_current = self.capacitance * output.delta_voltage / dt;
        
        Perturbation {
            delta_voltage: 0.0,
            delta_current,
            voltage: output.voltage,
            current: self.current + delta_current,
        }
    }
    
    fn update_state(&mut self, dt: f64) {
        self.charge += self.current * dt;
        self.voltage = self.charge / self.capacitance;
    }
    
    fn get_current(&self) -> f64 { self.current }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn reset(&mut self) {
        self.voltage = 0.0;
        self.current = 0.0;
        self.charge = 0.0;
    }
}

/// Voltage source perturbation model
pub struct VoltageSourceModel {
    voltage: f64,
    current: f64,
}

impl VoltageSourceModel {
    pub fn new(voltage: f64) -> Self {
        Self {
            voltage,
            current: 0.0,
        }
    }
}

impl PerturbationModel for VoltageSourceModel {
    fn forward_perturb(&mut self, _input: &Perturbation, _dt: f64) -> Perturbation {
        // Voltage source maintains constant voltage
        Perturbation {
            delta_voltage: 0.0,
            delta_current: 0.0,
            voltage: self.voltage,
            current: self.current,
        }
    }
    
    fn backward_perturb(&mut self, output: &Perturbation, _dt: f64) -> Perturbation {
        // Voltage source supplies whatever current is demanded
        self.current = output.current;
        
        Perturbation {
            delta_voltage: 0.0,
            delta_current: output.delta_current,
            voltage: self.voltage,
            current: self.current,
        }
    }
    
    fn update_state(&mut self, _dt: f64) {
        // Voltage remains constant
    }
    
    fn get_current(&self) -> f64 { self.current }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn reset(&mut self) {
        self.current = 0.0;
    }
}

/// Circuit representation for perturbation-based simulation
pub struct PerturbationCircuit {
    /// Components indexed by ID
    pub components: HashMap<usize, Box<dyn PerturbationModel>>,
    /// Nodes indexed by ID
    pub nodes: HashMap<usize, Node>,
    /// Component connections: (component_id, pin1_node, pin2_node)
    pub connections: Vec<(usize, usize, usize)>,
    /// Simulation time
    pub time: f64,
}

impl PerturbationCircuit {
    pub fn new() -> Self {
        Self {
            components: HashMap::new(),
            nodes: HashMap::new(),
            connections: Vec::new(),
            time: 0.0,
        }
    }
    
    /// Add a component to the circuit
    pub fn add_component(&mut self, id: usize, component: Box<dyn PerturbationModel>) {
        self.components.insert(id, component);
    }
    
    /// Add a node to the circuit
    pub fn add_node(&mut self, id: usize) {
        self.nodes.insert(id, Node::new(id));
    }
    
    /// Connect a two-terminal component between nodes
    pub fn connect(&mut self, component_id: usize, node1: usize, node2: usize) {
        self.connections.push((component_id, node1, node2));
        
        // Update node connections
        if let Some(n1) = self.nodes.get_mut(&node1) {
            n1.connections.push((component_id, 0));
        }
        if let Some(n2) = self.nodes.get_mut(&node2) {
            n2.connections.push((component_id, 1));
        }
    }
    
    /// Run one simulation time step
    pub fn step(&mut self, dt: f64) -> bool {
        let voltage_threshold = 1e-6; // 1 µV
        let current_threshold = 1e-9; // 1 nA
        let max_iterations = 100;
        
        let mut iteration = 0;
        let mut all_settled = false;
        
        while !all_settled && iteration < max_iterations {
            all_settled = true;
            
            // First, update node voltages based on connected components
            // For voltage sources, set the node voltage directly
            for &(comp_id, node1_id, node2_id) in &self.connections.clone() {
                if let Some(component) = self.components.get(&comp_id) {
                    // Check if this is a voltage source (simplified check)
                    if component.get_voltage() != 0.0 && component.get_current() == 0.0 {
                        // This is likely a voltage source
                        if let Some(node1) = self.nodes.get_mut(&node1_id) {
                            node1.voltage = component.get_voltage();
                        }
                    }
                }
            }
            
            // Forward pass: propagate perturbations through components
            let mut node_currents: HashMap<usize, Vec<f64>> = HashMap::new();
            
            for &(comp_id, node1_id, node2_id) in &self.connections.clone() {
                if let (Some(component), Some(node1), Some(node2)) = 
                    (self.components.get_mut(&comp_id), 
                     self.nodes.get(&node1_id),
                     self.nodes.get(&node2_id)) {
                    
                    // Calculate voltage across component
                    let voltage_diff = node1.voltage - node2.voltage;
                    let input_perturb = Perturbation {
                        delta_voltage: voltage_diff - component.get_voltage(),
                        delta_current: 0.0,
                        voltage: voltage_diff,
                        current: component.get_current(),
                    };
                    
                    // Get component's response
                    let output_perturb = component.forward_perturb(&input_perturb, dt);
                    
                    // Update node current lists
                    node_currents.entry(node1_id).or_insert_with(Vec::new).push(-output_perturb.current);
                    node_currents.entry(node2_id).or_insert_with(Vec::new).push(output_perturb.current);
                    
                    // Check if still changing
                    if !output_perturb.is_settled(voltage_threshold, current_threshold) {
                        all_settled = false;
                    }
                }
            }
            
            // Update component states
            for component in self.components.values_mut() {
                component.update_state(dt);
            }
            
            // Apply KCL at each node and update voltages
            for (node_id, currents) in node_currents {
                if let Some(node) = self.nodes.get_mut(&node_id) {
                    node.apply_kcl(&currents);
                    
                    // Update node voltage based on current imbalance
                    node.voltage += node.perturbation.delta_voltage;
                    
                    if !node.perturbation.is_settled(voltage_threshold, current_threshold) {
                        all_settled = false;
                    }
                }
            }
            
            iteration += 1;
        }
        
        self.time += dt;
        all_settled
    }
    
    /// Reset circuit to initial conditions
    pub fn reset(&mut self) {
        for component in self.components.values_mut() {
            component.reset();
        }
        for node in self.nodes.values_mut() {
            node.voltage = 0.0;
            node.perturbation = Perturbation::new();
        }
        self.time = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rc_circuit() {
        let mut circuit = PerturbationCircuit::new();
        
        // Create nodes
        circuit.add_node(0); // Input
        circuit.add_node(1); // Between R and C
        circuit.add_node(2); // Ground
        
        // Add components
        circuit.add_component(0, Box::new(VoltageSourceModel::new(5.0)));
        circuit.add_component(1, Box::new(ResistorModel::new(1000.0)));
        circuit.add_component(2, Box::new(CapacitorModel::new(1e-6)));
        
        // Connect components
        circuit.connect(0, 0, 2); // Voltage source
        circuit.connect(1, 0, 1); // Resistor
        circuit.connect(2, 1, 2); // Capacitor
        
        // Simulate for 10ms
        let dt = 1e-6; // 1 µs time step
        for _ in 0..10000 {
            circuit.step(dt);
        }
        
        // Check capacitor voltage (should be close to 5V)
        let cap_voltage = circuit.components.get(&2).unwrap().get_voltage();
        assert!((cap_voltage - 5.0).abs() < 0.1, "Capacitor voltage: {}", cap_voltage);
    }
}