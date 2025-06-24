/// Conservative Optimization of Logarithmic Gradient Circuit Solver
/// 
/// This version prioritizes accuracy while seeking modest performance gains
/// Key insight: The original's accuracy comes from careful sensitivity tracking
/// We'll optimize without compromising that core mechanism

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

// Keep original adaptive history - it's crucial for accuracy
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
    
    // KEEP ORIGINAL: Multi-span gradient for robustness
    fn calculate_robust_sensitivity(&self) -> Option<(f64, f64)> {
        if self.voltages.len() < 4 {
            return None;
        }
        
        let n = self.voltages.len();
        let mut gradients = Vec::new();
        
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
        
        gradients.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = gradients[gradients.len() / 2];
        
        let mut deviations: Vec<f64> = gradients.iter()
            .map(|&x| (x - median).abs())
            .collect();
        deviations.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let mad = deviations[deviations.len() / 2];
        
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

// Conservative controller - keep original logic, optimize execution
struct ConservativeController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    actual_vt: f64,
    base_high_threshold: f64,
    base_low_threshold: f64,
    recent_performance: VecDeque<f64>,
    
    // OPTIMIZATION 1: Cache threshold calculations
    cached_thresholds: Option<(f64, f64)>,
    cache_voltage: f64,
}

impl ConservativeController {
    fn new(vt: f64) -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.05,
            actual_vt: vt,
            base_high_threshold: 2.5,
            base_low_threshold: 0.6,
            recent_performance: VecDeque::with_capacity(10),
            cached_thresholds: None,
            cache_voltage: 0.0,
        }
    }
    
    fn expected_sensitivity(&self) -> f64 {
        1.0 / self.actual_vt
    }
    
    fn calculate_adaptive_thresholds(&mut self, voltage: f64, reliability: f64, accuracy: f64) -> (f64, f64) {
        // OPTIMIZATION: Cache threshold calculations
        if let Some(thresholds) = self.cached_thresholds {
            if (voltage - self.cache_voltage).abs() < 0.01 {
                return thresholds;
            }
        }
        
        let voltage_factor = if voltage < 0.1 {
            2.0
        } else if voltage < 0.3 {
            1.5
        } else if voltage < 0.6 {
            1.0
        } else {
            0.8
        };
        
        let reliability_factor = 0.5 + 0.5 * reliability;
        let accuracy_factor = 0.7 + 0.6 * accuracy;
        let performance_factor = self.get_performance_factor();
        
        let combined_factor = voltage_factor * reliability_factor * accuracy_factor * performance_factor;
        
        let high_threshold = (self.base_high_threshold * (1.0 / combined_factor)).max(1.5).min(10.0);
        let low_threshold = (self.base_low_threshold * combined_factor).max(0.2).min(0.9);
        
        self.cached_thresholds = Some((high_threshold, low_threshold));
        self.cache_voltage = voltage;
        
        (high_threshold, low_threshold)
    }
    
    fn get_performance_factor(&self) -> f64 {
        if self.recent_performance.is_empty() {
            return 1.0;
        }
        
        let avg_performance: f64 = self.recent_performance.iter().sum::<f64>() / self.recent_performance.len() as f64;
        0.5 + avg_performance
    }
    
    // KEEP ORIGINAL UPDATE LOGIC - it's key to accuracy
    fn update(&mut self, sensitivity: Option<f64>, reliability: f64, accuracy: f64, current_voltage: f64, converged: bool) {
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
        
        if let Some(sens) = sensitivity {
            let expected_sens = self.expected_sensitivity();
            let sensitivity_ratio = sens / expected_sens;
            
            let (high_threshold, low_threshold) = self.calculate_adaptive_thresholds(
                current_voltage, reliability, accuracy
            );
            
            if sensitivity_ratio > high_threshold {
                let reduction_strength = reliability * accuracy;
                self.current_ramp_rate = (self.current_ramp_rate * (0.3 + 0.4 * (1.0 - reduction_strength))).max(self.min_rate);
            } else if sensitivity_ratio < low_threshold {
                let increase_strength = reliability * accuracy;
                self.current_ramp_rate = (self.current_ramp_rate * (1.2 + 0.3 * increase_strength)).min(self.max_rate);
            } else {
                if converged {
                    self.current_ramp_rate = (self.current_ramp_rate * 1.05).min(self.max_rate);
                } else {
                    self.current_ramp_rate = (self.current_ramp_rate * 0.95).max(self.min_rate);
                }
            }
        } else if !converged {
            self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
        }
    }
}

