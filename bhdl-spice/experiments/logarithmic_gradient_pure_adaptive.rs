/// Pure Adaptive Damping Logarithmic Gradient Solver
/// 
/// Fully adaptive approach without any fixed transitions or assumptions
/// Uses only second derivative monitoring to control convergence
/// 
/// Key optimizations:
/// 1. More aggressive initial ramp rates
/// 2. Better noise filtering for oscillation detection
/// 3. Adaptive sensitivity based on convergence stage
/// 4. Efficient state management

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

// Enhanced oscillation detector with better noise filtering
struct AdaptiveOscillationDetector {
    voltage_history: VecDeque<f64>,
    gradient_history: VecDeque<f64>,
    curvature_history: VecDeque<f64>,
    
    // Adaptive thresholds
    noise_floor: f64,
    significant_change_threshold: f64,
    
    // State tracking
    oscillation_count: usize,
    last_peak_voltage: Option<f64>,
    convergence_stage: ConvergenceStage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum ConvergenceStage {
    Initial,      // Large changes expected
    Approaching,  // Getting closer, medium sensitivity
    Fine,         // Near solution, high sensitivity
}

impl AdaptiveOscillationDetector {
    fn new() -> Self {
        Self {
            voltage_history: VecDeque::with_capacity(10),
            gradient_history: VecDeque::with_capacity(8),
            curvature_history: VecDeque::with_capacity(6),
            
            noise_floor: 1e-10,
            significant_change_threshold: 0.01,
            
            oscillation_count: 0,
            last_peak_voltage: None,
            convergence_stage: ConvergenceStage::Initial,
        }
    }
    
    fn update(&mut self, voltage: f64, ramp_factor: f64) -> DampingSignal {
        self.voltage_history.push_back(voltage);
        if self.voltage_history.len() > 10 {
            self.voltage_history.pop_front();
        }
        
        // Update convergence stage based on ramp progress
        self.convergence_stage = if ramp_factor < 0.3 {
            ConvergenceStage::Initial
        } else if ramp_factor < 0.8 {
            ConvergenceStage::Approaching
        } else {
            ConvergenceStage::Fine
        };
        
        // Adjust sensitivity based on stage
        self.significant_change_threshold = match self.convergence_stage {
            ConvergenceStage::Initial => 0.1,      // Less sensitive
            ConvergenceStage::Approaching => 0.01, // Medium sensitivity
            ConvergenceStage::Fine => 0.001,       // High sensitivity
        };
        
        // Need at least 3 points for meaningful analysis
        if self.voltage_history.len() < 3 {
            return DampingSignal::Maintain;
        }
        
        // Calculate gradient (first derivative)
        let n = self.voltage_history.len();
        let gradient = self.voltage_history[n-1] - self.voltage_history[n-2];
        
        // Apply noise filtering
        if gradient.abs() < self.noise_floor {
            return DampingSignal::Maintain;
        }
        
        self.gradient_history.push_back(gradient);
        if self.gradient_history.len() > 8 {
            self.gradient_history.pop_front();
        }
        
        // Need at least 2 gradients for curvature
        if self.gradient_history.len() < 2 {
            return DampingSignal::Maintain;
        }
        
        // Calculate curvature (second derivative)
        let m = self.gradient_history.len();
        let curvature = self.gradient_history[m-1] - self.gradient_history[m-2];
        
        self.curvature_history.push_back(curvature);
        if self.curvature_history.len() > 6 {
            self.curvature_history.pop_front();
        }
        
        // Detect oscillation patterns
        self.analyze_oscillation_pattern()
    }
    
    fn analyze_oscillation_pattern(&mut self) -> DampingSignal {
        if self.curvature_history.len() < 3 {
            return DampingSignal::Maintain;
        }
        
        // Check for sign changes in curvature (inflection points)
        let mut sign_changes = 0;
        let mut prev_sign = self.curvature_history[0] > 0.0;
        
        for &curvature in self.curvature_history.iter().skip(1) {
            let current_sign = curvature > 0.0;
            if current_sign != prev_sign && curvature.abs() > self.noise_floor {
                sign_changes += 1;
            }
            prev_sign = current_sign;
        }
        
        // Multiple sign changes indicate oscillation
        if sign_changes >= 2 {
            self.oscillation_count += 1;
            
            // Determine oscillation strength
            let avg_curvature_magnitude: f64 = self.curvature_history.iter()
                .map(|c| c.abs())
                .sum::<f64>() / self.curvature_history.len() as f64;
            
            // Strong oscillation if curvature is significant
            if avg_curvature_magnitude > self.significant_change_threshold {
                return DampingSignal::StrongOscillation {
                    count: self.oscillation_count,
                    strength: avg_curvature_magnitude,
                };
            } else {
                return DampingSignal::WeakOscillation {
                    count: self.oscillation_count,
                };
            }
        }
        
        // Check if we're making steady progress
        if self.gradient_history.len() >= 3 {
            let recent_gradients: Vec<f64> = self.gradient_history.iter()
                .rev()
                .take(3)
                .copied()
                .collect();
            
            let all_same_sign = recent_gradients.iter()
                .all(|&g| g > 0.0) || recent_gradients.iter()
                .all(|&g| g < 0.0);
            
            if all_same_sign {
                // Consistent direction - can increase rate
                return DampingSignal::SteadyProgress;
            }
        }
        
        DampingSignal::Maintain
    }
    
