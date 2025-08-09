//! Corrected MAESTRO integration for GLACIER transient solver
//! 
//! This shows how to properly integrate MAESTRO selection without re-running the solver

use crate::{
    SpiceError, Result, AnalysisResult,
    maestro_production::{MaestroOrchestrator, CircuitPattern},
};

impl GlacierSolver {
    /// Get DC operating point using MAESTRO for intelligent selection
    /// This is the CORRECTED implementation that doesn't re-run the solver
    fn get_dc_with_maestro_correct(&mut self) -> Result<AnalysisResult> {
        // First get all solutions with current GLACIER
        let glacier_solutions = self.analyze()?;
        
        if glacier_solutions.is_empty() {
            return Err(SpiceError::ConvergenceFailed(0));
        }
        
        // If only one solution, use it
        if glacier_solutions.len() == 1 {
            info!("Only one DC solution found, using it directly");
            return Ok(glacier_solutions.into_iter().next().unwrap().3);
        }
        
        info!("Multiple DC solutions found ({}), using MAESTRO logic for selection", glacier_solutions.len());
        
        // Use MAESTRO's selection logic WITHOUT re-running the solver
        let selected = self.maestro_select_from_solutions(glacier_solutions)?;
        info!("MAESTRO selected solution with power={:.3}W", selected.total_power);
        
        Ok(selected)
    }
    
    /// Use MAESTRO's intelligent selection logic on already-found solutions
    fn maestro_select_from_solutions(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // Analyze circuit topology
        let mut maestro = MaestroOrchestrator::new(self.circuit.clone());
        for (name, model) in &self.models {
            maestro.add_model(name.clone(), model.clone());
        }
        
        let patterns = maestro.analyze_topology();
        info!("MAESTRO detected patterns: {:?}", patterns);
        
        // Selection logic based on circuit patterns
        match patterns.first() {
            Some(CircuitPattern::SeriesNonlinear { components, .. }) => {
                // For series nonlinear (like series LEDs), prefer moderate current
                info!("Series nonlinear circuit detected, selecting moderate current solution");
                self.select_moderate_current_solution(solutions)
            }
            Some(CircuitPattern::ParallelArray { identical, .. }) => {
                // For parallel arrays, prefer balanced current distribution
                info!("Parallel array detected (identical={}), selecting balanced solution", identical);
                self.select_balanced_solution(solutions)
            }
            Some(CircuitPattern::PowerConverter { .. }) => {
                // For power converters, prefer nominal operating point
                info!("Power converter detected, selecting nominal operating point");
                self.select_nominal_power_solution(solutions)
            }
            _ => {
                // Default: select solution with moderate power
                info!("Mixed/unknown circuit, using moderate power selection");
                self.select_moderate_power_solution(solutions)
            }
        }
    }
    
    /// Select solution with moderate current (good for series nonlinear)
    fn select_moderate_current_solution(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // Find solution with current closest to typical LED current (20mA)
        let target_current = 0.020; // 20mA typical
        
        let best = solutions.into_iter()
            .min_by_key(|(_, _, _, result)| {
                // Find max current in any branch
                let max_current = result.branch_currents.values()
                    .map(|&i| i.abs())
                    .fold(0.0f64, f64::max);
                
                // Distance from target
                ((max_current - target_current).abs() * 1e6) as i64
            })
            .map(|(_, _, _, result)| result);
        
        best.ok_or(SpiceError::ConvergenceFailed(0))
    }
    
    /// Select solution with balanced current distribution
    fn select_balanced_solution(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // Find solution with lowest current variance
        let best = solutions.into_iter()
            .min_by_key(|(_, _, _, result)| {
                // Calculate variance of branch currents
                let currents: Vec<f64> = result.branch_currents.values()
                    .map(|&i| i.abs())
                    .filter(|&i| i > 1e-9) // Ignore tiny currents
                    .collect();
                
                if currents.is_empty() {
                    return i64::MAX;
                }
                
                let mean = currents.iter().sum::<f64>() / currents.len() as f64;
                let variance = currents.iter()
                    .map(|&i| (i - mean).powi(2))
                    .sum::<f64>() / currents.len() as f64;
                
                (variance * 1e12) as i64
            })
            .map(|(_, _, _, result)| result);
        
        best.ok_or(SpiceError::ConvergenceFailed(0))
    }
    
    /// Select solution at nominal power (typically 50-70% of max)
    fn select_nominal_power_solution(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // Sort by power
        let mut sorted = solutions;
        sorted.sort_by(|a, b| a.3.total_power.partial_cmp(&b.3.total_power).unwrap());
        
        // Select solution at 60% percentile (nominal operation)
        let nominal_idx = (sorted.len() as f64 * 0.6) as usize;
        Ok(sorted.into_iter().nth(nominal_idx).unwrap().3)
    }
}

// Example of how the transient analysis should be modified:
/*
pub fn analyze_transient(
    &mut self, 
    t_stop: f64, 
    t_step: f64,
    initial_conditions: Option<AnalysisResult>
) -> Result<TransientResult> {
    // ... existing code ...
    
    let dc_solution = match initial_conditions {
        Some(ic) => ic,
        None => {
            info!("Computing DC operating point for initial conditions");
            // Use the CORRECTED method that doesn't re-run the solver
            self.get_dc_with_maestro_correct()?
        }
    };
    
    // ... rest of transient analysis ...
}
*/