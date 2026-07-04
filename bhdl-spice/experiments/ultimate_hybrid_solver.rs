/// Ultimate Hybrid Solver: 80% + Smart Damping
/// 
/// Combines the best of both approaches:
/// - Phase 1 (0-80%): Fast ramping with relaxed parameters (80% approach)
/// - Phase 2 (80-100%): Smart damping with oscillation control (critical damping theory)
/// 
/// Goal: Newton-level accuracy with most of the speed advantage

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Reuse element definitions
pub trait Element: Send + Sync {
    fn element_type(&self) -> ElementType;
    fn conductance(&self) -> f64 { 0.0 }
    fn is_nonlinear(&self) -> bool { false }
    fn current_at_voltage(&self, v: f64) -> f64;
    fn conductance_at_voltage(&self, v: f64) -> f64;
    fn get_voltage(&self) -> f64;
    fn set_voltage(&mut self, v: f64);
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElementType {
    Resistor,
    VoltageSource,
    Diode,
}

pub struct Resistor {
    resistance: f64,
    voltage: f64,
}

impl Resistor {
    pub fn new(r: f64) -> Self {
        Self { resistance: r, voltage: 0.0 }
    }
}

impl Element for Resistor {
    fn element_type(&self) -> ElementType { ElementType::Resistor }
    fn conductance(&self) -> f64 { 1.0 / self.resistance }
    fn current_at_voltage(&self, v: f64) -> f64 { v / self.resistance }
    fn conductance_at_voltage(&self, _v: f64) -> f64 { 1.0 / self.resistance }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

pub struct VoltageSource {
    voltage: f64,
}

impl VoltageSource {
    pub fn new(v: f64) -> Self {
        Self { voltage: v }
    }
}

impl Element for VoltageSource {
    fn element_type(&self) -> ElementType { ElementType::VoltageSource }
    fn current_at_voltage(&self, _v: f64) -> f64 { 0.0 }
    fn conductance_at_voltage(&self, _v: f64) -> f64 { 0.0 }
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

pub struct Diode {
    is: f64,
    vt: f64,
    voltage: f64,
}

impl Diode {
    pub fn new(is: f64, vt: f64) -> Self {
        Self { is, vt, voltage: 0.0 }
    }
}

impl Element for Diode {
    fn element_type(&self) -> ElementType { ElementType::Diode }
    fn is_nonlinear(&self) -> bool { true }
    
    fn current_at_voltage(&self, v: f64) -> f64 {
        const MAX_EXP: f64 = 50.0;
        let v_norm = v / self.vt;
        
        if v_norm > MAX_EXP {
            let i_max = self.is * (MAX_EXP.exp() - 1.0);
            let g_max = (self.is / self.vt) * MAX_EXP.exp();
            i_max + g_max * (v - MAX_EXP * self.vt)
        } else if v_norm < -5.0 {
            -self.is
        } else {
            self.is * (v_norm.exp() - 1.0)
        }
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        const MAX_EXP: f64 = 50.0;
        const MIN_G: f64 = 1e-14;
        let v_norm = v / self.vt;
        
        if v_norm > MAX_EXP {
            (self.is / self.vt) * MAX_EXP.exp()
        } else if v_norm < -5.0 {
            MIN_G
        } else {
            ((self.is / self.vt) * v_norm.exp()).max(MIN_G)
        }
    }
    
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

// Smart damping controller for Phase 2
#[derive(Debug, Clone, Copy)]
enum DampingStrategy {
    ImmediateOverdamp,  // Strategy 1: Immediate overdamping on oscillation
    ControlledDecay,    // Strategy 2: Allow 2-3 oscillations with decay
}

struct SmartDampingController {
    strategy: DampingStrategy,
    
    // Oscillation detection
    last_gradients: Vec<f64>,
    sign_changes: usize,
    
    // Damping parameters
    damping_factor: f64,
    base_damping: f64,
    
    // Adaptive step control
    adaptive_step: f64,
    base_step: f64,
    
