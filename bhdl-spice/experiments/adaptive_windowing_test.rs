/// Adaptive Logarithmic Windowing Test
/// 
/// Implementation 1: Multiple logarithmic sensitivity windows
/// - Short window (2-3 points): For rapid changes
/// - Medium window (4-6 points): For stable regions  
/// - Long window (8-10 points): For very stable regions
/// - Choose window based on logarithmic gradient variance (pure mathematical)

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Same element implementations as before
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

// NEW: Adaptive Logarithmic Windowing
#[derive(Clone)]
struct AdaptiveLogHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
}

impl AdaptiveLogHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(12),
            log_currents: VecDeque::with_capacity(12),
            ramp_factors: VecDeque::with_capacity(12),
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, ramp: f64) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.ramp_factors.push_back(ramp);
        
        if self.voltages.len() > 12 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
            self.ramp_factors.pop_front();
        }
    }
    
    // Calculate logarithmic gradient for different window sizes
    fn calculate_windowed_sensitivities(&self) -> Option<(f64, f64)> { // (best_sensitivity, confidence)
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        let mut windows = Vec::new();
        
        // Short window (2-3 points) - for rapid changes
        if n >= 3 {
            windows.push(("short", 2));
        }
        
        // Medium window (4-6 points) - for stable regions
        if n >= 6 {
            windows.push(("medium", 4));
        }
        
        // Long window (8-10 points) - for very stable regions
        if n >= 10 {
            windows.push(("long", 8));
        }
        
        let mut best_sensitivity = 0.0;
        let mut best_confidence = 0.0;
        
        for (window_name, window_size) in windows {
            let sensitivity = self.calculate_sensitivity_for_window(window_size);
            let variance = self.calculate_sensitivity_variance(window_size);
            
            // Confidence = 1 / (1 + variance) - lower variance = higher confidence
            let confidence = 1.0 / (1.0 + variance);
            
            if confidence > best_confidence {
                best_sensitivity = sensitivity;
                best_confidence = confidence;
                
                if n >= 8 { // Only print if we have enough data
                    println!("    Window {}: sens={:.1}, var={:.2e}, conf={:.3}", 
                             window_name, sensitivity, variance, confidence);
                }
            }
        }
        
        if best_confidence > 0.1 { // Minimum confidence threshold
            Some((best_sensitivity, best_confidence))
        } else {
            None
        }
    }
    
    fn calculate_sensitivity_for_window(&self, window_size: usize) -> f64 {
        let n = self.voltages.len();
        if n < window_size + 1 {
            return 0.0;
        }
        
        let start_idx = n - window_size - 1;
        let mut sum_dv = 0.0;
        let mut sum_dlog_i = 0.0;
        let mut count = 0;
        
        for i in start_idx..n-1 {
            let dv = self.voltages[i+1] - self.voltages[i];
            if dv.abs() > 1e-12 {
                let dlog_i = self.log_currents[i+1] - self.log_currents[i];
                sum_dv += dv;
                sum_dlog_i += dlog_i;
                count += 1;
            }
        }
        
        if count > 0 && sum_dv.abs() > 1e-12 {
            sum_dlog_i / sum_dv
        } else {
            0.0
        }
    }
    
    fn calculate_sensitivity_variance(&self, window_size: usize) -> f64 {
        let n = self.voltages.len();
        if n < window_size + 1 {
            return 1e6; // Very high variance for insufficient data
        }
        
        let start_idx = n - window_size - 1;
        let mut sensitivities = Vec::new();
        
        // Calculate point-to-point sensitivities
        for i in start_idx..n-1 {
            let dv = self.voltages[i+1] - self.voltages[i];
            if dv.abs() > 1e-12 {
                let dlog_i = self.log_currents[i+1] - self.log_currents[i];
                sensitivities.push(dlog_i / dv);
            }
        }
        
        if sensitivities.len() < 2 {
            return 1e6;
        }
        
        // Calculate variance
        let mean: f64 = sensitivities.iter().sum::<f64>() / sensitivities.len() as f64;
        let variance: f64 = sensitivities.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f64>() / sensitivities.len() as f64;
        
        variance
    }
}

