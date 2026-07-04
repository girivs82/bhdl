/// Smart closed-loop ramping with gradient-based detection and recovery
/// Uses logarithmic gradient changes to detect transitions and adapt

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Element trait and implementations (compact version)
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

// Smart gradient tracker with transition detection
#[derive(Clone)]
struct SmartGradientTracker {
    ramp_history: VecDeque<f64>,
    current_history: VecDeque<f64>,      // Store actual current (not log)
    gradient_history: VecDeque<f64>,
    state_history: VecDeque<Vec<f64>>,  // Store node voltages for recovery
    low_activity_count: usize,
    last_gradient: Option<f64>,
    adaptive_threshold: f64,  // Starts at 1.5, can adjust
    max_observed_ratio: f64,  // Track the maximum ratio we've seen
    // PID controller for convergence (used throughout, not just terminal)
    pid_integral: f64,
    pid_last_error: f64,
    pid_kp: f64,  // Proportional gain
    pid_ki: f64,  // Integral gain
    pid_kd: f64,  // Derivative gain
    // Multiple tracking modes for optimal sensitivity
    log_gradient: f64,           // d(log(I))/d(ramp)
    linear_gradient: f64,        // dI/d(ramp)  
    first_integral_log: f64,     // ∫(d(log(I))/d(ramp))d(ramp)
    first_integral_linear: f64,  // ∫(dI/d(ramp))d(ramp) = I
    second_integral_log: f64,    // ∫∫(d(log(I))/d(ramp))d(ramp)²
    second_integral_linear: f64, // ∫∫(dI/d(ramp))d(ramp)²
    active_mode: TrackingMode,   // Which mode we're using
    mode_locked: bool,           // Once we switch modes, lock it
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum TrackingMode {
    LogGradient,
    LinearGradient,
    FirstIntegralLog,
    FirstIntegralLinear,
    SecondIntegralLog,
    SecondIntegralLinear,
}

impl SmartGradientTracker {
    fn new() -> Self {
        Self {
            ramp_history: VecDeque::with_capacity(10),
            current_history: VecDeque::with_capacity(10),
            gradient_history: VecDeque::with_capacity(5),
            state_history: VecDeque::with_capacity(10),
            low_activity_count: 0,
            last_gradient: None,
            adaptive_threshold: 1.5,  // Start with low threshold
            max_observed_ratio: 1.0,
            // PID controller parameters - tuned for continuous use
            pid_integral: 0.0,
            pid_last_error: 0.0,
            pid_kp: 0.3,   // Moderate proportional gain
            pid_ki: 0.05,  // Small integral gain to avoid overshoot
            pid_kd: 0.02,  // Small derivative gain for stability
            // Initialize all tracking variables
            log_gradient: 0.0,
            linear_gradient: 0.0,
            first_integral_log: 0.0,
            first_integral_linear: 0.0,
            second_integral_log: 0.0,
            second_integral_linear: 0.0,
            active_mode: TrackingMode::LogGradient,  // Start with log gradient
            mode_locked: false,
        }
    }
    
