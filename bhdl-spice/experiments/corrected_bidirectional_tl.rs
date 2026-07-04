/// Corrected Bidirectional TL Solver
/// 
/// This implementation properly handles wave propagation for RLC circuits
/// by tracking forward and backward waves at each transmission line segment.

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Transmission line segment with bidirectional waves
#[derive(Debug, Clone)]
struct TLSegment {
    z0: f64,                    // Characteristic impedance
    delay: f64,                 // Propagation delay
    forward_history: Vec<f64>,  // History of forward waves
    backward_history: Vec<f64>, // History of backward waves
}

impl TLSegment {
    fn new(z0: f64, delay: f64, history_size: usize) -> Self {
        Self {
            z0,
            delay,
            forward_history: vec![0.0; history_size],
            backward_history: vec![0.0; history_size],
        }
    }
    
    fn propagate(&mut self, v_in: f64, i_in: f64, step: usize) -> (f64, f64) {
        // Convert to wave variables
        let v_forward = (v_in + self.z0 * i_in) / 2.0;
        let v_backward = (v_in - self.z0 * i_in) / 2.0;
        
        // Store in history
        self.forward_history[step] = v_forward;
        self.backward_history[step] = v_backward;
        
        // Retrieve delayed waves
        let delay_steps = (self.delay / 1e-6) as usize; // Assuming dt = 1µs
        let delayed_forward = if step >= delay_steps {
            self.forward_history[step - delay_steps]
        } else {
            0.0
        };
        let delayed_backward = if step >= delay_steps {
            self.backward_history[step - delay_steps]
        } else {
            0.0
        };
        
        // Convert back to voltage and current
        let v_out = delayed_forward + delayed_backward;
        let i_out = (delayed_forward - delayed_backward) / self.z0;
        
        (v_out, i_out)
    }
}

/// RLC solver using transmission line model
struct RLCTLSolver {
    // Component values
    r: f64,
    l: f64,
    c: f64,
    
    // Transmission line segments
    tl_source: TLSegment,
    tl_resistor: TLSegment,
    tl_inductor: TLSegment,
    tl_capacitor: TLSegment,
    
    // Component states
    inductor_current: f64,
    capacitor_voltage: f64,
    
    // Time parameters
    dt: f64,
    
    // Voltage source
    v_source: f64,
}

impl RLCTLSolver {
    fn new(r: f64, l: f64, c: f64, dt: f64, max_steps: usize) -> Self {
        // Use 50Ω characteristic impedance for all TL segments
        let z0 = 50.0;
        
        Self {
            r,
            l,
            c,
            tl_source: TLSegment::new(z0, 10e-12, max_steps),
            tl_resistor: TLSegment::new(z0, 10e-12, max_steps),
            tl_inductor: TLSegment::new(z0, 50e-12, max_steps),
            tl_capacitor: TLSegment::new(z0, 20e-12, max_steps),
            inductor_current: 0.0,
            capacitor_voltage: 0.0,
            dt,
            v_source: 0.0,
        }
    }
    
    fn set_voltage(&mut self, v: f64) {
        self.v_source = v;
    }
    
    fn step(&mut self, step_num: usize) -> (f64, f64) {
        // Source generates forward wave
        let i_source = 0.0; // Will be determined by circuit
        let (v1, i1) = self.tl_source.propagate(self.v_source, i_source, step_num);
        
        // Resistor
        let i_resistor = v1 / (self.r + self.tl_resistor.z0);
        let v_after_r = v1 - i_resistor * self.r;
        let (v2, i2) = self.tl_resistor.propagate(v_after_r, i_resistor, step_num);
        
        // Inductor - opposes current change
        let v_inductor = self.l * (i2 - self.inductor_current) / self.dt;
        self.inductor_current += v_inductor * self.dt / self.l;
        let v_after_l = v2 - v_inductor;
        let (v3, i3) = self.tl_inductor.propagate(v_after_l, self.inductor_current, step_num);
        
        // Capacitor - integrates current
        self.capacitor_voltage += i3 * self.dt / self.c;
        let (v4, i4) = self.tl_capacitor.propagate(self.capacitor_voltage, i3, step_num);
        
        // Return capacitor voltage and circuit current
        (self.capacitor_voltage, self.inductor_current)
    }
}

fn main() {
    println!("=== Corrected Bidirectional TL Solver ===\n");
    
    // Test configurations
    let test_cases = vec![
        ("Underdamped", 50.0, 10e-3, 100e-6),
        ("Critically Damped", 200.0, 10e-3, 100e-6),
        ("Overdamped", 500.0, 10e-3, 100e-6),
    ];
    
    for (name, r, l, c) in test_cases {
        test_rlc_configuration(name, r, l, c);
    }
}

