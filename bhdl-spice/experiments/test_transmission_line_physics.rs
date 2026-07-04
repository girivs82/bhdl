/// Real-world transmission line physics implementation
/// 
/// Each component is modeled as a transmission line segment with:
/// - Characteristic impedance Z₀
/// - Propagation delay τ = l/v
/// - Forward and backward traveling waves
/// - Scattering at impedance discontinuities

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Real-World Transmission Line Physics Test ===");
    test_rc_transmission_line();
}

fn test_rc_transmission_line() {
    println!("\n=== RC Circuit with Transmission Line Physics ===");
    
    let mut circuit = TransmissionLineCircuit::new(0);
    circuit.set_time_step(1e-9); // 1ns for wave propagation resolution
    
    // Add nodes
    circuit.add_node(1); // Source positive
    circuit.add_node(2); // Between R and C
    
    // Add transmission line segments
    // Each component is a TL segment with its own Z₀ and delay
    circuit.add_transmission_line("VSource", TLSegment {
        z0: 1.0,           // 1Ω characteristic impedance
        delay: 1e-12,      // 1ps propagation delay (very short)
        pos_node: 1,
        neg_node: 0,
        voltage_source: Some(5.0), // 5V source
    });
    
    circuit.add_transmission_line("Resistor", TLSegment {
        z0: 1000.0,        // 1kΩ characteristic impedance
        delay: 1e-10,      // 100ps propagation delay
        pos_node: 1,
        neg_node: 2,
        voltage_source: None,
    });
    
    circuit.add_transmission_line("Capacitor", TLSegment {
        z0: 10.0,          // 10Ω effective impedance (from companion model)
        delay: 1e-11,      // 10ps propagation delay
        pos_node: 2,
        neg_node: 0,
        voltage_source: None,
    });
    
    // Run for 1μs to see wave propagation
    let results = circuit.solve_transmission_line(1e-6);
    
    // Write results
    let mut file = File::create("tests/outputs/transmission_line_response.csv").expect("Could not create file");
    writeln!(file, "time_ns,v_source,v_capacitor,v_theory").expect("Could not write header");
    
    let tau = 1e-3; // Expected RC time constant
    
    println!("Transmission Line Results:");
    for (i, (time, voltages)) in results.iter().enumerate() {
        let v_source = voltages.get(&1).copied().unwrap_or(0.0);
        let v_capacitor = voltages.get(&2).copied().unwrap_or(0.0);
        let v_theory = 5.0 * (1.0 - (-time / tau).exp());
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6}", 
                 time * 1e9, v_source, v_capacitor, v_theory).expect("Could not write data");
        
        // Print every 1000 steps
        if i % 1000 == 0 {
            println!("  t = {:.1}ns: V_C = {:.3}V, V_source = {:.3}V", 
                     time * 1e9, v_capacitor, v_source);
        }
    }
    
    println!("Results saved to tests/outputs/transmission_line_response.csv");
}

/// Transmission line segment representing a component
#[derive(Debug, Clone)]
struct TLSegment {
    z0: f64,              // Characteristic impedance
    delay: f64,           // Propagation delay
    pos_node: usize,      // Positive node
    neg_node: usize,      // Negative node
    voltage_source: Option<f64>, // Optional voltage source
}

/// Wave packet traveling on transmission line
#[derive(Debug, Clone, Copy)]
struct WavePacket {
    voltage: f64,         // Wave voltage
    current: f64,         // Wave current
    arrival_time: f64,    // When wave arrives at destination
    source_segment: usize, // Which TL segment generated this wave
    direction: WaveDirection, // Forward or backward
}

#[derive(Debug, Clone, Copy)]
enum WaveDirection {
    Forward,  // From pos_node to neg_node
    Backward, // From neg_node to pos_node
}

/// Node in transmission line circuit
#[derive(Debug)]
struct TLNode {
    id: usize,
    voltage: f64,
    connected_segments: Vec<usize>,
    pending_waves: Vec<WavePacket>, // Waves that will arrive at this node
}

impl TLNode {
    fn new(id: usize) -> Self {
        Self {
            id,
            voltage: 0.0,
            connected_segments: Vec::new(),
            pending_waves: Vec::new(),
        }
    }
}

/// Real transmission line circuit with wave physics
struct TransmissionLineCircuit {
    nodes: HashMap<usize, TLNode>,
    segments: Vec<TLSegment>,
    ground_node: usize,
    time: f64,
    time_step: f64,
}

impl TransmissionLineCircuit {
    fn new(ground_node: usize) -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(ground_node, TLNode::new(ground_node));
        
