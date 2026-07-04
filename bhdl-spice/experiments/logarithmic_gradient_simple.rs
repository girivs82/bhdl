/// Simplified Logarithmic Gradient Solver
/// 
/// Tests the logarithmic gradient concept with a direct implementation
/// for the diode circuit without complex trait abstractions

use nalgebra::{DMatrix, DVector};
use std::time::Instant;
use std::collections::VecDeque;

// Simple logarithmic history tracking
#[derive(Clone)]
struct LogHistory {
    voltages: VecDeque<f64>,
    log_currents: VecDeque<f64>,
    ramp_factors: VecDeque<f64>,
}

impl LogHistory {
    fn new() -> Self {
        Self {
            voltages: VecDeque::with_capacity(5),
            log_currents: VecDeque::with_capacity(5),
            ramp_factors: VecDeque::with_capacity(5),
        }
    }
    
    fn add_point(&mut self, voltage: f64, log_current: f64, ramp: f64) {
        self.voltages.push_back(voltage);
        self.log_currents.push_back(log_current);
        self.ramp_factors.push_back(ramp);
        
        if self.voltages.len() > 5 {
            self.voltages.pop_front();
            self.log_currents.pop_front();
            self.ramp_factors.pop_front();
        }
    }
    
    fn calculate_log_gradients(&self) -> Option<(f64, f64, f64)> {
        if self.voltages.len() < 3 {
            return None;
        }
        
        let n = self.voltages.len();
        let v2 = self.voltages[n-1];
        let v1 = self.voltages[n-2];
        
        let log_i2 = self.log_currents[n-1];
        let log_i1 = self.log_currents[n-2];
        let log_i0 = self.log_currents[n-3];
        
        let r2 = self.ramp_factors[n-1];
        let r1 = self.ramp_factors[n-2];
        let r0 = self.ramp_factors[n-3];
        
        let dr1 = r1 - r0;
        let dr2 = r2 - r1;
        let dv = v2 - v1;
        
        if dr1 > 0.0 && dr2 > 0.0 && dv.abs() > 1e-9 {
            // Logarithmic gradient: d(log(I))/d(ramp)
            let dlog_i1 = (log_i1 - log_i0) / dr1;
            let dlog_i2 = (log_i2 - log_i1) / dr2;
            
            // Curvature in log space
            let d2log_i = (dlog_i2 - dlog_i1) / ((dr1 + dr2) / 2.0);
            
            // Voltage sensitivity: d(log(I))/dV
            let voltage_sensitivity = (log_i2 - log_i1) / dv;
            
            Some((dlog_i2, d2log_i, voltage_sensitivity))
        } else {
            None
        }
    }
}

// Logarithmic controller
struct LogController {
    current_ramp_rate: f64,
    min_rate: f64,
    max_rate: f64,
    target_log_gradient: f64,
}

impl LogController {
    fn new() -> Self {
        Self {
            current_ramp_rate: 0.01,
            min_rate: 0.0001,
            max_rate: 0.1,
            target_log_gradient: 10.0,
        }
    }
    
    fn update(&mut self, log_gradient: f64, log_curvature: f64, voltage_sensitivity: f64, expected_1_over_vt: f64) {
        println!("    Log grad: {:.2}, Log curv: {:.2e}, d(log(I))/dV: {:.1} (expect ~{:.1})", 
                 log_gradient, log_curvature, voltage_sensitivity, expected_1_over_vt);
        
        // Key insight: For diodes, d(log(I))/dV should be approximately 1/Vt
        // If it's much higher, we're in the exponential explosion region
        let sensitivity_ratio = voltage_sensitivity / expected_1_over_vt;
        
        if sensitivity_ratio > 2.0 {
            // Very high sensitivity - in exponential explosion region
            self.current_ramp_rate = (self.current_ramp_rate * 0.5).max(self.min_rate);
            println!("    HIGH SENSITIVITY ({:.1}x expected) - reducing ramp rate to {:.4}", 
                     sensitivity_ratio, self.current_ramp_rate);
        } else if sensitivity_ratio < 0.5 && log_curvature.abs() < 10.0 {
            // Low sensitivity and low curvature - can go faster
            self.current_ramp_rate = (self.current_ramp_rate * 1.5).min(self.max_rate);
            println!("    Low sensitivity ({:.1}x expected) - increasing ramp rate to {:.4}", 
                     sensitivity_ratio, self.current_ramp_rate);
        }
        
        // Handle excessive curvature in log space
        if log_curvature.abs() > 1000.0 {
            self.current_ramp_rate = (self.current_ramp_rate * 0.7).max(self.min_rate);
            println!("    High log curvature - reducing ramp rate");
        }
    }
}

