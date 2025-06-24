/// Balanced Optimization of Logarithmic Gradient Circuit Solver
/// 
/// This version seeks to improve performance while maintaining accuracy
/// Target: <1% error, <20ms runtime, <2000 iterations

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

// OPTIMIZATION 1: Streamlined history with critical data only
struct BalancedHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    convergence_history: VecDeque<bool>,
    
    // Cached computations
    last_sensitivity: Option<f64>,
    median_sensitivity: f64,
    convergence_rate: f64,
}

impl BalancedHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(8),
            log_currents: VecDeque::with_capacity(8),
            convergence_history: VecDeque::with_capacity(8),
            last_sensitivity: None,
            median_sensitivity: 0.0,
            convergence_rate: 1.0,
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, converged: bool) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.convergence_history.push_back(converged);
        
        // Maintain window size
        if self.voltages.len() > 8 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
            self.convergence_history.pop_front();
        }
        
        // Update cached values
        self.update_cached_values();
    }
    
    fn update_cached_values(&mut self) {
        // Update convergence rate
        if !self.convergence_history.is_empty() {
            let successes = self.convergence_history.iter().filter(|&&x| x).count();
            self.convergence_rate = successes as f64 / self.convergence_history.len() as f64;
        }
        
        // Update median sensitivity
        if self.voltages.len() >= 3 {
            let mut sensitivities = Vec::new();
            for i in 1..self.voltages.len() {
                let dv = self.voltages[i] - self.voltages[i-1];
                if dv.abs() > 1e-12 {
                    let dlog_i = self.log_currents[i] - self.log_currents[i-1];
                    sensitivities.push(dlog_i / dv);
                }
            }
            
            if !sensitivities.is_empty() {
                sensitivities.sort_by(|a, b| a.partial_cmp(b).unwrap());
                self.median_sensitivity = sensitivities[sensitivities.len() / 2];
                self.last_sensitivity = Some(self.median_sensitivity);
            }
        }
    }
    
    fn get_sensitivity(&self) -> Option<f64> {
        self.last_sensitivity
    }
    
    fn get_stability(&self) -> f64 {
        // Simple stability metric based on convergence rate
        self.convergence_rate
    }
}

// OPTIMIZATION 2: Smarter controller with memory
struct BalancedController {
    vt: f64,
    current_rate: f64,
    min_rate: f64,
    max_rate: f64,
    
    // State tracking
    consecutive_successes: usize,
    consecutive_failures: usize,
    high_sensitivity_count: usize,
    
    // Adaptive thresholds
    high_threshold: f64,
    low_threshold: f64,
}

impl BalancedController {
    fn new(vt: f64) -> Self {
        Self {
            vt,
            current_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.05,
            consecutive_successes: 0,
            consecutive_failures: 0,
            high_sensitivity_count: 0,
            high_threshold: 2.5,
            low_threshold: 0.5,
        }
    }
    
    fn update(&mut self, sensitivity: Option<f64>, stability: f64, voltage: f64, converged: bool) {
        // Track success/failure streaks
        if converged {
            self.consecutive_successes += 1;
            self.consecutive_failures = 0;
        } else {
            self.consecutive_failures += 1;
            self.consecutive_successes = 0;
        }
        
        if let Some(sens) = sensitivity {
            let expected = 1.0 / self.vt;
            let ratio = sens / expected;
            
            // OPTIMIZATION 3: Voltage-aware threshold adjustment
            let voltage_factor = if voltage < 0.1 { 1.5 } 
                              else if voltage < 0.5 { 1.2 }
                              else { 1.0 };
            
            let effective_high = self.high_threshold * voltage_factor;
            let effective_low = self.low_threshold / voltage_factor;
            
            // Track high sensitivity occurrences
            if ratio > effective_high {
                self.high_sensitivity_count += 1;
            } else {
                self.high_sensitivity_count = self.high_sensitivity_count.saturating_sub(1);
            }
            
            // OPTIMIZATION 4: Adaptive rate control with memory
            if ratio > effective_high || self.high_sensitivity_count > 2 {
                // High sensitivity - be cautious
                self.current_rate = (self.current_rate * 0.7).max(self.min_rate);
            } else if ratio < effective_low && self.consecutive_successes >= 3 && stability > 0.8 {
                // Low sensitivity with good history - accelerate
                let boost = 1.0 + 0.3 * stability;
                self.current_rate = (self.current_rate * boost).min(self.max_rate);
            } else if converged && stability > 0.6 {
                // Normal progress
                self.current_rate = (self.current_rate * 1.1).min(self.max_rate);
            }
        }
        
        // Handle failures
        if !converged || self.consecutive_failures > 1 {
            self.current_rate = (self.current_rate * 0.5).max(self.min_rate);
        }
        
        // OPTIMIZATION 5: Adaptive threshold adjustment
        if self.consecutive_successes > 5 {
            self.high_threshold = (self.high_threshold * 1.1).min(3.5);
            self.low_threshold = (self.low_threshold * 0.9).max(0.3);
        } else if self.consecutive_failures > 2 {
            self.high_threshold = (self.high_threshold * 0.9).max(2.0);
            self.low_threshold = (self.low_threshold * 1.1).min(0.7);
        }
    }
}

