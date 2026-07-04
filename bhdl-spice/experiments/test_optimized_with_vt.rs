/// Optimized logarithmic gradient solver with proper Vt parameter passing
/// This version passes the actual thermal voltage to the solver for correct
/// sensitivity expectations, fixing the High Vt case error.

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Element trait and implementations (same as before)
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

// IMPROVED: Solver that knows device thermal voltages
pub struct VtAwareOptimizedSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    nonlinear_elements: Vec<usize>,
    device_vts: Vec<f64>,  // Store Vt for each nonlinear device
}

impl VtAwareOptimizedSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            nonlinear_elements: Vec::new(),
            device_vts: Vec::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        let idx = self.elements.len();
        if element.is_nonlinear() {
            self.nonlinear_elements.push(idx);
        }
        self.elements.push(element);
        idx
    }
    
    // NEW: Add diode with known Vt
    pub fn add_diode(&mut self, is: f64, vt: f64) -> usize {
        let idx = self.add_element(Box::new(Diode::new(is, vt)));
        self.device_vts.push(vt);  // Store the Vt
        idx
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve_with_proper_vt(&mut self) -> (Vec<f64>, f64, usize) {
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
        
        // Ramping with device-specific expectations
        let mut ramp_factor = 0.0;
        let mut ramp_rate: f64 = 0.02;
        let mut sensitivity_history = VecDeque::with_capacity(5);
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve at current ramp
            let (converged, iters) = self.solve_to_convergence(&mut total_iterations);
            
            if converged && !self.nonlinear_elements.is_empty() {
                // Track sensitivity for first nonlinear device
                let elem_idx = self.nonlinear_elements[0];
                let device_idx = 0;  // Index into device_vts array
                
                let mut element_voltage = 0.0;
                for &(conn_elem, pos, neg) in &self.connections {
                    if conn_elem == elem_idx {
                        element_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                
                let current = self.elements[elem_idx].current_at_voltage(element_voltage);
                let log_current = (current.abs().max(1e-15)).ln();
                
                sensitivity_history.push_back((element_voltage, log_current));
                if sensitivity_history.len() > 5 {
                    sensitivity_history.pop_front();
                }
                
                // Adaptive rate using CORRECT expected sensitivity
                if sensitivity_history.len() >= 3 {
                    let n = sensitivity_history.len();
                    let dv = sensitivity_history[n-1].0 - sensitivity_history[n-3].0;
                    let dlog_i = sensitivity_history[n-1].1 - sensitivity_history[n-3].1;
                    
                    if dv.abs() > 1e-9 {
                        let sensitivity = (dlog_i / dv).abs();
                        
                        // Use the ACTUAL Vt for this device!
                        let expected = 1.0 / self.device_vts[device_idx];
                        let ratio = sensitivity / expected;
                        
                        // Adjust rate based on sensitivity
                        if ratio > 3.0 {
                            ramp_rate *= 0.7;
                        } else if ratio < 0.5 {
                            ramp_rate *= 1.3;
                        } else {
                            ramp_rate *= 1.1;
                        }
                        
                        ramp_rate = ramp_rate.max(0.001).min(0.1);
                    }
                }
            }
            
            if !converged {
                ramp_rate *= 0.5;
                continue;
            }
            
            ramp_factor += ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        self.solve_to_convergence(&mut total_iterations);
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (self.node_voltages.clone(), elapsed, total_iterations)
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
                
                let damping = if iter < 5 { 0.6 } else { 0.8 };
                
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

// Reference solution
fn analytical_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    let mut vd = 0.6;
    let tolerance = 1e-18;
    
    for _iter in 0..1000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        let delta = f / df_dvd;
        vd -= delta;
        if delta.abs() < tolerance { break; }
    }
    
    let id = (vs - vd) / rs;
    (vd, id)
}

fn main() {
    println!("=== VT-AWARE OPTIMIZED LOGARITHMIC GRADIENT TEST ===");
    println!("Now passing actual thermal voltage to solver for proper sensitivity expectations\n");
    
    let test_cases = vec![
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", 1.0, 100.0, 1e-12, 0.050),
        ("Low Current", 0.1, 1000.0, 1e-12, 0.026),
        ("High Voltage", 5.0, 100.0, 1e-12, 0.026),
        ("Low Resistance", 1.0, 10.0, 1e-12, 0.026),
        ("Extreme Low", 0.05, 2000.0, 1e-12, 0.026),
        ("High Current", 10.0, 50.0, 1e-12, 0.026),
    ];
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut count = 0;
    
    for (name, vs, rs, is, vt) in test_cases {
        let mut solver = VtAwareOptimizedSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r_idx = solver.add_element(Box::new(Resistor::new(rs)));
        let d_idx = solver.add_diode(is, vt);  // Use special method that tracks Vt
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_with_proper_vt();
        
        let (vd_ref, id_ref) = analytical_reference(vs, rs, is, vt);
        let vd_computed = voltages[2];
        let id_computed = (voltages[1] - voltages[2]) / rs;
        
        let v_err = ((vd_computed - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_computed - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        println!("{}: Vd={:.6}V (ref={:.6}V), Error={:.3}%, Time={:.1}ms, Iter={}", 
                 name, vd_computed, vd_ref, max_err, time, iterations);
        
        // Special attention to High Vt case
        if name == "High Vt" {
            println!("  → High Vt improvement: Using Vt={} for expected sensitivity={:.1}", 
                     vt, 1.0/vt);
        }
        
        total_error += max_err;
        total_time += time;
        count += 1;
    }
    
    let avg_error = total_error / count as f64;
    let avg_time = total_time / count as f64;
    
    println!("\n=== OPTIMIZATION RESULTS ===");
    println!("Average error: {:.2}%", avg_error);
    println!("Average time: {:.1}ms", avg_time);
    println!("\nPaper reference: 3.55% error, 21.5ms");
    
    if avg_error < 3.55 && avg_time < 21.5 {
        println!("✅ SUCCESS: Optimized solver beats paper reference!");
        println!("   Improvement: {:.1}x better accuracy, {:.1}x faster", 
                 3.55 / avg_error, 21.5 / avg_time);
    } else if avg_error < 3.55 || avg_time < 21.5 {
        println!("🔄 PARTIAL SUCCESS: One metric improved");
        if avg_time < 21.5 {
            println!("   Speed: {:.1}x faster", 21.5 / avg_time);
        }
        if avg_error < 3.55 {
            println!("   Accuracy: {:.1}x better", 3.55 / avg_error);
        }
    } else {
        println!("❌ More optimization needed");
    }
    
    println!("\n🎯 KEY INSIGHT: Passing correct Vt parameter is crucial!");
    println!("Without it, the solver uses wrong sensitivity expectations,");
    println!("leading to poor convergence especially for non-standard Vt values.");
}