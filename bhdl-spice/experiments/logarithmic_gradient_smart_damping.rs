/// Smart Damping Logarithmic Gradient Circuit Solver
/// 
/// Implements two advanced damping strategies:
/// 1. Immediate Overdamping: On oscillation detection, immediately overdamp to prevent further oscillation
/// 2. Controlled Decay: Allow 2-3 oscillations with progressively smaller amplitude
/// 
/// Key insight: Use second derivative direction changes as triggers for damping adjustments

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

#[derive(Debug, Clone, Copy, PartialEq)]
enum DampingStrategy {
    ImmediateOverdamp,  // Strategy 1: Immediate overdamping on oscillation
    ControlledDecay,    // Strategy 2: Allow 2-3 oscillations with decay
}

// Smart oscillation detector with derivative tracking
struct SmartOscillationDetector {
    error_history: VecDeque<f64>,
    first_derivatives: VecDeque<f64>,
    second_derivatives: VecDeque<f64>,
    zero_crossings: usize,
    last_first_deriv_sign: Option<bool>,
    last_second_deriv_sign: Option<bool>,
    oscillation_amplitudes: VecDeque<f64>,
}

impl SmartOscillationDetector {
    fn new() -> Self {
        Self {
            error_history: VecDeque::with_capacity(8),
            first_derivatives: VecDeque::with_capacity(6),
            second_derivatives: VecDeque::with_capacity(4),
            zero_crossings: 0,
            last_first_deriv_sign: None,
            last_second_deriv_sign: None,
            oscillation_amplitudes: VecDeque::with_capacity(3),
        }
    }
    
    fn update(&mut self, error: f64) -> Option<OscillationEvent> {
        self.error_history.push_back(error);
        if self.error_history.len() > 8 {
            self.error_history.pop_front();
        }
        
        // Calculate first derivative
        if self.error_history.len() >= 2 {
            let n = self.error_history.len();
            let first_deriv = self.error_history[n-1] - self.error_history[n-2];
            
            self.first_derivatives.push_back(first_deriv);
            if self.first_derivatives.len() > 6 {
                self.first_derivatives.pop_front();
            }
            
            // Track amplitude at zero crossing of first derivative
            let current_sign = first_deriv > 0.0;
            if let Some(prev_sign) = self.last_first_deriv_sign {
                if current_sign != prev_sign && first_deriv.abs() > 1e-12 {
                    // Zero crossing - record amplitude
                    self.oscillation_amplitudes.push_back(error.abs());
                    if self.oscillation_amplitudes.len() > 3 {
                        self.oscillation_amplitudes.pop_front();
                    }
                }
            }
            self.last_first_deriv_sign = Some(current_sign);
            
            // Calculate second derivative
            if self.first_derivatives.len() >= 2 {
                let m = self.first_derivatives.len();
                let second_deriv = self.first_derivatives[m-1] - self.first_derivatives[m-2];
                
                self.second_derivatives.push_back(second_deriv);
                if self.second_derivatives.len() > 4 {
                    self.second_derivatives.pop_front();
                }
                
                // Detect second derivative sign change
                let current_second_sign = second_deriv > 0.0;
                if let Some(prev_second_sign) = self.last_second_deriv_sign {
                    if current_second_sign != prev_second_sign && second_deriv.abs() > 1e-15 {
                        self.zero_crossings += 1;
                        
                        // Determine oscillation characteristics
                        let amplitude_decay = self.calculate_amplitude_decay();
                        let oscillation_strength = self.calculate_oscillation_strength();
                        
                        self.last_second_deriv_sign = Some(current_second_sign);
                        
                        return Some(OscillationEvent {
                            crossing_count: self.zero_crossings,
                            amplitude_decay,
                            oscillation_strength,
                            current_amplitude: error.abs(),
                        });
                    }
                }
                self.last_second_deriv_sign = Some(current_second_sign);
            }
        }
        
        None
    }
    
