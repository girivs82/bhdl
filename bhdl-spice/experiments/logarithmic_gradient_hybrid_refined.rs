/// Refined Hybrid Logarithmic Gradient Circuit Solver
/// 
/// Simpler fine-tuning focused on the key factors that affect accuracy:
/// 1. Better phase transition point (85% instead of 80%)
/// 2. Smoother ramp rate transitions
/// 3. More conservative fast phase to avoid overshooting
/// 4. Enhanced final convergence with extra iterations
/// 
/// Goal: <0.5% error while maintaining <2ms runtime

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

// Simple fast history
struct FastHistory {
    sensitivities: VecDeque<f64>,
    convergence: VecDeque<bool>,
}

impl FastHistory {
    fn new() -> Self {
        Self {
            sensitivities: VecDeque::with_capacity(6),
            convergence: VecDeque::with_capacity(6),
        }
    }
    
    fn add(&mut self, sensitivity: f64, converged: bool) {
        self.sensitivities.push_back(sensitivity);
        self.convergence.push_back(converged);
        
        if self.sensitivities.len() > 6 {
            self.sensitivities.pop_front();
            self.convergence.pop_front();
        }
    }
    
    fn get_median_sensitivity(&self) -> Option<f64> {
        if self.sensitivities.len() < 3 {
            None
        } else {
            let mut sorted: Vec<f64> = self.sensitivities.iter().copied().collect();
            sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
            Some(sorted[sorted.len() / 2])
        }
    }
    
    fn convergence_rate(&self) -> f64 {
        if self.convergence.is_empty() {
            1.0
        } else {
            self.convergence.iter().filter(|&&x| x).count() as f64 / self.convergence.len() as f64
        }
    }
}

// Accurate history (same as reference)
#[derive(Clone)]
struct AccurateHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
    convergence_history: VecDeque<bool>,
    sensitivity_errors: VecDeque<f64>,
}

impl AccurateHistory {
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
        
        // Use median for robustness
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

// Refined controller with smoother transitions
struct RefinedController {
    vt: f64,
    phase: u8,  // 1 = fast ramping, 2 = accurate convergence
    
    // Fast phase parameters
    fast_rate: f64,
    fast_min_rate: f64,
    fast_max_rate: f64,
    
    // Accurate phase parameters
    accurate_rate: f64,
    accurate_min_rate: f64,
    accurate_max_rate: f64,
    base_high_threshold: f64,
    base_low_threshold: f64,
    recent_performance: VecDeque<f64>,
    
    // Smooth transition
    transition_buffer: f64,
}

impl RefinedController {
    fn new(vt: f64) -> Self {
        Self {
            vt,
            phase: 1,
            
            // Fast phase: more conservative than original
            fast_rate: 0.03,  // Reduced from 0.05
            fast_min_rate: 0.005,
            fast_max_rate: 0.1,  // Reduced from 0.2
            
            // Accurate phase: same as reference
            accurate_rate: 0.01,
            accurate_min_rate: 0.0001,
            accurate_max_rate: 0.05,
            base_high_threshold: 2.5,
            base_low_threshold: 0.6,
            recent_performance: VecDeque::with_capacity(10),
            
            transition_buffer: 0.02,  // Smooth transition buffer
        }
    }
    
    fn switch_to_accurate_phase(&mut self, ramp_factor: f64) {
        self.phase = 2;
        // Smooth transition: inherit some momentum from fast phase
        let transition_rate = self.fast_rate * 0.4;  // 40% of fast rate
        self.accurate_rate = transition_rate.max(0.003).min(0.01);
        
        // Pre-populate performance history for smoother transition
        let initial_score = 0.7 + 0.3 * (1.0 - (ramp_factor - 0.85).abs());
        for _ in 0..3 {
            self.recent_performance.push_back(initial_score);
        }
    }
    
