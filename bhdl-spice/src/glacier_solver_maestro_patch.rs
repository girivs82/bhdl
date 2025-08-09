//! Patch to integrate MAESTRO into GLACIER transient solver
//! 
//! This shows the exact changes needed to glacier_solver.rs to use MAESTRO
//! for DC operating point selection instead of the max power heuristic.

// Add these imports at the top of glacier_solver.rs:
use crate::{
    solve_with_glacier_maestro,
    ProductionGlacierSolver,
    GlacierSolution as ProductionSolution,
};

// Replace the analyze_transient method's DC selection section (lines 2910-2922):
impl GlacierSolver {
    pub fn analyze_transient(
        &self,
        t_stop: f64, 
        t_step: f64,
        initial_conditions: Option<AnalysisResult>
    ) -> Result<TransientResult> {
        use crate::transient_models::{
            CapacitorCompanion, InductorCompanion, NonlinearCompanion, TransientSource
        };
        
        info!("Starting GLACIER transient analysis: t_stop={}, t_step={}", t_stop, t_step);
        
        // First, get DC operating point if not provided
        let dc_solution = match initial_conditions {
            Some(ic) => ic,
            None => {
                info!("Computing DC operating point using MAESTRO selection");
                
                // Check if we should use MAESTRO (could be a config option)
                let use_maestro = true; // Or read from config
                
                if use_maestro {
                    // Use MAESTRO for intelligent DC selection
                    self.get_dc_with_maestro()?
                } else {
                    // Fallback to old behavior (not recommended)
                    self.analyze()
                        .and_then(|solutions| solutions.into_iter()
                            .max_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap())
                            .map(|s| s.3)
                            .ok_or(SpiceError::ConvergenceFailed(0)))?
                }
            }
        };
        
        // ... rest of the transient analysis code remains the same
    }
    
    /// Get DC operating point using MAESTRO for intelligent selection
    fn get_dc_with_maestro(&self) -> Result<AnalysisResult> {
        // First try to get solutions with current GLACIER
        let glacier_solutions = self.analyze()?;
        
        if glacier_solutions.is_empty() {
            return Err(SpiceError::ConvergenceFailed(0));
        }
        
        // If only one solution, use it
        if glacier_solutions.len() == 1 {
            return Ok(glacier_solutions.into_iter().next().unwrap().3);
        }
        
        info!("Multiple DC solutions found, using MAESTRO for selection");
        
        // Use MAESTRO to select the best solution
        let maestro_solutions = solve_with_glacier_maestro(
            self.circuit.clone(), 
            self.models.clone()
        )?;
        
        if let Some(best_solution) = maestro_solutions.first() {
            // Convert from production format back to our format
            self.convert_production_solution(best_solution)
        } else {
            // Fallback to first solution if MAESTRO fails
            warn!("MAESTRO selection failed, using first solution");
            Ok(glacier_solutions.into_iter().next().unwrap().3)
        }
    }
    
    /// Convert production GLACIER solution to our AnalysisResult format
    fn convert_production_solution(&self, solution: &ProductionSolution) -> Result<AnalysisResult> {
        let mut node_voltages = HashMap::new();
        let mut branch_currents = HashMap::new();
        let mut total_power = 0.0;
        
        // Convert node voltages from name-based to index-based
        for (node_name, voltage) in &solution.node_voltages {
            if let Some((idx, _)) = self.circuit.get_node(node_name) {
                node_voltages.insert(idx, *voltage);
            }
        }
        
        // Convert branch currents and calculate total power
        for (branch_name, current) in &solution.branch_currents {
            if let Some((idx, _)) = self.circuit.get_branch(branch_name) {
                branch_currents.insert(idx, *current);
                
                // Calculate power contribution
                if let Some((n1, n2)) = self.circuit.branch_nodes(idx) {
                    let v1 = node_voltages.get(&n1).copied().unwrap_or(0.0);
                    let v2 = node_voltages.get(&n2).copied().unwrap_or(0.0);
                    total_power += (v1 - v2).abs() * current.abs();
                }
            }
        }
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power,
            iterations: solution.iterations,
        })
    }
}

// Alternative: Add a configuration option to GlacierSolver
pub struct GlacierSolverConfig {
    pub use_maestro_dc_selection: bool,
    pub max_iterations: usize,
    pub tolerance: f64,
}

impl Default for GlacierSolverConfig {
    fn default() -> Self {
        Self {
            use_maestro_dc_selection: true,  // Default to MAESTRO
            max_iterations: 100,
            tolerance: 1e-9,
        }
    }
}

// Then modify GlacierSolver to use config:
pub struct GlacierSolver {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    config: GlacierSolverConfig,
    // ... other fields
}

impl GlacierSolver {
    pub fn new_with_config(circuit: Circuit, config: GlacierSolverConfig) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
            config,
            // ... initialize other fields
        }
    }
}