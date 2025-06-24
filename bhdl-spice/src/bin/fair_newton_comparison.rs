/// Fair Newton-Raphson Implementation with Adaptive Ramping
/// 
/// This provides a proper Newton solver with adaptive step control
/// for fair comparison with the logarithmic gradient approach.

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Same element trait and implementations as before
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

// Improved Newton Solver with adaptive ramping
pub struct AdaptiveNewtonSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl AdaptiveNewtonSolver {
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
    
    pub fn solve(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        
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
        let mut ramp_step = 0.01; // Start with small steps
        let min_step = 0.0001;
        let max_step = 0.1;
        
        // Store previous good solution for backtracking
        let mut last_good_voltages = self.node_voltages.clone();
        let mut last_good_ramp = 0.0;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Try to solve at current ramp factor
            let (converged, iters) = self.solve_to_convergence(&mut total_iterations);
            
            if converged {
                // Success! Save state and try larger step
                last_good_voltages = self.node_voltages.clone();
                last_good_ramp = ramp_factor;
                
                // Increase step size if we converged quickly
                if iters < 5 {
                    ramp_step = (ramp_step * 1.5f64).min(max_step);
                } else if iters < 10 {
                    ramp_step = (ramp_step * 1.1f64).min(max_step);
                }
                
                // Advance ramp
                ramp_factor += ramp_step;
                ramp_factor = ramp_factor.min(1.0);
            } else {
                // Failed - backtrack and try smaller step
                self.node_voltages = last_good_voltages.clone();
                ramp_factor = last_good_ramp;
                ramp_step = (ramp_step * 0.5).max(min_step);
                
                if ramp_step <= min_step {
                    // Can't make progress even with minimum step
                    println!("  Newton: Stuck at ramp factor {:.3}", ramp_factor);
                    break;
                }
                
                ramp_factor += ramp_step;
                ramp_factor = ramp_factor.min(1.0);
            }
        }
        
        // Final solve at 100% if we made it
        if ramp_factor >= 1.0 {
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v);
            }
            self.solve_to_convergence(&mut total_iterations);
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 50; // More iterations allowed
        let tol = 1e-12;
        let mut iterations = 0;
        
        // Use adaptive damping
        let mut damping = 1.0;
        let min_damping = 0.1;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Try full Newton step first
                let mut new_voltages = vec![0.0; self.num_nodes];
                new_voltages[0] = 0.0; // Ground
                
                for i in 0..n {
                    new_voltages[i+1] = x[i];
                }
                
                // Check if update is reasonable
                let mut update_ok = true;
                for i in 1..self.num_nodes {
                    let delta = new_voltages[i] - old_v[i];
                    if delta.abs() > 3.0 { // Limit voltage jumps
                        update_ok = false;
                        break;
                    }
                }
                
                if !update_ok {
                    // Use damping
                    damping = (damping * 0.5f64).max(min_damping);
                    for i in 0..n {
                        let delta = x[i] - old_v[i+1];
                        self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                        max_change = max_change.max(delta.abs());
                    }
                } else {
                    // Full Newton step
                    self.node_voltages = new_voltages;
                    for i in 1..self.num_nodes {
                        let delta = self.node_voltages[i] - old_v[i];
                        max_change = max_change.max(delta.abs());
                    }
                    // Increase damping back toward 1.0
                    damping = (damping * 1.2).min(1.0);
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

// Test function
fn test_adaptive_newton(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64, usize, f64) {
    let mut solver = AdaptiveNewtonSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = solver.add_element(Box::new(Resistor::new(rs)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    solver.solve()
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
    println!("=== FAIR NEWTON-RAPHSON COMPARISON ===");
    println!("Improved Newton solver with adaptive ramping");
    
    let test_cases = [
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", 1.0, 100.0, 1e-12, 0.050),
        ("Low current", 0.1, 1000.0, 1e-12, 0.026),
        ("High voltage", 5.0, 100.0, 1e-12, 0.026),
        ("Low resistance", 1.0, 10.0, 1e-12, 0.026),
        ("Extreme low current", 0.05, 2000.0, 1e-12, 0.026),
        ("High current", 10.0, 50.0, 1e-12, 0.026),
    ];
    
    println!("\n{:>20} | {:>12} | {:>12} | {:>12} | {:>12} | {:>8} | {:>8} | {:>8}", 
             "Test Case", "SPICE Vd", "SPICE Id", "Newton Vd", "Newton Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    let mut success_count = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        let (vd_newton, id_newton, iters, time) = test_adaptive_newton(vs, rs, is, vt);
        
        let v_err = ((vd_newton - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_newton - id_ref) / id_ref * 100.0).abs();
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
                 vd_newton, id_newton * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    if success_count > 0 {
        let avg_error = total_error / success_count as f64;
        let avg_time = total_time / success_count as f64;
        let avg_iterations = total_iterations as f64 / success_count as f64;
        
        println!("\nNewton Solver Summary:");
        println!("  Success rate: {}/{} ({:.1}%)", success_count, test_cases.len(), 
                 success_count as f64 / test_cases.len() as f64 * 100.0);
        println!("  Average error: {:.4}%", avg_error);
        println!("  Average time: {:.1}ms", avg_time);
        println!("  Average iterations: {:.0}", avg_iterations);
    }
}