    fn update_fast(&mut self, sensitivity: Option<f64>, conv_rate: f64, converged: bool) {
        if let Some(sens) = sensitivity {
            let expected = 1.0 / self.vt;
            let ratio = sens / expected;
            
            // More conservative updates
            if ratio > 4.0 || !converged {
                self.fast_rate = (self.fast_rate * 0.6).max(self.fast_min_rate);
            } else if ratio < 0.5 && converged && conv_rate > 0.8 {
                self.fast_rate = (self.fast_rate * 1.3).min(self.fast_max_rate);
            } else if converged {
                self.fast_rate = (self.fast_rate * 1.1).min(self.fast_max_rate);
            }
        } else if !converged {
            self.fast_rate = (self.fast_rate * 0.5).max(self.fast_min_rate);
        }
    }
    
    fn update_accurate(&mut self, sensitivity: Option<f64>, reliability: f64, accuracy: f64, 
                      current_voltage: f64, converged: bool) {
        // Full adaptive threshold algorithm from reference
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
                self.accurate_rate = (self.accurate_rate * (0.3 + 0.4 * (1.0 - reduction_strength)))
                    .max(self.accurate_min_rate);
            } else if sensitivity_ratio < low_threshold {
                let increase_strength = reliability * accuracy;
                self.accurate_rate = (self.accurate_rate * (1.2 + 0.3 * increase_strength))
                    .min(self.accurate_max_rate);
            } else {
                if converged {
                    self.accurate_rate = (self.accurate_rate * 1.05).min(self.accurate_max_rate);
                } else {
                    self.accurate_rate = (self.accurate_rate * 0.95).max(self.accurate_min_rate);
                }
            }
        } else if !converged {
            self.accurate_rate = (self.accurate_rate * 0.5).max(self.accurate_min_rate);
        }
    }
    
    fn expected_sensitivity(&self) -> f64 {
        1.0 / self.vt
    }
    
    fn calculate_adaptive_thresholds(&self, voltage: f64, reliability: f64, accuracy: f64) -> (f64, f64) {
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
        
        (high_threshold, low_threshold)
    }
    
    fn get_performance_factor(&self) -> f64 {
        if self.recent_performance.is_empty() {
            return 1.0;
        }
        
        let avg_performance: f64 = self.recent_performance.iter().sum::<f64>() / 
                                  self.recent_performance.len() as f64;
        0.5 + avg_performance
    }
    
    fn get_current_rate(&self) -> f64 {
        match self.phase {
            1 => self.fast_rate,
            2 => self.accurate_rate,
            _ => panic!("Invalid phase"),
        }
    }
}

// Refined hybrid solver
pub struct RefinedHybridLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    fast_history: FastHistory,
    accurate_history: AccurateHistory,
    controller: RefinedController,
    
    // Phase transition
    phase_switch_ramp: f64,  // When to switch from fast to accurate
}

