/// Simple wave propagation solver starting with DC analysis
/// 
/// Wave Model Fundamentals:
/// 1. Each component has a characteristic impedance Z
/// 2. Waves propagate as voltage/current pairs: V = Z*I (forward), V = -Z*I (reflected)
/// 3. At nodes, waves superpose: V_node = sum of all arriving waves
/// 4. Components scatter waves based on impedance mismatch
/// 
/// DC Analysis:
/// In DC steady state, there are no reflections - just resistive division
/// This gives us the correct baseline to verify the wave model

use std::collections::HashMap;

/// Wave traveling through the circuit
#[derive(Debug, Clone, Copy, Default)]
pub struct Wave {
    /// Voltage amplitude
    pub voltage: f64,
    /// Current amplitude (positive = away from source)
    pub current: f64,
}

impl Wave {
    pub fn new(voltage: f64, current: f64) -> Self {
        Self { voltage, current }
    }
    
    pub fn power(&self) -> f64 {
        self.voltage * self.current
    }
}

/// Circuit node where waves meet and superpose
#[derive(Debug)]
pub struct Node {
    pub id: usize,
    /// Node voltage (from wave superposition)
    pub voltage: f64,
    /// Net current flowing out (for KCL verification)
    pub current_out: f64,
    /// List of components connected to this node
    pub connections: Vec<usize>, // component IDs
}

impl Node {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            voltage: 0.0,
            current_out: 0.0,
            connections: Vec::new(),
        }
    }
}

/// Component types with their characteristics
#[derive(Debug, Clone, Copy)]
pub enum ComponentType {
    /// Voltage source with internal resistance
    VoltageSource { voltage: f64, internal_resistance: f64 },
    /// AC voltage source with amplitude, frequency, and phase
    AcVoltageSource { amplitude: f64, frequency: f64, phase: f64, internal_resistance: f64 },
    /// Linear resistor
    Resistor { resistance: f64 },
    /// Capacitor
    Capacitor { capacitance: f64 },
    /// Inductor
    Inductor { inductance: f64 },
}

/// Companion model for reactive components
#[derive(Debug, Clone, Copy)]
pub struct CompanionModel {
    /// Equivalent resistance for this time step
    pub resistance: f64,
    /// Equivalent current source (Norton model)
    pub current_source: f64,
    /// Equivalent voltage source (Thevenin model)  
    pub voltage_source: f64,
}

impl CompanionModel {
    pub fn new() -> Self {
        Self {
            resistance: 0.0,
            current_source: 0.0,
            voltage_source: 0.0,
        }
    }
}

/// Component in the wave circuit
#[derive(Debug, Clone)]
pub struct Component {
    pub id: usize,
    pub comp_type: ComponentType,
    /// Connected nodes [positive, negative]
    pub nodes: [usize; 2],
    /// Component voltage (positive node - negative node)
    pub voltage: f64,
    /// Component current (positive = flows from pos to neg node)
    pub current: f64,
    /// Previous voltage (for time integration)
    pub prev_voltage: f64,
    /// Previous current (for time integration)
    pub prev_current: f64,
    /// Companion model for transient analysis
    pub companion: CompanionModel,
}

impl Component {
    pub fn new(id: usize, comp_type: ComponentType, pos_node: usize, neg_node: usize) -> Self {
        Self {
            id,
            comp_type,
            nodes: [pos_node, neg_node],
            voltage: 0.0,
            current: 0.0,
            prev_voltage: 0.0,
            prev_current: 0.0,
            companion: CompanionModel::new(),
        }
    }
    
