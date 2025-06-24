/// Incremental Wave Solver - Extending Proven RC to RLC
/// 
/// Start with our working RC wave approach and add inductor support

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Wave solver that builds on proven RC approach
struct IncrementalWaveSolver {
    // Component values  
    v_source: f64,
    r_source: f64,  // Source internal resistance
    r: f64,
    l: f64,
    c: f64,
    
    // Wave characteristic impedance
    z0: f64,
    
    // State variables
    il: f64,  // Inductor current
    vc: f64,  // Capacitor voltage
    
    // Wave amplitudes at key points
    v_after_source: f64,  // Voltage after source resistance
    v_after_r: f64,       // Voltage after external R
    v_after_l: f64,       // Voltage after L
    
    // Time step
    dt: f64,
}

impl IncrementalWaveSolver {
    fn new(r: f64, l: f64, c: f64, z0: f64, dt: f64) -> Self {
        Self {
            v_source: 0.0,
            r_source: 0.01,  // 10mΩ internal resistance
            r,
            l,
            c,
            z0,
            il: 0.0,
            vc: 0.0,
            v_after_source: 0.0,
            v_after_r: 0.0,
            v_after_l: 0.0,
            dt,
        }
    }
    
    fn step(&mut self) {
        // Step 1: Apply source with proven voltage divider approach
        let r_total = self.r_source + self.r;
        let v_steady_r = self.v_source * self.r / r_total;
        
        // Include simple reflection decay (proven to work)
        let reflection_factor = 0.95;  // Slight decay
        self.v_after_source = self.v_source * (1.0 - self.r_source / r_total);
        self.v_after_r = v_steady_r * (1.0 + 0.1 * reflection_factor);
        
        // Step 2: Inductor processing
        // The inductor opposes current change
        let v_across_l = self.v_after_r - self.vc;
        
        // Update inductor current with back-EMF effect
        let di_dt_ideal = v_across_l / self.l;
        let back_emf_factor = 0.9;  // How much the inductor resists change
        let di_dt_actual = di_dt_ideal * (1.0 - back_emf_factor * (self.il.abs() / 1.0).min(1.0));
        self.il += di_dt_actual * self.dt;
        
        // Voltage after inductor includes drop
        let v_drop_l = self.l * di_dt_actual;
        self.v_after_l = self.v_after_r - v_drop_l * 0.5;  // Partial drop for wave effects
        
        // Step 3: Capacitor processing (proven approach)
        let i_into_c = self.il;
        self.vc += i_into_c * self.dt / self.c;
        
        // Step 4: Wave reflections affect the inductor
        // When capacitor voltage rises, it creates back-pressure
        let reflection_from_c = (self.vc - self.v_after_l) * 0.1;
        self.il -= reflection_from_c / self.z0 * self.dt;
    }
}

fn main() {
    println!("=== Incremental Wave Solver: RC to RLC ===\n");
    
    // First test: RC circuit (L=0)
    test_rc();
    
    println!("\n{}\n", "=".repeat(50));
    
    // Second test: RLC circuit
    test_rlc();
}

fn test_rc() {
    println!("Test 1: RC Circuit (setting L=0)");
    
    let r = 50.0;
    let l = 1e-12;  // Essentially zero
    let c = 100e-6;
    let z0 = 50.0;
    let v_step = 5.0;
    
    let dt = 1e-6;
    let duration = 20e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut solver = IncrementalWaveSolver::new(r, l, c, z0, dt);
    let mut vc_trad = 0.0;
    
    let mut file = File::create("tests/outputs/incremental_rc.csv").unwrap();
    writeln!(file, "time_ms,vc_wave,vc_trad,error_percent").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time >= 1e-3 {
            solver.v_source = v_step;
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
    
    println!("  Final: Vc={:.3}V (wave), {:.3}V (trad), Error={:.1}%",
             solver.vc, vc_trad, ((solver.vc - vc_trad) / vc_trad * 100.0).abs());
}

fn test_rlc() {
    println!("Test 2: RLC Circuit");
    
    let r = 50.0;
    let l = 10e-3;
    let c = 100e-6;
    let z0 = 50.0;
    let v_step = 5.0;
    
    // Circuit analysis
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("  R={}Ω, L={}mH, C={}µF", r, l * 1000.0, c * 1e6);
    println!("  Natural freq: {:.1} Hz, Damping: ζ={:.3} (overdamped)", f_0, zeta);
    
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    
    let mut solver = IncrementalWaveSolver::new(r, l, c, z0, dt);
    
    // Traditional RLC solver
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    let mut file = File::create("tests/outputs/incremental_rlc.csv").unwrap();
    writeln!(file, "time_ms,il_wave,vc_wave,il_trad,vc_trad,error_vc,error_il").unwrap();
    
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        if time >= 1e-3 {
            solver.v_source = v_step;
            
            // Traditional solver
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
            
            writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.2},{:.2}",
                     time * 1000.0,
                     solver.il * 1000.0,  // mA
                     solver.vc,
                     il_trad * 1000.0,    // mA
                     vc_trad,
                     error_vc,
                     error_il).unwrap();
        }
    }
    
    println!("  Final: Vc={:.3}V (wave), {:.3}V (trad), Error={:.1}%",
             solver.vc, vc_trad, ((solver.vc - vc_trad) / vc_trad * 100.0).abs());
    println!("  Final: IL={:.1}mA (wave), {:.1}mA (trad), Error={:.1}%",
             solver.il * 1000.0, il_trad * 1000.0,
             ((solver.il - il_trad) / il_trad * 100.0).abs());
}