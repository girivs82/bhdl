/// Test LED using reference implementation style

use nalgebra::{DMatrix, DVector};

pub struct LED {
    vf: f64,  // Forward voltage
    is: f64,  // Saturation current
    vt: f64,  // Thermal voltage
    voltage: f64,
}

impl LED {
    pub fn new(vf: f64, forward_current: f64) -> Self {
        let vt = 0.026;
        // Calculate saturation current from forward current spec
        // We want forward_current at vf + 0.1V
        let test_v = 0.1_f64;
        let v_norm_test = test_v / vt;
        let is = forward_current / (v_norm_test.exp() - 1.0);
        
        Self { vf, is, vt, voltage: 0.0 }
    }
    
    fn current_at_voltage(&self, v: f64) -> f64 {
        let effective_v = v - self.vf;
        if effective_v <= 0.0 {
            -self.is  // Small reverse current
        } else {
            let v_norm = effective_v / self.vt;
            if v_norm > 50.0 {
                self.is * (50.0_f64.exp() - 1.0)
            } else {
                self.is * (v_norm.exp() - 1.0)
            }
        }
    }
    
    fn conductance_at_voltage(&self, v: f64) -> f64 {
        const MIN_G: f64 = 1e-14;
        let effective_v = v - self.vf;
        
        if effective_v <= 0.0 {
            MIN_G
        } else {
            let v_norm = effective_v / self.vt;
            if v_norm > 50.0 {
                (self.is / self.vt) * 50.0_f64.exp()
            } else {
                ((self.is / self.vt) * v_norm.exp()).max(MIN_G)
            }
        }
    }
}

fn solve_led_circuit() {
    println!("=== LED Circuit Test (Reference Style) ===\n");
    
    // Circuit: 5V -> 330Ω -> LED -> GND
    let vs_value = 5.0;
    let r_value = 330.0;
    let led = LED::new(2.0, 0.02); // 2V forward voltage, 20mA forward current
    
    println!("Circuit:");
    println!("  5V source");
    println!("  330Ω resistor");
    println!("  LED (Vf=2.0V, If=20mA)");
    println!("  Expected current: ~9mA\n");
    
    // Test at different ramp factors
    let ramp_factors = vec![0.0, 0.1, 0.2, 0.4, 0.6, 0.8, 1.0];
    
    for &ramp in &ramp_factors {
        let vs = vs_value * ramp;
        
        // Solve simple 2-node circuit
        // Node 0: ground (0V)
        // Node 1: junction between R and LED
        
        // Initial guess
        let mut v1 = vs * 0.5; // Start at half the source voltage
        
        // Newton-Raphson iteration
        for iter in 0..50 {
            // Calculate LED current and conductance
            let i_led = led.current_at_voltage(v1);
            let g_led = led.conductance_at_voltage(v1);
            
            // Norton equivalent for LED
            let i_norton_led = i_led - g_led * v1;
            
            // Build 1x1 system (only one unknown node)
            // KCL at node 1: (vs - v1)/R + i_led = 0
            // Linearized: (vs - v1)/R + g_led * v1 + i_norton_led = 0
            // Rearranged: (1/R + g_led) * v1 = vs/R - i_norton_led
            
            let g_r = 1.0 / r_value;
            let a = g_r + g_led;
            let b = vs * g_r - i_norton_led;
            
            // Solve for new v1
            let v1_new = b / a;
            
            // Check convergence
            let error = (v1_new - v1).abs();
            if error < 1e-12 {
                // Converged!
                let i_circuit = (vs - v1_new) / r_value;
                println!("Ramp {:.1} ({}V): Converged in {} iterations", 
                         ramp, vs, iter + 1);
                println!("  V_LED = {:.3}V, I = {:.6}A ({:.2}mA)", 
                         v1_new, i_circuit, i_circuit * 1000.0);
                break;
            }
            
            v1 = v1_new;
        }
    }
}

fn main() {
    solve_led_circuit();
}