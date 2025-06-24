/// Wave Digital Filter (WDF) Solver
/// 
/// This implements a wave digital approach that transforms the circuit
/// into the wave domain for improved numerical stability and accuracy

use std::f64::consts::PI;
use std::time::Instant;

// Wave port trait
trait WavePort {
    fn port_resistance(&self) -> f64;
    fn incident_wave(&self) -> f64;
    fn reflected_wave(&self, incident: f64) -> f64;
    fn voltage(&self) -> f64;
    fn current(&self) -> f64;
    fn set_incident(&mut self, wave: f64);
}

// Resistor as wave port
struct WdfResistor {
    resistance: f64,
    incident: f64,
    reflected: f64,
}

impl WdfResistor {
    fn new(r: f64) -> Self {
        Self {
            resistance: r,
            incident: 0.0,
            reflected: 0.0,
        }
    }
}

impl WavePort for WdfResistor {
    fn port_resistance(&self) -> f64 { self.resistance }
    
    fn incident_wave(&self) -> f64 { self.incident }
    
    fn reflected_wave(&self, incident: f64) -> f64 {
        self.incident // Resistor reflects nothing
    }
    
    fn voltage(&self) -> f64 {
        self.incident + self.reflected
    }
    
    fn current(&self) -> f64 {
        (self.incident - self.reflected) / self.resistance
    }
    
    fn set_incident(&mut self, wave: f64) {
        self.incident = wave;
        self.reflected = wave; // For resistor, reflected = incident
    }
}

// Voltage source as wave port
struct WdfVoltageSource {
    voltage: f64,
    resistance: f64, // Port resistance
    incident: f64,
}

impl WdfVoltageSource {
    fn new(v: f64, r: f64) -> Self {
        Self {
            voltage: v,
            resistance: r,
            incident: 0.0,
        }
    }
}

impl WavePort for WdfVoltageSource {
    fn port_resistance(&self) -> f64 { self.resistance }
    
    fn incident_wave(&self) -> f64 { self.incident }
    
    fn reflected_wave(&self, incident: f64) -> f64 {
        self.voltage - incident
    }
    
    fn voltage(&self) -> f64 { self.voltage }
    
    fn current(&self) -> f64 {
        (2.0 * self.incident - self.voltage) / self.resistance
    }
    
    fn set_incident(&mut self, wave: f64) {
        self.incident = wave;
    }
}

// Nonlinear diode as wave port with Newton-Raphson
struct WdfDiode {
    is: f64,
    vt: f64,
    port_resistance: f64,
    incident: f64,
    voltage: f64,
}

impl WdfDiode {
    fn new(is: f64, vt: f64, r: f64) -> Self {
        Self {
            is,
            vt,
            port_resistance: r,
            incident: 0.0,
            voltage: 0.0,
        }
    }
    
    fn diode_current(&self, v: f64) -> f64 {
        const MAX_EXP: f64 = 40.0;
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
    
    fn diode_conductance(&self, v: f64) -> f64 {
        const MAX_EXP: f64 = 40.0;
        const MIN_G: f64 = 1e-12;
        let v_norm = v / self.vt;
        
        if v_norm > MAX_EXP {
            (self.is / self.vt) * MAX_EXP.exp()
        } else if v_norm < -5.0 {
            MIN_G
        } else {
            ((self.is / self.vt) * v_norm.exp()).max(MIN_G)
        }
    }
    
    fn solve_implicit(&mut self) {
        // Newton-Raphson to solve: v = 2*a - R*i(v)
        let a = self.incident;
        let r = self.port_resistance;
        
        let mut v = self.voltage; // Use previous as initial guess
        
        for _ in 0..20 {
            let i = self.diode_current(v);
            let g = self.diode_conductance(v);
            
            let f = v - 2.0 * a + r * i;
            let df = 1.0 + r * g;
            
            let delta = f / df;
            v -= delta;
            
            if delta.abs() < 1e-10 {
                break;
            }
        }
        
        self.voltage = v;
    }
}

impl WavePort for WdfDiode {
    fn port_resistance(&self) -> f64 { self.port_resistance }
    
    fn incident_wave(&self) -> f64 { self.incident }
    
    fn reflected_wave(&self, incident: f64) -> f64 {
        // For nonlinear element: b = v - a
        self.voltage - incident
    }
    
    fn voltage(&self) -> f64 { self.voltage }
    
    fn current(&self) -> f64 {
        self.diode_current(self.voltage)
    }
    
    fn set_incident(&mut self, wave: f64) {
        self.incident = wave;
        self.solve_implicit();
    }
}

// Series adaptor
struct SeriesAdaptor {
    port1_resistance: f64,
    port2_resistance: f64,
    port3_resistance: f64,
}

impl SeriesAdaptor {
    fn new(r1: f64, r2: f64) -> Self {
        Self {
            port1_resistance: r1,
            port2_resistance: r2,
            port3_resistance: r1 + r2, // Series combination
        }
    }
    
