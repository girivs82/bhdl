/// Optimal Hybrid Logarithmic Gradient Circuit Solver
/// 
/// Fine-tuned version focusing on achieving <0.5% error with <2ms runtime.
/// Key improvements:
/// 1. Earlier phase transition (75% instead of 80/85%)
/// 2. More balanced fast phase (not too aggressive)
/// 3. Improved convergence criteria
/// 4. Better damping strategy

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

// Balanced history tracking for both phases
struct BalancedHistory {
    sensitivities: VecDeque<f64>,
    convergence: VecDeque<bool>,
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    phase: u8,
}

impl BalancedHistory {
    fn new() -> Self {
        Self {
            sensitivities: VecDeque::with_capacity(8),
            convergence: VecDeque::with_capacity(8),
            voltages: VecDeque::with_capacity(8),
            log_currents: VecDeque::with_capacity(8),
            phase: 1,
        }
    }
    
    fn switch_phase(&mut self, phase: u8) {
        self.phase = phase;
        // Keep some history for smooth transition
        while self.sensitivities.len() > 4 {
            self.sensitivities.pop_front();
            self.convergence.pop_front();
            self.voltages.pop_front();
            self.log_currents.pop_front();
        }
    }
    
    fn add(&mut self, voltage: f64, log_current: f64, sensitivity: f64, converged: bool) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.sensitivities.push_back(sensitivity);
        self.convergence.push_back(converged);
        
        let capacity = if self.phase == 1 { 6 } else { 8 };
        while self.sensitivities.len() > capacity {
            self.sensitivities.pop_front();
            self.convergence.pop_front();
            self.voltages.pop_front();
            self.log_currents.pop_front();
        }
    }
    
    fn get_robust_sensitivity(&self) -> Option<(f64, f64)> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        let mut gradients = Vec::new();
        
        // Use appropriate spans based on phase
        let spans = if self.phase == 1 { vec![1, 2] } else { vec![1, 2, 3] };
        
        for &span in &spans {
            if span < n {
                for i in span..n {
                    let dv = self.voltages[i] - self.voltages[i - span];
                    if dv.abs() > 1e-12 {
                        let dlog_i = self.log_currents[i] - self.log_currents[i - span];
                        gradients.push(dlog_i / dv);
                    }
                }
            }
        }
        
        if gradients.is_empty() {
            return None;
        }
        
        // Use median for robustness
        gradients.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median = gradients[gradients.len() / 2];
        
        // Simple reliability based on convergence
        let conv_rate = self.convergence.iter().filter(|&&x| x).count() as f64 / self.convergence.len() as f64;
        
        Some((median, conv_rate))
    }
}

// Optimal controller with balanced approach
struct OptimalController {
    vt: f64,
    phase: u8,
    
    // Phase-specific parameters
    ramp_rate: f64,
    recent_adjustments: VecDeque<f64>,
}

impl OptimalController {
    fn new(vt: f64) -> Self {
        Self {
            vt,
            phase: 1,
            ramp_rate: 0.02,  // Balanced initial rate
            recent_adjustments: VecDeque::with_capacity(5),
        }
    }
    
    fn switch_to_phase_2(&mut self) {
        self.phase = 2;
        self.ramp_rate = 0.005;  // Conservative for accuracy
        self.recent_adjustments.clear();
    }
    
    fn update(&mut self, sensitivity: Option<(f64, f64)>, converged: bool, ramp_factor: f64) {
        if let Some((sens, reliability)) = sensitivity {
            let expected = 1.0 / self.vt;
            let ratio = sens / expected;
            
            let adjustment = if self.phase == 1 {
                // Fast phase: more aggressive adjustments
                if !converged {
                    0.5  // Halve rate on failure
                } else if ratio > 3.0 {
                    0.7  // Reduce if too sensitive
                } else if ratio < 0.3 && reliability > 0.8 {
                    1.5  // Increase if too insensitive
                } else {
                    1.1  // Gentle increase
                }
            } else {
                // Accurate phase: careful adjustments
                if !converged {
                    0.5
                } else if ratio > 2.0 {
                    0.8
                } else if ratio < 0.5 && reliability > 0.9 {
                    1.2
                } else {
                    1.05
                }
            };
            
            // Track adjustments for stability
            self.recent_adjustments.push_back(adjustment);
            if self.recent_adjustments.len() > 5 {
                self.recent_adjustments.pop_front();
            }
            
            // Apply adjustment with bounds
            if self.phase == 1 {
                self.ramp_rate = (self.ramp_rate * adjustment).clamp(0.002, 0.1);
            } else {
                self.ramp_rate = (self.ramp_rate * adjustment).clamp(0.0005, 0.02);
            }
            
            // Near completion, be more conservative
            if ramp_factor > 0.95 {
                self.ramp_rate = self.ramp_rate.min(0.005);
            }
        } else if !converged {
            self.ramp_rate *= 0.5;
        }
    }
}

