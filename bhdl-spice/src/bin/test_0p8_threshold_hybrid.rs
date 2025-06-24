/// Test 0.8 Threshold Hybrid Approach vs Analytical Solution
/// 
/// This tests the empirical 0.8 hybrid approach:
/// - Ramp quickly to 0.8 (linear region)
/// - Use smaller steps from 0.8 to 1.0 (nonlinear region)
/// This should combine speed AND accuracy by recognizing that most circuits
/// are linear until about 80% of final voltage, then become highly nonlinear

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Element trait and implementations (identical for all solvers)
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

// ANALYTICAL reference solution (TRUE golden standard)
fn analytical_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    let mut vd = 0.6; // Good starting point
    let tolerance = 1e-18; // Ultra-high precision
    
    for _iter in 0..1000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs; // Circuit equation
        let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs; // Derivative
        let delta = f / df_dvd;
        vd -= delta;
        
        if delta.abs() < tolerance {
            break;
        }
    }
    
    let id = is * ((vd / vt).exp() - 1.0);
    (vd, id)
}

// 0.8 Threshold Hybrid Solver
pub struct ThresholdHybridSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    threshold: f64,      // Threshold to switch from fast to slow ramping
    fast_step: f64,      // Step size for linear region (0.0 to threshold)
    slow_step: f64,      // Step size for nonlinear region (threshold to 1.0)
}

impl ThresholdHybridSolver {
    pub fn new(num_nodes: usize, threshold: f64, fast_step: f64, slow_step: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            threshold,
            fast_step,
            slow_step,
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
        
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Get voltage sources for ramping
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Phase 1: Fast ramping to threshold (linear region)
        let mut ramp_factor = 0.0;
        while ramp_factor < self.threshold {
            // Scale voltage sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve with moderate damping (linear region is stable)
            let iters = self.solve_step(0.8);
            total_iterations += iters;
            
            ramp_factor += self.fast_step;
            ramp_factor = ramp_factor.min(self.threshold);
        }
        
        // Phase 2: Slow ramping from threshold to 1.0 (nonlinear region)
        while ramp_factor < 1.0 {
            // Scale voltage sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve with conservative damping (nonlinear region needs stability)
            let iters = self.solve_step(0.6);
            total_iterations += iters;
            
            ramp_factor += self.slow_step;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        let final_iters = self.solve_step(0.7);
        total_iterations += final_iters;
        
        let diode_voltage = self.node_voltages[2];
        let current = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (diode_voltage, current, total_iterations, elapsed)
    }
    
    fn solve_step(&mut self, damping: f64) -> usize {
        let max_iter = 50;
        let tol = 1e-12;
        
        for iter in 0..max_iter {
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
                    return iter + 1;
                }
            } else {
                return iter + 1;
            }
        }
        max_iter
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

// Simple Ramping Solver (for comparison)
pub struct SimpleRampingSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    step_size: f64,
}

impl SimpleRampingSolver {
    pub fn new(num_nodes: usize, step_size: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            step_size,
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
        
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Get voltage sources for ramping
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Simple uniform ramping
        let mut ramp_factor = 0.0;
        while ramp_factor <= 1.0 {
            // Scale voltage sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            let iters = self.solve_step();
            total_iterations += iters;
            
            ramp_factor += self.step_size;
        }
        
        let diode_voltage = self.node_voltages[2];
        let current = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (diode_voltage, current, total_iterations, elapsed)
    }
    
    fn solve_step(&mut self) -> usize {
        let max_iter = 30;
        let tol = 1e-12;
        let damping = 0.7;
        
        for iter in 0..max_iter {
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
                    return iter + 1;
                }
            } else {
                return iter + 1;
            }
        }
        max_iter
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        // Same as ThresholdHybridSolver::build_mna_system()
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

