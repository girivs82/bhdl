/// Comprehensive Comparison: Logarithmic Gradient vs Robust Newton
/// 
/// Compare the new logarithmic gradient solver with the robust Newton solver
/// across the same test cases that revealed the standard gradient weaknesses

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Common element implementations
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

// Base solver for MNA systems
struct BaseSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
}

impl BaseSolver {
    fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
        }
    }
    
    fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        self.elements.push(element);
        self.elements.len() - 1
    }
    
    fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
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

// Robust Newton Solver implementation
fn robust_newton_solver(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64, usize, f64, usize) {
    let start = Instant::now();
    let mut solver = BaseSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = solver.add_element(Box::new(Resistor::new(rs)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    solver.source_currents = vec![0.0; 1];
    
    let mut total_iterations = 0;
    let ramp_steps = 100;
    let mut actual_ramps = 0;
    
    // Source ramping with adaptive damping
    for ramp in 0..=ramp_steps {
        actual_ramps += 1;
        let factor = ramp as f64 / ramp_steps as f64;
        solver.elements[v].set_voltage(vs * factor);
        
        let mut damping = 1.0;
        let tol = 1e-12;
        
        for _iter in 0..50 {
            total_iterations += 1;
            let old_v = solver.node_voltages.clone();
            let (a, b) = solver.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = solver.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    solver.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..solver.source_currents.len() {
                    solver.source_currents[i] = x[n + i];
                }
                
                // Update element states
                for &(elem_idx, pos, neg) in &solver.connections {
                    let v = solver.node_voltages[pos] - solver.node_voltages[neg];
                    solver.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < tol {
                    break;
                } else if max_change > 1e3 {
                    damping *= 0.5;
                } else {
                    damping = (damping * 1.2).min(1.0);
                }
            }
        }
    }
    
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    (solver.node_voltages[2], solver.source_currents[0].abs(), total_iterations, elapsed, actual_ramps)
}

// Logarithmic Gradient Solver implementation
fn logarithmic_gradient_solver(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64, usize, f64, usize) {
    let start = Instant::now();
    let mut solver = BaseSolver::new(3);
    
    let v = solver.add_element(Box::new(VoltageSource::new(vs)));
    let r = solver.add_element(Box::new(Resistor::new(rs)));
    let d = solver.add_element(Box::new(Diode::new(is, vt)));
    
    solver.connect(v, 1, 0);
    solver.connect(r, 1, 2);
    solver.connect(d, 2, 0);
    
    solver.source_currents = vec![0.0; 1];
    
    let mut total_iterations = 0;
    
    // Adaptive ramping with logarithmic awareness
    let mut ramp_factor = 0.0;
    let mut ramp_rate = 0.01; // Start with 1%
    let min_rate = 0.0001;   // 0.01% minimum
    let max_rate = 0.1;      // 10% maximum
    let mut ramp_steps = 0;
    
    // Expected d(log(I))/dV = 1/Vt for this diode
    let expected_sensitivity = 1.0 / vt;
    
    let mut prev_log_current = -40.0; // Very small starting current
    let mut prev_vd = 0.0;
    
    while ramp_factor < 1.0 {
        total_iterations += 1;
        ramp_steps += 1;
        
        // Update source voltage
        solver.elements[v].set_voltage(vs * ramp_factor);
        
        // Newton-Raphson solve at this ramp point
        let mut converged = false;
        for _iter in 0..50 {
            let old_v = solver.node_voltages.clone();
            let (a, b) = solver.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = solver.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                let damping = 0.8; // Conservative damping
                
                for i in 0..n {
                    let delta = x[i] - old_v[i+1];
                    solver.node_voltages[i+1] = old_v[i+1] + damping * delta;
                    max_change = max_change.max(delta.abs());
                }
                
                for i in 0..solver.source_currents.len() {
                    solver.source_currents[i] = x[n + i];
                }
                
                // Update element states
                for &(elem_idx, pos, neg) in &solver.connections {
                    let v = solver.node_voltages[pos] - solver.node_voltages[neg];
                    solver.elements[elem_idx].set_voltage(v);
                }
                
                if max_change < 1e-12 {
                    converged = true;
                    break;
                }
            }
        }
        
        if !converged {
            ramp_rate *= 0.5;
            continue;
        }
        
        // Calculate logarithmic sensitivity for adaptive control
        if total_iterations > 5 {
            let i = solver.elements[d].current_at_voltage(solver.node_voltages[2]);
            let log_current = (i.abs() + 1e-18).ln();
            
            let dv = solver.node_voltages[2] - prev_vd;
            if dv.abs() > 1e-9 {
                let log_sensitivity = (log_current - prev_log_current) / dv;
                let sensitivity_ratio = log_sensitivity / expected_sensitivity;
                
                // Adaptive control based on logarithmic behavior
                if sensitivity_ratio > 2.0 {
                    // High sensitivity - reduce ramp rate
                    ramp_rate = (ramp_rate * 0.7f64).max(min_rate);
                } else if sensitivity_ratio < 0.5 && log_sensitivity > 0.0 {
                    // Low sensitivity - can increase ramp rate
                    ramp_rate = (ramp_rate * 1.3f64).min(max_rate);
                }
            }
            
            prev_log_current = log_current;
        }
        
        prev_vd = solver.node_voltages[2];
        
        ramp_factor += ramp_rate;
        ramp_factor = ramp_factor.min(1.0);
    }
    
    // Final solve at 100%
    solver.elements[v].set_voltage(vs);
    let (a, b) = solver.build_mna_system();
    if let Some(x) = a.lu().solve(&b) {
        let n = solver.num_nodes - 1;
        for i in 0..n {
            solver.node_voltages[i+1] = x[i];
        }
        for i in 0..solver.source_currents.len() {
            solver.source_currents[i] = x[n + i];
        }
    }
    
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    (solver.node_voltages[2], solver.source_currents[0].abs(), total_iterations, elapsed, ramp_steps)
}

// Calculate SPICE reference solution
fn spice_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    let mut vd = 0.7;
    
    for _ in 0..200 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let g = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        let delta = f / g;
        vd -= delta;
        if delta.abs() < 1e-16 {
            break;
        }
    }
    
    let id = (vs - vd) / rs;
    (vd, id)
}

