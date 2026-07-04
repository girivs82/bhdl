/// Ultimate sub-1% error logarithmic gradient solver
/// Uses aggressive adaptive PID with fine-tuned convergence criteria

use nalgebra::{DMatrix, DVector};
use std::time::Instant;

// Element trait and implementations (compact version)
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

// Ultra-aggressive adaptive PID for sub-1% error
struct UltraAggressivePID {
    base_kp: f64,
    base_ki: f64,
    base_kd: f64,
    kp: f64,
    ki: f64,
    kd: f64,
    integral: f64,
    last_error: f64,
    adaptation_history: Vec<(f64, f64)>, // (log_gradient, error)
}

impl UltraAggressivePID {
    fn new(base_kp: f64, base_ki: f64, base_kd: f64) -> Self {
        Self {
            base_kp,
            base_ki,
            base_kd,
            kp: base_kp,
            ki: base_ki,
            kd: base_kd,
            integral: 0.0,
            last_error: 0.0,
            adaptation_history: Vec::new(),
        }
    }
    
    fn adapt_gains(&mut self, log_gradient: f64, error: f64, ramp_factor: f64) {
        // Store history for learning
        self.adaptation_history.push((log_gradient, error));
        
        // Ultra-aggressive adaptation for sub-1% error
        if log_gradient < 1.0 {
            // Extremely low sensitivity (high Vt) - maximum aggression
            self.kp = self.base_kp * 10.0;
            self.ki = self.base_ki * 20.0;
            self.kd = self.base_kd * 0.1;
        } else if log_gradient < 5.0 {
            // Low sensitivity - very aggressive
            self.kp = self.base_kp * 5.0;
            self.ki = self.base_ki * 10.0;
            self.kd = self.base_kd * 0.2;
        } else if log_gradient < 20.0 {
            // Medium-low sensitivity - aggressive
            self.kp = self.base_kp * 2.0;
            self.ki = self.base_ki * 3.0;
            self.kd = self.base_kd * 0.5;
        } else if log_gradient > 40.0 {
            // High sensitivity - careful but not too conservative
            self.kp = self.base_kp * 0.8;
            self.ki = self.base_ki * 0.6;
            self.kd = self.base_kd * 1.5;
        } else {
            // Normal sensitivity
            self.kp = self.base_kp;
            self.ki = self.base_ki;
            self.kd = self.base_kd;
        }
        
        // Terminal phase super-precision mode
        if ramp_factor > 0.95 && error > 1e-15 {
            // In terminal phase, boost integral for precision
            self.ki *= 2.0;
            self.kd *= 0.5; // Reduce derivative to avoid oscillation
        }
    }
    
    fn update(&mut self, error: f64, dt: f64) -> f64 {
        // P term
        let p = self.kp * error;
        
        // I term with anti-windup
        self.integral += error * dt;
        self.integral = self.integral.max(-10.0).min(10.0); // Prevent windup
        let i = self.ki * self.integral;
        
        // D term with filtering
        let d = self.kd * (error - self.last_error) / dt;
        self.last_error = error;
        
        p + i + d
    }
    
    fn reset(&mut self) {
        self.integral = 0.0;
        self.last_error = 0.0;
    }
}

// Ultimate sub-1% solver
pub struct UltimateSub1PercentSolver {
    elements: Vec<Box<dyn Element>>,
    connections: Vec<(usize, usize, usize)>,
    node_voltages: Vec<f64>,
    source_currents: Vec<f64>,
    num_nodes: usize,
    nonlinear_elements: Vec<usize>,
}

impl UltimateSub1PercentSolver {
    pub fn new(num_nodes: usize) -> Self {
        Self {
            elements: Vec::new(),
            connections: Vec::new(),
            node_voltages: vec![0.0; num_nodes],
            source_currents: Vec::new(),
            num_nodes,
            nonlinear_elements: Vec::new(),
        }
    }
    
    pub fn add_element(&mut self, element: Box<dyn Element>) -> usize {
        let idx = self.elements.len();
        if element.is_nonlinear() {
            self.nonlinear_elements.push(idx);
        }
        self.elements.push(element);
        idx
    }
    
    pub fn connect(&mut self, elem_idx: usize, pos: usize, neg: usize) {
        self.connections.push((elem_idx, pos, neg));
    }
    