// OPTIMIZATION 6: Solver with improved convergence
pub struct BalancedLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: BalancedHistory,
    controller: BalancedController,
    
    // Optimization: Track diode element directly
    diode_idx: Option<usize>,
    diode_connection: Option<(usize, usize)>,
}

impl BalancedLogGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: BalancedHistory::new(),
            controller: BalancedController::new(vt),
            diode_idx: None,
            diode_connection: None,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        let idx = self.elements.len();
        
        // Track diode for faster access
        if element.is_nonlinear() {
            self.diode_idx = Some(idx);
        }
        
        self.elements.push(element);
        idx
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
        
        // Track diode connection
        if Some(elem_idx) == self.diode_idx {
            self.diode_connection = Some((pos, neg));
        }
    }
    
    fn get_diode_state(&self) -> (f64, f64) {
        if let (Some(idx), Some((pos, neg))) = (self.diode_idx, self.diode_connection) {
            let voltage = self.node_voltages[pos] - self.node_voltages[neg];
            let current = self.elements[idx].current_at_voltage(voltage);
            let log_current = (current.abs() + 1e-18).ln();
            (voltage, log_current)
        } else {
            (0.0, 0.0)
        }
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
        
        // OPTIMIZATION 7: Better initial guess
        let mut ramp_factor = 0.0;
        let mut last_good_state = self.node_voltages.clone();
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // OPTIMIZATION 8: Use previous solution as initial guess
            if ramp_factor > 0.0 {
                self.node_voltages = last_good_state.clone();
            }
            
            let (converged, iters) = self.solve_to_convergence(&mut total_iterations);
            
            if converged {
                // Save good state
                last_good_state = self.node_voltages.clone();
                
                // Update history
                let (diode_v, log_i) = self.get_diode_state();
                self.history.add_point(diode_v, log_i, true);
                
                // Update controller
                let sensitivity = self.history.get_sensitivity();
                let stability = self.history.get_stability();
                self.controller.update(sensitivity, stability, diode_v, true);
            } else {
                self.history.add_point(0.0, 0.0, false);
                self.controller.update(None, 0.0, 0.0, false);
                
                // Restore last good state
                self.node_voltages = last_good_state.clone();
            }
            
            if !converged {
                continue;
            }
            
            // Advance ramp
            ramp_factor += self.controller.current_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        self.solve_to_convergence(&mut total_iterations);
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 25; // Balanced iteration limit
        let tol = 1e-12; // Keep tight tolerance for accuracy
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // OPTIMIZATION 9: Iteration-dependent damping
                let damping = match iter {
                    0..=2 => 0.5,   // Heavy damping initially
                    3..=5 => 0.7,   // Moderate damping
                    _ => 0.85,      // Light damping when close
                };
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                // Update element states
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
    println!("=== BALANCED LOGARITHMIC GRADIENT SOLVER ===");
    println!("Optimizing for both speed and accuracy\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Balanced Vd", "Balanced Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    let mut success_count = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = BalancedLogGradientSolver::new(3, vt);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_bal, id_bal, iters, time) = solver.solve();
        
        let v_err = ((vd_bal - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_bal - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        if !max_err.is_nan() {
            success_count += 1;
            total_error += max_err;
            total_time += time;
            total_iterations += iters;
        }
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 name, 
                 vd_ref, id_ref * 1000.0,
                 vd_bal, id_bal * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    if success_count > 0 {
        let avg_error = total_error / success_count as f64;
        let avg_time = total_time / success_count as f64;
        let avg_iterations = total_iterations as f64 / success_count as f64;
        
        println!("\nBalanced Optimization Results:");
        println!("  Success rate: {}/{} ({:.1}%)", success_count, test_cases.len(), 
                 success_count as f64 / test_cases.len() as f64 * 100.0);
        println!("  Average error: {:.4}%", avg_error);
        println!("  Average time: {:.1}ms", avg_time);
        println!("  Average iterations: {:.0}", avg_iterations);
        
        println!("\nComparison:");
        println!("  Reference: 0.49% error, 55.5ms time, 8,032 iterations");
        println!("  Balanced:  {:.2}% error, {:.1}ms time, {:.0} iterations", 
                 avg_error, avg_time, avg_iterations);
        
        println!("\nImprovements:");
        if avg_iterations < 8032.0 {
            println!("  ✅ Iteration reduction: {:.1}x", 8032.0 / avg_iterations);
        }
        if avg_time < 55.5 {
            println!("  ✅ Speed improvement: {:.1}x", 55.5 / avg_time);
        }
        if avg_error < 1.0 {
            println!("  ✅ Accuracy maintained: <1% error");
        }
    }
    
    println!("\n=== BALANCED OPTIMIZATION TECHNIQUES ===");
    println!("1. Streamlined history tracking with cached values");
    println!("2. Voltage-aware adaptive thresholds");
    println!("3. Success/failure streak tracking");
    println!("4. Previous solution as initial guess");
    println!("5. Iteration-dependent damping (0.5→0.7→0.85)");
    println!("6. Direct diode state tracking");
    println!("7. Adaptive threshold adjustment based on performance");
    println!("8. Stability-based acceleration");
}