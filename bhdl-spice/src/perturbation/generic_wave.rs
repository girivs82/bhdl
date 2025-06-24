/// Generic wave propagation solver for arbitrary circuit topologies
/// 
/// This implementation models circuits as networks where waves propagate
/// bidirectionally through connections. Each node collects incoming waves
/// and each component scatters waves based on its impedance.

use std::collections::HashMap;

/// Wave traveling through a connection
#[derive(Debug, Clone, Copy, Default)]
pub struct Wave {
    /// Complex voltage amplitude (could be extended to complex for AC)
    pub voltage: f64,
    /// Complex current amplitude
    pub current: f64,
}

impl Wave {
    pub fn new(voltage: f64, current: f64) -> Self {
        Self { voltage, current }
    }
    
    /// Power carried by the wave
    pub fn power(&self) -> f64 {
        self.voltage * self.current
    }
    
    /// Add another wave (superposition)
    pub fn add(&self, other: &Wave) -> Wave {
        Wave {
            voltage: self.voltage + other.voltage,
            current: self.current + other.current,
        }
    }
    
    /// Scale wave by a factor
    pub fn scale(&self, factor: f64) -> Wave {
        Wave {
            voltage: self.voltage * factor,
            current: self.current * factor,
        }
    }
}

/// Node in the circuit where waves meet
#[derive(Debug)]
pub struct Node {
    pub id: usize,
    /// Node voltage (computed from waves)
    pub voltage: f64,
    /// Total current flowing out of node (for KCL check)
    pub current_sum: f64,
    /// Accumulated incoming waves from all connections
    pub incoming_wave: Wave,
}

impl Node {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            voltage: 0.0,
            current_sum: 0.0,
            incoming_wave: Wave::default(),
        }
    }
    
    /// Reset wave accumulation for new iteration
    pub fn reset_waves(&mut self) {
        self.incoming_wave = Wave::default();
        self.current_sum = 0.0;
    }
    
    /// Add an incoming wave from a connection
    pub fn add_incoming_wave(&mut self, wave: Wave) {
        self.incoming_wave = self.incoming_wave.add(&wave);
    }
    
    /// Update node voltage from accumulated waves
    pub fn update_voltage(&mut self) {
        // Node voltage is the superposition of all incoming waves
        self.voltage = self.incoming_wave.voltage;
    }
}

/// Connection between two nodes (bidirectional transmission line)
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: usize,
    pub node1_id: usize,
    pub node2_id: usize,
    /// Characteristic impedance of the connection
    pub z0: f64,
    /// Forward wave (node1 to node2)
    pub forward_wave: Wave,
    /// Backward wave (node2 to node1)
    pub backward_wave: Wave,
}

impl Connection {
    pub fn new(id: usize, node1_id: usize, node2_id: usize, z0: f64) -> Self {
        Self {
            id,
            node1_id,
            node2_id,
            z0,
            forward_wave: Wave::default(),
            backward_wave: Wave::default(),
        }
    }
    
    /// Update waves based on node voltages and currents
    pub fn update_waves(&mut self, v1: f64, v2: f64, i_forward: f64) {
        // Forward wave from node1 to node2
        self.forward_wave = Wave::new(
            (v1 + self.z0 * i_forward) / 2.0,
            i_forward
        );
        
        // Backward wave from node2 to node1
        let i_backward = (v1 - v2) / self.z0 - i_forward;
        self.backward_wave = Wave::new(
            (v2 - self.z0 * i_backward) / 2.0,
            -i_backward
        );
    }
}

/// Generic component model
#[derive(Debug, Clone)]
pub struct Component {
    pub id: usize,
    pub comp_type: ComponentType,
    /// Connected nodes
    pub nodes: Vec<usize>,
    /// Component voltage
    pub voltage: f64,
    /// Component current
    pub current: f64,
    /// Internal state (flux for L, charge for C)
    pub state: f64,
}

#[derive(Debug, Clone, Copy)]
pub enum ComponentType {
    VoltageSource { voltage: f64 },
    CurrentSource { current: f64 },
    Resistor { resistance: f64 },
    Inductor { inductance: f64 },
    Capacitor { capacitance: f64 },
}

impl Component {
    pub fn new(id: usize, comp_type: ComponentType, nodes: Vec<usize>) -> Self {
        Self {
            id,
            comp_type,
            nodes,
            voltage: 0.0,
            current: 0.0,
            state: 0.0,
        }
    }
    
