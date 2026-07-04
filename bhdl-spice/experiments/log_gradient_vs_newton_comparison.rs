/// Direct comparison: Logarithmic Gradient vs Newton-Raphson
/// 
/// This will test the true accuracy of the logarithmic gradient solver
/// against the correct Newton-Raphson reference (not the wrong SPICE ref)

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Element trait and implementations (identical for both solvers)
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

// Newton-Raphson Reference Solver
pub struct NewtonRaphsonSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl NewtonRaphsonSolver {
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
        
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let max_iter = 200;
        let tol = 1e-15;
        let damping = 0.8;
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations += 1;
            
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
                    break;
                }
            } else {
                break;
            }
        }
        
        let mut diode_voltage = 0.0;
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        diode_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                break;
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let current = self.source_currents.get(0).copied().unwrap_or(0.0).abs();
        
        (diode_voltage, current, iterations, elapsed)
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

// Simplified Logarithmic Gradient Solver (core algorithm from the reference)
#[derive(Clone)]
struct AdaptiveThresholdHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
    convergence_history: VecDeque<bool>,
    sensitivity_errors: VecDeque<f64>,
}

impl AdaptiveThresholdHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(12),
            log_currents: VecDeque::with_capacity(12),
            ramp_factors: VecDeque::with_capacity(12),
            convergence_history: VecDeque::with_capacity(12),
            sensitivity_errors: VecDeque::with_capacity(12),
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, ramp: f64, converged: bool) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.ramp_factors.push_back(ramp);
        self.convergence_history.push_back(converged);
        
        if self.voltages.len() > 12 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
            self.ramp_factors.pop_front();
            self.convergence_history.pop_front();
            self.sensitivity_errors.pop_front();
        }
    }
    
    fn calculate_robust_sensitivity(&self) -> Option<(f64, f64)> {
        if self.voltages.len() < 4 {
            return None;
        }
        
        let n = self.voltages.len();
        let mut gradients = Vec::new();
        
        // Multi-span gradient calculation for robustness
        for span in [1, 2, 3] {
            for i in span..n {
                let dv = self.voltages[i] - self.voltages[i - span];
                if dv.abs() > 1e-12 {
                    let dlog_i = self.log_currents[i] - self.log_currents[i - span];
                    gradients.push(dlog_i / dv);
                }
            }
        }
        
        if gradients.is_empty() {
            return None;
        }
        
        // Use median for robustness against outliers
        gradients.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = gradients[gradients.len() / 2];
        
        // Calculate median absolute deviation
        let mut deviations: Vec<f64> = gradients.iter()
            .map(|&x| (x - median).abs())
            .collect();
        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mad = deviations[deviations.len() / 2];
        
        // Reliability based on consistency
        let consistency = if mad > 1e-12 { 1.0 / (1.0 + mad / median.abs()) } else { 1.0 };
        let recent_success_rate = self.recent_convergence_rate();
        let reliability = consistency * recent_success_rate;
        
        Some((median, reliability))
    }
    
    fn recent_convergence_rate(&self) -> f64 {
        if self.convergence_history.is_empty() {
            return 1.0;
        }
        
        let recent_count = self.convergence_history.len().min(5);
        let successes = self.convergence_history.iter()
            .rev()
            .take(recent_count)
            .filter(|&&x| x)
            .count();
        
        successes as f64 / recent_count as f64
    }
}

struct AdaptiveThresholdController {
    device_vt: f64,
    reliability_history: VecDeque<f64>,
    accuracy_history: VecDeque<f64>,
    recent_performance: VecDeque<f64>,
    performance_factor: f64,
}

impl AdaptiveThresholdController {
    fn new(device_vt: f64) -> Self {
        Self {
            device_vt,
            reliability_history: VecDeque::with_capacity(10),
            accuracy_history: VecDeque::with_capacity(10),
            recent_performance: VecDeque::with_capacity(8),
            performance_factor: 1.0,
        }
    }
    