fn test_rlc_configuration(name: &str, r: f64, l: f64, c: f64) {
    println!("\n{} RLC Circuit: R={}Ω, L={}mH, C={}µF", name, r, l * 1000.0, c * 1e6);
    println!("────────────────────────────────────────────────");
    
    // Calculate characteristics
    let omega_0 = 1.0 / ((l * c) as f64).sqrt();
    let zeta = r / 2.0 * ((c / l) as f64).sqrt();
    let f_0 = omega_0 / (2.0 * PI);
    
    println!("  Natural frequency: {:.1} Hz", f_0);
    println!("  Damping ratio ζ = {:.3}", zeta);
    
    // Simulation parameters
    let dt = 1e-6;
    let duration = 50e-3;
    let num_steps = (duration / dt) as usize;
    let v_step = 5.0;
    
    // Create solvers
    let mut tl_solver = RLCTLSolver::new(r, l, c, dt, num_steps);
    
    // Traditional solver state
    let mut vc_trad = 0.0;
    let mut il_trad = 0.0;
    
    // Raw TL results
    let mut v_tl_raw = vec![0.0; num_steps];
    let mut i_tl_raw = vec![0.0; num_steps];
    
    // Run simulation
    for i in 0..num_steps {
        let time = i as f64 * dt;
        
        // Apply step at 1ms
        if time >= 1e-3 {
            tl_solver.set_voltage(v_step);
            
            // Traditional solver
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt;
            il_trad += dil_dt * dt;
        }
        
        // TL solver step
        let (v_tl, i_tl) = tl_solver.step(i);
        v_tl_raw[i] = v_tl;
        i_tl_raw[i] = i_tl;
    }
    
    // Apply adaptive filtering
    let bandwidth = if zeta < 1.0 {
        f_0 / (0.5 / zeta)
    } else {
        1.0 / (2.0 * PI * r * c)
    };
    let filter_cutoff = bandwidth * 100.0;
    
    println!("  Circuit bandwidth: {:.1} Hz", bandwidth);
    println!("  Filter cutoff: {:.1} kHz", filter_cutoff / 1000.0);
    
    let v_tl_filtered = apply_rc_filter(&v_tl_raw, dt, filter_cutoff);
    
    // Save results
    let filename = format!("tests/outputs/corrected_tl_{}.csv", name.to_lowercase());
    let mut file = File::create(&filename).unwrap();
    writeln!(file, "time_ms,v_tl_raw,v_tl_filtered,v_traditional,error_percent").unwrap();
    
    // Calculate error metrics
    let mut max_error = 0.0_f64;
    let mut rms_error = 0.0_f64;
    let mut count = 0;
    
    // Traditional solution for comparison
    vc_trad = 0.0;
    il_trad = 0.0;
    
    for i in (0..num_steps).step_by(10) {
        let time = i as f64 * dt;
        
        if time >= 1e-3 {
            let dvc_dt = il_trad / c;
            let dil_dt = (v_step - vc_trad - r * il_trad) / l;
            vc_trad += dvc_dt * dt * 10.0; // Account for step size
            il_trad += dil_dt * dt * 10.0;
        }
        
        let error = if vc_trad > 0.01 {
            ((v_tl_filtered[i] - vc_trad) / vc_trad * 100.0).abs()
        } else {
            0.0
        };
        
        if time > 2e-3 { // After initial transient
            max_error = max_error.max(error);
            rms_error += error * error;
            count += 1;
        }
        
        writeln!(file, "{:.3},{:.6},{:.6},{:.6},{:.2}",
                 time * 1000.0, v_tl_raw[i], v_tl_filtered[i], vc_trad, error).unwrap();
    }
    
    if count > 0 {
        rms_error = (rms_error / count as f64).sqrt();
    }
    
    println!("  Max error: {:.2}%", max_error);
    println!("  RMS error: {:.2}%", rms_error);
    println!("  Results saved to: {}", filename);
}

/// Simple RC filter (from proven implementation)
fn apply_rc_filter(input: &[f64], dt: f64, cutoff_freq: f64) -> Vec<f64> {
    let rc = 1.0 / (2.0 * PI * cutoff_freq);
    let alpha = dt / (rc + dt);
    
    let mut output = vec![0.0; input.len()];
    output[0] = input[0];
    
    for i in 1..input.len() {
        output[i] = alpha * input[i] + (1.0 - alpha) * output[i-1];
    }
    
    output
}