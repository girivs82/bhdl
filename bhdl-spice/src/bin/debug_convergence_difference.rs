/// Debug Convergence Difference Between NR and Hybrid
/// 
/// Investigates why the two solvers converge to different solutions
/// with identical device models and circuit implementations

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// IDENTICAL element implementations
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

// Newton-Raphson Solver with detailed debugging
pub struct DebugNewtonRaphsonSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl DebugNewtonRaphsonSolver {
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
    
    pub fn solve(&mut self) -> (Vec<f64>, f64, usize, f64) {
        let start = Instant::now();
        
        // Setup voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        println!("🔍 [DEBUG NR] Starting Newton-Raphson solve");
        
        let max_iter = 200;
        let tol = 1e-15;
        let damping = 0.8;
        let mut iterations = 0;
        
        // Print initial state
        println!("🔍 [DEBUG NR] Initial node voltages: {:?}", self.node_voltages);
        
        for iter in 0..max_iter {
            iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            // Debug first few iterations in detail
            if iter < 5 {
                println!("🔍 [DEBUG NR] Iter {}: old_v = {:?}", iter + 1, old_v);
                println!("🔍 [DEBUG NR] Iter {}: A matrix diagonal = {:?}", iter + 1, 
                         (0..a.nrows()).map(|i| a[(i,i)]).collect::<Vec<_>>());
                println!("🔍 [DEBUG NR] Iter {}: B vector = {:?}", iter + 1, b);
            }
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                if iter < 5 {
                    println!("🔍 [DEBUG NR] Iter {}: solution x = {:?}", iter + 1, x);
                }
                
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
                
                if iter < 5 {
                    println!("🔍 [DEBUG NR] Iter {}: new node voltages = {:?}", iter + 1, self.node_voltages);
                    println!("🔍 [DEBUG NR] Iter {}: max_change = {:.3e}", iter + 1, max_change);
                    
                    // Print diode voltage and current
                    for (i, elem) in self.elements.iter().enumerate() {
                        if elem.is_nonlinear() {
                            for &(idx, pos, neg) in &self.connections {
                                if idx == i {
                                    let v_diode = self.node_voltages[pos] - self.node_voltages[neg];
                                    let i_diode = elem.current_at_voltage(v_diode);
                                    println!("🔍 [DEBUG NR] Iter {}: Diode V={:.6}V, I={:.6}A", 
                                             iter + 1, v_diode, i_diode);
                                }
                            }
                        }
                    }
                    println!("🔍 [DEBUG NR] Iter {}: ---", iter + 1);
                }
                
                if max_change < tol {
                    println!("🔍 [DEBUG NR] Converged in {} iterations", iter + 1);
                    break;
                }
            } else {
                println!("🔍 [DEBUG NR] Matrix solve failed at iteration {}", iter + 1);
                break;
            }
        }
        
        // Print final state
        println!("🔍 [DEBUG NR] Final node voltages: {:?}", self.node_voltages);
        
        let mut device_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        let v_final = self.node_voltages[pos] - self.node_voltages[neg];
                        let i_final = elem.current_at_voltage(v_final);
                        device_voltages.push(v_final);
                        println!("🔍 [DEBUG NR] Final diode: V={:.9}V, I={:.9}A", v_final, i_final);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (device_voltages, 
         self.source_currents.get(0).copied().unwrap_or(0.0).abs(), 
         iterations, 
         elapsed)
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

// Ultimate Hybrid Solver with detailed debugging
#[derive(Debug, Clone, Copy)]
enum DampingStrategy {
    ImmediateOverdamp,
}

struct SmartDampingController {
    strategy: DampingStrategy,
    last_gradients: Vec<f64>,
    sign_changes: usize,
    damping_factor: f64,
    base_damping: f64,
    adaptive_step: f64,
    base_step: f64,
    max_history: usize,
}

impl SmartDampingController {
    fn new(strategy: DampingStrategy) -> Self {
        Self {
            strategy,
            last_gradients: Vec::new(),
            sign_changes: 0,
            damping_factor: 0.3,
            base_damping: 0.3,
            adaptive_step: 0.01,
            base_step: 0.01,
            max_history: 5,
        }
    }
    
