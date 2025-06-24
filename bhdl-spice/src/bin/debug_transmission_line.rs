/// Simple debug test for transmission line wave generation
/// 
/// This test focuses on understanding why waves aren't propagating

use std::collections::HashMap;

fn main() {
    println!("=== Debug Transmission Line Wave Generation ===");
    debug_simple_step();
}

fn debug_simple_step() {
    println!("\n=== Single Step Wave Debug ===");
    
    // Create a minimal circuit: VSource(5V) -> Resistor(1kΩ) 
    let mut circuit = DebugTransmissionLine::new();
    
    // Node 0 = ground, Node 1 = voltage source positive
    circuit.add_segment(0, "VSource", 1.0, 0, 1, Some(5.0));  // 1Ω, 5V source
    circuit.add_segment(1, "Resistor", 1000.0, 1, 0, None);   // 1kΩ resistor
    
    println!("Initial state:");
    circuit.print_state();
    
    // Apply voltage source constraint
    circuit.apply_voltage_sources();
    println!("\nAfter applying voltage sources:");
    circuit.print_state();
    
    // Try to generate waves
    circuit.debug_wave_generation();
    println!("\nAfter attempting wave generation:");
    circuit.print_state();
    
    // Process any pending waves
    circuit.process_waves();
    println!("\nAfter processing waves:");
    circuit.print_state();
}

#[derive(Debug)]
struct DebugSegment {
    name: String,
    z0: f64,
    pos_node: usize,
    neg_node: usize,
    voltage_source: Option<f64>,
}

#[derive(Debug)]
struct DebugNode {
    voltage: f64,
    pending_waves: Vec<f64>, // Just voltage for simplicity
}

struct DebugTransmissionLine {
    segments: Vec<DebugSegment>,
    nodes: HashMap<usize, DebugNode>,
}

impl DebugTransmissionLine {
    fn new() -> Self {
        let mut nodes = HashMap::new();
        nodes.insert(0, DebugNode { voltage: 0.0, pending_waves: Vec::new() }); // Ground
        
        Self {
            segments: Vec::new(),
            nodes,
        }
    }
    
    fn add_segment(&mut self, id: usize, name: &str, z0: f64, pos_node: usize, neg_node: usize, voltage_source: Option<f64>) {
        // Ensure nodes exist
        if !self.nodes.contains_key(&pos_node) {
            self.nodes.insert(pos_node, DebugNode { voltage: 0.0, pending_waves: Vec::new() });
        }
        if !self.nodes.contains_key(&neg_node) {
            self.nodes.insert(neg_node, DebugNode { voltage: 0.0, pending_waves: Vec::new() });
        }
        
        self.segments.push(DebugSegment {
            name: name.to_string(),
            z0,
            pos_node,
            neg_node,
            voltage_source,
        });
    }
    
    fn apply_voltage_sources(&mut self) {
        for segment in &self.segments {
            if let Some(source_voltage) = segment.voltage_source {
                println!("  Applying voltage source: {} = {}V", segment.name, source_voltage);
                
                // Set positive terminal to source voltage
                if let Some(pos_node) = self.nodes.get_mut(&segment.pos_node) {
                    pos_node.voltage = source_voltage;
                }
                
                // Set negative terminal to 0V (assuming connected to ground)
                if let Some(neg_node) = self.nodes.get_mut(&segment.neg_node) {
                    neg_node.voltage = 0.0;
                }
            }
        }
    }
    
    fn debug_wave_generation(&mut self) {
        println!("  Checking wave generation conditions:");
        
        for (i, segment) in self.segments.iter().enumerate() {
            println!("    Segment {}: {}", i, segment.name);
            
            let v_pos = self.nodes[&segment.pos_node].voltage;
            let v_neg = self.nodes[&segment.neg_node].voltage;
            let current_voltage = v_pos - v_neg;
            
            println!("      V_pos = {}V, V_neg = {}V, V_diff = {}V", v_pos, v_neg, current_voltage);
            
            if let Some(source_voltage) = segment.voltage_source {
                let voltage_error = source_voltage - current_voltage;
                println!("      Source voltage = {}V, error = {}V", source_voltage, voltage_error);
                
                if voltage_error.abs() > 1e-6 {
                    println!("      Should generate wave (error > 1e-6)");
                    
                    // Calculate load impedance (other segments connected to neg_node)
                    let load_impedance = self.calculate_load_impedance(segment.neg_node, i);
                    println!("      Load impedance = {}Ω", load_impedance);
                    
                    // Calculate incident wave
                    let total_impedance = segment.z0 + load_impedance;
                    let incident_voltage = source_voltage * load_impedance / total_impedance;
                    
                    println!("      Incident wave: {}V (total Z = {}Ω)", incident_voltage, total_impedance);
                    
                    // Add wave to target node
                    if let Some(target_node) = self.nodes.get_mut(&segment.neg_node) {
                        target_node.pending_waves.push(incident_voltage);
                        println!("      Wave added to node {}", segment.neg_node);
                    }
                } else {
                    println!("      No wave needed (error too small)");
                }
            } else {
                println!("      Not a voltage source");
            }
        }
    }
    
    fn calculate_load_impedance(&self, node_id: usize, exclude_segment: usize) -> f64 {
        let mut total_conductance = 0.0;
        
        for (i, segment) in self.segments.iter().enumerate() {
            if i != exclude_segment && (segment.pos_node == node_id || segment.neg_node == node_id) {
                total_conductance += 1.0 / segment.z0;
                println!("        Segment {} contributes G = 1/{}Ω = {}S", i, segment.z0, 1.0 / segment.z0);
            }
        }
        
        if total_conductance > 1e-12 {
            1.0 / total_conductance
        } else {
            1e12 // Open circuit
        }
    }
    
    fn process_waves(&mut self) {
        println!("  Processing pending waves:");
        
        for (&node_id, node) in self.nodes.iter_mut() {
            if !node.pending_waves.is_empty() {
                println!("    Node {}: {} waves pending", node_id, node.pending_waves.len());
                
                // Simple superposition: add all wave voltages
                let total_wave_voltage: f64 = node.pending_waves.iter().sum();
                println!("      Total wave voltage: {}V", total_wave_voltage);
                
                // Update node voltage (this is simplified - real implementation would be more complex)
                node.voltage = total_wave_voltage;
                node.pending_waves.clear();
                
                println!("      Node {} voltage updated to {}V", node_id, node.voltage);
            }
        }
    }
    
    fn print_state(&self) {
        println!("  Nodes:");
        for (&id, node) in &self.nodes {
            println!("    Node {}: {}V, {} pending waves", id, node.voltage, node.pending_waves.len());
        }
        
        println!("  Segments:");
        for (i, segment) in self.segments.iter().enumerate() {
            let v_pos = self.nodes[&segment.pos_node].voltage;
            let v_neg = self.nodes[&segment.neg_node].voltage;
            println!("    {}: {} ({}Ω), V = {}V", i, segment.name, segment.z0, v_pos - v_neg);
        }
    }
}