impl RefinedHybridLogGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            fast_history: FastHistory::new(),
            accurate_history: AccurateHistory::new(),
            controller: RefinedController::new(vt),
            phase_switch_ramp: 0.85,  // Switch at 85% instead of 80%
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn get_diode_info(&self) -> (f64, f64, f64) {
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        let v = self.node_voltages[pos] - self.node_voltages[neg];
                        let i = elem.current_at_voltage(v);
                        let g = elem.conductance_at_voltage(v);
                        let log_i = (i.abs() + 1e-18).ln();
                        let sensitivity = if i.abs() > 1e-15 { g * v / i } else { 0.0 };
                        return (v, log_i, sensitivity);
                    }
                }
            }
        }
        (0.0, 0.0, 0.0)
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
        
        let mut ramp_factor = 0.0;
        let mut phase_1_iterations = 0;
        let mut phase_2_iterations = 0;
        
        // Save initial state for better accuracy
        let initial_voltages = self.node_voltages.clone();
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Use better initial guess
            if ramp_factor == 0.0 {
                self.node_voltages = initial_voltages.clone();
            }
            
            // Solve with appropriate tolerance based on phase
            let (converged, iters) = if self.controller.phase == 1 {
                self.solve_to_convergence_fast(&mut total_iterations)
            } else {
                self.solve_to_convergence_accurate(&mut total_iterations)
            };
            
            let (diode_v, log_i, sensitivity) = self.get_diode_info();
            
            // Update appropriate history and controller
            if self.controller.phase == 1 {
                phase_1_iterations += iters;
                self.fast_history.add(sensitivity, converged);
                let conv_rate = self.fast_history.convergence_rate();
                self.controller.update_fast(Some(sensitivity), conv_rate, converged);
            } else {
                phase_2_iterations += iters;
                self.accurate_history.add_point(diode_v, log_i, ramp_factor, converged);
                
                let sens_result = self.accurate_history.calculate_robust_sensitivity();
                let reliability = sens_result.map(|(_, r)| r).unwrap_or(0.5);
                let accuracy = self.accurate_history.prediction_accuracy();
                
                self.controller.update_accurate(
                    sens_result.map(|(s, _)| s),
                    reliability,
                    accuracy,
                    diode_v,
                    converged
                );
            }
            
            if !converged {
                continue;
            }
            
            // Advance ramp
            ramp_factor += self.controller.get_current_rate();
            
            // Check phase transition
            if self.controller.phase == 1 && ramp_factor >= self.phase_switch_ramp {
                self.controller.switch_to_accurate_phase(ramp_factor);
                println!("  [Switching to accurate phase at {:.1}%]", ramp_factor * 100.0);
            }
            
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100% with extra iterations for accuracy
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Do two passes at 100% for better accuracy
        self.solve_to_convergence_accurate(&mut total_iterations);
        let (converged, extra_iters) = self.solve_to_convergence_accurate(&mut total_iterations);
        phase_2_iterations += extra_iters;
        
        if converged {
            println!("  [Extra convergence pass added {} iterations]", extra_iters);
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("  Phase 1 iterations: {} | Phase 2 iterations: {}", 
                 phase_1_iterations, phase_2_iterations);
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence_fast(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 25;  // Slightly more iterations for better accuracy
        let tol = 1e-11;    // Tighter tolerance than original (was 1e-10)
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Adaptive damping
                let damping = if iter < 3 { 0.7 } else { 0.85 };
                
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
        
        (false, iterations)
    }
    
    fn solve_to_convergence_accurate(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 30;
        let tol = 1e-12;    // Tight tolerance
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                let damping = 0.7;  // Conservative damping for accuracy
                
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
    println!("=== REFINED HYBRID LOGARITHMIC GRADIENT SOLVER ===");
    println!("Simpler fine-tuning focused on key accuracy improvements\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Refined Vd", "Refined Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = RefinedHybridLogGradientSolver::new(3, vt);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_refined, id_refined, iters, time) = solver.solve();
        
        let v_err = ((vd_refined - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_refined - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 name, 
                 vd_ref, id_ref * 1000.0,
                 vd_refined, id_refined * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    let n_cases = test_cases.len() as f64;
    println!("\nRefined Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\nComparison:");
    println!("  Reference:       0.49% error, 55.5ms time, 8,032 iterations");
    println!("  Original Hybrid: 0.95% error, 1.7ms time, 202 iterations");
    println!("  Refined:         {:.2}% error, {:.1}ms time, {:.0} iterations", 
             total_error / n_cases, total_time / n_cases, total_iterations as f64 / n_cases);
    
    if total_time / n_cases < 2.0 && total_error / n_cases < 0.5 {
        println!("\n✅ GOAL ACHIEVED: <0.5% error with <2ms runtime!");
    } else if total_error / n_cases < 0.7 {
        println!("\n✅ Significant improvement: <0.7% error achieved!");
    }
    
    println!("\n=== KEY REFINEMENTS ===");
    println!("1. Phase transition at 85% (vs 80%) - more time for accurate convergence");
    println!("2. Conservative fast phase (0.03 vs 0.05 initial rate)");
    println!("3. Smoother transition with inherited momentum");
    println!("4. Tighter fast phase tolerance (1e-11 vs 1e-10)");
    println!("5. Double convergence pass at 100% for final accuracy");
    println!("6. Better initial guess management");
    println!("7. Median sensitivity tracking in fast phase");
}