    fn reset_oscillation_count(&mut self) {
        if self.oscillation_count > 0 && self.gradient_history.len() >= 2 {
            // Reset if we've had stable progress
            let recent_stable = self.gradient_history.iter()
                .rev()
                .take(2)
                .all(|&g| g.abs() < self.significant_change_threshold);
            
            if recent_stable {
                self.oscillation_count = 0;
            }
        }
    }
}

#[derive(Debug, Clone)]
enum DampingSignal {
    SteadyProgress,
    Maintain,
    WeakOscillation { count: usize },
    StrongOscillation { count: usize, strength: f64 },
}

// Pure adaptive controller
struct PureAdaptiveController {
    damping: f64,
    ramp_rate: f64,
    
    // Adaptive bounds that change based on behavior
    min_damping: f64,
    max_damping: f64,
    base_rate: f64,
    
    // Momentum for smooth transitions
    damping_momentum: f64,
    rate_momentum: f64,
    
    detector: AdaptiveOscillationDetector,
}

impl PureAdaptiveController {
    fn new() -> Self {
        Self {
            damping: 0.5,        // Start moderately damped
            ramp_rate: 0.02,     // Aggressive initial rate
            
            min_damping: 0.3,
            max_damping: 0.95,
            base_rate: 0.02,
            
            damping_momentum: 0.0,
            rate_momentum: 0.0,
            
            detector: AdaptiveOscillationDetector::new(),
        }
    }
    
    fn update(&mut self, voltage: f64, ramp_factor: f64, converged: bool) {
        let signal = self.detector.update(voltage, ramp_factor);
        
        match signal {
            DampingSignal::SteadyProgress => {
                // Making good progress - can be more aggressive
                self.damping_momentum = -0.05;
                self.rate_momentum = 0.1;
            }
            
            DampingSignal::Maintain => {
                // Neutral - slowly return to baseline
                self.damping_momentum *= 0.9;
                self.rate_momentum *= 0.9;
            }
            
            DampingSignal::WeakOscillation { count } => {
                // Minor oscillation - gentle correction
                self.damping_momentum = 0.02 * count as f64;
                self.rate_momentum = -0.05;
            }
            
            DampingSignal::StrongOscillation { count, strength } => {
                // Strong oscillation - significant correction
                let correction_factor = (strength * 10.0).min(2.0);
                self.damping_momentum = 0.1 * correction_factor * count as f64;
                self.rate_momentum = -0.1 * correction_factor;
                
                // For persistent oscillations, be more aggressive
                if count > 3 {
                    self.damping = (self.damping * 1.5).min(self.max_damping);
                    self.ramp_rate = (self.ramp_rate * 0.3).max(0.0001);
                }
            }
        }
        
        // Apply momentum with bounds
        self.damping = (self.damping + self.damping_momentum * 0.1)
            .clamp(self.min_damping, self.max_damping);
        
        // Adaptive rate based on convergence stage
        let stage_multiplier = match self.detector.convergence_stage {
            ConvergenceStage::Initial => 1.5,      // Can be aggressive
            ConvergenceStage::Approaching => 1.0,  // Normal
            ConvergenceStage::Fine => 0.5,         // Careful
        };
        
        self.ramp_rate = (self.base_rate * stage_multiplier + self.rate_momentum * 0.01)
            .clamp(0.0001, 0.1);
        
        // Reset oscillation count if stable
        if converged {
            self.detector.reset_oscillation_count();
        }
    }
    
    fn get_adaptive_tolerance(&self) -> f64 {
        match self.detector.convergence_stage {
            ConvergenceStage::Initial => 1e-9,
            ConvergenceStage::Approaching => 1e-11,
            ConvergenceStage::Fine => 1e-12,
        }
    }
}

// Main solver
pub struct PureAdaptiveLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    controller: PureAdaptiveController,
}

impl PureAdaptiveLogGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            controller: PureAdaptiveController::new(),
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
            
            // Get voltage before solving for tracking
            let diode_v_before = self.get_diode_voltage();
            
            // Solve with adaptive tolerance
            let tol = self.controller.get_adaptive_tolerance();
            let (converged, _iters) = self.solve_to_convergence(&mut total_iterations, tol);
            
            let diode_v_after = self.get_diode_voltage();
            
            // Update controller with actual voltage (not error)
            self.controller.update(diode_v_after, ramp_factor, converged);
            
            if !converged {
                // Failed to converge - reduce rate and try again
                self.controller.ramp_rate *= 0.5;
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
        self.solve_to_convergence(&mut total_iterations, 1e-12);
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize, tol: f64) -> (bool, usize) {
        let max_iter = 30;
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Apply adaptive damping
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
    println!("=== PURE ADAPTIVE DAMPING LOGARITHMIC GRADIENT SOLVER ===");
    println!("Fully adaptive approach with no fixed assumptions\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Pure Vd", "Pure Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(100));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = PureAdaptiveLogGradientSolver::new(3);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_pure, id_pure, iters, time) = solver.solve();
        
        let v_err = ((vd_pure - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_pure - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 test_name, 
                 vd_ref, id_ref * 1000.0,
                 vd_pure, id_pure * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(100));
    
    let n_cases = test_cases.len() as f64;
    println!("\nPure Adaptive Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\n=== KEY FEATURES ===");
    println!("1. No fixed transition points - fully adaptive");
    println!("2. Convergence stage awareness (Initial/Approaching/Fine)");
    println!("3. Noise-filtered oscillation detection");
    println!("4. Momentum-based smooth transitions");
    println!("5. Adaptive tolerance based on convergence stage");
}