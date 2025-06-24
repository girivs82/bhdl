/// Adaptive Ramp Logarithmic Gradient Solver
/// 
/// Implements your binary search idea correctly:
/// 1. We're searching for the right ramp factor to apply to voltage sources
/// 2. At each ramp level, we measure how well the circuit converges
/// 3. Binary search finds the optimal ramp that gives stable convergence
/// 
/// The key insight: Instead of tracking voltage error against a target,
/// we track convergence quality at each ramp level

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

// Adaptive ramp controller
struct AdaptiveRampController {
    // Current state
    current_ramp: f64,
    step_size: f64,
    
    // Track convergence quality at different ramps
    last_convergence_quality: Option<f64>,
    last_voltage_change: Option<f64>,
    
    // Best solution tracking
    best_ramp: f64,
    best_quality: f64,
    
    iterations: usize,
}

impl AdaptiveRampController {
    fn new() -> Self {
        Self {
            current_ramp: 0.1,  // Start low
            step_size: 0.1,     // Initial step
            
            last_convergence_quality: None,
            last_voltage_change: None,
            
            best_ramp: 0.1,
            best_quality: f64::INFINITY,
            
            iterations: 0,
        }
    }
    
    fn update(&mut self, converged: bool, iterations: usize, voltage_change: f64) -> (f64, bool) {
        self.iterations += 1;
        
        // Calculate convergence quality (lower is better)
        let quality = if converged {
            // Good convergence - quality based on iteration count
            iterations as f64
        } else {
            // Failed to converge - high penalty
            1000.0 + voltage_change * 100.0
        };
        
        // Track best
        if quality < self.best_quality {
            self.best_quality = quality;
            self.best_ramp = self.current_ramp;
        }
        
        // Check if we've found good solution
        if converged && iterations < 20 && self.current_ramp > 0.9 {
            return (self.current_ramp, true);
        }
        
        // Adaptive step adjustment
        if let Some(last_quality) = self.last_convergence_quality {
            if quality > last_quality * 1.5 {
                // Getting worse - reduce step and reverse
                self.step_size *= 0.5;
                self.step_size = -self.step_size;
            } else if quality < last_quality * 0.8 {
                // Getting better - can increase step
                self.step_size = self.step_size.abs() * 1.2;
            }
        }
        
        // Update for next iteration
        self.last_convergence_quality = Some(quality);
        self.last_voltage_change = Some(voltage_change);
        
        // Next ramp value
        self.current_ramp += self.step_size;
        self.current_ramp = self.current_ramp.clamp(0.0, 1.0);
        
        // Check termination
        let done = self.iterations > 30 || self.step_size.abs() < 0.001;
        
        (if done { self.best_ramp } else { self.current_ramp }, done)
    }
}

// Main solver
pub struct AdaptiveRampLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: AdaptiveRampController,
}

impl AdaptiveRampLogGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: AdaptiveRampController::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn get_diode_voltage(&self) -> f64 {
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        return self.node_voltages[pos] - self.node_voltages[neg];
                    }
                }
            }
        }
        0.0
    }
    
    pub fn solve(&mut self) -> (f64, f64, usize, f64) {
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
        
        // Find voltage sources and their values
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Adaptive ramp search
        loop {
            // Reset node voltages for clean solve
            self.node_voltages = vec![0.0; self.num_nodes];
            
            // Set sources to current ramp
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * self.controller.current_ramp);
            }
            
            // Try to solve at this ramp level
            let (converged, iterations, max_change) = self.solve_at_ramp(&mut total_iterations);
            
            // Get diode voltage for monitoring
            let diode_v = self.get_diode_voltage();
            
            if self.controller.iterations <= 10 {
                println!("  [Ramp {:.3}: {} in {} iters, Vd={:.3}V]", 
                         self.controller.current_ramp,
                         if converged { "converged" } else { "failed" },
                         iterations,
                         diode_v);
            }
            
            // Update controller
            let (next_ramp, done) = self.controller.update(converged, iterations, max_change);
            
            if done {
                // Use best ramp for final solve
                for &(idx, v) in &vsources {
                    self.elements[idx].set_voltage(v * self.controller.best_ramp);
                }
                
                // Final solve with higher damping
                self.solve_final(&mut total_iterations);
                break;
            }
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_at_ramp(&mut self, total_iterations: &mut usize) -> (bool, usize, f64) {
        let max_iter = 30;
        let tol = 1e-10;
        let damping = 0.5;  // Underdamped
        let mut iterations = 0;
        let mut last_max_change = 0.0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
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
                
                last_max_change = max_change;
                
                if max_change < tol {
                    return (true, iterations, max_change);
                }
            } else {
                return (false, iterations, f64::INFINITY);
            }
        }
        
        (false, iterations, last_max_change)
    }
    
    fn solve_final(&mut self, total_iterations: &mut usize) {
        let max_iter = 100;
        let tol = 1e-12;
        let damping = 0.8;  // Higher damping for final convergence
        
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
    println!("=== ADAPTIVE RAMP LOGARITHMIC GRADIENT SOLVER ===");
    println!("Searches for optimal source ramping using convergence quality\n");
    
    let test_cases = [
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", 1.0, 100.0, 1e-12, 0.050),
        ("Low current", 0.1, 1000.0, 1e-12, 0.026),
        ("High voltage", 5.0, 100.0, 1e-12, 0.026),
        ("Low resistance", 1.0, 10.0, 1e-12, 0.026),
        ("Extreme low current", 0.05, 2000.0, 1e-12, 0.026),
        ("High current", 10.0, 50.0, 1e-12, 0.026),
    ];
    
    println!("{:>20} | {:>12} | {:>12} | {:>12} | {:>12} | {:>8} | {:>8} | {:>8}", 
             "Test Case", "SPICE Vd", "SPICE Id", "Ramp Vd", "Ramp Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(100));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = AdaptiveRampLogGradientSolver::new(3);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_ramp, id_ramp, iters, time) = solver.solve();
        
        let v_err = ((vd_ramp - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_ramp - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 test_name, 
                 vd_ref, id_ref * 1000.0,
                 vd_ramp, id_ramp * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(100));
    
    let n_cases = test_cases.len() as f64;
    println!("\nAdaptive Ramp Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\n=== KEY APPROACH ===");
    println!("1. Start with low ramp factor (10%)");
    println!("2. Gradually increase, monitoring convergence quality");
    println!("3. Quality = iteration count (if converged) or high penalty");
    println!("4. Adaptively adjust step size based on quality trend");
    println!("5. Use best ramp found for final accurate solve");
}