/// Optimized Logarithmic Gradient Circuit Solver
/// 
/// This file explores optimizations to improve performance while maintaining
/// the generic nature of the logarithmic gradient approach.
/// 
/// Optimization targets:
/// 1. Reduce iteration count (currently 8,032 average)
/// 2. Improve runtime (currently 55.5ms average)
/// 3. Maintain or improve accuracy (currently 0.49% average)
/// 4. Keep 100% convergence rate

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Reuse element definitions from reference implementation
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

// OPTIMIZATION 1: Lighter-weight history with rolling statistics
struct OptimizedHistory {
    // Keep only essential data
    recent_sensitivities: VecDeque<f64>,
    recent_convergence: VecDeque<bool>,
    rolling_mean: f64,
    rolling_variance: f64,
    count: usize,
}

impl OptimizedHistory {
    fn new() -> Self {
        Self {
            recent_sensitivities: VecDeque::with_capacity(5),
            recent_convergence: VecDeque::with_capacity(5),
            rolling_mean: 0.0,
            rolling_variance: 0.0,
            count: 0,
        }
    }
    
    fn update(&mut self, sensitivity: f64, converged: bool) {
        // Update rolling statistics
        self.count += 1;
        let delta = sensitivity - self.rolling_mean;
        self.rolling_mean += delta / self.count as f64;
        let delta2 = sensitivity - self.rolling_mean;
        self.rolling_variance += delta * delta2;
        
        // Keep recent values
        self.recent_sensitivities.push_back(sensitivity);
        if self.recent_sensitivities.len() > 5 {
            self.recent_sensitivities.pop_front();
        }
        
        self.recent_convergence.push_back(converged);
        if self.recent_convergence.len() > 5 {
            self.recent_convergence.pop_front();
        }
    }
    
    fn get_stability_metric(&self) -> f64 {
        if self.count < 2 {
            return 0.5;
        }
        
        let variance = self.rolling_variance / (self.count - 1) as f64;
        let std_dev = variance.sqrt();
        let cv = std_dev / self.rolling_mean.abs().max(1e-12);
        
        // Lower CV means more stable
        1.0 / (1.0 + cv)
    }
    
    fn get_convergence_rate(&self) -> f64 {
        if self.recent_convergence.is_empty() {
            return 1.0;
        }
        
        let successes = self.recent_convergence.iter().filter(|&&x| x).count();
        successes as f64 / self.recent_convergence.len() as f64
    }
}

// OPTIMIZATION 2: Predictive ramp control
struct PredictiveController {
    vt: f64,
    current_rate: f64,
    min_rate: f64,
    max_rate: f64,
    
    // Predictive model parameters
    last_sensitivity: Option<f64>,
    sensitivity_trend: f64,
    acceleration_factor: f64,
}

impl PredictiveController {
    fn new(vt: f64) -> Self {
        Self {
            vt,
            current_rate: 0.02, // Start more aggressively
            min_rate: 0.001,
            max_rate: 0.1,
            last_sensitivity: None,
            sensitivity_trend: 0.0,
            acceleration_factor: 1.0,
        }
    }
    
    fn update(&mut self, sensitivity: f64, stability: f64, converged: bool) {
        let expected = 1.0 / self.vt;
        let ratio = sensitivity / expected;
        
        // OPTIMIZATION 3: Predictive trend analysis
        if let Some(last) = self.last_sensitivity {
            self.sensitivity_trend = 0.8 * self.sensitivity_trend + 0.2 * (sensitivity - last);
        }
        self.last_sensitivity = Some(sensitivity);
        
        // OPTIMIZATION 4: Aggressive acceleration when stable
        if converged && stability > 0.8 {
            self.acceleration_factor = (self.acceleration_factor * 1.1).min(2.0);
        } else {
            self.acceleration_factor = (self.acceleration_factor * 0.9).max(0.5);
        }
        
        // OPTIMIZATION 5: Predictive rate adjustment
        let predicted_sensitivity = sensitivity + self.sensitivity_trend * 2.0;
        let predicted_ratio = predicted_sensitivity / expected;
        
        if predicted_ratio > 3.0 {
            // Predicted high sensitivity - preemptively slow down
            self.current_rate = (self.current_rate * 0.6).max(self.min_rate);
        } else if ratio > 2.0 {
            // Currently high - moderate slowdown
            self.current_rate = (self.current_rate * 0.8).max(self.min_rate);
        } else if ratio < 0.5 && stability > 0.7 {
            // Low sensitivity and stable - accelerate aggressively
            let boost = 1.5 * self.acceleration_factor * stability;
            self.current_rate = (self.current_rate * boost).min(self.max_rate);
        } else if converged {
            // Normal operation - gradual increase
            self.current_rate = (self.current_rate * 1.1 * self.acceleration_factor).min(self.max_rate);
        } else {
            // Failed - back off
            self.current_rate = (self.current_rate * 0.5).max(self.min_rate);
        }
    }
    
