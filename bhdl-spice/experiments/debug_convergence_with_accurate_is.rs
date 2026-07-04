//! Debug convergence issues with accurate Is value

fn main() {
    println!("Debugging Convergence with Accurate Is Value");
    println!("============================================\n");
    
    // Compare Is values
    let is_arbitrary = 1e-12;  // What we've been using
    let is_accurate = 1.0703309978026141e-24;  // From 2V @ 20mA datasheet
    
    println!("1. Saturation Current Comparison:");
    println!("---------------------------------");
    println!("Arbitrary Is: {:e}", is_arbitrary);
    println!("Accurate Is:  {:e} (12 orders of magnitude smaller!)", is_accurate);
    
    // LED parameters
    let n: f64 = 1.5;
    let vt: f64 = 0.026;
    
    println!("\n2. Impact on LED Equation:");
    println!("---------------------------");
    println!("I = Is * (e^(V/nVt) - 1)");
    println!("When Is is extremely small:");
    println!("- Need much higher voltage to get measurable current");
    println!("- Exponential term must be enormous");
    println!("- Numerical precision issues likely");
    
    // Calculate voltages for different currents
    println!("\n3. Voltage Required for Various Currents:");
    println!("-----------------------------------------");
    let test_currents = vec![1e-12, 1e-9, 1e-6, 1e-3, 0.01, 0.02];
    
    for &current in &test_currents {
        println!("\nFor I = {:e} A:", current);
        
        // With arbitrary Is
        let v_arbitrary = n * vt * ((current / is_arbitrary) + 1.0_f64).ln();
        println!("  With Is=1e-12:  V = {:.3}V", v_arbitrary);
        
        // With accurate Is
        let ratio = current / is_accurate;
        println!("  I/Is ratio: {:e}", ratio);
        
        if ratio > 1e308 {
            println!("  With Is=1e-24:  V = OVERFLOW (ratio too large)");
        } else {
            let v_accurate = n * vt * ((current / is_accurate) + 1.0_f64).ln();
            println!("  With Is=1e-24:  V = {:.3}V", v_accurate);
        }
    }
    
    println!("\n4. Numerical Issues:");
    println!("--------------------");
    
    // Check exponential overflow
    let v_test: f64 = 2.0;  // 2V across LED
    let exp_term_arbitrary = (v_test / (n * vt)).exp();
    let exp_term_value = v_test / (n * vt);
    
    println!("At V = {}V:", v_test);
    println!("  V/(n*Vt) = {:.1}", exp_term_value);
    println!("  e^(V/nVt) = {:e}", exp_term_arbitrary);
    
    println!("\nWith accurate Is:");
    let i_accurate = is_accurate * (exp_term_arbitrary - 1.0_f64);
    println!("  Current = {:e} * {:e} = {:e} A", is_accurate, exp_term_arbitrary - 1.0, i_accurate);
    
    println!("\n5. Why Convergence Fails:");
    println!("-------------------------");
    println!("a) Extreme scaling differences:");
    println!("   - Is = 1e-24 vs typical currents = 1e-3");
    println!("   - 21 orders of magnitude difference!");
    
    println!("\nb) Jacobian conditioning:");
    println!("   - dI/dV = (Is/nVt) * e^(V/nVt)");
    println!("   - When Is=1e-24, Jacobian elements are tiny");
    println!("   - Matrix becomes numerically singular");
    
    println!("\nc) Newton-Raphson step size:");
    println!("   - ΔV = -f/f' where f' is tiny");
    println!("   - Steps become enormous and unstable");
    
    println!("\n6. Potential Solutions:");
    println!("------------------------");
    
    println!("a) Variable scaling:");
    println!("   - Work with ln(I) instead of I");
    println!("   - Or scale currents by 1e12");
    
    println!("\nb) Modified LED model:");
    println!("   - Use dimensionless variables internally");
    println!("   - I_norm = I/I_ref where I_ref = 1mA");
    
    println!("\nc) Adaptive precision:");
    println!("   - Use higher precision arithmetic (f128?)");
    println!("   - Or symbolic manipulation");
    
    println!("\nd) Physics-based initialization:");
    println!("   - Start solver near expected solution");
    println!("   - V ≈ 2V for 20mA (from datasheet)");
    
    println!("\ne) Reformulated equations:");
    println!("   - Instead of I = Is*(e^x - 1)");
    println!("   - Use V = nVt*ln(I/Is + 1)");
    println!("   - Solve for V given I constraints");
    
    // Test if Is is really correct
    println!("\n7. Verification of Is Calculation:");
    println!("----------------------------------");
    let vf_datasheet: f64 = 2.0;
    let if_datasheet: f64 = 0.020;
    
    // Forward calculation: V -> I
    let i_calc = is_accurate * ((vf_datasheet / (n * vt)).exp() - 1.0_f64);
    println!("Given V={:.1}V, calculated I={:e}A", vf_datasheet, i_calc);
    println!("Expected I={:e}A", if_datasheet);
    println!("Match: {}", (i_calc - if_datasheet).abs() < 1e-6);
    
    // Reverse calculation: I -> V  
    let v_calc = n * vt * ((if_datasheet / is_accurate) + 1.0_f64).ln();
    println!("\nGiven I={:.3}A, calculated V={:.3}V", if_datasheet, v_calc);
    println!("Expected V={:.1}V", vf_datasheet);
    println!("Match: {}", (v_calc - vf_datasheet).abs() < 0.001);
}