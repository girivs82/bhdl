/// True Binary Search Logarithmic Gradient Solver
/// 
/// Implements actual binary search on error signal:
/// - Track error = V - expected
/// - On sign change, go BACK by half step
/// - Build up time/ramp incrementally
/// 
/// Example: target=2V
/// t=0, V=0, e=-2
/// t=1, V=10, e=+8 (sign change!) 
/// t=0.5 (went back!), V=1, e=-1 (sign change!)
/// t=0.75 (0.5+0.25), etc.

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

// True binary search controller
struct TrueBinarySearchController {
    // Current state
    current_ramp: f64,
    current_step: f64,
    
    // Error tracking
    last_error: Option<f64>,
    
    // For estimating target
    voltage_at_full_ramp: Option<f64>,
    
    // Convergence
    iterations_at_current_step: usize,
    total_iterations: usize,
}

impl TrueBinarySearchController {
    fn new() -> Self {
        Self {
            current_ramp: 0.0,
            current_step: 0.5,  // Start with large step
            
            last_error: None,
            voltage_at_full_ramp: None,
            
            iterations_at_current_step: 0,
            total_iterations: 0,
        }
    }
    
    fn update(&mut self, current_voltage: f64, target_voltage: f64) -> (f64, bool) {
        self.total_iterations += 1;
        
        // Calculate error
        let error = current_voltage - target_voltage;
        
        // Check for convergence
        if error.abs() < 1e-6 || self.current_step < 1e-8 || self.current_ramp >= 0.9999 {
            return (self.current_ramp, true);
        }
        
        // First iteration - just move forward
        if self.last_error.is_none() {
            self.last_error = Some(error);
            self.current_ramp = self.current_step;
            return (self.current_ramp, false);
        }
        
        let last_err = self.last_error.unwrap();
        
        // Check for sign change
        if last_err.signum() != error.signum() && error.abs() > 1e-10 {
            // Sign changed! Binary search logic
            self.current_step *= 0.5;  // Halve the step
            
            if error > 0.0 {
                // We overshot (V > target), go back
                self.current_ramp -= self.current_step;
            } else {
                // We undershot (V < target), go forward
                self.current_ramp += self.current_step;
            }
            
            // Debug output
            if self.total_iterations < 10 || self.total_iterations % 100 == 0 {
                println!("  [Iter {}: error {:.6} → {:.6}, ramp={:.6}, step={:.6}]", 
                         self.total_iterations, last_err, error, self.current_ramp, self.current_step);
            }
            
            self.iterations_at_current_step = 0;
        } else {
            // No sign change, continue in same direction
            if error < 0.0 {
                // Still below target, increase ramp
                self.current_ramp += self.current_step;
            } else {
                // Still above target, decrease ramp
                self.current_ramp -= self.current_step;
            }
            
            self.iterations_at_current_step += 1;
            
            // If we've been at this step size for too long without sign change,
            // we might be too far from target - increase step
            if self.iterations_at_current_step > 5 {
                self.current_step *= 1.5;
                self.iterations_at_current_step = 0;
            }
        }
        
        // Clamp ramp to valid range
        self.current_ramp = self.current_ramp.clamp(0.0, 1.0);
        
        // Update last error
        self.last_error = Some(error);
        
        (self.current_ramp, false)
    }
    
    fn estimate_target(&mut self, source_voltage: f64, diode_voltage_at_zero: f64) -> f64 {
        // Estimate the target diode voltage
        // For a diode circuit, it's typically around 0.6-0.7V
        // But we'll use the initial solve to estimate better
        
        if self.voltage_at_full_ramp.is_none() && self.current_ramp > 0.9 {
            self.voltage_at_full_ramp = Some(diode_voltage_at_zero);
        }
        
        // Simple estimate: diode drop is roughly 0.6V for silicon
        0.6
    }
}

// Main solver
pub struct TrueBinaryLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: TrueBinarySearchController,
    damping: f64,
}

impl TrueBinaryLogGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: TrueBinarySearchController::new(),
            damping: 0.5,  // Moderate damping
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
        
        // Find voltage sources
        let mut vsources = Vec::new();
        let mut source_voltage = 0.0;
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                source_voltage = elem.get_voltage();
                vsources.push((i, source_voltage));
            }
        }
        
        // First, solve at full voltage to get target
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        self.solve_to_convergence(&mut total_iterations);
        let target_voltage = self.get_diode_voltage();
        
        // Reset for binary search
        self.node_voltages = vec![0.0; self.num_nodes];
        
        // Binary search to find correct ramp factor
        loop {
            // Update sources with current ramp
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * self.controller.current_ramp);
            }
            
            // Solve at this ramp level
            let (converged, _) = self.solve_to_convergence(&mut total_iterations);
            
            if !converged {
                // If convergence fails, adjust damping
                self.damping = (self.damping * 1.1).min(0.9);
            }
            
            // Get current diode voltage
            let current_voltage = self.get_diode_voltage();
            
            // Update binary search
            let (new_ramp, done) = self.controller.update(current_voltage, target_voltage);
            
            if done {
                break;
            }
            
            // Safety check
            if total_iterations > 10000 {
                println!("  [Warning: Excessive iterations, stopping binary search]");
                break;
            }
        }
        
        // Final solve at target ramp
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v * self.controller.current_ramp);
        }
        
        // Higher damping for final convergence
        self.damping = 0.8;
        self.solve_to_convergence(&mut total_iterations);
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 50;
        let tol = 1e-12;
        let mut iterations = 0;
        
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
                    self.node_voltages[i+1] = old_v[i+1] + self.damping * delta;
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
                    return (true, iterations);
                }
            } else {
                return (false, iterations);
            }
        }
        
        (false, iterations)
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
    println!("=== TRUE BINARY SEARCH LOGARITHMIC GRADIENT SOLVER ===");
    println!("Binary search on error signal with proper backtracking\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Binary Vd", "Binary Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(100));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = TrueBinaryLogGradientSolver::new(3);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_binary, id_binary, iters, time) = solver.solve();
        
        let v_err = ((vd_binary - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_binary - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 test_name, 
                 vd_ref, id_ref * 1000.0,
                 vd_binary, id_binary * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(100));
    
    let n_cases = test_cases.len() as f64;
    println!("\nTrue Binary Search Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\n=== ALGORITHM EXPLANATION ===");
    println!("1. Track error = V - target (not derivatives!)");
    println!("2. On sign change: halve step and go BACK");
    println!("3. Build ramp incrementally: ramp = ramp ± step/2");
    println!("4. Continue until error < tolerance");
    println!("\nExample: target=2V");
    println!("  t=0.0, V=0, e=-2");
    println!("  t=0.5, V=10, e=+8 (sign change → go back!)");
    println!("  t=0.25, V=1, e=-1 (sign change → go forward)");
    println!("  t=0.375, etc.");
}