    /// Update component based on node voltages
    pub fn update(&mut self, node_voltages: &HashMap<usize, f64>, dt: f64) {
        match self.nodes.len() {
            2 => {
                // Two-terminal component
                let v1 = node_voltages.get(&self.nodes[0]).copied().unwrap_or(0.0);
                let v2 = node_voltages.get(&self.nodes[1]).copied().unwrap_or(0.0);
                let v_diff = v1 - v2;
                
                match self.comp_type {
                    ComponentType::VoltageSource { voltage } => {
                        self.voltage = voltage;
                        // Current will be determined by the circuit
                    }
                    
                    ComponentType::CurrentSource { current } => {
                        self.current = current;
                        self.voltage = v_diff;
                    }
                    
                    ComponentType::Resistor { resistance } => {
                        self.voltage = v_diff;
                        self.current = v_diff / resistance;
                    }
                    
                    ComponentType::Inductor { inductance } => {
                        // V = L * di/dt => i = ∫(V/L)dt
                        self.voltage = v_diff;
                        let di = v_diff * dt / inductance;
                        self.current += di;
                        self.state += v_diff * dt; // Flux linkage
                    }
                    
                    ComponentType::Capacitor { capacitance } => {
                        // I = C * dv/dt
                        let dv = v_diff - self.voltage;
                        self.current = capacitance * dv / dt;
                        self.voltage = v_diff;
                        self.state = capacitance * v_diff; // Charge
                    }
                }
            }
            _ => {
                // Multi-terminal component (future extension)
                panic!("Multi-terminal components not yet implemented");
            }
        }
    }
    
    /// Get currents flowing out of each connected node
    pub fn get_node_currents(&self) -> Vec<(usize, f64)> {
        match self.nodes.len() {
            2 => {
                // For two-terminal components:
                // Current flows out of node[0] and into node[1]
                vec![
                    (self.nodes[0], -self.current),
                    (self.nodes[1], self.current),
                ]
            }
            _ => vec![],
        }
    }
    
    /// Get scattering parameters for wave model
    pub fn get_impedance(&self) -> f64 {
        match self.comp_type {
            ComponentType::VoltageSource { .. } => 0.001, // Very low impedance
            ComponentType::CurrentSource { .. } => 1e6,   // Very high impedance
            ComponentType::Resistor { resistance } => resistance,
            ComponentType::Inductor { .. } => 100.0, // Nominal impedance for now
            ComponentType::Capacitor { .. } => 100.0, // Nominal impedance for now
        }
    }
}

/// Generic wave circuit solver
pub struct GenericWaveCircuit {
    /// Nodes indexed by ID
    pub nodes: HashMap<usize, Node>,
    /// Connections between nodes
    pub connections: Vec<Connection>,
    /// Components in the circuit
    pub components: Vec<Component>,
    /// Default characteristic impedance
    pub z0_default: f64,
    /// Simulation time
    pub time: f64,
    /// Convergence tolerance
    pub tolerance: f64,
}

impl GenericWaveCircuit {
    pub fn new(z0_default: f64) -> Self {
        Self {
            nodes: HashMap::new(),
            connections: Vec::new(),
            components: Vec::new(),
            z0_default,
            time: 0.0,
            tolerance: 1e-9,
        }
    }
    
    /// Add a node to the circuit
    pub fn add_node(&mut self, id: usize) -> usize {
        self.nodes.insert(id, Node::new(id));
        id
    }
    
    /// Add a connection between two nodes
    pub fn add_connection(&mut self, node1_id: usize, node2_id: usize, z0: Option<f64>) -> usize {
        let id = self.connections.len();
        let z0 = z0.unwrap_or(self.z0_default);
        self.connections.push(Connection::new(id, node1_id, node2_id, z0));
        id
    }
    
    /// Add a component to the circuit
    pub fn add_component(&mut self, comp_type: ComponentType, nodes: Vec<usize>) -> usize {
        // Ensure all nodes exist
        for &node_id in &nodes {
            if !self.nodes.contains_key(&node_id) {
                self.add_node(node_id);
            }
        }
        
        let id = self.components.len();
        
        // Add connections between component nodes if they don't exist
        if nodes.len() == 2 && !self.has_connection(nodes[0], nodes[1]) {
            self.add_connection(nodes[0], nodes[1], None);
        }
        
        self.components.push(Component::new(id, comp_type, nodes));
        
        id
    }
    
    /// Check if a connection exists between two nodes
    fn has_connection(&self, node1: usize, node2: usize) -> bool {
        self.connections.iter().any(|c| 
            (c.node1_id == node1 && c.node2_id == node2) ||
            (c.node1_id == node2 && c.node2_id == node1)
        )
    }
    
