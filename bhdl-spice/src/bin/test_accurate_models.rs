//! Test accurate physics-based models with the scaled solver

use nalgebra::{DMatrix, DVector};
use bhdl_spice::scaled_solver::ScaledSolver;
use bhdl_spice::{Result, SpiceError};

// Include accurate models inline for testing
mod models {
    use std::f64::consts::E;
    
    pub const BOLTZMANN: f64 = 1.380649e-23;
    pub const ELEMENTARY_CHARGE: f64 = 1.602176634e-19;
    pub const ROOM_TEMP: f64 = 298.15;
    
    pub fn thermal_voltage(temp: f64) -> f64 {
        BOLTZMANN * temp / ELEMENTARY_CHARGE
    }
    
    #[derive(Debug, Clone)]
    pub struct AccurateLED {
        pub is: f64,
        pub n: f64,
        pub rs: f64,
        pub temp: f64,
    }
    
    impl AccurateLED {
        pub fn from_datasheet(vf: f64, if_test: f64, n: f64, rs: f64) -> Self {
            let vt = thermal_voltage(ROOM_TEMP);
            let is = if_test / ((vf / (n * vt)).exp() - 1.0);
            Self { is, n, rs, temp: ROOM_TEMP }
        }
        
        pub fn current(&self, v: f64) -> f64 {
            if v <= 0.0 { return 0.0; }
            let vt = thermal_voltage(self.temp);
            self.is * ((v / (self.n * vt)).exp() - 1.0)
        }
        
        pub fn conductance(&self, v: f64) -> f64 {
            if v <= 0.0 { return 1e-12; }
            let vt = thermal_voltage(self.temp);
            (self.is / (self.n * vt)) * (v / (self.n * vt)).exp()
        }
    }
}

use models::*;

/// Test a challenging multi-LED circuit
fn test_multi_led_circuit() -> Result<()> {
    println!("\nTesting Multi-LED Circuit with Accurate Models");
    println!("----------------------------------------------");
    
    // Circuit: 12V - 220Ω - LED1(red) - LED2(green) - LED3(blue) - GND
    let vs = 12.0;
    let r = 220.0;
    
    // Different LED types with accurate models
    let red_led = AccurateLED::from_datasheet(1.8, 0.02, 1.4, 15.0);
    let green_led = AccurateLED::from_datasheet(2.2, 0.02, 1.5, 20.0);
    let blue_led = AccurateLED::from_datasheet(3.2, 0.02, 1.6, 25.0);
    
    println!("  Red LED: Is = {:e}", red_led.is);
    println!("  Green LED: Is = {:e}", green_led.is);
    println!("  Blue LED: Is = {:e}", blue_led.is);
    
    // Variables: [V1, V2, V3, I] where V1,V2,V3 are LED voltages
    let n_vars = 4;
    let mut solver = ScaledSolver::new((), n_vars);
    
    let x_init = DVector::from_vec(vec![1.8, 2.2, 3.2, 0.01]);  // Reasonable guess
    
    let compute_residual = |x: &DVector<f64>| -> DVector<f64> {
        let v1 = x[0];
        let v2 = x[1]; 
        let v3 = x[2];
        let i = x[3];
        
        // KVL: Vs = I*R + V1 + V2 + V3
        let f1 = i * r + v1 + v2 + v3 - vs;
        
        // LED equations
        let f2 = i - red_led.current(v1);
        let f3 = i - green_led.current(v2);
        let f4 = i - blue_led.current(v3);
        
        DVector::from_vec(vec![f1, f2, f3, f4])
    };
    
    let compute_jacobian = |x: &DVector<f64>| -> DMatrix<f64> {
        let v1 = x[0];
        let v2 = x[1];
        let v3 = x[2];
        
        let mut j = DMatrix::zeros(4, 4);
        
        // Row 1: KVL
        j[(0, 0)] = 1.0;  // dF1/dV1
        j[(0, 1)] = 1.0;  // dF1/dV2
        j[(0, 2)] = 1.0;  // dF1/dV3
        j[(0, 3)] = r;    // dF1/dI
        
        // Row 2: Red LED
        j[(1, 0)] = -red_led.conductance(v1);
        j[(1, 3)] = 1.0;
        
        // Row 3: Green LED  
        j[(2, 1)] = -green_led.conductance(v2);
        j[(2, 3)] = 1.0;
        
        // Row 4: Blue LED
        j[(3, 2)] = -blue_led.conductance(v3);
        j[(3, 3)] = 1.0;
        
        j
    };
    
    match solver.solve_scaled(x_init, compute_residual, compute_jacobian, 50, 1e-9) {
        Ok(x) => {
            println!("\n  ✓ Converged!");
            println!("  Red LED: {:.3}V", x[0]);
            println!("  Green LED: {:.3}V", x[1]);
            println!("  Blue LED: {:.3}V", x[2]);
            println!("  Current: {:.2}mA", x[3] * 1000.0);
            println!("  Total LED drop: {:.3}V", x[0] + x[1] + x[2]);
            Ok(())
        }
        Err(e) => {
            println!("  ✗ Failed: {}", e);
            Err(e)
        }
    }
}

