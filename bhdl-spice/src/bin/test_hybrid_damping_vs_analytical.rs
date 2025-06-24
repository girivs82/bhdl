/// Test Hybrid Approaches with Second Phase Damping vs Analytical Solution
/// 
/// This tests various hybrid solver approaches including second phase damping
/// against the TRUE analytical solution (0.576342543266094V) to see if they
/// help with accuracy or speed compared to the logarithmic gradient approach

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Element trait and implementations (identical for all solvers)
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

// ANALYTICAL reference solution (TRUE golden standard)
fn analytical_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    // Use ultra-high precision Newton's method to find the true analytical solution
    let mut vd = 0.6; // Good starting point
    let tolerance = 1e-18; // Ultra-high precision
    
    for _iter in 0..1000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs; // Circuit equation
        let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs; // Derivative
        let delta = f / df_dvd;
        vd -= delta;
        
        if delta.abs() < tolerance {
            break;
        }
    }
    
    // Calculate final current
    let id = is * ((vd / vt).exp() - 1.0);
    (vd, id)
}

// 1. Ultimate Hybrid Solver (ramping + smart damping)
pub struct UltimateHybridSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl UltimateHybridSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Get voltage sources for ramping
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Phase 1: Ramping with adaptive damping
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0.05;
        
        while ramp_factor < 1.0 {
            // Scale voltage sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve with adaptive damping based on ramp progress
            let damping = if ramp_factor < 0.3 { 0.9 }  // High damping early
                         else if ramp_factor < 0.7 { 0.7 }  // Medium damping middle
                         else { 0.5 };  // Lower damping near end
            
            let iters = self.solve_step_with_damping(damping);
            total_iterations += iters;
            
            ramp_factor += ramp_step;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Phase 2: Final solve at 100% with smart second-phase damping
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Second phase damping: Start conservative, then increase aggression
        let second_phase_iters = self.solve_with_second_phase_damping();
        total_iterations += second_phase_iters;
        
        let diode_voltage = self.node_voltages[2];
        let current = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (diode_voltage, current, total_iterations, elapsed)
    }
    
    fn solve_step_with_damping(&mut self, damping: f64) -> usize {
        let max_iter = 50;
        let tol = 1e-12;
        
        for iter in 0..max_iter {
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
                    return iter + 1;
                }
            } else {
                return iter + 1;
            }
        }
        max_iter
    }
    
    fn solve_with_second_phase_damping(&mut self) -> usize {
        let max_iter = 100;
        let tol = 1e-15;
        let mut total_iters = 0;
        
        // Phase 2a: Conservative damping for stability
        for iter in 0..30 {
            total_iters += 1;
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                let conservative_damping = 0.3; // Very conservative
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + conservative_damping * delta;
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
                    return total_iters;
                }
            }
        }
        
        // Phase 2b: Gradually increase damping for final convergence
        for iter in 0..(max_iter - 30) {
            total_iters += 1;
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Gradually increase damping from 0.3 to 0.8
                let progress = iter as f64 / (max_iter - 30) as f64;
                let adaptive_damping = 0.3 + 0.5 * progress;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + adaptive_damping * delta;
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
                    return total_iters;
                }
            }
        }
        
        total_iters
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

// 2. Smart Damping Hybrid Solver (varying damping strategies)
pub struct SmartDampingHybridSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    convergence_history: VecDeque<f64>,
}

