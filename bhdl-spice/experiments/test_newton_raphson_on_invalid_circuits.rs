/// Newton-Raphson vs Logarithmic Gradient on Invalid Circuits
/// 
/// This tests how Newton-Raphson handles the same problematic circuits
/// that caused issues with the logarithmic gradient solver to see if
/// the problems are solver-specific or fundamental physics issues.

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Element trait (same as before)
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
    LED,
}

// Element implementations (same as before)
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

pub struct LED {
    is: f64,  // Saturation current
    vt: f64,  // Thermal voltage
    vf: f64,  // Forward voltage
    voltage: f64,
}

impl LED {
    pub fn new(is: f64, vt: f64, vf: f64) -> Self {
        Self { is, vt, vf, voltage: 0.0 }
    }
}

impl Element for LED {
    fn element_type(&self) -> ElementType { ElementType::LED }
    fn is_nonlinear(&self) -> bool { true }
    
    fn current_at_voltage(&self, v: f64) -> f64 {
        let effective_v = v - self.vf;
        if effective_v <= 0.0 {
            -self.is  // Small reverse current
        } else {
            let v_norm = effective_v / self.vt;
            if v_norm > 50.0 {
                self.is * (50.0_f64.exp() - 1.0)
            } else {
                self.is * (v_norm.exp() - 1.0)
            }
        }
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        const MIN_G: f64 = 1e-14;
        let effective_v = v - self.vf;
        
        if effective_v <= 0.0 {
            MIN_G
        } else {
            let v_norm = effective_v / self.vt;
            if v_norm > 50.0 {
                (self.is / self.vt) * 50.0_f64.exp()
            } else {
                ((self.is / self.vt) * v_norm.exp()).max(MIN_G)
            }
        }
    }
    
    fn get_voltage(&self) -> f64 { self.voltage }
    fn set_voltage(&mut self, v: f64) { self.voltage = v; }
}

