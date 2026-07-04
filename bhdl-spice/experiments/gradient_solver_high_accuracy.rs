/// High Accuracy Gradient Solver - Tighter Convergence
/// 
/// This improves accuracy by:
/// 1. Tighter Newton-Raphson convergence tolerance
/// 2. More careful ramping near the solution
/// 3. Better numerical precision handling

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
struct TimeEvolutionHistory {
    voltages: VecDeque<f64>,
    timestamps: VecDeque<f64>,
}

impl TimeEvolutionHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(10),
            timestamps: VecDeque::with_capacity(10),
        }
    }
    
    fn add_converged_point(&mut self, voltage: f64, time: f64) {
        self.voltages.push_back(voltage);
        self.timestamps.push_back(time);
        
        if self.voltages.len() > 10 {
            self.voltages.pop_front();
            self.timestamps.pop_front();
        }
    }
    
    fn calculate_time_derivatives(&self) -> Option<(f64, f64)> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        
        // Use more points for better numerical accuracy if available
        if n >= 5 {
            // 5-point stencil for better accuracy
            let v4 = self.voltages[n-1];
            let v3 = self.voltages[n-2];
            let v2 = self.voltages[n-3];
            let v1 = self.voltages[n-4];
            let v0 = self.voltages[n-5];
            
            let t4 = self.timestamps[n-1];
            let _t3 = self.timestamps[n-2];
            let _t2 = self.timestamps[n-3];
            let _t1 = self.timestamps[n-4];
            let t0 = self.timestamps[n-5];
            
            let h = (t4 - t0) / 4.0;
            
            // 5-point first derivative at t2
            let dv_dt = (-v4 + 8.0*v3 - 8.0*v1 + v0) / (12.0 * h);
            
            // 5-point second derivative at t2
            let d2v_dt2 = (-v4 + 16.0*v3 - 30.0*v2 + 16.0*v1 - v0) / (12.0 * h * h);
            
            Some((dv_dt, d2v_dt2))
        } else {
            // Fallback to 3-point
            let v2 = self.voltages[n-1];
            let v1 = self.voltages[n-2];
            let v0 = self.voltages[n-3];
            
            let t2 = self.timestamps[n-1];
            let t1 = self.timestamps[n-2];
            let t0 = self.timestamps[n-3];
            
            let dt1 = t2 - t1;
            let dt0 = t1 - t0;
            
            let dv_dt1 = (v2 - v1) / dt1;
            let dv_dt0 = (v1 - v0) / dt0;
            
            let dt_avg = (dt1 + dt0) / 2.0;
            let d2v_dt2 = (dv_dt1 - dv_dt0) / dt_avg;
            
            Some((dv_dt1, d2v_dt2))
        }
    }
}

struct HighAccuracyController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    near_solution: bool,
}

impl HighAccuracyController {
    fn new() -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.00001,  // 0.001% for fine control near solution
            max_rate: 0.1,
            near_solution: false,
        }
    }
    
    fn update_based_on_convergence(&mut self, newton_iterations: usize, curvature: f64, ramp_factor: f64) {
        // Detect if we're near the solution (last 10%)
        if ramp_factor > 0.9 && !self.near_solution {
            self.near_solution = true;
            self.current_ramp_rate = 0.001; // Slow down for final approach
            println!("  Entering fine control mode for final convergence");
        }
        
        if self.near_solution {
            // Very careful control near solution
            if newton_iterations > 5 {
                self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
            } else if newton_iterations <= 2 && curvature.abs() < 100.0 {
                self.current_ramp_rate = (self.current_ramp_rate * 1.2).min(0.005);
            }
        } else {
            // Normal adaptive control
            if newton_iterations > 10 {
                self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
                println!("  Slowing ramp rate to {:.4}% (Newton took {} iterations)", 
                         self.current_ramp_rate * 100.0, newton_iterations);
            } else if newton_iterations < 3 && curvature.abs() < 1e3 {
                self.current_ramp_rate = (self.current_ramp_rate * 1.5).min(self.max_rate);
                println!("  Increasing ramp rate to {:.4}% (easy convergence)", 
                         self.current_ramp_rate * 100.0);
            }
        }
    }
}

pub struct HighAccuracyGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    node_histories: Vec<TimeEvolutionHistory>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: HighAccuracyController,
}