    pub fn solve_ultimate(&mut self) -> (Vec<f64>, f64, usize) {
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
        
        let mut vsources = Vec::new();
        for (i, elem) in self.elements.iter().enumerate() {
            if elem.element_type() == ElementType::VoltageSource {
                vsources.push((i, elem.get_voltage()));
            }
        }
        
        // Ultra-aggressive PID controller
        let mut pid = UltraAggressivePID::new(
            2.0,   // Very high base Kp
            0.5,   // High base Ki
            0.01   // Low base Kd
        );
        
        // Ramping with ultra-precise control
        let mut ramp_factor = 0.0;
        let mut ramp_rate = 0.1;  // Start with high rate
        
        // Track gradient for adaptation
        let mut last_current: f64 = 1e-15;
        let mut last_voltage: f64 = 0.0;
        let mut log_gradient: f64 = 20.0;
        
        // Precision tracking
        let mut consecutive_good_errors = 0;
        let mut best_error = f64::MAX;
        
        while ramp_factor < 1.0 {
            // Update sources
            for &(idx, v) in &vsources {
                self.elements[idx].set_voltage(v * ramp_factor);
            }
            
            // Solve at current ramp
            let (converged, iters, error) = self.solve_to_ultra_convergence(&mut total_iterations);
            
            if converged && !self.nonlinear_elements.is_empty() {
                // Get device current
                let elem_idx = self.nonlinear_elements[0];
                let mut element_voltage = 0.0;
                
                for &(conn_elem, pos, neg) in &self.connections {
                    if conn_elem == elem_idx {
                        element_voltage = self.node_voltages[pos] - self.node_voltages[neg];
                        break;
                    }
                }
                
                let current = self.elements[elem_idx].current_at_voltage(element_voltage);
                
                // Calculate log gradient
                if element_voltage > last_voltage + 1e-6 && current > 1e-15 {
                    let dv = element_voltage - last_voltage;
                    let dlog_i = current.ln() - last_current.ln();
                    log_gradient = (dlog_i / dv).abs();
                }
                
                // Adapt PID gains
                pid.adapt_gains(log_gradient, error, ramp_factor);
                
                // Ultra-precise PID control
                let target_error = if ramp_factor > 0.9 { 1e-16 } else { 1e-15 };
                let error_ratio = (error / target_error).ln().max(-15.0).min(15.0);
                let pid_output = pid.update(error_ratio, 0.01);
                
                // Rate control with precision focus
                let rate_multiplier = (-pid_output * 0.1).exp();
                ramp_rate *= rate_multiplier;
                
                // Adaptive bounds based on phase
                let min_rate = if ramp_factor > 0.95 { 1e-6 } else { 1e-5 };
                let max_rate = if ramp_factor < 0.1 { 0.2 } else { 0.05 };
                ramp_rate = ramp_rate.max(min_rate).min(max_rate);
                
                // Track best error
                if error < best_error {
                    best_error = error;
                }
                
                // Count consecutive good errors
                if error < 1e-14 {
                    consecutive_good_errors += 1;
                } else {
                    consecutive_good_errors = 0;
                }
                
                // Early exit with ultra-high precision
                if error < 1e-16 && ramp_factor > 0.99 {
                    println!("  → Ultra-precision achieved! error={:.2e}", error);
                    ramp_factor = 1.0;
                    break;
                }
                
                if consecutive_good_errors > 10 && ramp_factor > 0.95 {
                    println!("  → Sustained precision achieved! error={:.2e}", error);
                    ramp_factor = 1.0;
                    break;
                }
                
                // Update tracking
                last_voltage = element_voltage;
                last_current = current;
                
            } else if !converged {
                // Failed to converge, be more conservative
                ramp_rate *= 0.3;
            }
            
            ramp_factor += ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100% with maximum precision
        for &(idx, v) in &vsources {
            self.elements[idx].set_voltage(v);
        }
        
        // Multiple precision passes at 100%
        let mut final_error = 1.0;
        for pass in 0..10 {
            let (converged, iters, error) = self.solve_to_ultra_convergence(&mut total_iterations);
            final_error = error;
            if error < 1e-16 {
                println!("  → Final ultra-precision achieved in pass {}! error={:.2e}", pass+1, error);
                break;
            }
        }
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        (self.node_voltages.clone(), elapsed, total_iterations)
    }
    
    fn solve_to_ultra_convergence(&mut self, total_iterations: &mut usize) -> (bool, usize, f64) {
        let max_iter = 100;  // More iterations for precision
        let tol = 1e-16;     // Ultra-tight tolerance
        let mut iterations = 0;
        let mut last_error = 0.0;
        
        for iter in 0..max_iter {
            iterations = iter + 1;
            *total_iterations += 1;
            
            let old_v = self.node_voltages.clone();
            let (a, b) = self.build_mna_system();
            
            if let Some(x) = a.lu().solve(&b) {
                let n = self.num_nodes - 1;
                let mut max_change = 0.0f64;
                
                // Adaptive damping for precision
                let damping = if iter < 3 {
                    0.5
                } else if iter < 10 {
                    0.7
                } else if max_change < 1e-10 {
                    0.95  // Very close, use less damping
                } else {
                    0.85
                };
                
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
                
                last_error = max_change;
                
                if max_change < tol {
                    return (true, iterations, last_error);
                }
            } else {
                return (false, iterations, last_error);
            }
        }
        
        (false, iterations, last_error)
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

// Reference solution
fn analytical_reference(vs: f64, rs: f64, is: f64, vt: f64) -> (f64, f64) {
    let mut vd = 0.6;
    let tolerance = 1e-18;
    
    for _iter in 0..1000 {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * rs - vs;
        let df_dvd = 1.0 + (is / vt) * (vd / vt).exp() * rs;
        let delta = f / df_dvd;
        vd -= delta;
        if delta.abs() < tolerance { break; }
    }
    
    let id = (vs - vd) / rs;
    (vd, id)
}

fn main() {
    println!("=== ULTIMATE SUB-1% ERROR LOGARITHMIC GRADIENT SOLVER ===");
    println!("Ultra-aggressive adaptive PID with precision focus\n");
    
    let test_cases = vec![
        ("Baseline", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt", 1.0, 100.0, 1e-12, 0.050),
        ("Low Current", 0.1, 1000.0, 1e-12, 0.026),
        ("High Voltage", 5.0, 100.0, 1e-12, 0.026),
        ("Low Resistance", 1.0, 10.0, 1e-12, 0.026),
        ("Extreme Low", 0.05, 2000.0, 1e-12, 0.026),
        ("High Current", 10.0, 50.0, 1e-12, 0.026),
    ];
    
    let mut total_error = 0.0;
    let mut total_time = 0.0;
    let mut count = 0;
    
    for (name, vs, rs, is, vt) in test_cases {
        print!("Test {}: ", name);
        let mut solver = UltimateSub1PercentSolver::new(3);
        
        let vs_idx = solver.add_element(Box::new(VoltageSource::new(vs)));
        let r_idx = solver.add_element(Box::new(Resistor::new(rs)));
        let d_idx = solver.add_element(Box::new(Diode::new(is, vt)));
        
        solver.connect(vs_idx, 1, 0);
        solver.connect(r_idx, 1, 2);
        solver.connect(d_idx, 2, 0);
        
        let (voltages, time, iterations) = solver.solve_ultimate();
        
        let (vd_ref, id_ref) = analytical_reference(vs, rs, is, vt);
        let vd_computed = voltages[2];
        let id_computed = (voltages[1] - voltages[2]) / rs;
        
        let v_err = ((vd_computed - vd_ref) / vd_ref * 100.0).abs();
        let i_err = ((id_computed - id_ref) / id_ref * 100.0).abs();
        let max_err = v_err.max(i_err);
        
        println!("Error={:.3}%, Time={:.1}ms, Iter={}", max_err, time, iterations);
        
        total_error += max_err;
        total_time += time;
        count += 1;
    }
    
    let avg_error = total_error / count as f64;
    let avg_time = total_time / count as f64;
    
    println!("\n=== ULTIMATE RESULTS ===");
    println!("Average error: {:.3}%", avg_error);
    println!("Average time: {:.1}ms", avg_time);
    println!("\nPaper reference: 3.55% error, 21.5ms");
    
    if avg_error < 1.0 {
        println!("\n🎯 ACHIEVED SUB-1% ERROR!");
        println!("   Accuracy: {:.1}x better than paper", 3.55 / avg_error);
        println!("   Speed: {:.1}x {} than paper", 
                 if avg_time < 21.5 { avg_time / 21.5 } else { 21.5 / avg_time },
                 if avg_time < 21.5 { "faster" } else { "slower" });
    } else if avg_error < 3.55 && avg_time < 21.5 {
        println!("\n✅ SUCCESS: Beats paper on both metrics!");
    } else if avg_error < 3.55 || avg_time < 21.5 {
        println!("\n🔄 PARTIAL SUCCESS: One metric improved");
    } else {
        println!("\n📊 Results show room for optimization");
    }
    
    println!("\n🎯 KEY INNOVATIONS:");
    println!("1. Ultra-aggressive adaptive PID (up to 10x Kp, 20x Ki for low sensitivity)");
    println!("2. Terminal phase precision mode (boosts Ki when ramp > 95%)");
    println!("3. Multiple precision passes at 100% ramp");
    println!("4. Ultra-tight tolerance (1e-16) with adaptive damping");
}