    // History for oscillation detection
    max_history: usize,
}

impl SmartDampingController {
    fn new(strategy: DampingStrategy) -> Self {
        Self {
            strategy,
            last_gradients: Vec::new(),
            sign_changes: 0,
            damping_factor: 0.3,  // Start underdamped for speed
            base_damping: 0.3,
            adaptive_step: 0.01,
            base_step: 0.01,
            max_history: 5,
        }
    }
    
    fn update_damping(&mut self, gradient: f64) {
        // Store gradient history
        self.last_gradients.push(gradient.abs());
        if self.last_gradients.len() > self.max_history {
            self.last_gradients.remove(0);
        }
        
        // Detect sign changes in gradient (oscillation indicator)
        if self.last_gradients.len() >= 3 {
            let len = self.last_gradients.len();
            let current_change = self.last_gradients[len-1] - self.last_gradients[len-2];
            let prev_change = self.last_gradients[len-2] - self.last_gradients[len-3];
            
            if current_change.signum() != prev_change.signum() && gradient.abs() > 1e-10 {
                self.sign_changes += 1;
            }
        }
        
        // Apply damping strategy
        match self.strategy {
            DampingStrategy::ImmediateOverdamp => {
                if self.sign_changes > 2 {
                    // Immediate overdamping when oscillation detected
                    self.damping_factor = 0.9;
                    self.adaptive_step *= 0.5;  // Smaller steps
                } else {
                    // Gradually return to underdamped for speed
                    self.damping_factor = (self.damping_factor * 0.9 + self.base_damping * 0.1).max(self.base_damping);
                    self.adaptive_step = (self.adaptive_step + self.base_step) / 2.0;
                }
            }
            DampingStrategy::ControlledDecay => {
                if self.sign_changes > 1 {
                    // Reduce step size but allow some oscillation
                    self.adaptive_step *= 0.7;
                    self.damping_factor = (self.damping_factor + 0.6) / 2.0;  // Move toward critical damping
                    
                    if self.sign_changes > 4 {
                        // Too many oscillations - go overdamped
                        self.damping_factor = 0.85;
                    }
                }
            }
        }
        
        // Reset sign change counter periodically
        if self.sign_changes > 8 {
            self.sign_changes = 0;
        }
        
        // Bounds checking
        self.damping_factor = self.damping_factor.clamp(0.1, 0.95);
        self.adaptive_step = self.adaptive_step.clamp(0.001, 0.05);
    }
    
    fn get_damping(&self) -> f64 {
        self.damping_factor
    }
    
    fn get_step_size(&self) -> f64 {
        self.adaptive_step
    }
    
