/// Debug Gradient Instability - Systematic Analysis
/// 
/// This analyzes why the second-order gradient calculation becomes unstable

use nalgebra::{DMatrix, DVector};
use std::collections::VecDeque;

#[derive(Clone)]
struct NodeHistory {
    voltages: VecDeque<f64>,
    timestamps: VecDeque<f64>,
}

impl NodeHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(5),
            timestamps: VecDeque::with_capacity(5),
        }
    }
    
    fn add(&mut self, voltage: f64, time: f64) {
        self.voltages.push_back(voltage);
        self.timestamps.push_back(time);
        
        if self.voltages.len() > 5 {
            self.voltages.pop_front();
            self.timestamps.pop_front();
        }
    }
    
    fn calculate_gradients(&self) -> Option<(f64, f64)> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        let v2 = self.voltages[n-1];
        let v1 = self.voltages[n-2];
        let v0 = self.voltages[n-3];
        
        let t2 = self.timestamps[n-1];
        let t1 = self.timestamps[n-2];
        let t0 = self.timestamps[n-3];
        
        let dt1 = t2 - t1;
        let dt0 = t1 - t0;
        
        // First-order gradients
        let dv_dt1 = (v2 - v1) / dt1;
        let dv_dt0 = (v1 - v0) / dt0;
        
        // Second-order gradient
        let dt_avg = (dt1 + dt0) / 2.0;
        let d2v_dt2 = (dv_dt1 - dv_dt0) / dt_avg;
        
        Some((dv_dt1, d2v_dt2))
    }
}

fn main() {
    println!("=== GRADIENT INSTABILITY ANALYSIS ===\n");
    
    // Test 1: Analyze gradient calculation with different scenarios
    test_gradient_calculation();
    
    // Test 2: Analyze what happens during Newton-Raphson convergence
    test_newton_convergence();
    
    // Test 3: Analyze timestep effects
    test_timestep_effects();
    
    // Test 4: Analyze numerical precision issues
    test_numerical_precision();
    
    // Test 5: Analyze MNA behavior
    analyze_mna_behavior();
}

fn test_gradient_calculation() {
    println!("Test 1: Gradient Calculation Analysis");
    println!("=====================================\n");
    
    // Scenario 1: Linear voltage change
    println!("Scenario 1: Linear voltage change (V = 0.1 * t)");
    let mut history = NodeHistory::new();
    let dt = 1e-9;
    for i in 0..5 {
        let t = i as f64 * dt;
        let v = 0.1 * t;
        history.add(v, t);
        
        if let Some((dv_dt, d2v_dt2)) = history.calculate_gradients() {
            println!("  t={:e}, v={:e}, dv/dt={:e}, d²v/dt²={:e}", 
                     t, v, dv_dt, d2v_dt2);
        }
    }
    
    // Scenario 2: Exponential change (like diode)
    println!("\nScenario 2: Exponential change (V = 0.6 * (1 - exp(-t/τ)))");
    let mut history = NodeHistory::new();
    let tau = 1e-9;
    for i in 0..5 {
        let t = i as f64 * dt;
        let v = 0.6 * (1.0 - (-t/tau).exp());
        history.add(v, t);
        
        if let Some((dv_dt, d2v_dt2)) = history.calculate_gradients() {
            println!("  t={:e}, v={:e}, dv/dt={:e}, d²v/dt²={:e}", 
                     t, v, dv_dt, d2v_dt2);
        }
    }
    
    // Scenario 3: Near steady state
    println!("\nScenario 3: Near steady state (small changes)");
    let mut history = NodeHistory::new();
    for i in 0..5 {
        let t = i as f64 * dt;
        let v = 0.576342543 + 1e-10 * (i as f64).sin();
        history.add(v, t);
        
        if let Some((dv_dt, d2v_dt2)) = history.calculate_gradients() {
            println!("  t={:e}, v={:.12}, dv/dt={:e}, d²v/dt²={:e}", 
                     t, v, dv_dt, d2v_dt2);
        }
    }
}

