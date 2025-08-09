//! Simple test of LED model behavior

fn main() {
    println!("=== LED Model Test ===\n");
    
    // Test parameters
    let forward_voltage = 2.0;
    let emission_coefficient = 2.0;
    let vt = 0.026;
    let is = 1e-12;
    
    println!("LED parameters:");
    println!("  Vf = {:.1}V", forward_voltage);
    println!("  n = {:.1}", emission_coefficient);
    println!("  Is = {:.2e}A", is);
    println!("  Vt = {:.3}V", vt);
    
    // Test at various voltages
    let test_voltages = vec![0.0, 0.5, 1.0, 1.5, 1.9, 2.0, 2.1, 2.2, 2.5, 3.0];
    
    println!("\nVoltage-Current characteristics:");
    println!("V_LED  | I_LED        | G_LED");
    println!("-------|--------------|-------------");
    
    for v_led in test_voltages {
        let effective_v = v_led - forward_voltage;
        
        let i_led = if effective_v <= 0.0 {
            -is
        } else {
            let v_norm: f64 = effective_v / (emission_coefficient * vt);
            if v_norm > 40.0 {
                is * (40.0_f64.exp() - 1.0)
            } else {
                is * (v_norm.exp() - 1.0)
            }
        };
        
        let g_led = if effective_v <= 0.0 {
            1e-10
        } else {
            let v_norm: f64 = effective_v / (emission_coefficient * vt);
            if v_norm > 40.0 {
                (is / (emission_coefficient * vt)) * 40.0_f64.exp()
            } else {
                ((is / (emission_coefficient * vt)) * v_norm.exp()).max(1e-10)
            }
        };
        
        println!("{:.1}V   | {:>11.2e}A | {:>11.2e}S", v_led, i_led, g_led);
    }
    
    // Calculate what voltage gives 20mA
    println!("\nFinding voltage for 20mA:");
    let target_current = 0.02;
    
    // Newton-Raphson to find voltage
    let mut v_led = forward_voltage + 0.1; // Start guess
    for iter in 0..20 {
        let effective_v = v_led - forward_voltage;
        let v_norm = effective_v / (emission_coefficient * vt);
        
        let i_led = is * (v_norm.exp() - 1.0);
        let di_dv = (is / (emission_coefficient * vt)) * v_norm.exp();
        
        let error = i_led - target_current;
        let delta = error / di_dv;
        
        v_led -= delta;
        
        if error.abs() < 1e-12 {
            println!("  Converged in {} iterations", iter + 1);
            println!("  V_LED = {:.3}V for I_LED = {:.1}mA", v_led, target_current * 1000.0);
            break;
        }
    }
    
    // Test in simple circuit: 5V -> 330Ω -> LED -> GND
    println!("\nSimple circuit test (5V -> 330Ω -> LED -> GND):");
    let vs = 5.0;
    let r = 330.0;
    
    // Newton-Raphson to find LED voltage
    let mut v_led = 2.0; // Start guess
    for iter in 0..50 {
        let effective_v = v_led - forward_voltage;
        
        let i_led = if effective_v <= 0.0 {
            -is
        } else {
            let v_norm: f64 = effective_v / (emission_coefficient * vt);
            if v_norm > 40.0 {
                is * (40.0_f64.exp() - 1.0)
            } else {
                is * (v_norm.exp() - 1.0)
            }
        };
        
        let g_led = if effective_v <= 0.0 {
            1e-10
        } else {
            let v_norm: f64 = effective_v / (emission_coefficient * vt);
            if v_norm > 40.0 {
                (is / (emission_coefficient * vt)) * 40.0_f64.exp()
            } else {
                ((is / (emission_coefficient * vt)) * v_norm.exp()).max(1e-10)
            }
        };
        
        // KCL: (vs - v_led)/r = i_led
        let i_resistor = (vs - v_led) / r;
        let error = i_resistor - i_led;
        
        // Derivative: d(error)/d(v_led) = -1/r - di_led/dv_led = -1/r - g_led
        let deriv = -1.0/r - g_led;
        
        let delta = error / deriv;
        v_led -= delta;
        
        if error.abs() < 1e-12 {
            println!("  Converged in {} iterations", iter + 1);
            println!("  V_LED = {:.3}V", v_led);
            println!("  I_LED = {:.3}mA", i_resistor * 1000.0);
            break;
        }
    }
}