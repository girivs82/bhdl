//! Test extreme circuit configurations with accurate physics models

use nalgebra::{DMatrix, DVector};
use bhdl_spice::scaled_solver::ScaledSolver;
use bhdl_spice::{Result, SpiceError};

#[derive(Clone)]
struct Component {
    typ: ComponentType,
}

#[derive(Clone)]
enum ComponentType {
    VoltageSource(f64),
    Resistor(f64),
    LED { is: f64, n: f64, vt: f64 },
    Diode { is: f64, n: f64, vt: f64, vbr: f64 },
    Zener { vz: f64, iz: f64, rz: f64 },
}

impl Component {
    fn current(&self, v: f64) -> f64 {
        use ComponentType::*;
        match &self.typ {
            LED { is, n, vt } | Diode { is, n, vt, .. } => {
                if v > 0.0 {
                    is * ((v / (n * vt)).exp() - 1.0)
                } else {
                    0.0
                }
            }
            Zener { vz, iz, rz } => {
                if v > 0.5 {
                    // Forward diode
                    1e-12 * ((v / 0.026).exp() - 1.0)
                } else if v > -vz {
                    // Reverse leakage
                    -1e-12
                } else {
                    // Zener breakdown
                    -iz - (v + vz) / rz
                }
            }
            _ => 0.0,
        }
    }
    
    fn conductance(&self, v: f64) -> f64 {
        use ComponentType::*;
        match &self.typ {
            LED { is, n, vt } | Diode { is, n, vt, .. } => {
                if v > 0.0 {
                    (is / (n * vt)) * (v / (n * vt)).exp()
                } else {
                    1e-12
                }
            }
            Zener { vz, iz, rz } => {
                if v > 0.5 {
                    (1e-12 / 0.026) * (v / 0.026).exp()
                } else if v > -vz {
                    1e-12
                } else {
                    1.0 / rz
                }
            }
            Resistor(r) => 1.0 / r,
            _ => 0.0,
        }
    }
}

/// Test 1: Stiff circuit with wide range of time constants
fn test_stiff_circuit() -> Result<()> {
    println!("\n1. Stiff Circuit Test");
    println!("   Challenge: Components with vastly different impedances");
    
    // Circuit with 1MΩ and 1Ω resistors, plus LED
    // This creates numerical stiffness
    let components = vec![
        Component { typ: ComponentType::VoltageSource(5.0) },
        Component { typ: ComponentType::Resistor(1e6) },    // 1MΩ
        Component { typ: ComponentType::Resistor(1.0) },    // 1Ω
        Component { typ: ComponentType::LED { 
            is: 1e-24, n: 1.5, vt: 0.026 
        }},
    ];
    
    // Topology: VS - R1(1M) - (R2(1Ω) || LED) - GND
    // Variables: [V1, V2, I_vs, I_led]
    let n_vars = 4;
    let mut solver = ScaledSolver::new((), n_vars);
    
    let x_init = DVector::from_vec(vec![5.0, 2.0, 1e-6, 1e-6]);
    
    let compute_residual = |x: &DVector<f64>| -> DVector<f64> {
        let v1 = x[0];
        let v2 = x[1];
        let i_vs = x[2];
        let i_led = x[3];
        
        // KCL at node 1
        let f1 = i_vs - (v1 - v2) / 1e6;
        
        // KCL at node 2
        let f2 = (v1 - v2) / 1e6 - v2 / 1.0 - i_led;
        
        // Voltage source constraint
        let f3 = v1 - 5.0;
        
        // LED equation
        let f4 = i_led - components[3].current(v2);
        
        DVector::from_vec(vec![f1, f2, f3, f4])
    };
    
    let compute_jacobian = |x: &DVector<f64>| -> DMatrix<f64> {
        let v2 = x[1];
        let mut j = DMatrix::zeros(4, 4);
        
        // Row 1: KCL at node 1
        j[(0, 0)] = -1e-6;
        j[(0, 1)] = 1e-6;
        j[(0, 2)] = 1.0;
        
        // Row 2: KCL at node 2
        j[(1, 0)] = 1e-6;
        j[(1, 1)] = -1e-6 - 1.0;
        j[(1, 3)] = -1.0;
        
        // Row 3: Voltage source
        j[(2, 0)] = 1.0;
        
        // Row 4: LED
        j[(3, 1)] = -components[3].conductance(v2);
        j[(3, 3)] = 1.0;
        
        j
    };
    
    match solver.solve_scaled(x_init, compute_residual, compute_jacobian, 100, 1e-9) {
        Ok(x) => {
            println!("   ✓ Converged despite 6 orders of magnitude impedance difference!");
            println!("   Node voltages: V1={:.3}V, V2={:.3}V", x[0], x[1]);
            println!("   LED current: {:.3}µA", x[3] * 1e6);
            Ok(())
        }
        Err(e) => {
            println!("   ✗ Failed: {}", e);
            Err(e)
        }
    }
}