impl HighAccuracyGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            node_histories: vec![TimeEvolutionHistory::new(); num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: HighAccuracyController::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn high_accuracy_dc_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        println!("\nHigh Accuracy Gradient-Based DC Analysis");
        println!("Using tighter convergence criteria and refined ramping");
        
        // Count voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Get voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Adaptive source ramping
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Newton-Raphson to tight convergence
            let (converged, newton_iters) = self.solve_to_convergence(&mut total_iterations);
            
            if !converged {
                println!("  Warning: Newton-Raphson failed at ramp factor {:.6}", ramp_factor);
                self.controller.current_ramp_rate *= 0.5;
                continue;
            }
            
            // Record converged solution
            for i in 0..self.num_nodes {
                self.node_histories[i].add_converged_point(self.node_voltages[i], ramp_factor);
            }
            
            // Calculate curvature
            let mut max_curvature = 0.0f64;
            for i in 1..self.num_nodes {
                if let Some((_, d2v_dt2)) = self.node_histories[i].calculate_time_derivatives() {
                    max_curvature = max_curvature.max(d2v_dt2.abs());
                }
            }
            
            // Update controller
            self.controller.update_based_on_convergence(newton_iters, max_curvature, ramp_factor);
            
            // Advance ramp
            let old_ramp = ramp_factor;
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            
            // Ensure we hit exactly 1.0
            if old_ramp < 1.0 && ramp_factor > 1.0 {
                ramp_factor = 1.0;
            }
            
            ramp_step += 1;
            
            if ramp_step % 10 == 0 || self.controller.near_solution {
                println!("  Ramp: {:.6}, V2: {:.9}V, Iters: {}, Rate: {:.6}", 
                         ramp_factor, self.node_voltages[2], newton_iters, 
                         self.controller.current_ramp_rate);
            }
        }
        
        // Final solve at exactly 100% with extra tight tolerance
        println!("\nFinal solve at 100% with extra precision...");
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        let (converged, final_iters) = self.solve_to_convergence_tight(&mut total_iterations);
        
        if converged {
            println!("  Final convergence achieved in {} iterations", final_iters);
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("\n  Total ramp steps: {}", ramp_step);
        println!("  Total Newton iterations: {}", total_iterations);
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 50;
        let tol = 1e-12;  // Tighter tolerance
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let old_i = self.source_currents.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_v_change = 0.0f64;
                let mut max_i_change = 0.0f64;
                
                // Adaptive damping
                let damping = if iter < 3 { 0.7 } else if iter < 5 { 0.9 } else { 1.0 };
                
                // Update voltages
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_v_change = max_v_change.max(delta.abs());
                }
                
                // Update currents
                for i in 0..self.source_currents.len() {
                    let delta = x[n + i] - old_i[i];
                    self.source_currents[i] = old_i[i] + damping * delta;
                    max_i_change = max_i_change.max(delta.abs());
                }
                
                // Update element states
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                // Check both voltage and current convergence
                if max_v_change < tol && max_i_change < tol * 1e-3 {
                    return (true, iterations);
                }
            } else {
                return (false, iterations);
            }
        }
        
        (false, iterations)
    }
    
    fn solve_to_convergence_tight(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 100;
        let tol = 1e-15;  // Extra tight for final solution
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let _old_i = self.source_currents.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.clone().lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Full Newton step for final convergence
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = x[i];
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
                
                // Also check residual
                let (_, b_new) = self.build_mna_system();
                let residual = (&a * &x - &b_new).norm();
                
                if max_change < tol && residual < tol {
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
    println!("=== HIGH ACCURACY GRADIENT SOLVER ===");
    
    // SPICE reference with high precision
    let is = 1e-12;
    let vt = 0.026;
    let mut vd_spice = 0.7f64;
    
    // Extra iterations for high precision reference
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
    
    println!("\nSPICE Reference (high precision):");
    println!("  Vd = {:.12} V", vd_spice);
    println!("  Id = {:.12} mA", id_spice * 1000.0);
    
    // Test high accuracy solver
    let mut solver = HighAccuracyGradientSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(1.0)));
    let r = solver.add_element(Box::new(Resistor::new(100.0)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    let (vd, id, _iterations, time) = solver.high_accuracy_dc_analysis();
    
    let v_err = ((vd - vd_spice) / vd_spice * 100.0).abs();
    let i_err = ((id - id_spice) / id_spice * 100.0).abs();
    
    println!("\nHigh Accuracy Gradient Solver Results:");
    println!("  Vd = {:.12} V (error: {:.6}%)", vd, v_err);
    println!("  Id = {:.12} mA (error: {:.6}%)", id * 1000.0, i_err);
    println!("  Time: {:.1} ms", time);
    
    println!("\n=== ANALYSIS ===");
    if v_err < 0.001 && i_err < 0.001 {
        println!("✓ EXCELLENT: Achieved <0.001% accuracy!");
        println!("\nKey improvements:");
        println!("1. Tighter Newton-Raphson tolerance (1e-12 → 1e-15 for final)");
        println!("2. Fine ramping control near solution (0.001% steps)");
        println!("3. 5-point stencil for gradient calculation when possible");
        println!("4. Residual checking for final convergence");
        println!("5. Both voltage and current convergence criteria");
    } else if v_err < 0.01 && i_err < 0.01 {
        println!("✓ VERY GOOD: Achieved <0.01% accuracy!");
    } else if v_err < 0.1 && i_err < 0.1 {
        println!("✓ GOOD: Achieved <0.1% accuracy!");
    } else if v_err < 1.0 && i_err < 1.0 {
        println!("✓ SUCCESS: Achieved <1% accuracy!");
    } else {
        println!("○ Accuracy: {:.3}%", v_err.max(i_err));
    }
}