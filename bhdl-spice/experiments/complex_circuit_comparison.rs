/// Complex Circuit Comparison Test
/// 
/// Tests both hybrid (80%) and smart damping approaches on challenging circuits:
/// 1. Multi-diode bridge rectifier
/// 2. LED array with current limiting
/// 3. Voltage regulator with feedback
/// 4. Power supply with protection circuits
/// 5. Mixed linear/nonlinear with multiple operating points

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

// Hybrid solver (80% transition)
pub struct HybridLogGradientSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl HybridLogGradientSolver {
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
        
        // Phase 1: Fast ramping (0-80%)
        let mut ramp = 0.0;
        let phase1_end = 0.8;
        let phase1_step = 0.05; // Larger steps for speed
        
        while ramp < phase1_end {
            ramp = f64::min(ramp + phase1_step, phase1_end);
            
            // Set sources to current ramp
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            // Fast solve with relaxed parameters
            self.solve_at_ramp_fast(&mut total_iterations);
        }
        
        // Phase 2: Accurate convergence (80-100%)
        let phase2_step = 0.01; // Smaller steps for accuracy
        
        while ramp < 0.999 {
            ramp = f64::min(ramp + phase2_step, 1.0);
            
            // Set sources to current ramp
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            // Accurate solve
            self.solve_at_ramp_accurate(&mut total_iterations);
        }
        
