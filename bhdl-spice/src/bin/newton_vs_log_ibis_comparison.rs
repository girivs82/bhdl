/// Newton vs Logarithmic Gradient IBIS Model Comparison
/// 
/// Direct head-to-head performance comparison using identical IBIS models
/// to determine which approach is superior for industry-standard buffer simulation.
/// Both solvers use the same I-V data source, ensuring fair comparison.

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Common element trait
pub trait Element: Send + Sync {
    fn element_type(&self) -> ElementType;
    fn conductance(&self) -> f64 { 0.0 }
    fn is_nonlinear(&self) -> bool { false }
    fn current_at_voltage(&self, v: f64) -> f64;
    fn conductance_at_voltage(&self, v: f64) -> f64;
    fn get_voltage(&self) -> f64;
    fn set_voltage(&mut self, v: f64);
    fn clone_element(&self) -> Box<dyn Element>;
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ElementType {
    Resistor,
    VoltageSource,
    IBISBuffer,
}

// Resistor implementation (same for both solvers)
#[derive(Clone)]
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
    fn clone_element(&self) -> Box<dyn Element> { Box::new(self.clone()) }
}

// Voltage source implementation (same for both solvers)
#[derive(Clone)]
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
    fn clone_element(&self) -> Box<dyn Element> { Box::new(self.clone()) }
}

// IBIS Buffer implementation (same I-V data for both solvers)
#[derive(Clone)]
pub struct IBISBuffer {
    name: String,
    pullup_voltages: Vec<f64>,
    pullup_currents: Vec<f64>,
    pulldown_voltages: Vec<f64>,
    pulldown_currents: Vec<f64>,
    power_clamp_voltages: Vec<f64>,
    power_clamp_currents: Vec<f64>,
    ground_clamp_voltages: Vec<f64>,
    ground_clamp_currents: Vec<f64>,
    vcc: f64,
    vss: f64,
    voltage: f64,
    is_output_enabled: bool,
}

impl IBISBuffer {
    // Create standardized 3.3V CMOS buffer for fair comparison
    pub fn new_standard_3v3(name: &str) -> Self {
        let pullup_voltages = vec![
            -2.0, -1.0, 0.0, 1.0, 2.0, 2.5, 3.0, 3.3, 3.6, 4.0, 5.0
        ];
        let pullup_currents = vec![
            -0.100, -0.080, -0.060, -0.040, -0.025, -0.015, -0.008, -0.003, -0.001, 0.000, 0.002
        ];
        
        let pulldown_voltages = vec![
            -1.0, -0.5, 0.0, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 3.3, 4.0
        ];
        let pulldown_currents = vec![
            -0.001, 0.000, 0.001, 0.005, 0.015, 0.030, 0.050, 0.075, 0.105, 0.120, 0.150
        ];
        
        let power_clamp_voltages = vec![3.3, 3.5, 3.8, 4.0, 4.2, 4.5, 5.0];
        let power_clamp_currents = vec![0.000, -0.001, -0.010, -0.025, -0.050, -0.100, -0.200];
        
        let ground_clamp_voltages = vec![-1.0, -0.7, -0.5, -0.3, 0.0, 0.3];
        let ground_clamp_currents = vec![0.200, 0.100, 0.050, 0.010, 0.001, 0.000];
        
        Self {
            name: name.to_string(),
            pullup_voltages,
            pullup_currents,
            pulldown_voltages,
            pulldown_currents,
            power_clamp_voltages,
            power_clamp_currents,
            ground_clamp_voltages,
            ground_clamp_currents,
            vcc: 3.3,
            vss: 0.0,
            voltage: 0.0,
            is_output_enabled: true,
        }
    }
    
    // Linear interpolation helper
    fn interpolate(x_points: &[f64], y_points: &[f64], x: f64) -> f64 {
        if x_points.len() != y_points.len() || x_points.is_empty() {
            return 0.0;
        }
        
        if x <= x_points[0] {
            return y_points[0];
        }
        if x >= x_points[x_points.len() - 1] {
            return y_points[y_points.len() - 1];
        }
        
        for i in 0..(x_points.len() - 1) {
            if x >= x_points[i] && x <= x_points[i + 1] {
                let dx = x_points[i + 1] - x_points[i];
                let dy = y_points[i + 1] - y_points[i];
                let t = (x - x_points[i]) / dx;
                return y_points[i] + t * dy;
            }
        }
        
        0.0
    }
    
