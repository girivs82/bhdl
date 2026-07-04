//! Visualize IEEE TCAD test results for easy comparison

use std::collections::HashMap;

#[derive(Debug, Clone)]
struct CircuitResult {
    name: String,
    category: String,
    cpu_serial: Option<(bool, usize, f64)>,     // (success, regions, time_ms)
    cpu_parallel: Option<(bool, usize, f64)>,
    gpu: Option<(bool, usize, f64)>,
}

fn main() {
    let results = collect_results();
    
    println!("\n{}", "=".repeat(120));
    println!("IEEE TCAD SOLVER COMPARISON - VISUAL SUMMARY");
    println!("{}", "=".repeat(120));
    
    // Overall statistics
    print_overall_stats(&results);
    
    // Performance comparison chart
    print_performance_chart(&results);
    
    // Multi-region analysis
    print_region_analysis(&results);
    
    // Convergence matrix
    print_convergence_matrix(&results);
}

fn collect_results() -> Vec<CircuitResult> {
    vec![
        // Series Nonlinear Circuits
        CircuitResult {
            name: "Series-2-LEDs".to_string(),
            category: "Series Nonlinear".to_string(),
            cpu_serial: Some((true, 1, 92.66)),
            cpu_parallel: Some((true, 1, 82.23)),
            gpu: Some((false, 0, 0.0)),
        },
        CircuitResult {
            name: "Series-3-LEDs".to_string(),
            category: "Series Nonlinear".to_string(),
            cpu_serial: Some((true, 1, 84.49)),
            cpu_parallel: Some((true, 1, 83.73)),
            gpu: Some((false, 0, 0.0)),
        },
        CircuitResult {
            name: "Series-5-LEDs-extreme".to_string(),
            category: "Series Nonlinear".to_string(),
            cpu_serial: Some((true, 3, 1.74)),
            cpu_parallel: Some((true, 3, 1.55)),
            gpu: Some((false, 0, 0.0)),
        },
        CircuitResult {
            name: "Series-10-LEDs".to_string(),
            category: "Series Nonlinear".to_string(),
            cpu_serial: Some((true, 1, 55.53)),
            cpu_parallel: Some((true, 1, 55.01)),
            gpu: Some((false, 0, 0.0)),
        },
        // Parallel LED Arrays
        CircuitResult {
            name: "Parallel-3-LEDs-matched".to_string(),
            category: "Parallel Arrays".to_string(),
            cpu_serial: Some((true, 1, 1.30)),
            cpu_parallel: Some((true, 1, 1.15)),
            gpu: Some((true, 1, 74.25)),
        },
        CircuitResult {
            name: "Parallel-5-LEDs-mismatched".to_string(),
            category: "Parallel Arrays".to_string(),
            cpu_serial: Some((true, 3, 39.46)),
            cpu_parallel: Some((true, 3, 39.72)),
            gpu: Some((true, 1, 78.28)),
        },
        // Extreme Parameters
        CircuitResult {
            name: "Single-LED-Is=1e-38".to_string(),
            category: "Extreme Parameters".to_string(),
            cpu_serial: Some((true, 3, 1.06)),
            cpu_parallel: Some((true, 3, 0.82)),
            gpu: Some((true, 1, 11.71)),
        },
        // Power Converters
        CircuitResult {
            name: "Buck-converter".to_string(),
            category: "Power Converters".to_string(),
            cpu_serial: Some((true, 1, 0.77)),
            cpu_parallel: Some((true, 1, 0.82)),
            gpu: Some((true, 1, 11.38)),
        },
        // Protection
        CircuitResult {
            name: "TVS-protection".to_string(),
            category: "Protection".to_string(),
            cpu_serial: Some((true, 8, 4.48)),
            cpu_parallel: Some((true, 8, 4.48)),
            gpu: Some((true, 1, 29.20)),
        },
        CircuitResult {
            name: "Current-limiting".to_string(),
            category: "Protection".to_string(),
            cpu_serial: Some((true, 7, 2.93)),
            cpu_parallel: Some((true, 7, 2.88)),
            gpu: Some((true, 1, 33.17)),
        },
    ]
}

