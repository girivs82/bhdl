//! Debug LED equation evaluation

use bhdl_spice::ComponentModel;
use bhdl_spice::stdlib_model_loader::StdlibModelLoader;

fn main() {
    println!("=== DEBUG LED EQUATION ===\n");
    
    // Get LED model from stdlib
    let led_model = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    
    if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, forward_voltage, .. } = led_model {
        let is = saturation_current.unwrap();
        let n = emission_coefficient.unwrap();
        let vt = thermal_voltage.unwrap();
        
        println!("LED Parameters (from stdlib):");
        println!("  Is = {:e} A", is);
        println!("  n = {}", n);
        println!("  Vt = {} V", vt);
        println!("  Vf (nominal) = {} V\n", forward_voltage);
        
        // Test Shockley equation at different voltages
        println!("Shockley equation: I = Is * (exp(V/(n*Vt)) - 1)\n");
        println!("Voltage (V) | Current (mA) | exp term");
        println!("-----------+-------------+---------");
        
        for v in [0.0, 0.5, 1.0, 1.5, 1.8, 2.0, 2.2, 2.5] {
            let exp_arg = v / (n * vt);
            let exp_term = exp_arg.exp();
            let current = is * (exp_term - 1.0);
            println!("{:11.1} | {:11.3} | {:e}", v, current * 1000.0, exp_term);
        }
        
        // Find voltage for typical LED current (20mA)
        println!("\nSolving for V when I = 20mA:");
        let i_target = 0.020; // 20mA
        
        // Newton iteration: V = n*Vt * ln(I/Is + 1)
        let v_approx = n * vt * ((i_target / is) + 1.0).ln();
        println!("  Analytical: V ≈ {:.3} V", v_approx);
        
        // Verify
        let i_check = is * ((v_approx / (n * vt)).exp() - 1.0);
        println!("  Verification: I({:.3}V) = {:.3} mA", v_approx, i_check * 1000.0);
        
        // The issue: with Is = 1e-14 A, we get very low turn-on voltage!
        // Let's check what Is we need for Vf = 2V at 20mA
        println!("\nReverse calculation:");
        println!("  For Vf = 2.0V at If = 20mA:");
        let is_needed = i_target / ((2.0 / (n * vt)).exp() - 1.0);
        println!("  Required Is = {:e} A", is_needed);
        println!("  Stdlib has Is = {:e} A", is);
        println!("  Ratio = {:.2e}x too large!", is / is_needed);
    }
}