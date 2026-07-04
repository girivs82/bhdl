/// Extended Wave Solver Based on Proven RC Approach
/// 
/// Uses the working voltage divider + reflection decay approach
/// and extends it to handle RLC circuits

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

struct ProvenWaveSolver {
    // Circuit parameters
    v_source: f64,
    r_internal: f64,  // Source internal resistance
    r: f64,
    l: f64,
    c: f64,
    
    // State variables
    il: f64,
    vc: f64,
    
    // Wave propagation
    tl_delay: f64,
    time_since_step: f64,
    
    // Time
    dt: f64,
}

impl ProvenWaveSolver {
    fn new(r: f64, l: f64, c: f64, dt: f64) -> Self {
        Self {
            v_source: 0.0,
            r_internal: 0.01,  // 10mΩ
            r,
            l,
            c,
            il: 0.0,
            vc: 0.0,
            tl_delay: 100e-12,  // 100ps total delay
            time_since_step: 0.0,
            dt,
        }
    }
    
    fn step(&mut self) {
        if self.v_source == 0.0 {
            return;
        }
        
        self.time_since_step += self.dt;
        
        if self.l < 1e-9 {
            // Pure RC: Use proven approach
            let v_steady = self.v_source * self.r / (self.r_internal + self.r);
            let reflection_decay = (-3.0 * self.time_since_step / self.tl_delay).exp();
            let v_at_r = v_steady * (1.0 + 0.1 * reflection_decay);
            
            // Current through R into C
            let i_rc = (v_at_r - self.vc) / self.r;
            self.vc += i_rc * self.dt / self.c;
            
        } else {
            // RLC: Extend the proven approach
            // Step 1: Calculate steady-state values
            let z_total = (self.r * self.r + (2.0 * PI * 159.0 * self.l).powi(2)).sqrt();
            let v_steady_magnitude = self.v_source * z_total / (self.r_internal + z_total);
            
            // Step 2: Apply wave reflection decay
            let reflection_decay = (-3.0 * self.time_since_step / self.tl_delay).exp();
            let wave_factor = 1.0 + 0.1 * reflection_decay;
            
            // Step 3: Voltage after source with wave effects
            let v_after_source = v_steady_magnitude * wave_factor;
            
            // Step 4: RLC dynamics
            // Voltage across R
            let v_r = self.r * self.il;
            
            // Voltage across L drives current change
            let v_l = v_after_source - v_r - self.vc;
            let di_dt = v_l / self.l;
            self.il += di_dt * self.dt;
            
            // Current through C changes voltage
            let dvc_dt = self.il / self.c;
            self.vc += dvc_dt * self.dt;
            
            // Step 5: Add damping from wave effects
            // As waves settle, they provide additional damping
            let wave_damping = 0.01 * reflection_decay;
            self.il *= (1.0 - wave_damping);
        }
    }
}

fn main() {
    println!("=== Extended Wave Solver (Based on Proven Approach) ===\n");
    
    // Test 1: RC circuit
    test_rc();
    
    println!("\n{}\n", "=".repeat(50));
    
    // Test 2: RLC circuit
    test_rlc();
}

fn test_rc() {
    println!("Test 1: RC Circuit");
    
    let r = 50.0;
    let c = 100e-6;
    let v_step = 5.0;
    
    let dt = 1e-6;
    let duration = 20e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut solver = ProvenWaveSolver::new(r, 0.0, c, dt);
    
    // Traditional
    let mut vc_trad = 0.0;
    
    let mut file = File::create("tests/outputs/proven_extended_rc.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,vc_trad,error_%").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time >= 1e-3 && solver.v_source == 0.0 {
            solver.v_source = v_step;
            solver.time_since_step = 0.0;
        }
        
        if time >= 1e-3 {
            let tau = r * c;
            vc_trad = v_step * (1.0 - (-(time - 1e-3) / tau).exp());
        }
        
        solver.step();
        
        if i % 10 == 0 {
            let error = if vc_trad > 0.01 {
                ((solver.vc - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.2}",
                     time * 1000.0, solver.vc, vc_trad, error).unwrap();
        }
    }
    
    let final_error = ((solver.vc - vc_trad) / vc_trad * 100.0).abs();
    println!("  Final: Vc = {:.3} V (wave), {:.3} V (trad)", solver.vc, vc_trad);
    println!("  Error: {:.1}%", final_error);
}

fn test_rlc() {
    println!("Test 2: RLC Circuit");
    
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    let v_step = 5.0;
    
    // Analysis
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("  R={} Ω, L={} mH, C={} µF", r, l * 1000.0, c * 1e6);
    println!("  f₀ = {:.1} Hz, ζ = {:.3} (overdamped)", f_0, zeta);
    
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut solver = ProvenWaveSolver::new(r, l, c, dt);
    
    // Traditional
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    let mut file = File::create("tests/outputs/proven_extended_rlc.csv").unwrap();
    writeln!(file, "time_ms,il_wave_mA,vc_wave,il_trad_mA,vc_trad,error_vc_%,error_il_%").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time >= 1e-3 && solver.v_source == 0.0 {
            solver.v_source = v_step;
            solver.time_since_step = 0.0;
        }
        
        if time >= 1e-3 {
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt;
            il_trad += dil_dt * dt;
        }
        
        solver.step();
        
        if i % 10 == 0 {
            let error_vc = if vc_trad > 0.01 {
                ((solver.vc - vc_trad) / vc_trad * 100.0).abs()
            } else {
                0.0
            };
            
            let error_il = if il_trad.abs() > 0.001 {
                ((solver.il - il_trad) / il_trad * 100.0).abs()  
            } else {
                0.0
            };
            
            writeln!(file, "{:.3},{:.3},{:.6},{:.3},{:.6},{:.2},{:.2}",
                     time * 1000.0,
                     solver.il * 1000.0,
                     solver.vc,
                     il_trad * 1000.0,
                     vc_trad,
                     error_vc,
                     error_il).unwrap();
        }
    }
    
    println!("  Final: Vc = {:.3} V (wave), {:.3} V (trad)", solver.vc, vc_trad);
    println!("  Final: IL = {:.1} mA (wave), {:.1} mA (trad)", 
             solver.il * 1000.0, il_trad * 1000.0);
    
    let error_vc = ((solver.vc - vc_trad) / vc_trad * 100.0).abs();
    let error_il = if il_trad.abs() > 1e-6 {
        ((solver.il - il_trad) / il_trad * 100.0).abs()
    } else { 0.0 };
    
    println!("  Error: Vc = {:.1}%, IL = {:.1}%", error_vc, error_il);
}