    /// Generate companion model for transient analysis using backward Euler
    pub fn generate_companion_model(&mut self, dt: f64) {
        match self.comp_type {
            ComponentType::Capacitor { capacitance } => {
                // Backward Euler companion model for capacitor:
                // i(t) = C * (v(t) - v(t-1)) / dt
                // Rearranging: v(t) = v(t-1) + i(t) * dt / C
                // Norton equivalent: i(t) = v(t) * (C/dt) + v(t-1) * (C/dt)
                // G_eq = C/dt, I_eq = C * v(t-1) / dt
                
                self.companion.resistance = dt / capacitance; // R_eq = 1/G_eq = dt/C
                self.companion.current_source = capacitance * self.prev_voltage / dt; // Norton current
                self.companion.voltage_source = 0.0; // Not used in Norton model
            },
            
            ComponentType::Inductor { inductance } => {
                // Backward Euler companion model for inductor:
                // v(t) = L * (i(t) - i(t-1)) / dt  
                // Rearranging: i(t) = i(t-1) + v(t) * dt / L
                // Norton equivalent: i(t) = v(t) * (dt/L) + i(t-1)
                // G_eq = dt/L, I_eq = i(t-1)
                
                self.companion.resistance = inductance / dt; // R_eq = 1/G_eq = L/dt
                self.companion.current_source = self.prev_current; // Norton current
                self.companion.voltage_source = 0.0; // Not used in Norton model
            },
            
            ComponentType::Resistor { resistance } => {
                // Resistor is purely resistive - no companion model needed
                self.companion.resistance = resistance;
                self.companion.current_source = 0.0;
                self.companion.voltage_source = 0.0;
            },
            
            ComponentType::VoltageSource { voltage, internal_resistance } => {
                // Voltage source with internal resistance
                self.companion.resistance = internal_resistance;
                self.companion.current_source = 0.0;
                self.companion.voltage_source = voltage; // Thevenin voltage
            },
            
            ComponentType::AcVoltageSource { amplitude: _, frequency: _, phase: _, internal_resistance } => {
                // AC voltage source - voltage will be calculated based on time
                self.companion.resistance = internal_resistance;
                self.companion.current_source = 0.0;
                // voltage_source will be set in solve_transient based on current time
                self.companion.voltage_source = 0.0; 
            },
        }
    }
    
    /// Get effective resistance for circuit analysis (uses companion model in transient)
    pub fn effective_resistance(&self) -> f64 {
        self.companion.resistance
    }
    
    /// Get Norton equivalent current source
    pub fn norton_current(&self) -> f64 {
        self.companion.current_source
    }
    
    /// Store previous values for next time step
    pub fn store_previous_values(&mut self) {
        self.prev_voltage = self.voltage;
        self.prev_current = self.current;
    }
    
    /// Get DC resistance (impedance at f=0)
    pub fn dc_resistance(&self) -> f64 {
        match self.comp_type {
            ComponentType::VoltageSource { internal_resistance, .. } => internal_resistance,
            ComponentType::AcVoltageSource { internal_resistance, .. } => internal_resistance,
            ComponentType::Resistor { resistance } => resistance,
            ComponentType::Capacitor { .. } => 1e12, // Very high resistance (open circuit)
            ComponentType::Inductor { .. } => 1e-6, // Very low resistance (short circuit)
        }
    }
    
    /// Get complex impedance at given frequency
    pub fn impedance(&self, frequency: f64) -> num_complex::Complex<f64> {
        use num_complex::Complex;
        use std::f64::consts::PI;
        
        let omega = 2.0 * PI * frequency;
        
        match self.comp_type {
            ComponentType::VoltageSource { internal_resistance, .. } => {
                Complex::new(internal_resistance, 0.0)
            },
            ComponentType::AcVoltageSource { internal_resistance, .. } => {
                Complex::new(internal_resistance, 0.0)
            },
            ComponentType::Resistor { resistance } => {
                Complex::new(resistance, 0.0)
            },
            ComponentType::Capacitor { capacitance } => {
                if frequency == 0.0 {
                    Complex::new(1e12, 0.0) // Open circuit at DC
                } else {
                    Complex::new(0.0, -1.0 / (omega * capacitance)) // -j/(ωC)
                }
            },
            ComponentType::Inductor { inductance } => {
                if frequency == 0.0 {
                    Complex::new(1e-6, 0.0) // Short circuit at DC
                } else {
                    Complex::new(0.0, omega * inductance) // jωL
                }
            },
        }
    }
    
