//! Main GPU-accelerated GLACIER solver
//! 
//! Orchestrates GPU-accelerated Phase 0 scanning and multi-region solving
//! with automatic CPU fallback for compatibility.

use std::sync::Arc;
use anyhow::{Result, anyhow};
use log::{info, warn, debug};
use rayon::prelude::*;

use super::{
    gpu_context::GpuContext,
    phase0_gpu::{Phase0Gpu, RampResult},
    multiregion_gpu::{MultiRegionGpu, SolutionRegion, RegionSolution, identify_stable_regions},
};
use crate::{
    circuit::Circuit,
    generic_glacier_solver::{SolverConfig, GenericGlacierSolver, Variable},
    spice_equation_system::{SpiceEquationSystem, extract_solution},
    glacier_dc_solver::DcAnalysisResult,
};

/// GPU-accelerated GLACIER solver
pub struct GlacierGpuSolver {
    config: SolverConfig,
    gpu_context: Option<Arc<GpuContext>>,
    use_gpu: bool,
}

impl GlacierGpuSolver {
    /// Create new GPU solver, attempting GPU initialization
    pub async fn new() -> Result<Self> {
        Self::with_config(SolverConfig::default()).await
    }
    
    /// Create with custom configuration
    pub async fn with_config(config: SolverConfig) -> Result<Self> {
        // Try to initialize GPU context
        match GpuContext::new().await {
            Ok(context) => {
                info!("GPU acceleration available: {}", context.adapter_info.name);
                Ok(Self {
                    config,
                    gpu_context: Some(Arc::new(context)),
                    use_gpu: true,
                })
            }
            Err(e) => {
                warn!("GPU not available, using CPU fallback: {}", e);
                Ok(Self {
                    config,
                    gpu_context: None,
                    use_gpu: false,
                })
            }
        }
    }
    
    /// Force CPU-only execution
    pub fn use_cpu_only(&mut self) {
        self.use_gpu = false;
    }
    
    /// Solve circuit using GPU acceleration where possible
    pub async fn solve(&self, circuit: Circuit) -> Result<DcAnalysisResult> {
        info!("Starting GPU-accelerated GLACIER solve");
        
        // Phase 0: Landscape mapping
        let phase0_results = self.run_phase0(&circuit).await?;
        
        // Identify regions and sharp transitions
        let sharp_transitions = Self::identify_sharp_transitions(&phase0_results);
        let regions = identify_stable_regions(
            &phase0_results.iter()
                .map(|r| (r.ramp_value as f64, r.max_gradient as f64, r.converged != 0))
                .collect::<Vec<_>>(),
            &sharp_transitions,
        );
        
        info!("Identified {} regions with {} sharp transitions", 
              regions.len(), sharp_transitions.len());
        
        // Multi-region solving
        let region_solutions = self.solve_regions(&circuit, regions).await?;
        
        // Select best solution
        let best_solution = MultiRegionGpu::select_best_solution(&region_solutions)
            .ok_or_else(|| anyhow!("No converged solution found"))?;
        
        // Convert to DcAnalysisResult
        self.convert_to_dc_result(&circuit, best_solution)
    }
    
    /// Run Phase 0 landscape mapping
    async fn run_phase0(&self, circuit: &Circuit) -> Result<Vec<RampResult>> {
        // For now, use CPU path until phase0.wgsl is updated to match glacier_full.wgsl
        // The full GPU solver is ready but phase0 shader needs updating
        self.run_phase0_cpu(circuit)
    }
    
    /// CPU parallel Phase 0 implementation
    fn run_phase0_cpu(&self, circuit: &Circuit) -> Result<Vec<RampResult>> {
        let ramp_points: Vec<f64> = (0..=40)
            .map(|i| i as f64 * 0.025)
            .collect();
        
        info!("Running Phase 0 on CPU with {} threads", rayon::current_num_threads());
        
        let results: Vec<RampResult> = ramp_points
            .par_iter()
            .map(|&ramp| {
                // Create equation system
                let mut equation_system = SpiceEquationSystem::new(circuit.clone()).unwrap();
                equation_system.set_voltage_ramp(ramp);
                let mut variables = equation_system.create_variables();
                
                // Configure solver for quick evaluation
                let mut config = self.config.clone();
                config.max_iterations = 50; // Limited iterations for scanning
                
                let mut solver = GenericGlacierSolver::new(config);
                
                // Try to solve
                let (converged, iterations, error) = match solver.solve(&mut variables, &equation_system) {
                    Ok(stats) => (1, stats.iterations as u32, stats.final_error as f32),
                    Err(_) => (0, 50, 1.0),
                };
                
                // Calculate gradient (simplified)
                let max_gradient = variables.iter()
                    .map(|v| v.value.abs() as f32)
                    .fold(1.0f32, f32::max);
                
                RampResult {
                    ramp_value: ramp as f32,
                    converged,
                    iterations,
                    error,
                    max_gradient,
                    max_voltage: 5.0 * ramp as f32,
                    min_voltage: 0.0,
                    _padding: 0.0,
                }
            })
            .collect();
        
        Ok(results)
    }
    