fn main() {
    println!("=== LOGARITHMIC GRADIENT vs ROBUST NEWTON COMPARISON ===\n");
    
    // Test parameters - same as the comprehensive comparison
    struct TestCase {
        name: &'static str,
        vs: f64,
        rs: f64,
        is: f64,
        vt: f64,
    }
    
    let test_cases = vec![
        TestCase { name: "Baseline (1V, 100Ω)", vs: 1.0, rs: 100.0, is: 1e-12, vt: 0.026 },
        TestCase { name: "High voltage (5V)", vs: 5.0, rs: 100.0, is: 1e-12, vt: 0.026 },
        TestCase { name: "Low resistance (10Ω)", vs: 1.0, rs: 10.0, is: 1e-12, vt: 0.026 },
        TestCase { name: "High resistance (10kΩ)", vs: 1.0, rs: 10000.0, is: 1e-12, vt: 0.026 },
        TestCase { name: "Low Is (1e-15)", vs: 1.0, rs: 100.0, is: 1e-15, vt: 0.026 },
        TestCase { name: "High Is (1e-9)", vs: 1.0, rs: 100.0, is: 1e-9, vt: 0.026 },
        TestCase { name: "High temperature (50mV Vt)", vs: 1.0, rs: 100.0, is: 1e-12, vt: 0.050 },
        TestCase { name: "Low current (100mV, 1kΩ)", vs: 0.1, rs: 1000.0, is: 1e-12, vt: 0.026 },
        TestCase { name: "High current (10V, 10Ω)", vs: 10.0, rs: 10.0, is: 1e-12, vt: 0.026 },
        TestCase { name: "Near threshold", vs: 0.7, rs: 100.0, is: 1e-12, vt: 0.026 },
    ];
    
    // Results storage
    struct Results {
        case_name: String,
        spice_vd: f64,
        spice_id: f64,
        newton_vd: f64,
        newton_id: f64,
        newton_err_v: f64,
        newton_err_i: f64,
        newton_iters: usize,
        newton_time: f64,
        newton_ramps: usize,
        log_grad_vd: f64,
        log_grad_id: f64,
        log_grad_err_v: f64,
        log_grad_err_i: f64,
        log_grad_iters: usize,
        log_grad_time: f64,
        log_grad_ramps: usize,
    }
    
    let mut all_results = Vec::new();
    
    // Run tests
    for test in &test_cases {
        println!("Testing: {}", test.name);
        println!("  Parameters: Vs={:.1}V, Rs={:.0}Ω, Is={:.0e}, Vt={:.3}V", 
                 test.vs, test.rs, test.is, test.vt);
        
        // SPICE reference
        let (vd_ref, id_ref) = spice_reference(test.vs, test.rs, test.is, test.vt);
        println!("  SPICE: Vd={:.6}V, Id={:.6}mA", vd_ref, id_ref * 1000.0);
        
        // Robust Newton
        let (vd_n, id_n, iters_n, time_n, ramps_n) = 
            robust_newton_solver(test.vs, test.rs, test.is, test.vt);
        let err_v_n = ((vd_n - vd_ref) / vd_ref * 100.0).abs();
        let err_i_n = ((id_n - id_ref) / id_ref * 100.0).abs();
        println!("  Newton: Vd={:.6}V (err={:.4}%), {} iters, {:.1}ms", 
                 vd_n, err_v_n, iters_n, time_n);
        
        // Logarithmic Gradient
        let (vd_lg, id_lg, iters_lg, time_lg, ramps_lg) = 
            logarithmic_gradient_solver(test.vs, test.rs, test.is, test.vt);
        let err_v_lg = ((vd_lg - vd_ref) / vd_ref * 100.0).abs();
        let err_i_lg = ((id_lg - id_ref) / id_ref * 100.0).abs();
        println!("  Log Grad: Vd={:.6}V (err={:.4}%), {} iters, {:.1}ms\\n", 
                 vd_lg, err_v_lg, iters_lg, time_lg);
        
        all_results.push(Results {
            case_name: test.name.to_string(),
            spice_vd: vd_ref,
            spice_id: id_ref,
            newton_vd: vd_n,
            newton_id: id_n,
            newton_err_v: err_v_n,
            newton_err_i: err_i_n,
            newton_iters: iters_n,
            newton_time: time_n,
            newton_ramps: ramps_n,
            log_grad_vd: vd_lg,
            log_grad_id: id_lg,
            log_grad_err_v: err_v_lg,
            log_grad_err_i: err_i_lg,
            log_grad_iters: iters_lg,
            log_grad_time: time_lg,
            log_grad_ramps: ramps_lg,
        });
    }
    
    // Summary table
    println!("\\n=== SUMMARY TABLE ===\\n");
    println!("{:<25} {:>12} {:>12} {:>10} {:>10} {:>12} {:>12}",
             "Test Case", "Newton Err%", "LogGrad Err%", "Newton It", "LogGrad It", "Newton ms", "LogGrad ms");
    println!("{}", "-".repeat(110));
    
    for r in &all_results {
        println!("{:<25} {:>12.6} {:>12.6} {:>10} {:>10} {:>12.1} {:>12.1}",
                 r.case_name,
                 r.newton_err_v.max(r.newton_err_i),
                 r.log_grad_err_v.max(r.log_grad_err_i),
                 r.newton_iters,
                 r.log_grad_iters,
                 r.newton_time,
                 r.log_grad_time);
    }
    
    // Performance analysis
    println!("\\n=== PERFORMANCE ANALYSIS ===\\n");
    
    let avg_newton_err: f64 = all_results.iter()
        .map(|r| r.newton_err_v.max(r.newton_err_i))
        .sum::<f64>() / all_results.len() as f64;
    
    let avg_log_grad_err: f64 = all_results.iter()
        .map(|r| r.log_grad_err_v.max(r.log_grad_err_i))
        .sum::<f64>() / all_results.len() as f64;
    
    let avg_newton_iters: f64 = all_results.iter()
        .map(|r| r.newton_iters as f64)
        .sum::<f64>() / all_results.len() as f64;
    
    let avg_log_grad_iters: f64 = all_results.iter()
        .map(|r| r.log_grad_iters as f64)
        .sum::<f64>() / all_results.len() as f64;
    
    let avg_newton_time: f64 = all_results.iter()
        .map(|r| r.newton_time)
        .sum::<f64>() / all_results.len() as f64;
    
    let avg_log_grad_time: f64 = all_results.iter()
        .map(|r| r.log_grad_time)
        .sum::<f64>() / all_results.len() as f64;
    
    println!("Average Accuracy:");
    println!("  Newton:         {:.6}% error", avg_newton_err);
    println!("  Log Gradient:   {:.6}% error", avg_log_grad_err);
    if avg_newton_err > 0.0 {
        println!("  Improvement:    {:.1}x better accuracy", avg_newton_err / avg_log_grad_err.max(1e-10));
    }
    
    println!("\\nAverage Iterations:");
    println!("  Newton:         {:.0} iterations", avg_newton_iters);
    println!("  Log Gradient:   {:.0} iterations", avg_log_grad_iters);
    println!("  Ratio:          {:.2}x", avg_log_grad_iters / avg_newton_iters);
    
    println!("\\nAverage Time:");
    println!("  Newton:         {:.1} ms", avg_newton_time);
    println!("  Log Gradient:   {:.1} ms", avg_log_grad_time);
    println!("  Ratio:          {:.2}x", avg_log_grad_time / avg_newton_time);
    
    // Robustness analysis
    println!("\\n=== ROBUSTNESS ANALYSIS ===\\n");
    
    let newton_max_err = all_results.iter()
        .map(|r| r.newton_err_v.max(r.newton_err_i))
        .fold(0.0f64, |a, b| a.max(b));
    
    let log_grad_max_err = all_results.iter()
        .map(|r| r.log_grad_err_v.max(r.log_grad_err_i))
        .fold(0.0f64, |a, b| a.max(b));
    
    println!("Maximum Error:");
    println!("  Newton:         {:.6}%", newton_max_err);
    println!("  Log Gradient:   {:.6}%", log_grad_max_err);
    
    let newton_converged = all_results.iter()
        .filter(|r| r.newton_err_v < 0.01 && r.newton_err_i < 0.01)
        .count();
    
    let log_grad_converged = all_results.iter()
        .filter(|r| r.log_grad_err_v < 0.01 && r.log_grad_err_i < 0.01)
        .count();
    
    println!("\\nConvergence (<0.01% error):");
    println!("  Newton:         {} / {} cases", newton_converged, all_results.len());
    println!("  Log Gradient:   {} / {} cases", log_grad_converged, all_results.len());
    
    // Extreme parameter analysis
    println!("\\n=== EXTREME PARAMETER PERFORMANCE ===\\n");
    
    for r in &all_results {
        if r.case_name.contains("Low Is") || r.case_name.contains("High temperature") {
            let newton_err = r.newton_err_v.max(r.newton_err_i);
            let log_grad_err = r.log_grad_err_v.max(r.log_grad_err_i);
            
            println!("{}: Newton {:.3}%, Log Gradient {:.3}%", 
                     r.case_name, newton_err, log_grad_err);
            
            if newton_err > 0.01 && log_grad_err < 0.01 {
                println!("  ✓ Log Gradient significantly better!");
            }
        }
    }
    
    // Final recommendation
    println!("\\n=== FINAL ASSESSMENT ===\\n");
    
    if avg_log_grad_err < avg_newton_err * 0.1 {
        println!("🏆 LOGARITHMIC GRADIENT WINS:");
        println!("  - Significantly better accuracy");
        println!("  - Handles extreme parameters robustly");
        println!("  - Theoretical foundation for exponential devices");
    } else if avg_newton_time < avg_log_grad_time * 0.5 && newton_max_err < 1.0 {
        println!("🏆 NEWTON WINS:");
        println!("  - Much faster execution");
        println!("  - Acceptable accuracy for most cases");
    } else {
        println!("🤝 COMPLEMENTARY APPROACHES:");
        println!("  - Newton: Fast, established, good for typical cases");
        println!("  - Log Gradient: High accuracy, excellent for extreme parameters");
        println!("  - Choose based on accuracy requirements and parameter ranges");
    }
}