        // Get all diode voltages
        let mut diode_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        diode_voltages.push(self.node_voltages[pos] - self.node_voltages[neg]);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (diode_voltages, self.source_currents[0].abs(), total_iterations, elapsed)
    }
    
    fn solve_at_ramp_fast(&mut self, total_iterations: &mut usize) {
        let max_iter = 30;
        let tol = 1e-8; // Relaxed tolerance
        let damping = 0.5; // Underdamped for speed
        
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
    
    fn solve_at_ramp_accurate(&mut self, total_iterations: &mut usize) {
        let max_iter = 100;
        let tol = 1e-12; // High precision
        let damping = 0.7; // Moderate damping
        
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

// Smart damping solver (highest accuracy)
#[derive(Debug, Clone, Copy)]
enum DampingStrategy {
    ImmediateOverdamp,  // Strategy 1: Immediate overdamping on oscillation
}

struct SmartDampingSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    
    // Damping control
    strategy: DampingStrategy,
    last_gradients: Vec<f64>,
    sign_changes: usize,
    damping_factor: f64,
    adaptive_step: f64,
}

impl SmartDampingSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            
            strategy: DampingStrategy::ImmediateOverdamp,
            last_gradients: Vec::new(),
            sign_changes: 0,
            damping_factor: 0.3, // Start underdamped
            adaptive_step: 0.01,
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
        
        // Smart damping ramp with adaptive control
        let mut ramp = 0.0;
        
        while ramp < 0.999 {
            ramp = f64::min(ramp + self.adaptive_step, 1.0);
            
            // Set sources to current ramp
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp);
            }
            
            // Solve with smart damping
            self.solve_with_smart_damping(&mut total_iterations);
            
            // Update damping based on oscillation detection
            self.update_damping_parameters();
        }
        
        // Get all diode voltages
        let mut diode_voltages = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.is_nonlinear() {
                for &(idx, pos, neg) in &self.connections {
                    if idx == i {
                        diode_voltages.push(self.node_voltages[pos] - self.node_voltages[neg]);
                    }
                }
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (diode_voltages, self.source_currents[0].abs(), total_iterations, elapsed)
    }
    
    fn solve_with_smart_damping(&mut self, total_iterations: &mut usize) {
        let max_iter = 100;
        let tol = 1e-12;
        
        for _ in 0..max_iter {
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Calculate gradient before applying changes
                let mut current_gradient = 0.0;
                for i in 0..n {
                    current_gradient += (x[i] - old_v[i+1]).abs();
                }
                
                // Detect oscillations
                if let Some(&last_grad) = self.last_gradients.last() {
                    if (current_gradient - last_grad).signum() != 
                       self.last_gradients.get(self.last_gradients.len().saturating_sub(2))
                           .map(|&prev| (last_grad - prev).signum())
                           .unwrap_or(0.0) && current_gradient > 1e-10 {
                        self.sign_changes += 1;
                    }
                }
                
                // Apply updates with current damping
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    self.node_voltages[i+1] = old_v[i+1] + self.damping_factor * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..self.source_currents.len() {
                    self.source_currents[i] = x[n + i];
                }
                
                for &(elem_idx, pos, neg) in &self.connections {
                    let v = self.node_voltages[pos] - self.node_voltages[neg];
                    self.elements[elem_idx].set_voltage(v);
                }
                
                // Store gradient history
                self.last_gradients.push(current_gradient);
                if self.last_gradients.len() > 5 {
                    self.last_gradients.remove(0);
                }
                
                if max_change < tol {
                    return;
                }
            } else {
                return;
            }
        }
    }
    
    fn update_damping_parameters(&mut self) {
        match self.strategy {
            DampingStrategy::ImmediateOverdamp => {
                if self.sign_changes > 2 {
                    // Immediate overdamping when oscillation detected
                    self.damping_factor = 0.9;
                    self.adaptive_step *= 0.5; // Smaller steps
                } else {
                    // Gradually return to underdamped for speed
                    self.damping_factor = (self.damping_factor + 0.3) / 2.0;
                    self.adaptive_step = self.adaptive_step.max(0.005);
                }
            }
        }
        
        // Reset sign change counter periodically
        if self.sign_changes > 5 {
            self.sign_changes = 0;
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

// SPICE reference solver
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
    println!("=== COMPLEX CIRCUIT COMPARISON TEST ===");
    println!("Testing hybrid (80%) vs smart damping on challenging circuits\n");
    
    // Test Case 1: Multi-diode bridge rectifier
    println!("🔧 TEST 1: MULTI-DIODE BRIDGE RECTIFIER");
    println!("   Circuit: 4 diodes + 4 resistors + 2 voltage sources");
    test_bridge_rectifier();
    
    // Test Case 2: LED array with current limiting
    println!("\n🔧 TEST 2: LED ARRAY WITH CURRENT LIMITING");
    println!("   Circuit: 5 LEDs + 5 current limiting resistors");
    test_led_array();
    
    // Test Case 3: Voltage regulator with feedback
    println!("\n🔧 TEST 3: VOLTAGE REGULATOR WITH FEEDBACK");
    println!("   Circuit: Zener diode + feedback resistors");
    test_voltage_regulator();
    
    // Test Case 4: Power supply with protection
    println!("\n🔧 TEST 4: POWER SUPPLY WITH PROTECTION");
    println!("   Circuit: Multiple diodes + crowbar protection");
    test_power_supply_protection();
    
    // Test Case 5: Mixed linear/nonlinear
    println!("\n🔧 TEST 5: MIXED LINEAR/NONLINEAR CIRCUIT");
    println!("   Circuit: Multiple operating points + complex topology");
    test_mixed_circuit();
}

fn test_bridge_rectifier() {
    // Bridge rectifier: 4 diodes, 4 resistors, 2 AC sources (simulated as DC for testing)
    println!("  Testing bridge rectifier with 4 diodes...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(6); // 6 nodes
    
    // Add components
    let vs1 = hybrid.add_element(Box::new(VoltageSource::new(5.0)));
    let vs2 = hybrid.add_element(Box::new(VoltageSource::new(-5.0)));
    let d1 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d2 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d3 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d4 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let r1 = hybrid.add_element(Box::new(Resistor::new(100.0)));
    let r2 = hybrid.add_element(Box::new(Resistor::new(100.0)));
    let r3 = hybrid.add_element(Box::new(Resistor::new(100.0)));
    let r_load = hybrid.add_element(Box::new(Resistor::new(1000.0)));
    
    // Connect bridge rectifier
    hybrid.connect(vs1, 1, 0);  // AC source 1
    hybrid.connect(vs2, 2, 0);  // AC source 2
    hybrid.connect(d1, 1, 3);   // Diode 1: AC1 -> DC+
    hybrid.connect(d2, 2, 3);   // Diode 2: AC2 -> DC+
    hybrid.connect(d3, 0, 1);   // Diode 3: GND -> AC1
    hybrid.connect(d4, 0, 2);   // Diode 4: GND -> AC2
    hybrid.connect(r1, 1, 4);   // Input resistor 1
    hybrid.connect(r2, 2, 5);   // Input resistor 2
    hybrid.connect(r3, 3, 4);   // Coupling resistor
    hybrid.connect(r_load, 3, 0); // Load resistor
    
    let (diode_voltages_hybrid, current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(6);
    
    // Add same components
    let vs1 = smart.add_element(Box::new(VoltageSource::new(5.0)));
    let vs2 = smart.add_element(Box::new(VoltageSource::new(-5.0)));
    let d1 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d2 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d3 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d4 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let r1 = smart.add_element(Box::new(Resistor::new(100.0)));
    let r2 = smart.add_element(Box::new(Resistor::new(100.0)));
    let r3 = smart.add_element(Box::new(Resistor::new(100.0)));
    let r_load = smart.add_element(Box::new(Resistor::new(1000.0)));
    
    // Connect same circuit
    smart.connect(vs1, 1, 0);
    smart.connect(vs2, 2, 0);
    smart.connect(d1, 1, 3);
    smart.connect(d2, 2, 3);
    smart.connect(d3, 0, 1);
    smart.connect(d4, 0, 2);
    smart.connect(r1, 1, 4);
    smart.connect(r2, 2, 5);
    smart.connect(r3, 3, 4);
    smart.connect(r_load, 3, 0);
    
    let (diode_voltages_smart, current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} diodes, {:.1}ms, {} iters", 
             diode_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} diodes, {:.1}ms, {} iters", 
             diode_voltages_smart.len(), time_smart, iters_smart);
    println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
}

fn test_led_array() {
    // LED array: 5 LEDs with current limiting resistors
    println!("  Testing LED array with 5 LEDs...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(8); // 8 nodes
    
    // Add components
    let vs = hybrid.add_element(Box::new(VoltageSource::new(12.0)));
    let r_limit = hybrid.add_element(Box::new(Resistor::new(220.0))); // Current limiting
    
    // 5 LEDs with different forward voltages
    let led1 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led2 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led3 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led4 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led5 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    
    // Individual resistors for each LED
    let r1 = hybrid.add_element(Box::new(Resistor::new(470.0)));
    let r2 = hybrid.add_element(Box::new(Resistor::new(470.0)));
    let r3 = hybrid.add_element(Box::new(Resistor::new(470.0)));
    let r4 = hybrid.add_element(Box::new(Resistor::new(470.0)));
    let r5 = hybrid.add_element(Box::new(Resistor::new(470.0)));
    
    // Connect LED array
    hybrid.connect(vs, 1, 0);      // 12V source
    hybrid.connect(r_limit, 1, 2); // Current limiting resistor
    hybrid.connect(r1, 2, 3);      // LED1 resistor
    hybrid.connect(led1, 3, 0);    // LED1
    hybrid.connect(r2, 2, 4);      // LED2 resistor
    hybrid.connect(led2, 4, 0);    // LED2
    hybrid.connect(r3, 2, 5);      // LED3 resistor
    hybrid.connect(led3, 5, 0);    // LED3
    hybrid.connect(r4, 2, 6);      // LED4 resistor
    hybrid.connect(led4, 6, 0);    // LED4
    hybrid.connect(r5, 2, 7);      // LED5 resistor
    hybrid.connect(led5, 7, 0);    // LED5
    
    let (diode_voltages_hybrid, current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver  
    let mut smart = SmartDampingSolver::new(8);
    
    // Add same components
    let vs = smart.add_element(Box::new(VoltageSource::new(12.0)));
    let r_limit = smart.add_element(Box::new(Resistor::new(220.0)));
    let led1 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led2 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led3 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led4 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let led5 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let r1 = smart.add_element(Box::new(Resistor::new(470.0)));
    let r2 = smart.add_element(Box::new(Resistor::new(470.0)));
    let r3 = smart.add_element(Box::new(Resistor::new(470.0)));
    let r4 = smart.add_element(Box::new(Resistor::new(470.0)));
    let r5 = smart.add_element(Box::new(Resistor::new(470.0)));
    
    // Connect same circuit
    smart.connect(vs, 1, 0);
    smart.connect(r_limit, 1, 2);
    smart.connect(r1, 2, 3);
    smart.connect(led1, 3, 0);
    smart.connect(r2, 2, 4);
    smart.connect(led2, 4, 0);
    smart.connect(r3, 2, 5);
    smart.connect(led3, 5, 0);
    smart.connect(r4, 2, 6);
    smart.connect(led4, 6, 0);
    smart.connect(r5, 2, 7);
    smart.connect(led5, 7, 0);
    
    let (diode_voltages_smart, current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} LEDs, {:.1}ms, {} iters", 
             diode_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} LEDs, {:.1}ms, {} iters", 
             diode_voltages_smart.len(), time_smart, iters_smart);
    println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
}

fn test_voltage_regulator() {
    // Simple voltage regulator with Zener feedback
    println!("  Testing voltage regulator with feedback...");
    
    // Reference values for comparison
    let (vd_ref, id_ref) = spice_reference(15.0, 100.0, 1e-12, 0.026);
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(4);
    
    let vs = hybrid.add_element(Box::new(VoltageSource::new(15.0)));
    let r_series = hybrid.add_element(Box::new(Resistor::new(100.0)));
    let zener = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026))); // Zener diode
    let r_feedback = hybrid.add_element(Box::new(Resistor::new(1000.0)));
    let r_load = hybrid.add_element(Box::new(Resistor::new(500.0)));
    
    hybrid.connect(vs, 1, 0);
    hybrid.connect(r_series, 1, 2);
    hybrid.connect(zener, 2, 0);
    hybrid.connect(r_feedback, 2, 3);
    hybrid.connect(r_load, 3, 0);
    
    let (diode_voltages_hybrid, current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(4);
    
    let vs = smart.add_element(Box::new(VoltageSource::new(15.0)));
    let r_series = smart.add_element(Box::new(Resistor::new(100.0)));
    let zener = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let r_feedback = smart.add_element(Box::new(Resistor::new(1000.0)));
    let r_load = smart.add_element(Box::new(Resistor::new(500.0)));
    
    smart.connect(vs, 1, 0);
    smart.connect(r_series, 1, 2);
    smart.connect(zener, 2, 0);
    smart.connect(r_feedback, 2, 3);
    smart.connect(r_load, 3, 0);
    
    let (diode_voltages_smart, current_smart, iters_smart, time_smart) = smart.solve();
    
    // Calculate errors
    let error_hybrid = if !diode_voltages_hybrid.is_empty() {
        ((diode_voltages_hybrid[0] - vd_ref) / vd_ref * 100.0).abs()
    } else { 100.0 };
    
    let error_smart = if !diode_voltages_smart.is_empty() {
        ((diode_voltages_smart[0] - vd_ref) / vd_ref * 100.0).abs()
    } else { 100.0 };
    
    println!("    Reference: Vd={:.6}V, Id={:.3}mA", vd_ref, id_ref * 1000.0);
    println!("    Hybrid (80%):   Vd={:.6}V, {:.3}% error, {:.1}ms", 
             diode_voltages_hybrid.get(0).unwrap_or(&0.0), error_hybrid, time_hybrid);
    println!("    Smart Damping:  Vd={:.6}V, {:.3}% error, {:.1}ms", 
             diode_voltages_smart.get(0).unwrap_or(&0.0), error_smart, time_smart);
    println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
}

