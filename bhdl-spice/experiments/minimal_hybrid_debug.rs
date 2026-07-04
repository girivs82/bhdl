/// Minimal test to isolate the Ultimate Hybrid solver bug
/// 
/// The hybrid solver converges to wrong solution even with voltage source fix.
/// This will help identify the root cause in the algorithm itself.

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Simple Element implementations
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

// Ultra-simple hybrid solver to isolate the bug
pub struct MinimalHybridSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl MinimalHybridSolver {
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
        
        // Setup voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut total_iterations = 0;
        
        // Find voltage sources and their target values
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        println!("🔍 MINIMAL HYBRID: Target voltage sources: {:?}", vsources);
        
        // Simple two-phase ramping (no smart damping to isolate the issue)
        let mut ramp = 0.0;
        
        // Phase 1: Ramp to 80%
        while ramp < 0.8 {
            ramp += 0.1;
            
            println!("🔍 Phase 1: ramp = {:.1}", ramp);
            
            for &(idx, v) in &vsources {
                let ramped_v = v * ramp;
                self.elements[idx].set_voltage(ramped_v);
                println!("  Setting voltage source {} to {:.3}V", idx, ramped_v);
            }
            
            total_iterations += self.solve_step();
            
            // Print diode state
            for (i, elem) in self.elements.iter().enumerate() {
                if elem.is_nonlinear() {
                    for &(idx, pos, neg) in &self.connections {
                        if idx == i {
                            let v_diode = self.node_voltages[pos] - self.node_voltages[neg];
                            let i_diode = elem.current_at_voltage(v_diode);
                            println!("  Diode: V={:.6}V, I={:.6}mA", v_diode, i_diode * 1000.0);
                        }
                    }
                }
            }
        }
        
        // Phase 2: Ramp to 100%
        while ramp < 1.0 {
            ramp += 0.05;
            if ramp > 1.0 { ramp = 1.0; }
            
            println!("🔍 Phase 2: ramp = {:.3}", ramp);
            
            for &(idx, v) in &vsources {
                let ramped_v = v * ramp;
                self.elements[idx].set_voltage(ramped_v);
                println!("  Setting voltage source {} to {:.6}V", idx, ramped_v);
            }
            
            total_iterations += self.solve_step();
            
            if ramp >= 1.0 { break; }
        }
        
        // CRITICAL: Ensure voltage sources are at exact target values
        println!("🔍 CRITICAL FIX: Setting to exact target values");
        for &(idx, v) in &vsources {
            let before = self.elements[idx].get_voltage();
            self.elements[idx].set_voltage(v);
            let after = self.elements[idx].get_voltage();
            println!("  Voltage source {}: {:.9}V -> {:.9}V", idx, before, after);
        }
        
        // Final solve with high precision
        println!("🔍 Final solve with high precision");
        total_iterations += self.solve_final();
        
        // Get final diode voltage
        let mut diode_voltage = 0.0;
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        diode_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        let diode_current = elem.current_at_voltage(diode_voltage);
                        println!("🔍 FINAL: Diode V={:.9}V, I={:.9}mA", diode_voltage, diode_current * 1000.0);
                        break;
                    }
                }
                break;
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let current = self.source_currents.get(0).copied().unwrap_or(0.0).abs();
        
        (diode_voltage, current, total_iterations, elapsed)
    }
    
    fn solve_step(&mut self) -> usize {
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
    
    fn solve_final(&mut self) -> usize {
        let max_iter = 100;
        let tol = 1e-15;  // Same as Newton-Raphson
        let damping = 0.8;  // Same as Newton-Raphson
        
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
                
                if iter < 10 || iter % 10 == 0 {
                    println!("  Final iter {}: max_change = {:.3e}", iter + 1, max_change);
                }
                
                if max_change < tol {
                    println!("  Final converged in {} iterations", iter + 1);
                    return iter + 1;
                }
            } else {
                return iter + 1;
            }
        }
        
        max_iter
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
    println!("=== MINIMAL HYBRID SOLVER DEBUG ===");
    println!("Isolating the bug in the hybrid algorithm itself\\n");
    
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.026;
    
    println!("Circuit: {}V -> {}Ω -> diode -> ground", vs, rs);
    println!("Expected diode voltage: ~0.561V (Newton-Raphson result)\\n");
    
    let mut solver = MinimalHybridSolver::new(3);
    let v = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = solver.add_element(Box::new(Resistor::new(rs)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    let (vd, id, iters, time) = solver.solve();
    
    println!("\\n=== FINAL RESULT ===");
    println!("Diode voltage: {:.9}V", vd);
    println!("Diode current: {:.9}mA", id * 1000.0);
    println!("Iterations: {}", iters);
    println!("Time: {:.1}ms", time);
    
    // Compare with expected Newton-Raphson result
    let expected_vd = 0.561414;
    let v_error = ((vd - expected_vd) / expected_vd * 100.0).abs();
    
    println!("\\n=== ANALYSIS ===");
    println!("Expected (Newton-Raphson): {:.6}V", expected_vd);
    println!("Actual (Hybrid): {:.6}V", vd);
    println!("Error: {:.4}%", v_error);
    
    if v_error > 1.0 {
        println!("❌ SIGNIFICANT ERROR: Hybrid algorithm has a bug!");
    } else {
        println!("✅ Result within tolerance");
    }
}