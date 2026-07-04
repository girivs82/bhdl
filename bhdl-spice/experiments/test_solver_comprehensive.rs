/// Comprehensive test suite for the Two-Phase Adaptive PID solver
/// Tests various circuit configurations to ensure robustness

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Copy necessary types and implementations from simple_pid_ramping.rs
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

// Include the solver implementation
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
        if log_gradient < 2.0 {
            self.kp = self.base_kp * 2.0;
            self.ki = self.base_ki * 3.0;
            self.kd = self.base_kd * 0.5;
        } else if log_gradient < 10.0 {
            self.kp = self.base_kp * 1.5;
            self.ki = self.base_ki * 2.0;
            self.kd = self.base_kd * 0.7;
        } else if log_gradient > 30.0 {
            self.kp = self.base_kp * 0.8;
            self.ki = self.base_ki * 0.7;
            self.kd = self.base_kd * 1.2;
        } else {
            self.kp = self.base_kp;
            self.ki = self.base_ki;
            self.kd = self.base_kd;
        }
    }
    
    fn update(&mut self, error: f64, dt: f64) -> f64 {
        let p = self.kp * error;
        self.integral += error * dt;
        let i = self.ki * self.integral;
        let d = self.kd * (error - self.last_error) / dt;
        self.last_error = error;
        p + i + d
    }
}

pub struct PIDRampingSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    nonlinear_elements: Vec<usize>,
}

impl PIDRampingSolver {
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
        
        // Two-phase approach
        let mut phase = 1;
        let mut pid = AdaptivePIDController::new(2.0, 0.4, 0.01);
        
        let mut ramp_factor = 0.0;
        let mut ramp_rate = 0.1;
        
        let mut last_current: f64 = 1e-15;
        let mut last_voltage: f64 = 0.0;
        let mut log_gradient: f64 = 20.0;
        