fn main() {
    println!("=== 0.8 THRESHOLD HYBRID APPROACH vs ANALYTICAL SOLUTION ===");
    println!("Testing empirical 0.8 threshold: fast ramp to 0.8, then small steps to 1.0");
    println!("Compared against TRUE analytical solution (0.576342543266094V)\n");
    
    let test_cases = [
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", 1.0, 100.0, 1e-12, 0.050),
        ("Low current", 0.1, 1000.0, 1e-12, 0.026),
        ("High voltage", 5.0, 100.0, 1e-12, 0.026),
        ("Low resistance", 1.0, 10.0, 1e-12, 0.026),
    ];
    
    // Test different threshold configurations
    let configs = [
        ("0.8 thresh, 0.1/0.01 steps", 0.8, 0.1, 0.01),
        ("0.8 thresh, 0.05/0.005 steps", 0.8, 0.05, 0.005),
        ("0.7 thresh, 0.1/0.01 steps", 0.7, 0.1, 0.01),
        ("0.9 thresh, 0.1/0.01 steps", 0.9, 0.1, 0.01),
    ];
    
    println!("{:>15} | {:>12} | {:>12} | {:>12} | {:>12} | {:>12} | {:>8} | {:>8}", 
             "Test Case", "Analytical Vd", "Threshold Vd", "Simple Vd", "Thresh Err%", "Simple Err%", "T.Time", "S.Time");
    println!("{}", "=".repeat(120));
    
    for &(config_name, threshold, fast_step, slow_step) in &configs {
        println!("\n=== CONFIGURATION: {} ===", config_name);
        
        let mut total_thresh_error = 0.0;
        let mut total_simple_error = 0.0;
        let mut total_thresh_time = 0.0;
        let mut total_simple_time = 0.0;
        let mut total_thresh_iters = 0;
        let mut total_simple_iters = 0;
        
        for &(test_name, vs, rs, is, vt) in &test_cases {
            // TRUE analytical reference solution
            let (ref_vd, _ref_id) = analytical_reference(vs, rs, is, vt);
            
            // 1. Threshold Hybrid solve
            let mut thresh_solver = ThresholdHybridSolver::new(3, threshold, fast_step, slow_step);
            let v1 = thresh_solver.add_element(Box::new(VoltageSource::new(vs)));
            let r1 = thresh_solver.add_element(Box::new(Resistor::new(rs)));
            let d1 = thresh_solver.add_element(Box::new(Diode::new(is, vt)));
            thresh_solver.connect(v1, 1, 0);
            thresh_solver.connect(r1, 1, 2);
            thresh_solver.connect(d1, 2, 0);
            
            let (thresh_vd, _thresh_id, thresh_iters, thresh_time) = thresh_solver.solve();
            
            // 2. Simple Ramping solve (for comparison)
            let simple_step = (fast_step + slow_step) / 2.0; // Average step size
            let mut simple_solver = SimpleRampingSolver::new(3, simple_step);
            let v2 = simple_solver.add_element(Box::new(VoltageSource::new(vs)));
            let r2 = simple_solver.add_element(Box::new(Resistor::new(rs)));
            let d2 = simple_solver.add_element(Box::new(Diode::new(is, vt)));
            simple_solver.connect(v2, 1, 0);
            simple_solver.connect(r2, 1, 2);
            simple_solver.connect(d2, 2, 0);
            
            let (simple_vd, _simple_id, simple_iters, simple_time) = simple_solver.solve();
            
            // Calculate errors against TRUE analytical solution
            let thresh_err = if ref_vd != 0.0 { ((thresh_vd - ref_vd) / ref_vd * 100.0).abs() } else { 0.0 };
            let simple_err = if ref_vd != 0.0 { ((simple_vd - ref_vd) / ref_vd * 100.0).abs() } else { 0.0 };
            
            total_thresh_error += thresh_err;
            total_simple_error += simple_err;
            total_thresh_time += thresh_time;
            total_simple_time += simple_time;
            total_thresh_iters += thresh_iters;
            total_simple_iters += simple_iters;
            
            println!("{:>15} | {:>12.6} | {:>12.6} | {:>12.6} | {:>12.4} | {:>12.4} | {:>8.1} | {:>8.1}", 
                     test_name, 
                     ref_vd,
                     thresh_vd, simple_vd,
                     thresh_err, simple_err,
                     thresh_time, simple_time);
        }
        
        let n_cases = test_cases.len() as f64;
        println!("--- AVERAGES for {} ---", config_name);
        println!("Threshold: {:.4}% error, {:.1}ms, {} iters", 
                 total_thresh_error / n_cases, total_thresh_time / n_cases, total_thresh_iters as f64 / n_cases);
        println!("Simple:    {:.4}% error, {:.1}ms, {} iters", 
                 total_simple_error / n_cases, total_simple_time / n_cases, total_simple_iters as f64 / n_cases);
        
        // Performance comparison for this configuration
        let avg_thresh_err = total_thresh_error / n_cases;
        let avg_simple_err = total_simple_error / n_cases;
        let avg_thresh_time = total_thresh_time / n_cases;
        let avg_simple_time = total_simple_time / n_cases;
        
        if avg_thresh_err < avg_simple_err {
            println!("✅ ACCURACY: Threshold is {:.1}x more accurate", avg_simple_err / avg_thresh_err);
        } else {
            println!("❌ ACCURACY: Simple is {:.1}x more accurate", avg_thresh_err / avg_simple_err);
        }
        
        if avg_thresh_time < avg_simple_time {
            println!("✅ SPEED: Threshold is {:.1}x faster", avg_simple_time / avg_thresh_time);
        } else {
            println!("❌ SPEED: Simple is {:.1}x faster", avg_thresh_time / avg_simple_time);
        }
    }
    
    println!("\n=== OPTIMAL THRESHOLD ANALYSIS ===");
    println!("The 0.8 threshold approach recognizes that:");
    println!("1. 0.0-0.8: Circuit is mostly linear → can use large steps");
    println!("2. 0.8-1.0: Nonlinear effects dominate → need small steps");
    println!("3. This should provide both speed AND accuracy benefits");
    
    println!("\n🎯 KEY FINDINGS:");
    println!("The empirical 0.8 threshold was chosen because diode equations");
    println!("show exponential behavior becomes dominant above ~80% of final voltage.");
    println!("This hybrid approach should outperform both uniform ramping and");
    println!("complex adaptive schemes by matching the physics of the problem.");
}