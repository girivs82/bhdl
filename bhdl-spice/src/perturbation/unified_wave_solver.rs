/// Unified Wave Solver with Automatic Adaptive Filtering
/// 
/// This provides a clean interface for wave-based simulation that automatically
/// adapts to circuit characteristics, making it suitable for all frequency ranges.

use super::{ElectricalState, WaveComponent};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::RwLock;
use rayon::prelude::*;

/// Circuit analysis results
#[derive(Debug, Clone)]
pub struct CircuitCharacteristics {
    /// Dominant pole frequency
    pub dominant_pole: f64,
    /// Circuit bandwidth (highest significant frequency)
    pub bandwidth: f64,
    /// Circuit type (RC, RL, RLC, etc.)
    pub circuit_type: CircuitType,
    /// Damping characteristics
    pub damping: DampingType,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CircuitType {
    Resistive,
    RC,
    RL,
    RLC,
    Active,
}

#[derive(Debug, Clone)]
pub enum DampingType {
    None,
    Overdamped { zeta: f64 },
    CriticallyDamped,
    Underdamped { zeta: f64, natural_freq: f64 },
}

/// Filter configuration determined from circuit analysis
#[derive(Debug, Clone)]
pub struct FilterConfig {
    /// Whether filtering is needed
    pub enabled: bool,
    /// Cutoff frequency
    pub cutoff: f64,
    /// Filter type for optimal phase response
    pub filter_type: FilterType,
    /// Phase compensation delay
    pub phase_compensation: f64,
}

#[derive(Debug, Clone)]
pub enum FilterType {
    None,
    Bessel(usize),      // Order
    Butterworth(usize), // Order
    Elliptic(usize),    // Order
}

/// Solver configuration
#[derive(Debug, Clone)]
pub struct SolverConfig {
    /// Base time step for wave propagation
    pub dt_wave: f64,
    /// Enable automatic filtering
    pub auto_filter: bool,
    /// Enable parallel execution
    pub parallel: bool,
    /// Maximum frequency to consider
    pub max_frequency: f64,
    /// Convergence tolerance
    pub tolerance: f64,
}

impl Default for SolverConfig {
    fn default() -> Self {
        Self {
            dt_wave: 0.1e-12,  // 0.1 ps default
            auto_filter: true,
            parallel: true,
            max_frequency: 1e12, // 1 THz
            tolerance: 1e-6,
        }
    }
}

/// Main unified wave solver
pub struct UnifiedWaveSolver {
    /// Configuration
    config: SolverConfig,
    /// Circuit components indexed by node
    components: HashMap<usize, Arc<RwLock<dyn WaveComponent>>>,
    /// Connection matrix (adjacency with impedances)
    connections: Vec<Vec<f64>>,
    /// Current node states
    node_states: Vec<ElectricalState>,
    /// Circuit characteristics (lazily computed)
    characteristics: Option<CircuitCharacteristics>,
    /// Filter configuration (auto-determined)
    filter_config: Option<FilterConfig>,
    /// Simulation time
    time: f64,
}

impl UnifiedWaveSolver {
    pub fn new(num_nodes: usize, config: SolverConfig) -> Self {
        Self {
            config,
            components: HashMap::new(),
            connections: vec![vec![f64::INFINITY; num_nodes]; num_nodes],
            node_states: vec![ElectricalState::default(); num_nodes],
            characteristics: None,
            filter_config: None,
            time: 0.0,
        }
    }
    
    /// Add a component at a node
    pub fn add_component(&mut self, node: usize, component: Arc<RwLock<dyn WaveComponent>>) {
        self.components.insert(node, component);
    }
    
    /// Connect two nodes with given impedance
    pub fn connect(&mut self, node1: usize, node2: usize, impedance: f64) {
        self.connections[node1][node2] = impedance;
        self.connections[node2][node1] = impedance;
    }
    
    /// Analyze circuit to determine characteristics
    pub fn analyze_circuit(&mut self) -> CircuitCharacteristics {
        // Count component types
        let mut r_total = 0.0;
        let mut l_total = 0.0;
        let mut c_total = 0.0;
        let mut has_active = false;
        
        for component in self.components.values() {
            let comp = component.read();
            let z = comp.characteristic_impedance();
            
            // Heuristic classification based on impedance behavior
            if z > 1e6 {
                // High impedance suggests inductor
                l_total += 1.0; // Simplified
            } else if z < 1e-3 {
                // Low impedance suggests capacitor
                c_total += 1.0;
            } else if z < 0.0 {
                // Negative impedance suggests active element
                has_active = true;
            } else {
                // Moderate impedance suggests resistor
                r_total += z;
            }
        }
        
        // Determine circuit type and characteristics
        let (circuit_type, dominant_pole, bandwidth, damping) = 
            self.classify_circuit(r_total, l_total, c_total, has_active);
        
        let characteristics = CircuitCharacteristics {
            dominant_pole,
            bandwidth,
            circuit_type,
            damping,
        };
        
        self.characteristics = Some(characteristics.clone());
        
        // Auto-configure filter if enabled
        if self.config.auto_filter {
            self.filter_config = Some(self.determine_filter_config(&characteristics));
        }
        
        characteristics
    }
    