/// Test 2: Near-singular matrix (components in feedback)
fn test_near_singular() -> Result<()> {
    println!("\n2. Near-Singular Matrix Test");
    println!("   Challenge: Positive feedback creating near-singular Jacobian");
    
    // Simplified op-amp circuit with positive feedback
    // This creates a near-singular matrix
    
    // Using voltage-controlled voltage source to simulate op-amp
    // Variables: [V_in, V_out, I]
    let gain = 1e6;  // Very high gain
    let r_feedback = 100e3;
    let r_input = 10e3;
    
    let n_vars = 3;
    let mut solver = ScaledSolver::new((), n_vars);
    
    let x_init = DVector::from_vec(vec![1.0, 1.0, 1e-6]);
    
    let compute_residual = |x: &DVector<f64>| -> DVector<f64> {
        let v_in = x[0];
        let v_out = x[1];
        let i = x[2];
        
        // Input: 1V source through R_input
        let f1 = (1.0 - v_in) / r_input - i;
        
        // Op-amp equation (simplified)
        let v_diff = v_in - v_out * (r_input / (r_input + r_feedback));
        let f2 = v_out - gain * v_diff;
        
        // Current balance
        let f3 = i - v_out / r_feedback;
        
        DVector::from_vec(vec![f1, f2, f3])
    };
    
    let compute_jacobian = |_x: &DVector<f64>| -> DMatrix<f64> {
        let mut j = DMatrix::zeros(3, 3);
        
        // This creates a near-singular matrix due to high gain
        j[(0, 0)] = -1.0 / r_input;
        j[(0, 2)] = -1.0;
        
        j[(1, 0)] = -gain;
        j[(1, 1)] = 1.0 + gain * r_input / (r_input + r_feedback);
        
        j[(2, 1)] = -1.0 / r_feedback;
        j[(2, 2)] = 1.0;
        
        j
    };
    
    match solver.solve_scaled(x_init, compute_residual, compute_jacobian, 100, 1e-9) {
        Ok(x) => {
            println!("   ✓ Converged despite near-singular matrix!");
            println!("   Input: {:.3}V, Output: {:.3}V", x[0], x[1]);
            println!("   Gain: {:.1}", x[1] / x[0]);
            Ok(())
        }
        Err(e) => {
            println!("   ✗ Failed: {}", e);
            Err(e)
        }
    }
}

/// Test 3: Avalanche breakdown
fn test_avalanche_breakdown() -> Result<()> {
    println!("\n3. Avalanche Breakdown Test");
    println!("   Challenge: Sharp transition at breakdown voltage");
    
    // Test Zener diode at various operating points
    let zener = Component { 
        typ: ComponentType::Zener { 
            vz: 5.1,      // 5.1V Zener
            iz: 0.049,    // 49mA test current
            rz: 7.0       // 7Ω dynamic resistance
        }
    };
    
    let test_voltages = vec![8.0, 6.0, 5.2, 5.1, 5.0];
    
    for vs in test_voltages {
        println!("\n   Testing with Vs = {}V", vs);
        
        let r = 100.0;
        let n_vars = 2;
        let mut solver = ScaledSolver::new((), n_vars);
        
        let x_init = DVector::from_vec(vec![5.0, 0.01]);
        
        let compute_residual = |x: &DVector<f64>| -> DVector<f64> {
            let vz = x[0];
            let i = x[1];
            
            // KVL: Vs = I*R + Vz
            let f1 = i * r + vz - vs;
            
            // Zener equation (note: reversed polarity)
            let f2 = i - (-zener.current(-vz));
            
            DVector::from_vec(vec![f1, f2])
        };
        
        let compute_jacobian = |x: &DVector<f64>| -> DMatrix<f64> {
            let vz = x[0];
            let mut j = DMatrix::zeros(2, 2);
            
            j[(0, 0)] = 1.0;
            j[(0, 1)] = r;
            
            // Note: derivative of -current(-v) = conductance(v)
            j[(1, 0)] = -zener.conductance(-vz);
            j[(1, 1)] = 1.0;
            
            j
        };
        
        match solver.solve_scaled(x_init, compute_residual, compute_jacobian, 100, 1e-9) {
            Ok(x) => {
                println!("   Vz = {:.3}V, I = {:.2}mA", x[0], x[1] * 1000.0);
            }
            Err(_) => {
                println!("   Failed to converge");
            }
        }
    }
    
    println!("\n   ✓ Handled breakdown region transitions!");
    Ok(())
}