    fn get_oscillation_metric(&self) -> f64 {
        // Calculate oscillation severity
        if self.last_gradients.len() < 3 {
            return 0.0;
        }
        
        let recent_changes = (self.sign_changes as f64).min(4.0) / 4.0;
        
        // Calculate variance in gradients
        let mean_grad = self.last_gradients.iter().sum::<f64>() / self.last_gradients.len() as f64;
        let variance = self.last_gradients.iter()
            .map(|&g| (g - mean_grad).powi(2))
            .sum::<f64>() / self.last_gradients.len() as f64;
        
        let normalized_variance = (variance.sqrt() / (mean_grad + 1e-10)).min(1.0);
        
        // Combine metrics
        0.7 * recent_changes + 0.3 * normalized_variance
    }
}

// Ultimate hybrid solver
pub struct UltimateHybridSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    smart_damping: SmartDampingController,
}

impl UltimateHybridSolver {
    pub fn new(num_nodes: usize, strategy: DampingStrategy) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            smart_damping: SmartDampingController::new(strategy),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (Vec<f64>, f64, usize, f64, f64) {
        let start = Instant::now();
        
        // Setup
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        let mut phase1_iterations = 0;
        let mut phase2_iterations = 0;
        
        // Find voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        println!("  [Starting Ultimate Hybrid: 80% + Smart Damping]");
        
        // Phase 1: Fast ramping (0-80%) - Original 80% approach
        let mut ramp = 0.0;
        let phase1_end = 0.8;
        let phase1_step = 0.05;
        
        println!("  [Phase 1: Fast ramping 0-80%]");
        while ramp < phase1_end {
            ramp = f64::min(ramp + phase1_step, phase1_end);
            
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            let phase1_iters = self.solve_phase1(&mut total_iterations);
            phase1_iterations += phase1_iters;
        }
        
        println!("  [Phase 2: Smart damping 80-100%]");
        
        // Phase 2: Smart damping (80-100%) - Critical damping theory
        while ramp < 0.999 {
            let step_size = self.smart_damping.get_step_size();
            ramp = f64::min(ramp + step_size, 1.0);
            
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            let phase2_iters = self.solve_phase2(&mut total_iterations);
            phase2_iterations += phase2_iters;
            
            // Show oscillation metric for first few iterations
            if phase2_iterations <= 50 && phase2_iterations % 10 == 0 {
                let oscillation = self.smart_damping.get_oscillation_metric();
                let damping = self.smart_damping.get_damping();
                println!("    [Phase2 iter {}: ramp={:.3}, osc={:.3}, damp={:.3}]", 
                         phase2_iterations, ramp, oscillation, damping);
            }
        }
        
        // CRITICAL FIX: Ensure voltage sources reach their full target values
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);  // Set to full target voltage, not ramped
        }
        
        // Final solve with maximum precision
        println!("  [Final precision solve]");
        self.solve_final(&mut total_iterations);
        
        // Get nonlinear device voltages
        let mut device_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        device_voltages.push(self.node_voltages[pos] - self.node_voltages[neg]);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("  [Phase 1: {} iters | Phase 2: {} iters | Total: {} iters]", 
                 phase1_iterations, phase2_iterations, total_iterations);
        
        let final_oscillation = self.smart_damping.get_oscillation_metric();
        
        (device_voltages, 
         self.source_currents.get(0).copied().unwrap_or(0.0).abs(), 
         total_iterations, 
         elapsed,
         final_oscillation)
    }
    
    // Phase 1: Fast ramping (0-80%)
    fn solve_phase1(&mut self, total_iterations: &mut usize) -> usize {
        let max_iter = 30;
        let tol = 1e-8;  // Relaxed tolerance
        let damping = 0.5;  // Underdamped for speed
        let mut local_iters = 0;
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            local_iters += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    return local_iters;
                }
            } else {
                return local_iters;
            }
        }
        
        local_iters
    }
    
    // Phase 2: Smart damping (80-100%)
    fn solve_phase2(&mut self, total_iterations: &mut usize) -> usize {
        let max_iter = 50;
        let tol = 1e-12;  // High precision
        let mut local_iters = 0;
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            local_iters += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                let mut gradient_sum = 0.0f64;
                
                // Calculate current gradient
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    gradient_sum += delta.abs();
                }
                
                // Update smart damping based on gradient
                self.smart_damping.update_damping(gradient_sum);
                let adaptive_damping = self.smart_damping.get_damping();
                
                // Apply adaptive damping
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + adaptive_damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    return local_iters;
                }
            } else {
                return local_iters;
            }
        }
        
        local_iters
    }
    
    // Final solve with maximum precision
    fn solve_final(&mut self, total_iterations: &mut usize) {
        let max_iter = 100;
        let tol = 1e-15;  // Maximum precision
        let damping = 0.8;  // High damping for final stability
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    return;
                }
            } else {
                return;
            }
        }
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        let gmin = 1e-12;
        for i in 0..n {
            a[(i, i)] = gmin;
        }
        
        let mut vs_idx = 0;
        
        for &(elem_idx, pos, neg) in &self.connections {
            let elem = &self.elements[elem_idx];
            
            match elem.element_type() {
                ElementType::VoltageSource => {
                    let row = n + vs_idx;
                    
                    if pos > 0 {
                        a[(pos-1, row)] = 1.0;
                        a[(row, pos-1)] = 1.0;
                    }
                    if neg > 0 {
                        a[(neg-1, row)] = -1.0;
                        a[(row, neg-1)] = -1.0;
                    }
                    
                    b[row] = elem.get_voltage();
                    vs_idx += 1;
                }
                _ => {
                    let v_elem = self.node_voltages[pos] - self.node_voltages[neg];
                    let g = elem.conductance_at_voltage(v_elem);
                    let i_elem = elem.current_at_voltage(v_elem);
                    let i_norton = i_elem - g * v_elem;
                    
                    if pos > 0 {
                        a[(pos-1, pos-1)] += g;
                        b[pos-1] -= i_norton;
                    }
                    if neg > 0 {
                        a[(neg-1, neg-1)] += g;
                        b[neg-1] += i_norton;
                    }
                    if pos > 0 && neg > 0 {
                        a[(pos-1, neg-1)] -= g;
                        a[(neg-1, pos-1)] -= g;
                    }
                }
            }
        }
        
        (a, b)
    }
}