    fn update_damping(&mut self, gradient: f64) {
        self.last_gradients.push(gradient.abs());
        if self.last_gradients.len() > self.max_history {
            self.last_gradients.remove(0);
        }
        
        if self.last_gradients.len() >= 3 {
            let len = self.last_gradients.len();
            let current_change = self.last_gradients[len-1] - self.last_gradients[len-2];
            let prev_change = self.last_gradients[len-2] - self.last_gradients[len-3];
            
            if current_change.signum() != prev_change.signum() && gradient.abs() > 1e-10 {
                self.sign_changes += 1;
            }
        }
        
        match self.strategy {
            DampingStrategy::ImmediateOverdamp => {
                if self.sign_changes > 2 {
                    self.damping_factor = 0.9;
                    self.adaptive_step *= 0.5;
                } else {
                    self.damping_factor = (self.damping_factor * 0.9 + self.base_damping * 0.1).max(self.base_damping);
                    self.adaptive_step = (self.adaptive_step + self.base_step) / 2.0;
                }
            }
        }
        
        if self.sign_changes > 8 {
            self.sign_changes = 0;
        }
        
        self.damping_factor = self.damping_factor.clamp(0.1, 0.95);
        self.adaptive_step = self.adaptive_step.clamp(0.001, 0.05);
    }
    
    fn get_damping(&self) -> f64 {
        self.damping_factor
    }
    
    fn get_step_size(&self) -> f64 {
        self.adaptive_step
    }
}

pub struct DebugUltimateHybridSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    smart_damping: SmartDampingController,
}

impl DebugUltimateHybridSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            smart_damping: SmartDampingController::new(DampingStrategy::ImmediateOverdamp),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve(&mut self) -> (Vec<f64>, f64, usize, f64) {
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
        
        println!("🚀 [DEBUG HYBRID] Starting Ultimate Hybrid solve");
        println!("🚀 [DEBUG HYBRID] Voltage sources: {:?}", vsources);
        
        // Phase 1: Fast ramping (0-80%)
        let mut ramp = 0.0;
        let phase1_end = 0.8;
        let phase1_step = 0.05;
        
        println!("🚀 [DEBUG HYBRID] Phase 1: Fast ramping 0-80%");
        while ramp < phase1_end {
            ramp = f64::min(ramp + phase1_step, phase1_end);
            
            println!("🚀 [DEBUG HYBRID] Phase1 ramp = {:.2}", ramp);
            
            for &(idx, v) in &vsources {
                let ramped_voltage = v * ramp;
                self.elements[idx].set_voltage(ramped_voltage);
                println!("🚀 [DEBUG HYBRID] Setting voltage source {} to {:.6}V", idx, ramped_voltage);
            }
            
            let phase1_iters = self.solve_phase1();
            total_iterations += phase1_iters;
            
            // Print state after phase1 step
            println!("🚀 [DEBUG HYBRID] After phase1 step: node_voltages = {:?}", self.node_voltages);
            
            // Print diode state
            for (i, elem) in self.elements.iter().enumerate() {
                if elem.is_nonlinear() {
                    for &(idx, pos, neg) in &self.connections {
                        if idx == i {
                            let v_diode = self.node_voltages[pos] - self.node_voltages[neg];
                            let i_diode = elem.current_at_voltage(v_diode);
                            println!("🚀 [DEBUG HYBRID] Phase1 ramp={:.2}: Diode V={:.6}V, I={:.6}A", 
                                     ramp, v_diode, i_diode);
                        }
                    }
                }
            }
        }
        
        println!("🚀 [DEBUG HYBRID] Phase 2: Smart damping 80-100%");
        
