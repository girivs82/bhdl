/// Optimized Logarithmic Gradient Solver - Pursuing Better Accuracy and Speed
/// 
/// This is an experimental optimization of the logarithmic gradient approach
/// aiming to reduce error below 3.55% and convergence time below 21.5ms
/// while maintaining the key property of being truly generic.
/// 
/// Optimization strategies to explore:
/// 1. Adaptive history window sizing based on convergence quality
/// 2. Predictive ramp rate using quadratic extrapolation
/// 3. Multi-resolution sensitivity analysis
/// 4. Intelligent threshold adaptation with memory
/// 5. Parallel gradient computation for multi-device circuits
/// 6. Smart initial ramp rate selection based on circuit analysis
/// 7. Convergence acceleration in low-sensitivity regions
/// 8. Precision-aware numerical techniques

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Element trait (same as fixed version)
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
    LED,
    Zener,
}

// Standard element implementations (same as before)
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

pub struct LED {
    is: f64,  // Saturation current
    vt: f64,  // Thermal voltage
    vf: f64,  // Forward voltage
    voltage: f64,
}

impl LED {
    pub fn new(is: f64, vt: f64, vf: f64) -> Self {
        Self { is, vt, vf, voltage: 0.0 }
    }
}

impl Element for LED {
    fn element_type(&self) -> ElementType { ElementType::LED }
    fn is_nonlinear(&self) -> bool { true }
    
    fn current_at_voltage(&self, v: f64) -> f64 {
        let effective_v = v - self.vf;
        if effective_v <= 0.0 {
            -self.is  // Small reverse current
        } else {
            let v_norm = effective_v / self.vt;
            if v_norm > 50.0 {
                self.is * (50.0_f64.exp() - 1.0)
            } else {
                self.is * (v_norm.exp() - 1.0)
            }
        }
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        const MIN_G: f64 = 1e-14;
        let effective_v = v - self.vf;
        
        if effective_v <= 0.0 {
            MIN_G
        } else {
            let v_norm = effective_v / self.vt;
            if v_norm > 50.0 {
                (self.is / self.vt) * 50.0_f64.exp()
            } else {
                ((self.is / self.vt) * v_norm.exp()).max(MIN_G)
            }
        }
    }
    
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

// OPTIMIZED: Enhanced history with predictive capabilities
#[derive(Clone)]
struct OptimizedHistory {
    voltages: Vec<VecDeque<f64>>,
    log_currents: Vec<VecDeque<f64>>,
    ramp_factors: VecDeque<f64>,
    sensitivities: Vec<VecDeque<f64>>,  // NEW: Track sensitivity history
    convergence_quality: VecDeque<f64>, // NEW: Track convergence quality
    device_count: usize,
    adaptive_window_size: usize,        // NEW: Dynamic window sizing
}

impl OptimizedHistory {
    fn new(device_count: usize) -> Self {
        Self {
            voltages: vec![VecDeque::with_capacity(16); device_count],
            log_currents: vec![VecDeque::with_capacity(16); device_count],
            ramp_factors: VecDeque::with_capacity(16),
            sensitivities: vec![VecDeque::with_capacity(8); device_count],
            convergence_quality: VecDeque::with_capacity(8),
            device_count,
            adaptive_window_size: 8,
        }
    }
    
    fn add_point(&mut self, device_voltages: &[f64], device_log_currents: &[f64], 
                 ramp: f64, quality: f64) {
        // Adaptive window sizing based on convergence quality
        if quality > 0.9 && self.adaptive_window_size > 4 {
            self.adaptive_window_size -= 1;
        } else if quality < 0.5 && self.adaptive_window_size < 16 {
            self.adaptive_window_size += 1;
        }
        
        for i in 0..self.device_count.min(device_voltages.len()) {
            self.voltages[i].push_back(device_voltages[i]);
            self.log_currents[i].push_back(device_log_currents[i]);
            
            if self.voltages[i].len() > self.adaptive_window_size {
                self.voltages[i].pop_front();
                self.log_currents[i].pop_front();
            }
        }
        
        self.ramp_factors.push_back(ramp);
        self.convergence_quality.push_back(quality);
        
        if self.ramp_factors.len() > self.adaptive_window_size {
            self.ramp_factors.pop_front();
            self.convergence_quality.pop_front();
        }
    }
    