fn test_power_supply_protection() {
    // Power supply with crowbar protection (multiple diodes)
    println!("  Testing power supply with protection...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(5);
    
    let vs = hybrid.add_element(Box::new(VoltageSource::new(24.0)));
    let r_input = hybrid.add_element(Box::new(Resistor::new(10.0)));
    let d_rectifier = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d_protection = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d_crowbar = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let r_protection = hybrid.add_element(Box::new(Resistor::new(50.0)));
    let r_load = hybrid.add_element(Box::new(Resistor::new(200.0)));
    
    hybrid.connect(vs, 1, 0);
    hybrid.connect(r_input, 1, 2);
    hybrid.connect(d_rectifier, 2, 3);
    hybrid.connect(d_protection, 3, 4);
    hybrid.connect(r_protection, 4, 0);
    hybrid.connect(d_crowbar, 0, 4); // Reverse protection
    hybrid.connect(r_load, 3, 0);
    
    let (diode_voltages_hybrid, current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(5);
    
    let vs = smart.add_element(Box::new(VoltageSource::new(24.0)));
    let r_input = smart.add_element(Box::new(Resistor::new(10.0)));
    let d_rectifier = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d_protection = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d_crowbar = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let r_protection = smart.add_element(Box::new(Resistor::new(50.0)));
    let r_load = smart.add_element(Box::new(Resistor::new(200.0)));
    
    smart.connect(vs, 1, 0);
    smart.connect(r_input, 1, 2);
    smart.connect(d_rectifier, 2, 3);
    smart.connect(d_protection, 3, 4);
    smart.connect(r_protection, 4, 0);
    smart.connect(d_crowbar, 0, 4);
    smart.connect(r_load, 3, 0);
    
    let (diode_voltages_smart, current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} diodes, {:.1}ms, {} iters", 
             diode_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} diodes, {:.1}ms, {} iters", 
             diode_voltages_smart.len(), time_smart, iters_smart);
    println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
}