impl SmartDampingHybridSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            convergence_history: VecDeque::with_capacity(10),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Get voltage sources for ramping
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Ramping phase with smart damping
        let ramp_steps = 20;
        for step in 0..=ramp_steps {
            let ramp_factor = step as f64 / ramp_steps as f64;
            
            // Scale voltage sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Smart damping based on convergence history
            let iters = self.solve_step_with_smart_damping();
            total_iterations += iters;
        }
        
        let diode_voltage = self.node_voltages[2];
        let current = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (diode_voltage, current, total_iterations, elapsed)
    }
    
    fn solve_step_with_smart_damping(&mut self) -> usize {
        let max_iter = 50;
        let tol = 1e-12;
        
        for iter in 0..max_iter {
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Calculate smart damping based on recent convergence behavior
                let damping = self.calculate_smart_damping(iter);
                
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
                
                // Track convergence rate
                self.convergence_history.push_back(max_change);
                if self.convergence_history.len() > 10 {
                    self.convergence_history.pop_front();
                }
                
                if max_change < tol {
                    return iter + 1;
                }
            } else {
                return iter + 1;
            }
        }
        max_iter
    }
    
    fn calculate_smart_damping(&self, iter: usize) -> f64 {
        if self.convergence_history.len() < 3 {
            return 0.7; // Default damping
        }
        
        // Analyze convergence trend
        let recent = &self.convergence_history;
        let latest = recent[recent.len() - 1];
        let prev = recent[recent.len() - 2];
        
        let improvement_rate = if prev > 0.0 { latest / prev } else { 1.0 };
        
        // Adjust damping based on convergence behavior
        if improvement_rate < 0.5 {
            // Good convergence - can be more aggressive
            (0.8 + 0.1 * (iter as f64 / 50.0)).min(0.9)
        } else if improvement_rate < 0.9 {
            // Steady convergence - moderate damping
            0.7
        } else {
            // Slow or poor convergence - be conservative
            (0.5 - 0.1 * (iter as f64 / 50.0)).max(0.3)
        }
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        // Same as UltimateHybridSolver::build_mna_system()
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

// 3. Logarithmic Gradient Solver (for comparison)
pub struct LogarithmicGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl LogarithmicGradientSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Get voltage sources for ramping
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Standard ramping approach
        let ramp_steps = 100;
        for step in 0..=ramp_steps {
            let ramp_factor = step as f64 / ramp_steps as f64;
            
            // Scale voltage sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            let iters = self.solve_step();
            total_iterations += iters;
        }
        
        let diode_voltage = self.node_voltages[2];
        let current = self.source_currents[0].abs();
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (diode_voltage, current, total_iterations, elapsed)
    }
    
    fn solve_step(&mut self) -> usize {
        let max_iter = 30;
        let tol = 1e-12;
        let damping = 0.7;
        
        for iter in 0..max_iter {
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
                    return iter + 1;
                }
            } else {
                return iter + 1;
            }
        }
        max_iter
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        // Same as UltimateHybridSolver::build_mna_system()
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