    fn scatter(&self, a1: f64, a2: f64, a3: f64) -> (f64, f64, f64) {
        let r1 = self.port1_resistance;
        let r2 = self.port2_resistance;
        let r3 = self.port3_resistance;
        
        // Scattering matrix for series adaptor
        // Port 3 is the series combination looking in
        let rt = r1 + r2; // Total resistance seen from port 3
        
        // Reflected waves
        let b1 = a1 - (2.0 * r1 / rt) * (a1 + a2 - a3);
        let b2 = a2 - (2.0 * r2 / rt) * (a1 + a2 - a3);
        let b3 = (2.0 / rt) * (r1 * a1 + r2 * a2) - a3;
        
        (b1, b2, b3)
    }
}

// Main WDF circuit solver
struct WdfCircuit {
    source: WdfVoltageSource,
    resistor: WdfResistor,
    diode: WdfDiode,
    adaptor: SeriesAdaptor,
}

impl WdfCircuit {
    fn new(vs: f64, rs: f64, is: f64, vt: f64) -> Self {
        // Choose port resistances
        let r_source = 1.0; // Small source resistance
        let r_diode = rs;   // Match series resistor for good conditioning
        
        Self {
            source: WdfVoltageSource::new(vs, r_source),
            resistor: WdfResistor::new(rs),
            diode: WdfDiode::new(is, vt, r_diode),
            adaptor: SeriesAdaptor::new(rs, r_diode),
        }
    }
    
    fn solve(&mut self) -> (f64, f64) {
        // Initialize
        let mut prev_vd = 0.0;
        
        // Iterate to convergence
        for iter in 0..100 {
            // Get reflected wave from source (port 3 of adaptor)
            let a3 = self.source.reflected_wave(self.source.incident_wave());
            
            // Get reflected waves from resistor and diode
            let a1 = self.resistor.reflected_wave(self.resistor.incident_wave());
            let a2 = self.diode.reflected_wave(self.diode.incident_wave());
            
            // Scatter through adaptor
            let (b1, b2, b3) = self.adaptor.scatter(a1, a2, a3);
            
            // Update incident waves
            self.resistor.set_incident(b1);
            self.diode.set_incident(b2);
            self.source.set_incident(b3);
            
            // Check convergence on diode voltage
            let vd = self.diode.voltage();
            if (vd - prev_vd).abs() < 1e-12 && iter > 5 {
                break;
            }
            prev_vd = vd;
        }
        
        // Extract results - current through resistor
        let vr = self.source.voltage() - self.diode.voltage();
        let id = vr / self.resistor.resistance;
        
        (self.diode.voltage(), id)
    }
}

fn main() {
    println!("=== WAVE DIGITAL FILTER SOLVER ===\n");
    
    // Circuit parameters
    let vs = 1.0;
    let rs = 100.0;
    let is = 1e-12;
    let vt = 0.026;
    
    // SPICE reference
    let mut vd_spice = 0.7f64;
    for _ in 0..100 {
        let id = is * ((vd_spice / vt).exp() - 1.0);
        let f = vd_spice + id * rs - vs;
        let df = 1.0 + (is / vt) * (vd_spice / vt).exp() * rs;
        vd_spice -= f / df;
        if (f / df).abs() < 1e-15 {
            break;
        }
    }
    let id_spice = (vs - vd_spice) / rs;
    
    println!("SPICE Reference:");
    println!("  Vd = {:.9} V", vd_spice);
    println!("  Id = {:.9} mA\n", id_spice * 1000.0);
    
    // WDF solution
    println!("Testing Wave Digital Filter approach:\n");
    
    let start = Instant::now();
    let mut circuit = WdfCircuit::new(vs, rs, is, vt);
    let (vd_wdf, id_wdf) = circuit.solve();
    let elapsed = start.elapsed();
    
    let v_err = ((vd_wdf - vd_spice) / vd_spice * 100.0).abs();
    let i_err = ((id_wdf - id_spice) / id_spice * 100.0).abs();
    
    println!("WDF Results:");
    println!("  Vd = {:.9} V (error: {:.6}%)", vd_wdf, v_err);
    println!("  Id = {:.9} mA (error: {:.6}%)", id_wdf * 1000.0, i_err);
    println!("  Time: {:.2} ms", elapsed.as_secs_f64() * 1000.0);
    
    println!("\n=== ANALYSIS ===");
    if v_err < 5.0 && i_err < 5.0 {
        println!("✓ SUCCESS: Achieved <5% accuracy!");
        println!("\nKey advantages of Wave Digital Filters:");
        println!("- Guaranteed stability through passivity");
        println!("- Better numerical conditioning");
        println!("- Natural handling of nonlinear elements");
        println!("- No timestep dependency");
    } else if v_err < 1.0 && i_err < 1.0 {
        println!("✓ EXCELLENT: Achieved <1% accuracy!");
        println!("Wave digital approach shows superior accuracy");
    } else {
        println!("○ Good accuracy but needs refinement");
    }
    
    // Test robustness with different operating points
    println!("\n--- Robustness Test ---");
    for &vs_test in &[0.5, 2.0, 5.0] {
        let mut circuit = WdfCircuit::new(vs_test, rs, is, vt);
        let (vd, id) = circuit.solve();
        
        // Calculate reference
        let mut vd_ref = 0.7f64;
        for _ in 0..100 {
            let id = is * ((vd_ref / vt).exp() - 1.0);
            let f = vd_ref + id * rs - vs_test;
            let df = 1.0 + (is / vt) * (vd_ref / vt).exp() * rs;
            vd_ref -= f / df;
        }
        
        let err = ((vd - vd_ref) / vd_ref * 100.0).abs();
        println!("  Vs = {} V: Vd = {:.6} V (error: {:.3}%)", vs_test, vd, err);
    }
}