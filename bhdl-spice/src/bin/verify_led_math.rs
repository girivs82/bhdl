//! Verify the LED current calculation math

fn main() {
    println!("=== VERIFY LED MATH ===\n");
    
    let is = 3.96e-19;
    let n = 2.0;
    let vt = 0.026;
    let v = 2.5;
    
    println!("Parameters:");
    println!("  Is = {:e} A", is);
    println!("  n = {}", n);
    println!("  Vt = {} V", vt);
    println!("  V = {} V\n", v);
    
    // Step by step calculation
    let exp_arg: f64 = v / (n * vt);
    println!("Step 1: exp_arg = V/(n*Vt) = {:.3} / ({} * {}) = {}", v, n, vt, exp_arg);
    
    // Check if exp_arg is being limited
    let exp_arg_limited = exp_arg.min(50.0);
    println!("Step 2: exp_arg_limited = min({}, 50.0) = {}", exp_arg, exp_arg_limited);
    
    let exp_term = exp_arg_limited.exp();
    println!("Step 3: exp_term = exp({}) = {:e}", exp_arg_limited, exp_term);
    
    let exp_minus_1 = exp_term - 1.0;
    println!("Step 4: exp_term - 1 = {:e} - 1 = {:e}", exp_term, exp_minus_1);
    
    let current = is * exp_minus_1;
    println!("Step 5: I = Is * (exp_term - 1) = {:e} * {:e} = {:e} A", is, exp_minus_1, current);
    println!("        I = {:.3} mA\n", current * 1000.0);
    
    // But the debug output showed 300A!
    println!("DEBUG OUTPUT SHOWED: 3.0008036895728e2 A = 300 A!");
    println!("That's {:e} times larger than expected!", 300.0 / (current * 1000.0));
    
    // What would give 300A?
    let i_debug = 300.0;
    let is_implied = i_debug / exp_minus_1;
    println!("\nTo get 300A, Is would need to be: {:e} A", is_implied);
    println!("That's {:e} times larger than our Is", is_implied / is);
    
    // Check at lower voltage
    println!("\nAt V = 1.112V:");
    let v2 = 1.112;
    let exp_arg2: f64 = v2 / (n * vt);
    let exp_term2 = exp_arg2.exp();
    let current2 = is * (exp_term2 - 1.0);
    println!("  I = {:e} A = {:.6} mA", current2, current2 * 1000.0);
}