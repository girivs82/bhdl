// Debug the Sharp Clamp failure
use std::collections::HashMap;

#[derive(Debug, Clone)]
struct IbisTable {
    voltages: Vec<f64>,
    currents: Vec<f64>,
}

impl IbisTable {
    fn interpolate(&self, voltage: f64) -> f64 {
        if voltage <= self.voltages[0] {
            return self.currents[0];
        }
        if voltage >= *self.voltages.last().unwrap() {
            return *self.currents.last().unwrap();
        }
        
        for i in 1..self.voltages.len() {
            if voltage <= self.voltages[i] {
                let v0 = self.voltages[i-1];
                let v1 = self.voltages[i];
                let i0 = self.currents[i-1];
                let i1 = self.currents[i];
                let t = (voltage - v0) / (v1 - v0);
                return i0 + t * (i1 - i0);
            }
        }
        
        *self.currents.last().unwrap()
    }
    
    fn adaptive_delta(&self, voltage: f64) -> f64 {
        let mut min_spacing = 1.0;
        for i in 1..self.voltages.len() {
            if voltage >= self.voltages[i-1] && voltage <= self.voltages[i] {
                min_spacing = (self.voltages[i] - self.voltages[i-1]).min(min_spacing);
            }
        }
        (min_spacing * 0.01).min(1e-6)
    }
}

fn main() {
    println!("=== Sharp Clamp Debug ===");
    
    let clamp_table = IbisTable {
        voltages: vec![
            1.40, 1.43, 1.45, 1.47, 1.48, 1.49, 1.50,
            1.52, 1.53, 1.55, 1.58, 1.60
        ],
        currents: vec![
            -0.001,  // 1.40 V
            -0.003,  // 1.43 V
            -0.005,  // 1.45 V
            -0.020,  // 1.47 V
            -0.030,  // 1.48 V
            -0.040,  // 1.49 V
            -0.050,  // 1.50 V (10x jump from 1.45V!)
            -0.100,  // 1.52 V
            -0.140,  // 1.53 V
            -0.200,  // 1.55 V
            -0.300,  // 1.58 V
            -0.400   // 1.60 V
        ],
    };
    
    // Test interpolation across the sharp region
    println!("Voltage sweep across sharp transition:");
    for v in [1.40, 1.42, 1.44, 1.46, 1.48, 1.50, 1.52, 1.54, 1.56, 1.58, 1.60] {
        let i = clamp_table.interpolate(v);
        let delta = clamp_table.adaptive_delta(v);
        let i_plus = clamp_table.interpolate(v + delta);
        let i_minus = clamp_table.interpolate(v - delta);
        let conductance = (i_plus - i_minus) / (2.0 * delta);
        
        println!("  V={:.2}V: I={:.6}A, delta={:.1e}, g={:.3e}S", v, i, delta, conductance);
        if conductance.abs() > 1000.0 {
            println!("    ^^^ EXTREME CONDUCTANCE! This will cause convergence issues");
        }
    }
    
    // Show specific problem regions
    println!("\nAnalyzing sharp transitions:");
    for i in 1..clamp_table.voltages.len() {
        let v0 = clamp_table.voltages[i-1];
        let v1 = clamp_table.voltages[i];
        let i0 = clamp_table.currents[i-1];
        let i1 = clamp_table.currents[i];
        let dv = v1 - v0;
        let di = i1 - i0;
        let slope = di / dv;
        
        if slope.abs() > 1.0 {
            println!("  {:.2}V→{:.2}V: slope={:.1}A/V ({}x increase)", 
                     v0, v1, slope, (i1/i0).abs());
        }
    }
}
