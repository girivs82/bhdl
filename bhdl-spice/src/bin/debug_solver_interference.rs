/// Debug: Why does Ultimate Hybrid fail when Newton-Raphson is present?
/// 
/// This will test the exact same circuit with both solvers sequentially
/// and print detailed state to find where the interference occurs.

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

// Simple Newton-Raphson solver
pub struct NewtonRaphsonSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl NewtonRaphsonSolver {
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
        
        let max_iter = 200;
        let tol = 1e-15;
        let damping = 0.8;
        let mut iterations = 0;
        
        for _iter in 0..max_iter {
            iterations += 1;
            
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
                    break;
                }
            } else {
                break;
            }
        }
        
        // Get diode voltage
        let mut diode_voltage = 0.0;
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        diode_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                break;
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let current = self.source_currents.get(0).copied().unwrap_or(0.0).abs();
        
        (diode_voltage, current, iterations, elapsed)
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

// Minimal Ultimate Hybrid solver (just for one test case)
pub struct SimpleHybridSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl SimpleHybridSolver {
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
        
        // Find voltage sources
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Phase 1: Fast ramping (0-80%)
        let mut ramp = 0.0;
        let phase1_end = 0.8;
        let phase1_step = 0.05;
        
        while ramp < phase1_end {
            ramp = f64::min(ramp + phase1_step, phase1_end);
            
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            total_iterations += self.solve_phase();
        }
        
        // Phase 2: Remaining ramping (80-100%)
        while ramp < 0.999 {
            ramp = f64::min(ramp + 0.01, 1.0);
            
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            total_iterations += self.solve_phase();
        }
        
        // CRITICAL: Set voltage sources to full target values
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Final solve
        total_iterations += self.solve_final();
        
        // Get diode voltage
        let mut diode_voltage = 0.0;
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        diode_voltage = self.node_voltages[pos] - self.node_voltages[neg];
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
    
