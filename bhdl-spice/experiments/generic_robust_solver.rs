/// Generic Robust Solver - No topology assumptions
/// 
/// This implements a robust solver that works for any circuit without
/// requiring knowledge of the circuit structure

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

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
    CurrentSource,
    Diode,
    BJT,
    MOSFET,
}

// Basic elements
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

pub struct GenericRobustSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl GenericRobustSolver {
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
    
    pub fn solve_dc(&mut self) -> Vec<f64> {
        let start = Instant::now();
        println!("\nGeneric DC Analysis");
        
        // Count voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        // Generic initialization strategy:
        // 1. All node voltages start at 0
        // 2. Use GMIN to ensure matrix is never singular
        // 3. Multiple restart strategies if convergence fails
        
        let mut best_solution = self.node_voltages.clone();
        let mut best_residual = f64::INFINITY;
        
        // Try different initialization strategies
        let init_strategies = vec![
            InitStrategy::Zero,
            InitStrategy::Random(0.1),
            InitStrategy::Random(1.0),
            InitStrategy::SourceBased,
            InitStrategy::Incremental,
        ];
        
        for (strategy_idx, strategy) in init_strategies.iter().enumerate() {
            println!("  Trying strategy {}: {:?}", strategy_idx + 1, strategy);
            
            // Initialize based on strategy
            self.initialize_with_strategy(strategy);
            
            // Source ramping
            let ramp_steps = match strategy {
                InitStrategy::Incremental => 200,  // More steps for incremental
                _ => 100,
            };
            
            let mut converged = false;
            
            // Get voltage sources for ramping
            let mut vsources = Vec::new();
            for (i, elem) in self.elements.iter().enumerate() {
                if elem.element_type() == ElementType::VoltageSource {
                    vsources.push((i, elem.get_voltage()));
                }
            }
            
            // Ramping loop
            for ramp in 0..=ramp_steps {
                let factor = ramp as f64 / ramp_steps as f64;
                
                // Update sources
                for &(idx, v) in &vsources {
                    self.elements[idx].set_voltage(v * factor);
                }
                
                // Newton-Raphson with multiple damping strategies
                let damping_strategies = vec![1.0, 0.8, 0.5, 0.3, 0.1];
                
                for &initial_damping in &damping_strategies {
                    let mut damping = initial_damping;
                    let mut stalled_count = 0;
                    let mut last_residual = f64::INFINITY;
                    
                    for iter in 0..50 {
                        let old_voltages = self.node_voltages.clone();
                        let old_currents = self.source_currents.clone();
                        
                        // Build system with GMIN
                        let (a, b) = self.build_system_with_gmin(1e-12);
                        
                        // Calculate residual before solving
                        let residual = self.calculate_residual(&a, &b, &old_voltages, &old_currents);
                        
                        if residual < best_residual {
                            best_residual = residual;
                            best_solution = old_voltages.clone();
                        }
                        
                        if let Some(x) = a.lu().solve(&b) {
                            // Apply solution with damping
                            let n = self.num_nodes - 1;
                            let mut max_change = 0.0f64;
                            let mut max_voltage = 0.0f64;
                            
                            // Update voltages
                            for i in 0..n {
                                let delta = x[i] - old_voltages[i+1];
                                
                                // Limit voltage steps
                                let limited_delta = if delta.abs() > 3.0 {
                                    3.0 * delta.signum()
                                } else {
                                    delta
                                };
                                
                                self.node_voltages[i+1] = old_voltages[i+1] + damping * limited_delta;
                                max_change = max_change.max(limited_delta.abs());
                                max_voltage = max_voltage.max(self.node_voltages[i+1].abs());
                            }
                            
                            // Update source currents
                            for i in 0..vsource_count {
                                self.source_currents[i] = x[n + i];
                            }
                            
                            // Update element states
                            for &(elem_idx, pos, neg) in &self.connections {
                                let v = self.node_voltages[pos] - self.node_voltages[neg];
                                self.elements[elem_idx].set_voltage(v);
                            }
                            
                            // Check for convergence
                            if max_change < 1e-10 {
                                converged = true;
                                break;
                            }
                            
                            // Check for stalling
                            if (residual - last_residual).abs() < 1e-12 {
                                stalled_count += 1;
                                if stalled_count > 3 {
                                    damping *= 0.5;
                                    stalled_count = 0;
                                }
                            } else {
                                stalled_count = 0;
                            }
                            
                            // Adaptive damping
                            if residual < last_residual {
                                damping = (damping * 1.1).min(1.0);
                            } else if residual > last_residual * 2.0 {
                                damping *= 0.5;
                            }
                            
                            last_residual = residual;
                            
                            // Detect runaway solutions
                            if max_voltage > 1e6 {
                                // Reset to previous state
                                self.node_voltages = old_voltages;
                                self.source_currents = old_currents;
                                break;
                            }
                        } else {
                            break;
                        }
                    }
                    
                    if converged {
                        break;
                    }
                }
                
                if converged && ramp == ramp_steps {
                    println!("    Converged at full source values!");
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    println!("    Time: {:.2} ms", elapsed);
                    return self.node_voltages.clone();
                }
            }
            
            if !converged {
                println!("    Did not converge with this strategy");
            }
        }
        
        // If no strategy converged, return best solution found
        println!("  Warning: No strategy achieved full convergence");
        println!("  Returning best solution with residual: {:e}", best_residual);
        self.node_voltages = best_solution;
        self.node_voltages.clone()
    }
    