    fn calculate_amplitude_decay(&self) -> f64 {
        if self.oscillation_amplitudes.len() < 2 {
            return 1.0;
        }
        
        let n = self.oscillation_amplitudes.len();
        let ratio = self.oscillation_amplitudes[n-1] / (self.oscillation_amplitudes[n-2] + 1e-15);
        ratio.clamp(0.0, 2.0)
    }
    
    fn calculate_oscillation_strength(&self) -> f64 {
        if self.second_derivatives.is_empty() {
            return 0.0;
        }
        
        // RMS of second derivatives
        let sum_sq: f64 = self.second_derivatives.iter()
            .map(|&x| x * x)
            .sum();
        (sum_sq / self.second_derivatives.len() as f64).sqrt()
    }
    
    fn reset_if_stable(&mut self, iterations: usize) {
        // Reset crossing count periodically if stable
        if iterations % 30 == 0 && self.zero_crossings == 0 {
            self.zero_crossings = 0;
            self.oscillation_amplitudes.clear();
        }
    }
}

#[derive(Debug, Clone)]
struct OscillationEvent {
    crossing_count: usize,
    amplitude_decay: f64,
    oscillation_strength: f64,
    current_amplitude: f64,
}

// Smart damping controller
struct SmartDampingController {
    strategy: DampingStrategy,
    damping: f64,
    ramp_rate: f64,
    
    // Strategy-specific parameters
    target_oscillations: usize,  // For controlled decay
    overdamp_factor: f64,        // For immediate overdamp
    
    // Bounds
    min_damping: f64,
    max_damping: f64,
    min_rate: f64,
    max_rate: f64,
    
    // State
    detector: SmartOscillationDetector,
    oscillation_events: Vec<OscillationEvent>,
}

impl SmartDampingController {
    fn new(strategy: DampingStrategy) -> Self {
        let (target_osc, overdamp) = match strategy {
            DampingStrategy::ImmediateOverdamp => (1, 2.0),    // Overdamp immediately
            DampingStrategy::ControlledDecay => (3, 1.3),      // Allow 2-3 oscillations
        };
        
        Self {
            strategy,
            damping: 0.5,  // Start underdamped
            ramp_rate: 0.01,
            
            target_oscillations: target_osc,
            overdamp_factor: overdamp,
            
            min_damping: 0.3,
            max_damping: 0.95,
            min_rate: 0.0001,
            max_rate: 0.1,
            
            detector: SmartOscillationDetector::new(),
            oscillation_events: Vec::new(),
        }
    }
    
    fn update(&mut self, error: f64, converged: bool, iterations: usize) {
        // Update detector and check for oscillation event
        if let Some(event) = self.detector.update(error) {
            self.oscillation_events.push(event.clone());
            
            // React based on strategy
            match self.strategy {
                DampingStrategy::ImmediateOverdamp => {
                    self.handle_immediate_overdamp(&event);
                }
                DampingStrategy::ControlledDecay => {
                    self.handle_controlled_decay(&event);
                }
            }
        }
        
        // Regular updates
        if converged && self.detector.zero_crossings == 0 {
            // No oscillations - can carefully increase rate
            self.ramp_rate = (self.ramp_rate * 1.05).min(self.max_rate);
        } else if !converged {
            // Failed to converge - reduce rate
            self.ramp_rate = (self.ramp_rate * 0.5).max(self.min_rate);
        }
        
        self.detector.reset_if_stable(iterations);
    }
    
    fn handle_immediate_overdamp(&mut self, event: &OscillationEvent) {
        // Strategy 1: Immediately go to overdamped state
        println!("  [Oscillation detected! Immediately overdamping]");
        
        // Increase damping significantly
        self.damping = (self.damping * self.overdamp_factor).min(self.max_damping);
        
        // Reduce step size proportionally to oscillation strength
        let rate_reduction = 0.3 / (1.0 + event.oscillation_strength);
        self.ramp_rate = (self.ramp_rate * rate_reduction).max(self.min_rate);
        
        // After first oscillation, gradually relax damping
        if event.crossing_count > 1 {
            self.damping *= 0.95;
        }
    }
    