    fn solve_phase(&mut self) -> usize {
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
        let tol = 1e-15;
        let damping = 0.8;
        
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
    println!("=== DEBUGGING SOLVER INTERFERENCE ===");
    println!("Testing why Ultimate Hybrid fails when Newton-Raphson is present\n");
    
    // Test parameters
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.026;
    
    println!("🧪 TEST 1: Newton-Raphson ONLY");
    {
        let mut nr_solver = NewtonRaphsonSolver::new(3);
        let v = nr_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = nr_solver.add_element(Box::new(Resistor::new(rs)));
        let d = nr_solver.add_element(Box::new(Diode::new(is, vt)));
        nr_solver.connect(v, 1, 0);
        nr_solver.connect(r, 1, 2);
        nr_solver.connect(d, 2, 0);
        
        let (nr_vd, nr_id, nr_iters, nr_time) = nr_solver.solve();
        println!("  Result: Vd={:.6}V, Id={:.6}mA, {} iters, {:.1}ms", 
                 nr_vd, nr_id * 1000.0, nr_iters, nr_time);
    }
    
    println!("\n🧪 TEST 2: Ultimate Hybrid ONLY");
    {
        let mut hybrid_solver = SimpleHybridSolver::new(3);
        let v = hybrid_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = hybrid_solver.add_element(Box::new(Resistor::new(rs)));
        let d = hybrid_solver.add_element(Box::new(Diode::new(is, vt)));
        hybrid_solver.connect(v, 1, 0);
        hybrid_solver.connect(r, 1, 2);
        hybrid_solver.connect(d, 2, 0);
        
        let (hybrid_vd, hybrid_id, hybrid_iters, hybrid_time) = hybrid_solver.solve();
        println!("  Result: Vd={:.6}V, Id={:.6}mA, {} iters, {:.1}ms", 
                 hybrid_vd, hybrid_id * 1000.0, hybrid_iters, hybrid_time);
    }
    
    println!("\n🧪 TEST 3: BOTH SOLVERS (NR first, then Hybrid)");
    {
        // Newton-Raphson first
        let mut nr_solver = NewtonRaphsonSolver::new(3);
        let v = nr_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = nr_solver.add_element(Box::new(Resistor::new(rs)));
        let d = nr_solver.add_element(Box::new(Diode::new(is, vt)));
        nr_solver.connect(v, 1, 0);
        nr_solver.connect(r, 1, 2);
        nr_solver.connect(d, 2, 0);
        
        let (nr_vd, nr_id, nr_iters, nr_time) = nr_solver.solve();
        println!("  NR Result: Vd={:.6}V, Id={:.6}mA, {} iters, {:.1}ms", 
                 nr_vd, nr_id * 1000.0, nr_iters, nr_time);
        
        // Then Ultimate Hybrid
        let mut hybrid_solver = SimpleHybridSolver::new(3);
        let v = hybrid_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = hybrid_solver.add_element(Box::new(Resistor::new(rs)));
        let d = hybrid_solver.add_element(Box::new(Diode::new(is, vt)));
        hybrid_solver.connect(v, 1, 0);
        hybrid_solver.connect(r, 1, 2);
        hybrid_solver.connect(d, 2, 0);
        
        let (hybrid_vd, hybrid_id, hybrid_iters, hybrid_time) = hybrid_solver.solve();
        println!("  Hybrid Result: Vd={:.6}V, Id={:.6}mA, {} iters, {:.1}ms", 
                 hybrid_vd, hybrid_id * 1000.0, hybrid_iters, hybrid_time);
        
        // Calculate error
        let v_err = if nr_vd != 0.0 { ((hybrid_vd - nr_vd) / nr_vd * 100.0).abs() } else { 0.0 };
        let i_err = if nr_id != 0.0 { ((hybrid_id - nr_id) / nr_id * 100.0).abs() } else { 0.0 };
        
        println!("\n📊 COMPARISON:");
        println!("  Voltage error: {:.4}%", v_err);
        println!("  Current error: {:.4}%", i_err);
        
        if v_err > 5.0 || i_err > 5.0 {
            println!("  ❌ SIGNIFICANT ERROR DETECTED!");
            println!("  This confirms interference between solvers");
        } else {
            println!("  ✅ Both solvers agree within reasonable tolerance");
        }
    }
    
    println!("\n🧪 TEST 4: BOTH SOLVERS (Hybrid first, then NR)");
    {
        // Ultimate Hybrid first
        let mut hybrid_solver = SimpleHybridSolver::new(3);
        let v = hybrid_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = hybrid_solver.add_element(Box::new(Resistor::new(rs)));
        let d = hybrid_solver.add_element(Box::new(Diode::new(is, vt)));
        hybrid_solver.connect(v, 1, 0);
        hybrid_solver.connect(r, 1, 2);
        hybrid_solver.connect(d, 2, 0);
        
        let (hybrid_vd, hybrid_id, hybrid_iters, hybrid_time) = hybrid_solver.solve();
        println!("  Hybrid Result: Vd={:.6}V, Id={:.6}mA, {} iters, {:.1}ms", 
                 hybrid_vd, hybrid_id * 1000.0, hybrid_iters, hybrid_time);
        
        // Then Newton-Raphson
        let mut nr_solver = NewtonRaphsonSolver::new(3);
        let v = nr_solver.add_element(Box::new(VoltageSource::new(vs)));
        let r = nr_solver.add_element(Box::new(Resistor::new(rs)));
        let d = nr_solver.add_element(Box::new(Diode::new(is, vt)));
        nr_solver.connect(v, 1, 0);
        nr_solver.connect(r, 1, 2);
        nr_solver.connect(d, 2, 0);
        
        let (nr_vd, nr_id, nr_iters, nr_time) = nr_solver.solve();
        println!("  NR Result: Vd={:.6}V, Id={:.6}mA, {} iters, {:.1}ms", 
                 nr_vd, nr_id * 1000.0, nr_iters, nr_time);
        
        // Calculate error
        let v_err = if nr_vd != 0.0 { ((hybrid_vd - nr_vd) / nr_vd * 100.0).abs() } else { 0.0 };
        let i_err = if nr_id != 0.0 { ((hybrid_id - nr_id) / nr_id * 100.0).abs() } else { 0.0 };
        
        println!("\n📊 COMPARISON:");
        println!("  Voltage error: {:.4}%", v_err);
        println!("  Current error: {:.4}%", i_err);
        
        if v_err > 5.0 || i_err > 5.0 {
            println!("  ❌ SIGNIFICANT ERROR DETECTED!");
            println!("  This confirms interference between solvers");
        } else {
            println!("  ✅ Both solvers agree within reasonable tolerance");
        }
    }
    
    println!("\n=== DEBUGGING CONCLUSION ===");
    println!("If errors appear only in TEST 3 and/or TEST 4, then there's");
    println!("interference between the solvers when both are present.");
    println!("If errors appear in all tests, the issue is in the solver itself.");
}