fn main() {
    println!("=== HYBRID APPROACHES WITH SECOND PHASE DAMPING vs ANALYTICAL SOLUTION ===");
    println!("Testing if hybrid approaches with damping improve accuracy or speed");
    println!("Compared against TRUE analytical solution (0.576342543266094V)\n");
    
    let test_cases = [
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", 1.0, 100.0, 1e-12, 0.050),
        ("Low current", 0.1, 1000.0, 1e-12, 0.026),
        ("High voltage", 5.0, 100.0, 1e-12, 0.026),
        ("Low resistance", 1.0, 10.0, 1e-12, 0.026),
    ];
    
    println!("{:>15} | {:>12} | {:>12} | {:>12} | {:>12} | {:>12} | {:>12} | {:>8} | {:>8} | {:>8}", 
             "Test Case", "Analytical Vd", "Ultimate Vd", "Smart Vd", "LogGrad Vd", "Ultimate Err%", "Smart Err%", "U.Time", "S.Time", "L.Time");
    println!("{}", "=".repeat(150));
    
    let mut total_ultimate_error = 0.0;
    let mut total_smart_error = 0.0;
    let mut total_loggra_error = 0.0;
    let mut total_ultimate_time = 0.0;
    let mut total_smart_time = 0.0;
    let mut total_loggra_time = 0.0;
    let mut total_ultimate_iters = 0;
    let mut total_smart_iters = 0;
    let mut total_loggra_iters = 0;
    
    for &(test_name, vs, rs, is, vt) in &test_cases {
        // TRUE analytical reference solution
        let (ref_vd, _ref_id) = analytical_reference(vs, rs, is, vt);
        
        // 1. Ultimate Hybrid solve
        let mut ultimate_solver = UltimateHybridSolver::new(3);
        let v1 = ultimate_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r1 = ultimate_solver.add_element(Box::new(Resistor::new(rs)));
        let d1 = ultimate_solver.add_element(Box::new(Diode::new(is, vt)));
        ultimate_solver.connect(v1, 1, 0);
        ultimate_solver.connect(r1, 1, 2);
        ultimate_solver.connect(d1, 2, 0);
        
        let (ultimate_vd, _ultimate_id, ultimate_iters, ultimate_time) = ultimate_solver.solve();
        
        // 2. Smart Damping Hybrid solve
        let mut smart_solver = SmartDampingHybridSolver::new(3);
        let v2 = smart_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r2 = smart_solver.add_element(Box::new(Resistor::new(rs)));
        let d2 = smart_solver.add_element(Box::new(Diode::new(is, vt)));
        smart_solver.connect(v2, 1, 0);
        smart_solver.connect(r2, 1, 2);
        smart_solver.connect(d2, 2, 0);
        
        let (smart_vd, _smart_id, smart_iters, smart_time) = smart_solver.solve();
        
        // 3. Logarithmic Gradient solve (baseline)
        let mut lg_solver = LogarithmicGradientSolver::new(3);
        let v3 = lg_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r3 = lg_solver.add_element(Box::new(Resistor::new(rs)));
        let d3 = lg_solver.add_element(Box::new(Diode::new(is, vt)));
        lg_solver.connect(v3, 1, 0);
        lg_solver.connect(r3, 1, 2);
        lg_solver.connect(d3, 2, 0);
        
        let (lg_vd, _lg_id, lg_iters, lg_time) = lg_solver.solve();
        
        // Calculate errors against TRUE analytical solution
        let ultimate_err = if ref_vd != 0.0 { ((ultimate_vd - ref_vd) / ref_vd * 100.0).abs() } else { 0.0 };
        let smart_err = if ref_vd != 0.0 { ((smart_vd - ref_vd) / ref_vd * 100.0).abs() } else { 0.0 };
        let lg_err = if ref_vd != 0.0 { ((lg_vd - ref_vd) / ref_vd * 100.0).abs() } else { 0.0 };
        
        total_ultimate_error += ultimate_err;
        total_smart_error += smart_err;
        total_loggra_error += lg_err;
        total_ultimate_time += ultimate_time;
        total_smart_time += smart_time;
        total_loggra_time += lg_time;
        total_ultimate_iters += ultimate_iters;
        total_smart_iters += smart_iters;
        total_loggra_iters += lg_iters;
        
        println!("{:>15} | {:>12.6} | {:>12.6} | {:>12.6} | {:>12.6} | {:>12.4} | {:>12.4} | {:>8.1} | {:>8.1} | {:>8.1}", 
                 test_name, 
                 ref_vd,
                 ultimate_vd, smart_vd, lg_vd,
                 ultimate_err, smart_err,
                 ultimate_time, smart_time, lg_time);
    }
    
    println!("{}", "=".repeat(150));
    
    let n_cases = test_cases.len() as f64;
    println!("\n📊 HYBRID APPROACHES PERFORMANCE SUMMARY:");
    
    println!("\n1. ULTIMATE HYBRID (Ramping + Second Phase Damping):");
    println!("   Average error vs analytical: {:.6}%", total_ultimate_error / n_cases);
    println!("   Average time: {:.1}ms", total_ultimate_time / n_cases);
    println!("   Average iterations: {:.0}", total_ultimate_iters as f64 / n_cases);
    
    println!("\n2. SMART DAMPING HYBRID (Adaptive Damping):");
    println!("   Average error vs analytical: {:.6}%", total_smart_error / n_cases);
    println!("   Average time: {:.1}ms", total_smart_time / n_cases);
    println!("   Average iterations: {:.0}", total_smart_iters as f64 / n_cases);
    
    println!("\n3. LOGARITHMIC GRADIENT (Baseline):");
    println!("   Average error vs analytical: {:.6}%", total_loggra_error / n_cases);
    println!("   Average time: {:.1}ms", total_loggra_time / n_cases);
    println!("   Average iterations: {:.0}", total_loggra_iters as f64 / n_cases);
    
    // Performance comparison
    println!("\n=== COMPARISON RESULTS ===");
    
    let avg_ultimate_err = total_ultimate_error / n_cases;
    let avg_smart_err = total_smart_error / n_cases;
    let avg_lg_err = total_loggra_error / n_cases;
    
    let avg_ultimate_time = total_ultimate_time / n_cases;
    let avg_smart_time = total_smart_time / n_cases;
    let avg_lg_time = total_loggra_time / n_cases;
    
    // Find best accuracy
    if avg_ultimate_err <= avg_smart_err && avg_ultimate_err <= avg_lg_err {
        println!("🎯 ACCURACY WINNER: Ultimate Hybrid ({:.4}% error)", avg_ultimate_err);
        if avg_smart_err > 0.0 { println!("   {:.1}x more accurate than Smart Damping", avg_smart_err / avg_ultimate_err); }
        if avg_lg_err > 0.0 { println!("   {:.1}x more accurate than Log Gradient", avg_lg_err / avg_ultimate_err); }
    } else if avg_smart_err <= avg_lg_err {
        println!("🎯 ACCURACY WINNER: Smart Damping Hybrid ({:.4}% error)", avg_smart_err);
        if avg_ultimate_err > 0.0 { println!("   {:.1}x more accurate than Ultimate Hybrid", avg_ultimate_err / avg_smart_err); }
        if avg_lg_err > 0.0 { println!("   {:.1}x more accurate than Log Gradient", avg_lg_err / avg_smart_err); }
    } else {
        println!("🎯 ACCURACY WINNER: Logarithmic Gradient ({:.4}% error)", avg_lg_err);
        if avg_ultimate_err > 0.0 { println!("   {:.1}x more accurate than Ultimate Hybrid", avg_ultimate_err / avg_lg_err); }
        if avg_smart_err > 0.0 { println!("   {:.1}x more accurate than Smart Damping", avg_smart_err / avg_lg_err); }
    }
    
    // Find best speed
    if avg_ultimate_time <= avg_smart_time && avg_ultimate_time <= avg_lg_time {
        println!("\n⚡ SPEED WINNER: Ultimate Hybrid ({:.1}ms)", avg_ultimate_time);
        println!("   {:.1}x faster than Smart Damping", avg_smart_time / avg_ultimate_time);
        println!("   {:.1}x faster than Log Gradient", avg_lg_time / avg_ultimate_time);
    } else if avg_smart_time <= avg_lg_time {
        println!("\n⚡ SPEED WINNER: Smart Damping Hybrid ({:.1}ms)", avg_smart_time);
        println!("   {:.1}x faster than Ultimate Hybrid", avg_ultimate_time / avg_smart_time);
        println!("   {:.1}x faster than Log Gradient", avg_lg_time / avg_smart_time);
    } else {
        println!("\n⚡ SPEED WINNER: Logarithmic Gradient ({:.1}ms)", avg_lg_time);
        println!("   {:.1}x faster than Ultimate Hybrid", avg_ultimate_time / avg_lg_time);
        println!("   {:.1}x faster than Smart Damping", avg_smart_time / avg_lg_time);
    }
    
    // Overall assessment
    println!("\n=== HYBRID DAMPING ASSESSMENT ===");
    if avg_ultimate_err < avg_lg_err || avg_smart_err < avg_lg_err {
        println!("✅ HYBRID APPROACHES HELP: Second phase damping improves accuracy");
        
        if avg_ultimate_time < avg_lg_time || avg_smart_time < avg_lg_time {
            println!("✅ SPEED BENEFIT: Hybrid damping also improves convergence speed");
        } else {
            println!("⚠️  SPEED TRADE-OFF: Better accuracy but slower convergence");
        }
    } else {
        println!("❌ LIMITED BENEFIT: Hybrid damping doesn't significantly improve accuracy");
        
        if avg_ultimate_time < avg_lg_time || avg_smart_time < avg_lg_time {
            println!("✅ SPEED BENEFIT: But hybrid approaches are faster");
        } else {
            println!("❌ NO BENEFIT: Hybrid approaches are both less accurate and slower");
        }
    }
    
    println!("\n🎯 KEY FINDING:");
    println!("All approaches are now compared against the TRUE analytical solution (0.576342543266094V)");
    println!("This shows the REAL performance differences between hybrid damping strategies.");
}