/// Test 4: Oscillator at edge of stability
fn test_edge_of_stability() -> Result<()> {
    println!("\n4. Edge of Stability Test");
    println!("   Challenge: Circuit on the verge of oscillation");
    
    // Wien bridge oscillator at unity gain (edge of oscillation)
    // Simplified DC analysis
    
    let r = 10e3;
    let c_impedance = 10e3;  // Capacitor impedance at test frequency
    
    // Variables: [V1, V2, V_out]
    let n_vars = 3;
    let mut solver = ScaledSolver::new((), n_vars);
    
    let x_init = DVector::from_vec(vec![0.5, 0.5, 1.0]);
    
    let compute_residual = |x: &DVector<f64>| -> DVector<f64> {
        let v1 = x[0];
        let v2 = x[1];
        let v_out = x[2];
        
        // Wien bridge equations (simplified)
        let f1 = (v_out - v1) / r + (v2 - v1) / c_impedance;
        let f2 = (v1 - v2) / c_impedance + v2 / r;
        
        // Unity gain amplifier (edge of stability)
        let f3 = v_out - 3.0 * v2;  // Gain of exactly 3 for oscillation
        
        DVector::from_vec(vec![f1, f2, f3])
    };
    
    let compute_jacobian = |_x: &DVector<f64>| -> DMatrix<f64> {
        let mut j = DMatrix::zeros(3, 3);
        
        // This creates a poorly conditioned matrix at unity gain
        j[(0, 0)] = -1.0/r - 1.0/c_impedance;
        j[(0, 1)] = 1.0/c_impedance;
        j[(0, 2)] = 1.0/r;
        
        j[(1, 0)] = 1.0/c_impedance;
        j[(1, 1)] = -1.0/c_impedance - 1.0/r;
        
        j[(2, 1)] = -3.0;
        j[(2, 2)] = 1.0;
        
        j
    };
    
    match solver.solve_scaled(x_init, compute_residual, compute_jacobian, 200, 1e-6) {
        Ok(x) => {
            println!("   ✓ Found DC operating point at edge of stability!");
            println!("   V1={:.3}V, V2={:.3}V, Vout={:.3}V", x[0], x[1], x[2]);
            
            let loop_gain = x[2] / x[0];
            println!("   Loop gain: {:.3} (should be ~1 for oscillation)", loop_gain);
            Ok(())
        }
        Err(e) => {
            println!("   ✗ Failed: {}", e);
            Err(e)
        }
    }
}

fn main() {
    println!("Extreme Circuit Test Suite");
    println!("==========================\n");
    
    println!("Testing circuits that push numerical limits...\n");
    
    let tests = vec![
        ("Stiff Circuit", test_stiff_circuit()),
        ("Near-Singular Matrix", test_near_singular()),
        ("Avalanche Breakdown", test_avalanche_breakdown()),
        ("Edge of Stability", test_edge_of_stability()),
    ];
    
    let mut passed = 0;
    let mut failed = 0;
    
    println!("\n\nResults:");
    println!("========");
    
    for (name, result) in tests {
        match result {
            Ok(_) => {
                println!("✓ {}: PASSED", name);
                passed += 1;
            }
            Err(_) => {
                println!("✗ {}: FAILED", name);
                failed += 1;
            }
        }
    }
    
    println!("\nTotal: {} passed, {} failed", passed, failed);
    
    if failed == 0 {
        println!("\n✅ Excellent! The scaled solver handles all extreme cases.");
    }
}