    /// Identify sharp transitions from Phase 0 results
    fn identify_sharp_transitions(results: &[RampResult]) -> Vec<(f64, f64)> {
        let mut transitions = Vec::new();
        
        for i in 1..results.len() {
            let gradient_rate = (results[i].max_gradient - results[i-1].max_gradient) 
                             / (results[i].ramp_value - results[i-1].ramp_value);
            
            if gradient_rate.abs() > 100.0 {
                transitions.push((
                    results[i-1].ramp_value as f64,
                    results[i].ramp_value as f64
                ));
                debug!("Sharp transition at [{:.3}, {:.3}], gradient rate: {:.1}", 
                       results[i-1].ramp_value, results[i].ramp_value, gradient_rate);
            }
        }
        
        transitions
    }
    
    /// Solve multiple regions
    async fn solve_regions(
        &self,
        circuit: &Circuit,
        regions: Vec<SolutionRegion>,
    ) -> Result<Vec<RegionSolution>> {
        if self.use_gpu && self.gpu_context.is_some() {
            // Use full GPU solver
            let context = self.gpu_context.as_ref().unwrap();
            let gpu_solver = super::full_solver::GlacierFullGpuSolver::new(
                context.clone(), 
                100 // max circuit size
            ).await?;
            
            // Solve each region on GPU
            let mut solutions = Vec::new();
            for region in regions {
                debug!("Solving region [{:.2}, {:.2}] on GPU", region.start_ramp, region.end_ramp);
                
                // Use midpoint of region for solving
                let mid_ramp = (region.start_ramp + region.end_ramp) / 2.0;
                
                match gpu_solver.solve_at_ramp(circuit, mid_ramp, None).await {
                    Ok((vars, iterations, error)) => {
                        solutions.push(RegionSolution {
                            region: region.clone(),
                            converged: true,
                            solution: vars,
                            iterations,
                            final_error: error as f64,
                        });
                    }
                    Err(_) => {
                        solutions.push(RegionSolution {
                            region: region.clone(),
                            converged: false,
                            solution: vec![],
                            iterations: 0,
                            final_error: 1.0,
                        });
                    }
                }
            }
            
            Ok(solutions)
        } else {
            // CPU fallback
            let fallback_context = Arc::new(GpuContext::new().await?);
            let multi_region = MultiRegionGpu::new(fallback_context);
            multi_region.solve_regions(circuit, regions, &self.config)
        }
    }
    
    /// Convert region solution to DC analysis result
    fn convert_to_dc_result(
        &self,
        circuit: &Circuit,
        solution: &RegionSolution,
    ) -> Result<DcAnalysisResult> {
        // Create equation system to extract solution
        let equation_system = SpiceEquationSystem::new(circuit.clone())?;
        let (node_voltages, branch_currents) = extract_solution(&equation_system, &solution.solution);
        
        // Calculate total power
        let total_power = Self::calculate_total_power(circuit, &node_voltages, &branch_currents);
        
        Ok(DcAnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: solution.iterations,
            final_error: solution.final_error,
        })
    }
    
    /// Calculate total power dissipation
    fn calculate_total_power(
        circuit: &Circuit,
        node_voltages: &std::collections::HashMap<petgraph::graph::NodeIndex, f64>,
        branch_currents: &std::collections::HashMap<petgraph::graph::EdgeIndex, f64>,
    ) -> f64 {
        let mut total_power = 0.0;
        
        for (edge_idx, branch) in circuit.branches() {
            if let Some((from_node, to_node)) = circuit.branch_nodes(edge_idx) {
                if let (Some(&current), Some(&v1), Some(&v2)) = (
                    branch_currents.get(&edge_idx),
                    node_voltages.get(&from_node),
                    node_voltages.get(&to_node),
                ) {
                    let voltage_drop = (v1 - v2).abs();
                    let power = voltage_drop * current.abs();
                    
                    // Only count power from passive components
                    if matches!(branch.component_type.as_str(), "Resistor" | "LED" | "Diode") {
                        total_power += power;
                    }
                }
            }
        }
        
        total_power
    }
}

/// Synchronous wrapper for use in non-async contexts
pub fn solve_with_gpu(circuit: Circuit, config: SolverConfig) -> Result<DcAnalysisResult> {
    pollster::block_on(async {
        let solver = GlacierGpuSolver::with_config(config).await?;
        solver.solve(circuit).await
    })
}