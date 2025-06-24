/// GPU-ready perturbation solver
/// 
/// This implementation is designed to be easily portable to GPU
/// using parallel component updates and minimal synchronization.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic float wrapper for GPU-style operations
#[derive(Debug)]
pub struct AtomicF64 {
    storage: AtomicU64,
}

impl AtomicF64 {
    pub fn new(value: f64) -> Self {
        Self {
            storage: AtomicU64::new(value.to_bits()),
        }
    }
    
    pub fn load(&self) -> f64 {
        f64::from_bits(self.storage.load(Ordering::Relaxed))
    }
    
    pub fn store(&self, value: f64) {
        self.storage.store(value.to_bits(), Ordering::Relaxed);
    }
    
    /// Atomic add operation (would be atomicAdd on GPU)
    pub fn add(&self, delta: f64) {
        loop {
            let current_bits = self.storage.load(Ordering::Relaxed);
            let current = f64::from_bits(current_bits);
            let new = current + delta;
            let new_bits = new.to_bits();
            
            if self.storage.compare_exchange_weak(
                current_bits,
                new_bits,
                Ordering::Relaxed,
                Ordering::Relaxed
            ).is_ok() {
                break;
            }
        }
    }
}

/// Node structure optimized for GPU
#[derive(Debug)]
pub struct GpuNode {
    pub voltage: AtomicF64,
    pub current_sum: AtomicF64,
    pub capacitance: f64, // Virtual capacitance for stability
    pub conductance: f64, // Virtual conductance for damping
}

impl GpuNode {
    pub fn new() -> Self {
        Self {
            voltage: AtomicF64::new(0.0),
            current_sum: AtomicF64::new(0.0),
            capacitance: 1e-12, // 1pF virtual capacitance
            conductance: 1e-9, // 1nS virtual conductance for damping
        }
    }
}

/// Component data for GPU processing
#[derive(Debug, Clone)]
pub struct GpuComponent {
    pub comp_type: ComponentType,
    pub node1: usize,
    pub node2: usize,
    pub state: ComponentState,
}

#[derive(Debug, Clone, Copy)]
pub enum ComponentType {
    Resistor { r: f64 },
    Capacitor { c: f64 },
    Inductor { l: f64 },
    VoltageSource { v: f64 },
}

#[derive(Debug, Clone, Copy)]
pub struct ComponentState {
    pub current: f64,
    pub voltage: f64,
    pub internal: f64, // flux for L, charge for C
}

impl ComponentState {
    pub fn new() -> Self {
        Self {
            current: 0.0,
            voltage: 0.0,
            internal: 0.0,
        }
    }
}

/// GPU-ready circuit solver
pub struct GpuCircuit {
    pub nodes: Vec<Arc<GpuNode>>,
    pub components: Vec<GpuComponent>,
    pub time: f64,
    pub dt_base: f64,
}

impl GpuCircuit {
    pub fn new(num_nodes: usize) -> Self {
        let mut nodes = Vec::with_capacity(num_nodes);
        for _ in 0..num_nodes {
            nodes.push(Arc::new(GpuNode::new()));
        }
        
        Self {
            nodes,
            components: Vec::new(),
            time: 0.0,
            dt_base: 1e-6,
        }
    }
    
    pub fn add_component(&mut self, comp_type: ComponentType, node1: usize, node2: usize) {
        self.components.push(GpuComponent {
            comp_type,
            node1,
            node2,
            state: ComponentState::new(),
        });
    }
    
    /// Parallel component update (maps to GPU kernel)
    pub fn update_components(&mut self, dt: f64) {
        // In GPU version, this would be a parallel kernel
        // Each thread processes one component
        
        for comp in &mut self.components {
            // Read node voltages (would be texture/shared memory reads on GPU)
            let v1 = self.nodes[comp.node1].voltage.load();
            let v2 = self.nodes[comp.node2].voltage.load();
            let v_diff = v1 - v2;
            
            // Component-specific physics
            let (new_current, di) = match comp.comp_type {
                ComponentType::Resistor { r } => {
                    let i = v_diff / r;
                    (i, i - comp.state.current)
                }
                ComponentType::Capacitor { c } => {
                    let dv = v_diff - comp.state.voltage;
                    let i = c * dv / dt;
                    comp.state.voltage = v_diff;
                    comp.state.internal += i * dt; // Update charge
                    (i, i - comp.state.current)
                }
                ComponentType::Inductor { l } => {
                    let di = v_diff * dt / l;
                    let i = comp.state.current + di;
                    comp.state.internal += v_diff * dt; // Update flux
                    (i, di)
                }
                ComponentType::VoltageSource { v } => {
                    // Voltage source: calculate current based on connected impedance
                    // For now, assume it can supply any current needed
                    let i = if v > 0.0 && comp.state.internal == 0.0 {
                        // Just turned on - limit initial current
                        comp.state.internal = 1.0; // Mark as on
                        v / 1000.0 // Assume 1kΩ initial impedance
                    } else {
                        // Use small conductance to stabilize
                        (v - v1) * 1e3 // 1mS conductance
                    };
                    (i, i - comp.state.current)
                }
            };
            
            // Update component state
            comp.state.current = new_current;
            
            // Accumulate currents at nodes (atomic operations on GPU)
            if comp.node1 != 0 {
                self.nodes[comp.node1].current_sum.add(-new_current);
            }
            if comp.node2 != 0 {
                self.nodes[comp.node2].current_sum.add(new_current);
            }
        }
    }
    
