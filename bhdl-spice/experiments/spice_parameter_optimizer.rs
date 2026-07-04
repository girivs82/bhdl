/// SPICE Parameter Optimization Tool
/// 
/// This tool compares the perturbation method results with traditional SPICE
/// algorithms to find optimal parameters (timestep, ramp steps, relaxation factor)

use std::fs::File;
use std::io::Write;
use std::collections::HashMap;
use nalgebra::{DMatrix, DVector};

// Import the robust generic solver as a module
#[path = "robust_generic_solver.rs"]
mod solver;

use solver::*;

/// Traditional SPICE expected values for common circuits
struct SpiceReference {
    diode_forward_voltage: f64,      // Silicon diode ~0.7V
    diode_saturation_current: f64,   // Is = 1e-12 A
    thermal_voltage: f64,            // Vt = 26mV at room temp
    tolerance: f64,                  // Acceptable deviation
}

impl Default for SpiceReference {
    fn default() -> Self {
        Self {
            diode_forward_voltage: 0.7,
            diode_saturation_current: 1e-12,
            thermal_voltage: 0.026,
            tolerance: 0.01, // 1% tolerance
        }
    }
}

/// Parameter set for optimization
#[derive(Debug, Clone)]
struct SimulationParams {
    timestep: f64,
    ramp_steps: usize,
    relaxation_factor: f64,
}

/// Results from a simulation run
#[derive(Debug)]
struct SimulationResult {
    converged: bool,
    iterations_used: usize,
    diode_voltage: f64,
    diode_current: f64,
    deviation_percent: f64,
    computation_time_ms: f64,
}

/// Calculate expected diode voltage using Newton-Raphson (SPICE-like)
fn calculate_spice_diode_voltage(
    supply_voltage: f64, 
    series_resistance: f64,
    is: f64,
    vt: f64
) -> f64 {
    // Solve: V_supply = V_diode + I_diode * R_series
    // where I_diode = Is * (exp(V_diode/Vt) - 1)
    
    let mut vd = 0.6; // Initial guess
    let max_iter = 50;
    let tol = 1e-9;
    
    for _ in 0..max_iter {
        let id = is * ((vd / vt).exp() - 1.0);
        let f = vd + id * series_resistance - supply_voltage;
        let df = 1.0 + (is / vt) * (vd / vt).exp() * series_resistance;
        
        let delta = f / df;
        vd -= delta;
        
        if delta.abs() < tol {
            break;
        }
    }
    
    vd
}

/// Run simulation with given parameters and measure results
fn run_simulation_test(params: &SimulationParams) -> SimulationResult {
    let start_time = std::time::Instant::now();
    
    // Create simple diode circuit: 1V -> R(100Ω) -> D -> GND
    let mut circuit = RobustGenericSolver::new(3);  // 3 nodes: 0=supply, 1=ground, 2=diode anode
    
    // Add elements
    circuit.add_element(0, Box::new(VoltageSource::new(1.0, "V1")));
    circuit.add_element(1, Box::new(Resistor::new(100.0, "R1")));
    circuit.add_element(2, Box::new(Diode::new(1e-12, 0.026, "D1")));
    
    // Connect circuit
    circuit.connect(0, 0, 1); // V1: node 0 to node 1 (ground)
    circuit.connect(1, 0, 2); // R1: node 0 to node 2 (diode anode)
    circuit.connect(2, 2, 1); // D1: node 2 to node 1 (ground)
    
    // Override DC analysis parameters
    circuit.dc_timestep = params.timestep;
    circuit.dc_ramp_steps = params.ramp_steps;
    circuit.relaxation_factor = params.relaxation_factor;
    
    // Run DC analysis
    let converged = circuit.dc_analysis();
    let iterations_used = circuit.total_iterations_used;
    
    // Get results
    let vd = circuit.get_node_voltage(2);
    let id = if let Some(element) = circuit.get_element(2) {
        element.get_current()
    } else {
        0.0
    };
    
    // Calculate expected SPICE result
    let spice_ref = SpiceReference::default();
    let expected_vd = calculate_spice_diode_voltage(
        1.0, 
        100.0, 
        spice_ref.diode_saturation_current,
        spice_ref.thermal_voltage
    );
    
    let deviation_percent = ((vd - expected_vd) / expected_vd * 100.0).abs();
    let computation_time_ms = start_time.elapsed().as_secs_f64() * 1000.0;
    
    SimulationResult {
        converged,
        iterations_used,
        diode_voltage: vd,
        diode_current: id * 1000.0, // Convert to mA
        deviation_percent,
        computation_time_ms,
    }
}

