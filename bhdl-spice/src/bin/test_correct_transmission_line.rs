/// Corrected transmission line test with proper RC circuit topology
/// 
/// This implements the correct circuit: VSource -> Resistor -> Capacitor -> GND
/// Each segment represents the transmission line between components.

use std::collections::HashMap;
use std::fs::File;
use std::io::Write;

fn main() {
    println!("=== Corrected Transmission Line RC Test ===");
    test_rc_step_response();
}

fn test_rc_step_response() {
    println!("\n=== RC Circuit: 5V -> R(1kΩ) -> C(1μF) -> GND ===");
    
    let mut circuit = CorrectedTransmissionLine::new();
    
    // Circuit topology:
    // Node 0: Ground (0V)
    // Node 1: Voltage source positive terminal (5V)  
    // Node 2: Junction between R and C (the node we want to monitor)
    
    // VSource: creates 5V potential at node 1
    circuit.add_voltage_source(1, 5.0, 1.0);  // 5V source with 1Ω internal resistance at node 1
    
    // Transmission line segments:
    // Resistor segment: connects source (node 1) to capacitor (node 2)
    circuit.add_segment("Resistor", 1000.0, 1, 2, 100e-12);  // 1kΩ, 100ps delay
    
    // Capacitor segment: connects capacitor node (node 2) to ground (node 0)  
    circuit.add_segment("Capacitor", 10.0, 2, 0, 10e-12);   // 10Ω impedance, 10ps delay
    
    println!("Initial circuit state:");
    circuit.print_state();
    
    // Run simulation
    let duration = 5e-9; // 5ns
    let time_step = 1e-12; // 1ps
    let results = circuit.simulate(duration, time_step);
    
    // Save results
    let mut file = File::create("tests/outputs/corrected_transmission_line.csv").expect("Could not create file");
    writeln!(file, "time_ps,v_source,v_capacitor,v_theory").expect("Could not write header");
    
    let tau = 1e-9; // RC = 1kΩ * 1μF = 1ms -> but in our model it's different
    
    println!("\nSimulation Results:");
    for (i, (time, v_source, v_capacitor)) in results.iter().enumerate() {
        let v_theory = 5.0 * (1.0 - (-time / tau).exp());
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6}", 
                 time * 1e12, v_source, v_capacitor, v_theory).expect("Could not write data");
        
        if i % 100 == 0 {
            println!("  t = {:.1}ps: V_C = {:.3}V, V_source = {:.3}V", 
                     time * 1e12, v_capacitor, v_source);
        }
    }
    
    println!("Results saved to tests/outputs/corrected_transmission_line.csv");
}

struct CorrectedTransmissionLine {
    nodes: HashMap<usize, f64>,  // node_id -> voltage
    segments: Vec<TLSegment>,
    voltage_sources: HashMap<usize, (f64, f64)>, // node_id -> (voltage, resistance)
    pending_waves: HashMap<usize, Vec<Wave>>, // node_id -> waves arriving
    time: f64,
}

struct TLSegment {
    name: String,
    z0: f64,          // Characteristic impedance
    pos_node: usize,  // Positive terminal
    neg_node: usize,  // Negative terminal  
    delay: f64,       // Propagation delay
}

struct Wave {
    voltage: f64,
    arrival_time: f64,
    source_segment: usize,
}

impl CorrectedTransmissionLine {
    fn new() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(0, 0.0); // Ground
        
        let mut pending_waves = HashMap::new();
        pending_waves.insert(0, Vec::new());
        