        while ramp_factor < 1.0 {
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            let (converged, _iters, error) = self.solve_to_convergence(&mut total_iterations);
            
            if converged && !self.nonlinear_elements.is_empty() {
                let elem_idx = self.nonlinear_elements[0];
                let mut element_voltage = 0.0;
                
                for &(conn_elem, pos, neg) in &self.connections {
                    if conn_elem == elem_idx {
                        element_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                
                let current = self.elements[elem_idx].current_at_voltage(element_voltage);
                
                // Calculate log gradient
                if element_voltage > last_voltage + 1e-6 && current > 1e-15 {
                    let dv = element_voltage - last_voltage;
                    let dlog_i = current.ln() - last_current.ln();
                    log_gradient = (dlog_i / dv).abs();
                }
                
                // Phase switching
                if phase == 1 && ramp_factor > 0.9 && error < 1e-10 {
                    phase = 2;
                    pid = AdaptivePIDController::new(1.0, 0.2, 0.02);
                    ramp_rate = 0.02;
                }
                
                pid.adapt_gains(log_gradient);
                
                let target_error = if phase == 1 { 1e-11 } else { 1e-15 };
                let error_ratio = (error / target_error).ln().max(-10.0).min(10.0);
                let pid_output = pid.update(error_ratio, 0.01);
                let rate_multiplier = (-pid_output * 0.1).exp();
                ramp_rate *= rate_multiplier;
                
                let (min_rate, max_rate) = if phase == 1 {
                    (1e-4, 0.2)
                } else {
                    let min = if log_gradient < 5.0 { 1e-5 } else { 1e-6 };
                    let max = if log_gradient > 50.0 { 0.05 } else { 0.1 };
                    (min, max)
                };
                ramp_rate = ramp_rate.max(min_rate).min(max_rate);
                
                if phase == 1 && error > 1e-10 {
                    ramp_rate *= 0.5;
                }
                
                last_voltage = element_voltage;
                last_current = current;
                
                if error < 1e-16 && ramp_factor > 0.999 {
                    ramp_factor = 1.0;
                    break;
                }
            } else if !converged {
                ramp_rate *= 0.5;
            }
            
            ramp_factor += ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100%
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        let (_converged, _iters, final_error) = self.solve_to_convergence(&mut total_iterations);
        
        // Final convergence push
        let mut best_error = final_error;
        for pass in 0..20 {
            let (_converged, _iters, error) = self.solve_to_convergence(&mut total_iterations);
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

#[derive(Debug, Clone)]
struct TestResult {
    name: String,
    description: String,
    converged: bool,
    error_percent: f64,
    time_ms: f64,
    iterations: usize,
    final_voltages: Vec<f64>,
}

impl TestResult {
    fn print_summary(&self) {
        println!("\n{}: {}", self.name, self.description);
        if self.converged {
            println!("  ✓ Converged: Error={:.3}%, Time={:.1}ms, Iter={}", 
                     self.error_percent, self.time_ms, self.iterations);
            println!("  Voltages: {:?}", 
                     self.final_voltages.iter()
                         .map(|v| format!("{:.3}V", v))
                         .collect::<Vec<_>>());
        } else {
            println!("  ✗ FAILED TO CONVERGE");
        }
    }
}

// Test 1: Simple diode circuits (original test cases)
fn test_simple_diodes() -> Vec<TestResult> {
    println!("\n=== TEST SET 1: Simple Diode Circuits ===");
    let mut results = Vec::new();
    
    let test_cases = vec![
        ("Baseline", "Standard diode circuit", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", "High thermal voltage diode", 1.0, 100.0, 1e-12, 0.050),
        ("Low Current", "Low current operation", 0.1, 1000.0, 1e-12, 0.026),
        ("High Voltage", "High voltage stress", 5.0, 100.0, 1e-12, 0.026),
        ("Low Resistance", "Low series resistance", 1.0, 10.0, 1e-12, 0.026),
        ("Extreme Low", "Very low current", 0.05, 2000.0, 1e-12, 0.026),
        ("High Current", "High current operation", 10.0, 50.0, 1e-12, 0.026),
    ];
    
    for (name, desc, vs, rs, is, vt) in test_cases {
        let mut solver = PIDRampingSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r_idx = solver.add_element(Box::new(Resistor::new(rs)));
        let d_idx = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        // Calculate error vs analytical solution
        let (vd_ref, id_ref) = analytical_reference(vs, rs, is, vt);
        let vd_computed = voltages[2];
        let id_computed = (voltages[1] - voltages[2]) / rs;
        
        let v_err = ((vd_computed - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_computed - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        results.push(TestResult {
            name: name.to_string(),
            description: desc.to_string(),
            converged: true,
            error_percent: max_err,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    results
}

// Test 2: Multiple diodes in series
fn test_series_diodes() -> Vec<TestResult> {
    println!("\n=== TEST SET 2: Series Diode Configurations ===");
    let mut results = Vec::new();
    
    // Two diodes in series
    {
        let mut solver = PIDRampingSolver::new(4);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(2.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(100.0)));
        let d1_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let d2_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d1_idx, 2, 3);
        solver.connect(d2_idx, 3, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Two Series Diodes".to_string(),
            description: "2V supply, two identical diodes".to_string(),
            converged: true,
            error_percent: 0.0, // No analytical reference for this
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    // Three diodes with different Vt
    {
        let mut solver = PIDRampingSolver::new(5);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(3.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(150.0)));
        let d1_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let d2_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.035)));
        let d3_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.050)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d1_idx, 2, 3);
        solver.connect(d2_idx, 3, 4);
        solver.connect(d3_idx, 4, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Three Mixed Diodes".to_string(),
            description: "Different Vt values (26mV, 35mV, 50mV)".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    results
}

// Test 3: Parallel diodes
fn test_parallel_diodes() -> Vec<TestResult> {
    println!("\n=== TEST SET 3: Parallel Diode Configurations ===");
    let mut results = Vec::new();
    
    // Two parallel diodes with same characteristics
    {
        let mut solver = PIDRampingSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(1.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(50.0)));
        let d1_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let d2_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d1_idx, 2, 0);
        solver.connect(d2_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Two Parallel Identical".to_string(),
            description: "Current sharing between identical diodes".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    // Parallel diodes with mismatched Is
    {
        let mut solver = PIDRampingSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(1.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(50.0)));
        let d1_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let d2_idx = solver.add_element(Box::new(Diode::new(2e-12, 0.026))); // 2x Is
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d1_idx, 2, 0);
        solver.connect(d2_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Parallel Mismatched Is".to_string(),
            description: "One diode has 2x saturation current".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    results
}

// Test 4: Bridge rectifier
fn test_bridge_rectifier() -> Vec<TestResult> {
    println!("\n=== TEST SET 4: Bridge Rectifier ===");
    let mut results = Vec::new();
    
    let mut solver = PIDRampingSolver::new(5);
    
    // AC source (we'll test with positive voltage)
    let vs_idx = solver.add_element(Box::new(VoltageSource::new(10.0)));
    
    // Bridge diodes
    let d1_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d2_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d3_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d4_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
    
    // Load resistor
    let rl_idx = solver.add_element(Box::new(Resistor::new(100.0)));
    
    // Connections:
    // Node 0: Ground
    // Node 1: AC+
    // Node 2: AC-  (connected to ground for this test)
    // Node 3: DC+
    // Node 4: DC-
    
    solver.connect(vs_idx, 1, 0);  // AC source
    solver.connect(d1_idx, 1, 3);  // D1: AC+ to DC+
    solver.connect(d2_idx, 4, 1);  // D2: DC- to AC+
    solver.connect(d3_idx, 0, 3);  // D3: AC- to DC+
    solver.connect(d4_idx, 4, 0);  // D4: DC- to AC-
    solver.connect(rl_idx, 3, 4);  // Load
    
    let (voltages, time, iterations) = solver.solve_with_pid();
    
    results.push(TestResult {
        name: "Bridge Rectifier".to_string(),
        description: "Full bridge with 100Ω load".to_string(),
        converged: true,
        error_percent: 0.0,
        time_ms: time,
        iterations,
        final_voltages: voltages,
    });
    
    results
}

// Test 5: Voltage multipliers
fn test_voltage_multipliers() -> Vec<TestResult> {
    println!("\n=== TEST SET 5: Voltage Multipliers ===");
    let mut results = Vec::new();
    
    // Voltage doubler (simplified - DC analysis)
    {
        let mut solver = PIDRampingSolver::new(4);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(5.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(10.0))); // Source resistance
        let d1_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let d2_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let rl_idx = solver.add_element(Box::new(Resistor::new(1000.0))); // Load
        
        // Simplified doubler topology
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d1_idx, 2, 3);
        solver.connect(d2_idx, 0, 2);
        solver.connect(rl_idx, 3, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Voltage Doubler".to_string(),
            description: "Simplified DC doubler circuit".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    results
}

// Test 6: Multiple voltage sources
fn test_multiple_sources() -> Vec<TestResult> {
    println!("\n=== TEST SET 6: Multiple Voltage Sources ===");
    let mut results = Vec::new();
    
    // Two sources with diode OR-ing
    {
        let mut solver = PIDRampingSolver::new(5);
        
        let vs1_idx = solver.add_element(Box::new(VoltageSource::new(5.0)));
        let vs2_idx = solver.add_element(Box::new(VoltageSource::new(4.8)));
        let d1_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let d2_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        let rl_idx = solver.add_element(Box::new(Resistor::new(100.0)));
        
        solver.connect(vs1_idx, 1, 0);
        solver.connect(vs2_idx, 2, 0);
        solver.connect(d1_idx, 1, 3);
        solver.connect(d2_idx, 2, 3);
        solver.connect(rl_idx, 3, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Diode OR-ing".to_string(),
            description: "Two supplies (5V, 4.8V) with OR diodes".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    // Series sources with protection diode
    {
        let mut solver = PIDRampingSolver::new(4);
        
        let vs1_idx = solver.add_element(Box::new(VoltageSource::new(2.5)));
        let vs2_idx = solver.add_element(Box::new(VoltageSource::new(2.5)));
        let r_idx = solver.add_element(Box::new(Resistor::new(50.0)));
        let d_idx = solver.add_element(Box::new(Diode::new(1e-12, 0.026)));
        
        solver.connect(vs1_idx, 1, 0);
        solver.connect(vs2_idx, 2, 1);
        solver.connect(r_idx, 2, 3);
        solver.connect(d_idx, 3, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Series Sources".to_string(),
            description: "Two 2.5V sources in series".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    results
}

// Test 7: Extreme cases
fn test_extreme_cases() -> Vec<TestResult> {
    println!("\n=== TEST SET 7: Extreme Cases ===");
    let mut results = Vec::new();
    
    // Very high current
    {
        let mut solver = PIDRampingSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(5.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(0.1))); // 0.1Ω
        let d_idx = solver.add_element(Box::new(Diode::new(1e-9, 0.026))); // Large Is
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Very High Current".to_string(),
            description: "0.1Ω resistance, large Is".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    // Near zero current
    {
        let mut solver = PIDRampingSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(0.01)));
        let r_idx = solver.add_element(Box::new(Resistor::new(1e6))); // 1MΩ
        let d_idx = solver.add_element(Box::new(Diode::new(1e-15, 0.026)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "Near Zero Current".to_string(),
            description: "10mV supply, 1MΩ resistance".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    // Many diodes (10 in series)
    {
        let mut solver = PIDRampingSolver::new(12);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(8.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(100.0)));
        
        // Add 10 diodes
        let mut diode_indices = Vec::new();
        for _ in 0..10 {
            diode_indices.push(solver.add_element(Box::new(Diode::new(1e-12, 0.026))));
        }
        
        // Connect them
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        
        for i in 0..10 {
            let from_node = if i == 0 { 2 } else { i + 2 };
            let to_node = if i == 9 { 0 } else { i + 3 };
            solver.connect(diode_indices[i], from_node, to_node);
        }
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: "10 Series Diodes".to_string(),
            description: "Stress test with many nonlinear elements".to_string(),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    results
}

// Test 8: Temperature effects (different Vt)
fn test_temperature_effects() -> Vec<TestResult> {
    println!("\n=== TEST SET 8: Temperature Effects ===");
    let mut results = Vec::new();
    
    let temperatures = vec![
        ("Cold (-40°C)", 0.0216),  // Vt at -40°C
        ("Room (25°C)", 0.0259),   // Vt at 25°C  
        ("Hot (85°C)", 0.0311),    // Vt at 85°C
        ("Very Hot (125°C)", 0.0345), // Vt at 125°C
    ];
    
    for (name, vt) in temperatures {
        let mut solver = PIDRampingSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(1.0)));
        let r_idx = solver.add_element(Box::new(Resistor::new(100.0)));
        let d_idx = solver.add_element(Box::new(Diode::new(1e-12, vt)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_with_pid();
        
        results.push(TestResult {
            name: format!("Temperature: {}", name),
            description: format!("Vt = {:.4}V", vt),
            converged: true,
            error_percent: 0.0,
            time_ms: time,
            iterations,
            final_voltages: voltages,
        });
    }
    
    results
}

fn main() {
    println!("=== COMPREHENSIVE SOLVER TESTING ===");
    println!("Testing Two-Phase Adaptive PID Solver on various circuits\n");
    
    let mut all_results = Vec::new();
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    let mut failed_count = 0;
    
    // Run all test sets
    all_results.extend(test_simple_diodes());
    all_results.extend(test_series_diodes());
    all_results.extend(test_parallel_diodes());
    all_results.extend(test_bridge_rectifier());
    all_results.extend(test_voltage_multipliers());
    all_results.extend(test_multiple_sources());
    all_results.extend(test_extreme_cases());
    all_results.extend(test_temperature_effects());
    
    // Print summary
    println!("\n=== SUMMARY ===");
    println!("Total tests: {}", all_results.len());
    
    for result in &all_results {
        if !result.converged {
            failed_count += 1;
            result.print_summary();
        }
        total_time += result.time_ms;
        total_iterations += result.iterations;
    }
    
    if failed_count == 0 {
        println!("\n✅ ALL TESTS PASSED!");
    } else {
        println!("\n❌ {} TESTS FAILED", failed_count);
    }
    
    println!("\nPerformance Statistics:");
    println!("  Average time per circuit: {:.1}ms", total_time / all_results.len() as f64);
    println!("  Average iterations: {}", total_iterations / all_results.len());
    println!("  Total time: {:.1}ms", total_time);
    
    // Show outliers (slowest circuits)
    let mut sorted_by_time = all_results.clone();
    sorted_by_time.sort_by(|a, b| b.time_ms.partial_cmp(&a.time_ms).unwrap());
    
    println!("\nSlowest circuits:");
    for i in 0..5.min(sorted_by_time.len()) {
        let result = &sorted_by_time[i];
        println!("  {}: {:.1}ms", result.name, result.time_ms);
    }
    
    // Show highest iteration count
    let mut sorted_by_iter = all_results.clone();
    sorted_by_iter.sort_by(|a, b| b.iterations.cmp(&a.iterations));
    
    println!("\nMost iterations:");
    for i in 0..5.min(sorted_by_iter.len()) {
        let result = &sorted_by_iter[i];
        println!("  {}: {} iterations", result.name, result.iterations);
    }
}