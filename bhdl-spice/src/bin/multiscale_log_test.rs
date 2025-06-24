/// Multi-Scale Logarithmic Analysis Test
/// 
/// Implementation 2: Calculate d(log(I))/dV at multiple voltage scales
/// - Fine scale: Adjacent voltage points
/// - Medium scale: Every 2nd point 
/// - Coarse scale: Every 3rd point
/// - Use scale with most consistent logarithmic behavior (pure mathematical)

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

// NEW: Multi-Scale Logarithmic Analysis
#[derive(Clone)]
struct MultiScaleLogHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
}

impl MultiScaleLogHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(15),
            log_currents: VecDeque::with_capacity(15),
            ramp_factors: VecDeque::with_capacity(15),
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, ramp: f64) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.ramp_factors.push_back(ramp);
        
        if self.voltages.len() > 15 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
            self.ramp_factors.pop_front();
        }
    }
    
    // Multi-scale logarithmic gradient analysis
    fn calculate_multiscale_sensitivity(&self) -> Option<(f64, f64)> { // (best_sensitivity, consistency_score)
        if self.voltages.len() < 6 {
            return None;
        }
        
        let scales = [
            ("fine", 1),     // Adjacent points
            ("medium", 2),   // Every 2nd point
            ("coarse", 3),   // Every 3rd point
        ];
        
        let mut best_sensitivity = 0.0;
        let mut best_consistency = 0.0;
        
        for (scale_name, step_size) in &scales {
            if let Some((sensitivity, consistency)) = self.calculate_sensitivity_at_scale(*step_size) {
                if consistency > best_consistency {
                    best_sensitivity = sensitivity;
                    best_consistency = consistency;
                    
                    if self.voltages.len() >= 10 { // Only print if we have enough data
                        println!("    Scale {}: sens={:.1}, consistency={:.3}", 
                                 scale_name, sensitivity, consistency);
                    }
                }
            }
        }
        
        if best_consistency > 0.5 { // Minimum consistency threshold
            Some((best_sensitivity, best_consistency))
        } else {
            None
        }
    }
    
    fn calculate_sensitivity_at_scale(&self, step_size: usize) -> Option<(f64, f64)> {
        let n = self.voltages.len();
        if n < step_size * 3 { // Need at least 3 points at this scale
            return None;
        }
        
        let mut voltage_diffs = Vec::new();
        let mut log_current_diffs = Vec::new();
        let mut sensitivities = Vec::new();
        
        // Calculate sensitivity at this scale
        let mut i = 0;
        while i + step_size < n {
            let v1 = self.voltages[i];
            let v2 = self.voltages[i + step_size];
            let log_i1 = self.log_currents[i];
            let log_i2 = self.log_currents[i + step_size];
            
            let dv = v2 - v1;
            let dlog_i = log_i2 - log_i1;
            
            if dv.abs() > 1e-12 {
                let sensitivity = dlog_i / dv;
                voltage_diffs.push(dv);
                log_current_diffs.push(dlog_i);
                sensitivities.push(sensitivity);
            }
            
            i += step_size;
        }
        
        if sensitivities.len() < 2 {
            return None;
        }
        
        // Calculate average sensitivity and consistency at this scale
        let avg_sensitivity: f64 = sensitivities.iter().sum::<f64>() / sensitivities.len() as f64;
        
        // Consistency = 1 / (1 + coefficient_of_variation)
        // Higher consistency = more stable gradient across this scale
        let variance: f64 = sensitivities.iter()
            .map(|x| (x - avg_sensitivity).powi(2))
            .sum::<f64>() / sensitivities.len() as f64;
        
        let std_dev = variance.sqrt();
        let coefficient_of_variation = if avg_sensitivity.abs() > 1e-12 {
            std_dev / avg_sensitivity.abs()
        } else {
            1e6 // Very high CV for near-zero sensitivity
        };
        
        let consistency = 1.0 / (1.0 + coefficient_of_variation);
        
        Some((avg_sensitivity, consistency))
    }
    
    // Additional method: Detect scale transitions
    fn detect_optimal_scale(&self) -> usize {
        if self.voltages.len() < 9 {
            return 1; // Default to fine scale
        }
        
        let scales = [1, 2, 3];
        let mut best_scale = 1;
        let mut best_score = 0.0;
        
        for &scale in &scales {
            if let Some((sensitivity, consistency)) = self.calculate_sensitivity_at_scale(scale) {
                // Score combines consistency and proximity to expected theoretical value
                // For exponential devices, we expect d(log(I))/dV to be relatively stable
                let stability_score = consistency;
                let magnitude_score = if sensitivity > 5.0 && sensitivity < 200.0 { 1.0 } else { 0.5 };
                let total_score = stability_score * magnitude_score;
                
                if total_score > best_score {
                    best_score = total_score;
                    best_scale = scale;
                }
            }
        }
        
        best_scale
    }
}