fn test_mixed_circuit() {
    // Complex mixed circuit with multiple operating points
    println!("  Testing mixed linear/nonlinear circuit...");
    
    // Create hybrid solver
    let mut hybrid = HybridLogGradientSolver::new(6);
    
    let vs1 = hybrid.add_element(Box::new(VoltageSource::new(10.0)));
    let vs2 = hybrid.add_element(Box::new(VoltageSource::new(5.0)));
    let r1 = hybrid.add_element(Box::new(Resistor::new(100.0)));
    let r2 = hybrid.add_element(Box::new(Resistor::new(200.0)));
    let r3 = hybrid.add_element(Box::new(Resistor::new(300.0)));
    let r4 = hybrid.add_element(Box::new(Resistor::new(150.0)));
    let d1 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d2 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.030))); // Different Vt
    let d3 = hybrid.add_element(Box::new(Diode::new(1e-12, 0.022))); // Different Vt
    
    // Complex interconnection
    hybrid.connect(vs1, 1, 0);
    hybrid.connect(vs2, 2, 0);
    hybrid.connect(r1, 1, 3);
    hybrid.connect(r2, 2, 4);
    hybrid.connect(r3, 3, 4);
    hybrid.connect(r4, 4, 5);
    hybrid.connect(d1, 3, 5);
    hybrid.connect(d2, 4, 0);
    hybrid.connect(d3, 5, 0);
    
    let (diode_voltages_hybrid, current_hybrid, iters_hybrid, time_hybrid) = hybrid.solve();
    
    // Create smart damping solver
    let mut smart = SmartDampingSolver::new(6);
    
    let vs1 = smart.add_element(Box::new(VoltageSource::new(10.0)));
    let vs2 = smart.add_element(Box::new(VoltageSource::new(5.0)));
    let r1 = smart.add_element(Box::new(Resistor::new(100.0)));
    let r2 = smart.add_element(Box::new(Resistor::new(200.0)));
    let r3 = smart.add_element(Box::new(Resistor::new(300.0)));
    let r4 = smart.add_element(Box::new(Resistor::new(150.0)));
    let d1 = smart.add_element(Box::new(Diode::new(1e-12, 0.026)));
    let d2 = smart.add_element(Box::new(Diode::new(1e-12, 0.030)));
    let d3 = smart.add_element(Box::new(Diode::new(1e-12, 0.022)));
    
    smart.connect(vs1, 1, 0);
    smart.connect(vs2, 2, 0);
    smart.connect(r1, 1, 3);
    smart.connect(r2, 2, 4);
    smart.connect(r3, 3, 4);
    smart.connect(r4, 4, 5);
    smart.connect(d1, 3, 5);
    smart.connect(d2, 4, 0);
    smart.connect(d3, 5, 0);
    
    let (diode_voltages_smart, current_smart, iters_smart, time_smart) = smart.solve();
    
    println!("    Hybrid (80%):   {} diodes, {:.1}ms, {} iters", 
             diode_voltages_hybrid.len(), time_hybrid, iters_hybrid);
    println!("    Smart Damping:  {} diodes, {:.1}ms, {} iters", 
             diode_voltages_smart.len(), time_smart, iters_smart);
    println!("    Speed ratio: {:.1}x", time_smart / time_hybrid);
    
    // Show voltage details for comparison
    println!("    Diode voltages (Hybrid):   {:?}", 
             diode_voltages_hybrid.iter().map(|&v| format!("{:.3}V", v)).collect::<Vec<_>>());
    println!("    Diode voltages (Smart):    {:?}", 
             diode_voltages_smart.iter().map(|&v| format!("{:.3}V", v)).collect::<Vec<_>>());
}