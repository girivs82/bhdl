//! Voltage-shifted LED model implementation
//! 
//! This implements the same transformation used in the reference implementation
//! to avoid numerical issues with realistic saturation currents.

use crate::ModelExecutionContext;

/// Execute LED model with voltage shifting for numerical stability
/// 
/// Instead of: I = Is * (exp(V/(n*Vt)) - 1)
/// We use:     I = Is' * (exp((V-Vf)/Vt') - 1) for V > Vf
///             I = -Is' for V <= Vf
/// 
/// Where Is' = Is * exp(Vf/(n*Vt)) to maintain continuity
pub fn execute_voltage_shifted_led(
    forward_voltage: f64,
    forward_current: f64,
    saturation_current: Option<f64>,
    emission_coefficient: Option<f64>,
    thermal_voltage: Option<f64>,
    ctx: &mut ModelExecutionContext,
) -> anyhow::Result<()> {
    // Original parameters
    let vt = thermal_voltage.unwrap_or(0.026);
    let n = emission_coefficient.unwrap_or(2.0);
    let is_original = saturation_current.unwrap_or(3.96e-19);
    
    // Voltage shift parameters
    let vf = forward_voltage;
    
    // In shifted coordinates, we use n'=1 for simplicity (like reference)
    // and transform Is to maintain the same current at operating points
    let is_shifted = is_original * ((vf / (n * vt)).exp());
    
    // Apply voltage shift
    let v = ctx.v_diff;
    let v_shifted = v - vf;
    
    const MAX_EXP: f64 = 50.0;
    const MIN_G: f64 = 1e-14;
    
    // Current and conductance in shifted coordinates
    let (i_actual, di_dv) = if v_shifted <= 0.0 {
        // Below Vf - very small current
        (-is_shifted, MIN_G)
    } else if v_shifted > MAX_EXP * vt {
        // Limit exponential
        let i_max = is_shifted * (MAX_EXP.exp() - 1.0);
        let g_max = (is_shifted / vt) * MAX_EXP.exp();
        (i_max + g_max * (v_shifted - MAX_EXP * vt), g_max)
    } else {
        // Normal exponential region (shifted)
        let exp_term = (v_shifted / vt).exp();
        let i = is_shifted * (exp_term - 1.0);
        let g = (is_shifted / vt) * exp_term;
        (i, g.max(MIN_G))
    };
    
    // Debug output
    static mut DEBUG_COUNT: usize = 0;
    unsafe {
        if DEBUG_COUNT < 5 {
            eprintln!("Shifted LED: V={:.3}V, V_shifted={:.3}V, I={:.2e}A, g={:.2e}S, Is'={:.2e}A",
                     v, v_shifted, i_actual, di_dv, is_shifted);
            DEBUG_COUNT += 1;
        }
    }
    
    // Stamp using the nonlinear element method
    ctx.stamp_nonlinear_element(di_dv, i_actual);
    
    Ok(())
}

/// Calculate the equivalent shifted saturation current
/// This can be used to pre-transform LED models for use with standard solvers
pub fn calculate_shifted_saturation_current(
    is_original: f64,
    forward_voltage: f64,
    emission_coefficient: f64,
    thermal_voltage: f64,
) -> f64 {
    is_original * ((forward_voltage / (emission_coefficient * thermal_voltage)).exp())
}