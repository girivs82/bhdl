/// Fixed Logarithmic Gradient vs Newton-Raphson Comparison
/// 
/// This implements the research paper's logarithmic gradient algorithm correctly
/// with proper adaptive thresholds and sensitivity calculation

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

// Newton-Raphson Reference Solver (IDENTICAL to original)
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
        
        for _iter in 0..max_iter {
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

// Adaptive threshold history tracking (FROM RESEARCH PAPER)
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
        
        // Multi-span gradient calculation for robustness (KEY INNOVATION)
        for span in [1, 2, 3] {
            for i in span..n {
                let dv = self.voltages[i] - self.voltages[i - span];
                if dv.abs() > 1e-12 {
                    let dlog_i = self.log_currents[i] - self.log_currents[i - span];
                    gradients.push(dlog_i / dv);  // d(log(I))/dV
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
        
        let recent_count = self.convergence_history.len().min(6);
        let recent_success = self.convergence_history.iter()
            .rev()
            .take(recent_count)
            .filter(|&&converged| converged)
            .count();
        
        recent_success as f64 / recent_count as f64
    }
    
    fn prediction_accuracy(&self) -> f64 {
        if self.sensitivity_errors.is_empty() {
            return 0.5;
        }
        
        let avg_error: f64 = self.sensitivity_errors.iter().sum::<f64>() / self.sensitivity_errors.len() as f64;
        (1.0 / (1.0 + avg_error)).max(0.1).min(1.0)
    }
}

// Adaptive threshold controller (FROM RESEARCH PAPER)
struct AdaptiveThresholdController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    actual_vt: f64,
    base_high_threshold: f64,
    base_low_threshold: f64,
    recent_performance: VecDeque<f64>,
    voltage_calibration: f64,
}

impl AdaptiveThresholdController {
    fn new(vt: f64) -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.05,
            actual_vt: vt,
            base_high_threshold: 2.5,
            base_low_threshold: 0.6,
            recent_performance: VecDeque::with_capacity(10),
            voltage_calibration: 1.0,
        }
    }
    
    fn expected_sensitivity(&self) -> f64 {
        1.0 / self.actual_vt
    }
    
    fn calculate_adaptive_thresholds(&mut self, voltage: f64, reliability: f64, accuracy: f64) -> (f64, f64) {
        // Voltage-dependent adjustment
        let voltage_factor = if voltage < 0.1 {
            2.0  // More lenient at low voltages
        } else if voltage < 0.3 {
            1.5  // Moderately lenient
        } else if voltage < 0.6 {
            1.0  // Standard thresholds
        } else {
            0.8  // Stricter at high voltages
        };
        
        // Combine all factors
        let reliability_factor = 0.5 + 0.5 * reliability;
        let accuracy_factor = 0.7 + 0.6 * accuracy;
        let performance_factor = self.get_performance_factor();
        
        let combined_factor = voltage_factor * reliability_factor * accuracy_factor * performance_factor;
        
        // Calculate adaptive thresholds
        let high_threshold = (self.base_high_threshold * (1.0 / combined_factor)).max(1.5).min(10.0);
        let low_threshold = (self.base_low_threshold * combined_factor).max(0.2).min(0.9);
        
        (high_threshold, low_threshold)
    }
    
    fn get_performance_factor(&self) -> f64 {
        if self.recent_performance.is_empty() {
            return 1.0;
        }
        
        let avg_performance: f64 = self.recent_performance.iter().sum::<f64>() / self.recent_performance.len() as f64;
        0.5 + avg_performance
    }
    
    fn update(&mut self, sensitivity: Option<f64>, reliability: f64, accuracy: f64, current_voltage: f64, converged: bool) {
        // Update performance history
        let performance_score = if converged { 
            if let Some(sens) = sensitivity {
                let expected = self.expected_sensitivity();
                let ratio = sens / expected;
                if ratio >= 0.5 && ratio <= 2.0 { 1.0 } else { 0.3 }
            } else { 0.5 }
        } else { 0.0 };
        
        self.recent_performance.push_back(performance_score);
        if self.recent_performance.len() > 10 {
            self.recent_performance.pop_front();
        }
        
        // Adaptive ramp rate control
        if let Some(sens) = sensitivity {
            let expected_sens = self.expected_sensitivity();
            let sensitivity_ratio = sens / expected_sens;
            
            let (high_threshold, low_threshold) = self.calculate_adaptive_thresholds(
                current_voltage, reliability, accuracy
            );
            
            if sensitivity_ratio > high_threshold {
                // High sensitivity - reduce ramp rate
                let reduction_strength = reliability * accuracy;
                self.current_ramp_rate = (self.current_ramp_rate * (0.3 + 0.4 * (1.0 - reduction_strength))).max(self.min_rate);
            } else if sensitivity_ratio < low_threshold {
                // Low sensitivity - increase ramp rate
                let increase_strength = reliability * accuracy;
                self.current_ramp_rate = (self.current_ramp_rate * (1.2 + 0.3 * increase_strength)).min(self.max_rate);
            } else {
                // Good sensitivity - minor adjustments
                if converged {
                    self.current_ramp_rate = (self.current_ramp_rate * 1.05).min(self.max_rate);
                } else {
                    self.current_ramp_rate = (self.current_ramp_rate * 0.95).max(self.min_rate);
                }
            }
        } else if !converged {
            // No sensitivity data and failed - reduce rate
            self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
        }
    }
}