    /// Get impedance magnitude at given frequency
    pub fn impedance_magnitude(&self, frequency: f64) -> f64 {
        self.impedance(frequency).norm()
    }
    
    /// Check if this is a voltage source
    pub fn is_voltage_source(&self) -> bool {
        matches!(self.comp_type, ComponentType::VoltageSource { .. } | ComponentType::AcVoltageSource { .. })
    }
    
    /// Get the voltage that should be enforced (for voltage sources)
    pub fn source_voltage(&self) -> Option<f64> {
        match self.comp_type {
            ComponentType::VoltageSource { voltage, .. } => Some(voltage),
            ComponentType::AcVoltageSource { .. } => None, // AC sources need time parameter
            _ => None,
        }
    }
    
    /// Get the AC voltage at a specific time (for AC sources)
    pub fn ac_source_voltage(&self, time: f64) -> Option<f64> {
        match self.comp_type {
            ComponentType::AcVoltageSource { amplitude, frequency, phase, .. } => {
                use std::f64::consts::PI;
                let omega = 2.0 * PI * frequency;
                Some(amplitude * (omega * time + phase).sin())
            },
            _ => None,
        }
    }
}

/// Simple wave circuit for DC, AC, and transient analysis
pub struct SimpleWaveCircuit {
    /// Circuit nodes
    pub nodes: HashMap<usize, Node>,
    /// Circuit components
    pub components: HashMap<usize, Component>,
    /// Ground node ID
    pub ground_node: usize,
    /// Convergence tolerance
    pub tolerance: f64,
    /// Current simulation time
    pub time: f64,
    /// Time step for transient analysis
    pub time_step: f64,
}

impl SimpleWaveCircuit {
    pub fn new(ground_node: usize) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(ground_node, Node::new(ground_node));
        
