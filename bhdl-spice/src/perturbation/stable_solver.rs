/// Stable perturbation solver using implicit methods
/// 
/// This implementation uses backward Euler integration for stability
/// and proper handling of stiff systems like RLC circuits.

use std::collections::HashMap;
use nalgebra::{DMatrix, DVector};

/// Component types for the solver
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ComponentType {
    Resistor(f64),      // Resistance in Ohms
    Capacitor(f64),     // Capacitance in Farads
    Inductor(f64),      // Inductance in Henries
    VoltageSource(f64), // Voltage in Volts
}

/// Component in the circuit
#[derive(Debug, Clone)]
pub struct Component {
    pub id: usize,
    pub comp_type: ComponentType,
    pub node1: usize,
    pub node2: usize,
    pub current: f64,
    pub voltage: f64,
}

impl Component {
    pub fn new(id: usize, comp_type: ComponentType, node1: usize, node2: usize) -> Self {
        Self {
            id,
            comp_type,
            node1,
            node2,
            current: 0.0,
            voltage: 0.0,
        }
    }
    
    /// Get conductance (1/R for resistors, special handling for others)
    pub fn conductance(&self, dt: f64) -> f64 {
        match self.comp_type {
            ComponentType::Resistor(r) => 1.0 / r,
            ComponentType::Capacitor(c) => c / dt, // Backward Euler
            ComponentType::Inductor(l) => dt / l,  // Backward Euler
            ComponentType::VoltageSource(_) => 0.0,
        }
    }
    
    /// Get equivalent current source for companion model
    pub fn companion_current(&self, dt: f64) -> f64 {
        match self.comp_type {
            ComponentType::Resistor(_) => 0.0,
            ComponentType::Capacitor(c) => c * self.voltage / dt,
            ComponentType::Inductor(l) => -self.current + dt * self.voltage / l,
            ComponentType::VoltageSource(v) => v,
        }
    }
}

/// Stable circuit solver
pub struct StableCircuit {
    /// Components in the circuit
    pub components: Vec<Component>,
    /// Node voltages (node 0 is ground)
    pub node_voltages: Vec<f64>,
    /// Number of nodes (including ground)
    pub num_nodes: usize,
    /// Simulation time
    pub time: f64,
    /// Convergence tolerance
    pub tolerance: f64,
}