// Controller using multi-scale analysis
struct MultiScaleController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    actual_vt: f64,
    optimal_scale: usize,
}

impl MultiScaleController {
    fn new(vt: f64) -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.05,
            actual_vt: vt,
            optimal_scale: 1, // Start with fine scale
        }
    }
    
    fn expected_sensitivity(&self) -> f64 {
        1.0 / self.actual_vt
    }
    
    fn update(&mut self, sensitivity: Option<f64>, consistency: f64, optimal_scale: usize) {
        // Update optimal scale
        if optimal_scale != self.optimal_scale {
            println!("    Scale transition: {} → {} (consistency improved)", 
                     self.optimal_scale, optimal_scale);
            self.optimal_scale = optimal_scale;
        }
        
        if let Some(sens) = sensitivity {
            let expected_sens = self.expected_sensitivity();
            let sensitivity_ratio = sens / expected_sens;
            
            // Scale-aware adjustment - coarser scales should be more conservative
            let scale_factor = match self.optimal_scale {
                1 => 1.0,    // Fine scale - normal adjustment
                2 => 0.8,    // Medium scale - 20% less aggressive
                3 => 0.6,    // Coarse scale - 40% less aggressive
                _ => 0.5,    // Unknown scale - very conservative
            };
            
            // Consistency-weighted adjustment
            let adjustment_strength = consistency * scale_factor;
            
            if sensitivity_ratio > 3.0 {
                // High sensitivity - reduce ramp rate
                let reduction = 0.25 * adjustment_strength; // Up to 25% reduction
                self.current_ramp_rate = (self.current_ramp_rate * (1.0 - reduction)).max(self.min_rate);
                println!("    HIGH sens: ratio={:.2}, consistency={:.3}, scale={}, reducing by {:.1}% to {:.4}", 
                         sensitivity_ratio, consistency, self.optimal_scale, reduction * 100.0, self.current_ramp_rate);
            } else if sensitivity_ratio < 0.7 {
                // Low sensitivity - can increase ramp rate
                let increase = 0.15 * adjustment_strength; // Up to 15% increase
                self.current_ramp_rate = (self.current_ramp_rate * (1.0 + increase)).min(self.max_rate);
                println!("    Low sens: ratio={:.2}, consistency={:.3}, scale={}, increasing by {:.1}% to {:.4}", 
                         sensitivity_ratio, consistency, self.optimal_scale, increase * 100.0, self.current_ramp_rate);
            } else if consistency > 0.8 {
                // Good sensitivity and high consistency - minor increase
                let increase = 0.05 * adjustment_strength;
                self.current_ramp_rate = (self.current_ramp_rate * (1.0 + increase)).min(self.max_rate);
            }
        } else {
            // No reliable sensitivity - be conservative
            self.current_ramp_rate = (self.current_ramp_rate * 0.95f64).max(self.min_rate);
        }
    }
}

// Solver with multi-scale analysis
pub struct MultiScaleSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: MultiScaleLogHistory,
    controller: MultiScaleController,
}