    // Calculate total IBIS current from all curves
    fn calculate_total_current(&self, v: f64) -> f64 {
        let mut total_current = 0.0;
        
        if self.is_output_enabled {
            let pullup_current = Self::interpolate(&self.pullup_voltages, &self.pullup_currents, v);
            let pulldown_current = Self::interpolate(&self.pulldown_voltages, &self.pulldown_currents, v);
            
            // Voltage-dependent weighting (simplified logic state)
            let high_weight = if v > self.vcc / 2.0 { 1.0 } else { 0.0 };
            let low_weight = 1.0 - high_weight;
            
            total_current += high_weight * pullup_current + low_weight * pulldown_current;
        }
        
        // ESD clamps (always active)
        if v > self.vcc + 0.3 {
            let power_clamp_current = Self::interpolate(&self.power_clamp_voltages, &self.power_clamp_currents, v);
            total_current += power_clamp_current;
        }
        
        if v < self.vss - 0.3 {
            let ground_clamp_current = Self::interpolate(&self.ground_clamp_voltages, &self.ground_clamp_currents, v);
            total_current += ground_clamp_current;
        }
        
        total_current
    }
    
    // Numerical conductance calculation (same for both solvers)
    fn calculate_conductance(&self, v: f64) -> f64 {
        let dv = 0.001; // 1mV step
        let i1 = self.calculate_total_current(v - dv / 2.0);
        let i2 = self.calculate_total_current(v + dv / 2.0);
        let conductance = (i2 - i1) / dv;
        conductance.max(1e-12) // Minimum for stability
    }
}

impl Element for IBISBuffer {
    fn element_type(&self) -> ElementType { ElementType::IBISBuffer }
    fn is_nonlinear(&self) -> bool { true }
    
    fn current_at_voltage(&self, v: f64) -> f64 {
        self.calculate_total_current(v)
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        self.calculate_conductance(v)
    }
    
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
    fn clone_element(&self) -> Box<dyn Element> { Box::new(self.clone()) }
}

// Logarithmic Gradient Solver (from our previous implementation)
#[derive(Clone)]
struct LogHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
}

impl LogHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(8),
            log_currents: VecDeque::with_capacity(8),
            ramp_factors: VecDeque::with_capacity(8),
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, ramp: f64) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.ramp_factors.push_back(ramp);
        
        if self.voltages.len() > 8 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
            self.ramp_factors.pop_front();
        }
    }
    
    fn calculate_sensitivity(&self) -> Option<f64> {
        if self.voltages.len() < 4 {
            return None;
        }
        
        let n = self.voltages.len();
        let mut sum_dv = 0.0;
        let mut sum_dlog_i = 0.0;
        let mut count = 0;
        
        for i in 1..n {
            let dv = self.voltages[i] - self.voltages[i-1];
            if dv.abs() > 1e-12 {
                let dlog_i = self.log_currents[i] - self.log_currents[i-1];
                sum_dv += dv;
                sum_dlog_i += dlog_i;
                count += 1;
            }
        }
        
        if count > 0 && sum_dv.abs() > 1e-12 {
            Some(sum_dlog_i / sum_dv)
        } else {
            None
        }
    }
}

struct LogController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    target_sensitivity: f64,
}

impl LogController {
    fn new() -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.05,
            target_sensitivity: 10.0,
        }
    }
    
    fn update(&mut self, sensitivity: Option<f64>, converged: bool) {
        if !converged {
            self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
            return;
        }
        
        if let Some(sens) = sensitivity {
            let ratio = sens / self.target_sensitivity;
            
            if ratio > 2.0 {
                self.current_ramp_rate = (self.current_ramp_rate * 0.8).max(self.min_rate);
            } else if ratio < 0.3 {
                self.current_ramp_rate = (self.current_ramp_rate * 1.3).min(self.max_rate);
            } else {
                self.current_ramp_rate = (self.current_ramp_rate * 1.05).min(self.max_rate);
            }
        }
    }
}

pub struct LogarithmicGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    history: LogHistory,
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
            history: LogHistory::new(),
            controller: LogController::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn log_current_for_buffer(&self, voltage: f64, current: f64) -> f64 {
        let i_min = 1e-18;
        (current.abs() + i_min).ln()
    }
    
    pub fn solve_circuit(&mut self) -> (Vec<f64>, f64, usize, bool) {
        let start = Instant::now();
        
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
        
        // Find IBIS buffer for monitoring
        let mut ibis_element_idx = None;
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::IBISBuffer {
                ibis_element_idx = Some(i);
                break;
            }
        }
        
        // Adaptive ramping
        let mut ramp_factor = 0.0;
        let mut success = true;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            let (converged, iters) = self.solve_to_convergence(&mut total_iterations);
            
            if !converged {
                success = false;
                break;
            }
            
            // Update history with IBIS buffer state
            if let Some(ibis_idx) = ibis_element_idx {
                let mut buffer_voltage = 0.0;
                for &(elem_idx, pos, neg) in &self.connections {
                    if elem_idx == ibis_idx {
                        buffer_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                
                let buffer_current = self.elements[ibis_idx].current_at_voltage(buffer_voltage);
                let log_current = self.log_current_for_buffer(buffer_voltage, buffer_current);
                
                self.history.add_point(buffer_voltage, log_current, ramp_factor);
                
                let sensitivity = self.history.calculate_sensitivity();
                self.controller.update(sensitivity, converged);
            } else {
                self.controller.update(None, converged);
            }
            
            // Advance ramp
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            
            if total_iterations > 10000 { // Safety limit
                success = false;
                break;
            }
        }
        
        // Final solve at 100%
        if success {
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v);
            }
            let (final_converged, _) = self.solve_to_convergence(&mut total_iterations);
            success = final_converged;
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (self.node_voltages.clone(), elapsed, total_iterations, success)
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