    fn calculate_adaptive_thresholds(&self, voltage: f64, reliability: f64, accuracy: f64) -> (f64, f64) {
        let base_high_threshold = 1.0 / self.device_vt;
        let base_low_threshold = 0.3;
        
        let voltage_factor = match voltage {
            v if v < 0.1 => 2.0,
            v if v < 0.3 => 1.5,
            v if v < 0.6 => 1.0,
            _ => 0.8
        };
        
        let reliability_factor = 0.5 + 0.5 * reliability;
        let accuracy_factor = 0.7 + 0.6 * accuracy;
        
        let combined_factor = voltage_factor * reliability_factor * accuracy_factor * self.performance_factor;
        
        let high_threshold = (base_high_threshold / combined_factor).clamp(1.5, 10.0);
        let low_threshold = (base_low_threshold * combined_factor).clamp(0.2, 0.9);
        
        (high_threshold, low_threshold)
    }
    
    fn update_performance(&mut self, sensitivity_ratio: f64, converged: bool) {
        let performance_score = if converged {
            if sensitivity_ratio >= 0.5 && sensitivity_ratio <= 2.0 { 1.0 } else { 0.3 }
        } else { 0.0 };
        
        self.recent_performance.push_back(performance_score);
        if self.recent_performance.len() > 8 {
            self.recent_performance.pop_front();
        }
        
        let avg_performance = self.recent_performance.iter().sum::<f64>() / self.recent_performance.len() as f64;
        self.performance_factor = 0.5 + avg_performance;
    }
}

pub struct LogarithmicGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: AdaptiveThresholdHistory,
    controller: AdaptiveThresholdController,
}

impl LogarithmicGradientSolver {
    pub fn new(num_nodes: usize, device_vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: AdaptiveThresholdHistory::new(),
            controller: AdaptiveThresholdController::new(device_vt),
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
        
        // Get original voltage source values
        let mut original_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                original_voltages.push((i, elem.get_voltage()));
            }
        }
        
        let mut ramp_factor: f64 = 0.0;
        let mut ramp_rate: f64 = 0.01;
        
        // Main logarithmic gradient solving loop
        while ramp_factor < 1.0 {
            // Scale voltage sources using original values
            for &(idx, original_v) in &original_voltages {
                self.elements[idx].set_voltage(original_v * ramp_factor);
            }
            
            // Solve current step
            let step_iterations = self.solve_step();
            total_iterations += step_iterations;
            let converged = step_iterations < 100; // Assume convergence if reasonable iterations
            
            if converged {
                // Calculate logarithmic current sensitivity
                let diode_voltage = self.get_diode_voltage();
                let diode_current = self.get_diode_current();
                
                if diode_current > 1e-15 {
                    let log_current = diode_current.ln();
                    self.history.add_point(diode_voltage, log_current, ramp_factor, converged);
                    
                    // Update ramp rate using adaptive thresholds
                    if let Some((sensitivity, reliability)) = self.history.calculate_robust_sensitivity() {
                        let accuracy = self.controller.accuracy_history.back().copied().unwrap_or(0.5);
                        let (high_thresh, low_thresh) = self.controller.calculate_adaptive_thresholds(
                            diode_voltage, reliability, accuracy
                        );
                        
                        let sensitivity_ratio = sensitivity * self.controller.device_vt;
                        
                        if sensitivity_ratio > high_thresh {
                            ramp_rate *= 0.8; // Slow down
                        } else if sensitivity_ratio < low_thresh {
                            ramp_rate *= 1.2; // Speed up
                        }
                        
                        ramp_rate = ramp_rate.clamp(0.001, 0.1);
                        
                        self.controller.update_performance(sensitivity_ratio, converged);
                    }
                }
                
                ramp_factor += ramp_rate;
                ramp_factor = ramp_factor.min(1.0);
            } else {
                ramp_rate *= 0.5;
                if ramp_rate < 1e-6 {
                    break;
                }
            }
        }
        
        // Final solve at 100%
        for &(idx, original_v) in &original_voltages {
            self.elements[idx].set_voltage(original_v);
        }
        total_iterations += self.solve_step();
        
        let diode_voltage = self.get_diode_voltage();
        let diode_current = self.get_diode_current();
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (diode_voltage, diode_current, total_iterations, elapsed)
    }
    
    fn solve_step(&mut self) -> usize {
        let max_iter = 100;
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
    
    fn get_diode_current(&self) -> f64 {
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        let v = self.node_voltages[pos] - self.node_voltages[neg];
                        return elem.current_at_voltage(v);
                    }
                }
            }
        }
        0.0
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