    fn add_point(&mut self, ramp: f64, current: f64, node_state: Vec<f64>) {
        self.ramp_history.push_back(ramp);
        self.current_history.push_back(current);
        self.state_history.push_back(node_state);
        
        if self.ramp_history.len() > 10 {
            self.ramp_history.pop_front();
            self.current_history.pop_front();
            self.state_history.pop_front();
        }
        
        // Calculate all gradients and integrals
        if self.ramp_history.len() >= 2 {
            let n = self.ramp_history.len();
            let dramp = self.ramp_history[n-1] - self.ramp_history[n-2];
            let i1 = self.current_history[n-1];
            let i2 = self.current_history[n-2];
            let di = i1 - i2;
            
            if dramp > 1e-9 && i1 > 1e-18 && i2 > 1e-18 {
                // Calculate log gradient
                let log_i1 = i1.ln();
                let log_i2 = i2.ln();
                self.log_gradient = (log_i1 - log_i2) / dramp;
                
                // Calculate linear gradient
                self.linear_gradient = di / dramp;
                
                // Update first integrals (trapezoidal rule)
                self.first_integral_log += self.log_gradient * dramp;
                self.first_integral_linear += self.linear_gradient * dramp;
                
                // Update second integrals
                self.second_integral_log += self.first_integral_log * dramp;
                self.second_integral_linear += self.first_integral_linear * dramp;
                
                // Choose the best tracking mode based on sensitivity and stability
                self.choose_best_tracking_mode(ramp);
                
                // Get the active gradient for history tracking
                let active_gradient = match self.active_mode {
                    TrackingMode::LogGradient => self.log_gradient,
                    TrackingMode::LinearGradient => self.linear_gradient,
                    TrackingMode::FirstIntegralLog => self.first_integral_log,
                    TrackingMode::FirstIntegralLinear => self.first_integral_linear,
                    TrackingMode::SecondIntegralLog => self.second_integral_log,
                    TrackingMode::SecondIntegralLinear => self.second_integral_linear,
                };
                
                self.gradient_history.push_back(active_gradient);
                if self.gradient_history.len() > 5 {
                    self.gradient_history.pop_front();
                }
                
                // Detect low activity based on mode
                let activity_threshold = match self.active_mode {
                    TrackingMode::LogGradient => 5.0,
                    TrackingMode::LinearGradient => 1e-10,
                    TrackingMode::FirstIntegralLog => 0.1,
                    TrackingMode::FirstIntegralLinear => 1e-8,
                    TrackingMode::SecondIntegralLog => 1.0,  // Higher threshold for second integral
                    TrackingMode::SecondIntegralLinear => 1e-6,
                };
                
                if active_gradient.abs() < activity_threshold {
                    self.low_activity_count += 1;
                } else {
                    self.low_activity_count = 0;
                }
                
                self.last_gradient = Some(active_gradient);
            }
        }
    }
    
    fn choose_best_tracking_mode(&mut self, ramp: f64) {
        // Only choose mode once - don't keep switching
        if self.mode_locked {
            return;
        }
        
        // Check if we have low sensitivity with log gradient
        if self.log_gradient.abs() < 20.0 && ramp > 0.4 {
            // We detected low sensitivity - choose based on how low
            let log_sens = self.log_gradient.abs();
            
            let best_mode = if log_sens < 5.0 {
                // Ultra-low sensitivity (like High Vt case) - need second integral
                TrackingMode::SecondIntegralLog
            } else if log_sens < 10.0 {
                // Moderate low sensitivity - first integral should work
                TrackingMode::FirstIntegralLog
            } else {
                // Mild low sensitivity - linear gradient might be enough
                TrackingMode::LinearGradient
            };
            
            // Get the value for debugging
            let value = match best_mode {
                TrackingMode::LinearGradient => self.linear_gradient.abs(),
                TrackingMode::FirstIntegralLog => self.first_integral_log.abs(),
                TrackingMode::SecondIntegralLog => self.second_integral_log.abs(),
                _ => 0.0,
            };
            
            println!("  → Low sensitivity detected (log_grad={:.1}) at ramp={:.3}, switching to {:?} mode (value={:.2e})", 
                     log_sens, ramp, best_mode, value);
            self.active_mode = best_mode;
            self.mode_locked = true; // Lock the mode - no more switching
        }
    }
    
    fn detect_transition(&mut self) -> Option<(bool, f64)> {
        if self.gradient_history.len() < 3 {
            return None;
        }
        
        let n = self.gradient_history.len();
        let current_gradient = self.gradient_history[n-1];
        let prev_gradient = self.gradient_history[n-2];
        let prev_prev_gradient = self.gradient_history[n-3];
        
        // Calculate gradient ratio and absolute change
        let gradient_ratio = current_gradient.abs() / (prev_gradient.abs() + 1e-10);
        let gradient_change = (current_gradient - prev_gradient).abs();
        
        // Also look at gradient acceleration (second derivative)
        let prev_change = (prev_gradient - prev_prev_gradient).abs();
        let accel_ratio = if prev_change > 1e-10 {
            gradient_change / prev_change
        } else {
            1.0
        };
        
        // Update max observed ratio
        if gradient_ratio > self.max_observed_ratio {
            self.max_observed_ratio = gradient_ratio;
        }
        
        // Multiple detection criteria:
        // 1. Gradient ratio exceeds threshold (adaptive)
        // 2. Gradient change is significant
        // 3. Acceleration ratio is high (>2.0) - but not in terminal phase
        // Adjust thresholds based on tracking mode
        let (ratio_threshold, change_threshold) = match self.active_mode {
            TrackingMode::LogGradient => (self.adaptive_threshold, 10.0),
            TrackingMode::LinearGradient => (10.0, 1e-10),
            TrackingMode::FirstIntegralLog => (5.0, 1.0),
            TrackingMode::FirstIntegralLinear => (10.0, 1e-8),
            TrackingMode::SecondIntegralLog => (3.0, 10.0),  // More sensitive thresholds
            TrackingMode::SecondIntegralLinear => (10.0, 1e-6),
        };
        
        let ratio_trigger = gradient_ratio > ratio_threshold;
        let change_trigger = gradient_change > change_threshold;
        let accel_trigger = accel_ratio > 2.0 && 
                           matches!(self.active_mode, TrackingMode::LogGradient);
        
        if ratio_trigger || change_trigger || accel_trigger {
            println!("  → Transition detected! ratio={:.2}x, change={:.2e}, accel={:.1}x", 
                     gradient_ratio, gradient_change, accel_ratio);
            println!("    Triggers: ratio={}, change={}, accel={} (mode={:?})", 
                     ratio_trigger, change_trigger, accel_trigger, self.active_mode);
            Some((true, gradient_ratio))
        } else {
            Some((false, gradient_ratio))
        }
    }
    
