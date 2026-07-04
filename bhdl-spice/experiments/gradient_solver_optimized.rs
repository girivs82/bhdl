/// Optimized High Accuracy Gradient Solver
/// 
/// This achieves high accuracy more efficiently by:
/// 1. Using quadratic convergence detection
/// 2. Smart final approach strategy
/// 3. Optimized convergence criteria

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

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

#[derive(Clone)]
struct OptimizedHistory {
    voltages: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
    convergence_rates: VecDeque<f64>,
}

impl OptimizedHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(5),
            ramp_factors: VecDeque::with_capacity(5),
            convergence_rates: VecDeque::with_capacity(5),
        }
    }
    
    fn add_point(&mut self, voltage: f64, ramp: f64, conv_rate: f64) {
        self.voltages.push_back(voltage);
        self.ramp_factors.push_back(ramp);
        self.convergence_rates.push_back(conv_rate);
        
        if self.voltages.len() > 5 {
            self.voltages.pop_front();
            self.ramp_factors.pop_front();
            self.convergence_rates.pop_front();
        }
    }
    
    fn estimate_final_value(&self) -> Option<f64> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        let v2 = self.voltages[n-1];
        let v1 = self.voltages[n-2];
        let v0 = self.voltages[n-3];
        
        let r2 = self.ramp_factors[n-1];
        let r1 = self.ramp_factors[n-2];
        let r0 = self.ramp_factors[n-3];
        
        // Check if we're close to 1.0
        if r2 > 0.95 {
            // Use quadratic extrapolation
            let dr1 = r1 - r0;
            let dr2 = r2 - r1;
            let dv1 = v1 - v0;
            let dv2 = v2 - v1;
            
            if dr1 > 0.0 && dr2 > 0.0 {
                let ddv = (dv2/dr2 - dv1/dr1) / ((r2 + r1)/2.0 - (r1 + r0)/2.0);
                let dv_at_1 = dv2/dr2 + ddv * (1.0 - r2);
                Some(v2 + dv_at_1 * (1.0 - r2))
            } else {
                None
            }
        } else {
            None
        }
    }
    
    fn detect_quadratic_convergence(&self) -> bool {
        if self.convergence_rates.len() < 3 {
            return false;
        }
        
        let n = self.convergence_rates.len();
        let rate2 = self.convergence_rates[n-1];
        let rate1 = self.convergence_rates[n-2];
        let rate0 = self.convergence_rates[n-3];
        
        // Check if convergence is accelerating (quadratic)
        rate2 < rate1 * 0.5 && rate1 < rate0 * 0.5
    }
}

struct OptimizedController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    final_approach: bool,
}

impl OptimizedController {
    fn new() -> Self {
        Self {
            current_ramp_rate: 0.02,
            min_rate: 0.00001,
            max_rate: 0.1,
            final_approach: false,
        }
    }
    
    fn update(&mut self, history: &OptimizedHistory, ramp_factor: f64) {
        // Check if we should enter final approach
        if !self.final_approach {
            if let Some(v_final) = history.estimate_final_value() {
                if ramp_factor > 0.95 {
                    self.final_approach = true;
                    self.current_ramp_rate = 0.0001;
                    println!("  Estimated final voltage: {:.9}V", v_final);
                    println!("  Entering final approach mode");
                    return;
                }
            }
        }
        
        // Adaptive control based on convergence pattern
        if history.detect_quadratic_convergence() {
            // We have quadratic convergence, can be more aggressive
            self.current_ramp_rate = (self.current_ramp_rate * 1.5).min(self.max_rate);
        } else if history.convergence_rates.len() > 0 {
            let latest_rate = history.convergence_rates.back().unwrap();
            if *latest_rate > 0.1 {
                // Slow convergence, reduce ramp rate
                self.current_ramp_rate = (self.current_ramp_rate * 0.7).max(self.min_rate);
            }
        }
        
        // Fine control in final approach
        if self.final_approach && ramp_factor > 0.999 {
            self.current_ramp_rate = self.min_rate;
        }
    }
}

pub struct OptimizedGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: OptimizedController,
}

