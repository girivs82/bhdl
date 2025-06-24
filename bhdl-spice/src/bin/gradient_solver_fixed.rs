/// Fixed Gradient Solver - Addressing Instability Issues
/// 
/// Based on the analysis, this implements a stable version of the
/// second-order gradient adaptive timestep method

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

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

// Fixed history tracking with better numerical stability
#[derive(Clone)]
struct StableNodeHistory {
    voltages: VecDeque<f64>,
    timestamps: VecDeque<f64>,
    voltage_scale: f64, // For normalization
}

impl StableNodeHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(5),
            timestamps: VecDeque::with_capacity(5),
            voltage_scale: 1.0,
        }
    }
    
    fn add(&mut self, voltage: f64, time: f64) {
        self.voltages.push_back(voltage);
        self.timestamps.push_back(time);
        
        // Update scale based on voltage magnitude
        if voltage.abs() > 0.0 {
            self.voltage_scale = voltage.abs().max(self.voltage_scale);
        }
        
        if self.voltages.len() > 5 {
            self.voltages.pop_front();
            self.timestamps.pop_front();
        }
    }
    
    fn calculate_stable_gradients(&self) -> Option<(f64, f64, f64)> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        
        // Use more points for stability (if available)
        if n >= 4 {
            // Use 4-point finite difference for better accuracy
            let v3 = self.voltages[n-1];
            let v2 = self.voltages[n-2];
            let v1 = self.voltages[n-3];
            let v0 = self.voltages[n-4];
            
            let t3 = self.timestamps[n-1];
            let t2 = self.timestamps[n-2];
            let t1 = self.timestamps[n-3];
            let t0 = self.timestamps[n-4];
            
            // Check for minimum timestep separation
            let dt_min = 1e-12; // Based on numerical analysis
            if (t3 - t0) < 3.0 * dt_min {
                return None; // Too small timesteps
            }
            
            // First derivative using central difference
            let h = (t3 - t0) / 3.0;
            let dv_dt = (-v0 + 9.0*v1 - 9.0*v2 + v3) / (6.0 * h);
            
            // Second derivative using central difference
            let d2v_dt2 = (v0 - 2.0*v1 + 2.0*v2 - v3) / (2.0 * h * h);
            
            // Normalized curvature for stability
            let curvature = if self.voltage_scale > 1e-10 {
                d2v_dt2 / self.voltage_scale
            } else {
                d2v_dt2
            };
            
            Some((dv_dt, d2v_dt2, curvature))
        } else {
            // Fallback to 3-point
            let v2 = self.voltages[n-1];
            let v1 = self.voltages[n-2];
            let v0 = self.voltages[n-3];
            
            let t2 = self.timestamps[n-1];
            let t1 = self.timestamps[n-2];
            let t0 = self.timestamps[n-3];
            
            let dt1 = t2 - t1;
            let dt0 = t1 - t0;
            
            // Check minimum timestep
            if dt1 < 1e-12 || dt0 < 1e-12 {
                return None;
            }
            
            let dv_dt1 = (v2 - v1) / dt1;
            let dv_dt0 = (v1 - v0) / dt0;
            
            let dt_avg = (dt1 + dt0) / 2.0;
            let d2v_dt2 = (dv_dt1 - dv_dt0) / dt_avg;
            
            let curvature = if self.voltage_scale > 1e-10 {
                d2v_dt2 / self.voltage_scale
            } else {
                d2v_dt2
            };
            
            Some((dv_dt1, d2v_dt2, curvature))
        }
    }
}

// Improved adaptive controller
struct StableAdaptiveController {
    current_dt: f64,
    min_dt: f64,
    max_dt: f64,
    // Use log-scale thresholds for better stability
    log_threshold_high: f64,
    log_threshold_low: f64,
    confidence_counter: i32,
    last_log_curvature: f64,
}

impl StableAdaptiveController {
    fn new() -> Self {
        Self {
            current_dt: 1e-9,      // Start conservatively
            min_dt: 1e-12,         // Based on numerical analysis
            max_dt: 1e-6,          // Reasonable upper bound
            log_threshold_high: 3.0,  // log10(1000)
            log_threshold_low: 0.0,   // log10(1)
            confidence_counter: 0,
            last_log_curvature: 0.0,
        }
    }
    
    fn update(&mut self, curvature: f64) -> f64 {
        // Work in log scale for better numerical behavior
        let log_curv = if curvature.abs() > 1e-30 {
            curvature.abs().log10()
        } else {
            -30.0 // Floor value
        };
        
        // Determine trend with hysteresis
        let increasing = log_curv > self.last_log_curvature + 0.5;
        let decreasing = log_curv < self.last_log_curvature - 0.5;
        
        let mut decision = 0;
        
        if log_curv > self.log_threshold_high && increasing {
            decision = -1; // Need smaller timestep
        } else if log_curv < self.log_threshold_low || 
                  (decreasing && log_curv < self.log_threshold_high - 1.0) {
            decision = 1;  // Can use larger timestep
        }
        
        // Update confidence
        if decision != 0 {
            if (decision < 0 && self.confidence_counter < 0) || 
               (decision > 0 && self.confidence_counter > 0) {
                self.confidence_counter += decision;
            } else {
                self.confidence_counter = decision;
            }
        } else {
            // Decay confidence slowly
            if self.confidence_counter > 0 {
                self.confidence_counter -= 1;
            } else if self.confidence_counter < 0 {
                self.confidence_counter += 1;
            }
        }
        
        // Change timestep with confidence
        if self.confidence_counter.abs() >= 3 {
            if self.confidence_counter < 0 {
                // Decrease by sqrt(10) instead of 10 for smoother adaptation
                self.current_dt = (self.current_dt / 3.16).max(self.min_dt);
                println!("  Decreasing timestep to {:e} (log_curvature: {:.2})", 
                         self.current_dt, log_curv);
            } else {
                // Increase by sqrt(10)
                self.current_dt = (self.current_dt * 3.16).min(self.max_dt);
                println!("  Increasing timestep to {:e} (log_curvature: {:.2})", 
                         self.current_dt, log_curv);
            }
            self.confidence_counter = 0;
        }
        
        self.last_log_curvature = log_curv;
        self.current_dt
    }
}