/// Perform parameter sweep to find optimal settings
fn parameter_sweep() -> Vec<(SimulationParams, SimulationResult)> {
    let mut results = Vec::new();
    
    // Define parameter ranges
    let timesteps = vec![
        1e-3,   // millisecond
        1e-6,   // microsecond  
        1e-9,   // nanosecond
        1e-12,  // picosecond
        1e-15,  // femtosecond
    ];
    
    let ramp_steps = vec![10, 20, 50, 100, 200];
    let relaxation_factors = vec![0.05, 0.1, 0.2, 0.3, 0.5];
    
    println!("Starting parameter sweep...\n");
    println!("Timestep       Ramp Steps  Relax Factor  Converged  Iterations  Vd(V)    Id(mA)   Deviation(%)  Time(ms)");
    println!("--------------------------------------------------------------------------------------------------------");
    
    for &dt in &timesteps {
        for &ramps in &ramp_steps {
            for &relax in &relaxation_factors {
                let params = SimulationParams {
                    timestep: dt,
                    ramp_steps: ramps,
                    relaxation_factor: relax,
                };
                
                let result = run_simulation_test(&params);
                
                println!("{:e}  {:10}  {:12.2}  {:9}  {:10}  {:7.4}  {:7.3}  {:12.3}  {:8.2}",
                    params.timestep,
                    params.ramp_steps,
                    params.relaxation_factor,
                    result.converged,
                    result.iterations_used,
                    result.diode_voltage,
                    result.diode_current,
                    result.deviation_percent,
                    result.computation_time_ms
                );
                
                results.push((params.clone(), result));
            }
        }
    }
    
    results
}

/// Analyze results and find optimal parameters
fn analyze_results(results: &[(SimulationParams, SimulationResult)]) {
    println!("\n=== ANALYSIS RESULTS ===\n");
    
    // Filter only converged results
    let converged_results: Vec<_> = results.iter()
        .filter(|(_, r)| r.converged)
        .collect();
    
    if converged_results.is_empty() {
        println!("No converged solutions found!");
        return;
    }
    
    println!("Total simulations: {}", results.len());
    println!("Converged: {} ({:.1}%)", 
        converged_results.len(), 
        converged_results.len() as f64 / results.len() as f64 * 100.0
    );
    
    // Find best accuracy (lowest deviation)
    let best_accuracy = converged_results.iter()
        .min_by(|a, b| a.1.deviation_percent.partial_cmp(&b.1.deviation_percent).unwrap())
        .unwrap();
    
    println!("\nBest Accuracy:");
    println!("  Timestep: {:e} s", best_accuracy.0.timestep);
    println!("  Ramp steps: {}", best_accuracy.0.ramp_steps);
    println!("  Relaxation factor: {}", best_accuracy.0.relaxation_factor);
    println!("  Deviation: {:.4}%", best_accuracy.1.deviation_percent);
    println!("  Vd: {:.4} V (SPICE: ~0.7 V)", best_accuracy.1.diode_voltage);
    
    // Find fastest accurate solution (deviation < 5%)
    let fast_accurate = converged_results.iter()
        .filter(|(_, r)| r.deviation_percent < 5.0)
        .min_by(|a, b| a.1.computation_time_ms.partial_cmp(&b.1.computation_time_ms).unwrap());
    
    if let Some(fast) = fast_accurate {
        println!("\nFastest Accurate Solution (< 5% deviation):");
        println!("  Timestep: {:e} s", fast.0.timestep);
        println!("  Ramp steps: {}", fast.0.ramp_steps);
        println!("  Relaxation factor: {}", fast.0.relaxation_factor);
        println!("  Deviation: {:.4}%", fast.1.deviation_percent);
        println!("  Time: {:.2} ms", fast.1.computation_time_ms);
    }
    
    // Group by timestep to see trends
    println!("\nDeviation by Timestep (averaged):");
    let mut timestep_groups: HashMap<String, Vec<f64>> = HashMap::new();
    
    for (params, result) in converged_results {
        let key = format!("{:e}", params.timestep);
        timestep_groups.entry(key).or_insert_with(Vec::new).push(result.deviation_percent);
    }
    
    let mut sorted_timesteps: Vec<_> = timestep_groups.into_iter().collect();
    sorted_timesteps.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    
    for (timestep, deviations) in sorted_timesteps {
        let avg_deviation = deviations.iter().sum::<f64>() / deviations.len() as f64;
        println!("  {}: {:.4}% average deviation", timestep, avg_deviation);
    }
}

/// Test more complex circuits
fn test_complex_circuits(optimal_params: &SimulationParams) {
    println!("\n=== TESTING COMPLEX CIRCUITS WITH OPTIMAL PARAMETERS ===\n");
    
    // Test 1: Half-wave rectifier
    println!("Test 1: Half-wave Rectifier");
    test_half_wave_rectifier_spice(optimal_params);
    
    // Test 2: Voltage clamp
    println!("\nTest 2: Voltage Clamp Circuit");
    test_voltage_clamp_spice(optimal_params);
    
    // Test 3: LED current limiting
    println!("\nTest 3: LED Current Limiting");
    test_led_circuit_spice(optimal_params);
}