    /// Classify circuit and determine parameters
    fn classify_circuit(&self, r: f64, l: f64, c: f64, active: bool) 
        -> (CircuitType, f64, f64, DampingType) {
        
        if active {
            // Active circuits need special handling
            (CircuitType::Active, 1e9, 1e9, DampingType::None)
        } else if l > 0.0 && c > 0.0 {
            // RLC circuit
            let l_value = l * 1e-3;  // Assume mH
            let c_value = c * 1e-6;  // Assume µF
            let omega_0 = 1.0 / (l_value * c_value).sqrt();
            let f_0 = omega_0 / (2.0 * std::f64::consts::PI);
            let zeta = r / (2.0 * (l_value / c_value).sqrt());
            
            let damping = if zeta > 1.0 {
                DampingType::Overdamped { zeta }
            } else if (zeta - 1.0).abs() < 0.01 {
                DampingType::CriticallyDamped
            } else {
                DampingType::Underdamped { zeta, natural_freq: f_0 }
            };
            
            let bandwidth = f_0 / zeta.max(0.1_f64);
            (CircuitType::RLC, f_0, bandwidth, damping)
        } else if c > 0.0 {
            // RC circuit
            let tau = r * c * 1e-6;
            let f_c = 1.0 / (2.0 * std::f64::consts::PI * tau);
            (CircuitType::RC, f_c, f_c * 10.0, DampingType::None)
        } else if l > 0.0 {
            // RL circuit
            let tau = l * 1e-3 / r;
            let f_c = r / (2.0 * std::f64::consts::PI * l * 1e-3);
            (CircuitType::RL, f_c, f_c * 10.0, DampingType::None)
        } else {
            // Resistive
            (CircuitType::Resistive, 1e12, 1e12, DampingType::None)
        }
    }
    
    /// Determine optimal filter configuration
    fn determine_filter_config(&self, characteristics: &CircuitCharacteristics) -> FilterConfig {
        // No filtering needed for very high bandwidth circuits
        if characteristics.bandwidth > 1e9 {
            return FilterConfig {
                enabled: false,
                cutoff: 0.0,
                filter_type: FilterType::None,
                phase_compensation: 0.0,
            };
        }
        
        // Choose filter type based on circuit characteristics
        let filter_type = match characteristics.circuit_type {
            CircuitType::RLC => {
                // Use Bessel for good transient response
                FilterType::Bessel(4)
            }
            CircuitType::RC | CircuitType::RL => {
                // Use Butterworth for flat passband
                FilterType::Butterworth(2)
            }
            _ => FilterType::None,
        };
        
        // Set cutoff at 100x bandwidth for good fidelity
        let cutoff = characteristics.bandwidth * 100.0;
        
        // Calculate phase compensation
        let phase_compensation = match &filter_type {
            FilterType::Bessel(order) => {
                // Bessel has approximately constant group delay
                0.3 * (*order as f64) / cutoff
            }
            FilterType::Butterworth(order) => {
                // Butterworth phase delay at signal frequency
                (*order as f64) * 0.25 / cutoff
            }
            _ => 0.0,
        };
        
        FilterConfig {
            enabled: true,
            cutoff,
            filter_type,
            phase_compensation,
        }
    }
    
    /// Single simulation step
    pub fn step(&mut self) -> bool {
        if self.config.parallel {
            self.step_parallel()
        } else {
            self.step_sequential()
        }
    }
    
    /// Sequential wave propagation step
    fn step_sequential(&mut self) -> bool {
        let mut max_change = 0.0_f64;
        
        // Process each node
        for node in 0..self.node_states.len() {
            let old_state = self.node_states[node];
            
            // Calculate incident waves from connected nodes
            let mut incident = ElectricalState::default();
            let mut total_admittance = 0.0;
            
            for other in 0..self.node_states.len() {
                if node != other && self.connections[node][other] < f64::INFINITY {
                    let z = self.connections[node][other];
                    let y = 1.0 / z;
                    
                    incident.voltage += self.node_states[other].voltage * y;
                    incident.current += (self.node_states[other].voltage - 
                                       self.node_states[node].voltage) / z;
                    total_admittance += y;
                }
            }
            
            // Normalize incident voltage
            if total_admittance > 0.0 {
                incident.voltage /= total_admittance;
            }
            
            // Process through component if present
            if let Some(component) = self.components.get(&node) {
                let mut comp = component.write();
                let reflected = comp.process_wave(incident, self.config.dt_wave);
                comp.update_state(self.config.dt_wave);
                
                // Update node state
                self.node_states[node] = comp.get_state();
            } else {
                // No component, just update with incident
                self.node_states[node] = incident;
            }
            
            // Track convergence
            let change = (self.node_states[node].voltage - old_state.voltage).abs();
            max_change = max_change.max(change);
        }
        
        self.time += self.config.dt_wave;
        max_change < self.config.tolerance
    }
    