        Self {
            nodes,
            components: HashMap::new(),
            ground_node,
            tolerance: 1e-9,
            time: 0.0,
            time_step: 1e-6, // Default 1μs time step
        }
    }
    
    /// Set time step for transient analysis
    pub fn set_time_step(&mut self, dt: f64) {
        self.time_step = dt;
    }
    
    /// Add a node to the circuit
    pub fn add_node(&mut self, node_id: usize) {
        if !self.nodes.contains_key(&node_id) {
            self.nodes.insert(node_id, Node::new(node_id));
        }
    }
    
    /// Add a component between two nodes
    pub fn add_component(&mut self, comp_type: ComponentType, pos_node: usize, neg_node: usize) -> usize {
        // Ensure nodes exist
        self.add_node(pos_node);
        self.add_node(neg_node);
        
        let comp_id = self.components.len();
        let component = Component::new(comp_id, comp_type, pos_node, neg_node);
        
        // Update node connections
        self.nodes.get_mut(&pos_node).unwrap().connections.push(comp_id);
        self.nodes.get_mut(&neg_node).unwrap().connections.push(comp_id);
        
        self.components.insert(comp_id, component);
        comp_id
    }
    
    /// Solve DC operating point using iterative method
    pub fn solve_dc(&mut self, max_iterations: usize) -> bool {
        // Ground node is always at 0V
        self.nodes.get_mut(&self.ground_node).unwrap().voltage = 0.0;
        
        for iteration in 0..max_iterations {
            let mut max_change: f64 = 0.0;
            
            // Store old voltages for convergence check
            let old_voltages: HashMap<usize, f64> = self.nodes.iter()
                .map(|(&id, node)| (id, node.voltage))
                .collect();
            
            // Step 1: Update component voltages and currents based on node voltages
            // For voltage sources, handle them separately to avoid conflicts
            for component in self.components.values_mut() {
                let v_pos = self.nodes[&component.nodes[0]].voltage;
                let v_neg = self.nodes[&component.nodes[1]].voltage;
                
                match component.comp_type {
                    ComponentType::VoltageSource { voltage, .. } => {
                        // Voltage source maintains its specified voltage
                        component.voltage = voltage;
                        // Current will be determined by KCL (calculated later)
                    },
                    ComponentType::AcVoltageSource { .. } => {
                        // AC voltage source - voltage depends on time
                        if let Some(ac_voltage) = component.ac_source_voltage(self.time) {
                            component.voltage = ac_voltage;
                        }
                        // Current will be determined by KCL (calculated later)
                    },
                    ComponentType::Resistor { .. } | ComponentType::Capacitor { .. } | ComponentType::Inductor { .. } => {
                        component.voltage = v_pos - v_neg;
                        component.current = component.voltage / component.dc_resistance();
                    }
                }
            }
            
            // Step 2: Apply KCL at each node (except ground) to get new voltages
            for (&node_id, node) in self.nodes.iter_mut() {
                if node_id == self.ground_node {
                    continue; // Ground voltage is fixed at 0V
                }
                
                // Calculate net current flowing out of this node
                let mut current_sum = 0.0;
                for &comp_id in &node.connections {
                    let component = &self.components[&comp_id];
                    if component.nodes[0] == node_id {
                        // Current flows out of positive terminal
                        current_sum += component.current;
                    } else {
                        // Current flows into negative terminal
                        current_sum -= component.current;
                    }
                }
                
                // KCL: sum of currents = 0, so if there's an imbalance,
                // adjust the node voltage to reduce it
                // Use a simple damped iteration: ΔV = -α * I_imbalance / G_total
                let total_conductance: f64 = node.connections.iter()
                    .map(|&comp_id| 1.0 / self.components[&comp_id].dc_resistance())
                    .sum();
                
                if total_conductance > 1e-12 {
                    let damping = 0.5; // Damping factor for stability
                    let voltage_correction = -damping * current_sum / total_conductance;
                    node.voltage += voltage_correction;
                }
                
                node.current_out = current_sum;
            }
            
            // Step 3: Enforce voltage source constraints
            // First pass: collect voltage source information and update node voltages
            let mut voltage_source_currents = Vec::new();
            for (&comp_id, component) in &self.components {
                // Handle both DC and AC voltage sources
                let source_voltage = if let Some(dc_voltage) = component.source_voltage() {
                    Some(dc_voltage)
                } else if let Some(ac_voltage) = component.ac_source_voltage(self.time) {
                    Some(ac_voltage)
                } else {
                    None
                };
                
                if let Some(voltage) = source_voltage {
                    let pos_node = component.nodes[0];
                    let neg_node = component.nodes[1];
                    
                    // Set positive node voltage relative to negative node
                    let neg_voltage = self.nodes[&neg_node].voltage;
                    if let Some(pos_node_obj) = self.nodes.get_mut(&pos_node) {
                        pos_node_obj.voltage = neg_voltage + voltage;
                    }
                    
                    // Calculate the current needed by the voltage source
                    let mut current_into_pos_node = 0.0;
                    for &other_comp_id in &self.nodes[&pos_node].connections {
                        if other_comp_id != comp_id {
                            let other_comp = &self.components[&other_comp_id];
                            if other_comp.nodes[0] == pos_node {
                                // Current flows out of this node through this component
                                current_into_pos_node -= other_comp.current;
                            } else if other_comp.nodes[1] == pos_node {
                                // Current flows into this node through this component
                                current_into_pos_node += other_comp.current;
                            }
                        }
                    }
                    
                    voltage_source_currents.push((comp_id, current_into_pos_node));
                }
            }
            
            // Second pass: update voltage source currents
            for (comp_id, current) in voltage_source_currents {
                if let Some(component) = self.components.get_mut(&comp_id) {
                    component.current = current;
                }
            }
            
            // Step 4: Check convergence
            for (&node_id, node) in &self.nodes {
                let old_v = old_voltages.get(&node_id).copied().unwrap_or(0.0);
                let change = (node.voltage - old_v).abs();
                max_change = max_change.max(change);
            }
            
            if max_change < self.tolerance {
                println!("DC analysis converged in {} iterations", iteration + 1);
                return true;
            }
        }
        
        println!("DC analysis failed to converge in {} iterations", max_iterations);
        false
    }
    
    /// Get node voltage
    pub fn get_node_voltage(&self, node_id: usize) -> f64 {
        self.nodes.get(&node_id).map(|n| n.voltage).unwrap_or(0.0)
    }
    
    /// Get component current
    pub fn get_component_current(&self, comp_id: usize) -> f64 {
        self.components.get(&comp_id).map(|c| c.current).unwrap_or(0.0)
    }
    
    /// Get component voltage
    pub fn get_component_voltage(&self, comp_id: usize) -> f64 {
        self.components.get(&comp_id).map(|c| c.voltage).unwrap_or(0.0)
    }
    
    /// Perform transient analysis using companion models and backward Euler integration
    pub fn solve_transient(&mut self, duration: f64) -> Vec<(f64, HashMap<usize, f64>)> {
        let mut results = Vec::new();
        let num_steps = (duration / self.time_step) as usize;
        
        println!("Starting transient analysis with companion models:");
        println!("  Duration: {:.3}ms", duration * 1000.0);
        println!("  Time step: {:.3}μs", self.time_step * 1e6);
        println!("  Steps: {}", num_steps);
        println!("  Integration method: Backward Euler");
        
        // Initial conditions (t=0)
        self.time = 0.0;
        
        // Generate initial companion models
        for component in self.components.values_mut() {
            component.generate_companion_model(self.time_step);
        }
        
        // Solve initial state
        let initial_converged = self.solve_transient_step();
        if !initial_converged {
            println!("Warning: Failed to converge at t = 0");
        }
        
        // Record initial state
        let mut initial_voltages = HashMap::new();
        for (&node_id, node) in &self.nodes {
            initial_voltages.insert(node_id, node.voltage);
        }
        results.push((0.0, initial_voltages));
        
        // Time stepping loop
        for step in 1..=num_steps {
            self.time = step as f64 * self.time_step;
            
            // Store previous values
            for component in self.components.values_mut() {
                component.store_previous_values();
            }
            
            // Generate new companion models for this time step
            for component in self.components.values_mut() {
                component.generate_companion_model(self.time_step);
                
                // Update AC source voltages based on current time
                if let ComponentType::AcVoltageSource { amplitude, frequency, phase, .. } = component.comp_type {
                    use std::f64::consts::PI;
                    let omega = 2.0 * PI * frequency;
                    let ac_voltage = amplitude * (omega * self.time + phase).sin();
                    component.companion.voltage_source = ac_voltage;
                }
            }
            
            // Solve the companion circuit at this time step
            let converged = self.solve_transient_step();
            
            if !converged {
                println!("Warning: Failed to converge at t = {:.6}s", self.time);
            }
            
            // Record node voltages
            let mut voltages = HashMap::new();
            for (&node_id, node) in &self.nodes {
                voltages.insert(node_id, node.voltage);
            }
            results.push((self.time, voltages));
            
            // Progress indicator
            if step % (num_steps / 10).max(1) == 0 {
                println!("  Progress: {:.1}%", 100.0 * step as f64 / num_steps as f64);
            }
        }
        
        println!("Transient analysis complete.");
        results
    }
    
    /// Solve one time step using companion models (modified nodal analysis)
    fn solve_transient_step(&mut self) -> bool {
        let max_iterations = 100;
        
        // Ground node is always at 0V
        self.nodes.get_mut(&self.ground_node).unwrap().voltage = 0.0;
        
        for iteration in 0..max_iterations {
            let mut max_change: f64 = 0.0;
            
            // Store old voltages for convergence check
            let old_voltages: HashMap<usize, f64> = self.nodes.iter()
                .map(|(&id, node)| (id, node.voltage))
                .collect();
            
            // Step 1: Update component voltages based on node voltages
            for component in self.components.values_mut() {
                let v_pos = self.nodes[&component.nodes[0]].voltage;
                let v_neg = self.nodes[&component.nodes[1]].voltage;
                component.voltage = v_pos - v_neg;
            }
            
            // Step 2: Update currents for non-voltage-source components using companion models
            for component in self.components.values_mut() {
                match component.comp_type {
                    ComponentType::VoltageSource { .. } | ComponentType::AcVoltageSource { .. } => {
                        // Voltage sources will be handled as constraints
                        // Current will be determined by KCL at the end
                    },
                    _ => {
                        // Use companion model for current calculation
                        // I = V / R_eq (where R_eq includes reactive effects)
                        // The Norton current is handled separately in the nodal analysis
                        component.current = component.voltage / component.effective_resistance();
                    }
                }
            }
            
            // Step 3: Apply KCL at each node (except ground) to solve for node voltages
            for (&node_id, node) in self.nodes.iter_mut() {
                if node_id == self.ground_node {
                    continue; // Ground voltage is fixed at 0V
                }
                
                // Calculate total conductance and current injection for this node
                let mut total_conductance = 0.0;
                let mut current_injection = 0.0;
                
                for &comp_id in &node.connections {
                    let component = &self.components[&comp_id];
                    
                    // Skip voltage sources - they'll be handled as constraints
                    if component.is_voltage_source() {
                        continue;
                    }
                    
                    let conductance = 1.0 / component.effective_resistance();
                    total_conductance += conductance;
                    
                    // Norton current injection
                    let norton_current = component.norton_current();
                    if component.nodes[0] == node_id {
                        // Component's positive terminal connects to this node
                        current_injection += norton_current;
                    } else {
                        // Component's negative terminal connects to this node
                        current_injection -= norton_current;
                    }
                    
                    // Current from other nodes through this component
                    let other_node_id = if component.nodes[0] == node_id {
                        component.nodes[1]
                    } else {
                        component.nodes[0]
                    };
                    let other_node_voltage = old_voltages[&other_node_id];
                    
                    if component.nodes[0] == node_id {
                        // Current flows from other node to this node
                        current_injection += conductance * other_node_voltage;
                    } else {
                        // Current flows from this node to other node
                        current_injection += conductance * other_node_voltage;
                    }
                }
                
                // Apply KCL: sum of currents = 0
                // Total current into node = G * V_node
                // So: G * V_node = current_injection
                // Therefore: V_node = current_injection / G
                if total_conductance > 1e-12 {
                    let new_voltage = current_injection / total_conductance;
                    let damping = 0.6; // Damping for stability
                    node.voltage = (1.0 - damping) * node.voltage + damping * new_voltage;
                }
                
                // Debug output for capacitor node
                if node_id == 2 && self.time < 0.001 {
                    println!("t={:.6}s: Node 2: V={:.6}V, G_total={:.6}, I_inject={:.6}", 
                             self.time, node.voltage, total_conductance, current_injection);
                }
            }
            
            // Step 4: Enforce voltage source constraints
            let mut voltage_source_currents = Vec::new();
            for (&comp_id, component) in &self.components {
                let source_voltage = match component.comp_type {
                    ComponentType::VoltageSource { .. } => component.companion.voltage_source,
                    ComponentType::AcVoltageSource { .. } => component.companion.voltage_source,
                    _ => continue,
                };
                
                let pos_node = component.nodes[0];
                let neg_node = component.nodes[1];
                
                // Set positive node voltage relative to negative node
                let neg_voltage = self.nodes[&neg_node].voltage;
                if let Some(pos_node_obj) = self.nodes.get_mut(&pos_node) {
                    pos_node_obj.voltage = neg_voltage + source_voltage;
                }
                
                // Calculate required current for voltage source
                let mut current_into_pos_node = 0.0;
                for &other_comp_id in &self.nodes[&pos_node].connections {
                    if other_comp_id != comp_id {
                        let other_comp = &self.components[&other_comp_id];
                        if other_comp.nodes[0] == pos_node {
                            current_into_pos_node -= other_comp.current;
                        } else if other_comp.nodes[1] == pos_node {
                            current_into_pos_node += other_comp.current;
                        }
                    }
                }
                
                voltage_source_currents.push((comp_id, current_into_pos_node));
            }
            
            // Update voltage source currents
            for (comp_id, current) in voltage_source_currents {
                if let Some(component) = self.components.get_mut(&comp_id) {
                    component.current = current;
                }
            }
            
            // Step 5: Check convergence
            for (&node_id, node) in &self.nodes {
                let old_v = old_voltages.get(&node_id).copied().unwrap_or(0.0);
                let change = (node.voltage - old_v).abs();
                max_change = max_change.max(change);
            }
            
            if max_change < self.tolerance {
                return true; // Converged
            }
        }
        
        false // Failed to converge
    }
    
    /// Print circuit state for debugging
    pub fn print_state(&self) {
        println!("\n=== Circuit State (t = {:.6}s) ===", self.time);
        println!("Nodes:");
        for (&id, node) in &self.nodes {
            println!("  Node {}: {:.6}V, I_out = {:.6}A", id, node.voltage, node.current_out);
        }
        
        println!("Components:");
        for (&id, comp) in &self.components {
            println!("  Component {}: {:.6}V, {:.6}A", id, comp.voltage, comp.current);
            match comp.comp_type {
                ComponentType::VoltageSource { voltage, internal_resistance } => {
                    println!("    Type: Voltage Source ({}V, {}Ω internal)", voltage, internal_resistance);
                },
                ComponentType::AcVoltageSource { amplitude, frequency, phase, internal_resistance } => {
                    println!("    Type: AC Voltage Source ({}V @ {}Hz, {}° phase, {}Ω internal)", 
                             amplitude, frequency, phase * 180.0 / std::f64::consts::PI, internal_resistance);
                },
                ComponentType::Resistor { resistance } => {
                    println!("    Type: Resistor ({}Ω)", resistance);
                },
                ComponentType::Capacitor { capacitance } => {
                    println!("    Type: Capacitor ({}F)", capacitance);
                },
                ComponentType::Inductor { inductance } => {
                    println!("    Type: Inductor ({}H)", inductance);
                },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_simple_voltage_divider() {
        // Test circuit: 5V -> R1(1k) -> R2(1k) -> GND
        // Expected: V_middle = 2.5V
        
        let mut circuit = SimpleWaveCircuit::new(0); // Ground = node 0
        
        // Nodes: 0=ground, 1=source+, 2=middle
        circuit.add_node(1);
        circuit.add_node(2);
        
        // Components - realistic internal resistance
        circuit.add_component(ComponentType::VoltageSource { voltage: 5.0, internal_resistance: 1.0 }, 1, 0);
        circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 1, 2);
        circuit.add_component(ComponentType::Resistor { resistance: 1000.0 }, 2, 0);
        
        // Solve DC
        let converged = circuit.solve_dc(100);
        assert!(converged, "DC analysis should converge");
        
        // Check results
        let v_source = circuit.get_node_voltage(1);
        let v_middle = circuit.get_node_voltage(2);
        let v_ground = circuit.get_node_voltage(0);
        
        println!("Voltage divider results:");
        println!("  V_source = {:.3}V (expected 5.0V)", v_source);
        println!("  V_middle = {:.3}V (expected 2.5V)", v_middle);
        println!("  V_ground = {:.3}V (expected 0.0V)", v_ground);
        
        assert!((v_source - 5.0).abs() < 0.01, "Source voltage error too large");
        assert!((v_middle - 2.5).abs() < 0.01, "Middle voltage error too large");
        assert!(v_ground.abs() < 0.01, "Ground voltage should be 0V");
    }
}