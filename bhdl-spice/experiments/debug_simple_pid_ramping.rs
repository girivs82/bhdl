/// Debug version of simple PID-controlled ramping solver
/// Adding extensive debugging to understand how it works

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

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

// Adaptive PID controller that adjusts gains based on log gradient
struct AdaptivePIDController {
    base_kp: f64,
    base_ki: f64,
    base_kd: f64,
    kp: f64,
    ki: f64,
    kd: f64,
    integral: f64,
    last_error: f64,
}

impl AdaptivePIDController {
    fn new(base_kp: f64, base_ki: f64, base_kd: f64) -> Self {
        Self {
            base_kp,
            base_ki,
            base_kd,
            kp: base_kp,
            ki: base_ki,
            kd: base_kd,
            integral: 0.0,
            last_error: 0.0,
        }
    }
    
    fn adapt_gains(&mut self, log_gradient: f64) {
        // Adapt gains based on device sensitivity (log gradient)
        // For phase 1 (high base gains), we're already aggressive
        // Still adapt but with less extreme multipliers
        
        if log_gradient < 2.0 {
            // Very low sensitivity (like high Vt diode) - maximum aggression
            self.kp = self.base_kp * 2.0;   // With high base, this is very aggressive
            self.ki = self.base_ki * 3.0;   // Much more integral action
            self.kd = self.base_kd * 0.5;   // Less derivative
        } else if log_gradient < 10.0 {
            // Low sensitivity - be aggressive
            self.kp = self.base_kp * 1.5;
            self.ki = self.base_ki * 2.0;   // More integral action
            self.kd = self.base_kd * 0.7;   // Less derivative
        } else if log_gradient > 30.0 {
            // High sensitivity - slightly reduce aggression
            self.kp = self.base_kp * 0.8;
            self.ki = self.base_ki * 0.7;   // Less integral
            self.kd = self.base_kd * 1.2;   // More derivative for stability
        } else {
            // Normal sensitivity - use base gains
            self.kp = self.base_kp;
            self.ki = self.base_ki;
            self.kd = self.base_kd;
        }
    }
    
    fn update(&mut self, error: f64, dt: f64) -> f64 {
        // P term
        let p = self.kp * error;
        
        // I term
        self.integral += error * dt;
        let i = self.ki * self.integral;
        
        // D term
        let d = self.kd * (error - self.last_error) / dt;
        self.last_error = error;
        
        p + i + d
    }
    
    fn reset(&mut self) {
        self.integral = 0.0;
        self.last_error = 0.0;
    }
}

// PID-controlled solver
pub struct DebugPIDRampingSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    nonlinear_elements: Vec<usize>,
}

