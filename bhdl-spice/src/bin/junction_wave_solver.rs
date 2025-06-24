/// Junction-Based Wave Solver
/// 
/// Demonstrates how to handle wave splitting at parallel junctions
/// Key insight: Waves split based on impedance ratios

use std::fs::File;
use std::io::Write;
use std::f64::consts::PI;

/// Wave at a connection
#[derive(Debug, Clone, Copy, Default)]
struct Wave {
    voltage: f64,
    current: f64,
}

impl Wave {
    fn new(v: f64, i: f64) -> Self {
        Self { voltage: v, current: i }
    }
    
    fn power(&self) -> f64 {
        self.voltage * self.current
    }
}

/// Junction where waves split/combine
#[derive(Debug)]
struct Junction {
    id: usize,
    voltage: f64,
    
    // Connected branches with their impedances
    branches: Vec<(usize, f64)>, // (branch_id, impedance)
    
    // Wave information
    incident_waves: Vec<Wave>,
    reflected_waves: Vec<Wave>,
}

impl Junction {
    fn new(id: usize) -> Self {
        Self {
            id,
            voltage: 0.0,
            branches: Vec::new(),
            incident_waves: Vec::new(),
            reflected_waves: Vec::new(),
        }
    }
    
    fn add_branch(&mut self, branch_id: usize, impedance: f64) {
        self.branches.push((branch_id, impedance));
        self.incident_waves.push(Wave::default());
        self.reflected_waves.push(Wave::default());
    }
    
    /// Process waves at junction using impedance-based splitting
    fn process_waves(&mut self) {
        // Calculate total admittance (1/Z)
        let total_admittance: f64 = self.branches.iter()
            .map(|(_, z)| 1.0 / z)
            .sum();
        
        // Calculate equivalent impedance seen at junction
        let z_eq = 1.0 / total_admittance;
        
        // Sum all incident waves
        let total_incident_current: f64 = self.incident_waves.iter()
            .map(|w| w.current)
            .sum();
        
        // Junction voltage from incident waves
        self.voltage = total_incident_current * z_eq;
        
        // Calculate reflected waves for each branch
        for (i, (_, z_branch)) in self.branches.iter().enumerate() {
            // Reflection coefficient for this branch
            let gamma = (z_branch - z_eq) / (z_branch + z_eq);
            
            // Reflected wave
            self.reflected_waves[i].voltage = gamma * self.incident_waves[i].voltage;
            self.reflected_waves[i].current = -gamma * self.incident_waves[i].current;
            
            // Current into branch (based on voltage division)
            let branch_current = self.voltage / z_branch;
            
            // Update transmitted wave into branch
            // This would feed into the next component
        }
    }
}

/// Test parallel RLC with proper wave splitting
fn test_parallel_rlc_waves() {
    println!("=== Junction-Based Wave Solver Demo ===\n");
    println!("Circuit: 5V -> 10Ω -> Junction -> (100Ω || 10mH || 100µF) -> GND\n");
    
    let dt = 1e-6;
    let freq = 1000.0; // 1kHz for impedance calculation
    let omega = 2.0 * PI * freq;
    
    // Component impedances at 1kHz
    let z_source = 10.0;
    let z_r = 100.0;
    let z_l = omega * 10e-3; // jωL ≈ 62.8Ω at 1kHz
    let z_c = 1.0 / (omega * 100e-6); // 1/(jωC) ≈ 1.59Ω at 1kHz
    
    println!("Impedances at {} Hz:", freq);
    println!("  Source resistor: {:.1} Ω", z_source);
    println!("  Parallel R: {:.1} Ω", z_r);
    println!("  Parallel L: {:.1} Ω (reactive)", z_l);
    println!("  Parallel C: {:.1} Ω (reactive)", z_c);
    
    // Create junction
    let mut junction = Junction::new(0);
    junction.add_branch(0, z_source); // From source
    junction.add_branch(1, z_r);      // To resistor
    junction.add_branch(2, z_l);      // To inductor
    junction.add_branch(3, z_c);      // To capacitor
    
    // Calculate equivalent impedance of parallel RLC
    let y_parallel = 1.0/z_r + 1.0/z_l + 1.0/z_c;
    let z_parallel = 1.0 / y_parallel;
    let z_total = z_source + z_parallel;
    
    println!("\nCircuit analysis:");
    println!("  Parallel impedance: {:.1} Ω", z_parallel);
    println!("  Total impedance: {:.1} Ω", z_total);
    
    // Apply 5V step
    let v_source = 5.0;
    let i_total = v_source / z_total;
    
    println!("\nSteady-state (at {} Hz):", freq);
    println!("  Total current: {:.1} mA", i_total * 1000.0);
    
    // Incident wave from source
    junction.incident_waves[0] = Wave::new(v_source, i_total);
    
    // Process waves at junction
    junction.process_waves();
    
    println!("\nWave splitting at junction:");
    println!("  Junction voltage: {:.3} V", junction.voltage);
    
    // Current into each branch
    let i_r = junction.voltage / z_r;
    let i_l = junction.voltage / z_l;
    let i_c = junction.voltage / z_c;
    
    println!("  Current into R: {:.2} mA", i_r * 1000.0);
    println!("  Current into L: {:.2} mA", i_l * 1000.0);
    println!("  Current into C: {:.2} mA", i_c * 1000.0);
    println!("  Total: {:.2} mA (should equal source current)", (i_r + i_l + i_c) * 1000.0);
    
    println!("\nReflection coefficients:");
    for (i, (branch_id, z)) in junction.branches.iter().enumerate() {
        if *branch_id > 0 { // Skip source branch
            let gamma = (z - z_parallel) / (z + z_parallel);
            println!("  Branch {}: Γ = {:.3}", branch_id, gamma);
        }
    }
    
    // Save results
    let mut file = File::create("tests/outputs/junction_wave_demo.csv").unwrap();
    writeln!(file, "branch,impedance_ohm,current_mA,reflection_coeff").unwrap();
    writeln!(file, "R,{:.1},{:.2},{:.3}", z_r, i_r * 1000.0, (z_r - z_parallel)/(z_r + z_parallel)).unwrap();
    writeln!(file, "L,{:.1},{:.2},{:.3}", z_l, i_l * 1000.0, (z_l - z_parallel)/(z_l + z_parallel)).unwrap();
    writeln!(file, "C,{:.1},{:.2},{:.3}", z_c, i_c * 1000.0, (z_c - z_parallel)/(z_c + z_parallel)).unwrap();
    
    println!("\n{}", "=".repeat(60));
    println!("\nKEY INSIGHTS:");
    println!("1. Waves split at junctions based on impedance ratios");
    println!("2. Each branch sees different reflection coefficient");
    println!("3. Low impedance branches (C) get more current");
    println!("4. High impedance branches (R) have larger reflections");
    println!("\nThis is what's missing from the empirical approach!");
    println!("For general circuits, we need this impedance-based splitting.");
}

fn main() {
    test_parallel_rlc_waves();
}