    fn initialize_with_strategy(&mut self, strategy: &InitStrategy) {
        match strategy {
            InitStrategy::Zero => {
                // All voltages start at 0
                for i in 0..self.num_nodes {
                    self.node_voltages[i] = 0.0;
                }
            }
            InitStrategy::Random(scale) => {
                // Small random perturbations
                for i in 1..self.num_nodes {
                    self.node_voltages[i] = scale * (0.5 - (i as f64 * 0.13).fract());
                }
            }
            InitStrategy::SourceBased => {
                // Try to propagate source voltages
                for &(elem_idx, pos, neg) in &self.connections {
                    if self.elements[elem_idx].element_type() == ElementType::VoltageSource {
                        let v = self.elements[elem_idx].get_voltage();
                        if pos > 0 && self.node_voltages[pos] == 0.0 {
                            self.node_voltages[pos] = v;
                        }
                    }
                }
            }
            InitStrategy::Incremental => {
                // Start with small voltage differences
                for i in 1..self.num_nodes {
                    self.node_voltages[i] = 0.1 * i as f64;
                }
            }
        }
    }
    
    fn build_system_with_gmin(&self, gmin: f64) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        // Add GMIN to all diagonal elements for numerical stability
        for i in 0..n {
            a[(i, i)] = gmin;
        }
        
        let mut vs_idx = 0;
        
        // Process elements
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
    
    fn calculate_residual(&self, a: &DMatrix<f64>, b: &DVector<f64>, 
                         voltages: &Vec<f64>, currents: &Vec<f64>) -> f64 {
        let n = self.num_nodes - 1;
        let mut x = DVector::zeros(n + currents.len());
        
        // Fill x vector
        for i in 0..n {
            x[i] = voltages[i+1];
        }
        for i in 0..currents.len() {
            x[n + i] = currents[i];
        }
        
        // Calculate Ax - b
        let residual = a * &x - b;
        
        // Return norm
        let mut sum = 0.0;
        for i in 0..residual.len() {
            sum += residual[i] * residual[i];
        }
        sum.sqrt()
    }
}

#[derive(Debug, Clone)]
enum InitStrategy {
    Zero,
    Random(f64),
    SourceBased,
    Incremental,
}

fn main() {
    println!("=== GENERIC ROBUST SOLVER ===");
    
    // Test circuit 1: Simple diode circuit
    println!("\nTest 1: Diode Circuit");
    test_diode_circuit();
    
    // Test circuit 2: Multiple diodes
    println!("\nTest 2: Multiple Diodes");
    test_multi_diode_circuit();
    
    // Test circuit 3: Resistor divider
    println!("\nTest 3: Resistor Divider");
    test_resistor_divider();
}

fn test_diode_circuit() {
    let is = 1e-12;
    let vt = 0.026;
    
    // Calculate reference
    let mut vd_ref = 0.7f64;
    for _ in 0..100 {
        let id = is * ((vd_ref / vt).exp() - 1.0);
        let f = vd_ref + id * 100.0 - 1.0;
        let df = 1.0 + (is / vt) * (vd_ref / vt).exp() * 100.0;
        vd_ref -= f / df;
    }
    let id_ref = (1.0 - vd_ref) / 100.0;
    
    println!("Reference: Vd = {:.6} V, Id = {:.6} mA", vd_ref, id_ref * 1000.0);
    
    let mut solver = GenericRobustSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(1.0)));
    let r = solver.add_element(Box::new(Resistor::new(100.0)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    let voltages = solver.solve_dc();
    
    println!("Solution: V1 = {:.6} V, V2 = {:.6} V", voltages[1], voltages[2]);
    let id = (voltages[1] - voltages[2]) / 100.0;
    println!("         Id = {:.6} mA", id * 1000.0);
    
    let v_err = ((voltages[2] - vd_ref) / vd_ref * 100.0).abs();
    let i_err = ((id - id_ref) / id_ref * 100.0).abs();
    println!("Errors: V = {:.3}%, I = {:.3}%", v_err, i_err);
    
    if v_err < 5.0 && i_err < 5.0 {
        println!("✓ SUCCESS: <5% accuracy achieved!");
    }
}

fn test_multi_diode_circuit() {
    let is = 1e-12;
    let vt = 0.026;
    
    let mut solver = GenericRobustSolver::new(4);
    
    let v = solver.add_element(Box::new(VoltageSource::new(3.0)));
    let r1 = solver.add_element(Box::new(Resistor::new(100.0)));
    let d1 = solver.add_element(Box::new(Diode::new(is, vt)));
    let d2 = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r1, 1, 2);
    solver.connect(d1, 2, 3);
    solver.connect(d2, 3, 0);
    
    let voltages = solver.solve_dc();
    
    println!("Solution: V1 = {:.6} V, V2 = {:.6} V, V3 = {:.6} V", 
             voltages[1], voltages[2], voltages[3]);
    let id = (voltages[1] - voltages[2]) / 100.0;
    println!("         Id = {:.6} mA", id * 1000.0);
    println!("         Vd1 = {:.6} V, Vd2 = {:.6} V", 
             voltages[2] - voltages[3], voltages[3]);
}

fn test_resistor_divider() {
    let mut solver = GenericRobustSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(10.0)));
    let r1 = solver.add_element(Box::new(Resistor::new(1000.0)));
    let r2 = solver.add_element(Box::new(Resistor::new(1000.0)));
    
    solver.connect(v, 1, 0);
    solver.connect(r1, 1, 2);
    solver.connect(r2, 2, 0);
    
    let voltages = solver.solve_dc();
    
    println!("Solution: V1 = {:.6} V, V2 = {:.6} V", voltages[1], voltages[2]);
    let expected = 5.0; // Voltage divider
    let error = ((voltages[2] - expected) / expected * 100.0).abs();
    println!("Expected: V2 = {:.6} V, Error = {:.3}%", expected, error);
    
    if error < 0.01 {
        println!("✓ SUCCESS: Voltage divider solved correctly!");
    }
}