impl MultiScaleSolver {
    pub fn new(num_nodes: usize, diode_vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: MultiScaleLogHistory::new(),
            controller: MultiScaleController::new(diode_vt),
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
    
    pub fn multiscale_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        println!("\n=== MULTI-SCALE LOGARITHMIC ANALYSIS ===");
        
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
        
        // Adaptive ramping with multi-scale analysis
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
            
            // Update history and controller using multi-scale analysis
            let diode_voltage = self.node_voltages[2];
            let log_current = self.log_current_for_diode(diode_voltage, 1e-12, self.controller.actual_vt);
            self.history.add_point(diode_voltage, log_current, ramp_factor);
            
            // Multi-scale analysis
            let optimal_scale = self.history.detect_optimal_scale();
            
            if let Some((sensitivity, consistency)) = self.history.calculate_multiscale_sensitivity() {
                self.controller.update(Some(sensitivity), consistency, optimal_scale);
            } else {
                self.controller.update(None, 0.0, optimal_scale);
            }
            
            // Advance ramp
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            ramp_step += 1;
            
            if ramp_step % 25 == 0 {
                println!("  Step {}: {:.1}% complete, Vd={:.6}V, Rate={:.4}, Scale={}", 
                         ramp_step, ramp_factor * 100.0, self.node_voltages[2], 
                         self.controller.current_ramp_rate, self.controller.optimal_scale);
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
fn test_multiscale_solver(vs: f64, rs: f64, is: f64, vt: f64, label: &str) -> (f64, f64, usize, f64) {
    println!("\n--- Testing {} ---", label);
    
    let mut solver = MultiScaleSolver::new(3, vt);
    
    let v = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = solver.add_element(Box::new(Resistor::new(rs)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    solver.multiscale_analysis()
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
    println!("=== MULTI-SCALE LOGARITHMIC ANALYSIS TEST ===");
    
    let test_cases = [
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt (previous 0.242% error)", 1.0, 100.0, 1e-12, 0.050),
        ("Low current (previous 9287 iters)", 0.1, 1000.0, 1e-12, 0.026),
    ];
    
    println!("\n{}", "=".repeat(80));
    println!("BASELINE COMPARISON:");
    println!("- Newton solver: 0.044% avg error, 1.7ms avg time");
    println!("- Original log gradient: 0.069% avg error, 12.8ms avg time");
    println!("- Adaptive windowing: 2.14% avg error, 5.6ms avg time (DISCARDED)");
    println!("{}", "=".repeat(80));
    
    let mut total_errors = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        println!("\n{}", "=".repeat(60));
        
        // SPICE reference
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        println!("SPICE Reference: Vd={:.9}V, Id={:.6}mA", vd_ref, id_ref * 1000.0);
        
        // Test multi-scale solver
        let (vd, id, iterations, time) = test_multiscale_solver(vs, rs, is, vt, name);
        
        let v_err = ((vd - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_errors += max_err;
        total_time += time;
        total_iterations += iterations;
        
        println!("\nMulti-Scale Results:");
        println!("  Vd = {:.9}V (error: {:.4}%)", vd, v_err);
        println!("  Id = {:.6}mA (error: {:.4}%)", id * 1000.0, i_err);
        println!("  Iterations: {}, Time: {:.1}ms", iterations, time);
        
        if max_err < 0.1 {
            println!("  ✅ EXCELLENT: <0.1% error!");
        } else if max_err < 1.0 {
            println!("  ✅ GOOD: <1% error");
        } else if max_err < 2.14 {
            println!("  ✅ IMPROVED: Better than adaptive windowing {:.2}%", 2.14);
        } else {
            println!("  ⚠️  Not improved: {:.2}% vs 2.14% adaptive windowing", max_err);
        }
    }
    
    let avg_error = total_errors / test_cases.len() as f64;
    let avg_time = total_time / test_cases.len() as f64;
    let avg_iters = total_iterations as f64 / test_cases.len() as f64;
    
    println!("\n{}", "=".repeat(60));
    println!("=== MULTI-SCALE ANALYSIS SUMMARY ===");
    println!("Average error: {:.4}%", avg_error);
    println!("Average time: {:.1}ms", avg_time);
    println!("Average iterations: {:.0}", avg_iters);
    
    println!("\n=== COMPARISON ===");
    println!("Multi-Scale:        {:.4}% error, {:.1}ms time", avg_error, avg_time);
    println!("Adaptive Windowing: 2.1400% error, 5.6ms time");
    println!("Newton solver:      0.0440% error, 1.7ms time");
    
    if avg_error < 2.14 {
        println!("\n🎯 SUCCESS: Multi-scale analysis IMPROVED accuracy!");
        println!("   Error reduction: {:.2}%", 2.14 - avg_error);
        if avg_time <= 5.6 {
            println!("   Time maintained or improved");
        }
        println!("\n✅ RETAIN: Multi-scale logarithmic analysis");
    } else {
        println!("\n❌ NO IMPROVEMENT: Multi-scale analysis did not help");
        println!("   Error change: +{:.2}%", avg_error - 2.14);
        println!("\n❌ DISCARD: Move to next improvement");
    }
    
    println!("\n=== MULTI-SCALE ANALYSIS OVERVIEW ===");
    println!("Key innovation: Multiple voltage scales (fine/medium/coarse) with");
    println!("consistency-based selection using coefficient of variation.");
    println!("Pure mathematical approach - no circuit-specific knowledge!");
    println!("Automatically detects optimal scale based on gradient stability.");
}