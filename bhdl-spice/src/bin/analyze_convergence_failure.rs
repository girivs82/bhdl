/// Analyze Convergence Failure - Deep diagnostic
/// 
/// This solver analyzes why Newton-Raphson fails and the matrix conditioning

use nalgebra::{DMatrix, DVector};

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

pub struct ConvergenceAnalyzer {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    num_nodes: usize,
}

impl ConvergenceAnalyzer {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
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
    
    pub fn analyze_convergence(&mut self) {
        println!("=== CONVERGENCE ANALYSIS ===\n");
        
        // Test different scenarios
        let test_cases = vec![
            ("Zero voltage", vec![0.0, 0.0, 0.0]),
            ("Small forward bias", vec![0.0, 0.3, 0.2]),
            ("Normal forward bias", vec![0.0, 1.0, 0.6]),
            ("Large forward bias", vec![0.0, 5.0, 3.0]),
            ("Reverse bias", vec![0.0, -1.0, -0.5]),
            ("Extreme case", vec![0.0, 100.0, 50.0]),
        ];
        
        for (name, voltages) in test_cases {
            println!("Test case: {}", name);
            self.node_voltages = voltages.clone();
            
            // Update element voltages
            for &(elem_idx, pos, neg) in &self.connections {
                let v = self.node_voltages[pos] - self.node_voltages[neg];
                self.elements[elem_idx].set_voltage(v);
            }
            
            // Build system
            let (a, b) = self.build_system();
            
            // Analyze matrix
            println!("  Initial voltages: V1={}, V2={}", voltages[1], voltages[2]);
            
            // Matrix properties
            let det = a.determinant();
            println!("  Matrix determinant: {:e}", det);
            
            // Condition number estimate
            let norm_a = matrix_norm(&a);
            if let Some(a_inv) = a.clone().try_inverse() {
                let norm_a_inv = matrix_norm(&a_inv);
                let cond = norm_a * norm_a_inv;
                println!("  Condition number: {:e}", cond);
                
                if cond > 1e10 {
                    println!("  WARNING: Ill-conditioned matrix!");
                }
            } else {
                println!("  WARNING: Matrix is singular!");
            }
            
            // Print matrix for small systems
            if a.nrows() <= 3 {
                println!("  Matrix A:");
                for i in 0..a.nrows() {
                    print!("    [");
                    for j in 0..a.ncols() {
                        print!("{:12.6e} ", a[(i, j)]);
                    }
                    println!("]");
                }
                println!("  Vector b: {:?}", b.as_slice());
            }
            
            // Try to solve
            if let Some(x) = a.lu().solve(&b) {
                println!("  Solution: {:?}", x.as_slice());
                
                // Check residual
                let residual = &a * &x - &b;
                let residual_norm = vector_norm(&residual);
                println!("  Residual norm: {:e}", residual_norm);
                
                // Check if solution is reasonable
                for (i, &val) in x.as_slice().iter().enumerate() {
                    if val.abs() > 1e6 {
                        println!("  WARNING: Solution component {} is very large: {:e}", i, val);
                    }
                    if val.is_nan() || val.is_infinite() {
                        println!("  ERROR: Solution component {} is NaN or infinite!", i);
                    }
                }
            } else {
                println!("  ERROR: Failed to solve system!");
            }
            
            // Analyze diode behavior at this operating point
            if let Some(diode_elem) = self.elements.iter().find(|e| e.element_type() == ElementType::Diode) {
                let vd = diode_elem.get_voltage();
                let id = diode_elem.current_at_voltage(vd);
                let gd = diode_elem.conductance_at_voltage(vd);
                
                println!("  Diode analysis:");
                println!("    Voltage: {:e} V", vd);
                println!("    Current: {:e} A", id);
                println!("    Conductance: {:e} S", gd);
                
                if gd > 1e6 {
                    println!("    WARNING: Very high conductance!");
                }
                if gd < 1e-12 {
                    println!("    WARNING: Very low conductance!");
                }
            }
            
            println!();
        }
        
        // Test Newton-Raphson convergence
        println!("=== NEWTON-RAPHSON CONVERGENCE TEST ===\n");
        
        // Start from different initial guesses
        let initial_guesses = vec![
            vec![0.0, 1.0, 0.0],   // Poor guess
            vec![0.0, 1.0, 0.3],   // Moderate guess
            vec![0.0, 1.0, 0.6],   // Good guess
            vec![0.0, 1.0, 0.8],   // High guess
        ];
        
        for (i, guess) in initial_guesses.iter().enumerate() {
            println!("Initial guess {}: V2={}", i + 1, guess[2]);
            self.node_voltages = guess.clone();
            
            let mut converged = false;
            for iter in 0..10 {
                let old_v = self.node_voltages.clone();
                let (a, b) = self.build_system();
                
                if let Some(x) = a.lu().solve(&b) {
                    // Update voltages
                    for j in 0..x.len() {
                        if j < self.num_nodes - 1 {
                            self.node_voltages[j+1] = x[j];
                        }
                    }
                    
                    // Check convergence
                    let mut max_change = 0.0f64;
                    for j in 1..self.num_nodes {
                        max_change = max_change.max((self.node_voltages[j] - old_v[j]).abs());
                    }
                    
                    println!("  Iter {}: V2={:.6}, change={:e}", iter, self.node_voltages[2], max_change);
                    
                    if max_change < 1e-9 {
                        converged = true;
                        break;
                    }
                } else {
                    println!("  Iter {}: Matrix solve failed!", iter);
                    break;
                }
            }
            
            if !converged {
                println!("  Failed to converge!");
            }
            println!();
        }
    }
    