fn test_half_wave_rectifier_spice(params: &SimulationParams) {
    let mut circuit = RobustGenericSolver::new(3);
    
    // AC source approximated as DC for this test
    circuit.add_element(0, Box::new(VoltageSource::new(5.0, "Vac")));
    circuit.add_element(1, Box::new(Diode::new(1e-12, 0.026, "D1")));
    circuit.add_element(2, Box::new(Resistor::new(1000.0, "RL")));
    
    circuit.connect(0, 0, 1); // Vac
    circuit.connect(1, 0, 2); // D1
    circuit.connect(2, 2, 1); // RL
    
    circuit.dc_timestep = params.timestep;
    circuit.dc_ramp_steps = params.ramp_steps;
    circuit.relaxation_factor = params.relaxation_factor;
    
    if circuit.dc_analysis() {
        let vout = circuit.get_node_voltage(2);
        let expected = 5.0 - 0.7; // Supply minus diode drop
        let deviation = ((vout - expected) / expected * 100.0).abs();
        
        println!("  Output voltage: {:.3} V (expected: ~{:.3} V)", vout, expected);
        println!("  Deviation: {:.2}%", deviation);
    } else {
        println!("  Failed to converge!");
    }
}

fn test_voltage_clamp_spice(params: &SimulationParams) {
    let mut circuit = RobustGenericSolver::new(3);
    
    circuit.add_element(0, Box::new(VoltageSource::new(10.0, "Vin")));
    circuit.add_element(1, Box::new(Resistor::new(100.0, "R1")));
    circuit.add_element(2, Box::new(Diode::new(1e-12, 0.026, "D1")));
    
    circuit.connect(0, 0, 1);
    circuit.connect(1, 0, 2);
    circuit.connect(2, 2, 1);
    
    circuit.dc_timestep = params.timestep;
    circuit.dc_ramp_steps = params.ramp_steps;
    circuit.relaxation_factor = params.relaxation_factor;
    
    if circuit.dc_analysis() {
        let vclamped = circuit.get_node_voltage(2);
        println!("  Clamped voltage: {:.3} V (expected: ~0.7 V)", vclamped);
        println!("  Deviation: {:.2}%", ((vclamped - 0.7) / 0.7 * 100.0).abs());
    } else {
        println!("  Failed to converge!");
    }
}

fn test_led_circuit_spice(params: &SimulationParams) {
    let mut circuit = RobustGenericSolver::new(3);
    
    // LED with typical forward voltage ~2V
    let led_is = 1e-12;
    let led_vt = 0.026;
    let led_n = 2.0; // Ideality factor for LED
    
    circuit.add_element(0, Box::new(VoltageSource::new(5.0, "Vcc")));
    circuit.add_element(1, Box::new(Resistor::new(220.0, "R1"))); // Current limiting
    circuit.add_element(2, Box::new(Diode::new(led_is, led_vt * led_n, "LED")));
    
    circuit.connect(0, 0, 1);
    circuit.connect(1, 0, 2);
    circuit.connect(2, 2, 1);
    
    circuit.dc_timestep = params.timestep;
    circuit.dc_ramp_steps = params.ramp_steps;
    circuit.relaxation_factor = params.relaxation_factor;
    
    if circuit.dc_analysis() {
        let vled = circuit.get_node_voltage(2);
        let iled = (5.0 - vled) / 220.0 * 1000.0; // in mA
        
        println!("  LED voltage: {:.3} V (expected: ~2.0 V)", vled);
        println!("  LED current: {:.1} mA (expected: ~14 mA)", iled);
    } else {
        println!("  Failed to converge!");
    }
}

fn main() {
    println!("=== SPICE PARAMETER OPTIMIZATION ===\n");
    
    // Run parameter sweep
    let results = parameter_sweep();
    
    // Analyze results
    analyze_results(&results);
    
    // Find optimal parameters
    let optimal = results.iter()
        .filter(|(_, r)| r.converged && r.deviation_percent < 5.0)
        .min_by(|a, b| {
            // Balance accuracy and speed
            let score_a = a.1.deviation_percent + a.1.computation_time_ms / 100.0;
            let score_b = b.1.deviation_percent + b.1.computation_time_ms / 100.0;
            score_a.partial_cmp(&score_b).unwrap()
        });
    
    if let Some((params, _)) = optimal {
        println!("\n=== RECOMMENDED PARAMETERS ===");
        println!("Timestep: {:e} s", params.timestep);
        println!("Ramp steps: {}", params.ramp_steps);
        println!("Relaxation factor: {}", params.relaxation_factor);
        
        // Test with complex circuits
        test_complex_circuits(params);
    }
    
    // Save detailed results
    let mut file = File::create("tests/outputs/spice_optimization_results.csv").unwrap();
    writeln!(file, "timestep,ramp_steps,relaxation_factor,converged,iterations,vd,id_ma,deviation_percent,time_ms").unwrap();
    
    for (params, result) in &results {
        writeln!(file, "{:e},{},{},{},{},{:.6},{:.3},{:.4},{:.2}",
            params.timestep,
            params.ramp_steps,
            params.relaxation_factor,
            result.converged,
            result.iterations_used,
            result.diode_voltage,
            result.diode_current,
            result.deviation_percent,
            result.computation_time_ms
        ).unwrap();
    }
    
    println!("\nDetailed results saved to: tests/outputs/spice_optimization_results.csv");
}