//! Simple test to demonstrate log transformation concept

use std::f64::consts::E;

fn main() {
    println!("Log Transformation Analysis for LED Convergence");
    println!("==============================================\n");
    
    // LED parameters
    let is: f64 = 1e-12;  // Saturation current
    let n: f64 = 1.5;     // Emission coefficient
    let vt: f64 = 0.026;  // Thermal voltage (26mV)
    
    // Two operating points
    let i_low: f64 = 0.0004;   // 0.4 mA (low-energy solution)
    let i_high: f64 = 0.0017;  // 1.7 mA (high-energy solution)
    
    // Calculate corresponding voltages using Shockley equation
    // I = Is * (e^(V/nVt) - 1)
    // For I >> Is: V ≈ n*Vt * ln(I/Is)
    let v_low = n * vt * (i_low / is).ln();
    let v_high = n * vt * (i_high / is).ln();
    
    println!("1. LED Operating Points:");
    println!("------------------------");
    println!("Low-energy state:");
    println!("  Current: {:.3} mA", i_low * 1000.0);
    println!("  Voltage: {:.3} V", v_low);
    println!("  Power:   {:.3} mW", v_low * i_low * 1000.0);
    
    println!("\nHigh-energy state:");
    println!("  Current: {:.3} mA", i_high * 1000.0);
    println!("  Voltage: {:.3} V", v_high);
    println!("  Power:   {:.3} mW", v_high * i_high * 1000.0);
    
    println!("\n2. Linear Space Analysis:");
    println!("-------------------------");
    println!("Current ratio: {:.2}x", i_high / i_low);
    println!("Voltage difference: {:.3} V", v_high - v_low);
    println!("Power ratio: {:.2}x", (v_high * i_high) / (v_low * i_low));
    
    println!("\n3. Log Space Analysis:");
    println!("----------------------");
    println!("ln(I_low):  {:.3}", i_low.ln());
    println!("ln(I_high): {:.3}", i_high.ln());
    println!("Difference: {:.3}", i_high.ln() - i_low.ln());
    println!("In log space, the difference is only {:.1}x", 
             (i_high.ln() - i_low.ln()).abs());
    
    println!("\n4. Jacobian Comparison:");
    println!("-----------------------");
    
    // Jacobian element: dI/dV
    // Linear: dI/dV = (Is/nVt) * e^(V/nVt)
    // Log: d(ln I)/dV = 1/(nVt) (constant!)
    
    let jacobian_low_linear = (is / (n * vt)) * E.powf(v_low / (n * vt));
    let jacobian_high_linear = (is / (n * vt)) * E.powf(v_high / (n * vt));
    let jacobian_log = 1.0 / (n * vt);
    
    println!("Linear space Jacobian:");
    println!("  At low current:  {:.2e}", jacobian_low_linear);
    println!("  At high current: {:.2e}", jacobian_high_linear);
    println!("  Ratio: {:.2}x", jacobian_high_linear / jacobian_low_linear);
    
    println!("\nLog space Jacobian:");
    println!("  Constant: {:.2} (independent of operating point!)", jacobian_log);
    
    println!("\n5. Convergence Implications:");
    println!("----------------------------");
    println!("Linear space:");
    println!("  - Extreme gradient variations ({:.0}x)", 
             jacobian_high_linear / jacobian_low_linear);
    println!("  - Newton steps unstable at high currents");
    println!("  - Naturally favors low-current solution");
    
    println!("\nLog space:");
    println!("  - Constant gradients everywhere");
    println!("  - Stable Newton steps");
    println!("  - No inherent bias toward either solution");
    
    println!("\n6. Energy Function Transformation:");
    println!("----------------------------------");
    
    // For a series circuit with 2 LEDs and resistor
    let r = 330.0;
    let vcc = 5.0;
    
    // Energy function: minimize |VCC - I*R - 2*V_LED(I)|²
    let energy_low = (vcc - i_low * r - 2.0 * v_low).powi(2);
    let energy_high = (vcc - i_high * r - 2.0 * v_high).powi(2);
    
    println!("Energy at low current:  {:.6}", energy_low);
    println!("Energy at high current: {:.6}", energy_high);
    
    if energy_low < energy_high {
        println!("\nLinear space: Low-current solution has lower energy");
        println!("This is why Newton-Raphson finds it!");
    } else {
        println!("\nLinear space: High-current solution has lower energy");
    }
    
    println!("\nIn log space, both solutions become equally accessible");
    println!("because the extreme curvature is removed.");
    
    println!("\n7. Practical Implementation:");
    println!("----------------------------");
    println!("Instead of solving: f(I, V) = 0");
    println!("We solve: f(exp(log_I), V) = 0");
    println!("With variable substitution: log_I = ln(I)");
    println!("\nThis transforms the LED equation from:");
    println!("  I - Is*(e^(V/nVt) - 1) = 0");
    println!("To:");
    println!("  log_I - ln(Is*(e^(V/nVt) - 1)) = 0");
    println!("\nThe solver works with log_I, then transforms back:");
    println!("  I = exp(log_I)");
}