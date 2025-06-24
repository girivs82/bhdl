/// Debug Logarithmic Gradient Solver Outliers
/// 
/// Analyze why the log gradient solver struggles with:
/// 1. High temperature (50mV Vt) - 0.242% error
/// 2. Low current (100mV, 1kΩ) - 9287 iterations

struct DiodeCircuit {
    vs: f64,
    rs: f64,
    is: f64,
    vt: f64,
}

impl DiodeCircuit {
    fn new(vs: f64, rs: f64, is: f64, vt: f64) -> Self {
        Self { vs, rs, is, vt }
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
    
    fn theoretical_log_sensitivity(&self) -> f64 {
        1.0 / self.vt
    }
    
    fn spice_solution(&self) -> (f64, f64) {
        let mut vd = 0.7;
        for _ in 0..100 {
            let id = self.is * ((vd / self.vt).exp() - 1.0);
            let f = vd + id * self.rs - self.vs;
            let g = 1.0 + (self.is / self.vt) * (vd / self.vt).exp() * self.rs;
            let delta = f / g;
            vd -= delta;
            if delta.abs() < 1e-15 {
                break;
            }
        }
        let id = (self.vs - vd) / self.rs;
        (vd, id)
    }
    
    fn analyze_behavior_across_range(&self) {
        println!("=== BEHAVIOR ANALYSIS ===");
        println!("Circuit: Vs={:.3}V, Rs={:.0}Ω, Is={:.0e}, Vt={:.3}V", 
                 self.vs, self.rs, self.is, self.vt);
        
        let (vd_final, id_final) = self.spice_solution();
        println!("Final solution: Vd={:.6}V, Id={:.6}mA", vd_final, id_final * 1000.0);
        println!("Expected d(log(I))/dV = {:.2}", self.theoretical_log_sensitivity());
        
        println!("\n--- Voltage Sweep Analysis ---");
        println!("{:>8} {:>12} {:>12} {:>15} {:>15}", 
                 "Vd (V)", "Id (mA)", "log(Id)", "dlog/dV", "Ratio");
        
        let mut prev_vd = 0.0;
        let mut prev_log_i = 0.0;
        
        for i in 0..=20 {
            let vd = vd_final * (i as f64) / 20.0;
            let id = self.diode_current(vd);
            let log_i = self.log_current(vd);
            
            let mut log_sensitivity = 0.0;
            let mut ratio = 0.0;
            
            if i > 0 {
                let dv = vd - prev_vd;
                if dv > 0.0 {
                    log_sensitivity = (log_i - prev_log_i) / dv;
                    ratio = log_sensitivity / self.theoretical_log_sensitivity();
                }
            }
            
            println!("{:8.4} {:12.6} {:12.2} {:15.2} {:15.2}", 
                     vd, id * 1000.0, log_i, log_sensitivity, ratio);
            
            prev_vd = vd;
            prev_log_i = log_i;
        }
        
        println!("\n--- Critical Regions ---");
        
        // Check near zero
        let near_zero = 0.01;
        let i_near_zero = self.diode_current(near_zero);
        let log_near_zero = self.log_current(near_zero);
        println!("Near zero (Vd={}V): Id={:.3e}A, log(Id)={:.1}", 
                 near_zero, i_near_zero, log_near_zero);
        
        // Check thermal voltage region
        let thermal_region = self.vt;
        let i_thermal = self.diode_current(thermal_region);
        let log_thermal = self.log_current(thermal_region);
        println!("Thermal voltage (Vd={}V): Id={:.3e}A, log(Id)={:.1}", 
                 thermal_region, i_thermal, log_thermal);
        
        // Check forward bias
        let forward_bias = 0.7;
        let i_forward = self.diode_current(forward_bias);
        let log_forward = self.log_current(forward_bias);
        println!("Forward bias (Vd={}V): Id={:.3e}A, log(Id)={:.1}", 
                 forward_bias, i_forward, log_forward);
    }
    
    fn debug_ramp_progression(&self) {
        println!("\n=== RAMP PROGRESSION DEBUG ===");
        
        let mut vd = 0.0;
        let mut ramp_factor = 0.0;
        let mut ramp_rate = 0.01;
        let min_rate = 0.0001;
        let max_rate = 0.1;
        
        let expected_sensitivity = self.theoretical_log_sensitivity();
        let mut prev_log_current = -40.0;
        let mut prev_vd = 0.0;
        let mut step = 0;
        
        println!("{:>5} {:>8} {:>8} {:>12} {:>15} {:>15} {:>10}", 
                 "Step", "Ramp%", "Rate", "Vd (V)", "d(log(I))/dV", "Ratio", "Action");
        
        while ramp_factor < 1.0 && step < 50 {
            // Newton solve at current ramp
            let vs_ramp = self.vs * ramp_factor;
            vd = self.newton_solve_at_voltage(vs_ramp);
            
            let log_current = self.log_current(vd);
            
            let mut action = "continue";
            
            if step > 2 {
                let dv = vd - prev_vd;
                if dv.abs() > 1e-9 {
                    let log_sensitivity = (log_current - prev_log_current) / dv;
                    let sensitivity_ratio = log_sensitivity / expected_sensitivity;
                    
                    println!("{:5} {:8.1} {:8.4} {:12.6} {:15.2} {:15.2} {:>10}", 
                             step, ramp_factor * 100.0, ramp_rate, vd, 
                             log_sensitivity, sensitivity_ratio, action);
                    
                    // Apply logarithmic control logic
                    if sensitivity_ratio > 2.0 {
                        ramp_rate = (ramp_rate * 0.7f64).max(min_rate);
                        action = "slow down";
                    } else if sensitivity_ratio < 0.5 && log_sensitivity > 0.0 {
                        ramp_rate = (ramp_rate * 1.3f64).min(max_rate);
                        action = "speed up";
                    }
                } else {
                    println!("{:5} {:8.1} {:8.4} {:12.6} {:>15} {:>15} {:>10}", 
                             step, ramp_factor * 100.0, ramp_rate, vd, "div/0", "---", "stall");
                }
            } else {
                println!("{:5} {:8.1} {:8.4} {:12.6} {:>15} {:>15} {:>10}", 
                         step, ramp_factor * 100.0, ramp_rate, vd, "---", "---", "warmup");
            }
            
            prev_log_current = log_current;
            prev_vd = vd;
            
            ramp_factor += ramp_rate;
            ramp_factor = ramp_factor.min(1.0);
            step += 1;
        }
        
        println!("\nFinal: {} steps to reach 100%", step);
    }
    
    fn newton_solve_at_voltage(&self, vs: f64) -> f64 {
        let mut vd = 0.0;
        for _iter in 0..50 {
            let id = self.diode_current(vd);
            let gd = self.diode_conductance(vd);
            
            let f = (vs - vd) / self.rs - id;
            let df_dvd = -1.0 / self.rs - gd;
            
            let delta = f / df_dvd;
            vd -= delta;
            
            if delta.abs() < 1e-12 {
                break;
            }
        }
        vd
    }
}

fn main() {
    println!("=== LOGARITHMIC GRADIENT SOLVER OUTLIER ANALYSIS ===\n");
    
    // Problematic cases
    let cases = [
        ("Baseline (good)", 1.0, 100.0, 1e-12, 0.026),
        ("High Vt (bad)", 1.0, 100.0, 1e-12, 0.050),
        ("Low current (bad)", 0.1, 1000.0, 1e-12, 0.026),
    ];
    
    for &(name, vs, rs, is, vt) in &cases {
        println!("\n{}", "=".repeat(60));
        println!("CASE: {}", name);
        
        let circuit = DiodeCircuit::new(vs, rs, is, vt);
        circuit.analyze_behavior_across_range();
        circuit.debug_ramp_progression();
    }
    
    println!("\n{}", "=".repeat(60));
    println!("=== ROOT CAUSE ANALYSIS ===");
    
    println!("\n1. HIGH TEMPERATURE ISSUE (Vt=50mV):");
    println!("   - Larger Vt means lower expected d(log(I))/dV = 1/Vt = 20");
    println!("   - Algorithm expects ~38 but sees ~20 → thinks it's 'low sensitivity'");
    println!("   - Increases ramp rate inappropriately");
    println!("   - FIX: Use actual Vt in sensitivity calculation, not hardcoded 26mV");
    
    println!("\n2. LOW CURRENT ISSUE (100mV, 1kΩ):");
    println!("   - Very small voltage (100mV) puts diode in linear region");
    println!("   - d(log(I))/dV becomes unreliable near zero current");
    println!("   - Algorithm gets confused by noise in log gradients");
    println!("   - FIX: Detect linear region and switch to standard Newton solver");
    
    println!("\n3. LOGARITHMIC GRADIENT ASSUMPTIONS:");
    println!("   - Works best in exponential region (forward bias)");
    println!("   - Breaks down in linear region (reverse/low bias)");
    println!("   - Needs temperature-aware sensitivity calculation");
    
    println!("\n=== PROPOSED FIXES ===");
    println!("1. Temperature-aware sensitivity: expect 1/Vt_actual, not 1/0.026");
    println!("2. Region detection: switch to Newton when Vd < 2*Vt");
    println!("3. Noise filtering: smooth log gradients over multiple points");
    println!("4. Adaptive thresholds: adjust sensitivity ratios based on operating point");
}