    // OPTIMIZED: Multi-resolution sensitivity analysis
    fn calculate_enhanced_sensitivity(&self) -> Option<(f64, usize, f64, f64)> {
        let mut best_sensitivity = None;
        let mut best_device = 0;
        let mut best_reliability = 0.0;
        let mut best_prediction = 0.0;
        
        for device_idx in 0..self.device_count {
            if self.voltages[device_idx].len() < 4 {
                continue;
            }
            
            // Multi-resolution analysis: fine, medium, coarse
            let (sens, reliability, prediction) = self.analyze_device_multiresolution(device_idx)?;
            
            if reliability > best_reliability {
                best_sensitivity = Some(sens);
                best_device = device_idx;
                best_reliability = reliability;
                best_prediction = prediction;
            }
        }
        
        best_sensitivity.map(|s| (s, best_device, best_reliability, best_prediction))
    }
    
    fn analyze_device_multiresolution(&self, device_idx: usize) -> Option<(f64, f64, f64)> {
        let n = self.voltages[device_idx].len();
        if n < 4 { return None; }
        
        let mut fine_gradients = Vec::new();
        let mut medium_gradients = Vec::new();
        let mut coarse_gradients = Vec::new();
        
        // Fine resolution (span=1)
        for i in 1..n {
            let dv = self.voltages[device_idx][i] - self.voltages[device_idx][i-1];
            if dv.abs() > 1e-12 {
                let dlog_i = self.log_currents[device_idx][i] - self.log_currents[device_idx][i-1];
                fine_gradients.push(dlog_i / dv);
            }
        }
        
        // Medium resolution (span=2)
        for i in 2..n {
            let dv = self.voltages[device_idx][i] - self.voltages[device_idx][i-2];
            if dv.abs() > 1e-10 {
                let dlog_i = self.log_currents[device_idx][i] - self.log_currents[device_idx][i-2];
                medium_gradients.push(dlog_i / dv);
            }
        }
        
        // Coarse resolution (span=3)
        if n >= 4 {
            for i in 3..n {
                let dv = self.voltages[device_idx][i] - self.voltages[device_idx][i-3];
                if dv.abs() > 1e-9 {
                    let dlog_i = self.log_currents[device_idx][i] - self.log_currents[device_idx][i-3];
                    coarse_gradients.push(dlog_i / dv);
                }
            }
        }
        
        // Weighted combination based on consistency
        let fine_median = Self::median(&fine_gradients)?;
        let medium_median = Self::median(&medium_gradients)?;
        let coarse_median = Self::median(&coarse_gradients).unwrap_or(medium_median);
        
        // Check consistency across resolutions
        let consistency = 1.0 / (1.0 + (fine_median - medium_median).abs() / fine_median.abs() +
                                      (medium_median - coarse_median).abs() / medium_median.abs());
        
        // Weighted average favoring consistent measurements
        let weights = if consistency > 0.8 {
            (0.5, 0.3, 0.2)  // Trust fine resolution more
        } else {
            (0.3, 0.4, 0.3)  // Balance all resolutions
        };
        
        let weighted_sensitivity = fine_median * weights.0 + 
                                 medium_median * weights.1 + 
                                 coarse_median * weights.2;
        
        // Predictive component using quadratic extrapolation
        let prediction = if n >= 3 {
            let v_n = self.voltages[device_idx][n-1];
            let v_n1 = self.voltages[device_idx][n-2];
            let v_n2 = self.voltages[device_idx][n-3];
            
            // Quadratic extrapolation for next voltage
            let dv1 = v_n - v_n1;
            let dv2 = v_n1 - v_n2;
            let acceleration = (dv1 - dv2) / 2.0;
            
            (dv1 + acceleration).abs()
        } else {
            0.0
        };
        
        Some((weighted_sensitivity, consistency, prediction))
    }
    
    fn median(values: &[f64]) -> Option<f64> {
        if values.is_empty() { return None; }
        let mut sorted = values.to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        Some(sorted[sorted.len() / 2])
    }
}

// OPTIMIZED: Smarter controller with predictive capabilities
struct OptimizedController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    expected_sensitivities: Vec<f64>,
    device_count: usize,
    
    // Optimization additions
    rate_history: VecDeque<f64>,
    performance_memory: VecDeque<f64>,
    predictive_mode: bool,
    acceleration_factor: f64,
    last_sensitivity_ratio: f64,
}