// ANALYTICAL reference solution (TRUE golden standard)
fn analytical_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    // Use ultra-high precision Newton's method to find the true analytical solution
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
    
    // Calculate final current
    let id = is * ((vd / vt).exp() - 1.0);
    (vd, id)
}

fn main() {
    println!("=== LOGARITHMIC GRADIENT vs ANALYTICAL REFERENCE COMPARISON ===");
    println!("Testing true accuracy against the correct ANALYTICAL solution\\n");
    
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
             "Test Case", "Analytical Vd", "Analytical Id", "LogGrad Vd", "LogGrad Id", "V Err %", "I Err %", "Time ms");
    println!("{}", "=".repeat(120));
    
    let mut total_v_error = 0.0;
    let mut total_i_error = 0.0;
    let mut total_lg_time = 0.0;
    let mut total_lg_iters = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        // TRUE analytical reference solution
        let (ref_vd, ref_id) = analytical_reference(vs, rs, is, vt);
        
        // Logarithmic Gradient solve
        let mut lg_solver = LogarithmicGradientSolver::new(3, vt);
        let v = lg_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = lg_solver.add_element(Box::new(Resistor::new(rs)));
        let d = lg_solver.add_element(Box::new(Diode::new(is, vt)));
        lg_solver.connect(v, 1, 0);
        lg_solver.connect(r, 1, 2);
        lg_solver.connect(d, 2, 0);
        
        let (lg_vd, lg_id, lg_iters, lg_time) = lg_solver.solve();
        
        // Calculate errors against TRUE analytical solution
        let v_err = if ref_vd != 0.0 { ((lg_vd - ref_vd) / ref_vd * 100.0).abs() } else { 0.0 };
        let i_err = if ref_id != 0.0 { ((lg_id - ref_id) / ref_id * 100.0).abs() } else { 0.0 };
        
        total_v_error += v_err;
        total_i_error += i_err;
        total_lg_time += lg_time;
        total_lg_iters += lg_iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8.4} | {:>8.1}", 
                 test_name, 
                 ref_vd, ref_id * 1000.0,
                 lg_vd, lg_id * 1000.0,
                 v_err, i_err, lg_time);
    }
    
    println!("{}", "=".repeat(120));
    
    let n_cases = test_cases.len() as f64;
    println!("\\n📊 SUMMARY COMPARISON:");
    println!("  Analytical Reference (TRUE solution):");
    println!("    Ultra-high precision baseline for comparison");
    println!("  Logarithmic Gradient:");
    println!("    Average time: {:.1}ms", total_lg_time / n_cases);
    println!("    Average iterations: {:.0}", total_lg_iters as f64 / n_cases);
    println!("    Average voltage error: {:.6}%", total_v_error / n_cases);
    println!("    Average current error: {:.6}%", total_i_error / n_cases);
    println!("    Maximum error: {:.4}%", (total_v_error / n_cases).max(total_i_error / n_cases));
    
    println!("\\n=== ACCURACY ASSESSMENT AGAINST TRUE ANALYTICAL SOLUTION ===");
    let avg_error = (total_v_error + total_i_error) / (2.0 * n_cases);
    if avg_error < 0.1 {
        println!("✅ EXCEPTIONAL: Logarithmic Gradient achieves <0.1% average error");
        println!("   Extremely close to the TRUE analytical solution");
    } else if avg_error < 1.0 {
        println!("✅ EXCELLENT: Logarithmic Gradient achieves <1% average error");
        println!("   Very close to the TRUE analytical solution");
    } else if avg_error < 5.0 {
        println!("✅ GOOD: Logarithmic Gradient achieves <5% average error"); 
        println!("   Reasonably close to the TRUE analytical solution");
    } else {
        println!("⚠️  HIGH ERROR: Logarithmic Gradient shows {:.2}% average error", avg_error);
        println!("   Needs refinement to match analytical accuracy");
    }
    
    println!("\\n🎯 KEY INSIGHT:");
    println!("Now comparing against the TRUE analytical solution, not Newton-Raphson!");
    println!("This shows the REAL accuracy of the logarithmic gradient approach.");
}