impl StableCircuit {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            components: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            num_nodes,
            time: 0.0,
            tolerance: 1e-6,
        }
    }
    
    /// Add a component to the circuit
    pub fn add_component(&mut self, comp_type: ComponentType, node1: usize, node2: usize) {
        let id = self.components.len();
        self.components.push(Component::new(id, comp_type, node1, node2));
    }
    
    /// Build conductance matrix using Modified Nodal Analysis (MNA)
    fn build_mna_system(&self, dt: f64) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes;
        let m = self.components.iter()
            .filter(|c| matches!(c.comp_type, ComponentType::VoltageSource(_)))
            .count();
        
        let size = n + m;
        let mut g = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        let mut vsource_idx = 0;
        
        for comp in &self.components {
            match comp.comp_type {
                ComponentType::VoltageSource(v) => {
                    // Voltage source equations
                    let idx = n + vsource_idx;
                    
                    // KCL equations
                    if comp.node1 > 0 {
                        g[(comp.node1, idx)] = 1.0;
                        g[(idx, comp.node1)] = 1.0;
                    }
                    if comp.node2 > 0 {
                        g[(comp.node2, idx)] = -1.0;
                        g[(idx, comp.node2)] = -1.0;
                    }
                    
                    // Voltage constraint
                    b[idx] = v;
                    vsource_idx += 1;
                }
                _ => {
                    // Regular components (R, L, C)
                    let g_comp = comp.conductance(dt);
                    let i_comp = comp.companion_current(dt);
                    
                    // Stamp conductance matrix
                    if comp.node1 > 0 && comp.node1 < n {
                        g[(comp.node1, comp.node1)] += g_comp;
                        b[comp.node1] += i_comp;
                    }
                    if comp.node2 > 0 && comp.node2 < n {
                        g[(comp.node2, comp.node2)] += g_comp;
                        b[comp.node2] -= i_comp;
                    }
                    if comp.node1 > 0 && comp.node2 > 0 && comp.node1 < n && comp.node2 < n {
                        g[(comp.node1, comp.node2)] -= g_comp;
                        g[(comp.node2, comp.node1)] -= g_comp;
                    }
                }
            }
        }
        
        // Remove ground node equation (row 0, col 0)
        let g_reduced = g.slice((1, 1), (size - 1, size - 1)).clone_owned();
        let b_reduced = b.rows(1, size - 1).clone_owned();
        
        (g_reduced, b_reduced)
    }
    
    /// Solve one time step using backward Euler
    pub fn step(&mut self, dt: f64) -> bool {
        // Build MNA system
        let (g, b) = self.build_mna_system(dt);
        
        // Solve Gx = b
        match g.lu().solve(&b) {
            Some(x) => {
                // Update node voltages (skip ground node)
                for i in 1..self.num_nodes {
                    self.node_voltages[i] = x[i - 1];
                }
                
                // Update component states
                for comp in &mut self.components {
                    let v1 = self.node_voltages[comp.node1];
                    let v2 = self.node_voltages[comp.node2];
                    let v_new = v1 - v2;
                    
                    match comp.comp_type {
                        ComponentType::Resistor(r) => {
                            comp.voltage = v_new;
                            comp.current = v_new / r;
                        }
                        ComponentType::Capacitor(c) => {
                            comp.current = c * (v_new - comp.voltage) / dt;
                            comp.voltage = v_new;
                        }
                        ComponentType::Inductor(l) => {
                            comp.current += v_new * dt / l;
                            comp.voltage = v_new;
                        }
                        ComponentType::VoltageSource(_) => {
                            comp.voltage = v_new;
                            // Current is solved in MNA
                        }
                    }
                }
                
                self.time += dt;
                true
            }
            None => {
                eprintln!("Failed to solve circuit equations");
                false
            }
        }
    }
    
    /// Get voltage across a component
    pub fn get_component_voltage(&self, comp_id: usize) -> f64 {
        self.components.get(comp_id)
            .map(|c| c.voltage)
            .unwrap_or(0.0)
    }
    
    /// Get current through a component
    pub fn get_component_current(&self, comp_id: usize) -> f64 {
        self.components.get(comp_id)
            .map(|c| c.current)
            .unwrap_or(0.0)
    }
    
    /// Update voltage source value
    pub fn set_voltage_source(&mut self, comp_id: usize, voltage: f64) {
        if let Some(comp) = self.components.get_mut(comp_id) {
            comp.comp_type = ComponentType::VoltageSource(voltage);
        }
    }
    
    /// Reset circuit to initial conditions
    pub fn reset(&mut self) {
        self.node_voltages.fill(0.0);
        for comp in &mut self.components {
            comp.current = 0.0;
            comp.voltage = 0.0;
        }
        self.time = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_rc_circuit() {
        // Simple RC circuit: 5V -> R(1kΩ) -> C(1µF) -> GND
        let mut circuit = StableCircuit::new(3);
        
        // Add components
        circuit.add_component(ComponentType::VoltageSource(5.0), 1, 0);
        circuit.add_component(ComponentType::Resistor(1000.0), 1, 2);
        circuit.add_component(ComponentType::Capacitor(1e-6), 2, 0);
        
        // Simulate for 1 time constant
        let dt = 1e-5; // 10 µs
        let tau = 1000.0 * 1e-6; // RC = 1ms
        let steps = (tau / dt) as usize;
        
        for _ in 0..steps {
            circuit.step(dt);
        }
        
        // Check capacitor voltage (should be ~63% of 5V after 1 tau)
        let v_cap = circuit.get_component_voltage(2);
        assert!((v_cap - 5.0 * (1.0 - (-1.0_f64).exp())).abs() < 0.1);
    }
}