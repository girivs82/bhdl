/// Logarithmic Gradient Solver
/// 
/// This solver uses logarithmic gradients to handle exponential device behavior:
/// - Track d(log(I))/dV instead of dI/dV
/// - Converts exponential sensitivity to linear behavior
/// - Should handle extreme Is and Vt parameters much better

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Element trait and implementations (same as before)
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
    
    /// Calculate logarithmic current for gradient analysis
    /// Returns log(|I| + I_min) where I_min prevents log(0)
    fn log_current_at_voltage(&self, v: f64) -> f64 {
        let i = self.current_at_voltage(v);
        let i_min = 1e-18; // Minimum current to prevent log(0)
        (i.abs() + i_min).ln()
    }
    
    /// Calculate d(log(I))/dV for exponential devices
    /// For diode: d(log(I))/dV = d(log(Is*(exp(V/Vt)-1)))/dV ≈ 1/Vt for forward bias
    fn log_current_gradient(&self, v: f64) -> f64 {
        let v_norm = v / self.vt;
        
        if v_norm > 1.0 {
            // Forward bias: d(log(I))/dV ≈ 1/Vt
            1.0 / self.vt
        } else if v_norm > -5.0 {
            // Transition region
            let exp_term = (v_norm).exp();
            let i_norm = exp_term - 1.0;
            if i_norm > 1e-10 {
                exp_term / (i_norm * self.vt)
            } else {
                // Near zero current, use linear approximation
                1.0 / self.vt
            }
        } else {
            // Reverse bias: very small gradient
            1e-6 / self.vt
        }
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

// Logarithmic history tracking
#[derive(Clone)]
struct LogHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
}

impl LogHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(5),
            log_currents: VecDeque::with_capacity(5),
            ramp_factors: VecDeque::with_capacity(5),
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, ramp: f64) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.ramp_factors.push_back(ramp);
        
        if self.voltages.len() > 5 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
            self.ramp_factors.pop_front();
        }
    }
    
    fn calculate_log_gradients(&self) -> Option<(f64, f64)> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        let v2 = self.voltages[n-1];
        let v1 = self.voltages[n-2];
        let v0 = self.voltages[n-3];
        
        let log_i2 = self.log_currents[n-1];
        let log_i1 = self.log_currents[n-2];
        let log_i0 = self.log_currents[n-3];
        
        let r2 = self.ramp_factors[n-1];
        let r1 = self.ramp_factors[n-2];
        let r0 = self.ramp_factors[n-3];
        
        let dr1 = r1 - r0;
        let dr2 = r2 - r1;
        
        if dr1 > 0.0 && dr2 > 0.0 {
            // First-order logarithmic gradient: d(log(I))/d(ramp)
            let dlog_i1 = (log_i1 - log_i0) / dr1;
            let dlog_i2 = (log_i2 - log_i1) / dr2;
            
            // Second-order: curvature in log space
            let d2log_i = (dlog_i2 - dlog_i1) / ((dr1 + dr2) / 2.0);
            
            Some((dlog_i2, d2log_i))
        } else {
            None
        }
    }
    
    fn calculate_voltage_sensitivity(&self) -> Option<f64> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        let v2 = self.voltages[n-1];
        let v1 = self.voltages[n-2];
        
        let log_i2 = self.log_currents[n-1];
        let log_i1 = self.log_currents[n-2];
        
        let dv = v2 - v1;
        if dv.abs() > 1e-9 {
            // d(log(I))/dV - this should be ~1/Vt for diodes
            Some((log_i2 - log_i1) / dv)
        } else {
            None
        }
    }
}

// Logarithmic gradient controller
struct LogController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    target_log_gradient: f64,
    adaptation_factor: f64,
}