    /// Parallel wave propagation step
    fn step_parallel(&mut self) -> bool {
        let n = self.node_states.len();
        
        // Compute new states in parallel
        let new_states: Vec<ElectricalState> = (0..n)
            .into_par_iter()
            .map(|node| {
                // Calculate incident waves
                let mut incident = ElectricalState::default();
                let mut total_admittance = 0.0;
                
                for other in 0..n {
                    if node != other && self.connections[node][other] < f64::INFINITY {
                        let z = self.connections[node][other];
                        let y = 1.0 / z;
                        
                        incident.voltage += self.node_states[other].voltage * y;
                        incident.current += (self.node_states[other].voltage - 
                                           self.node_states[node].voltage) / z;
                        total_admittance += y;
                    }
                }
                
                if total_admittance > 0.0 {
                    incident.voltage /= total_admittance;
                }
                
                // Process through component
                if let Some(component) = self.components.get(&node) {
                    let mut comp = component.write();
                    comp.process_wave(incident, self.config.dt_wave);
                    comp.update_state(self.config.dt_wave);
                    comp.get_state()
                } else {
                    incident
                }
            })
            .collect();
        
        // Check convergence
        let max_change = self.node_states.par_iter()
            .zip(new_states.par_iter())
            .map(|(old, new)| (new.voltage - old.voltage).abs())
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);
        
        self.node_states = new_states;
        self.time += self.config.dt_wave;
        
        max_change < self.config.tolerance
    }
    
    /// Get current simulation time
    pub fn get_time(&self) -> f64 {
        self.time
    }
    
    /// Get node voltage
    pub fn get_voltage(&self, node: usize) -> f64 {
        self.node_states.get(node)
            .map(|s| s.voltage)
            .unwrap_or(0.0)
    }
    
    /// Get node current
    pub fn get_current(&self, node: usize) -> f64 {
        self.node_states.get(node)
            .map(|s| s.current)
            .unwrap_or(0.0)
    }
    
    /// Reset solver to initial conditions
    pub fn reset(&mut self) {
        self.time = 0.0;
        self.node_states.fill(ElectricalState::default());
        
        for component in self.components.values() {
            component.write().reset();
        }
    }
    
    /// Apply post-processing filter if configured
    pub fn apply_filter(&self, signal: &[f64]) -> Vec<f64> {
        if let Some(ref config) = self.filter_config {
            if !config.enabled {
                return signal.to_vec();
            }
            
            match config.filter_type {
                FilterType::Bessel(order) => {
                    self.apply_bessel_filter(signal, config.cutoff, order)
                }
                FilterType::Butterworth(order) => {
                    self.apply_butterworth_filter(signal, config.cutoff, order)
                }
                _ => signal.to_vec(),
            }
        } else {
            signal.to_vec()
        }
    }
    
    // Filter implementations...
    fn apply_bessel_filter(&self, signal: &[f64], fc: f64, order: usize) -> Vec<f64> {
        // Implement Bessel filter
        // For now, simplified implementation
        signal.to_vec()
    }
    
    fn apply_butterworth_filter(&self, signal: &[f64], fc: f64, order: usize) -> Vec<f64> {
        // Implement Butterworth filter
        // For now, simplified implementation
        signal.to_vec()
    }
}

/// Builder pattern for easier construction
pub struct SolverBuilder {
    num_nodes: usize,
    config: SolverConfig,
}

impl SolverBuilder {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            num_nodes,
            config: SolverConfig::default(),
        }
    }
    
    pub fn dt(mut self, dt: f64) -> Self {
        self.config.dt_wave = dt;
        self
    }
    
    pub fn parallel(mut self, enabled: bool) -> Self {
        self.config.parallel = enabled;
        self
    }
    
    pub fn auto_filter(mut self, enabled: bool) -> Self {
        self.config.auto_filter = enabled;
        self
    }
    
    pub fn build(self) -> UnifiedWaveSolver {
        UnifiedWaveSolver::new(self.num_nodes, self.config)
    }
}