        Self {
            nodes,
            segments: Vec::new(),
            voltage_sources: HashMap::new(),
            pending_waves,
            time: 0.0,
        }
    }
    
    fn add_voltage_source(&mut self, node_id: usize, voltage: f64, resistance: f64) {
        self.nodes.insert(node_id, voltage);
        self.voltage_sources.insert(node_id, (voltage, resistance));
        self.pending_waves.insert(node_id, Vec::new());
    }
    
    fn add_segment(&mut self, name: &str, z0: f64, pos_node: usize, neg_node: usize, delay: f64) {
        // Ensure nodes exist
        if !self.nodes.contains_key(&pos_node) {
            self.nodes.insert(pos_node, 0.0);
            self.pending_waves.insert(pos_node, Vec::new());
        }
        if !self.nodes.contains_key(&neg_node) {
            self.nodes.insert(neg_node, 0.0);
            self.pending_waves.insert(neg_node, Vec::new());
        }
        
        self.segments.push(TLSegment {
            name: name.to_string(),
            z0,
            pos_node,
            neg_node,
            delay,
        });
    }
    
    fn simulate(&mut self, duration: f64, time_step: f64) -> Vec<(f64, f64, f64)> {
        let mut results = Vec::new();
        let num_steps = (duration / time_step) as usize;
        
        println!("Starting corrected transmission line simulation:");
        println!("  Duration: {:.1}ns", duration * 1e9);
        println!("  Time step: {:.1}ps", time_step * 1e12);
        println!("  Steps: {}", num_steps);
        
        for step in 0..=num_steps {
            self.time = step as f64 * time_step;
            
            // Step 1: Process arriving waves
            self.process_arriving_waves();
            
            // Step 2: Apply voltage source constraints
            self.apply_voltage_sources();
            
            // Step 3: Generate new waves
            self.generate_waves();
            
            // Step 4: Record results
            let v_source = self.nodes[&1];
            let v_capacitor = self.nodes.get(&2).copied().unwrap_or(0.0);
            results.push((self.time, v_source, v_capacitor));
            
            if step % (num_steps / 10).max(1) == 0 {
                println!("  t = {:.1}ps: V_C = {:.3}V", self.time * 1e12, v_capacitor);
            }
        }
        
        results
    }
    
    fn process_arriving_waves(&mut self) {
        let mut voltage_updates = Vec::new();
        
        // Collect waves arriving at current time
        for (&node_id, waves) in &mut self.pending_waves {
            let mut arriving = Vec::new();
            waves.retain(|wave| {
                if (wave.arrival_time - self.time).abs() < 1e-15 {
                    arriving.push(wave.voltage);
                    false
                } else {
                    true
                }
            });
            
            if !arriving.is_empty() {
                let total_voltage: f64 = arriving.iter().sum();
                voltage_updates.push((node_id, total_voltage));
                
                if self.time < 100e-12 && node_id == 2 {
                    println!("    t={:.1}ps: Node 2 received {} waves, total V={:.3}V", 
                             self.time * 1e12, arriving.len(), total_voltage);
                }
            }
        }
        
        // Apply voltage updates (except for voltage source nodes)
        for (node_id, voltage) in voltage_updates {
            if !self.voltage_sources.contains_key(&node_id) {
                *self.nodes.get_mut(&node_id).unwrap() = voltage;
            }
        }
    }
    
    fn apply_voltage_sources(&mut self) {
        for (&node_id, &(voltage, _)) in &self.voltage_sources {
            *self.nodes.get_mut(&node_id).unwrap() = voltage;
        }
    }
    
    fn generate_waves(&mut self) {
        // Generate waves from voltage sources
        for (&source_node, &(source_voltage, source_resistance)) in &self.voltage_sources {
            for segment in &self.segments {
                if segment.pos_node == source_node {
                    // This segment is driven by the voltage source
                    let load_impedance = segment.z0; // Simplified: each segment has its own impedance
                    let total_impedance = source_resistance + load_impedance;
                    let incident_voltage = source_voltage * load_impedance / total_impedance;
                    
                    let wave = Wave {
                        voltage: incident_voltage,
                        arrival_time: self.time + segment.delay,
                        source_segment: 0, // Simplified
                    };
                    
                    self.pending_waves.get_mut(&segment.neg_node).unwrap().push(wave);
                    
                    if self.time < 100e-12 {
                        println!("    t={:.1}ps: VSource launched {:.3}V wave to node {} via {}", 
                                 self.time * 1e12, incident_voltage, segment.neg_node, segment.name);
                    }
                }
            }
        }
        
        // Generate waves from voltage differences on passive segments
        for segment in &self.segments {
            // Skip if this segment is driven by a voltage source
            if self.voltage_sources.contains_key(&segment.pos_node) {
                continue;
            }
            
            let v_pos = self.nodes[&segment.pos_node];
            let v_neg = self.nodes[&segment.neg_node];
            let v_diff = v_pos - v_neg;
            
            if v_diff.abs() > 1e-6 {
                // Generate wave toward the lower potential
                let wave_voltage = v_diff.abs() / 2.0;
                let target_node = if v_diff > 0.0 { segment.neg_node } else { segment.pos_node };
                
                let wave = Wave {
                    voltage: wave_voltage,
                    arrival_time: self.time + segment.delay,
                    source_segment: 0, // Simplified
                };
                
                self.pending_waves.get_mut(&target_node).unwrap().push(wave);
                
                if self.time < 100e-12 {
                    println!("    t={:.1}ps: {} launched {:.3}V wave to node {} (V_diff={:.3}V)", 
                             self.time * 1e12, segment.name, wave_voltage, target_node, v_diff);
                }
            }
        }
    }
    
    fn print_state(&self) {
        println!("  Nodes:");
        for (&id, &voltage) in &self.nodes {
            println!("    Node {}: {:.3}V", id, voltage);
        }
        
        println!("  Segments:");
        for segment in &self.segments {
            let v_pos = self.nodes[&segment.pos_node];
            let v_neg = self.nodes[&segment.neg_node];
            println!("    {}: {:.0}Ω, V = {:.3}V", segment.name, segment.z0, v_pos - v_neg);
        }
        
        println!("  Voltage Sources:");
        for (&node_id, &(voltage, resistance)) in &self.voltage_sources {
            println!("    Node {}: {:.1}V, {:.1}Ω", node_id, voltage, resistance);
        }
    }
}