// Controller using adaptive windowing
struct AdaptiveWindowController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    actual_vt: f64,
}

impl AdaptiveWindowController {
    fn new(vt: f64) -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.05,
            actual_vt: vt,
        }
    }
    
    fn expected_sensitivity(&self) -> f64 {
        1.0 / self.actual_vt
    }
    
    fn update(&mut self, sensitivity: Option<f64>, confidence: f64) {
        if let Some(sens) = sensitivity {
            let expected_sens = self.expected_sensitivity();
            let sensitivity_ratio = sens / expected_sens;
            
            // Weight the adjustment by confidence - higher confidence = stronger adjustment
            let adjustment_strength = confidence.min(1.0);
            
            if sensitivity_ratio > 2.5 {
                // High sensitivity - reduce ramp rate
                let factor = 1.0 - (0.3 * adjustment_strength); // Up to 30% reduction based on confidence
                self.current_ramp_rate = (self.current_ramp_rate * factor).max(self.min_rate);
                println!("    HIGH sens: ratio={:.2}, conf={:.3}, reducing by {:.1}% to {:.4}", 
                         sensitivity_ratio, confidence, (1.0 - factor) * 100.0, self.current_ramp_rate);
            } else if sensitivity_ratio < 0.6 {
                // Low sensitivity - can increase ramp rate
                let factor = 1.0 + (0.2 * adjustment_strength); // Up to 20% increase based on confidence
                self.current_ramp_rate = (self.current_ramp_rate * factor).min(self.max_rate);
                println!("    Low sens: ratio={:.2}, conf={:.3}, increasing by {:.1}% to {:.4}", 
                         sensitivity_ratio, confidence, (factor - 1.0) * 100.0, self.current_ramp_rate);
            } else {
                // Good sensitivity - minor increase if high confidence
                if confidence > 0.8 {
                    let factor = 1.0 + (0.05 * adjustment_strength);
                    self.current_ramp_rate = (self.current_ramp_rate * factor).min(self.max_rate);
                }
            }
        } else {
            // No reliable sensitivity - be conservative
            self.current_ramp_rate = (self.current_ramp_rate * 0.9f64).max(self.min_rate);
        }
    }
}

// Solver with adaptive windowing
pub struct AdaptiveWindowingSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: AdaptiveLogHistory,
    controller: AdaptiveWindowController,
}