// Newton-Raphson solver using IDENTICAL Element implementations
pub struct NewtonRaphsonSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl NewtonRaphsonSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (Vec<f64>, f64, usize, f64) {
        let start = Instant::now();
        
        // Setup voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let max_iter = 200;
        let tol = 1e-15;
        let damping = 0.8;
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    break;
                }
            } else {
                break;
            }
        }
        
        let mut device_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        device_voltages.push(self.node_voltages[pos] - self.node_voltages[neg]);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (device_voltages, 
         self.source_currents.get(0).copied().unwrap_or(0.0).abs(), 
         iterations, 
         elapsed)
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        let gmin = 1e-12;
        for i in 0..n {
            a[(i, i)] = gmin;
        }
        
        let mut vs_idx = 0;
        
        for &(elem_idx, pos, neg) in &self.connections {
            let elem = &self.elements[elem_idx];
            
            match elem.element_type() {
                ElementType::VoltageSource => {
                    let row = n + vs_idx;
                    
                    if pos > 0 {
                        a[(pos-1, row)] = 1.0;
                        a[(row, pos-1)] = 1.0;
                    }
                    if neg > 0 {
                        a[(neg-1, row)] = -1.0;
                        a[(row, neg-1)] = -1.0;
                    }
                    
                    b[row] = elem.get_voltage();
                    vs_idx += 1;
                }
                _ => {
                    let v_elem = self.node_voltages[pos] - self.node_voltages[neg];
                    let g = elem.conductance_at_voltage(v_elem);
                    let i_elem = elem.current_at_voltage(v_elem);
                    let i_norton = i_elem - g * v_elem;
                    
                    if pos > 0 {
                        a[(pos-1, pos-1)] += g;
                        b[pos-1] -= i_norton;
                    }
                    if neg > 0 {
                        a[(neg-1, neg-1)] += g;
                        b[neg-1] += i_norton;
                    }
                    if pos > 0 && neg > 0 {
                        a[(pos-1, neg-1)] -= g;
                        a[(neg-1, pos-1)] -= g;
                    }
                }
            }
        }
        
        (a, b)
    }
}

// SPICE reference solver
fn spice_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    let mut vd = 0.7;
    for _ in 0..100 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let g = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        let delta = f / g;
        vd -= delta;
        if delta.abs() < 1e-15 {
            break;
        }
    }
    let id = (vs - vd) / rs;
    (vd, id)
}