// Newton-Raphson Solver with IBIS models
pub struct NewtonSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    max_newton_iterations: usize,
    newton_tolerance: f64,
}

impl NewtonSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            max_newton_iterations: 50,
            newton_tolerance: 1e-12,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve_circuit(&mut self) -> (Vec<f64>, f64, usize, bool) {
        let start = Instant::now();
        
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
        
        // Source stepping with Newton-Raphson
        let num_steps = 100;
        let mut success = true;
        
        for step in 0..=num_steps {
            let ramp_factor = step as f64 / num_steps as f64;
            
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Newton-Raphson iterations at this ramp level
            let (converged, iters) = self.newton_iterations(&mut total_iterations);
            
            if !converged {
                success = false;
                break;
            }
            
            if total_iterations > 10000 { // Safety limit
                success = false;
                break;
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (self.node_voltages.clone(), elapsed, total_iterations, success)
    }
    
    fn newton_iterations(&mut self, total_iterations: &mut usize) -> (bool, usize) {
        let mut iterations = 0;
        
        for iter in 0..self.max_newton_iterations {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Newton damping factor
                let damping = 0.5;
                
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
                
                if max_change < self.newton_tolerance {
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

// Test case structure
#[derive(Clone)]
struct IBISTestCase {
    name: String,
    supply_voltage: f64,
    series_resistance: f64,
    load_resistance: f64,
}

impl IBISTestCase {
    fn new(name: &str, vs: f64, rs: f64, rl: f64) -> Self {
        Self {
            name: name.to_string(),
            supply_voltage: vs,
            series_resistance: rs,
            load_resistance: rl,
        }
    }
}

// Comparison runner
fn run_comparison_test(test_case: &IBISTestCase) -> (
    (Vec<f64>, f64, usize, bool), // Log gradient results
    (Vec<f64>, f64, usize, bool)  // Newton results
) {
    println!("\n--- Testing {} ---", test_case.name);
    
    // Test Logarithmic Gradient Solver
    let mut log_solver = LogarithmicGradientSolver::new(3);
    
    let vs_log = log_solver.add_element(Box::new(VoltageSource::new(test_case.supply_voltage)));
    let rs_log = log_solver.add_element(Box::new(Resistor::new(test_case.series_resistance)));
    let buffer_log = log_solver.add_element(Box::new(IBISBuffer::new_standard_3v3("LogTest")));
    let rl_log = log_solver.add_element(Box::new(Resistor::new(test_case.load_resistance)));
    
    log_solver.connect(vs_log, 1, 0);
    log_solver.connect(rs_log, 1, 2);
    log_solver.connect(buffer_log, 2, 0);
    log_solver.connect(rl_log, 2, 0);
    
    let log_results = log_solver.solve_circuit();
    
    // Test Newton Solver
    let mut newton_solver = NewtonSolver::new(3);
    
    let vs_newton = newton_solver.add_element(Box::new(VoltageSource::new(test_case.supply_voltage)));
    let rs_newton = newton_solver.add_element(Box::new(Resistor::new(test_case.series_resistance)));
    let buffer_newton = newton_solver.add_element(Box::new(IBISBuffer::new_standard_3v3("NewtonTest")));
    let rl_newton = newton_solver.add_element(Box::new(Resistor::new(test_case.load_resistance)));
    
    newton_solver.connect(vs_newton, 1, 0);
    newton_solver.connect(rs_newton, 1, 2);
    newton_solver.connect(buffer_newton, 2, 0);
    newton_solver.connect(rl_newton, 2, 0);
    
    let newton_results = newton_solver.solve_circuit();
    
    (log_results, newton_results)
}

fn main() {
    println!("=== NEWTON vs LOGARITHMIC GRADIENT IBIS COMPARISON ===");
    println!("Direct performance comparison using identical IBIS buffer models");
    
    let test_cases = vec![
        IBISTestCase::new("Standard 3.3V Buffer", 3.3, 50.0, 50.0),
        IBISTestCase::new("High Drive Current", 3.3, 25.0, 25.0),
        IBISTestCase::new("Light Load", 3.3, 100.0, 200.0),
        IBISTestCase::new("Heavy Load", 3.3, 33.0, 10.0),
        IBISTestCase::new("High Voltage Supply", 5.0, 50.0, 50.0),
        IBISTestCase::new("Weak Drive", 2.5, 150.0, 100.0),
        IBISTestCase::new("Bus Interface", 3.3, 22.0, 47.0),
    ];
    
    let mut log_successes = 0;
    let mut newton_successes = 0;
    let mut log_total_time = 0.0;
    let mut newton_total_time = 0.0;
    let mut log_total_iterations = 0;
    let mut newton_total_iterations = 0;
    
    println!("\n{}", "=".repeat(80));
    println!("| {:20} | {:15} | {:15} | {:10} |", "Test Case", "Log Gradient", "Newton", "Winner");
    println!("{}", "=".repeat(80));
    
    for test_case in &test_cases {
        let ((log_voltages, log_time, log_iters, log_success), 
             (newton_voltages, newton_time, newton_iters, newton_success)) = run_comparison_test(test_case);
        
        let log_result = if log_success {
            log_successes += 1;
            log_total_time += log_time;
            log_total_iterations += log_iters;
            format!("{:.2}V, {:.1}ms", log_voltages[2], log_time)
        } else {
            "FAILED".to_string()
        };
        
        let newton_result = if newton_success {
            newton_successes += 1;
            newton_total_time += newton_time;
            newton_total_iterations += newton_iters;
            format!("{:.2}V, {:.1}ms", newton_voltages[2], newton_time)
        } else {
            "FAILED".to_string()
        };
        
        let winner = match (log_success, newton_success) {
            (true, true) => if log_time < newton_time { "LOG" } else { "NEWTON" },
            (true, false) => "LOG",
            (false, true) => "NEWTON",
            (false, false) => "BOTH FAIL",
        };
        
        println!("| {:20} | {:15} | {:15} | {:10} |", 
                 test_case.name, log_result, newton_result, winner);
    }
    
    println!("{}", "=".repeat(80));
    
    // Summary statistics
    println!("\n=== PERFORMANCE SUMMARY ===");
    println!("                         | Logarithmic Gradient | Newton Solver");
    println!("-------------------------|---------------------|----------------");
    println!("Success Rate             | {}/{} ({:.1}%)           | {}/{} ({:.1}%)", 
             log_successes, test_cases.len(), 
             log_successes as f64 / test_cases.len() as f64 * 100.0,
             newton_successes, test_cases.len(),
             newton_successes as f64 / test_cases.len() as f64 * 100.0);
    
    if log_successes > 0 {
        println!("Average Time             | {:.1}ms                | {:.1}ms", 
                 log_total_time / log_successes as f64,
                 if newton_successes > 0 { newton_total_time / newton_successes as f64 } else { 0.0 });
        println!("Average Iterations       | {:.0}                  | {:.0}", 
                 log_total_iterations as f64 / log_successes as f64,
                 if newton_successes > 0 { newton_total_iterations as f64 / newton_successes as f64 } else { 0.0 });
    }
    
    println!("\n=== ANALYSIS ===");
    
    if log_successes > newton_successes {
        println!("🏆 WINNER: Logarithmic Gradient Solver");
        println!("   - Higher success rate: {:.1}% vs {:.1}%", 
                 log_successes as f64 / test_cases.len() as f64 * 100.0,
                 newton_successes as f64 / test_cases.len() as f64 * 100.0);
        if log_successes > 0 && newton_successes > 0 {
            println!("   - Average time: {:.1}ms vs {:.1}ms", 
                     log_total_time / log_successes as f64,
                     newton_total_time / newton_successes as f64);
        }
    } else if newton_successes > log_successes {
        println!("🏆 WINNER: Newton Solver");
        println!("   - Higher success rate: {:.1}% vs {:.1}%", 
                 newton_successes as f64 / test_cases.len() as f64 * 100.0,
                 log_successes as f64 / test_cases.len() as f64 * 100.0);
    } else {
        println!("🤝 TIE: Both solvers achieved same success rate");
    }
    
    println!("\n=== KEY INSIGHTS ===");
    println!("1. Both solvers use IDENTICAL IBIS I-V data - fair comparison");
    println!("2. Same MNA matrix formulation - only solution strategy differs");
    println!("3. Logarithmic gradient uses adaptive ramping and sensitivity control");
    println!("4. Newton uses fixed source stepping with traditional damping");
    println!("5. IBIS models test real-world industry-standard nonlinear devices");
    
    if log_successes > newton_successes {
        println!("\n✅ CONCLUSION: Logarithmic gradient approach demonstrates");
        println!("   superior reliability on IBIS models - key for industry adoption!");
    }
}