// Optimal hybrid solver
pub struct OptimalHybridLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: BalancedHistory,
    controller: OptimalController,
    
    // Phase transition point
    phase_switch_ramp: f64,
}

impl OptimalHybridLogGradientSolver {
    pub fn new(num_nodes: usize, vt: f64) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            history: BalancedHistory::new(),
            controller: OptimalController::new(vt),
            phase_switch_ramp: 0.75,  // Earlier transition for better accuracy
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
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve with phase-appropriate method
            let (converged, iters) = if self.controller.phase == 1 {
                let result = self.solve_to_convergence_balanced(&mut total_iterations, 1e-10, 0.8);
                phase_1_iterations += result.1;
                result
            } else {
                let result = self.solve_to_convergence_balanced(&mut total_iterations, 1e-12, 0.7);
                phase_2_iterations += result.1;
                result
            };
            
            let (diode_v, log_i, sensitivity) = self.get_diode_info();
            
            // Update history
            self.history.add(diode_v, log_i, sensitivity, converged);
            
            // Get robust sensitivity
            let sens_result = self.history.get_robust_sensitivity();
            
            // Update controller
            self.controller.update(sens_result, converged, ramp_factor);
            
            if !converged {
                continue;
            }
            
            // Advance ramp
            ramp_factor += self.controller.ramp_rate;
            
            // Check phase transition
            if self.controller.phase == 1 && ramp_factor >= self.phase_switch_ramp {
                self.controller.switch_to_phase_2();
                self.history.switch_phase(2);
                println!("  [Switching to accurate phase at {:.1}%]", ramp_factor * 100.0);
            }
            
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100% with tight tolerance
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Multiple passes for best accuracy
        for pass in 0..2 {
            let (converged, extra_iters) = self.solve_to_convergence_balanced(&mut total_iterations, 1e-13, 0.65);
            phase_2_iterations += extra_iters;
            if converged && pass == 0 {
                println!("  [Final convergence pass added {} iterations]", extra_iters);
            }
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("  Phase 1 iterations: {} | Phase 2 iterations: {}", 
                 phase_1_iterations, phase_2_iterations);
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence_balanced(&mut self, total_iterations: &mut usize, tol: f64, damping: f64) -> (bool, usize) {
        let max_iter = 25;
        let mut iterations = 0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Adaptive damping based on iteration
                let iter_damping = if iter < 3 {
                    damping * 0.8  // More conservative initially
                } else {
                    damping
                };
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + iter_damping * delta;
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
    println!("=== OPTIMAL HYBRID LOGARITHMIC GRADIENT SOLVER ===");
    println!("Fine-tuned for <0.5% error with <2ms runtime\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Optimal Vd", "Optimal Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(110));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = OptimalHybridLogGradientSolver::new(3, vt);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_optimal, id_optimal, iters, time) = solver.solve();
        
        let v_err = ((vd_optimal - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_optimal - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 name, 
                 vd_ref, id_ref * 1000.0,
                 vd_optimal, id_optimal * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(110));
    
    let n_cases = test_cases.len() as f64;
    println!("\nOptimal Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\nComparison:");
    println!("  Reference:       0.49% error, 55.5ms time, 8,032 iterations");
    println!("  Original Hybrid: 0.95% error, 1.7ms time, 202 iterations");
    println!("  Optimal:         {:.2}% error, {:.1}ms time, {:.0} iterations", 
             total_error / n_cases, total_time / n_cases, total_iterations as f64 / n_cases);
    
    if total_time / n_cases < 2.0 && total_error / n_cases < 0.5 {
        println!("\n✅ GOAL ACHIEVED: <0.5% error with <2ms runtime!");
    } else if total_error / n_cases < 0.8 {
        println!("\n✅ Good result: <0.8% error achieved!");
    }
    
    println!("\n=== KEY OPTIMIZATIONS ===");
    println!("1. Earlier phase transition at 75% for more accurate convergence time");
    println!("2. Balanced initial rate (0.02) - not too fast, not too slow");
    println!("3. Adaptive damping based on iteration count");
    println!("4. Conservative final convergence (1e-13 tolerance)");
    println!("5. Multiple final passes for best accuracy");
    println!("6. Smooth phase transition with history preservation");
}