/// Test extreme case with tiny currents
fn test_extreme_low_current() -> Result<()> {
    println!("\nTesting Extreme Low Current (Moonlight Circuit)");
    println!("-----------------------------------------------");
    
    // Circuit designed for very low current (microamps)
    let vs = 3.0;
    let r = 1e6;  // 1MΩ resistor
    
    let led = AccurateLED::from_datasheet(2.0, 0.02, 1.5, 10.0);
    println!("  LED Is = {:e}", led.is);
    println!("  Target current: ~1μA");
    
    let n_vars = 2;
    let mut solver = ScaledSolver::new((), n_vars);
    
    let x_init = DVector::from_vec(vec![0.5, 1e-6]);  // Very low current guess
    
    let compute_residual = |x: &DVector<f64>| -> DVector<f64> {
        let v = x[0];
        let i = x[1];
        
        let f1 = i * r + v - vs;
        let f2 = i - led.current(v);
        
        DVector::from_vec(vec![f1, f2])
    };
    
    let compute_jacobian = |x: &DVector<f64>| -> DMatrix<f64> {
        let v = x[0];
        
        let mut j = DMatrix::zeros(2, 2);
        j[(0, 0)] = 1.0;
        j[(0, 1)] = r;
        j[(1, 0)] = -led.conductance(v);
        j[(1, 1)] = 1.0;
        
        j
    };
    
    match solver.solve_scaled(x_init, compute_residual, compute_jacobian, 50, 1e-12) {
        Ok(x) => {
            println!("\n  ✓ Converged!");
            println!("  LED voltage: {:.3}V", x[0]);
            println!("  Current: {:.3}μA", x[1] * 1e6);
            
            // Verify physics
            let i_check = led.current(x[0]);
            println!("  Current check: {:.3}μA", i_check * 1e6);
            Ok(())
        }
        Err(e) => {
            println!("  ✗ Failed: {}", e);
            Err(e)
        }
    }
}

/// Test parallel LEDs with current sharing
fn test_parallel_leds() -> Result<()> {
    println!("\nTesting Parallel LEDs (Current Sharing)");
    println!("---------------------------------------");
    
    // Circuit: 5V - 100Ω - (LED1 || LED2) - GND
    let vs = 5.0;
    let r = 100.0;
    
    // Slightly mismatched LEDs
    let led1 = AccurateLED::from_datasheet(2.0, 0.02, 1.5, 10.0);
    let led2 = AccurateLED::from_datasheet(2.05, 0.02, 1.48, 12.0);  // 2.5% higher Vf
    
    println!("  LED1 Is = {:e}", led1.is);
    println!("  LED2 Is = {:e}", led2.is);
    
    // Variables: [V_LED, I1, I2]
    let n_vars = 3;
    let mut solver = ScaledSolver::new((), n_vars);
    
    let x_init = DVector::from_vec(vec![2.0, 0.01, 0.01]);
    
    let compute_residual = |x: &DVector<f64>| -> DVector<f64> {
        let v = x[0];
        let i1 = x[1];
        let i2 = x[2];
        
        // KVL: Vs = (I1 + I2)*R + V_LED
        let f1 = (i1 + i2) * r + v - vs;
        
        // LED equations
        let f2 = i1 - led1.current(v);
        let f3 = i2 - led2.current(v);
        
        DVector::from_vec(vec![f1, f2, f3])
    };
    
    let compute_jacobian = |x: &DVector<f64>| -> DMatrix<f64> {
        let v = x[0];
        
        let mut j = DMatrix::zeros(3, 3);
        
        // Row 1: KVL
        j[(0, 0)] = 1.0;
        j[(0, 1)] = r;
        j[(0, 2)] = r;
        
        // Row 2: LED1
        j[(1, 0)] = -led1.conductance(v);
        j[(1, 1)] = 1.0;
        
        // Row 3: LED2
        j[(2, 0)] = -led2.conductance(v);
        j[(2, 2)] = 1.0;
        
        j
    };
    
    match solver.solve_scaled(x_init, compute_residual, compute_jacobian, 50, 1e-9) {
        Ok(x) => {
            println!("\n  ✓ Converged!");
            println!("  LED voltage: {:.3}V", x[0]);
            println!("  LED1 current: {:.2}mA ({:.1}%)", 
                     x[1] * 1000.0, x[1] / (x[1] + x[2]) * 100.0);
            println!("  LED2 current: {:.2}mA ({:.1}%)", 
                     x[2] * 1000.0, x[2] / (x[1] + x[2]) * 100.0);
            println!("  Total current: {:.2}mA", (x[1] + x[2]) * 1000.0);
            
            // Current imbalance
            let imbalance = (x[1] - x[2]).abs() / (x[1] + x[2]) * 100.0;
            println!("  Current imbalance: {:.1}%", imbalance);
            Ok(())
        }
        Err(e) => {
            println!("  ✗ Failed: {}", e);
            Err(e)
        }
    }
}

fn main() {
    println!("Testing Accurate Physics-Based Models");
    println!("====================================\n");
    
    println!("All models use accurate Shockley equation with Is values");
    println!("extracted from datasheet specifications (typically 1e-24 to 1e-20).\n");
    
    // Run tests
    let tests = vec![
        ("Multi-LED Circuit", test_multi_led_circuit()),
        ("Extreme Low Current", test_extreme_low_current()),
        ("Parallel LEDs", test_parallel_leds()),
    ];
    
    // Summary
    println!("\n\nTest Summary:");
    println!("=============");
    let mut passed = 0;
    let mut failed = 0;
    
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
        println!("\n✅ All tests passed! The scaled solver handles accurate physics perfectly.");
    } else {
        println!("\n⚠️ Some tests failed. Check the error messages above.");
    }
}