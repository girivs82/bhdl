/// Critical Damping Logarithmic Gradient Circuit Solver
/// 
/// Refined implementation of adaptive damping control:
/// 1. Simplified oscillation detection
/// 2. Smooth damping transitions
/// 3. Proper bounds and hysteresis
/// 4. Less verbose output

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

// Oscillation detector with smoothing
struct OscillationDetector {
    error_history: VecDeque<f64>,
    gradient_history: VecDeque<f64>,
    sign_changes: usize,
    last_gradient_sign: Option<bool>,
}

impl OscillationDetector {
    fn new() -> Self {
        Self {
            error_history: VecDeque::with_capacity(6),
            gradient_history: VecDeque::with_capacity(4),
            sign_changes: 0,
            last_gradient_sign: None,
        }
    }
    
    fn update(&mut self, error: f64) {
        self.error_history.push_back(error);
        if self.error_history.len() > 6 {
            self.error_history.pop_front();
        }
        
        // Calculate gradient if we have enough points
        if self.error_history.len() >= 2 {
            let n = self.error_history.len();
            let gradient = self.error_history[n-1] - self.error_history[n-2];
            
            self.gradient_history.push_back(gradient);
            if self.gradient_history.len() > 4 {
                self.gradient_history.pop_front();
            }
            
            // Check for sign change (oscillation)
            let current_sign = gradient > 0.0;
            if let Some(prev_sign) = self.last_gradient_sign {
                if current_sign != prev_sign && gradient.abs() > 1e-10 {
                    self.sign_changes += 1;
                }
            }
            self.last_gradient_sign = Some(current_sign);
        }
    }
    
    fn get_oscillation_metric(&self) -> f64 {
        if self.gradient_history.len() < 2 {
            return 0.0;
        }
        
        // Normalized oscillation metric (0 = no oscillation, 1 = strong oscillation)
        let recent_changes = (self.sign_changes as f64).min(4.0) / 4.0;
        
        // Also consider gradient variance
        let mean_grad: f64 = self.gradient_history.iter().sum::<f64>() / self.gradient_history.len() as f64;
        let variance: f64 = self.gradient_history.iter()
            .map(|&g| (g - mean_grad).powi(2))
            .sum::<f64>() / self.gradient_history.len() as f64;
        
        let normalized_variance = (variance.sqrt() / (mean_grad.abs() + 1e-10)).min(1.0);
        
        // Combine metrics
        0.7 * recent_changes + 0.3 * normalized_variance
    }
    
    fn reset_if_needed(&mut self, iterations: usize) {
        // Reset sign change count periodically
        if iterations % 20 == 0 {
            self.sign_changes = 0;
        }
    }
}

// Critical damping controller
struct CriticalDampingController {
    vt: f64,
    damping: f64,
    ramp_rate: f64,
    
    // Target and bounds
    critical_damping: f64,
    min_damping: f64,
    max_damping: f64,
    
    // Smooth transitions
    damping_momentum: f64,
    rate_momentum: f64,
    
    // State
    oscillation_detector: OscillationDetector,
    stable_steps: usize,
}

impl CriticalDampingController {
    fn new(vt: f64) -> Self {
        Self {
            vt,
            damping: 0.6,  // Start slightly underdamped
            ramp_rate: 0.01,
            
            critical_damping: 0.707,
            min_damping: 0.4,
            max_damping: 0.9,
            
            damping_momentum: 0.0,
            rate_momentum: 0.0,
            
            oscillation_detector: OscillationDetector::new(),
            stable_steps: 0,
        }
    }
    
    fn update(&mut self, voltage_error: f64, converged: bool, iterations: usize) {
        // Update oscillation detector
        self.oscillation_detector.update(voltage_error);
        self.oscillation_detector.reset_if_needed(iterations);
        
        let oscillation_metric = self.oscillation_detector.get_oscillation_metric();
        
        // Determine target damping based on oscillation
        let target_damping = if oscillation_metric > 0.6 {
            // Strong oscillation - increase damping
            (self.damping + 0.1).min(self.max_damping)
        } else if oscillation_metric < 0.2 && self.stable_steps > 5 {
            // Very stable - can reduce damping for speed
            (self.damping - 0.05).max(self.min_damping)
        } else {
            // Converge toward critical damping
            self.damping + (self.critical_damping - self.damping) * 0.1
        };
        
        // Apply smooth transition with momentum
        self.damping_momentum = 0.7 * self.damping_momentum + 0.3 * (target_damping - self.damping);
        self.damping += self.damping_momentum;
        self.damping = self.damping.clamp(self.min_damping, self.max_damping);
        
        // Update ramp rate based on convergence
        if converged {
            self.stable_steps += 1;
            
            // Can increase rate if stable
            if self.stable_steps > 3 && oscillation_metric < 0.3 {
                let rate_increase = 1.05 + 0.05 * (1.0 - oscillation_metric);
                self.ramp_rate *= rate_increase;
            }
        } else {
            self.stable_steps = 0;
            self.ramp_rate *= 0.5;  // Aggressive reduction on failure
        }
        
        // Apply bounds
        self.ramp_rate = self.ramp_rate.clamp(0.0001, 0.1);
    }
    
    fn get_state_string(&self) -> &str {
        let osc = self.oscillation_detector.get_oscillation_metric();
        if osc > 0.6 {
            "Underdamped"
        } else if osc < 0.2 && (self.damping - self.critical_damping).abs() > 0.1 {
            "Overdamped"
        } else {
            "Critical"
        }
    }
}

// Main solver
pub struct CriticalDampingLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: CriticalDampingController,
}

impl CriticalDampingLogGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: CriticalDampingController::new(vt),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
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
        let mut state_changes = Vec::new();
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Store previous voltage for error calculation
            let prev_diode_v = self.get_diode_voltage();
            
            // Get current state before update
            let prev_state = self.controller.get_state_string().to_string();
            
            // Solve with adaptive damping
            let (converged, _iters) = self.solve_to_convergence(&mut total_iterations);
            
            let diode_v = self.get_diode_voltage();
            let voltage_error = (diode_v - prev_diode_v).abs();
            
            // Update controller
            self.controller.update(voltage_error, converged, total_iterations);
            
            // Track state changes
            let new_state = self.controller.get_state_string();
            if new_state != prev_state {
                state_changes.push((ramp_factor, new_state.to_string()));
            }
            
            if !converged {
                continue;
            }
            
            // Advance ramp
            ramp_factor += self.controller.ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        self.solve_to_convergence(&mut total_iterations);
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        // Print concise state summary
        if !state_changes.is_empty() {
            print!("  [States:");
            for (ramp, state) in state_changes.iter().take(3) {
                print!(" {}@{:.0}%", &state[0..1], ramp * 100.0);
            }
            if state_changes.len() > 3 {
                print!(" ...");
            }
            println!("] Final damping: {:.3}", self.controller.damping);
        }
        
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
                
                // Use adaptive damping
                let damping = self.controller.damping;
                
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
    println!("=== CRITICAL DAMPING LOGARITHMIC GRADIENT SOLVER ===");
    println!("Adaptive damping control for optimal convergence\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Critical Vd", "Critical Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = CriticalDampingLogGradientSolver::new(3, vt);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_critical, id_critical, iters, time) = solver.solve();
        
        let v_err = ((vd_critical - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_critical - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 name, 
                 vd_ref, id_ref * 1000.0,
                 vd_critical, id_critical * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    let n_cases = test_cases.len() as f64;
    println!("\nCritical Damping Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
}