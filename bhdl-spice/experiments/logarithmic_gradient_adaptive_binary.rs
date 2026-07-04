/// Adaptive Binary Search Logarithmic Gradient Solver
/// 
/// Refined implementation of binary search using second derivatives:
/// - Start underdamped for fast initial progress
/// - Track voltage changes to detect overshoots
/// - Use second derivative sign to determine if we should go backward
/// - Progressively refine step size like binary search
/// 
/// Key improvements:
/// - Better overshoot detection
/// - Smarter backward/forward decisions
/// - Adaptive damping based on search phase

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

// Adaptive binary search state
struct AdaptiveBinarySearchState {
    // History tracking
    voltage_history: VecDeque<f64>,
    ramp_history: VecDeque<f64>,
    
    // Derivative tracking
    first_derivatives: VecDeque<f64>,
    second_derivatives: VecDeque<f64>,
    
    // Search state
    current_ramp: f64,
    step_size: f64,
    direction: f64,  // +1 forward, -1 backward
    
    // Overshoot detection
    overshoots: usize,
    last_overshoot_ramp: f64,
    
    // Convergence tracking
    stable_count: usize,
}

impl AdaptiveBinarySearchState {
    fn new() -> Self {
        Self {
            voltage_history: VecDeque::with_capacity(6),
            ramp_history: VecDeque::with_capacity(6),
            first_derivatives: VecDeque::with_capacity(5),
            second_derivatives: VecDeque::with_capacity(4),
            
            current_ramp: 0.0,
            step_size: 0.03,  // Start with moderate step
            direction: 1.0,
            
            overshoots: 0,
            last_overshoot_ramp: 0.0,
            
            stable_count: 0,
        }
    }
    
    fn update(&mut self, voltage: f64) -> RampUpdate {
        // Add to history
        self.voltage_history.push_back(voltage);
        self.ramp_history.push_back(self.current_ramp);
        
        if self.voltage_history.len() > 6 {
            self.voltage_history.pop_front();
            self.ramp_history.pop_front();
        }
        
        // Need at least 2 points for first derivative
        if self.voltage_history.len() < 2 {
            self.current_ramp += self.step_size * self.direction;
            return RampUpdate::Continue(self.current_ramp.clamp(0.0, 1.0));
        }
        
        // Calculate first derivative (dV/dramp)
        let n = self.voltage_history.len();
        let dv = self.voltage_history[n-1] - self.voltage_history[n-2];
        let dr = self.ramp_history[n-1] - self.ramp_history[n-2];
        
        if dr.abs() < 1e-15 {
            return RampUpdate::Continue(self.current_ramp);
        }
        
        let first_deriv = dv / dr;
        self.first_derivatives.push_back(first_deriv);
        
        if self.first_derivatives.len() > 5 {
            self.first_derivatives.pop_front();
        }
        
        // Need at least 2 first derivatives for second derivative
        if self.first_derivatives.len() < 2 {
            self.current_ramp += self.step_size * self.direction;
            return RampUpdate::Continue(self.current_ramp.clamp(0.0, 1.0));
        }
        
        // Calculate second derivative
        let m = self.first_derivatives.len();
        let second_deriv = self.first_derivatives[m-1] - self.first_derivatives[m-2];
        self.second_derivatives.push_back(second_deriv);
        
        if self.second_derivatives.len() > 4 {
            self.second_derivatives.pop_front();
        }
        
        // Detect overshoot: first derivative changes sign
        if m >= 2 {
            let prev_sign = self.first_derivatives[m-2] > 0.0;
            let curr_sign = self.first_derivatives[m-1] > 0.0;
            
            if prev_sign != curr_sign && first_deriv.abs() > 1e-10 {
                // Overshoot detected!
                self.overshoots += 1;
                self.last_overshoot_ramp = self.current_ramp;
                
                // Binary search reduction
                self.step_size *= 0.5;  // Halve the step size
                
                // Use second derivative to decide direction
                if second_deriv > 0.0 {
                    // Positive curvature - we overshot, go back
                    self.direction = -self.direction;
                    println!("  [Overshoot #{} at ramp={:.6}, reversing, step={:.6}]", 
                             self.overshoots, self.current_ramp, self.step_size);
                } else {
                    // Negative curvature - continue but with smaller steps
                    println!("  [Overshoot #{} at ramp={:.6}, continuing smaller, step={:.6}]", 
                             self.overshoots, self.current_ramp, self.step_size);
                }
                
                // Jump to midpoint if we have bounds
                if self.overshoots > 1 && self.last_overshoot_ramp > 0.0 {
                    let midpoint = (self.current_ramp + self.last_overshoot_ramp) / 2.0;
                    self.current_ramp = midpoint;
                    return RampUpdate::Jump(midpoint);
                }
            }
        }
        
        // Check for stability (small changes)
        if first_deriv.abs() < 1e-6 && self.second_derivatives.len() >= 2 {
            let recent_second_derivs: Vec<f64> = self.second_derivatives
                .iter()
                .rev()
                .take(2)
                .copied()
                .collect();
            
            if recent_second_derivs.iter().all(|&d| d.abs() < 1e-8) {
                self.stable_count += 1;
                if self.stable_count > 3 {
                    // We're stable, can increase step size
                    self.step_size = (self.step_size * 1.5).min(0.05);
                }
            } else {
                self.stable_count = 0;
            }
        }
        
        // Check convergence
        if self.step_size < 1e-6 || (self.current_ramp > 0.999 && first_deriv.abs() < 1e-9) {
            return RampUpdate::Converged;
        }
        
        // Normal update
        self.current_ramp += self.step_size * self.direction;
        self.current_ramp = self.current_ramp.clamp(0.0, 1.0);
        
        RampUpdate::Continue(self.current_ramp)
    }
    