// Simplified solver for diode circuit
struct SimpleLogSolver {
    vs: f64,
    rs: f64,
    is: f64,
    vt: f64,
    v_node1: f64,  // Source voltage
    v_node2: f64,  // Diode voltage
    i_source: f64,
    history: LogHistory,
    controller: LogController,
}

impl SimpleLogSolver {
    fn new(vs: f64, rs: f64, is: f64, vt: f64) -> Self {
        Self {
            vs, rs, is, vt,
            v_node1: 0.0,
            v_node2: 0.0,
            i_source: 0.0,
            history: LogHistory::new(),
            controller: LogController::new(),
        }
    }
    
    fn diode_current(&self, vd: f64) -> f64 {
        let v_norm = vd / self.vt;
        if v_norm > 50.0 {
            let i_max = self.is * (50.0_f64.exp() - 1.0);
            let g_max = (self.is / self.vt) * 50.0_f64.exp();
            i_max + g_max * (vd - 50.0 * self.vt)
        } else if v_norm < -5.0 {
            -self.is
        } else {
            self.is * (v_norm.exp() - 1.0)
        }
    }
    
    fn diode_conductance(&self, vd: f64) -> f64 {
        let v_norm = vd / self.vt;
        if v_norm > 50.0 {
            (self.is / self.vt) * 50.0_f64.exp()
        } else if v_norm < -5.0 {
            1e-14
        } else {
            ((self.is / self.vt) * v_norm.exp()).max(1e-14)
        }
    }
    
    fn log_current(&self, vd: f64) -> f64 {
        let i = self.diode_current(vd);
        let i_min = 1e-18;
        (i.abs() + i_min).ln()
    }
    
    fn solve_at_ramp(&mut self, ramp_factor: f64) -> bool {
        self.v_node1 = self.vs * ramp_factor;
        
        // Newton-Raphson for the diode voltage
        let mut vd = self.v_node2; // Start from previous solution
        
        for _iter in 0..50 {
            let id = self.diode_current(vd);
            let gd = self.diode_conductance(vd);
            
            // KCL at diode node: (V1 - Vd)/Rs - Id = 0
            let f = (self.v_node1 - vd) / self.rs - id;
            let df_dvd = -1.0 / self.rs - gd;
            
            let delta = f / df_dvd;
            vd -= delta;
            
            if delta.abs() < 1e-12 {
                self.v_node2 = vd;
                self.i_source = (self.v_node1 - vd) / self.rs;
                return true;
            }
        }
        
        false
    }
    
    fn logarithmic_dc_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        println!("\nSimple Logarithmic Gradient DC Analysis");
        println!("Circuit: Vs={}V, Rs={}Ω, Is={:.0e}, Vt={:.3}V", self.vs, self.rs, self.is, self.vt);
        
        let mut total_iterations = 0;
        let mut ramp_factor = 0.0;
        let mut ramp_step = 0;
        
        // Expected theoretical value for voltage sensitivity
        let expected_1_over_vt = 1.0 / self.vt;
        
