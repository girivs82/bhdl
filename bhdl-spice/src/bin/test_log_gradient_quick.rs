/// Quick Test of Logarithmic Gradient Solver
/// 
/// Tests only the extreme cases that failed with standard gradient

use std::time::Instant;

struct SimpleLogSolver {
    vs: f64,
    rs: f64,
    is: f64,
    vt: f64,
    v_node2: f64,  // Diode voltage
    i_source: f64,
}

impl SimpleLogSolver {
    fn new(vs: f64, rs: f64, is: f64, vt: f64) -> Self {
        Self { vs, rs, is, vt, v_node2: 0.0, i_source: 0.0 }
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
    
    fn solve_at_ramp(&mut self, ramp_factor: f64) -> bool {
        let v_node1 = self.vs * ramp_factor;
        let mut vd = self.v_node2;
        
        for _iter in 0..50 {
            let id = self.diode_current(vd);
            let gd = self.diode_conductance(vd);
            
            let f = (v_node1 - vd) / self.rs - id;
            let df_dvd = -1.0 / self.rs - gd;
            
            let delta = f / df_dvd;
            vd -= delta;
            
            if delta.abs() < 1e-12 {
                self.v_node2 = vd;
                self.i_source = (v_node1 - vd) / self.rs;
                return true;
            }
        }
        false
    }
    
    fn logarithmic_dc_analysis(&mut self) -> (f64, f64, usize, f64) {
        let start = Instant::now();
        let mut total_iterations = 0;
        
        // Adaptive ramping with logarithmic awareness
        let mut ramp_factor = 0.0;
        let mut ramp_rate = 0.01; // Start with 1%
        let min_rate = 0.0001;   // 0.01% minimum
        let max_rate = 0.1;      // 10% maximum
        
        // Expected d(log(I))/dV = 1/Vt for this diode
        let expected_sensitivity = 1.0 / self.vt;
        
        let mut prev_log_current = -40.0; // Very small starting current
        let mut prev_vd = 0.0;
        
        while ramp_factor < 1.0 {
            total_iterations += 1;
            
            if !self.solve_at_ramp(ramp_factor) {
                ramp_rate *= 0.5;
                continue;
            }
            
            // Calculate logarithmic sensitivity
            let i = self.diode_current(self.v_node2);
            let log_current = (i.abs() + 1e-18).ln();
            
            if total_iterations > 5 {
                let dv = self.v_node2 - prev_vd;
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
            }
            
            prev_log_current = log_current;
            prev_vd = self.v_node2;
            
            ramp_factor += ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
        }
        
        // Final solve at 100%
        self.solve_at_ramp(1.0);
        
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
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
    println!("SPICE: Vd={:.9}V, Id={:.6}mA", vd_ref, id_ref * 1000.0);
    
    // Test logarithmic solver
    let mut solver = SimpleLogSolver::new(vs, rs, is, vt);
    let (vd, id, iterations, time) = solver.logarithmic_dc_analysis();
    
    let v_err = ((vd - vd_ref) / vd_ref * 100.0).abs();
    let i_err = ((id - id_ref) / id_ref * 100.0).abs();
    
    println!("Log Solver: Vd={:.9}V, Id={:.6}mA", vd, id * 1000.0);
    println!("Errors: V={:.2}%, I={:.2}%, Steps={}, Time={:.1}ms", 
             v_err, i_err, iterations, time);
    
    if v_err < 1.0 && i_err < 1.0 {
        println!("✓ EXCELLENT: <1% error!");
    } else if v_err < 5.0 && i_err < 5.0 {
        println!("✓ GOOD: <5% error");
    } else {
        println!("○ Fair: {:.1}% max error", v_err.max(i_err));
    }
}

fn main() {
    println!("=== LOGARITHMIC GRADIENT SOLVER - QUICK TEST ===");
    
    // Test the cases that failed with standard gradient solver
    test_case(1.0, 100.0, 1e-12, 0.026, "Baseline (Standard gradient: OK)");
    test_case(1.0, 100.0, 1e-15, 0.026, "Low Is (Standard gradient: 34.7% error)");
    test_case(1.0, 100.0, 1e-12, 0.050, "High Vt (Standard gradient: 71.6% error)");
    
    println!("\n=== LOGARITHMIC GRADIENT ANALYSIS ===");
    println!("Theory: d(log(I))/dV = 1/Vt makes sensitivity parameter-independent");
    println!("- Baseline Vt=26mV: expect d(log(I))/dV ≈ 38.5");
    println!("- High Vt=50mV: expect d(log(I))/dV ≈ 20.0");
    println!("- Is value doesn't matter for logarithmic sensitivity!");
    println!("\nThis should solve the fundamental issues with extreme parameters.");
}