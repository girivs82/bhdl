// Debug the 3.3V Sharp Clamp circuit

fn main() {
    println!("=== 3.3V Sharp Clamp Analysis ===");
    
    // At 50% ramp: V_VDD = 3.3 * 0.5 = 1.65V
    // At 75% ramp: V_VDD = 3.3 * 0.75 = 2.475V
    
    println!("Ramp analysis with 3.3V supply:");
    for ramp in [0.30, 0.40, 0.50, 0.60, 0.75, 0.90] {
        let v_vdd = 3.3 * ramp;
        println!("\nRamp {:.0}% -> V_VDD = {:.3}V:", ramp * 100.0, v_vdd);
        
        // The CLAMP table only goes up to 1.60V!
        // If V_VDD > 1.60V, we're extrapolating at the high end
        
        if v_vdd > 1.60 {
            println!("  PROBLEM: V_VDD > 1.60V - exceeds IBIS table range!");
            println!("  The clamp table ends at 1.60V, but VDD is {:.3}V", v_vdd);
            println!("  This forces extrapolation beyond the defined clamp region");
            
            // Extrapolated current (constant at table maximum)
            let i_clamp = -0.400; // Maximum in table
            let v_out_required = -i_clamp * 50.0; // V = I * R
            println!("  Extrapolated I_clamp = {:.3}A", i_clamp);
            println!("  Required V_OUT = {:.1}V", v_out_required);
            
            if v_out_required > v_vdd {
                println!("  IMPOSSIBLE: V_OUT ({:.1}V) > V_VDD ({:.3}V)!", v_out_required, v_vdd);
            }
        } else {
            println!("  OK: V_VDD within table range [1.40, 1.60]V");
        }
    }
    
    println!("\n=== CONCLUSION ===");
    println!("The Sharp Clamp test is fundamentally flawed:");
    println!("1. Low ramp (0-40%): V_VDD < 1.4V → extrapolation below table");
    println!("2. High ramp (50-100%): V_VDD > 1.6V → extrapolation above table");
    println!("3. Only 40-50% ramp range is within the IBIS table!");
    println!("4. But even there, the sharp gradient makes convergence difficult");
    println!("\nThis test needs a completely different approach or table design.");
}