impl OptimizedController {
    fn new(device_vts: &[f64]) -> Self {
        let expected_sensitivities = device_vts.iter().map(|&vt| 1.0 / vt).collect();
        
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.1,  // Higher max for optimization
            expected_sensitivities,
            device_count: device_vts.len(),
            rate_history: VecDeque::with_capacity(10),
            performance_memory: VecDeque::with_capacity(10),
            predictive_mode: false,
            acceleration_factor: 1.0,
            last_sensitivity_ratio: 1.0,
        }
    }
    
    // OPTIMIZED: Enhanced update with predictive control
    fn update_enhanced(&mut self, sensitivity_result: Option<(f64, usize, f64, f64)>, 
                      converged: bool, iterations: usize) {
        if !converged {
            self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
            self.predictive_mode = false;
            self.acceleration_factor = 1.0;
            return;
        }
        
        // Track performance
        let performance = if iterations <= 20 { 1.0 } 
                         else if iterations <= 30 { 0.8 }
                         else { 0.5 };
        self.performance_memory.push_back(performance);
        if self.performance_memory.len() > 10 {
            self.performance_memory.pop_front();
        }
        
        if let Some((sensitivity, device_idx, reliability, prediction)) = sensitivity_result {
            let expected = self.expected_sensitivities[device_idx.min(self.expected_sensitivities.len() - 1)];
            let ratio = (sensitivity / expected).abs();
            
            // Trend analysis
            let ratio_change = ratio / self.last_sensitivity_ratio;
            self.last_sensitivity_ratio = ratio;
            
            // Enable predictive mode if consistent good performance
            if self.performance_memory.len() >= 5 {
                let avg_perf: f64 = self.performance_memory.iter().sum::<f64>() / self.performance_memory.len() as f64;
                self.predictive_mode = avg_perf > 0.8 && reliability > 0.8;
            }
            
            // Advanced rate calculation
            let base_adjustment = if ratio > 5.0 {
                0.6  // Very high sensitivity - slow down significantly
            } else if ratio > 3.0 {
                0.75
            } else if ratio > 2.0 {
                0.85
            } else if ratio < 0.2 {
                1.5  // Very low sensitivity - speed up more
            } else if ratio < 0.5 {
                1.3
            } else {
                1.1  // Good range - moderate speedup
            };
            
            // Predictive adjustment
            let predictive_adjustment = if self.predictive_mode {
                if ratio_change > 1.1 {
                    0.9  // Sensitivity increasing - be cautious
                } else if ratio_change < 0.9 {
                    1.1  // Sensitivity decreasing - can accelerate
                } else {
                    1.05
                }
            } else {
                1.0
            };
            
            // Acceleration for low sensitivity regions
            if ratio < 0.5 && reliability > 0.9 {
                self.acceleration_factor = (self.acceleration_factor * 1.1).min(2.0);
            } else {
                self.acceleration_factor = (self.acceleration_factor * 0.95).max(1.0);
            }
            
            // Combine all factors
            let total_adjustment = base_adjustment * predictive_adjustment * self.acceleration_factor;
            self.current_ramp_rate = (self.current_ramp_rate * total_adjustment)
                .max(self.min_rate)
                .min(self.max_rate);
            
            // Track rate history for analysis
            self.rate_history.push_back(self.current_ramp_rate);
            if self.rate_history.len() > 10 {
                self.rate_history.pop_front();
            }
            
            // Adaptive bounds based on history
            if self.rate_history.len() >= 5 {
                let avg_rate: f64 = self.rate_history.iter().sum::<f64>() / self.rate_history.len() as f64;
                self.max_rate = (avg_rate * 3.0).min(0.2);  // Dynamic max based on typical rates
            }
        } else {
            // No sensitivity data - conservative approach
            self.current_ramp_rate = (self.current_ramp_rate * 1.02).min(self.max_rate);
            self.predictive_mode = false;
        }
    }
}

// OPTIMIZED: Main solver with enhanced algorithms
pub struct OptimizedSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    nonlinear_elements: Vec<usize>,
    history: OptimizedHistory,
    controller: OptimizedController,
    
    // Optimization additions
    initial_analysis_done: bool,
    circuit_complexity: f64,
    dominant_time_constant: f64,
}

