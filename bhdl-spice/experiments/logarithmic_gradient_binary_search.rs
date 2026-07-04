/// Binary Search Logarithmic Gradient Solver
/// 
/// Uses oscillations as binary search markers:
/// - Start very underdamped for fast initial response
/// - On direction change (overshoot), go backward with halved step
/// - Continue binary search until convergence
/// 
/// Key insight: Use second derivative sign to determine search direction

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

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

// Binary search state tracker
struct BinarySearchTracker {
    voltage_history: VecDeque<f64>,
    ramp_history: VecDeque<f64>,
    gradient_history: VecDeque<f64>,
    
    // Binary search state
    last_gradient_sign: Option<bool>,
    direction_changes: usize,
    current_step: f64,
    
    // Bounds for binary search
    lower_bound: f64,
    upper_bound: f64,
    best_ramp: f64,
    best_voltage: f64,
}

impl BinarySearchTracker {
    fn new(initial_step: f64) -> Self {
        Self {
            voltage_history: VecDeque::with_capacity(5),
            ramp_history: VecDeque::with_capacity(5),
            gradient_history: VecDeque::with_capacity(4),
            
            last_gradient_sign: None,
            direction_changes: 0,
            current_step: initial_step,
            
            lower_bound: 0.0,
            upper_bound: 1.0,
            best_ramp: 0.0,
            best_voltage: 0.0,
        }
    }
    
    fn update(&mut self, voltage: f64, ramp_factor: f64) -> SearchSignal {
        // Store history
        self.voltage_history.push_back(voltage);
        self.ramp_history.push_back(ramp_factor);
        if self.voltage_history.len() > 5 {
            self.voltage_history.pop_front();
            self.ramp_history.pop_front();
        }
        
        // Need at least 2 points for gradient
        if self.voltage_history.len() < 2 {
            return SearchSignal::Continue { step: self.current_step };
        }
        
        // Calculate gradient (rate of change of voltage w.r.t ramp)
        let n = self.voltage_history.len();
        let dv = self.voltage_history[n-1] - self.voltage_history[n-2];
        let dr = self.ramp_history[n-1] - self.ramp_history[n-2];
        
        if dr.abs() < 1e-15 {
            return SearchSignal::Continue { step: self.current_step };
        }
        
        let gradient = dv / dr;
        
        self.gradient_history.push_back(gradient);
        if self.gradient_history.len() > 4 {
            self.gradient_history.pop_front();
        }
        
        // Detect direction change
        let current_sign = gradient > 0.0;
        
        if let Some(prev_sign) = self.last_gradient_sign {
            if current_sign != prev_sign && gradient.abs() > 1e-10 {
                // Direction changed - we overshot!
                self.direction_changes += 1;
                
                // Binary search logic
                if current_sign {
                    // Gradient positive now, was negative - we went too far
                    self.upper_bound = ramp_factor;
                } else {
                    // Gradient negative now, was positive - we didn't go far enough
                    self.lower_bound = ramp_factor;
                }
                
                // Halve the step size (or use logarithmic reduction)
                let reduction_factor = if self.direction_changes < 3 {
                    0.5  // Aggressive halving initially
                } else {
                    0.618  // Golden ratio for finer control
                };
                
                self.current_step *= reduction_factor;
                
                // Calculate second derivative to determine direction
                if self.gradient_history.len() >= 2 {
                    let m = self.gradient_history.len();
                    let second_deriv = self.gradient_history[m-1] - self.gradient_history[m-2];
                    
                    self.last_gradient_sign = Some(current_sign);
                    
                    // Determine search direction based on second derivative
                    if second_deriv > 0.0 {
                        // Curvature upward - we're on the right side of the curve
                        return SearchSignal::Reverse { 
                            step: self.current_step,
                            target: (self.lower_bound + ramp_factor) / 2.0
                        };
                    } else {
                        // Curvature downward - continue forward but smaller
                        return SearchSignal::Continue { 
                            step: self.current_step 
                        };
                    }
                }
            }
        }
        
        self.last_gradient_sign = Some(current_sign);
        
        // Check if we're converging
        if self.current_step < 1e-6 || self.direction_changes > 20 {
            return SearchSignal::Converged;
        }
        
        SearchSignal::Continue { step: self.current_step }
    }
    