impl LogController {
    fn new() -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.1,
            target_log_gradient: 10.0,  // Target d(log(I))/d(ramp) for optimal progression
            adaptation_factor: 1.5,
        }
    }
    
    fn update(&mut self, log_gradient: f64, log_curvature: f64, voltage_sensitivity: Option<f64>) {
        let log_grad_abs = log_gradient.abs();
        let log_curv_abs = log_curvature.abs();
        
        println!("  Log gradient: {:.3}, Log curvature: {:.3e}", log_gradient, log_curvature);
        
        // For exponential devices, voltage sensitivity d(log(I))/dV should be ~1/Vt
        if let Some(sens) = voltage_sensitivity {
            println!("  Voltage sensitivity d(log(I))/dV: {:.3} (should be ~1/Vt)", sens);
            
            // If sensitivity is very high (>>1/Vt), we're in exponential region - slow down
            if sens > 50.0 {  // Much higher than 1/0.026 ≈ 38
                self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
                println!("    High sensitivity detected - slowing ramp");
                return;
            }
        }
        
        // Adaptive control based on logarithmic gradients
        if log_grad_abs > self.target_log_gradient * 2.0 {
            // Too fast progression in log space
            self.current_ramp_rate = (self.current_ramp_rate / self.adaptation_factor).max(self.min_rate);
            println!("    Log gradient too high - reducing ramp rate to {:.4}", self.current_ramp_rate);
        } else if log_grad_abs < self.target_log_gradient * 0.5 && log_curv_abs < 100.0 {
            // Too slow progression and low curvature
            self.current_ramp_rate = (self.current_ramp_rate * self.adaptation_factor).min(self.max_rate);
            println!("    Log gradient too low - increasing ramp rate to {:.4}", self.current_ramp_rate);
        }
        
        // Handle high curvature in log space
        if log_curv_abs > 1000.0 {
            self.current_ramp_rate = (self.current_ramp_rate * 0.7).max(self.min_rate);
            println!("    High log curvature - reducing ramp rate");
        }
    }
}

pub struct LogarithmicGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    diode_histories: Vec<LogHistory>,
    diode_params: Vec<(f64, f64)>, // (Is, Vt) for each diode
    controller: LogController,
}