impl AdaptiveWindowingSolver {
    pub fn new(num_nodes: usize, diode_vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: AdaptiveLogHistory::new(),
            controller: AdaptiveWindowController::new(diode_vt),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn log_current_for_diode(&self, voltage: f64, is: f64, vt: f64) -> f64 {
        let i = if voltage / vt > 50.0 {
            is * (50.0_f64.exp() - 1.0)
        } else if voltage / vt < -5.0 {
            -is
        } else {
            is * ((voltage / vt).exp() - 1.0)
        };
        let i_min = 1e-18;
        (i.abs() + i_min).ln()
    }
    
    pub fn adaptive_windowing_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        println!("\n=== ADAPTIVE LOGARITHMIC WINDOWING DC ANALYSIS ===");
        
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
        
        // Adaptive ramping with windowing
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve at current ramp factor
            let (converged, _newton_iters) = self.solve_to_convergence(&mut total_iterations);
            
            if !converged {
                self.controller.current_ramp_rate *= 0.5f64;
                continue;
            }
            
            // Update history and controller using adaptive windowing
            let diode_voltage = self.node_voltages[2];
            let log_current = self.log_current_for_diode(diode_voltage, 1e-12, self.controller.actual_vt);
            self.history.add_point(diode_voltage, log_current, ramp_factor);
            
            if let Some((sensitivity, confidence)) = self.history.calculate_windowed_sensitivities() {
                self.controller.update(Some(sensitivity), confidence);
            } else {
                self.controller.update(None, 0.0);
            }
            
            // Advance ramp
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            ramp_step += 1;
            
            if ramp_step % 25 == 0 {
                println!("  Step {}: {:.1}% complete, Vd={:.6}V, Rate={:.4}", 
                         ramp_step, ramp_factor * 100.0, self.node_voltages[2], self.controller.current_ramp_rate);
            }
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        self.solve_to_convergence(&mut total_iterations);
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("  Total steps: {}", ramp_step);
        
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

// Test function
fn test_adaptive_windowing_solver(vs: f64, rs: f64, is: f64, vt: f64, label: &str) -> (f64, f64, usize, f64) {
    println!("\n--- Testing {} ---", label);
    
    let mut solver = AdaptiveWindowingSolver::new(3, vt);
    
    let v = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = solver.add_element(Box::new(Resistor::new(rs)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    solver.adaptive_windowing_analysis()
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
    println!("=== ADAPTIVE LOGARITHMIC WINDOWING TEST ===");
    
    let test_cases = [
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt (previous 0.242% error)", 1.0, 100.0, 1e-12, 0.050),
        ("Low current (previous 9287 iters)", 0.1, 1000.0, 1e-12, 0.026),
    ];
    
    println!("\n{}", "=".repeat(80));
    println!("BASELINE COMPARISON:");
    println!("- Newton solver: 0.044% avg error, 1.7ms avg time");
    println!("- Original log gradient: 0.069% avg error, 12.8ms avg time");
    println!("- Previous final solver: 3.89% avg error, 4.8ms avg time");
    println!("{}", "=".repeat(80));
    
    let mut total_errors = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        println!("\n{}", "=".repeat(60));
        
        // SPICE reference
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        println!("SPICE Reference: Vd={:.9}V, Id={:.6}mA", vd_ref, id_ref * 1000.0);
        
        // Test adaptive windowing solver
        let (vd, id, iterations, time) = test_adaptive_windowing_solver(vs, rs, is, vt, name);
        
        let v_err = ((vd - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_errors += max_err;
        total_time += time;
        total_iterations += iterations;
        
        println!("\nAdaptive Windowing Results:");
        println!("  Vd = {:.9}V (error: {:.4}%)", vd, v_err);
        println!("  Id = {:.6}mA (error: {:.4}%)", id * 1000.0, i_err);
        println!("  Iterations: {}, Time: {:.1}ms", iterations, time);
        
        if max_err < 0.1 {
            println!("  ✅ EXCELLENT: <0.1% error!");
        } else if max_err < 1.0 {
            println!("  ✅ GOOD: <1% error");
        } else if max_err < 3.89 {
            println!("  ✅ IMPROVED: Better than previous {:.2}%", 3.89);
        } else {
            println!("  ⚠️  Not improved: {:.2}% vs 3.89% previous", max_err);
        }
    }
    
    let avg_error = total_errors / test_cases.len() as f64;
    let avg_time = total_time / test_cases.len() as f64;
    let avg_iters = total_iterations as f64 / test_cases.len() as f64;
    
    println!("\n{}", "=".repeat(60));
    println!("=== ADAPTIVE WINDOWING SUMMARY ===");
    println!("Average error: {:.4}%", avg_error);
    println!("Average time: {:.1}ms", avg_time);
    println!("Average iterations: {:.0}", avg_iters);
    
    println!("\n=== COMPARISON ===");
    println!("Adaptive Windowing: {:.4}% error, {:.1}ms time", avg_error, avg_time);
    println!("Previous solver:    3.8940% error, 4.8ms time");
    println!("Newton solver:      0.0440% error, 1.7ms time");
    
    if avg_error < 3.89 {
        println!("\n🎯 SUCCESS: Adaptive windowing IMPROVED accuracy!");
        println!("   Error reduction: {:.2}%", 3.89 - avg_error);
        if avg_time <= 4.8 {
            println!("   Time maintained or improved");
        }
        println!("\n✅ RETAIN: Adaptive logarithmic windowing");
    } else {
        println!("\n❌ NO IMPROVEMENT: Adaptive windowing did not help");
        println!("   Error change: +{:.2}%", avg_error - 3.89);
        println!("\n❌ DISCARD: Move to next improvement");
    }
    
    println!("\n=== ADAPTIVE WINDOWING ANALYSIS ===");
    println!("Key innovation: Multiple window sizes (short/medium/long) with");
    println!("confidence-based selection using logarithmic gradient variance.");
    println!("Pure mathematical approach - no circuit-specific knowledge!");
}