    fn should_accelerate(&self) -> bool {
        // Accelerate if we've seen consistent low activity
        self.low_activity_count >= 3
    }
    
    fn get_recovery_state(&self, steps_back: usize) -> Option<(f64, Vec<f64>)> {
        let n = self.state_history.len();
        if n > steps_back {
            let idx = n - steps_back - 1;
            Some((self.ramp_history[idx], self.state_history[idx].clone()))
        } else {
            None
        }
    }
    
    fn compute_pid_ramp_adjustment(&mut self, error: f64, ramp_factor: f64) -> f64 {
        // PID controller to determine optimal ramp rate based on convergence error
        // Target: achieve 1e-15 error smoothly without overshooting
        
        // Scale error based on how close we are to target
        let target_error = 1e-15;
        let error_ratio = (error / target_error).ln().max(-10.0).min(10.0);
        
        // P term: proportional to log error ratio
        let p_term = self.pid_kp * error_ratio;
        
        // I term: accumulate error over time
        self.pid_integral += error_ratio * 0.01;  // Small integration step
        self.pid_integral = self.pid_integral.max(-10.0).min(10.0);  // Prevent windup
        let i_term = self.pid_ki * self.pid_integral;
        
        // D term: rate of change of error
        let d_term = self.pid_kd * (error_ratio - self.pid_last_error);
        self.pid_last_error = error_ratio;
        
        // Combined PID output (positive means we can go faster)
        let pid_output = -(p_term + i_term + d_term);
        
        // Convert to ramp rate multiplier (1.0 = nominal, >1 = faster, <1 = slower)
        let rate_multiplier = (1.0 + pid_output * 0.1).max(0.1).min(3.0);
        
        // Near the end, be more conservative
        if ramp_factor > 0.95 {
            rate_multiplier.min(1.5)
        } else {
            rate_multiplier
        }
    }
}

// Smart closed-loop solver
pub struct SmartClosedLoopSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    nonlinear_elements: Vec<usize>,
    gradient_tracker: SmartGradientTracker,
}