// Main solver with fixes
pub struct FixedGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    node_histories: Vec<StableNodeHistory>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    time: f64,
    controller: StableAdaptiveController,
}

impl FixedGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            node_histories: vec![StableNodeHistory::new(); num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            time: 0.0,
            controller: StableAdaptiveController::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn stable_gradient_dc_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        println!("\nStable Gradient-Based DC Analysis");
        
        // Count voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        let ramp_steps = 100;
        
        // Get voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Source ramping
        for ramp in 0..=ramp_steps {
            let factor = ramp as f64 / ramp_steps as f64;
            
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * factor);
            }
            
            // Reset time for each ramp
            self.time = 0.0;
            let max_time = 1e-3; // 1ms max per ramp
            
            // Time-stepping loop with gradient monitoring
            while self.time < max_time {
                total_iterations += 1;
                
                // Get current timestep
                let dt = self.controller.current_dt;
                
                // Newton-Raphson at current time
                let converged = self.solve_newton_raphson();
                
                if !converged {
                    println!("  Warning: Newton-Raphson failed at t={:e}", self.time);
                    // Reduce timestep and retry
                    self.controller.current_dt = (self.controller.current_dt / 10.0)
                        .max(self.controller.min_dt);
                    continue;
                }
                
                // Update histories
                for i in 0..self.num_nodes {
                    self.node_histories[i].add(self.node_voltages[i], self.time);
                }
                
                // Calculate gradients and update timestep
                let mut max_curvature = 0.0f64;
                let mut has_valid_gradient = false;
                
                for i in 1..self.num_nodes {
                    if let Some((_, _, curvature)) = self.node_histories[i].calculate_stable_gradients() {
                        max_curvature = max_curvature.max(curvature.abs());
                        has_valid_gradient = true;
                    }
                }
                
                // Only update timestep if we have valid gradients
                if has_valid_gradient {
                    self.controller.update(max_curvature);
                }
                
                // Advance time
                self.time += dt;
                
                // Check for steady state
                if ramp == ramp_steps {
                    let mut steady = true;
                    for i in 1..self.num_nodes {
                        if let Some((dv_dt, _, _)) = self.node_histories[i].calculate_stable_gradients() {
                            if dv_dt.abs() > 1e-6 {
                                steady = false;
                                break;
                            }
                        }
                    }
                    if steady {
                        println!("  Steady state reached at t={:e}", self.time);
                        break;
                    }
                }
            }
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        println!("  Final timestep: {:e}", self.controller.current_dt);
        println!("  Total iterations: {}", total_iterations);
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_newton_raphson(&mut self) -> bool {
        let max_iter = 20;
        let tol = 1e-10;
        let damping = 0.8;
        
        for _iter in 0..max_iter {
            let old_v = self.node_voltages.clone();
            let old_i = self.source_currents.clone();
            
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Update with damping
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                // Update source currents
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                // Update element states
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    return true;
                }
            } else {
                return false;
            }
        }
        
        false
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        // Add GMIN for stability
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

fn main() {
    println!("=== FIXED GRADIENT SOLVER ===");
    
    // SPICE reference
    let is = 1e-12;
    let vt = 0.026;
    let mut vd_spice = 0.7f64;
    
    for _ in 0..100 {
        let id = is * ((vd_spice / vt).exp() - 1.0);
        let f = vd_spice + id * 100.0 - 1.0;
        let g = 1.0 + (is / vt) * (vd_spice / vt).exp() * 100.0;
        let delta = f / g;
        vd_spice -= delta;
        if delta.abs() < 1e-15 {
            break;
        }
    }
    
    let id_spice = (1.0 - vd_spice) / 100.0;
    
    println!("\nSPICE Reference:");
    println!("  Vd = {:.9} V", vd_spice);
    println!("  Id = {:.9} mA\n", id_spice * 1000.0);
    
    // Test fixed gradient solver
    let mut solver = FixedGradientSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(1.0)));
    let r = solver.add_element(Box::new(Resistor::new(100.0)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    let (vd, id, iterations, time) = solver.stable_gradient_dc_analysis();
    
    let v_err = ((vd - vd_spice) / vd_spice * 100.0).abs();
    let i_err = ((id - id_spice) / id_spice * 100.0).abs();
    
    println!("\nFixed Gradient Solver Results:");
    println!("  Vd = {:.9} V (error: {:.3}%)", vd, v_err);
    println!("  Id = {:.9} mA (error: {:.3}%)", id * 1000.0, i_err);
    println!("  Time: {:.1} ms", time);
    
    println!("\n=== ANALYSIS ===");
    if v_err < 5.0 && i_err < 5.0 {
        println!("✓ SUCCESS: Achieved <5% accuracy!");
        println!("\nKey fixes applied:");
        println!("1. Minimum timestep limit (1e-12) to avoid numerical issues");
        println!("2. Normalized curvature by voltage scale");
        println!("3. Log-scale thresholds for better stability");
        println!("4. Smoother timestep adaptation (sqrt(10) instead of 10x)");
        println!("5. 4-point finite difference when possible");
        println!("6. GMIN for matrix conditioning");
    } else {
        println!("Accuracy: {:.2}%", v_err.max(i_err));
    }
}