fn test_newton_convergence() {
    println!("\n\nTest 2: Newton-Raphson Convergence Pattern");
    println!("==========================================\n");
    
    // Simulate Newton-Raphson convergence pattern
    let voltages = vec![
        0.0,         // Initial
        0.6,         // First guess
        0.584271,    // After iter 1
        0.577390,    // After iter 2
        0.576362,    // After iter 3
        0.576343,    // After iter 4
        0.576342543, // Converged
    ];
    
    let mut history = NodeHistory::new();
    let dt = 1e-9;
    
    println!("Simulating Newton-Raphson convergence:");
    for (i, &v) in voltages.iter().enumerate() {
        let t = i as f64 * dt;
        history.add(v, t);
        
        if let Some((dv_dt, d2v_dt2)) = history.calculate_gradients() {
            println!("  Step {}: v={:.9}, dv/dt={:e}, d²v/dt²={:e}", 
                     i, v, dv_dt, d2v_dt2);
            
            // Check for extreme values
            if d2v_dt2.abs() > 1e20 {
                println!("    WARNING: Extremely large curvature!");
                
                // Analyze why
                if history.voltages.len() >= 3 {
                    let n = history.voltages.len();
                    let v2 = history.voltages[n-1];
                    let v1 = history.voltages[n-2];
                    let v0 = history.voltages[n-3];
                    
                    println!("    v0={:.12}, v1={:.12}, v2={:.12}", v0, v1, v2);
                    println!("    Δv1={:e}, Δv2={:e}", v1-v0, v2-v1);
                    println!("    dt={:e}", dt);
                }
            }
        }
    }
}

fn test_timestep_effects() {
    println!("\n\nTest 3: Timestep Effects on Gradient");
    println!("====================================\n");
    
    let timesteps = vec![1e-6, 1e-9, 1e-12, 1e-15, 1e-18];
    
    for dt in timesteps {
        println!("Timestep = {:e}:", dt);
        
        let mut history = NodeHistory::new();
        
        // Add three points with small voltage change
        let v_base = 0.576342543;
        let dv = 1e-9; // 1 nanovolt change
        
        history.add(v_base, 0.0);
        history.add(v_base + dv, dt);
        history.add(v_base + 2.0*dv, 2.0*dt);
        
        if let Some((dv_dt, d2v_dt2)) = history.calculate_gradients() {
            println!("  dv/dt = {:e}", dv_dt);
            println!("  d²v/dt² = {:e}", d2v_dt2);
            
            // Analyze numerical issues
            let dv_dt_expected = dv / dt;
            let relative_error = ((dv_dt - dv_dt_expected) / dv_dt_expected).abs();
            println!("  Relative error in dv/dt: {:e}", relative_error);
            
            if dt < 1e-15 {
                println!("  WARNING: Timestep approaching machine precision limits!");
            }
        }
        println!();
    }
}

fn test_numerical_precision() {
    println!("\n\nTest 4: Numerical Precision Analysis");
    println!("===================================\n");
    
    // Test floating point precision effects
    let v = 0.576342543;
    let dt = 1e-18;
    
    println!("Testing precision at v={}, dt={:e}", v, dt);
    
    // Test addition precision
    let v_plus_eps = v + 1e-15;
    let diff = v_plus_eps - v;
    println!("  v + 1e-15 - v = {:e} (expected: 1e-15)", diff);
    
    // Test division by small dt
    let gradient = diff / dt;
    println!("  (1e-15) / {:e} = {:e}", dt, gradient);
    
    // Test catastrophic cancellation
    let v1 = 0.576342543;
    let v2 = 0.576342544;
    let small_diff = v2 - v1;
    println!("\n  Catastrophic cancellation test:");
    println!("  v1 = {:.12}", v1);
    println!("  v2 = {:.12}", v2);
    println!("  v2 - v1 = {:e} (actual)", small_diff);
    println!("  v2 - v1 = {:e} (expected)", 1e-9);
    
    // Machine epsilon test
    println!("\n  Machine epsilon = {:e}", f64::EPSILON);
    println!("  Smallest representable change at v={}: {:e}", v, v * f64::EPSILON);
    
    // Recommendations
    println!("\n  Recommendations:");
    println!("  - Minimum safe timestep: {:e}", 1e-12);
    println!("  - Minimum meaningful voltage change: {:e}", v * f64::EPSILON * 100.0);
}

fn analyze_mna_behavior() {
    println!("\n\nBonus: MNA Matrix Behavior During Convergence");
    println!("============================================\n");
    
    // Simple diode circuit MNA
    let vs = 1.0f64;
    let rs = 100.0f64;
    let is = 1e-12f64;
    let vt = 0.026f64;
    
    let test_voltages = vec![0.0, 0.3, 0.576, 0.576342543];
    
    for vd in test_voltages {
        println!("At Vd = {} V:", vd);
        
        // Diode parameters
        let id = is * ((vd / vt).exp() - 1.0);
        let gd = (is / vt) * (vd / vt).exp();
        
        println!("  Id = {:e} A", id);
        println!("  Gd = {:e} S", gd);
        println!("  Rd = {:e} Ω", 1.0/gd);
        
        // Condition number estimate
        let cond_estimate = (1.0/rs + gd) / ((1.0/rs).min(gd));
        println!("  Condition number estimate: {:e}", cond_estimate);
        
        if gd > 1e6 {
            println!("  WARNING: Very high conductance - may cause numerical issues");
        }
        println!();
    }
}