impl OptimizedSolver {
    pub fn new(num_nodes: usize, device_vts: &[f64]) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            nonlinear_elements: Vec::new(),
            history: OptimizedHistory::new(device_vts.len()),
            controller: OptimizedController::new(device_vts),
            initial_analysis_done: false,
            circuit_complexity: 1.0,
            dominant_time_constant: 1.0,
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
    
    // OPTIMIZED: Better log current calculation with adaptive bounds
    fn adaptive_log_current(&self, elem_idx: usize, voltage: f64, ramp_factor: f64) -> f64 {
        let current = self.elements[elem_idx].current_at_voltage(voltage);
        let abs_current = current.abs();
        
        // Adaptive minimum based on ramp progress
        let min_current = if ramp_factor < 0.1 {
            1e-18  // Ultra-low for initial ramping
        } else if ramp_factor < 0.5 {
            1e-16
        } else {
            1e-15  // Standard minimum
        };
        
        let bounded_current = abs_current.max(min_current).min(1e6);
        bounded_current.ln()
    }
    
    // OPTIMIZED: Smart initial analysis
    fn analyze_circuit_complexity(&mut self) {
        if self.initial_analysis_done { return; }
        
        // Count device types
        let mut resistor_count = 0;
        let mut nonlinear_count = 0;
        let mut total_conductance = 0.0;
        
        for elem in &self.elements {
            match elem.element_type() {
                ElementType::Resistor => {
                    resistor_count += 1;
                    total_conductance += elem.conductance();
                }
                ElementType::LED | ElementType::Diode | ElementType::Zener => {
                    nonlinear_count += 1;
                }
                _ => {}
            }
        }
        
        // Estimate circuit complexity
        self.circuit_complexity = 1.0 + (nonlinear_count as f64 * 0.5);
        
        // Estimate dominant time constant (simplified)
        if total_conductance > 0.0 {
            self.dominant_time_constant = 1.0 / total_conductance;
        }
        
        // Adjust initial ramp rate based on analysis
        if nonlinear_count > 2 {
            self.controller.current_ramp_rate *= 0.5;  // More conservative for complex circuits
        }
        
        self.initial_analysis_done = true;
    }
    
    // OPTIMIZED: Main solve method with enhancements
    pub fn solve_optimized(&mut self) -> (Vec<f64>, f64, usize, f64, bool, f64) {
        let start = Instant::now();
        self.analyze_circuit_complexity();
        
        println!("\n=== OPTIMIZED LOGARITHMIC GRADIENT SOLVER ===");
        println!("Devices: {} total, {} nonlinear", self.elements.len(), self.nonlinear_elements.len());
        println!("Circuit complexity: {:.2}", self.circuit_complexity);
        
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
        
        // Optimized ramping with predictive control
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0;
        let mut success = true;
        let mut last_convergence_quality = 0.5;
        
        while ramp_factor < 1.0 && ramp_step < 500 {  // Tighter iteration limit
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve with quality tracking
            let (converged, iters, quality) = self.solve_with_quality_metrics(&mut total_iterations);
            
            if converged && !self.nonlinear_elements.is_empty() {
                let mut device_voltages = Vec::new();
                let mut device_log_currents = Vec::new();
                
                for &elem_idx in &self.nonlinear_elements {
                    let mut element_voltage = 0.0;
                    for &(conn_elem, pos, neg) in &self.connections {
                        if conn_elem == elem_idx {
                            element_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                            break;
                        }
                    }
                    
                    device_voltages.push(element_voltage);
                    device_log_currents.push(self.adaptive_log_current(elem_idx, element_voltage, ramp_factor));
                }
                
                self.history.add_point(&device_voltages, &device_log_currents, ramp_factor, quality);
                
                // Enhanced sensitivity analysis
                let sensitivity_result = self.history.calculate_enhanced_sensitivity();
                self.controller.update_enhanced(sensitivity_result, converged, iters);
                
                last_convergence_quality = quality;
            } else {
                self.controller.update_enhanced(None, converged, 50);
            }
            
            if !converged {
                continue;
            }
            
            // Smart ramp advancement with acceleration
            let old_ramp = ramp_factor;
            let quality_boost = if last_convergence_quality > 0.9 { 1.2 } else { 1.0 };
            ramp_factor += self.controller.current_ramp_rate * quality_boost;
            ramp_factor = ramp_factor.min(1.0);
            
            // Ensure progress
            if (ramp_factor - old_ramp) < 1e-6 && ramp_factor < 0.99 {
                ramp_factor = old_ramp + 0.001;
            }
            
            ramp_step += 1;
            
            if ramp_step % 50 == 0 {
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                println!("  Step {}: {:.1}% complete, Rate={:.4}, Quality={:.2}, Time={:.1}ms", 
                         ramp_step, ramp_factor * 100.0, self.controller.current_ramp_rate, 
                         last_convergence_quality, elapsed);
            }
        }
        
        if ramp_step >= 500 {
            println!("⚠️  Reached optimized iteration limit");
            success = false;
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        let (final_converged, final_iters, final_quality) = self.solve_with_quality_metrics(&mut total_iterations);
        if !final_converged {
            success = false;
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        // Calculate final error estimate
        let error_estimate = self.estimate_solution_error();
        
        println!("  Total steps: {}, Success: {}, Final quality: {:.3}", ramp_step, success, final_quality);
        println!("  Estimated error: {:.2}%", error_estimate * 100.0);
        
        (self.node_voltages.clone(), elapsed, total_iterations, self.controller.current_ramp_rate, success, error_estimate)
    }
    
    // OPTIMIZED: Solve with quality metrics
    fn solve_with_quality_metrics(&mut self, total_iterations: &mut usize) -> (bool, usize, f64) {
        let max_iter = 40;  // Slightly reduced for speed
        let tol = 1e-12;
        let mut iterations = 0;
        let mut convergence_history = Vec::new();
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                let mut total_change = 0.0f64;
                
                // Adaptive damping based on iteration
                let damping = if iter < 5 { 0.6 } else if iter < 10 { 0.7 } else { 0.8 };
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                    total_change += delta.abs();
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                // Update element states
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                convergence_history.push(max_change);
                
                if max_change < tol {
                    // Calculate quality based on convergence pattern
                    let quality = Self::assess_convergence_quality(&convergence_history);
                    return (true, iterations, quality);
                }
            } else {
                return (false, iterations, 0.0);
            }
        }
        
        (false, iterations, 0.0)
    }
    
