/// Compare all solver implementations
/// 
/// This test runs the same RLC circuit through:
/// 1. Stable solver (traditional approach)
/// 2. GPU-ready solver (perturbation with parallelism)
/// 3. Traditional SPICE solver (if available)

use bhdl_spice::perturbation::stable_solver::{StableCircuit, ComponentType as StableComponentType};
use bhdl_spice::perturbation::gpu_ready::{GpuCircuit, ComponentType as GpuComponentType};
use std::fs::File;
use std::io::Write;
use std::time::Instant;

fn main() {
    println!("=== Multi-Solver Comparison ===\n");
    
    // Circuit parameters
    let r: f64 = 50.0;   // 50Ω (overdamped)
    let l: f64 = 10e-3;  // 10mH
    let c: f64 = 100e-6; // 100µF
    let v_step = 5.0;    // 5V step
    
    // Simulation parameters
    let dt = 10e-6;       // 10 µs
    let total_time = 0.02; // 20 ms (enough to see response)
    let steps = (total_time / dt) as usize;
    let step_time = 0.001; // Apply step at 1ms
    
    println!("Circuit: {}Ω - {}mH - {}µF", r, l * 1000.0, c * 1e6);
    println!("Simulation: {} steps of {} µs\n", steps, dt * 1e6);
    
    // 1. Run stable solver
    println!("1. Running stable solver (MNA with backward Euler)...");
    let start = Instant::now();
    
    let mut stable_circuit = StableCircuit::new(4);
    stable_circuit.add_component(StableComponentType::VoltageSource(0.0), 1, 0);
    stable_circuit.add_component(StableComponentType::Resistor(r), 1, 2);
    stable_circuit.add_component(StableComponentType::Inductor(l), 2, 3);
    stable_circuit.add_component(StableComponentType::Capacitor(c), 3, 0);
    
    let mut stable_results = Vec::new();
    
    for step in 0..steps {
        let time = step as f64 * dt;
        if time >= step_time && time < step_time + dt {
            stable_circuit.set_voltage_source(0, v_step);
        }
        
        stable_circuit.step(dt);
        
        if step % 10 == 0 {
            stable_results.push((
                time * 1000.0,
                stable_circuit.get_component_voltage(3), // Capacitor voltage
                stable_circuit.get_component_current(1), // Current
            ));
        }
    }
    
    let stable_time = start.elapsed();
    println!("   Completed in {:.2} ms", stable_time.as_secs_f64() * 1000.0);
    println!("   Final V_cap: {:.3} V", stable_results.last().unwrap().1);
    
    // 2. Run GPU-ready solver
    println!("\n2. Running GPU-ready perturbation solver...");
    let start = Instant::now();
    
    let mut gpu_circuit = GpuCircuit::new(4);
    gpu_circuit.add_component(GpuComponentType::VoltageSource { v: 0.0 }, 1, 0);
    gpu_circuit.add_component(GpuComponentType::Resistor { r }, 1, 2);
    gpu_circuit.add_component(GpuComponentType::Inductor { l }, 2, 3);
    gpu_circuit.add_component(GpuComponentType::Capacitor { c }, 3, 0);
    
    let mut gpu_results = Vec::new();
    
    for step in 0..steps {
        let time = step as f64 * dt;
        if time >= step_time && time < step_time + dt {
            gpu_circuit.set_voltage_source(0, v_step);
        }
        
        gpu_circuit.step(dt);
        
        if step % 10 == 0 {
            gpu_results.push((
                time * 1000.0,
                gpu_circuit.get_component_voltage(3), // Capacitor voltage
                gpu_circuit.get_component_current(1), // Current
            ));
        }
    }
    
    let gpu_time = start.elapsed();
    println!("   Completed in {:.2} ms", gpu_time.as_secs_f64() * 1000.0);
    println!("   Final V_cap: {:.3} V", gpu_results.last().unwrap().1);
    
    // 3. Compare results
    println!("\n3. Comparing results...");
    
    let mut max_voltage_diff: f64 = 0.0;
    let mut max_current_diff: f64 = 0.0;
    let mut sum_sq_diff: f64 = 0.0;
    
    for i in 0..stable_results.len().min(gpu_results.len()) {
        let v_diff = (stable_results[i].1 - gpu_results[i].1).abs();
        let i_diff = (stable_results[i].2 - gpu_results[i].2).abs();
        
        max_voltage_diff = max_voltage_diff.max(v_diff);
        max_current_diff = max_current_diff.max(i_diff);
        sum_sq_diff += v_diff * v_diff;
    }
    
    let rms_diff = (sum_sq_diff / stable_results.len() as f64).sqrt();
    
    println!("   Max voltage difference: {:.3} mV", max_voltage_diff * 1000.0);
    println!("   Max current difference: {:.3} mA", max_current_diff * 1000.0);
    println!("   RMS voltage difference: {:.3} mV", rms_diff * 1000.0);
    
    // Write comparison data
    let mut output = File::create("tests/outputs/solver_comparison.csv").unwrap();
    writeln!(output, "time_ms,v_stable,i_stable,v_gpu,i_gpu,v_diff_mV").unwrap();
    
    for i in 0..stable_results.len().min(gpu_results.len()) {
        writeln!(output, "{:.3},{:.6},{:.6},{:.6},{:.6},{:.3}",
            stable_results[i].0,
            stable_results[i].1,
            stable_results[i].2,
            gpu_results[i].1,
            gpu_results[i].2,
            (stable_results[i].1 - gpu_results[i].1) * 1000.0
        ).unwrap();
    }
    
    // Performance comparison
    println!("\n4. Performance Summary:");
    println!("   Stable solver: {:.2} ms", stable_time.as_secs_f64() * 1000.0);
    println!("   GPU-ready solver: {:.2} ms", gpu_time.as_secs_f64() * 1000.0);
    println!("   Speedup: {:.1}x", stable_time.as_secs_f64() / gpu_time.as_secs_f64());
    
    println!("\n5. Accuracy Summary:");
    if rms_diff < 0.001 {
        println!("   ✓ Excellent agreement between solvers (<1mV RMS)");
    } else if rms_diff < 0.01 {
        println!("   ✓ Good agreement between solvers (<10mV RMS)");
    } else {
        println!("   ⚠ Moderate agreement between solvers ({}mV RMS)", rms_diff * 1000.0);
    }
    
    println!("\nResults written to: tests/outputs/solver_comparison.csv");
}