        while ramp_factor < 1.0 {
            total_iterations += 1;
            
            if !self.solve_at_ramp(ramp_factor) {
                println!("  Warning: Failed to converge at ramp {:.4}", ramp_factor);
                self.controller.current_ramp_rate *= 0.5;
                continue;
            }
            
            // Record logarithmic history
            let log_current = self.log_current(self.v_node2);
            self.history.add_point(self.v_node2, log_current, ramp_factor);
            
            // Calculate logarithmic gradients
            if let Some((log_grad, log_curv, volt_sens)) = self.history.calculate_log_gradients() {
                self.controller.update(log_grad, log_curv, volt_sens, expected_1_over_vt);
            }
            
            // Advance ramp
            ramp_factor += self.controller.current_ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            ramp_step += 1;
            
            if ramp_step % 10 == 0 {
                println!("  Ramp {:.0}%: Vd={:.6}V, Id={:.3}mA, Rate={:.4}", 
                         ramp_factor * 100.0, self.v_node2, self.i_source * 1000.0, 
                         self.controller.current_ramp_rate);
            }
        }
        
        // Final solve at 100%
        self.solve_at_ramp(1.0);
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        println!("  Final: Vd={:.9}V, Id={:.6}mA", self.v_node2, self.i_source * 1000.0);
        println!("  Total steps: {}, Time: {:.1}ms", ramp_step, elapsed);
        
        (self.v_node2, self.i_source, total_iterations, elapsed)
    }
}

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

fn test_case(vs: f64, rs: f64, is: f64, vt: f64, label: &str) {
    println!("\n=== {} ===", label);
    
    // SPICE reference
    let (vd_ref, id_ref) = spice_reference(vs, rs, is, vt);
    println!("SPICE Reference: Vd={:.9}V, Id={:.6}mA", vd_ref, id_ref * 1000.0);
    
    // Test logarithmic solver
    let mut solver = SimpleLogSolver::new(vs, rs, is, vt);
    let (vd, id, iterations, time) = solver.logarithmic_dc_analysis();
    
    let v_err = ((vd - vd_ref) / vd_ref * 100.0).abs();
    let i_err = ((id - id_ref) / id_ref * 100.0).abs();
    
    println!("\nLogarithmic Solver: Vd={:.9}V, Id={:.6}mA", vd, id * 1000.0);
    println!("Errors: Voltage {:.3}%, Current {:.3}%", v_err, i_err);
    println!("Performance: {} steps, {:.1}ms", iterations, time);
    
    if v_err < 1.0 && i_err < 1.0 {
        println!("✓ EXCELLENT: <1% error!");
    } else if v_err < 5.0 && i_err < 5.0 {
        println!("✓ GOOD: <5% error");
    } else {
        println!("○ Needs improvement: {:.1}% max error", v_err.max(i_err));
    }
}

fn main() {
    println!("=== LOGARITHMIC GRADIENT SOLVER TEST ===");
    
    println!("\n=== THEORY ===");
    println!("For exponential devices: I = Is * exp(V/Vt)");
    println!("Linear gradient: dI/dV = (Is/Vt) * exp(V/Vt) → EXPLOSIVE");
    println!("Log gradient: d(log(I))/dV = 1/Vt → CONSTANT!");
    println!("This transforms exponential sensitivity into linear control.");
    
    // Test the problematic cases from the comprehensive comparison
    test_case(1.0, 100.0, 1e-12, 0.026, "Baseline Case");
    test_case(1.0, 100.0, 1e-15, 0.026, "Low Is (34.7% error with standard gradient)");
    test_case(1.0, 100.0, 1e-12, 0.050, "High Vt (71.6% error with standard gradient)");
    
    println!("\n=== ANALYSIS ===");
    println!("The logarithmic gradient approach should:");
    println!("1. Convert exponential behavior to linear gradients");
    println!("2. Make d(log(I))/dV ≈ 1/Vt regardless of Is");
    println!("3. Provide stable control even with extreme parameters");
    println!("4. Eliminate the sharp transition issues of standard gradients");
    
    println!("\nKey advantage: Voltage sensitivity becomes parameter-independent!");
    println!("- Standard: dI/dV ∝ Is → varies by orders of magnitude");
    println!("- Logarithmic: d(log(I))/dV ≈ 1/Vt → predictable and constant");
}