impl SmartClosedLoopSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            nonlinear_elements: Vec::new(),
            gradient_tracker: SmartGradientTracker::new(),
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
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve_smart(&mut self) -> (Vec<f64>, f64, usize) {
        let start = Instant::now();
        
        // Setup voltage sources
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
        
        // Smart ramping with gradient-based control
        let mut ramp_factor = 0.0;
        let mut ramp_rate: f64 = 0.01;
        let mut in_careful_mode = false;
        let mut last_ramp_voltage = 0.0;  // Track voltage for oscillation detection
        
        while ramp_factor < 1.0 - 1e-9 {  // Small tolerance to ensure we reach 1.0
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve at current ramp
            let (converged, iters, convergence_error) = self.solve_to_convergence(&mut total_iterations);
            
            if converged && !self.nonlinear_elements.is_empty() {
                // Get device state
                let elem_idx = self.nonlinear_elements[0];
                let mut element_voltage = 0.0;
                
                for &(conn_elem, pos, neg) in &self.connections {
                    if conn_elem == elem_idx {
                        element_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                
                let current = self.elements[elem_idx].current_at_voltage(element_voltage);
                
                // Track gradient with state - pass actual current
                self.gradient_tracker.add_point(ramp_factor, current, self.node_voltages.clone());
                
                // Check for transition
                if let Some((transition_detected, ratio)) = self.gradient_tracker.detect_transition() {
                    // Show more detailed info at key points
                    let show_debug = ramp_factor < 0.05 || 
                                   (ramp_factor > 0.05 && (ramp_factor * 20.0) as i32 % 2 == 0) ||
                                   transition_detected;
                    
                    if show_debug && self.gradient_tracker.gradient_history.len() >= 2 {
                        let grads = &self.gradient_tracker.gradient_history;
                        let n = grads.len();
                        println!("  Ramp {:.3}: grad={:.1}, prev={:.1}, ratio={:.2}x, V={:.3}V, I={:.2e}A", 
                                ramp_factor, 
                                grads[n-1], 
                                grads[n-2], 
                                ratio,
                                element_voltage,
                                current);
                    }
                    
                    if transition_detected && !in_careful_mode {
                        // BACK OFF! Gradient jumped suddenly
                        if let Some((prev_ramp, prev_state)) = self.gradient_tracker.get_recovery_state(2) {
                            println!("  → Backing off to ramp={:.3} and entering careful mode", prev_ramp);
                            ramp_factor = prev_ramp;
                            self.node_voltages = prev_state;
                            ramp_rate = 0.001;  // Much slower now
                            in_careful_mode = true;
                            continue;
                        }
                    }
                }
                
                // Use PID throughout to control ramp rate based on convergence error
                let error = convergence_error;
                
                // Get PID-based rate multiplier
                let pid_multiplier = self.gradient_tracker.compute_pid_ramp_adjustment(error, ramp_factor);
                
                // Apply PID adjustment to current ramp rate
                let old_rate = ramp_rate;
                ramp_rate *= pid_multiplier;
                
                // Show PID action when error is significant or near the end
                if (error > 1e-10 || ramp_factor > 0.95) && total_iterations % 10 == 0 {
                    println!("  → PID control: error={:.2e}, multiplier={:.2}x, rate: {:.2e} -> {:.2e}", 
                             error, pid_multiplier, old_rate, ramp_rate);
                }
                
                // If we're very close to target error and near 100%, we can finish
                if error < 1e-15 && ramp_factor > 0.99 {
                    println!("  → Target convergence achieved! error={:.2e}", error);
                    ramp_factor = 1.0;
                    break;
                }
                
                last_ramp_voltage = element_voltage;
                
                // Additional rate adjustments based on mode and activity
                // (PID has already made primary adjustment)
                if matches!(self.gradient_tracker.active_mode, 
                    TrackingMode::LinearGradient | TrackingMode::FirstIntegralLinear | 
                    TrackingMode::SecondIntegralLinear | TrackingMode::FirstIntegralLog |
                    TrackingMode::SecondIntegralLog) {
                    // Using integral or linear mode - indicates low sensitivity
                    if self.gradient_tracker.should_accelerate() && ramp_factor < 0.9 {
                        println!("  → Low sensitivity + low activity, boosting rate");
                        ramp_rate *= 1.2;  // Boost on top of PID
                    }
                } else if !in_careful_mode && self.gradient_tracker.should_accelerate() && ramp_factor < 0.8 {
                    println!("  → Low activity detected, boosting rate");
                    ramp_rate *= 1.5;  // Boost on top of PID
                }
                
                // Apply bounds based on tracking mode
                let max_rate = match self.gradient_tracker.active_mode {
                    TrackingMode::LogGradient => if in_careful_mode { 0.01 } else { 0.05 },
                    TrackingMode::LinearGradient => 0.04,
                    TrackingMode::FirstIntegralLog => 0.04,
                    TrackingMode::FirstIntegralLinear => 0.04,
                    TrackingMode::SecondIntegralLog => 0.05,
                    TrackingMode::SecondIntegralLinear => 0.04,
                };
                
                ramp_rate = ramp_rate.min(max_rate).max(1e-6);
                
                ramp_factor += ramp_rate;
                ramp_factor = ramp_factor.min(1.0);
                
            } else {
                // Failed to converge
                ramp_rate *= 0.5;
                if ramp_rate < 0.0001 {
                    // Force progress
                    ramp_factor += 0.0001;
                }
            }
        }
        
        // Make sure we're at exactly 100%
        if ramp_factor < 1.0 {
            println!("  → Completing ramp to 100% (was at {:.3})", ramp_factor);
            ramp_factor = 1.0;
        }
        
        // Final solve at 100% with extra precision
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Do multiple convergence attempts for best accuracy
        let (converged, iters, final_error) = self.solve_to_convergence(&mut total_iterations);
        
        if !converged {
            // Try harder with tighter tolerance
            let (extra_converged, extra_iters) = self.solve_to_convergence_tight(&mut total_iterations);
        }
        
        // One more solve with extra tight tolerance for best accuracy
        let (final_converged, final_iters, final_final_error) = self.solve_to_convergence_extra_tight(&mut total_iterations);
        
        // Special handling for cases that need extra convergence
        if final_final_error > 1e-15 {
            println!("  → Final push for ultra-tight convergence...");
            for _ in 0..10 {  // More attempts for difficult cases
                let (ultra_converged, ultra_iters, ultra_error) = self.solve_to_convergence_extra_tight(&mut total_iterations);
                if ultra_error < 1e-16 {
                    break;
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (self.node_voltages.clone(), elapsed, total_iterations)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize, f64) {
        let max_iter = 30;
        let tol = 1e-12;
        let mut iterations = 0;
        let mut last_error = 0.0;
        
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
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                last_error = max_change;
                
                if max_change < tol {
                    return (true, iterations, last_error);
                }
            } else {
                return (false, iterations, last_error);
            }
        }
        
        (false, iterations, last_error)
    }
    
    fn solve_to_convergence_tight(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 50;
        let tol = 1e-15;  // Much tighter tolerance
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Very conservative damping for tight convergence
                let damping = 0.4;
                
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
    
    fn solve_to_convergence_extra_tight(&mut self, total_iterations: &mut usize) -> (bool, usize, f64) {
        let max_iter = 200;  // More iterations for difficult cases
        let tol = 1e-16;  // Ultra-tight tolerance
        let mut iterations = 0;
        let mut last_error = 0.0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Adaptive damping - more aggressive as we converge
                let damping = if max_change < 1e-10 {
                    0.5  // Can be more aggressive when close
                } else {
                    0.3  // Conservative when far
                };
                
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
                
                last_error = max_change;
                
                if max_change < tol {
                    return (true, iterations, last_error);
                }
            } else {
                return (false, iterations, last_error);
            }
        }
        
        (false, iterations, last_error)
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
    println!("=== SMART CLOSED-LOOP RAMPING ===");
    println!("Uses gradient changes to detect transitions");
    println!("Backs off and slows down when gradient jumps\n");
    
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
        println!("\nTest: {}", name);
        let mut solver = SmartClosedLoopSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r_idx = solver.add_element(Box::new(Resistor::new(rs)));
        let d_idx = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_smart();
        
        let (vd_ref, id_ref) = analytical_reference(vs, rs, is, vt);
        let vd_computed = voltages[2];
        let id_computed = (voltages[1] - voltages[2]) / rs;
        
        let v_err = ((vd_computed - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_computed - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        println!("Result: Vd={:.6}V (ref={:.6}V), Error={:.3}%, Time={:.1}ms, Iter={}", 
                 vd_computed, vd_ref, max_err, time, iterations);
        
        total_error += max_err;
        total_time += time;
        count += 1;
    }
    
    let avg_error = total_error / count as f64;
    let avg_time = total_time / count as f64;
    
    println!("\n=== SMART CLOSED-LOOP RESULTS ===");
    println!("Average error: {:.2}%", avg_error);
    println!("Average time: {:.1}ms", avg_time);
    println!("\nPaper reference: 3.55% error, 21.5ms");
    
    if avg_error < 1.0 {
        println!("\n🎯 ACHIEVED SUB-1% ERROR!");
        println!("   Accuracy: {:.1}x better than paper", 3.55 / avg_error);
        println!("   Speed: {:.1}x faster than paper", 21.5 / avg_time);
    } else if avg_error < 3.55 && avg_time < 21.5 {
        println!("\n✅ SUCCESS: Smart closed-loop beats paper reference!");
        println!("   Accuracy: {:.1}x better", 3.55 / avg_error);
        println!("   Speed: {:.1}x faster", 21.5 / avg_time);
    } else if avg_error < 3.55 || avg_time < 21.5 {
        println!("\n🔄 PARTIAL SUCCESS: One metric improved");
    } else {
        println!("\n📊 Results show room for further optimization");
    }
    
    println!("\n🎯 KEY MECHANISM:");
    println!("1. Monitor d(log(I))/d(ramp) continuously");
    println!("2. Detect sudden gradient changes (>5x jump)");
    println!("3. Back off to previous state when transition detected");
    println!("4. Switch to careful mode with slow ramping");
    println!("5. Accelerate when gradient is consistently low");
}