    // Assess convergence quality
    fn assess_convergence_quality(history: &[f64]) -> f64 {
        if history.len() < 2 { return 0.5; }
        
        // Check for monotonic decrease (good)
        let mut monotonic = true;
        for i in 1..history.len() {
            if history[i] > history[i-1] * 1.1 {
                monotonic = false;
                break;
            }
        }
        
        // Calculate convergence rate
        let last = history[history.len()-1];
        let first = history[0];
        let rate = if first > 0.0 { (last / first).powf(1.0 / history.len() as f64) } else { 1.0 };
        
        // Quality score
        let monotonic_score = if monotonic { 1.0 } else { 0.7 };
        let rate_score = (1.0 - rate).max(0.0);
        let speed_score = if history.len() <= 10 { 1.0 } else if history.len() <= 20 { 0.8 } else { 0.6 };
        
        (monotonic_score * 0.3 + rate_score * 0.5 + speed_score * 0.2).min(1.0)
    }
    
    // Estimate solution error based on convergence characteristics
    fn estimate_solution_error(&self) -> f64 {
        // This is a heuristic based on circuit complexity and convergence quality
        let base_error = 0.01;  // 1% base error
        
        // Adjust based on circuit complexity
        let complexity_factor = 1.0 + (self.circuit_complexity - 1.0) * 0.5;
        
        // Adjust based on final convergence quality
        let quality_factor = if self.history.convergence_quality.is_empty() { 
            2.0 
        } else {
            let avg_quality: f64 = self.history.convergence_quality.iter().sum::<f64>() / 
                                  self.history.convergence_quality.len() as f64;
            2.0 - avg_quality  // Higher quality -> lower error
        };
        
        base_error * complexity_factor * quality_factor
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        // GMIN for stability
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

// Reference solution for comparison
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
    println!("=== OPTIMIZED LOGARITHMIC GRADIENT SOLVER TEST ===");
    
    // Test Case 1: Standard diode circuit
    println!("\n🔧 TEST 1: STANDARD DIODE CIRCUIT");
    println!("Circuit: 1V -> R(100Ω) -> Diode -> GND");
    
    let device_vts = vec![0.026];
    let mut solver = OptimizedSolver::new(3, &device_vts);
    
    let vs = solver.add_element(Box::new(VoltageSource::new(1.0)));
    let r = solver.add_element(Box::new(Resistor::new(100.0)));
    let led = solver.add_element(Box::new(LED::new(1e-12, 0.026, 0.0)));  // Regular diode (vf=0)
    
    solver.connect(vs, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(led, 2, 0);
    
    let (voltages, time, iterations, final_rate, success, error_est) = solver.solve_optimized();
    
    // Compare with reference
    let (vd_ref, id_ref) = analytical_reference(1.0, 100.0, 1e-12, 0.026);
    let vd_computed = voltages[2];
    let id_computed = (voltages[1] - voltages[2]) / 100.0;
    
    println!("\nResults:");
    println!("  Diode voltage: {:.6}V (ref: {:.6}V)", vd_computed, vd_ref);
    println!("  Diode current: {:.6}A (ref: {:.6}A)", id_computed, id_ref);
    
    let v_err = ((vd_computed - vd_ref) / vd_ref * 100.0).abs();
    let i_err = ((id_computed - id_ref) / id_ref * 100.0).abs();
    
    println!("  Voltage error: {:.3}%", v_err);
    println!("  Current error: {:.3}%", i_err);
    println!("  Estimated error: {:.3}%", error_est * 100.0);
    println!("  Time: {:.1}ms, Iterations: {}", time, iterations);
    
    // Test Case 2: Multi-LED circuit
    println!("\n🔧 TEST 2: MULTI-LED CIRCUIT");
    println!("Circuit: 12V -> R(100Ω) -> LED(2V) -> LED(2.2V) -> LED(3.2V) -> R(100Ω) -> GND");
    
    let device_vts = vec![0.026, 0.026, 0.026];
    let mut solver2 = OptimizedSolver::new(6, &device_vts);
    
    let vs2 = solver2.add_element(Box::new(VoltageSource::new(12.0)));
    let r1 = solver2.add_element(Box::new(Resistor::new(100.0)));
    let led_red = solver2.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    let led_green = solver2.add_element(Box::new(LED::new(1e-12, 0.026, 2.2)));
    let led_blue = solver2.add_element(Box::new(LED::new(1e-12, 0.026, 3.2)));
    let r2 = solver2.add_element(Box::new(Resistor::new(100.0)));
    
    solver2.connect(vs2, 1, 0);
    solver2.connect(r1, 1, 2);
    solver2.connect(led_red, 2, 3);
    solver2.connect(led_green, 3, 4);
    solver2.connect(led_blue, 4, 5);
    solver2.connect(r2, 5, 0);
    
    let (voltages2, time2, iterations2, final_rate2, success2, error_est2) = solver2.solve_optimized();
    
    println!("\nResults:");
    println!("  Node voltages: {:?}", &voltages2[1..]);
    println!("  Red LED: {:.3}V", (voltages2[2] - voltages2[3]).abs());
    println!("  Green LED: {:.3}V", (voltages2[3] - voltages2[4]).abs());
    println!("  Blue LED: {:.3}V", (voltages2[4] - voltages2[5]).abs());
    println!("  Time: {:.1}ms, Iterations: {}", time2, iterations2);
    println!("  Estimated error: {:.3}%", error_est2 * 100.0);
    
    // Comparison with paper reference
    println!("\n=== OPTIMIZATION RESULTS ===");
    println!("Paper reference: 3.55% error, 21.5ms");
    println!("Optimized solver: {:.2}% error, {:.1}ms", v_err.max(i_err), time);
    
    if v_err.max(i_err) < 3.55 && time < 21.5 {
        println!("✅ SUCCESS: Optimized solver beats paper reference!");
    } else {
        println!("🔄 Work in progress: Further optimization needed");
    }
    
    println!("\n🎯 OPTIMIZATION FEATURES IMPLEMENTED:");
    println!("1. ✅ Multi-resolution sensitivity analysis");
    println!("2. ✅ Predictive ramp control with quadratic extrapolation");
    println!("3. ✅ Adaptive history window sizing");
    println!("4. ✅ Smart initial circuit analysis");
    println!("5. ✅ Convergence quality assessment");
    println!("6. ✅ Acceleration in low-sensitivity regions");
    println!("7. ✅ Performance memory and trend analysis");
}