impl DebugPIDRampingSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            nonlinear_elements: Vec::new(),
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
    
    pub fn solve_with_pid(&mut self) -> (Vec<f64>, f64, usize) {
        let start = Instant::now();
        
        println!("\n=== DEBUG PID RAMPING SOLVER ===");
        println!("Circuit: {} nodes, {} elements", self.num_nodes, self.elements.len());
        
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
                println!("Voltage source {} at {:.1}V", i, elem.get_voltage());
            }
        }
        
        // Two-phase approach: aggressive first, precise second
        let mut phase = 1;  // Start in aggressive phase
        
        // Adaptive PID controller - moderately aggressive for phase 1
        let mut pid = AdaptivePIDController::new(
            2.0,   // Phase 1: Higher Kp for faster progress
            0.4,   // Phase 1: Higher Ki for convergence  
            0.01   // Phase 1: Low Kd to avoid oscillation
        );
        
        // Ramping with adaptive PID control
        let mut ramp_factor = 0.0;
        let mut ramp_rate = 0.1;  // Moderately aggressive start
        
        // Track gradient for adaptation
        let mut last_current: f64 = 1e-15;  // Small non-zero to avoid ln(0)
        let mut last_voltage: f64 = 0.0;
        let mut log_gradient: f64 = 20.0;   // Default gradient (normal sensitivity)
        
        println!("\nStarting ramping loop...");
        let mut ramp_step = 0;
        
        while ramp_factor < 1.0 {
            ramp_step += 1;
            
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Debug output for first few steps
            if ramp_step <= 5 || (ramp_factor > 0.09 && ramp_factor < 0.11) {
                println!("\n--- Ramp step {} ---", ramp_step);
                println!("Ramp factor: {:.3}, Rate: {:.4}", ramp_factor, ramp_rate);
                println!("Vsource: {:.3}V", vsources[0].1 * ramp_factor);
            }
            
            // Solve at current ramp
            let (converged, iters, error) = self.solve_to_convergence(&mut total_iterations);
            
            if ramp_step <= 5 || (ramp_factor > 0.09 && ramp_factor < 0.11) {
                println!("Converged: {}, Iterations: {}, Error: {:.2e}", converged, iters, error);
            }
            
            if converged && !self.nonlinear_elements.is_empty() {
                // Get device current
                let elem_idx = self.nonlinear_elements[0];
                let mut element_voltage = 0.0;
                
                for &(conn_elem, pos, neg) in &self.connections {
                    if conn_elem == elem_idx {
                        element_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                
                let current = self.elements[elem_idx].current_at_voltage(element_voltage);
                
                // Debug output for gradient calculation
                if ramp_step <= 5 || (ramp_factor > 0.09 && ramp_factor < 0.11) {
                    println!("Diode voltage: {:.3}V, current: {:.2e}A", element_voltage, current);
                }
                
                // Calculate log gradient for adaptation
                if element_voltage > last_voltage + 1e-6 && current > 1e-15 {
                    let dv = element_voltage - last_voltage;
                    let dlog_i = current.ln() - last_current.ln();
                    log_gradient = (dlog_i / dv).abs();
                    
                    if ramp_step <= 5 || (ramp_factor > 0.09 && ramp_factor < 0.11) {
                        println!("Log gradient: {:.2}", log_gradient);
                    }
                }
                
                // Phase switching logic - only switch after significant progress
                if phase == 1 && ramp_factor > 0.9 && error < 1e-10 {
                    println!("\n  → Switching to precision phase at ramp={:.3}", ramp_factor);
                    phase = 2;
                    // Reset PID with precision parameters
                    pid = AdaptivePIDController::new(
                        1.0,   // Phase 2: Moderate Kp for stability
                        0.2,   // Phase 2: Moderate Ki for precision
                        0.02   // Phase 2: Small Kd
                    );
                    ramp_rate = 0.02; // Slow down for precision
                }
                
                // Adapt PID gains based on log gradient
                pid.adapt_gains(log_gradient);
                
                // Use PID to control ramp rate based on convergence error
                let target_error = if phase == 1 { 1e-11 } else { 1e-15 };
                let error_ratio = (error / target_error).ln().max(-10.0).min(10.0);
                let pid_output = pid.update(error_ratio, 0.01);
                
                // Convert PID output to rate multiplier
                let rate_multiplier = (-pid_output * 0.1).exp();
                ramp_rate *= rate_multiplier;
                
                // Bounds on ramp rate (adaptive based on phase and sensitivity)
                let (min_rate, max_rate) = if phase == 1 {
                    // Phase 1: Fast but controlled ramping
                    (1e-4, 0.2)
                } else {
                    // Phase 2: Precision bounds
                    let min = if log_gradient < 5.0 { 1e-5 } else { 1e-6 };
                    let max = if log_gradient > 50.0 { 0.05 } else { 0.1 };
                    (min, max)
                };
                ramp_rate = ramp_rate.max(min_rate).min(max_rate);
                
                // In phase 1, ensure minimum convergence before moving on
                if phase == 1 && error > 1e-10 {
                    ramp_rate *= 0.5; // Slow down if not converged well
                }
                
                // Update tracking variables
                last_voltage = element_voltage;
                last_current = current;
                
                // Exit conditions - DON'T exit early, keep refining
                if error < 1e-16 && ramp_factor > 0.999 {
                    println!("  → Ultra-precision achieved! error={:.2e}", error);
                    ramp_factor = 1.0;
                    break;
                }
            } else if !converged {
                // Failed to converge, reduce rate
                ramp_rate *= 0.5;
                println!("  → Convergence failed at ramp={:.3}, reducing rate to {:.4}", 
                         ramp_factor, ramp_rate);
            }
            
            ramp_factor += ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        println!("\nRamping complete after {} steps", ramp_step);
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        let (converged, iters, final_error) = self.solve_to_convergence(&mut total_iterations);
        
        println!("\nFinal solve: converged={}, error={:.2e}", converged, final_error);
        
        // Always do final convergence push for precision
        println!("  → Final convergence push...");
        let mut best_error = final_error;
        for pass in 0..20 {
            let (converged, iters, error) = self.solve_to_convergence(&mut total_iterations);
            if error < best_error {
                best_error = error;
            }
            if error < 1e-16 || (pass > 10 && error < 1e-15) {
                break;
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (self.node_voltages.clone(), elapsed, total_iterations)
    }
    
    fn solve_to_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize, f64) {
        let max_iter = 50;
        let tol = 1e-12;
        let mut iterations = 0;
        let mut last_error = 0.0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            // Debug MNA system for first iteration of first few ramp steps
            if iter == 0 && *total_iterations <= 5 {
                println!("\n  MNA System (iter 0):");
                println!("  Matrix size: {}x{}", a.nrows(), a.ncols());
                if a.nrows() <= 4 {
                    println!("  A matrix:\n{}", a);
                    println!("  b vector: {}", b.transpose());
                }
            }
            
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
                println!("  LU decomposition failed!");
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
    println!("=== DEBUG SIMPLE PID-CONTROLLED RAMPING ===");
    println!("Testing simple diode circuit\n");
    
    // Test simple circuit: 1V -> 100Ω -> Diode -> GND
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.026;
    
    println!("Circuit parameters:");
    println!("  Voltage source: {}V", vs);
    println!("  Resistor: {}Ω", rs);
    println!("  Diode: Is={:.2e}A, Vt={}V", is, vt);
    
    let mut solver = DebugPIDRampingSolver::new(3);
    
    let vs_idx = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r_idx = solver.add_element(Box::new(Resistor::new(rs)));
    let d_idx = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(vs_idx, 1, 0);  // VS between nodes 1 and 0
    solver.connect(r_idx, 1, 2);   // R between nodes 1 and 2
    solver.connect(d_idx, 2, 0);   // D between nodes 2 and 0
    
    let (voltages, time, iterations) = solver.solve_with_pid();
    
    let (vd_ref, id_ref) = analytical_reference(vs, rs, is, vt);
    let vd_computed = voltages[2];
    let id_computed = (voltages[1] - voltages[2]) / rs;
    
    let v_err = ((vd_computed - vd_ref) / vd_ref * 100.0).abs();
    let i_err = ((id_computed - id_ref) / id_ref * 100.0).abs();
    let max_err = v_err.max(i_err);
    
    println!("\n=== RESULTS ===");
    println!("Node voltages: {:?}", voltages);
    println!("Diode voltage: {:.6}V (ref: {:.6}V)", vd_computed, vd_ref);
    println!("Diode current: {:.6}A (ref: {:.6}A)", id_computed, id_ref);
    println!("Error: {:.3}%", max_err);
    println!("Time: {:.1}ms", time);
    println!("Total iterations: {}", iterations);
}