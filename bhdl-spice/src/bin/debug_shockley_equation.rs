//! Debug the Shockley equation implementation

use bhdl_spice::stdlib_model_loader::StdlibModelLoader;
use bhdl_spice::ComponentModel;

fn main() {
    println!("=== DEBUG SHOCKLEY EQUATION ===\n");
    
    // Get LED model
    let led = StdlibModelLoader::create_led_model("D1", "red").unwrap();
    
    if let ComponentModel::LED { saturation_current, emission_coefficient, thermal_voltage, .. } = &led {
        let is = saturation_current.unwrap();
        let n = emission_coefficient.unwrap();
        let vt = thermal_voltage.unwrap();
        
        println!("LED Parameters:");
        println!("  Is = {:e} A", is);
        println!("  n = {}", n);
        println!("  Vt = {} V\n", vt);
        
        // Test the actual equation at V=1.112V
        let v_test = 1.112;
        let exp_arg = v_test / (n * vt);
        let exp_term = exp_arg.exp();
        let i_calc = is * (exp_term - 1.0);
        
        println!("At V = {} V:", v_test);
        println!("  exp_arg = V/(n*Vt) = {}", exp_arg);
        println!("  exp_term = exp(exp_arg) = {:e}", exp_term);
        println!("  exp_term - 1 = {:e}", exp_term - 1.0);
        println!("  I = Is * (exp_term - 1) = {:e} A", i_calc);
        println!("  I = {:.6} mA\n", i_calc * 1000.0);
        
        // The issue might be numerical precision
        println!("Numerical precision check:");
        println!("  Is = {:e}", is);
        println!("  exp_term - 1 = {:e}", exp_term - 1.0);
        println!("  Product = {:e}", is * (exp_term - 1.0));
        
        // Check if the issue is with very small Is
        println!("\nTesting with larger Is:");
        let is_test = 1e-12;
        let i_test = is_test * (exp_term - 1.0);
        println!("  With Is = 1e-12: I = {:e} A = {:.3} mA", i_test, i_test * 1000.0);
        
        // What voltage gives 17.675mA with our Is?
        println!("\nReverse calculation:");
        let i_target = 0.017675; // 17.675mA
        let v_needed = n * vt * ((i_target / is) + 1.0).ln();
        println!("  For I = 17.675 mA:");
        println!("  V = n*Vt*ln(I/Is + 1) = {:.3} V", v_needed);
        
        // Maybe the solver is using a different Is?
        println!("\nWhat Is would give 17.675mA at 1.112V?");
        let is_solver = i_target / (exp_term - 1.0);
        println!("  Is_solver = I / (exp_term - 1) = {:e} A", is_solver);
        println!("  Ratio to correct Is = {:e}", is_solver / is);
    }
}