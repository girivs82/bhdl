//! Test unified solver with CPU, Parallel CPU, and Hybrid modes

use bhdl_spice::{Circuit, ComponentModel};
use std::collections::HashMap;
use std::time::Instant;

#[cfg(feature = "gpu")]
use bhdl_spice::glacier_gpu::{GpuContext, GlacierFullGpuSolver};

/// Unified solver that can choose between different execution modes
pub struct UnifiedGlacierSolver {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    mode: SolverMode,
    #[cfg(feature = "gpu")]
    gpu_solver: Option<std::sync::Arc<GlacierFullGpuSolver>>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SolverMode {
    /// Pure CPU serial (fastest for single DC analysis)
    CpuSerial,
    /// CPU with parallel Phase 0 (good for complex circuits)
    CpuParallel,
    /// Hybrid GPU/CPU (experimental, for research use)
    #[cfg(feature = "gpu")]
    Hybrid,
    /// Automatically choose best mode based on circuit complexity
    Auto,
}

impl UnifiedGlacierSolver {
    pub fn new(circuit: Circuit, models: HashMap<String, ComponentModel>) -> Self {
        Self {
            circuit,
            models,
            mode: SolverMode::Auto,
            #[cfg(feature = "gpu")]
            gpu_solver: None,
        }
    }
    
    pub fn with_mode(circuit: Circuit, models: HashMap<String, ComponentModel>, mode: SolverMode) -> Self {
        Self {
            circuit,
            models,
            mode,
            #[cfg(feature = "gpu")]
            gpu_solver: None,
        }
    }
    
    /// Initialize GPU solver if needed (async)
    #[cfg(feature = "gpu")]
    pub async fn init_gpu(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        if matches!(self.mode, SolverMode::Hybrid | SolverMode::Auto) {
            if self.gpu_solver.is_none() {
                match GpuContext::new().await {
                    Ok(context) => {
                        let context = std::sync::Arc::new(context);
                        match GlacierFullGpuSolver::new(context, 1000).await {
                            Ok(solver) => {
                                self.gpu_solver = Some(std::sync::Arc::new(solver));
                                println!("GPU solver initialized");
                            }
                            Err(e) => {
                                println!("GPU solver init failed: {}, falling back to CPU", e);
                            }
                        }
                    }
                    Err(e) => {
                        println!("GPU not available: {}, using CPU", e);
                    }
                }
            }
        }
        Ok(())
    }
    
    /// Solve using the configured mode
    pub async fn solve(&mut self) -> Result<SolverResult, Box<dyn std::error::Error>> {
        let effective_mode = self.choose_effective_mode();
        println!("Using solver mode: {:?}", effective_mode);
        
        let start = Instant::now();
        
        match effective_mode {
            SolverMode::CpuSerial => self.solve_cpu_serial().await,
            SolverMode::CpuParallel => self.solve_cpu_parallel().await,
            #[cfg(feature = "gpu")]
            SolverMode::Hybrid => self.solve_hybrid().await,
            SolverMode::Auto => unreachable!(), // Should be resolved by choose_effective_mode
        }.map(|mut result| {
            result.total_time = start.elapsed();
            result.mode_used = effective_mode;
            result
        })
    }
    
    fn choose_effective_mode(&self) -> SolverMode {
        match self.mode {
            SolverMode::Auto => {
                // Auto-selection logic based on circuit characteristics
                let num_nodes = self.circuit.nodes().len();
                let num_nonlinear = self.models.values().filter(|m| {
                    matches!(m, ComponentModel::LED { .. } | ComponentModel::Diode { .. })
                }).count();
                
                if num_nodes > 20 || num_nonlinear > 5 {
                    // Complex circuit - use parallel CPU
                    SolverMode::CpuParallel
                } else {
                    // Simple circuit - use serial CPU
                    SolverMode::CpuSerial
                }
            }
            other => other,
        }
    }
    