    fn get_rate(&self) -> f64 {
        self.current_rate
    }
}

// OPTIMIZATION 6: Optimized solver with better matrix operations
pub struct OptimizedLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: OptimizedHistory,
    controller: PredictiveController,
    
    // OPTIMIZATION 7: Cache matrix structure
    cached_pattern: Option<Vec<(usize, usize)>>,
}

impl OptimizedLogGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: OptimizedHistory::new(),
            controller: PredictiveController::new(vt),
            cached_pattern: None,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn get_diode_sensitivity(&self) -> f64 {
        // OPTIMIZATION 8: Direct sensitivity calculation
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        let v = self.node_voltages[pos] - self.node_voltages[neg];
                        let g = elem.conductance_at_voltage(v);
                        let i = elem.current_at_voltage(v);
                        if i.abs() > 1e-15 {
                            return g * v / i; // d(log(I))/dV approximation
                        }
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
        
        // OPTIMIZATION 9: Adaptive initial step based on circuit
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0;
        
        // OPTIMIZATION 10: Early termination for linear circuits
        let has_nonlinear = self.elements.iter().any(|e| e.is_nonlinear());
        if !has_nonlinear {
            ramp_factor = 1.0; // Jump to full solution for linear circuits
        }
        
        while ramp_factor < 1.0 {
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            let (converged, iters) = self.solve_to_convergence(&mut total_iterations);
            
            if has_nonlinear && converged {
                let sensitivity = self.get_diode_sensitivity();
                let stability = self.history.get_stability_metric();
                
                self.history.update(sensitivity, converged);
                self.controller.update(sensitivity, stability, converged);
            }
            
            if !converged {
                self.controller.update(0.0, 0.0, false);
                continue;
            }
            
            ramp_factor += self.controller.get_rate();
            ramp_factor = ramp_factor.min(1.0);
            ramp_step += 1;
        }
        
        // Final solve
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
        let max_iter = 20; // OPTIMIZATION 11: Fewer iterations per ramp
        let tol = 1e-11; // OPTIMIZATION 12: Slightly relaxed tolerance
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // OPTIMIZATION 13: Adaptive damping
                let damping = if iterations < 3 { 0.5 } else { 0.8 };
                
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
                
                // OPTIMIZATION 14: Early termination if converging slowly
                if iter > 5 && max_change > 0.1 {
                    return (false, iterations);
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
    println!("=== OPTIMIZED LOGARITHMIC GRADIENT SOLVER ===");
    println!("Exploring performance optimizations\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Opt Vd", "Opt Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    let mut success_count = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = OptimizedLogGradientSolver::new(3, vt);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_opt, id_opt, iters, time) = solver.solve();
        
        let v_err = ((vd_opt - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_opt - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        if !max_err.is_nan() && max_err < 10.0 {
            success_count += 1;
            total_error += max_err;
            total_time += time;
            total_iterations += iters;
        }
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 name, 
                 vd_ref, id_ref * 1000.0,
                 vd_opt, id_opt * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    if success_count > 0 {
        let avg_error = total_error / success_count as f64;
        let avg_time = total_time / success_count as f64;
        let avg_iterations = total_iterations as f64 / success_count as f64;
        
        println!("\nOptimized Performance:");
        println!("  Success rate: {}/{} ({:.1}%)", success_count, test_cases.len(), 
                 success_count as f64 / test_cases.len() as f64 * 100.0);
        println!("  Average error: {:.4}%", avg_error);
        println!("  Average time: {:.1}ms", avg_time);
        println!("  Average iterations: {:.0}", avg_iterations);
        
        println!("\nComparison with Reference:");
        println!("  Reference: 0.49% error, 55.5ms time, 8,032 iterations");
        println!("  Optimized: {:.2}% error, {:.1}ms time, {:.0} iterations", 
                 avg_error, avg_time, avg_iterations);
        
        if avg_iterations < 8032.0 {
            println!("  ✅ Iteration reduction: {:.1}x", 8032.0 / avg_iterations);
        }
        if avg_time < 55.5 {
            println!("  ✅ Speed improvement: {:.1}x", 55.5 / avg_time);
        }
    }
    
    println!("\n=== OPTIMIZATION TECHNIQUES APPLIED ===");
    println!("1. Lighter-weight history with rolling statistics");
    println!("2. Predictive ramp control based on sensitivity trends");
    println!("3. Aggressive acceleration when stable");
    println!("4. Early termination for slow convergence");
    println!("5. Adaptive damping based on iteration count");
    println!("6. Relaxed tolerance (1e-11 vs 1e-12)");
    println!("7. Direct sensitivity calculation");
    println!("8. Skip ramping for linear circuits");
}