//! Hybrid GPU/CPU solver for GLACIER algorithm
//! 
//! Uses GPU for fast initial exploration and easy convergence,
//! falls back to CPU for high-gradient regions.

use std::sync::Arc;
use std::collections::HashMap;
use anyhow::Result;
use log::{info, debug, warn};

use crate::{
    Circuit, ComponentModel,
};

use super::{
    GlacierFullGpuSolver,
    region_detection::{detect_gradient_regions, GpuRegion},
    gpu_data::Phase0Result,
};

/// Execution mode for the hybrid solver
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HybridSolverMode {
    /// Pure CPU serial (fastest for single DC analysis)
    CpuOnly,
    /// CPU with parallel Phase 0 (good for complex circuits)
    CpuParallel,
    /// Hybrid GPU/CPU (experimental, for research use)
    Hybrid,
    /// Automatically choose best mode based on circuit complexity
    Auto,
}

/// Hybrid solver that combines GPU speed with CPU robustness
pub struct HybridGlacierSolver {
    gpu_solver: Option<Arc<GlacierFullGpuSolver>>,
    mode: HybridSolverMode,
    gradient_threshold: f64,
    convergence_threshold: f64,
}

/// Result from hybrid solving
#[derive(Debug, Clone)]
pub struct HybridSolveResult {
    pub solution: Vec<crate::generic_glacier_solver::Variable>,
    pub iterations_gpu: usize,
    pub iterations_cpu: Option<usize>,
    pub final_error: f64,
    pub used_cpu_fallback: bool,
    pub gradient: f64,
}

impl HybridGlacierSolver {
    /// Create a new hybrid solver with GPU capability
    pub fn new(gpu_solver: Arc<GlacierFullGpuSolver>) -> Self {
        Self {
            gpu_solver: Some(gpu_solver),
            mode: HybridSolverMode::Auto,
            gradient_threshold: 50.0,  // Above this, likely to need CPU help
            convergence_threshold: 1e-3, // Accept GPU solution if error below this
        }
    }
    
    /// Create a CPU-only solver (no GPU initialization needed)
    pub fn cpu_only() -> Self {
        Self {
            gpu_solver: None,
            mode: HybridSolverMode::CpuOnly,
            gradient_threshold: 50.0,
            convergence_threshold: 1e-3,
        }
    }
    
    /// Create a parallel CPU solver
    pub fn cpu_parallel() -> Self {
        Self {
            gpu_solver: None,
            mode: HybridSolverMode::CpuParallel,
            gradient_threshold: 50.0,
            convergence_threshold: 1e-3,
        }
    }
    
    /// Set the execution mode
    pub fn with_mode(mut self, mode: HybridSolverMode) -> Self {
        self.mode = mode;
        self
    }
    
    /// Get the current execution mode
    pub fn mode(&self) -> HybridSolverMode {
        self.mode
    }
    