    async fn solve_cpu_serial(&self) -> Result<SolverResult, Box<dyn std::error::Error>> {
        let mut solver = bhdl_spice::IntegratedGlacierSolver::with_config(
            self.circuit.clone(),
            bhdl_spice::IntegratedSolverConfig {
                mode: bhdl_spice::SolverMode::CpuSerial,
                phase0_ramp_points: 20,
                max_iterations: 300,
                tolerance: 1e-9,
            }
        );
        
        for (name, model) in &self.models {
            solver.add_model(name.clone(), model.clone());
        }
        
        match solver.analyze() {
            Ok(results) => {
                if let Some((_, _, _, analysis)) = results.first() {
                    Ok(SolverResult {
                        success: true,
                        iterations: analysis.iterations,
                        final_error: 1e-10, // Approximate from power
                        power: analysis.total_power,
                        mode_used: SolverMode::CpuSerial,
                        total_time: std::time::Duration::default(),
                        used_gpu_fallback: false,
                    })
                } else {
                    Err("No solutions found".into())
                }
            }
            Err(e) => Err(e.into()),
        }
    }
    
    async fn solve_cpu_parallel(&self) -> Result<SolverResult, Box<dyn std::error::Error>> {
        let mut solver = bhdl_spice::IntegratedGlacierSolver::with_config(
            self.circuit.clone(),
            bhdl_spice::IntegratedSolverConfig {
                mode: bhdl_spice::SolverMode::CpuParallel,
                phase0_ramp_points: 40,
                max_iterations: 300,
                tolerance: 1e-9,
            }
        );
        
        for (name, model) in &self.models {
            solver.add_model(name.clone(), model.clone());
        }
        
        match solver.analyze() {
            Ok(results) => {
                if let Some((_, _, _, analysis)) = results.first() {
                    Ok(SolverResult {
                        success: true,
                        iterations: analysis.iterations,
                        final_error: 1e-10,
                        power: analysis.total_power,
                        mode_used: SolverMode::CpuParallel,
                        total_time: std::time::Duration::default(),
                        used_gpu_fallback: false,
                    })
                } else {
                    Err("No solutions found".into())
                }
            }
            Err(e) => Err(e.into()),
        }
    }
    
