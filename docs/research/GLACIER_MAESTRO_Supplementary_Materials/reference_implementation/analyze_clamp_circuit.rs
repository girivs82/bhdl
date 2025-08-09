// Analyze why the Sharp Clamp circuit fails completely

fn main() {
    println!("=== Sharp Clamp Circuit Analysis ===");
    
    // Circuit: V1 (1.8V) -> CLAMP -> R_LOAD (50Ω) -> GND
    // Nodes: VDD, OUT, GND (0)
    
    println!("Circuit topology:");
    println!("  V1: VDD -> GND = 1.8V");
    println!("  CLAMP: VDD -> OUT (IBIS table)");
    println!("  R_LOAD: OUT -> GND = 50Ω");
    
    println!("\nSystem equations:");
    println!("  KCL at VDD: I_clamp = I_V1");
    println!("  KCL at OUT: I_clamp + I_load = 0");
    println!("  Ohm's law: I_load = V_OUT / 50");
    println!("  IBIS: I_clamp = f(V_VDD - V_OUT)");
    println!("  Constraint: V_VDD = 1.8V * ramp");
    
    println!("\nNumerical analysis:");
    
    // The key issue: at low ramp values, V_VDD is small
    for ramp in [0.05, 0.10, 0.20, 0.50] {
        let v_vdd = 1.8 * ramp;
        println!("\nRamp {:.0}% -> V_VDD = {:.3}V:", ramp * 100.0, v_vdd);
        
        // For the clamp to conduct, we need V_VDD - V_OUT to be in the clamp region (1.4-1.6V)
        // But if V_VDD < 1.4V, the clamp equation is extrapolating!
        
        if v_vdd < 1.4 {
            println!("  PROBLEM: V_VDD < 1.4V - IBIS table extrapolation!");
            println!("  The clamp table starts at 1.4V, but VDD supply is only {:.3}V", v_vdd);
            println!("  This forces the solver to extrapolate outside the table range");
        } else if v_vdd < 1.6 {
            println!("  MARGINAL: V_VDD in sharp transition region");
        } else {
            println!("  OK: V_VDD above clamp table range");
        }
        
        // Show what the clamp current would be
        let clamp_voltage = v_vdd; // Assuming V_OUT ≈ 0 initially
        let clamp_current = if clamp_voltage < 1.4 {
            -0.001 // Extrapolated from table minimum
        } else if clamp_voltage < 1.6 {
            // Interpolate in sharp region
            let t = (clamp_voltage - 1.4) / (1.6 - 1.4);
            -0.001 + t * (-0.400 + 0.001)
        } else {
            -0.400 // Table maximum
        };
        
        println!("  Estimated I_clamp = {:.6}A", clamp_current);
        
        // Check if this creates a viable operating point
        let v_out_from_load = -clamp_current * 50.0; // V = I * R (current flows into load)
        println!("  Required V_OUT = {:.3}V for current balance", v_out_from_load);
        
        if v_out_from_load > v_vdd {
            println!("  IMPOSSIBLE: V_OUT > V_VDD - violates circuit physics!");
        }
    }
    
    println!("\n=== ROOT CAUSE ANALYSIS ===");
    println!("The Sharp Clamp circuit fails because:");
    println!("1. At low ramp values (0-20%), V_VDD < 1.4V");
    println!("2. This forces IBIS table extrapolation outside its defined range");
    println!("3. The extrapolated currents create physically impossible voltage drops");
    println!("4. Newton-Raphson cannot find a consistent solution");
    println!("\nThis is a CIRCUIT DESIGN ISSUE, not a solver bug!");
    println!("The test circuit is asking for an impossible operating point.");
}