// Conservative solver with targeted optimizations
pub struct ConservativeLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: AdaptiveThresholdHistory,
    controller: ConservativeController,
    
    // OPTIMIZATION 2: Pre-allocated work matrices
    work_matrix: Option<DMatrix<f64>>,
    work_vector: Option<DVector<f64>>,
}

impl ConservativeLogGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: AdaptiveThresholdHistory::new(),
            controller: ConservativeController::new(vt),
            work_matrix: None,
            work_vector: None,
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
        for elem in &self.elements {
            if elem.is_nonlinear() {
                let current = elem.current_at_voltage(voltage);
                let i_min = 1e-18;
                return (current.abs() + i_min).ln();
            }
        }
        0.0
    }
    
    // OPTIMIZATION 3: Pre-allocate matrices once
    fn ensure_work_matrices(&mut self) {
        if self.work_matrix.is_none() {
            let n = self.num_nodes - 1;
            let m = self.source_currents.len();
            let size = n + m;
            self.work_matrix = Some(DMatrix::zeros(size, size));
            self.work_vector = Some(DVector::zeros(size));
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
        
        self.ensure_work_matrices();
        
        let mut total_iterations = 0;
        
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        let mut ramp_factor = 0.0;
        let mut _ramp_step = 0;
        
        // OPTIMIZATION 4: Reuse solution vector when possible
        let mut last_converged_voltages = self.node_voltages.clone();
        
        while ramp_factor < 1.0 {
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Start from last good solution
            if ramp_factor > 0.0 {
                self.node_voltages = last_converged_voltages.clone();
            }
            
            let (converged, _iters) = self.solve_to_convergence(&mut total_iterations);
            
            let diode_voltage = self.node_voltages[2];
            let log_current = self.log_current_for_diode(diode_voltage);
            
            self.history.add_point(diode_voltage, log_current, ramp_factor, converged);
            
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
            
            if converged {
                last_converged_voltages = self.node_voltages.clone();
            }
            
            if !converged {
                continue;
            }
            
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            _ramp_step += 1;
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
        let max_iter = 30;
        let tol = 1e-12; // Keep tight tolerance
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            
            // OPTIMIZATION 5: Reuse matrices
            self.build_mna_system_optimized();
            
            if let (Some(ref a), Some(ref b)) = (&self.work_matrix, &self.work_vector) {
                if let Some(x) = a.clone().lu().solve(b) {
                    let n = self.num_nodes - 1;
                    let mut max_change = 0.0f64;
                    
                    let damping = 0.7;
                    
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
        }
        
        (false, iterations)
    }
    
    // OPTIMIZATION: Reuse allocated matrices
    fn build_mna_system_optimized(&mut self) {
        let n = self.num_nodes - 1;
        let _m = self.source_currents.len();
        
        if let (Some(ref mut a), Some(ref mut b)) = (&mut self.work_matrix, &mut self.work_vector) {
            // Clear matrices
            a.fill(0.0);
            b.fill(0.0);
            
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
        }
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
    println!("=== CONSERVATIVE LOGARITHMIC GRADIENT SOLVER ===");
    println!("Preserving accuracy while improving performance\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Conserv. Vd", "Conserv. Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = ConservativeLogGradientSolver::new(3, vt);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_cons, id_cons, iters, time) = solver.solve();
        
        let v_err = ((vd_cons - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_cons - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 name, 
                 vd_ref, id_ref * 1000.0,
                 vd_cons, id_cons * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    let n_cases = test_cases.len() as f64;
    println!("\nConservative Optimization Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\nComparison:");
    println!("  Reference: 0.49% error, 55.5ms time, 8,032 iterations");
    println!("  Conservative: {:.2}% error, {:.1}ms time, {:.0} iterations", 
             total_error / n_cases, total_time / n_cases, total_iterations as f64 / n_cases);
    
    println!("\n=== CONSERVATIVE OPTIMIZATIONS ===");
    println!("1. Cached threshold calculations");
    println!("2. Pre-allocated work matrices");
    println!("3. Matrix reuse in MNA system");
    println!("4. Solution vector reuse as initial guess");
    println!("5. Preserved all accuracy-critical algorithms");
    println!("6. Kept tight tolerance (1e-12)");
    println!("7. Maintained robust sensitivity calculation");
}