    /// Single time step using wave propagation
    pub fn step(&mut self, dt: f64) -> bool {
        let max_iterations = 50;
        let mut converged = false;
        
        // Store old node voltages for convergence check
        let old_voltages: HashMap<usize, f64> = self.nodes.iter()
            .map(|(&id, node)| (id, node.voltage))
            .collect();
        
        for _iter in 0..max_iterations {
            // Step 1: Update components based on current node voltages
            let node_voltages: HashMap<usize, f64> = self.nodes.iter()
                .map(|(&id, node)| (id, node.voltage))
                .collect();
            
            for component in &mut self.components {
                component.update(&node_voltages, dt);
            }
            
            // Step 2: Reset node wave accumulation
            for node in self.nodes.values_mut() {
                node.reset_waves();
            }
            
            // Step 3: Calculate currents at each node from components
            let mut node_currents: HashMap<usize, f64> = HashMap::new();
            for component in &self.components {
                for (node_id, current) in component.get_node_currents() {
                    *node_currents.entry(node_id).or_insert(0.0) += current;
                }
            }
            
            // Step 4: Update waves in connections based on node states
            for connection in &mut self.connections {
                let v1 = self.nodes.get(&connection.node1_id).map(|n| n.voltage).unwrap_or(0.0);
                let v2 = self.nodes.get(&connection.node2_id).map(|n| n.voltage).unwrap_or(0.0);
                
                // Get current flowing through this connection
                // This is a simplified model - in practice we'd solve for the actual current
                let i_forward = (v1 - v2) / connection.z0;
                
                connection.update_waves(v1, v2, i_forward);
            }
            
            // Step 5: Propagate waves to nodes
            for connection in &self.connections {
                // Forward wave arrives at node2
                if let Some(node2) = self.nodes.get_mut(&connection.node2_id) {
                    node2.add_incoming_wave(connection.forward_wave);
                }
                
                // Backward wave arrives at node1
                if let Some(node1) = self.nodes.get_mut(&connection.node1_id) {
                    node1.add_incoming_wave(connection.backward_wave);
                }
            }
            
            // Step 6: Update node voltages from accumulated waves
            let mut max_change: f64 = 0.0;
            
            // Update node voltages first
            for (&id, node) in &mut self.nodes {
                let old_v = old_voltages.get(&id).copied().unwrap_or(0.0);
                node.update_voltage();
                let change = (node.voltage - old_v).abs();
                max_change = max_change.max(change);
            }
            
            // Apply voltage source constraints in a separate pass
            let voltage_source_constraints: Vec<(usize, usize, f64)> = self.components
                .iter()
                .filter_map(|component| {
                    if let ComponentType::VoltageSource { voltage } = component.comp_type {
                        if component.nodes.len() >= 2 {
                            Some((component.nodes[0], component.nodes[1], voltage))
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();
            
            // Collect all voltage updates first (reading phase)
            let mut node_voltage_updates = Vec::new();
            for (pos_node, ref_node, voltage) in voltage_source_constraints {
                let ref_v = self.nodes.get(&ref_node).map(|n| n.voltage).unwrap_or(0.0);
                node_voltage_updates.push((pos_node, ref_v + voltage));
            }
            
            // Apply all voltage updates (writing phase)
            for (node_id, new_voltage) in node_voltage_updates {
                if let Some(node) = self.nodes.get_mut(&node_id) {
                    node.voltage = new_voltage;
                }
            }
            
            // Check convergence
            if max_change < self.tolerance {
                converged = true;
                break;
            }
        }
        
        self.time += dt;
        converged
    }
    
    /// Get voltage at a specific node
    pub fn get_node_voltage(&self, node_id: usize) -> f64 {
        self.nodes.get(&node_id).map(|n| n.voltage).unwrap_or(0.0)
    }
    
    /// Get current through a specific component
    pub fn get_component_current(&self, comp_id: usize) -> f64 {
        self.components.get(comp_id).map(|c| c.current).unwrap_or(0.0)
    }
    
    /// Get voltage across a specific component
    pub fn get_component_voltage(&self, comp_id: usize) -> f64 {
        self.components.get(comp_id).map(|c| c.voltage).unwrap_or(0.0)
    }
    
    /// Reset circuit to initial conditions
    pub fn reset(&mut self) {
        for node in self.nodes.values_mut() {
            node.voltage = 0.0;
            node.current_sum = 0.0;
            node.incoming_wave = Wave::default();
        }
        
        for connection in &mut self.connections {
            connection.forward_wave = Wave::default();
            connection.backward_wave = Wave::default();
        }
        
        for component in &mut self.components {
            component.voltage = 0.0;
            component.current = 0.0;
            component.state = 0.0;
        }
        
        self.time = 0.0;
    }
}