impl OptimizedGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: OptimizedController::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn optimized_dc_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        println!("\nOptimized High Accuracy DC Analysis");
        
        // Count voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        let mut history = OptimizedHistory::new();
        
        // Get voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Smart ramping
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve with adaptive tolerance
            let tol = if self.controller.final_approach { 1e-14 } else { 1e-12 };
            let (converged, newton_iters, conv_rate) = self.solve_adaptive(&mut total_iterations, tol);
            
            if !converged {
                println!("  Warning: Failed at ramp {:.6}", ramp_factor);
                self.controller.current_ramp_rate *= 0.5;
                continue;
            }
            
            // Record history
            history.add_point(self.node_voltages[2], ramp_factor, conv_rate);
            
            // Update controller
            self.controller.update(&history, ramp_factor);
            
            // Advance ramp
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            
            ramp_step += 1;
            
            if ramp_step % 5 == 0 || self.controller.final_approach {
                println!("  Ramp: {:.6}, V2: {:.9}V, Iters: {}, Conv: {:.2e}", 
                         ramp_factor, self.node_voltages[2], newton_iters, conv_rate);
            }
        }
        
        // Final solve at exactly 100%
        println!("\nFinal solve at 100%...");
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        let (converged, final_iters, _) = self.solve_adaptive(&mut total_iterations, 1e-15);
        
        if converged {
            println!("  Converged in {} iterations", final_iters);
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("  Total ramp steps: {}", ramp_step);
        println!("  Total iterations: {}", total_iterations);
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_adaptive(&mut self, total_iterations: &mut usize, tol: f64) -> (bool, usize, f64) {
        let max_iter = 50;
        let mut iterations = 0;
        let mut last_change = 1.0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Adaptive damping
                let damping = if iter < 2 { 0.7 } 
                             else if last_change > 0.1 { 0.9 } 
                             else { 1.0 };
                
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
                
                // Convergence rate
                let conv_rate = if last_change > 0.0 { max_change / last_change } else { 1.0 };
                last_change = max_change;
                
                if max_change < tol {
                    return (true, iterations, conv_rate);
                }
            } else {
                return (false, iterations, 1.0);
            }
        }
        
        (false, iterations, 1.0)
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        // GMIN for stability
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

fn main() {
    println!("=== OPTIMIZED GRADIENT SOLVER ===");
    
    // SPICE reference
    let is = 1e-12;
    let vt = 0.026;
    let mut vd_spice = 0.7f64;
    
    for _ in 0..200 {
        let id = is * ((vd_spice / vt).exp() - 1.0);
        let f = vd_spice + id * 100.0 - 1.0;
        let g = 1.0 + (is / vt) * (vd_spice / vt).exp() * 100.0;
        let delta = f / g;
        vd_spice -= delta;
        if delta.abs() < 1e-16 {
            break;
        }
    }
    
    let id_spice = (1.0 - vd_spice) / 100.0;
    
    println!("\nSPICE Reference:");
    println!("  Vd = {:.12} V", vd_spice);
    println!("  Id = {:.12} mA", id_spice * 1000.0);
    
    // Test optimized solver
    let mut solver = OptimizedGradientSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(1.0)));
    let r = solver.add_element(Box::new(Resistor::new(100.0)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    let (vd, id, _iterations, time) = solver.optimized_dc_analysis();
    
    let v_err = ((vd - vd_spice) / vd_spice * 100.0).abs();
    let i_err = ((id - id_spice) / id_spice * 100.0).abs();
    
    println!("\nOptimized Gradient Solver Results:");
    println!("  Vd = {:.12} V (error: {:.6}%)", vd, v_err);
    println!("  Id = {:.12} mA (error: {:.6}%)", id * 1000.0, i_err);
    println!("  Time: {:.1} ms", time);
    
    println!("\n=== ANALYSIS ===");
    if v_err < 0.0001 && i_err < 0.0001 {
        println!("✓ EXCELLENT: Achieved <0.0001% accuracy!");
        println!("\nKey optimizations:");
        println!("1. Quadratic convergence detection");
        println!("2. Smart final value estimation");
        println!("3. Adaptive tolerance based on phase");
        println!("4. Convergence rate tracking");
    } else if v_err < 0.001 && i_err < 0.001 {
        println!("✓ VERY GOOD: Achieved <0.001% accuracy!");
    } else if v_err < 0.01 && i_err < 0.01 {
        println!("✓ GOOD: Achieved <0.01% accuracy!");
    } else if v_err < 0.1 && i_err < 0.1 {
        println!("✓ SUCCESS: Achieved <0.1% accuracy!");
    } else {
        println!("○ Accuracy: {:.3}%", v_err.max(i_err));
    }
}