fn print_overall_stats(results: &[CircuitResult]) {
    println!("\n📊 OVERALL STATISTICS");
    println!("{}", "-".repeat(60));
    
    let total = results.len() as f64;
    
    // Success rates
    let cpu_serial_success = results.iter()
        .filter(|r| r.cpu_serial.map(|(s, _, _)| s).unwrap_or(false))
        .count() as f64 / total * 100.0;
    let cpu_parallel_success = results.iter()
        .filter(|r| r.cpu_parallel.map(|(s, _, _)| s).unwrap_or(false))
        .count() as f64 / total * 100.0;
    let gpu_success = results.iter()
        .filter(|r| r.gpu.map(|(s, _, _)| s).unwrap_or(false))
        .count() as f64 / total * 100.0;
    
    println!("Success Rates:");
    println!("  CPU Serial:   {:.1}% ✅", cpu_serial_success);
    println!("  CPU Parallel: {:.1}% ✅", cpu_parallel_success);
    println!("  GPU F32:      {:.1}% ⚠️", gpu_success);
    
    // Average regions found
    let cpu_avg_regions: f64 = results.iter()
        .filter_map(|r| r.cpu_serial.map(|(s, regions, _)| if s { regions as f64 } else { 0.0 }))
        .sum::<f64>() / results.iter().filter(|r| r.cpu_serial.map(|(s, _, _)| s).unwrap_or(false)).count() as f64;
    
    let gpu_avg_regions: f64 = results.iter()
        .filter_map(|r| r.gpu.map(|(s, regions, _)| if s { regions as f64 } else { 0.0 }))
        .sum::<f64>() / results.iter().filter(|r| r.gpu.map(|(s, _, _)| s).unwrap_or(false)).count() as f64;
    
    println!("\nAverage Regions Found (when successful):");
    println!("  CPU Solvers:  {:.1} regions", cpu_avg_regions);
    println!("  GPU Solver:   {:.1} regions", gpu_avg_regions);
}

fn print_performance_chart(results: &[CircuitResult]) {
    println!("\n📈 PERFORMANCE COMPARISON (ms)");
    println!("{}", "-".repeat(100));
    println!("{:30} {:>12} {:>12} {:>12} {:>15}", "Circuit", "CPU Serial", "CPU Parallel", "GPU", "GPU Overhead");
    println!("{}", "-".repeat(100));
    
    for result in results {
        let cpu_time = result.cpu_serial.map(|(_, _, t)| t).unwrap_or(0.0);
        let gpu_time = result.gpu.map(|(s, _, t)| if s { t } else { 0.0 }).unwrap_or(0.0);
        let overhead = if cpu_time > 0.0 && gpu_time > 0.0 {
            format!("{:.1}x", gpu_time / cpu_time)
        } else {
            "-".to_string()
        };
        
        println!("{:30} {:>12} {:>12} {:>12} {:>15}",
            result.name,
            format_time_result(result.cpu_serial),
            format_time_result(result.cpu_parallel),
            format_time_result(result.gpu),
            overhead
        );
    }
}

fn print_region_analysis(results: &[CircuitResult]) {
    println!("\n🔍 MULTI-REGION DISCOVERY");
    println!("{}", "-".repeat(80));
    println!("{:30} {:>15} {:>15} {:>20}", "Circuit", "CPU Regions", "GPU Regions", "Region Loss");
    println!("{}", "-".repeat(80));
    
    for result in results {
        if let (Some((true, cpu_regions, _)), Some((true, gpu_regions, _))) = (result.cpu_serial, result.gpu) {
            if cpu_regions > 1 {
                let loss = cpu_regions - gpu_regions;
                println!("{:30} {:>15} {:>15} {:>20}",
                    result.name,
                    cpu_regions,
                    gpu_regions,
                    format!("-{} ({:.0}%)", loss, (loss as f64 / cpu_regions as f64) * 100.0)
                );
            }
        }
    }
}

fn print_convergence_matrix(results: &[CircuitResult]) {
    println!("\n✅ CONVERGENCE MATRIX");
    println!("{}", "-".repeat(80));
    println!("{:30} {:^10} {:^10} {:^10}", "Circuit", "CPU-S", "CPU-P", "GPU");
    println!("{}", "-".repeat(80));
    
    // Group by category
    let mut by_category: HashMap<String, Vec<&CircuitResult>> = HashMap::new();
    for result in results {
        by_category.entry(result.category.clone()).or_insert_with(Vec::new).push(result);
    }
    
    for (category, circuits) in by_category {
        println!("\n{}", category);
        for circuit in circuits {
            println!("{:30} {:^10} {:^10} {:^10}",
                circuit.name,
                if circuit.cpu_serial.map(|(s, _, _)| s).unwrap_or(false) { "✅" } else { "❌" },
                if circuit.cpu_parallel.map(|(s, _, _)| s).unwrap_or(false) { "✅" } else { "❌" },
                if circuit.gpu.map(|(s, _, _)| s).unwrap_or(false) { "✅" } else { "❌" }
            );
        }
    }
}

fn format_time_result(result: Option<(bool, usize, f64)>) -> String {
    match result {
        Some((true, _, time)) => format!("{:.2}", time),
        Some((false, _, _)) => "FAIL".to_string(),
        None => "-".to_string(),
    }
}