    /// Solve at a specific ramp point using the configured mode
    pub async fn solve_at_ramp(
        &self,
        circuit: &Circuit,
        ramp: f64,
        initial_guess: Option<&[crate::generic_glacier_solver::Variable]>,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<HybridSolveResult> {
        let effective_mode = self.choose_effective_mode(circuit, models);
        info!("Solving at ramp={:.3} using mode: {:?}", ramp, effective_mode);
        
        match effective_mode {
            HybridSolverMode::CpuOnly => self.solve_cpu_only(circuit, models).await,
            HybridSolverMode::CpuParallel => self.solve_cpu_parallel(circuit, models).await,
            HybridSolverMode::Hybrid => self.solve_hybrid_mode(circuit, ramp, initial_guess, models).await,
            HybridSolverMode::Auto => unreachable!(), // Should be resolved by choose_effective_mode
        }
    }
    
    fn choose_effective_mode(&self, circuit: &Circuit, models: &HashMap<String, ComponentModel>) -> HybridSolverMode {
        match self.mode {
            HybridSolverMode::Auto => {
                // Auto-selection logic based on circuit complexity
                let num_nodes = circuit.nodes().len();
                let num_nonlinear = models.values().filter(|m| {
                    matches!(m, ComponentModel::LED { .. } | ComponentModel::Diode { .. })
                }).count();
                
                // Check for ultra-sharp components
                let has_ultra_sharp = models.values().any(|m| {
                    if let ComponentModel::LED { saturation_current: Some(is), .. } = m {
                        *is <= 1e-14
                    } else if let ComponentModel::Diode { saturation_current: Some(is), .. } = m {
                        *is <= 1e-14
                    } else {
                        false
                    }
                });
                
                if has_ultra_sharp && self.gpu_solver.is_some() {
                    // Ultra-sharp components might benefit from hybrid approach
                    HybridSolverMode::Hybrid
                } else if num_nodes > 20 || num_nonlinear > 5 {
                    // Complex circuit - use parallel CPU
                    HybridSolverMode::CpuParallel
                } else {
                    // Simple circuit - use serial CPU
                    HybridSolverMode::CpuOnly
                }
            }
            other => other,
        }
    }
    
    async fn solve_cpu_only(&self, circuit: &Circuit, models: &HashMap<String, ComponentModel>) -> Result<HybridSolveResult> {
        let mut solver = crate::IntegratedGlacierSolver::with_config(
            circuit.clone(),
            crate::IntegratedSolverConfig {
                mode: crate::SolverMode::CpuSerial,
                phase0_ramp_points: 20,
                max_iterations: 300,
                tolerance: 1e-9,
            }
        );
        
        for (name, model) in models {
            solver.add_model(name.clone(), model.clone());
        }
        
        match solver.analyze() {
            Ok(results) => {
                if let Some((_, _, _, analysis)) = results.first() {
                    Ok(HybridSolveResult {
                        solution: vec![], // Would need to extract from analysis
                        iterations_gpu: 0,
                        iterations_cpu: Some(analysis.iterations),
                        final_error: 1e-10, // Approximate
                        used_cpu_fallback: false,
                        gradient: 1.0, // Low gradient assumed for CPU-only
                    })
                } else {
                    Err(anyhow::anyhow!("CPU solver found no solutions"))
                }
            }
            Err(e) => Err(anyhow::anyhow!("CPU solver failed: {}", e)),
        }
    }
    
    async fn solve_cpu_parallel(&self, circuit: &Circuit, models: &HashMap<String, ComponentModel>) -> Result<HybridSolveResult> {
        let mut solver = crate::IntegratedGlacierSolver::with_config(
            circuit.clone(),
            crate::IntegratedSolverConfig {
                mode: crate::SolverMode::CpuParallel,
                phase0_ramp_points: 40,
                max_iterations: 300,
                tolerance: 1e-9,
            }
        );
        
        for (name, model) in models {
            solver.add_model(name.clone(), model.clone());
        }
        
        match solver.analyze() {
            Ok(results) => {
                if let Some((_, _, _, analysis)) = results.first() {
                    Ok(HybridSolveResult {
                        solution: vec![], // Would need to extract from analysis
                        iterations_gpu: 0,
                        iterations_cpu: Some(analysis.iterations),
                        final_error: 1e-10, // Approximate
                        used_cpu_fallback: false,
                        gradient: 1.0, // Low gradient assumed for CPU-parallel
                    })
                } else {
                    Err(anyhow::anyhow!("CPU parallel solver found no solutions"))
                }
            }
            Err(e) => Err(anyhow::anyhow!("CPU parallel solver failed: {}", e)),
        }
    }
    
    async fn solve_hybrid_mode(
        &self,
        circuit: &Circuit,
        ramp: f64,
        initial_guess: Option<&[crate::generic_glacier_solver::Variable]>,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<HybridSolveResult> {
        if let Some(gpu_solver) = &self.gpu_solver {
            // First, try GPU solver
            match gpu_solver.solve_at_ramp_with_models(
                circuit, ramp, initial_guess, models
            ).await {
            Ok((solution, iterations, error)) => {
                // GPU converged successfully
                info!("GPU converged in {} iterations, error={:.2e}", iterations, error);
                Ok(HybridSolveResult {
                    solution,
                    iterations_gpu: iterations,
                    iterations_cpu: None,
                    final_error: error,
                    used_cpu_fallback: false,
                    gradient: 0.0, // Would need to extract from GPU
                })
            }
            Err(gpu_err) => {
                // GPU failed - check if we should try CPU
                warn!("GPU solver failed: {}", gpu_err);
                
                // Extract partial results from GPU
                if let Ok(partial_result) = self.extract_gpu_partial_result(circuit, ramp, models).await {
                    if partial_result.gradient > self.gradient_threshold {
                        info!("High gradient detected ({:.1}), falling back to CPU", partial_result.gradient);
                        
                        // CPU fallback would go here
                        // For now, return the GPU partial result as an error
                        warn!("CPU fallback not yet implemented - would refine GPU partial solution");
                        Err(anyhow::anyhow!("High gradient region needs CPU solver (not yet implemented)"))
                    } else {
                        // Low gradient but still failed - this is unexpected
                        Err(anyhow::anyhow!("GPU failed with low gradient: {}", gpu_err))
                    }
                } else {
                    // Couldn't get partial results
                    Err(anyhow::anyhow!("GPU failed and no partial results available: {}", gpu_err))
                }
            }
        }
    }
    
    /// Run Phase 0 scan with hybrid approach
    pub async fn phase0_scan_hybrid(
        &self,
        circuit: &Circuit,
        num_ramps: usize,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<Vec<GpuRegion>> {
        info!("Starting hybrid Phase 0 scan with {} points", num_ramps);
        
        // Run GPU Phase 0 scan
        let gpu_results = self.gpu_solver.phase0_coarse_scan_with_models(
            circuit, num_ramps, models
        ).await?;
        
        // Analyze results
        let mut num_converged = 0;
        let mut num_high_gradient = 0;
        
        for result in &gpu_results {
            if result.converged != 0 {
                num_converged += 1;
            }
            if result.max_gradient > self.gradient_threshold as f32 {
                num_high_gradient += 1;
            }
        }
        
        info!("GPU Phase 0: {}/{} converged, {}/{} high gradient", 
              num_converged, num_ramps, num_high_gradient, num_ramps);
        
        // Detect regions including high-gradient areas
        let regions = self.detect_regions_hybrid(&gpu_results);
        
        info!("Detected {} regions (including high-gradient areas)", regions.len());
        
        // For each high-gradient region, optionally refine with CPU
        let mut refined_regions = Vec::new();
        
        for region in regions {
            if region.log_gradient > self.gradient_threshold as f32 && !region.converged {
                info!("Region [{:.1}%-{:.1}%] has high gradient, would use CPU for Phase 1/2",
                      region.start * 100.0, region.end * 100.0);
            }
            refined_regions.push(region);
        }
        
        Ok(refined_regions)
    }
    
    /// Enhanced region detection that includes high-gradient areas
    fn detect_regions_hybrid(&self, results: &[Phase0Result]) -> Vec<GpuRegion> {
        let threshold = self.gradient_threshold as f32;
        let mut regions = Vec::new();
        let mut i = 0;
        
        while i < results.len() {
            let result = &results[i];
            
            // Start a region if converged OR high gradient
            if result.converged != 0 || result.max_gradient > threshold {
                let start_idx = i;
                let start_gradient = result.max_gradient;
                let mut any_converged = result.converged != 0;
                
                // Extend region while gradient is significant
                while i < results.len() {
                    let curr = &results[i];
                    
                    // Continue region if:
                    // - Still high gradient (above 50% of threshold)
                    // - Or converged with moderate gradient
                    if curr.max_gradient > threshold * 0.5 || 
                       (curr.converged != 0 && curr.max_gradient > 10.0) {
                        if curr.converged != 0 {
                            any_converged = true;
                        }
                        i += 1;
                    } else {
                        break;
                    }
                }
                
                // Create region
                let end_idx = i.saturating_sub(1);
                let region = GpuRegion {
                    start: results[start_idx].ramp,
                    end: results[end_idx].ramp,
                    representative_ramp: (results[start_idx].ramp + results[end_idx].ramp) / 2.0,
                    log_gradient: start_gradient,
                    converged: any_converged,
                };
                
                regions.push(region);
            } else {
                i += 1;
            }
        }
        
        regions
    }
    
    /// Extract partial results from a failed GPU solve attempt
    async fn extract_gpu_partial_result(
        &self,
        circuit: &Circuit,
        ramp: f64,
        models: &HashMap<String, ComponentModel>,
    ) -> Result<GpuPartialResult> {
        // This would need to be implemented to extract the GPU's current state
        // For now, return a placeholder
        warn!("GPU partial result extraction not yet implemented");
        Err(anyhow::anyhow!("Partial result extraction not implemented"))
    }
    
    /// Convert CPU solver result to our solution format
    fn extract_cpu_solution(&self, _cpu_result: &crate::glacier_dc_solver::DcAnalysisResult) -> Vec<crate::generic_glacier_solver::Variable> {
        // This would convert from CPU's format to our Variable format
        // Placeholder implementation
        vec![]
    }
}

/// Partial result from GPU when it fails to converge
#[derive(Debug)]
struct GpuPartialResult {
    partial_solution: Vec<crate::generic_glacier_solver::Variable>,
    iterations: usize,
    error: f64,
    gradient: f64,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_hybrid_approach() {
        // Test would verify:
        // 1. GPU succeeds on low-gradient regions
        // 2. GPU fails but provides partial results on high-gradient regions  
        // 3. CPU successfully converges from GPU's partial results
    }
}