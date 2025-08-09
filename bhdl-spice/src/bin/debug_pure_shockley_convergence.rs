//! Debug pure Shockley equation convergence issues
//! 
//! Focus on understanding why the solver struggles despite good conditioning

use nalgebra::{DMatrix, DVector};
use bhdl_spice::{Circuit, GlacierSolver};

fn main() -> anyhow::Result<()> {
    println!("=== Pure Shockley LED Convergence Debug ===\n");
    
    // Create simple LED circuit: VCC -> R -> LED -> GND
    let mut circuit = Circuit::new();
    
    // Add nodes
    let vcc_node = circuit.add_node("VCC".to_string());
    let led_node = circuit.add_node("LED".to_string()); 
    let gnd_node = circuit.add_node("GND".to_string());
    
    // Add components
    circuit.add_resistor("R1".to_string(), vcc_node, led_node, 470.0)?;
    circuit.add_led("D1".to_string(), led_node, gnd_node, 2.0, 0.02)?; // 2V, 20mA
    circuit.add_voltage_source("V1".to_string(), vcc_node, gnd_node, 5.0)?;
    
    // Set ground reference
    circuit.set_ground_node(gnd_node);
    
    println!("Circuit: 5V -> 470Ω -> LED(2V, 20mA) -> GND");
    println!("Expected: V_LED ≈ 2.0V, I ≈ 6.4mA\n");
    
    // Test single Newton-Raphson iteration
    let mut solver = GlacierSolver::new();
    
    // Test different starting guesses
    let test_points = vec![
        ("Zero start", vec![0.0, 0.0]),
        ("Small start", vec![0.1, 0.1]), 
        ("Mid start", vec![1.0, 1.0]),
        ("Near solution", vec![2.0, 1.8]),
        ("At solution", vec![2.575, 0.65]),
    ];
    
    for (name, initial_guess) in test_points {
        println!("Testing starting point: {}", name);
        println!("Initial guess: {:?}", initial_guess);
        
        // Create initial solution vector (exclude ground node)
        let mut x = DVector::from_vec(initial_guess);
        
        // Test a few Newton-Raphson iterations to see convergence behavior
        for iter in 0..5 {
            // Build system matrices
            let (jacobian, residual) = circuit.build_modified_nodal_analysis(&x)?;
            
            println!("  Iter {}: x = [{:.3}, {:.3}]", iter, x[0], x[1]);
            println!("    Residual norm: {:.2e}", residual.norm());
            
            // Check matrix condition
            if let Some(svd) = jacobian.clone().try_svd(true, true, 1e-30, 100) {
                let cond = svd.singular_values.max() / svd.singular_values.min();
                println!("    Condition number: {:.2e}", cond);
                
                // Show matrix structure
                println!("    Jacobian:");
                for i in 0..jacobian.nrows() {
                    print!("      [");
                    for j in 0..jacobian.ncols() {
                        print!(" {:8.2e}", jacobian[(i, j)]);
                    }
                    println!(" ]");
                }
            }
            
            // Solve for step
            if let Some(step) = jacobian.lu().solve(&residual) {
                println!("    Step: [{:.3e}, {:.3e}]", step[0], step[1]);
                println!("    Step norm: {:.2e}", step.norm());
                
                // Apply step with some damping
                let damping = 0.7;
                x -= damping * step;
                
                // Check convergence
                if residual.norm() < 1e-6 {
                    println!("    ✅ Converged!");
                    break;
                }
            } else {
                println!("    ❌ Singular matrix!");
                break;
            }
        }
        println!();
    }
    
    // Test the LED model specifically at different voltages
    println!("=== LED Model Behavior Analysis ===");
    println!("V [V]    I [A]        dI/dV [S]");
    println!("-" .repeat(35));
    
    let is = 3.96e-19;  // Realistic saturation current
    let n = 2.0;        // Emission coefficient
    let vt = 0.026;     // Thermal voltage
    
    let test_voltages = vec![0.0, 0.5, 1.0, 1.5, 1.8, 1.9, 2.0, 2.1, 2.2, 2.5, 3.0];
    
    for v in test_voltages {
        let v_norm = v / (n * vt);
        
        let (i, di_dv) = if v_norm > 50.0 {
            // Limit exponential
            let i_max = is * (50.0_f64.exp() - 1.0);
            let g_max = (is / (n * vt)) * 50.0_f64.exp();
            (i_max + g_max * (v - 50.0 * n * vt), g_max)
        } else if v_norm < -5.0 {
            (-is, 1e-14)
        } else {
            let exp_term = v_norm.exp();
            let i = is * (exp_term - 1.0);
            let g = ((is / (n * vt)) * exp_term).max(1e-14);
            (i, g)
        };
        
        println!("{:4.1}     {:8.2e}     {:8.2e}", v, i, di_dv);
    }
    
    // Analyze what happens in the problematic voltage range (1.8V to 2.2V)
    println!("\n=== Critical Voltage Range Analysis ===");
    println!("The LED turns on sharply around 2V. Let's see the conductance variation:");
    
    for v in (18..23).map(|x| x as f64 / 10.0) {
        let v_norm = v / (n * vt);
        let exp_term = v_norm.exp();
        let i = is * (exp_term - 1.0);
        let g = ((is / (n * vt)) * exp_term).max(1e-14);
        
        println!("V={:.1}V: I={:.2e}A, g={:.2e}S, g_ratio_to_R={:.1e}", 
                v, i, g, g / (1.0/470.0));
    }
    
    Ok(())
}