        Self {
            nodes,
            segments: Vec::new(),
            ground_node,
            time: 0.0,
            time_step: 1e-9,
        }
    }
    
    fn set_time_step(&mut self, dt: f64) {
        self.time_step = dt;
    }
    
    fn add_node(&mut self, node_id: usize) {
        if !self.nodes.contains_key(&node_id) {
            self.nodes.insert(node_id, TLNode::new(node_id));
        }
    }
    
    fn add_transmission_line(&mut self, _name: &str, segment: TLSegment) {
        self.add_node(segment.pos_node);
        self.add_node(segment.neg_node);
        
        let segment_id = self.segments.len();
        
        // Connect segment to nodes
        self.nodes.get_mut(&segment.pos_node).unwrap().connected_segments.push(segment_id);
        self.nodes.get_mut(&segment.neg_node).unwrap().connected_segments.push(segment_id);
        
        self.segments.push(segment);
    }
    
    fn solve_transmission_line(&mut self, duration: f64) -> Vec<(f64, HashMap<usize, f64>)> {
        let mut results = Vec::new();
        let num_steps = (duration / self.time_step) as usize;
        
        println!("Starting transmission line simulation:");
        println!("  Duration: {:.1}μs", duration * 1e6);
        println!("  Time step: {:.1}ns", self.time_step * 1e9);
        println!("  Steps: {}", num_steps);
        
        // Initial conditions
        self.time = 0.0;
        self.nodes.get_mut(&self.ground_node).unwrap().voltage = 0.0;
        
        // Record initial state
        let mut initial_voltages = HashMap::new();
        for (&node_id, node) in &self.nodes {
            initial_voltages.insert(node_id, node.voltage);
        }
        results.push((0.0, initial_voltages));
        
        // Time stepping with wave propagation
        for step in 1..=num_steps {
            self.time = step as f64 * self.time_step;
            
            // Step 1: Process arriving waves at each node
            self.process_arriving_waves();
            
            // Step 2: Apply voltage source constraints
            self.apply_voltage_sources();
            
            // Step 3: Generate new waves from voltage changes
            self.generate_new_waves();
            
            // Step 4: Record results
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
        
        println!("Transmission line simulation complete.");
        results
    }
    
    /// Process waves arriving at nodes at current time
    fn process_arriving_waves(&mut self) {
        let mut voltage_updates = Vec::new();
        
        // First pass: collect arriving waves  
        for (node_id, node) in &mut self.nodes {
            if *node_id == self.ground_node {
                continue; // Ground voltage is fixed
            }
            
            // Find waves arriving at this time
            let mut arriving_waves = Vec::new();
            node.pending_waves.retain(|wave| {
                if (wave.arrival_time - self.time).abs() < self.time_step / 2.0 {
                    arriving_waves.push(*wave);
                    false // Remove from pending
                } else {
                    true // Keep in pending
                }
            });
            
            if !arriving_waves.is_empty() {
                voltage_updates.push((*node_id, arriving_waves));
            }
        }
        
        // Second pass: calculate voltages and apply updates
        for (node_id, arriving_waves) in voltage_updates {
            // Calculate new voltage using simple wave scattering physics
            let new_voltage = if arriving_waves.is_empty() {
                self.nodes[&node_id].voltage
            } else {
                // Collect impedances and incident voltages
                let mut impedances = Vec::new();
                let mut incident_voltages = Vec::new();
                
                for wave in &arriving_waves {
                    let segment = &self.segments[wave.source_segment];
                    impedances.push(segment.z0);
                    incident_voltages.push(wave.voltage);
                }
                
                // Apply wave scattering: V = (Σ V_i/Z_i) / (Σ 1/Z_i)
                let numerator: f64 = incident_voltages.iter()
                    .zip(impedances.iter())
                    .map(|(v, z)| v / z)
                    .sum();
                
                let denominator: f64 = impedances.iter()
                    .map(|z| 1.0 / z)
                    .sum();
                
                if denominator > 1e-12 {
                    numerator / denominator
                } else {
                    self.nodes[&node_id].voltage
                }
            };
            
            // Apply voltage update
            if let Some(node) = self.nodes.get_mut(&node_id) {
                node.voltage = new_voltage;
                
                // Debug output for early times
                if self.time < 10e-9 && node_id == 2 {
                    println!("t={:.1}ns: Node 2: {} waves arrived, V={:.3}V", 
                             self.time * 1e9, arriving_waves.len(), node.voltage);
                }
            }
        }
    }
    
    /// Apply voltage source constraints
    fn apply_voltage_sources(&mut self) {
        for (segment_id, segment) in self.segments.iter().enumerate() {
            if let Some(source_voltage) = segment.voltage_source {
                // Voltage source forces voltage difference
                let pos_voltage = source_voltage;
                let neg_voltage = 0.0; // Assuming negative terminal connected to ground
                
                if let Some(pos_node) = self.nodes.get_mut(&segment.pos_node) {
                    pos_node.voltage = pos_voltage;
                }
                if let Some(neg_node) = self.nodes.get_mut(&segment.neg_node) {
                    neg_node.voltage = neg_voltage;
                }
            }
        }
    }
    
    /// Generate new waves from voltage sources and reflections
    fn generate_new_waves(&mut self) {
        // First pass: collect all wave generation data
        let mut wave_gen_data = Vec::new();
        
        for (segment_id, segment) in self.segments.iter().enumerate() {
            match segment.voltage_source {
                Some(source_voltage) => {
                    // For voltage sources, always generate waves to maintain the voltage
                    // The source continuously launches waves to enforce its voltage
                    let load_impedance = self.calculate_load_impedance(segment.neg_node, segment_id);
                    wave_gen_data.push((segment_id, source_voltage, load_impedance, segment.z0, segment.delay, segment.neg_node, true));
                },
                None => {
                    // Collect data for passive component wave generation
                    let v_pos = self.nodes[&segment.pos_node].voltage;
                    let v_neg = self.nodes[&segment.neg_node].voltage;
                    let v_diff = v_pos - v_neg;
                    
                    if v_diff.abs() > 1e-6 {
                        wave_gen_data.push((segment_id, v_diff, segment.z0, segment.z0, segment.delay, 
                                          if v_diff > 0.0 { segment.neg_node } else { segment.pos_node }, false));
                    }
                }
            }
        }
        
        // Second pass: generate waves using collected data
        for (segment_id, voltage_param, impedance_param, z0, delay, target_node, is_voltage_source) in wave_gen_data {
            if is_voltage_source {
                // Generate voltage source wave
                let source_voltage = voltage_param;
                let load_impedance = impedance_param;
                
                // For ideal voltage source, the source launches waves that will
                // result in the correct voltage across the load
                let total_impedance = z0 + load_impedance;
                let incident_voltage = source_voltage * load_impedance / total_impedance;
                let incident_current = incident_voltage / z0;
                
                let forward_wave = WavePacket {
                    voltage: incident_voltage,
                    current: incident_current,
                    arrival_time: self.time + delay,
                    source_segment: segment_id,
                    direction: WaveDirection::Forward,
                };
                
                self.nodes.get_mut(&target_node).unwrap().pending_waves.push(forward_wave);
                
                // Debug output for wave generation
                if self.time < 50e-9 {
                    println!("t={:.1}ns: VSource launched wave V={:.3}V, I={:.3}mA to node {} (Load Z={}Ω)", 
                             self.time * 1e9, incident_voltage, incident_current * 1000.0, target_node, load_impedance);
                }
            } else {
                // Generate passive component wave
                let v_diff = voltage_param;
                let current = v_diff / z0;
                let wave_voltage = v_diff / 2.0;
                
                let forward_wave = WavePacket {
                    voltage: wave_voltage,
                    current: current / 2.0,
                    arrival_time: self.time + delay,
                    source_segment: segment_id,
                    direction: WaveDirection::Forward,
                };
                
                self.nodes.get_mut(&target_node).unwrap().pending_waves.push(forward_wave);
            }
        }
    }
    
    
    /// Calculate impedance seen looking into a node (excluding one segment)
    fn calculate_load_impedance(&self, node_id: usize, exclude_segment: usize) -> f64 {
        let node = &self.nodes[&node_id];
        
        let total_conductance: f64 = node.connected_segments.iter()
            .filter(|&&seg_id| seg_id != exclude_segment)
            .map(|&seg_id| 1.0 / self.segments[seg_id].z0)
            .sum();
        
        if total_conductance > 1e-12 {
            1.0 / total_conductance
        } else {
            1e12 // Open circuit
        }
    }
    
    /// Calculate reflection coefficient at a node
    fn calculate_reflection_coefficient(&self, node_id: usize, z_incident: f64) -> f64 {
        let node = &self.nodes[&node_id];
        
        // Calculate total impedance seen at this node
        let total_conductance: f64 = node.connected_segments.iter()
            .map(|&seg_id| 1.0 / self.segments[seg_id].z0)
            .sum();
        
        let z_load = if total_conductance > 1e-12 {
            1.0 / total_conductance
        } else {
            1e12 // Open circuit
        };
        
        // Reflection coefficient: Γ = (Z_L - Z_0) / (Z_L + Z_0)
        (z_load - z_incident) / (z_load + z_incident)
    }
    
    /// Apply wave scattering at a node when multiple waves arrive (static version)
    fn scatter_arriving_waves_static(&self, node_id: usize, waves: &[WavePacket]) -> f64 {
        if waves.is_empty() {
            return self.nodes[&node_id].voltage;
        }
        
        // Collect impedances and incident voltages
        let mut impedances = Vec::new();
        let mut incident_voltages = Vec::new();
        
        for wave in waves {
            let segment = &self.segments[wave.source_segment];
            impedances.push(segment.z0);
            incident_voltages.push(wave.voltage);
        }
        
        // Apply generic wave scattering formula: V = (Σ V_i/Z_i) / (Σ 1/Z_i)
        let numerator: f64 = incident_voltages.iter()
            .zip(impedances.iter())
            .map(|(v, z)| v / z)
            .sum();
        
        let denominator: f64 = impedances.iter()
            .map(|z| 1.0 / z)
            .sum();
        
        if denominator > 1e-12 {
            numerator / denominator
        } else {
            self.nodes[&node_id].voltage // No change if no conductance
        }
    }
}