    /// Parallel node update (maps to GPU kernel)
    pub fn update_nodes(&mut self, dt: f64) -> f64 {
        let mut max_change: f64 = 0.0;
        
        // In GPU version, this would be another parallel kernel
        for (i, node) in self.nodes.iter().enumerate() {
            if i == 0 { continue; } // Skip ground
            
            // KCL: Sum of currents = 0
            let current_imbalance = node.current_sum.load();
            
            // Add damping current
            let v_current = node.voltage.load();
            let damping_current = -v_current * node.conductance;
            let total_current = current_imbalance + damping_current;
            
            // Update voltage based on current imbalance
            // dV = I * dt / C (virtual capacitance for stability)
            let dv = total_current * dt / node.capacitance;
            
            // Apply damping to prevent oscillations
            let damped_dv = dv * 0.9; // 90% of change to ensure stability
            node.voltage.add(damped_dv);
            
            // Track convergence
            max_change = max_change.max(dv.abs());
            
            // Reset current sum for next iteration
            node.current_sum.store(0.0);
        }
        
        max_change
    }
    
    /// Single time step with sub-cycling for stability
    pub fn step(&mut self, dt_target: f64) -> bool {
        let sub_steps = (dt_target / self.dt_base).ceil() as usize;
        let dt = dt_target / sub_steps as f64;
        
        for _ in 0..sub_steps {
            // Clear node current accumulators
            for node in &self.nodes {
                node.current_sum.store(0.0);
            }
            
            // Parallel component update
            self.update_components(dt);
            
            // Parallel node update
            let max_change = self.update_nodes(dt);
            
            // Simple convergence check
            if max_change < 1e-6 {
                // Converged early
                break;
            }
        }
        
        self.time += dt_target;
        true
    }
    
    pub fn get_node_voltage(&self, node_id: usize) -> f64 {
        self.nodes.get(node_id)
            .map(|n| n.voltage.load())
            .unwrap_or(0.0)
    }
    
    pub fn get_component_current(&self, comp_id: usize) -> f64 {
        self.components.get(comp_id)
            .map(|c| c.state.current)
            .unwrap_or(0.0)
    }
    
    pub fn get_component_voltage(&self, comp_id: usize) -> f64 {
        self.components.get(comp_id)
            .map(|c| {
                let v1 = self.nodes[c.node1].voltage.load();
                let v2 = self.nodes[c.node2].voltage.load();
                v1 - v2
            })
            .unwrap_or(0.0)
    }
    
    pub fn set_voltage_source(&mut self, comp_id: usize, voltage: f64) {
        if let Some(comp) = self.components.get_mut(comp_id) {
            comp.comp_type = ComponentType::VoltageSource { v: voltage };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_atomic_f64() {
        let atomic = AtomicF64::new(1.0);
        atomic.add(2.5);
        assert!((atomic.load() - 3.5).abs() < 1e-10);
    }
    
    #[test]
    fn test_rc_circuit() {
        let mut circuit = GpuCircuit::new(3);
        
        // 5V -> R(1k) -> C(1µF) -> GND
        circuit.add_component(ComponentType::VoltageSource { v: 5.0 }, 1, 0);
        circuit.add_component(ComponentType::Resistor { r: 1000.0 }, 1, 2);
        circuit.add_component(ComponentType::Capacitor { c: 1e-6 }, 2, 0);
        
        // Run for 5ms
        for _ in 0..500 {
            circuit.step(10e-6);
        }
        
        let v_cap = circuit.get_node_voltage(2);
        // Should be about 99% of 5V
        assert!((v_cap - 5.0).abs() < 0.1);
    }
}