fn main() {
    println!("=== ULTIMATE HYBRID SOLVER TEST ===");
    println!("80% Fast Ramping + Smart Damping for Newton-level accuracy\n");
    
    let test_cases = [
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", 1.0, 100.0, 1e-12, 0.050),
        ("Low current", 0.1, 1000.0, 1e-12, 0.026),
        ("High voltage", 5.0, 100.0, 1e-12, 0.026),
        ("Low resistance", 1.0, 10.0, 1e-12, 0.026),
        ("Extreme low current", 0.05, 2000.0, 1e-12, 0.026),
        ("High current", 10.0, 50.0, 1e-12, 0.026),
    ];
    
    // Test both damping strategies
    let strategies = [
        (DampingStrategy::ImmediateOverdamp, "Immediate Overdamp"),
        (DampingStrategy::ControlledDecay, "Controlled Decay"),
    ];
    
    for &(strategy, strategy_name) in &strategies {
        println!("\n🔧 TESTING STRATEGY: {}", strategy_name);
        println!("{:>20} | {:>12} | {:>12} | {:>12} | {:>12} | {:>8} | {:>8} | {:>8} | {:>8}", 
                 "Test Case", "SPICE Vd", "SPICE Id", "Ultimate Vd", "Ultimate Id", "Error %", "Iters", "Time ms", "Osc");
        println!("{}", "=".repeat(120));
        
        let mut total_error = 0.0;
        let mut total_time = 0.0;
        let mut total_iterations = 0;
        
        for &(test_name, vs, rs, is, vt) in &test_cases {
            let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
            
            let mut solver = UltimateHybridSolver::new(3, strategy);
            
            let v = solver.add_element(Box::new(VoltageSource::new(vs)));
            let r = solver.add_element(Box::new(Resistor::new(rs)));
            let d = solver.add_element(Box::new(Diode::new(is, vt)));
            
            solver.connect(v, 1, 0);
            solver.connect(r, 1, 2);
            solver.connect(d, 2, 0);
            
            let (device_voltages, id_ultimate, iters, time, oscillation) = solver.solve();
            let vd_ultimate = device_voltages.get(0).copied().unwrap_or(0.0);
            
            let v_err = ((vd_ultimate - vd_ref) / vd_ref * 100.0).abs();
            let i_err = ((id_ultimate - id_ref) / id_ref * 100.0).abs();
            let max_err = v_err.max(i_err);
            
            total_error += max_err;
            total_time += time;
            total_iterations += iters;
            
            println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1} | {:>8.3}", 
                     test_name, 
                     vd_ref, id_ref * 1000.0,
                     vd_ultimate, id_ultimate * 1000.0,
                     max_err, iters, time, oscillation);
        }
        
        println!("{}", "=".repeat(120));
        
        let n_cases = test_cases.len() as f64;
        println!("\n{} Results:", strategy_name);
        println!("  Average error: {:.6}%", total_error / n_cases);
        println!("  Average time: {:.1}ms", total_time / n_cases);
        println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    }
    
    // Newton-Raphson vs Ultimate Hybrid direct comparison
    println!("\n🔥 DIRECT NEWTON-RAPHSON vs ULTIMATE HYBRID COMPARISON");
    println!("Using IDENTICAL Element implementations, device models, and circuit setup\n");
    
    println!("{:>20} | {:>12} | {:>12} | {:>12} | {:>12} | {:>8} | {:>8} | {:>8} | {:>8}", 
             "Test Case", "NR Vd", "NR Id", "Hybrid Vd", "Hybrid Id", "V Err %", "I Err %", "NR ms", "Hybrid ms");
    println!("{}", "=".repeat(130));
    
    let mut total_v_error = 0.0;
    let mut total_i_error = 0.0;
    let mut total_nr_time = 0.0;
    let mut total_hybrid_time = 0.0;
    let mut total_nr_iters = 0;
    let mut total_hybrid_iters = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        // Newton-Raphson solve with SAME elements
        let mut nr_solver = NewtonRaphsonSolver::new(3);
        let v = nr_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = nr_solver.add_element(Box::new(Resistor::new(rs)));
        let d = nr_solver.add_element(Box::new(Diode::new(is, vt)));
        nr_solver.connect(v, 1, 0);
        nr_solver.connect(r, 1, 2);
        nr_solver.connect(d, 2, 0);
        
        let (nr_device_voltages, nr_id, nr_iters, nr_time) = nr_solver.solve();
        let nr_vd = nr_device_voltages.get(0).copied().unwrap_or(0.0);
        
        // Ultimate Hybrid solve with SAME elements (using best strategy)
        let mut hybrid_solver = UltimateHybridSolver::new(3, DampingStrategy::ImmediateOverdamp);
        let v = hybrid_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = hybrid_solver.add_element(Box::new(Resistor::new(rs)));
        let d = hybrid_solver.add_element(Box::new(Diode::new(is, vt)));
        hybrid_solver.connect(v, 1, 0);
        hybrid_solver.connect(r, 1, 2);
        hybrid_solver.connect(d, 2, 0);
        
        let (hybrid_device_voltages, hybrid_id, hybrid_iters, hybrid_time, _osc) = hybrid_solver.solve();
        let hybrid_vd = hybrid_device_voltages.get(0).copied().unwrap_or(0.0);
        
        // Calculate errors
        let v_err = if nr_vd != 0.0 { ((hybrid_vd - nr_vd) / nr_vd * 100.0).abs() } else { 0.0 };
        let i_err = if nr_id != 0.0 { ((hybrid_id - nr_id) / nr_id * 100.0).abs() } else { 0.0 };
        
        total_v_error += v_err;
        total_i_error += i_err;
        total_nr_time += nr_time;
        total_hybrid_time += hybrid_time;
        total_nr_iters += nr_iters;
        total_hybrid_iters += hybrid_iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8.4} | {:>8.1} | {:>8.1}", 
                 test_name, 
                 nr_vd, nr_id * 1000.0,
                 hybrid_vd, hybrid_id * 1000.0,
                 v_err, i_err, nr_time, hybrid_time);
    }
    
    println!("{}", "=".repeat(130));
    
    let n_cases = test_cases.len() as f64;
    println!("\n📊 APPLES-TO-APPLES COMPARISON RESULTS:");
    println!("  Newton-Raphson:");
    println!("    Average time: {:.1}ms", total_nr_time / n_cases);
    println!("    Average iterations: {:.0}", total_nr_iters as f64 / n_cases);
    println!("  Ultimate Hybrid (ImmediateOverdamp):");
    println!("    Average time: {:.1}ms", total_hybrid_time / n_cases);
    println!("    Average iterations: {:.0}", total_hybrid_iters as f64 / n_cases);
    println!("    Average voltage error: {:.6}%", total_v_error / n_cases);
    println!("    Average current error: {:.6}%", total_i_error / n_cases);
    
    let speed_ratio = if total_hybrid_time > 0.0 { total_nr_time / total_hybrid_time } else { 0.0 };
    let iter_ratio = if total_hybrid_iters > 0 { total_nr_iters as f64 / total_hybrid_iters as f64 } else { 0.0 };
    
    println!("\n🏆 FINAL VERDICT:");
    if speed_ratio > 1.0 {
        println!("  Newton-Raphson is {:.1}x FASTER", speed_ratio);
    } else {
        println!("  Ultimate Hybrid is {:.1}x FASTER", 1.0 / speed_ratio);
    }
    if iter_ratio > 1.0 {
        println!("  Newton-Raphson uses {:.1}x FEWER iterations", iter_ratio);
    } else {
        println!("  Ultimate Hybrid uses {:.1}x FEWER iterations", 1.0 / iter_ratio);
    }
    println!("  Ultimate Hybrid maximum error: {:.4}%", (total_v_error / n_cases).max(total_i_error / n_cases));
    
    println!("\n=== ULTIMATE HYBRID APPROACH ===");
    println!("Phase 1 (0-80%): Fast ramping, relaxed tolerance, underdamped");
    println!("Phase 2 (80-100%): Smart damping with oscillation control");
    println!("Final: Maximum precision solve with high damping");
    println!("Goal: Newton-level accuracy with speed advantage");
}