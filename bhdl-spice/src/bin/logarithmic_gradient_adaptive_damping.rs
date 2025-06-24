/// Adaptive Damping Logarithmic Gradient Circuit Solver
/// 
/// Implements critical damping control based on second gradient monitoring.
/// Key insights:
/// 1. Start underdamped for faster initial response
/// 2. Monitor second gradient (gradient of sensitivity)
/// 3. Detect oscillations via sign changes in second gradient
/// 4. Adaptively adjust damping to approach critical damping
/// 5. Use oscillation frequency to tune step size

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Reuse element definitions from previous implementations
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

// Enhanced history tracking with second gradient
#[derive(Clone)]
struct DampingHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    sensitivities: VecDeque<f64>,
    second_gradients: VecDeque<f64>,
    voltage_errors: VecDeque<f64>,
    oscillation_count: usize,
    last_sign: Option<bool>,
}

impl DampingHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(10),
            log_currents: VecDeque::with_capacity(10),
            sensitivities: VecDeque::with_capacity(10),
            second_gradients: VecDeque::with_capacity(10),
            voltage_errors: VecDeque::with_capacity(10),
            oscillation_count: 0,
            last_sign: None,
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, voltage_error: Option<f64>) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        
        if let Some(error) = voltage_error {
            self.voltage_errors.push_back(error);
            if self.voltage_errors.len() > 10 {
                self.voltage_errors.pop_front();
            }
        }
        
        // Calculate first gradient (sensitivity)
        if self.voltages.len() >= 2 {
            let n = self.voltages.len();
            let dv = self.voltages[n-1] - self.voltages[n-2];
            let dlog_i = self.log_currents[n-1] - self.log_currents[n-2];
            
            if dv.abs() > 1e-12 {
                let sensitivity = dlog_i / dv;
                self.sensitivities.push_back(sensitivity);
                
                // Calculate second gradient
                if self.sensitivities.len() >= 2 {
                    let m = self.sensitivities.len();
                    let second_grad = self.sensitivities[m-1] - self.sensitivities[m-2];
                    self.second_gradients.push_back(second_grad);
                    
                    // Detect oscillation via sign change
                    let current_sign = second_grad > 0.0;
                    if let Some(prev_sign) = self.last_sign {
                        if current_sign != prev_sign {
                            self.oscillation_count += 1;
                        }
                    }
                    self.last_sign = Some(current_sign);
                }
            }
        }
        
        // Maintain window size
        if self.voltages.len() > 10 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
        }
        if self.sensitivities.len() > 8 {
            self.sensitivities.pop_front();
        }
        if self.second_gradients.len() > 6 {
            self.second_gradients.pop_front();
        }
    }
    
    fn get_damping_state(&self) -> DampingState {
        if self.second_gradients.len() < 3 {
            return DampingState::Unknown;
        }
        
        // Check recent oscillations
        let recent_oscillations = self.oscillation_count;
        
        // Calculate average magnitude of second gradient
        let avg_second_grad: f64 = self.second_gradients.iter()
            .map(|&x| x.abs())
            .sum::<f64>() / self.second_gradients.len() as f64;
        
        // Check error trend
        let error_decreasing = if self.voltage_errors.len() >= 3 {
            let n = self.voltage_errors.len();
            self.voltage_errors[n-1] < self.voltage_errors[n-3]
        } else {
            true
        };
        
        if recent_oscillations >= 2 && avg_second_grad > 0.1 {
            DampingState::Underdamped
        } else if recent_oscillations == 0 && avg_second_grad < 0.01 && error_decreasing {
            DampingState::Overdamped
        } else {
            DampingState::Critical
        }
    }
    
    fn get_oscillation_period(&self) -> Option<usize> {
        if self.oscillation_count < 2 {
            return None;
        }
        
        // Estimate period from oscillation frequency
        // This is simplified - a more sophisticated approach would track zero crossings
        Some((self.second_gradients.len() / self.oscillation_count.max(1)).max(2))
    }
    
    fn reset_oscillation_count(&mut self) {
        self.oscillation_count = 0;
    }
}

#[derive(Debug, Clone, Copy)]
enum DampingState {
    Underdamped,
    Critical,
    Overdamped,
    Unknown,
}

// Adaptive damping controller
struct AdaptiveDampingController {
    vt: f64,
    current_damping: f64,
    ramp_rate: f64,
    
    // Damping bounds
    min_damping: f64,  // Underdamped
    max_damping: f64,  // Overdamped
    critical_damping: f64,  // Target
    
    // Control parameters
    damping_adjustment_rate: f64,
    rate_adjustment_factor: f64,
    
    // State tracking
    consecutive_critical_steps: usize,
}

impl AdaptiveDampingController {
    fn new(vt: f64) -> Self {
        Self {
            vt,
            current_damping: 0.5,  // Start underdamped for fast response
            ramp_rate: 0.01,
            
            min_damping: 0.3,
            max_damping: 0.95,
            critical_damping: 0.707,  // Classic critical damping value
            
            damping_adjustment_rate: 0.1,
            rate_adjustment_factor: 1.2,
            
            consecutive_critical_steps: 0,
        }
    }
    
