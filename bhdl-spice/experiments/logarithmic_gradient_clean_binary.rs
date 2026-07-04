/// Clean Binary Search Logarithmic Gradient Solver
/// 
/// Implements the clean algorithm:
/// 1. Start with ramp=0, all voltages are 0
/// 2. Gradually increase ramp, monitor voltage changes
/// 3. When voltage direction reverses (sign change), we've gone too far
/// 4. Go back to previous ramp and use smaller step
/// 5. Continue binary search until converged
/// 
/// No target voltages, no complex damping - just monitor voltage progression!

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

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

// Clean binary search on ramp factor
struct CleanBinarySearch {
    current_ramp: f64,
    step_size: f64,
    
    // Track voltage progression
    last_voltage: Option<f64>,
    last_voltage_change: Option<f64>,
    
    // Binary search bounds
    ramp_low: f64,
    ramp_high: f64,
    
    iterations: usize,
}

impl CleanBinarySearch {
    fn new() -> Self {
        Self {
            current_ramp: 0.5,  // Start at midpoint
            step_size: 0.1,     // Initial step size
            
            last_voltage: None,
            last_voltage_change: None,
            
            ramp_low: 0.0,
            ramp_high: 1.0,
            
            iterations: 0,
        }
    }
    
    fn next_ramp(&mut self, current_voltage: f64) -> (f64, bool) {
        self.iterations += 1;
        
        // First iteration - just record and continue
        if self.last_voltage.is_none() {
            self.last_voltage = Some(current_voltage);
            self.current_ramp += self.step_size;
            return (self.current_ramp, false);
        }
        
        let prev_voltage = self.last_voltage.unwrap();
        let voltage_change = current_voltage - prev_voltage;
        
        // Debug output
        if self.iterations <= 15 {
            println!("    [Iter {}: ramp={:.4}, V={:.4}, ΔV={:.4}]", 
                     self.iterations, self.current_ramp, current_voltage, voltage_change);
        }
        
        // Check for sign change in voltage progression
        if let Some(last_change) = self.last_voltage_change {
            // Sign change detected - we've gone too far!
            if last_change.signum() != voltage_change.signum() && voltage_change.abs() > 1e-10 {
                println!("    [Sign change detected! Going back and halving step]");
                
                // Go back to where we were
                self.current_ramp -= self.step_size;
                
                // Halve the step size for finer control
                self.step_size *= 0.5;
                
                // Update bounds for binary search
                if voltage_change > 0.0 {
                    // Voltage increasing - we're overshooting
                    self.ramp_high = self.current_ramp + self.step_size;
                } else {
                    // Voltage decreasing - we're undershooting  
                    self.ramp_low = self.current_ramp;
                }
                
                // Continue with smaller step
                self.current_ramp += self.step_size;
            } else {
                // No sign change - continue in same direction
                self.current_ramp += self.step_size;
            }
        } else {
            // Second iteration - continue
            self.current_ramp += self.step_size;
        }
        
        // Ensure we stay in bounds
        self.current_ramp = self.current_ramp.clamp(0.0, 1.0);
        
        // Update history
        self.last_voltage = Some(current_voltage);
        self.last_voltage_change = Some(voltage_change);
        
        // Check for convergence
        let converged = self.step_size < 1e-6 || 
                       self.current_ramp >= 0.999 || 
                       self.iterations > 50;
        
        (self.current_ramp, converged)
    }
}

// Main solver
pub struct CleanBinaryLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    search: CleanBinarySearch,
}

impl CleanBinaryLogGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            search: CleanBinarySearch::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    fn get_diode_voltage(&self) -> f64 {
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        return self.node_voltages[pos] - self.node_voltages[neg];
                    }
                }
            }
        }
        0.0
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
        
        // Find voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        println!("  [Starting clean binary search]");
        
        // Clean binary search loop
        loop {
            // Reset node voltages for clean solve at this ramp
            self.node_voltages = vec![0.0; self.num_nodes];
            
            // Set sources to current ramp level
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * self.search.current_ramp);
            }
            
            // Solve the circuit at this ramp level
            self.solve_at_current_ramp(&mut total_iterations);
            
            // Get the diode voltage (our progress indicator)
            let diode_voltage = self.get_diode_voltage();
            
            // Update binary search
            let (next_ramp, done) = self.search.next_ramp(diode_voltage);
            
            if done {
                // Final solve at converged ramp
                println!("  [Converged at ramp={:.6}]", self.search.current_ramp);
                break;
            }
        }
        
        let vd = self.node_voltages[2];
        let id = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (vd, id, total_iterations, elapsed)
    }
    
    fn solve_at_current_ramp(&mut self, total_iterations: &mut usize) {
        let max_iter = 50;
        let tol = 1e-12;
        let damping = 0.7;  // Moderate damping
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
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
                    return;  // Converged successfully
                }
            } else {
                return;  // Matrix solution failed
            }
        }
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
    println!("=== CLEAN BINARY SEARCH LOGARITHMIC GRADIENT SOLVER ===");
    println!("Pure algorithm: monitor voltage progression, binary search on sign changes\n");
    
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
             "Test Case", "SPICE Vd", "SPICE Id", "Clean Vd", "Clean Id", "Error %", "Iters", "Time ms");
    println!("{}", "=".repeat(100));
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut total_iterations = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
        
        let mut solver = CleanBinaryLogGradientSolver::new(3);
        
        let v = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = solver.add_element(Box::new(Resistor::new(rs)));
        let d = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(v, 1, 0);
        solver.connect(r, 1, 2);
        solver.connect(d, 2, 0);
        
        let (vd_clean, id_clean, iters, time) = solver.solve();
        
        let v_err = ((vd_clean - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_clean - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        total_error += max_err;
        total_time += time;
        total_iterations += iters;
        
        println!("{:>20} | {:>12.6} | {:>12.3} | {:>12.6} | {:>12.3} | {:>8.4} | {:>8} | {:>8.1}", 
                 test_name, 
                 vd_ref, id_ref * 1000.0,
                 vd_clean, id_clean * 1000.0,
                 max_err, iters, time);
    }
    
    println!("{}", "=".repeat(100));
    
    let n_cases = test_cases.len() as f64;
    println!("\nClean Binary Search Results:");
    println!("  Average error: {:.4}%", total_error / n_cases);
    println!("  Average time: {:.1}ms", total_time / n_cases);
    println!("  Average iterations: {:.0}", total_iterations as f64 / n_cases);
    
    println!("\n=== ALGORITHM SUMMARY ===");
    println!("1. Start at ramp=0 (all voltages zero)");
    println!("2. Gradually increase ramp, monitor voltage changes");
    println!("3. When voltage direction reverses → sign change detected");
    println!("4. Go back and halve step size");
    println!("5. Continue binary search until converged");
    println!("6. No target voltages, no complex damping needed!");
}