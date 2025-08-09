//! GPU-accelerated multi-region parallel solving
//! 
//! Enables concurrent solving of multiple stable regions identified
//! by Phase 0, with each region processed on separate GPU streams.

use std::sync::Arc;
use anyhow::Result;
use log::{info, debug};
use rayon::prelude::*;

use super::gpu_context::GpuContext;
use crate::{
    circuit::Circuit,
    generic_glacier_solver::{GenericGlacierSolver, SolverConfig, Variable},
    spice_equation_system::SpiceEquationSystem,
};

/// Region definition for parallel solving
#[derive(Debug, Clone)]
pub struct SolutionRegion {
    pub start_ramp: f64,
    pub end_ramp: f64,
    pub initial_guess: Vec<f64>,
    pub is_sharp_transition: bool,
}

/// Solution from a specific region
#[derive(Debug, Clone)]
pub struct RegionSolution {
    pub region: SolutionRegion,
    pub converged: bool,
    pub solution: Vec<Variable>,
    pub iterations: usize,
    pub final_error: f64,
}

/// Multi-region GPU solver
pub struct MultiRegionGpu {
    context: Arc<GpuContext>,
    use_gpu: bool,
}

impl MultiRegionGpu {
    pub fn new(context: Arc<GpuContext>) -> Self {
        Self {
            context,
            use_gpu: true,
        }
    }
    
    /// Solve multiple regions in parallel
    pub fn solve_regions(
        &self,
        circuit: &Circuit,
        regions: Vec<SolutionRegion>,
        config: &SolverConfig,
    ) -> Result<Vec<RegionSolution>> {
        info!("Solving {} regions in parallel", regions.len());
        
        if self.use_gpu && regions.len() > 1 {
            // GPU path: Use multiple compute queues/streams
            self.solve_regions_gpu(circuit, regions, config)
        } else {
            // CPU fallback: Use rayon for parallel execution
            self.solve_regions_cpu(circuit, regions, config)
        }
    }
    
    /// GPU-accelerated multi-region solving
    fn solve_regions_gpu(
        &self,
        circuit: &Circuit,
        regions: Vec<SolutionRegion>,
        config: &SolverConfig,
    ) -> Result<Vec<RegionSolution>> {
        // For now, we'll use CPU parallelism with rayon
        // Full GPU implementation would create separate command queues
        // and dispatch region solvers to different GPU streams
        
        info!("Using GPU-aware CPU parallelism for multi-region solving");
        self.solve_regions_cpu(circuit, regions, config)
    }
    
    /// CPU parallel multi-region solving using rayon
    fn solve_regions_cpu(
        &self,
        circuit: &Circuit,
        regions: Vec<SolutionRegion>,
        config: &SolverConfig,
    ) -> Result<Vec<RegionSolution>> {
        // Solve regions in parallel using rayon
        let solutions: Vec<RegionSolution> = regions
            .into_par_iter()
            .map(|region| {
                debug!("Solving region [{:.2}, {:.2}]", region.start_ramp, region.end_ramp);
                
                // Create equation system for this region
                let mut equation_system = match SpiceEquationSystem::new(circuit.clone()) {
                    Ok(sys) => sys,
                    Err(e) => {
                        return RegionSolution {
                            region: region.clone(),
                            converged: false,
                            solution: vec![],
                            iterations: 0,
                            final_error: 1.0,
                        };
                    }
                };
                
                // Create variables with initial guess from region
                let mut variables = equation_system.create_variables();
                
                // Apply initial guess if available
                if !region.initial_guess.is_empty() {
                    for (var, &value) in variables.iter_mut().zip(region.initial_guess.iter()) {
                        var.value = value;
                    }
                }
                
                // Configure solver for this region
                let mut region_config = config.clone();
                if region.is_sharp_transition {
                    // Use more conservative settings for sharp transitions
                    region_config.max_iterations *= 2;
                    region_config.damping_factor = 0.3;
                }
                
                // Set ramp to midpoint of region
                let mid_ramp = (region.start_ramp + region.end_ramp) / 2.0;
                equation_system.set_voltage_ramp(mid_ramp);
                
                // Create solver instance
                let mut solver = GenericGlacierSolver::new(region_config);
                
                // Solve
                match solver.solve(&mut variables, &equation_system) {
                    Ok(stats) => RegionSolution {
                        region: region.clone(),
                        converged: true,
                        solution: variables,
                        iterations: stats.iterations,
                        final_error: stats.final_error,
                    },
                    Err(_) => RegionSolution {
                        region: region.clone(),
                        converged: false,
                        solution: variables,
                        iterations: config.max_iterations,
                        final_error: 1.0,
                    },
                }
            })
            .collect();
        
        // Log results
        let converged_count = solutions.iter().filter(|s| s.converged).count();
        info!(
            "Multi-region solving complete: {}/{} regions converged",
            converged_count,
            solutions.len()
        );
        
        Ok(solutions)
    }
    
    /// Select best solution from multiple regions
    pub fn select_best_solution(solutions: &[RegionSolution]) -> Option<&RegionSolution> {
        solutions
            .iter()
            .filter(|s| s.converged)
            .min_by(|a, b| {
                // Prefer solutions with lower error
                a.final_error
                    .partial_cmp(&b.final_error)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

/// Identify stable regions from Phase 0 results
pub fn identify_stable_regions(
    ramp_results: &[(f64, f64, bool)], // (ramp, gradient, converged)
    sharp_transitions: &[(f64, f64)],
) -> Vec<SolutionRegion> {
    let mut regions = Vec::new();
    let mut current_start = 0.0;
    
    // Sort sharp transitions
    let mut transitions = sharp_transitions.to_vec();
    transitions.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    
    // Create regions between transitions
    for (trans_start, trans_end) in transitions {
        if current_start < trans_start {
            // Stable region before transition
            regions.push(SolutionRegion {
                start_ramp: current_start,
                end_ramp: trans_start,
                initial_guess: vec![],
                is_sharp_transition: false,
            });
        }
        
        // Sharp transition region (needs careful handling)
        regions.push(SolutionRegion {
            start_ramp: trans_start,
            end_ramp: trans_end,
            initial_guess: vec![],
            is_sharp_transition: true,
        });
        
        current_start = trans_end;
    }
    
    // Final region
    if current_start < 1.0 {
        regions.push(SolutionRegion {
            start_ramp: current_start,
            end_ramp: 1.0,
            initial_guess: vec![],
            is_sharp_transition: false,
        });
    }
    
    regions
}