    fn update(&mut self, state: DampingState, history: &DampingHistory, converged: bool) {
        match state {
            DampingState::Underdamped => {
                // Increase damping to reduce oscillations
                self.current_damping = (self.current_damping + self.damping_adjustment_rate)
                    .min(self.max_damping);
                
                // Reduce step size based on oscillation period
                if let Some(period) = history.get_oscillation_period() {
                    // Shorter period = more aggressive oscillation = need smaller steps
                    let period_factor = (period as f64 / 4.0).clamp(0.5, 1.0);
                    self.ramp_rate *= 0.8 * period_factor;
                } else {
                    self.ramp_rate *= 0.8;
                }
                
                self.consecutive_critical_steps = 0;
                println!("  [Underdamped: increasing damping to {:.3}, reducing rate to {:.4}]", 
                         self.current_damping, self.ramp_rate);
            }
            
            DampingState::Overdamped => {
                // Decrease damping for faster response
                self.current_damping = (self.current_damping - self.damping_adjustment_rate)
                    .max(self.min_damping);
                
                // Can increase step size slightly
                if converged {
                    self.ramp_rate *= 1.1;
                }
                
                self.consecutive_critical_steps = 0;
                println!("  [Overdamped: decreasing damping to {:.3}, rate = {:.4}]", 
                         self.current_damping, self.ramp_rate);
            }
            
            DampingState::Critical => {
                // Near optimal - make small adjustments
                self.consecutive_critical_steps += 1;
                
                // Fine-tune toward ideal critical damping
                let damping_error = self.critical_damping - self.current_damping;
                self.current_damping += damping_error * 0.1;
                
                // If stable at critical damping, can increase rate
                if self.consecutive_critical_steps > 3 && converged {
                    self.ramp_rate *= 1.05;
                    println!("  [Critical damping maintained: rate increased to {:.4}]", self.ramp_rate);
                }
            }
            
            DampingState::Unknown => {
                // Not enough data - maintain current settings
            }
        }
        
        // Apply bounds
        self.ramp_rate = self.ramp_rate.clamp(0.0001, 0.1);
        
        // If not converging, always reduce rate
        if !converged {
            self.ramp_rate *= 0.5;
        }
    }
    
    fn get_damping(&self) -> f64 {
        self.current_damping
    }
}

// Main solver with adaptive damping
pub struct AdaptiveDampingLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: DampingHistory,
    controller: AdaptiveDampingController,
}

impl AdaptiveDampingLogGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: DampingHistory::new(),
            controller: AdaptiveDampingController::new(vt),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn get_diode_info(&self) -> (f64, f64) {
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        let v = self.node_voltages[pos] - self.node_voltages[neg];
                        let i = elem.current_at_voltage(v);
                        let log_i = (i.abs() + 1e-18).ln();
                        return (v, log_i);
                    }
                }
            }
        }
        (0.0, 0.0)
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
        let mut oscillation_resets = 0;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Store previous voltage for error calculation
            let prev_diode_v = self.get_diode_info().0;
            
            // Solve with adaptive damping
            let (converged, iters) = self.solve_to_convergence_adaptive(&mut total_iterations);
            
            let (diode_v, log_i) = self.get_diode_info();
            let voltage_error = (diode_v - prev_diode_v).abs();
            
            // Update history with error information
            self.history.add_point(diode_v, log_i, Some(voltage_error));
            
            // Get damping state and update controller
            let damping_state = self.history.get_damping_state();
            self.controller.update(damping_state, &self.history, converged);
            
            // Reset oscillation count periodically to track recent behavior
            if total_iterations % 50 == 0 {
                self.history.reset_oscillation_count();
                oscillation_resets += 1;
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
        self.solve_to_convergence_adaptive(&mut total_iterations);
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("  Oscillation resets: {} | Final damping: {:.3}", 
                 oscillation_resets, self.controller.get_damping());
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence_adaptive(&mut self, total_iterations: &mut usize) -> (bool, usize) {
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
                let damping = self.controller.get_damping();
                
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
    println!("=== ADAPTIVE DAMPING LOGARITHMIC GRADIENT SOLVER ===");
    println!("Critical damping control based on second gradient monitoring\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Adaptive Vd", "Adaptive Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = AdaptiveDampingLogGradientSolver::new(3, vt);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_adaptive, id_adaptive, iters, time) = solver.solve();
        
        let v_err = ((vd_adaptive - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_adaptive - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 name, 
                 vd_ref, id_ref * 1000.0,
                 vd_adaptive, id_adaptive * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    let n_cases = test_cases.len() as f64;
    println!("\nAdaptive Damping Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\nComparison:");
    println!("  Reference:        0.49% error, 55.5ms time, 8,032 iterations");
    println!("  Hybrid Two-Phase: 0.95% error, 1.7ms time, 202 iterations");
    println!("  Adaptive Damping: {:.2}% error, {:.1}ms time, {:.0} iterations", 
             total_error / n_cases, total_time / n_cases, total_iterations as f64 / n_cases);
    
    println!("\n=== ADAPTIVE DAMPING PRINCIPLES ===");
    println!("1. Start underdamped (ζ=0.5) for fast initial response");
    println!("2. Monitor second gradient to detect oscillations");
    println!("3. Adjust damping toward critical (ζ=0.707) based on behavior");
    println!("4. Use oscillation period to tune step size");
    println!("5. Achieve fastest convergence without overshooting");
}