    fn reset_for_new_search(&mut self) {
        self.voltage_history.clear();
        self.ramp_history.clear();
        self.gradient_history.clear();
        self.last_gradient_sign = None;
        self.direction_changes = 0;
        // Keep current_step as it might be good for next iteration
    }
}

#[derive(Debug, Clone)]
enum SearchSignal {
    Continue { step: f64 },
    Reverse { step: f64, target: f64 },
    Converged,
}

// Binary search controller
struct BinarySearchController {
    damping: f64,
    tracker: BinarySearchTracker,
    
    // State
    current_ramp: f64,
    search_direction: f64,  // +1 or -1
    min_step: f64,
}

impl BinarySearchController {
    fn new() -> Self {
        Self {
            damping: 0.3,  // Very underdamped for fast response
            tracker: BinarySearchTracker::new(0.05),  // Aggressive initial step
            
            current_ramp: 0.0,
            search_direction: 1.0,
            min_step: 1e-6,
        }
    }
    
    fn get_next_ramp(&mut self, voltage: f64) -> (f64, bool) {
        let signal = self.tracker.update(voltage, self.current_ramp);
        
        match signal {
            SearchSignal::Continue { step } => {
                self.current_ramp += step * self.search_direction;
                self.current_ramp = self.current_ramp.clamp(0.0, 1.0);
                (self.current_ramp, false)
            }
            
            SearchSignal::Reverse { step, target } => {
                // Reverse direction
                self.search_direction *= -1.0;
                
                // Jump to binary search midpoint
                self.current_ramp = target.clamp(0.0, 1.0);
                
                println!("  [Binary search: reversing at {:.6}, step={:.6}]", 
                         self.current_ramp, step);
                
                (self.current_ramp, false)
            }
            
            SearchSignal::Converged => {
                (self.current_ramp, true)
            }
        }
    }
    
    fn prepare_for_final_approach(&mut self) {
        // For final approach, use moderate damping
        self.damping = 0.7;
        self.tracker.current_step = 0.001;
    }
}

// Main solver
pub struct BinarySearchLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: BinarySearchController,
}

impl BinarySearchLogGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: BinarySearchController::new(),
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
        
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Binary search approach
        loop {
            let diode_v = self.get_diode_voltage();
            let (next_ramp, converged) = self.controller.get_next_ramp(diode_v);
            
            if converged || next_ramp >= 0.999 {
                break;
            }
            
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * next_ramp);
            }
            
            // Solve at this ramp level
            let (conv, _) = self.solve_to_convergence(&mut total_iterations);
            
            if !conv {
                // Failed to converge - reduce step and try again
                self.controller.tracker.current_step *= 0.5;
            }
        }
        
        // Final solve at 100%
        self.controller.prepare_for_final_approach();
        
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Multiple iterations at 100% for accuracy
        for _ in 0..3 {
            self.solve_to_convergence(&mut total_iterations);
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 30;
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
                
                // Use controller's damping
                let damping = self.controller.damping;
                
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
    println!("=== BINARY SEARCH LOGARITHMIC GRADIENT SOLVER ===");
    println!("Uses oscillations as binary search markers\n");
    
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
        
        let mut solver = BinarySearchLogGradientSolver::new(3);
        
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
    println!("\nBinary Search Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\n=== KEY FEATURES ===");
    println!("1. Very underdamped start for fast oscillations");
    println!("2. Binary search using direction changes");
    println!("3. Backward/forward based on second derivative");
    println!("4. Progressive step reduction (halving/golden ratio)");
    println!("5. Convergence detection based on step size");
}