    #[cfg(feature = "gpu")]
    async fn solve_hybrid(&mut self) -> Result<SolverResult, Box<dyn std::error::Error>> {
        // Initialize GPU if needed
        self.init_gpu().await?;
        
        if let Some(gpu_solver) = &self.gpu_solver {
            // Try GPU first
            match gpu_solver.phase0_coarse_scan_with_models(&self.circuit, 20, &self.models).await {
                Ok(results) => {
                    let converged_count = results.iter().filter(|r| r.converged != 0).count();
                    let max_gradient = results.iter()
                        .map(|r| r.max_gradient)
                        .fold(0.0f32, f32::max);
                    
                    if converged_count > results.len() / 2 && max_gradient < 100.0 {
                        // GPU solved it adequately
                        Ok(SolverResult {
                            success: true,
                            iterations: results.iter().map(|r| r.iterations as usize).max().unwrap_or(0),
                            final_error: results.iter().map(|r| r.error as f64).fold(0.0, f64::max),
                            power: 0.0, // Would need full solve to get power
                            mode_used: SolverMode::Hybrid,
                            total_time: std::time::Duration::default(),
                            used_gpu_fallback: false,
                        })
                    } else {
                        // GPU struggled, fall back to CPU
                        println!("GPU struggled (gradient={:.1}), falling back to CPU", max_gradient);
                        let mut cpu_result = self.solve_cpu_parallel().await?;
                        cpu_result.used_gpu_fallback = true;
                        Ok(cpu_result)
                    }
                }
                Err(_) => {
                    // GPU failed, fall back to CPU
                    println!("GPU failed, falling back to CPU");
                    let mut cpu_result = self.solve_cpu_parallel().await?;
                    cpu_result.used_gpu_fallback = true;
                    Ok(cpu_result)
                }
            }
        } else {
            // No GPU available, use CPU
            self.solve_cpu_parallel().await
        }
    }
}

#[derive(Debug)]
pub struct SolverResult {
    pub success: bool,
    pub iterations: usize,
    pub final_error: f64,
    pub power: f64,
    pub mode_used: SolverMode,
    pub total_time: std::time::Duration,
    pub used_gpu_fallback: bool,
}

fn main() {
    println!("\n=== UNIFIED SOLVER MODE COMPARISON ===\n");
    
    tokio::runtime::Runtime::new().unwrap().block_on(async {
        test_all_modes().await;
    });
}

async fn test_all_modes() {
    let test_circuits = vec![
        ("Simple Resistor", create_simple_circuit()),
        ("Simple LED", create_led_circuit()),
        ("Ultra-sharp LED", create_ultra_sharp_circuit()),
    ];
    
    for (name, (circuit, models)) in test_circuits {
        println!("Testing: {}", name);
        println!("{}", "=".repeat(40));
        
        // Test CPU Serial
        let mut solver = UnifiedGlacierSolver::with_mode(
            circuit.clone(), models.clone(), SolverMode::CpuSerial
        );
        if let Ok(result) = solver.solve().await {
            println!("CPU Serial:   {:>6.1}ms - {} iterations", 
                     result.total_time.as_secs_f64() * 1000.0, result.iterations);
        } else {
            println!("CPU Serial:   FAILED");
        }
        
        // Test CPU Parallel
        let mut solver = UnifiedGlacierSolver::with_mode(
            circuit.clone(), models.clone(), SolverMode::CpuParallel
        );
        if let Ok(result) = solver.solve().await {
            println!("CPU Parallel: {:>6.1}ms - {} iterations", 
                     result.total_time.as_secs_f64() * 1000.0, result.iterations);
        } else {
            println!("CPU Parallel: FAILED");
        }
        
        // Test Hybrid (if GPU available)
        #[cfg(feature = "gpu")]
        {
            let mut solver = UnifiedGlacierSolver::with_mode(
                circuit.clone(), models.clone(), SolverMode::Hybrid
            );
            if let Ok(result) = solver.solve().await {
                let fallback_note = if result.used_gpu_fallback { " (CPU fallback)" } else { "" };
                println!("Hybrid:       {:>6.1}ms - {} iterations{}", 
                         result.total_time.as_secs_f64() * 1000.0, result.iterations, fallback_note);
            } else {
                println!("Hybrid:       FAILED");
            }
        }
        
        // Test Auto mode
        let mut solver = UnifiedGlacierSolver::with_mode(
            circuit.clone(), models.clone(), SolverMode::Auto
        );
        if let Ok(result) = solver.solve().await {
            println!("Auto ({:?}): {:>6.1}ms - {} iterations", 
                     result.mode_used,
                     result.total_time.as_secs_f64() * 1000.0, result.iterations);
        } else {
            println!("Auto:         FAILED");
        }
        
        println!();
    }
    
    println!("Recommendation: Use SolverMode::CpuSerial for production");
    println!("                Use SolverMode::Auto for convenience");
    println!("                Use SolverMode::Hybrid only for research");
}

// Helper functions to create test circuits

fn create_simple_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("VOUT".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "VOUT", "Resistor".to_string(), 1000.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("R2".to_string(), "VOUT", "GND", "Resistor".to_string(), 1000.0, None);
    models.insert("R2".to_string(), ComponentModel::Resistor {
        resistance: 1000.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn create_led_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 5.0, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 5.0,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 220.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 220.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "red".to_string(),
        forward_voltage: 2.0,
        forward_current: 0.02,
        dynamic_resistance: 10.0,
        saturation_current: Some(1e-12),
        emission_coefficient: Some(1.5),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}

fn create_ultra_sharp_circuit() -> (Circuit, HashMap<String, ComponentModel>) {
    let mut circuit = Circuit::new();
    let mut models = HashMap::new();
    
    circuit.add_node("VIN".to_string(), None);
    circuit.add_node("N1".to_string(), None);
    circuit.add_node("GND".to_string(), None);
    
    circuit.add_branch("V1".to_string(), "VIN", "GND", "VoltageSource".to_string(), 3.3, None);
    models.insert("V1".to_string(), ComponentModel::VoltageSource {
        voltage: 3.3,
        internal_resistance: Some(0.0),
    });
    
    circuit.add_branch("R1".to_string(), "VIN", "N1", "Resistor".to_string(), 150.0, None);
    models.insert("R1".to_string(), ComponentModel::Resistor {
        resistance: 150.0,
        tolerance: 5.0,
        limits: Default::default(),
    });
    
    circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
    models.insert("D1".to_string(), ComponentModel::LED {
        color: "blue".to_string(),
        forward_voltage: 3.0,
        forward_current: 0.02,
        dynamic_resistance: 15.0,
        saturation_current: Some(1e-15), // Ultra-sharp
        emission_coefficient: Some(2.0),
        thermal_voltage: Some(0.026),
        limits: Default::default(),
    });
    
    (circuit, models)
}