    fn build_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.connections.iter()
            .filter(|&&(i, _, _)| self.elements[i].element_type() == ElementType::VoltageSource)
            .count();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        let mut vs_idx = 0;
        
        for &(elem_idx, pos, neg) in &self.connections {
            let elem = &self.elements[elem_idx];
            
            match elem.element_type() {
                ElementType::VoltageSource => {
                    let row = n + vs_idx;
                    if pos > 0 {
                        a[(pos-1, row)] = -1.0;
                        a[(row, pos-1)] = 1.0;
                    }
                    if neg > 0 {
                        a[(neg-1, row)] = 1.0;
                        a[(row, neg-1)] = -1.0;
                    }
                    b[row] = elem.get_voltage();
                    vs_idx += 1;
                }
                _ => {
                    let v_elem = self.node_voltages[pos] - self.node_voltages[neg];
                    let g = elem.conductance_at_voltage(v_elem);
                    let i_norton = if elem.is_nonlinear() {
                        elem.current_at_voltage(v_elem) - g * v_elem
                    } else {
                        0.0
                    };
                    
                    if pos > 0 {
                        a[(pos-1, pos-1)] += g;
                        b[pos-1] += i_norton;
                    }
                    if neg > 0 {
                        a[(neg-1, neg-1)] += g;
                        b[neg-1] -= i_norton;
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

fn matrix_norm(a: &DMatrix<f64>) -> f64 {
    // Frobenius norm
    let mut sum = 0.0;
    for i in 0..a.nrows() {
        for j in 0..a.ncols() {
            sum += a[(i, j)] * a[(i, j)];
        }
    }
    sum.sqrt()
}

fn vector_norm(v: &DVector<f64>) -> f64 {
    let mut sum = 0.0;
    for &val in v.as_slice() {
        sum += val * val;
    }
    sum.sqrt()
}

fn main() {
    let mut analyzer = ConvergenceAnalyzer::new(3);
    
    // Build circuit: V1 -> R -> D -> GND
    let v = analyzer.add_element(Box::new(VoltageSource::new(1.0)));
    let r = analyzer.add_element(Box::new(Resistor::new(100.0)));
    let d = analyzer.add_element(Box::new(Diode::new(1e-12, 0.026)));
    
    analyzer.connect(v, 1, 0);
    analyzer.connect(r, 1, 2);
    analyzer.connect(d, 2, 0);
    
    analyzer.analyze_convergence();
}