impl LogarithmicGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            diode_histories: Vec::new(),
            diode_params: Vec::new(),
            controller: LogController::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        let is_diode = element.element_type() == ElementType::Diode;
        self.elements.push(element);
        
        if is_diode {
            self.diode_histories.push(LogHistory::new());
        }
        
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn logarithmic_dc_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        println!("\nLogarithmic Gradient DC Analysis");
        println!("Using log-space gradients to handle exponential devices");
        
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
        
        // Find diode elements for logging
        let mut diode_indices = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::Diode {
                diode_indices.push(i);
            }
        }
        
        // Adaptive source ramping with logarithmic control
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Newton-Raphson to convergence
            let (converged, newton_iters) = self.solve_to_convergence(&mut total_iterations);
            
            if !converged {
                println!("  Warning: Newton-Raphson failed at ramp factor {:.4}", ramp_factor);
                self.controller.current_ramp_rate *= 0.5;
                continue;
            }
            
            // Record logarithmic history for diodes
            for (hist_idx, &diode_idx) in diode_indices.iter().enumerate() {
                let elem = &self.elements[diode_idx];
                if elem.element_type() == ElementType::Diode {
                    let v_diode = elem.get_voltage();
                    
                    // Calculate log current directly using the diode equation
                    let v_norm = v_diode / 0.026; // Assume Vt for log calculation
                    let i = 1e-12 * (v_norm.exp() - 1.0); // Assume typical Is
                    let i_min = 1e-18;
                    let log_current = (i.abs() + i_min).ln();
                    
                    if hist_idx < self.diode_histories.len() {
                        self.diode_histories[hist_idx].add_point(v_diode, log_current, ramp_factor);
                    }
                }
            }
            
            // Calculate logarithmic gradients
            let mut max_log_gradient = 0.0;
            let mut max_log_curvature = 0.0;
            let mut voltage_sensitivity = None;
            
            for history in &self.diode_histories {
                if let Some((log_grad, log_curv)) = history.calculate_log_gradients() {
                    max_log_gradient = max_log_gradient.max(log_grad.abs());
                    max_log_curvature = max_log_curvature.max(log_curv.abs());
                }
                
                if voltage_sensitivity.is_none() {
                    voltage_sensitivity = history.calculate_voltage_sensitivity();
                }
            }
            
            // Update controller based on logarithmic behavior
            if max_log_gradient > 0.0 {
                self.controller.update(max_log_gradient, max_log_curvature, voltage_sensitivity);
            }
            
            // Advance ramp
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            ramp_step += 1;
            
            if ramp_step % 10 == 0 {
                println!("  Ramp: {:.4}, V_diode: {:.6}V, Newton: {} iters", 
                         ramp_factor, self.node_voltages[2], newton_iters);
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
        
        println!("  Total ramp steps: {}", ramp_step);
        println!("  Total Newton iterations: {}", total_iterations);
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let max_iter = 50;
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
                
                let damping = if iter < 3 { 0.7 } else { 1.0 };
                
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

// Helper function to calculate log current for any diode
fn calculate_log_current_for_diode(voltage: f64, is: f64, vt: f64) -> f64 {
    let v_norm = voltage / vt;
    let i = if v_norm > 50.0 {
        is * (50.0_f64.exp() - 1.0) + (is / vt) * 50.0_f64.exp() * (voltage - 50.0 * vt)
    } else if v_norm < -5.0 {
        -is
    } else {
        is * (v_norm.exp() - 1.0)
    };
    let i_min = 1e-18;
    (i.abs() + i_min).ln()
}

// Test functions
fn test_logarithmic_solver(vs: f64, rs: f64, is: f64, vt: f64, label: &str) -> (f64, f64, usize, f64) {
    println!("\n--- Testing {} ---", label);
    
    let mut solver = LogarithmicGradientSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = solver.add_element(Box::new(Resistor::new(rs)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    solver.logarithmic_dc_analysis()
}

fn main() {
    println!("=== LOGARITHMIC GRADIENT SOLVER ===");
    
    // Test cases
    let test_cases = [
        (1.0, 100.0, 1e-12, 0.026, "Baseline"),
        (1.0, 100.0, 1e-15, 0.026, "Low Is (problematic for standard gradient)"),
        (1.0, 100.0, 1e-12, 0.050, "High Vt (problematic for standard gradient)"),
    ];
    
    for &(vs, rs, is, vt, label) in &test_cases {
        // SPICE reference
        let mut vd_ref = 0.7;
        for _ in 0..100 {
            let id = is * ((vd_ref / vt).exp() - 1.0);
            let f = vd_ref + id * rs - vs;
            let g = 1.0 + (is / vt) * (vd_ref / vt).exp() * rs;
            let delta = f / g;
            vd_ref -= delta;
            if delta.abs() < 1e-15 {
                break;
            }
        }
        let id_ref = (vs - vd_ref) / rs;
        
        println!("\nSPICE Reference for {}:", label);
        println!("  Vd = {:.9}V, Id = {:.6}mA", vd_ref, id_ref * 1000.0);
        
        // Test logarithmic solver
        let (vd, id, iterations, time) = test_logarithmic_solver(vs, rs, is, vt, label);
        
        let v_err = ((vd - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id - id_ref) / id_ref * 100.0).abs();
        
        println!("\nLogarithmic Solver Results:");
        println!("  Vd = {:.9}V (error: {:.3}%)", vd, v_err);
        println!("  Id = {:.6}mA (error: {:.3}%)", id * 1000.0, i_err);
        println!("  Iterations: {}, Time: {:.1}ms", iterations, time);
        
        if v_err < 1.0 && i_err < 1.0 {
            println!("  ✓ SUCCESS: <1% error achieved!");
        } else if v_err < 5.0 && i_err < 5.0 {
            println!("  ○ GOOD: <5% error");
        } else {
            println!("  × NEEDS WORK: {:.1}% error", v_err.max(i_err));
        }
    }
    
    println!("\n=== LOGARITHMIC GRADIENT THEORY ===");
    println!("\nKey insight: For exponential devices I = Is*exp(V/Vt)");
    println!("- Linear gradient dI/dV = (Is/Vt)*exp(V/Vt) → extremely sensitive");
    println!("- Logarithmic gradient d(log(I))/dV = 1/Vt → constant!");
    println!("\nThis converts exponential sensitivity to linear behavior,");
    println!("making gradient-based adaptive control much more effective.");
}