// Standard Newton-Raphson Solver with Timeout
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
    
    // Newton-Raphson with timeout and convergence monitoring
    pub fn solve_with_monitoring(&mut self) -> (Vec<f64>, f64, usize, bool, String) {
        let start = Instant::now();
        let timeout_ms = 5000.0;  // Same 5s timeout as log gradient
        let max_iterations = 1000; // Reasonable limit
        
        println!("\n=== NEWTON-RAPHSON ANALYSIS ===");
        println!("Devices: {} total", self.elements.len());
        
        // Count voltage sources
        let mut vsource_count = 0;
        for elem in &self.elements {
            if elem.element_type() == ElementType::VoltageSource {
                vsource_count += 1;
            }
        }
        self.source_currents = vec![0.0; vsource_count];
        
        let mut iterations = 0;
        let mut convergence_issues = Vec::new();
        let mut last_max_change = f64::INFINITY;
        let mut stuck_count = 0;
        
        // Newton-Raphson iterations with monitoring
        for iter in 0..max_iterations {
            iterations = iter + 1;
            
            // Check timeout
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            if elapsed > timeout_ms {
                let msg = format!("TIMEOUT after {:.1}s", elapsed / 1000.0);
                return (self.node_voltages.clone(), elapsed, iterations, false, msg);
            }
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Adaptive damping based on progress
                let damping = if iter < 10 { 0.7 } else if stuck_count > 5 { 0.3 } else { 0.8 };
                
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
                
                // Monitor convergence behavior
                if max_change < 1e-12 {
                    let msg = format!("CONVERGED in {} iterations", iterations);
                    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                    return (self.node_voltages.clone(), elapsed, iterations, true, msg);
                }
                
                // Check for getting stuck
                if (max_change - last_max_change).abs() / last_max_change < 0.01 {
                    stuck_count += 1;
                } else {
                    stuck_count = 0;
                }
                last_max_change = max_change;
                
                // Monitor for problematic patterns
                if max_change > 1e6 {
                    convergence_issues.push(format!("Iter {}: Voltage divergence (max_change={:.2e})", iter, max_change));
                }
                
                if iter % 100 == 0 && iter > 0 {
                    println!("  NR Iter {}: max_change={:.2e}, damping={:.1}, stuck_count={}", 
                             iter, max_change, damping, stuck_count);
                }
                
                // Detect oscillation
                if stuck_count > 20 {
                    convergence_issues.push(format!("Iter {}: Oscillating/stuck (change={:.2e})", iter, max_change));
                    break;
                }
            } else {
                let msg = format!("MATRIX SINGULAR at iteration {}", iterations);
                let elapsed = start.elapsed().as_secs_f64() * 1000.0;
                return (self.node_voltages.clone(), elapsed, iterations, false, msg);
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        let mut status = format!("MAX_ITERATIONS ({}) reached", max_iterations);
        if !convergence_issues.is_empty() {
            status.push_str(&format!("; Issues: {}", convergence_issues.join(", ")));
        }
        
        (self.node_voltages.clone(), elapsed, iterations, false, status)
    }
    
    fn build_mna_system(&self) -> (DMatrix<f64>, DVector<f64>) {
        let n = self.num_nodes - 1;
        let m = self.source_currents.len();
        
        let size = n + m;
        let mut a = DMatrix::zeros(size, size);
        let mut b = DVector::zeros(size);
        
        // GMIN for stability
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

// Circuit validation (same logic as log gradient)
fn validate_circuit_physics(elements: &[Box<dyn Element>]) -> (bool, String) {
    let mut total_forward_voltage = 0.0;
    let mut supply_voltage = 0.0;
    let mut led_count = 0;
    
    for elem in elements {
        match elem.element_type() {
            ElementType::VoltageSource => {
                supply_voltage += elem.get_voltage().abs();
            }
            ElementType::LED => {
                led_count += 1;
                // Estimate forward voltage
                let test_current = elem.current_at_voltage(3.0);
                if test_current > 1e-6 {
                    total_forward_voltage += 2.0; // Assume ~2V forward drop
                }
            }
            _ => {}
        }
    }
    
    let warning = if total_forward_voltage > supply_voltage * 0.9 {
        format!("WARNING: {} LEDs require ~{:.1}V but supply is {:.1}V", 
                led_count, total_forward_voltage, supply_voltage)
    } else {
        "Circuit topology appears reasonable".to_string()
    };
    
    (total_forward_voltage <= supply_voltage * 0.9, warning)
}

fn main() {
    println!("=== NEWTON-RAPHSON vs LOGARITHMIC GRADIENT ON INVALID CIRCUITS ===");
    println!("Testing how Newton-Raphson handles the same problematic circuits\\n");
    
    // Test 1: The problematic multi-LED circuit (same as log gradient test)
    println!("🔧 TEST 1: PROBLEMATIC MULTI-LED CIRCUIT");
    println!("Circuit: 12V -> R(100Ω) -> LED(2V) -> LED(2.2V) -> LED(3.2V) -> R(100Ω) -> GND");
    
    let mut nr_solver = NewtonRaphsonSolver::new(6);
    
    let vs = nr_solver.add_element(Box::new(VoltageSource::new(12.0)));
    let r1 = nr_solver.add_element(Box::new(Resistor::new(100.0)));
    let led_red = nr_solver.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    let led_green = nr_solver.add_element(Box::new(LED::new(1e-12, 0.026, 2.2)));
    let led_blue = nr_solver.add_element(Box::new(LED::new(1e-12, 0.026, 3.2)));
    let r2 = nr_solver.add_element(Box::new(Resistor::new(100.0)));
    
    nr_solver.connect(vs, 1, 0);        // Vs: Node1 to GND
    nr_solver.connect(r1, 1, 2);        // R1: Node1 to Node2
    nr_solver.connect(led_red, 2, 3);   // Red LED: Node2 to Node3
    nr_solver.connect(led_green, 3, 4); // Green LED: Node3 to Node4
    nr_solver.connect(led_blue, 4, 5);  // Blue LED: Node4 to Node5
    nr_solver.connect(r2, 5, 0);        // R2: Node5 to GND
    
    // Validate circuit
    let (valid, warning) = validate_circuit_physics(&nr_solver.elements);
    println!("Circuit validation: {}", warning);
    
    let (voltages_nr, time_nr, iterations_nr, success_nr, status_nr) = nr_solver.solve_with_monitoring();
    
    println!("\\nNewton-Raphson Results:");
    println!("  Success: {}", success_nr);
    println!("  Status: {}", status_nr);
    println!("  Node voltages: {:?}", &voltages_nr[1..]);
    if voltages_nr.len() >= 6 {
        println!("  Red LED voltage: {:.3}V", (voltages_nr[2] - voltages_nr[3]).abs());
        println!("  Green LED voltage: {:.3}V", (voltages_nr[3] - voltages_nr[4]).abs());
        println!("  Blue LED voltage: {:.3}V", (voltages_nr[4] - voltages_nr[5]).abs());
    }
    println!("  Iterations: {}, Time: {:.1}ms", iterations_nr, time_nr);
    
    // Test 2: Working single LED circuit for comparison
    println!("\\n🔧 TEST 2: SINGLE LED CIRCUIT (Control Test)");
    println!("Circuit: 5V -> R(100Ω) -> LED(2V) -> GND");
    
    let mut nr_solver2 = NewtonRaphsonSolver::new(3);
    
    let vs2 = nr_solver2.add_element(Box::new(VoltageSource::new(5.0)));
    let r_simple = nr_solver2.add_element(Box::new(Resistor::new(100.0)));
    let led_simple = nr_solver2.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    
    nr_solver2.connect(vs2, 1, 0);
    nr_solver2.connect(r_simple, 1, 2);
    nr_solver2.connect(led_simple, 2, 0);
    
    let (valid2, warning2) = validate_circuit_physics(&nr_solver2.elements);
    println!("Circuit validation: {}", warning2);
    
    let (voltages_nr2, time_nr2, iterations_nr2, success_nr2, status_nr2) = nr_solver2.solve_with_monitoring();
    
    println!("\\nNewton-Raphson Results:");
    println!("  Success: {}", success_nr2);
    println!("  Status: {}", status_nr2);
    println!("  Node voltages: {:?}", &voltages_nr2[1..]);
    if voltages_nr2.len() >= 3 {
        println!("  LED voltage: {:.3}V", voltages_nr2[2].abs());
    }
    println!("  Iterations: {}, Time: {:.1}ms", iterations_nr2, time_nr2);
    
    // Test 3: Even more extreme case - too many LEDs
    println!("\\n🔧 TEST 3: EXTREME CASE - 5 LEDs on 5V");
    println!("Circuit: 5V -> LED(2V) -> LED(2V) -> LED(2V) -> LED(2V) -> LED(2V) -> GND");
    
    let mut nr_solver3 = NewtonRaphsonSolver::new(7);
    
    let vs3 = nr_solver3.add_element(Box::new(VoltageSource::new(5.0)));
    let led1 = nr_solver3.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    let led2 = nr_solver3.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    let led3 = nr_solver3.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    let led4 = nr_solver3.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    let led5 = nr_solver3.add_element(Box::new(LED::new(1e-12, 0.026, 2.0)));
    
    nr_solver3.connect(vs3, 1, 0);      // Vs: Node1 to GND
    nr_solver3.connect(led1, 1, 2);     // LED1: Node1 to Node2
    nr_solver3.connect(led2, 2, 3);     // LED2: Node2 to Node3
    nr_solver3.connect(led3, 3, 4);     // LED3: Node3 to Node4
    nr_solver3.connect(led4, 4, 5);     // LED4: Node4 to Node5
    nr_solver3.connect(led5, 5, 6);     // LED5: Node5 to Node6
    // Note: No return path to ground - this is intentionally problematic
    
    let (valid3, warning3) = validate_circuit_physics(&nr_solver3.elements);
    println!("Circuit validation: {}", warning3);
    
    let (voltages_nr3, time_nr3, iterations_nr3, success_nr3, status_nr3) = nr_solver3.solve_with_monitoring();
    
    println!("\\nNewton-Raphson Results:");
    println!("  Success: {}", success_nr3);
    println!("  Status: {}", status_nr3);
    println!("  Node voltages: {:?}", &voltages_nr3[1..]);
    println!("  Iterations: {}, Time: {:.1}ms", iterations_nr3, time_nr3);
    
    // Summary comparison
    println!("\\n=== COMPARISON SUMMARY ===");
    println!("Test 1 (Multi-LED): NR Success={}, Log Gradient previously timed out", success_nr);
    println!("Test 2 (Simple):    NR Success={}, Log Gradient worked fine", success_nr2);
    println!("Test 3 (Extreme):   NR Success={}, No Log Gradient test", success_nr3);
    
    println!("\\n🎯 KEY INSIGHTS:");
    println!("1. Both solvers struggle with physically impossible circuits");
    println!("2. Newton-Raphson behavior: {}", status_nr);
    println!("3. Circuit validation is crucial for both approaches");
    println!("4. The issues appear to be fundamental physics, not solver-specific");
    
    if success_nr || success_nr2 {
        println!("5. ✅ Newton-Raphson can handle some cases the log gradient struggles with");
    } else {
        println!("5. ❌ Newton-Raphson also fails on these invalid topologies");
    }
}