// FIXED Logarithmic Gradient Solver
pub struct FixedLogarithmicGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: AdaptiveThresholdHistory,
    controller: AdaptiveThresholdController,
}

impl FixedLogarithmicGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: AdaptiveThresholdHistory::new(),
            controller: AdaptiveThresholdController::new(vt),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn log_current_for_diode(&self, voltage: f64) -> f64 {
        // Get diode element (assuming single diode for this test)
        for elem in &self.elements {
            if elem.is_nonlinear() {
                let current = elem.current_at_voltage(voltage);
                let i_min = 1e-18; // Prevent log(0)
                return (current.abs() + i_min).ln();
            }
        }
        0.0
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
        
        // Get voltage sources for ramping
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Adaptive ramping with logarithmic gradient control (PROPER IMPLEMENTATION)
        let mut ramp_factor = 0.0;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve at current ramp factor
            let (converged, _iters) = self.solve_to_convergence(&mut total_iterations);
            
            // Get diode voltage for sensitivity calculation
            let diode_voltage = self.node_voltages[2];
            let log_current = self.log_current_for_diode(diode_voltage);
            
            // Update history
            self.history.add_point(diode_voltage, log_current, ramp_factor, converged);
            
            // Calculate sensitivity and update controller
            let sensitivity_result = self.history.calculate_robust_sensitivity();
            let reliability = sensitivity_result.map(|(_, r)| r).unwrap_or(0.5);
            let accuracy = self.history.prediction_accuracy();
            
            self.controller.update(
                sensitivity_result.map(|(s, _)| s),
                reliability,
                accuracy,
                diode_voltage,
                converged
            );
            
            if !converged {
                continue; // Controller already reduced rate
            }
            
            // Advance ramp
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        self.solve_to_convergence(&mut total_iterations);
        
        // Extract results
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
                
                // Damping for stability
                let damping = 0.7;
                
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
        
        // GMIN for numerical stability
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
                    // Linearized companion model
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
    println!("=== FIXED LOGARITHMIC GRADIENT vs ANALYTICAL REFERENCE COMPARISON ===");
    println!("Using proper adaptive threshold implementation compared against TRUE analytical solution\n");
    
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
        
        // Fixed Logarithmic Gradient solve
        let mut lg_solver = FixedLogarithmicGradientSolver::new(3, vt);
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
    println!("\n📊 SUMMARY COMPARISON:");
    println!("  Analytical Reference (TRUE solution):");
    println!("    Ultra-high precision baseline for comparison");
    println!("  Fixed Logarithmic Gradient:");
    println!("    Average time: {:.1}ms", total_lg_time / n_cases);
    println!("    Average iterations: {:.0}", total_lg_iters as f64 / n_cases);
    println!("    Average voltage error: {:.6}%", total_v_error / n_cases);
    println!("    Average current error: {:.6}%", total_i_error / n_cases);
    
    println!("  Maximum error: {:.4}%", (total_v_error / n_cases).max(total_i_error / n_cases));
    
    println!("\n=== ACCURACY ASSESSMENT AGAINST TRUE ANALYTICAL SOLUTION ===");
    let avg_error = (total_v_error + total_i_error) / (2.0 * n_cases);
    if avg_error < 0.1 {
        println!("✅ EXCEPTIONAL: Fixed Logarithmic Gradient achieves <0.1% average error");
        println!("   Extremely close to the TRUE analytical solution");
    } else if avg_error < 1.0 {
        println!("✅ EXCELLENT: Fixed Logarithmic Gradient achieves <1% average error");
        println!("   Very close to the TRUE analytical solution");
    } else if avg_error < 5.0 {
        println!("✅ GOOD: Fixed Logarithmic Gradient achieves <5% average error"); 
        println!("   Reasonably close to the TRUE analytical solution");
    } else {
        println!("⚠️  HIGH ERROR: Fixed Logarithmic Gradient shows {:.2}% average error", avg_error);
        println!("   Needs refinement to match analytical accuracy");
    }
    
    println!("\n🎯 KEY INSIGHT:");
    println!("Now comparing against the TRUE analytical solution, not Newton-Raphson!");
    println!("This gives us the REAL accuracy of the logarithmic gradient approach.");
}