        // Phase 2: Smart damping (80-100%)
        let mut phase2_steps = 0;
        while ramp < 0.999 && phase2_steps < 100 {  // Add safety limit
            phase2_steps += 1;
            let step_size = self.smart_damping.get_step_size();
            let old_ramp = ramp;
            ramp = f64::min(ramp + step_size, 1.0);
            
            if phase2_steps <= 10 {
                println!("🚀 [DEBUG HYBRID] Phase2 step {}: ramp {:.6} -> {:.6} (step={:.6})", 
                         phase2_steps, old_ramp, ramp, step_size);
            }
            
            for &(idx, v) in &vsources {
                let ramped_voltage = v * ramp;
                self.elements[idx].set_voltage(ramped_voltage);
            }
            
            let phase2_iters = self.solve_phase2();
            total_iterations += phase2_iters;
            
            if phase2_steps <= 5 {
                // Print diode state
                for (i, elem) in self.elements.iter().enumerate() {
                    if elem.is_nonlinear() {
                        for &(idx, pos, neg) in &self.connections {
                            if idx == i {
                                let v_diode = self.node_voltages[pos] - self.node_voltages[neg];
                                let i_diode = elem.current_at_voltage(v_diode);
                                println!("🚀 [DEBUG HYBRID] Phase2 step={}: Diode V={:.6}V, I={:.6}A", 
                                         phase2_steps, v_diode, i_diode);
                            }
                        }
                    }
                }
            }
        }
        
        // CRITICAL FIX: Ensure voltage sources reach their full target values
        println!("🚀 [DEBUG HYBRID] CRITICAL FIX: Setting voltage sources to full target values");
        for &(idx, v) in &vsources {
            let old_voltage = self.elements[idx].get_voltage();
            self.elements[idx].set_voltage(v);  // Set to full target voltage, not ramped
            println!("🚀 [DEBUG HYBRID] Fixed voltage source {}: {:.9}V -> {:.9}V", 
                     idx, old_voltage, v);
        }
        
        println!("🚀 [DEBUG HYBRID] Final solve");
        self.solve_final(&mut total_iterations);
        
        // Print final state
        println!("🚀 [DEBUG HYBRID] Final node voltages: {:?}", self.node_voltages);
        