    fn handle_controlled_decay(&mut self, event: &OscillationEvent) {
        // Strategy 2: Allow controlled oscillations with decay
        let oscillation_num = (event.crossing_count + 1) / 2;  // Each full oscillation = 2 crossings
        
        if oscillation_num <= self.target_oscillations {
            // Within target oscillations - adjust for decay
            println!("  [Oscillation {}/{} - decay: {:.3}]", 
                     oscillation_num, self.target_oscillations, event.amplitude_decay);
            
            if event.amplitude_decay > 0.7 {
                // Not decaying fast enough - increase damping
                self.damping = (self.damping * 1.2).min(self.max_damping);
                self.ramp_rate *= 0.8;
            } else if event.amplitude_decay < 0.3 {
                // Decaying too fast - might be overdamped
                self.damping = (self.damping * 0.9).max(self.min_damping);
            }
            // else: Good decay rate, maintain current damping
            
        } else {
            // Exceeded target oscillations - need stronger damping
            println!("  [Exceeded target oscillations - increasing damping]");
            self.damping = (self.damping * 1.5).min(self.max_damping);
            self.ramp_rate = (self.ramp_rate * 0.5).max(self.min_rate);
        }
    }
    
    fn get_summary(&self) -> String {
        if self.oscillation_events.is_empty() {
            "No oscillations".to_string()
        } else {
            let n = self.oscillation_events.len();
            let last = &self.oscillation_events[n-1];
            format!("{} events, final decay: {:.2}", n, last.amplitude_decay)
        }
    }
}

// Main solver
pub struct SmartDampingLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: SmartDampingController,
}

impl SmartDampingLogGradientSolver {
    pub fn new(num_nodes: usize, strategy: DampingStrategy) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: SmartDampingController::new(strategy),
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
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Store previous voltage for error calculation
            let prev_diode_v = self.get_diode_voltage();
            
            // Solve with adaptive damping
            let (converged, _iters) = self.solve_to_convergence(&mut total_iterations);
            
            let diode_v = self.get_diode_voltage();
            let voltage_error = diode_v - prev_diode_v;  // Keep sign for derivative
            
            // Update controller
            self.controller.update(voltage_error, converged, total_iterations);
            
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

fn test_strategy(strategy: DampingStrategy, name: &str) {
    println!("\n=== {} STRATEGY ===", name);
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Smart Vd", "Smart Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = SmartDampingLogGradientSolver::new(3, strategy);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_smart, id_smart, iters, time) = solver.solve();
        
        let v_err = ((vd_smart - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_smart - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        let summary = solver.controller.get_summary();
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1} | {}", 
                 test_name, 
                 vd_ref, id_ref * 1000.0,
                 vd_smart, id_smart * 1000.0,
                 max_err, iters, time, summary);
    }
    
    println!("{}", "=".repeat(110));
    
    let n_cases = test_cases.len() as f64;
    println!("\n{} Results:", name);
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
}

fn main() {
    println!("=== SMART DAMPING LOGARITHMIC GRADIENT SOLVER ===");
    println!("Advanced oscillation control based on second derivative monitoring");
    
    // Test only controlled decay for cleaner output
    test_strategy(DampingStrategy::ControlledDecay, "CONTROLLED DECAY");
    
    println!("\n=== COMPARISON WITH OTHER METHODS ===");
    println!("Reference (Adaptive Thresholds): 0.49% error, 55.5ms");
    println!("Hybrid Two-Phase: 0.95% error, 1.7ms");
    println!("Newton-Raphson: 0.31% error, 0.6ms");
    
    println!("\n=== KEY INSIGHTS ===");
    println!("1. Second derivative sign changes trigger damping adjustments");
    println!("2. Immediate overdamping prevents oscillation but may be slower");
    println!("3. Controlled decay allows fast approach with managed oscillations");
    println!("4. Amplitude decay ratio indicates convergence quality");
}