    fn get_adaptive_damping(&self) -> f64 {
        // Start very underdamped, increase with overshoots
        match self.overshoots {
            0 => 0.3,   // Very underdamped for fast response
            1 => 0.5,   // Still underdamped
            2 => 0.7,   // Near critical
            _ => 0.85,  // More overdamped for final convergence
        }
    }
}

#[derive(Debug)]
enum RampUpdate {
    Continue(f64),
    Jump(f64),
    Converged,
}

// Main solver
pub struct AdaptiveBinaryLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    search_state: AdaptiveBinarySearchState,
}

impl AdaptiveBinaryLogGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            search_state: AdaptiveBinarySearchState::new(),
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
        
        // Adaptive binary search loop
        loop {
            // Get current diode voltage
            let diode_v = self.get_diode_voltage();
            
            // Update search state and get next ramp
            match self.search_state.update(diode_v) {
                RampUpdate::Continue(ramp) | RampUpdate::Jump(ramp) => {
                    // Update sources
                    for &(idx, v) in &vsources {
                        self.elements[idx].set_voltage(v * ramp);
                    }
                    
                    // Solve with adaptive damping
                    let damping = self.search_state.get_adaptive_damping();
                    let (converged, _) = self.solve_to_convergence(&mut total_iterations, damping);
                    
                    if !converged {
                        // Convergence failed, reduce step
                        self.search_state.step_size *= 0.7;
                    }
                }
                
                RampUpdate::Converged => {
                    break;
                }
            }
            
            // Safety check
            if total_iterations > 50000 {
                println!("  [Warning: Excessive iterations, stopping]");
                break;
            }
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Use higher damping for final convergence
        for _ in 0..3 {
            self.solve_to_convergence(&mut total_iterations, 0.9);
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize, damping: f64) -> (bool, usize) {
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
    println!("=== ADAPTIVE BINARY SEARCH LOGARITHMIC GRADIENT SOLVER ===");
    println!("Refined binary search with smart overshoot handling\n");
    
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
        
        let mut solver = AdaptiveBinaryLogGradientSolver::new(3);
        
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
    println!("\nAdaptive Binary Search Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\n=== KEY IMPROVEMENTS ===");
    println!("1. Better overshoot detection using first derivative sign changes");
    println!("2. Second derivative guides backward/forward decisions");
    println!("3. True binary search with midpoint jumps");
    println!("4. Adaptive damping increases with overshoots");
    println!("5. Safety limits to prevent runaway iterations");
}