        let mut device_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        let v_final = self.node_voltages[pos] - self.node_voltages[neg];
                        let i_final = elem.current_at_voltage(v_final);
                        device_voltages.push(v_final);
                        println!("🚀 [DEBUG HYBRID] Final diode: V={:.9}V, I={:.9}A", v_final, i_final);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        
        (device_voltages, 
         self.source_currents.get(0).copied().unwrap_or(0.0).abs(), 
         total_iterations, 
         elapsed)
    }
    
    fn solve_phase1(&mut self) -> usize {
        let max_iter = 30;
        let tol = 1e-8;
        let damping = 0.5;
        
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
    
    fn solve_phase2(&mut self) -> usize {
        let max_iter = 50;
        let tol = 1e-12;
        
        for iter in 0..max_iter {
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                let mut gradient_sum = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    gradient_sum += delta.abs();
                }
                
                self.smart_damping.update_damping(gradient_sum);
                let adaptive_damping = self.smart_damping.get_damping();
                
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
                    return iter + 1;
                }
            } else {
                return iter + 1;
            }
        }
        
        max_iter
    }
    
    fn solve_final(&mut self, total_iterations: &mut usize) {
        let max_iter = 100;
        let tol = 1e-15;
        let damping = 0.8;
        
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
                    return;
                }
            } else {
                return;
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

fn main() {
    println!("=== DEBUG: CONVERGENCE DIFFERENCE ANALYSIS ===");
    println!("Investigating why NR and Hybrid converge to different solutions");
    println!("Using IDENTICAL device models on simple test case\n");
    
    // Simple test case
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.026;
    
    println!("📋 Test Circuit: 1V source -> 100Ω resistor -> diode -> ground");
    println!("📋 Diode: Is={:.0e}A, Vt={:.3}V\n", is, vt);
    
    // Newton-Raphson solve
    println!("{}", "=".repeat(60));
    println!("NEWTON-RAPHSON DETAILED TRACE");
    println!("{}", "=".repeat(60));
    
    let mut nr_solver = DebugNewtonRaphsonSolver::new(3);
    let v = nr_solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = nr_solver.add_element(Box::new(Resistor::new(rs)));
    let d = nr_solver.add_element(Box::new(Diode::new(is, vt)));
    nr_solver.connect(v, 1, 0);
    nr_solver.connect(r, 1, 2);
    nr_solver.connect(d, 2, 0);
    
    let (nr_device_voltages, nr_id, nr_iters, nr_time) = nr_solver.solve();
    let nr_vd = nr_device_voltages.get(0).copied().unwrap_or(0.0);
    
    println!("🏁 NR FINAL RESULT: Vd={:.9}V, Id={:.9}A, {} iters, {:.1}ms\n", 
             nr_vd, nr_id, nr_iters, nr_time);
    
    // Ultimate Hybrid solve
    println!("{}", "=".repeat(60));
    println!("ULTIMATE HYBRID DETAILED TRACE");
    println!("{}", "=".repeat(60));
    
    let mut hybrid_solver = DebugUltimateHybridSolver::new(3);
    let v = hybrid_solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = hybrid_solver.add_element(Box::new(Resistor::new(rs)));
    let d = hybrid_solver.add_element(Box::new(Diode::new(is, vt)));
    hybrid_solver.connect(v, 1, 0);
    hybrid_solver.connect(r, 1, 2);
    hybrid_solver.connect(d, 2, 0);
    
    let (hybrid_device_voltages, hybrid_id, hybrid_iters, hybrid_time) = hybrid_solver.solve();
    let hybrid_vd = hybrid_device_voltages.get(0).copied().unwrap_or(0.0);
    
    println!("🏁 HYBRID FINAL RESULT: Vd={:.9}V, Id={:.9}A, {} iters, {:.1}ms\n", 
             hybrid_vd, hybrid_id, hybrid_iters, hybrid_time);
    
    // Compare results
    println!("{}", "=".repeat(60));
    println!("CONVERGENCE ANALYSIS");
    println!("{}", "=".repeat(60));
    
    let v_diff = (hybrid_vd - nr_vd).abs();
    let i_diff = (hybrid_id - nr_id).abs();
    let v_err_pct = if nr_vd != 0.0 { v_diff / nr_vd * 100.0 } else { 0.0 };
    let i_err_pct = if nr_id != 0.0 { i_diff / nr_id * 100.0 } else { 0.0 };
    
    println!("🔍 Voltage difference: {:.9}V ({:.4}%)", v_diff, v_err_pct);
    println!("🔍 Current difference: {:.9}A ({:.4}%)", i_diff, i_err_pct);
    
    if v_err_pct > 1.0 || i_err_pct > 1.0 {
        println!("❌ SIGNIFICANT DIFFERENCE DETECTED!");
        println!("   This indicates the algorithms are converging to different solutions");
        println!("   or there's a bug in one of the implementations");
    } else {
        println!("✅ Both algorithms converged to essentially the same solution");
        println!("   The difference is within numerical precision");
    }
    
    // Manual verification
    println!("\n🧮 MANUAL VERIFICATION:");
    println!("For circuit: 1V -> 100Ω -> diode -> gnd");
    println!("KCL at diode node: (V1-Vd)/R = Is*(exp(Vd/Vt)-1)");
    println!("Expected: Vd ≈ 0.561V for 1V source with 100Ω resistor");
    
    if (nr_vd - 0.561).abs() < 0.01 {
        println!("✅ NR result matches expected value");
    } else {
        println!("❌ NR result deviates from expected value");
    }
    
    if (hybrid_vd - 0.561).abs() < 0.01 {
        println!("✅ Hybrid result matches expected value");
    } else {
        println!("❌ Hybrid result deviates from expected value");
    }
}