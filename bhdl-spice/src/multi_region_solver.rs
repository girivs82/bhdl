//! Multi-region solver that finds solutions in different operating regions

use crate::{GlacierSolver, AnalysisResult, Result, SpiceError};
use crate::ComponentModel;

pub struct RegionSolution {
    pub ramp_start: f64,
    pub ramp_end: f64,
    pub average_gradient: f64,
    pub result: AnalysisResult,
}

pub struct MultiRegionSolver {
    solver: GlacierSolver,
}

impl MultiRegionSolver {
    pub fn new(solver: GlacierSolver) -> Self {
        Self { solver }
    }
    
    /// Analyze circuit and find solutions in all stable regions
    pub fn analyze_all_regions(&mut self) -> Result<Vec<RegionSolution>> {
        // Use the solver's multi-region analysis
        match self.solver.analyze_all_regions() {
            Ok(solutions) => {
                Ok(solutions.into_iter().map(|(start, end, gradient, result)| {
                    RegionSolution {
                        ramp_start: start,
                        ramp_end: end,
                        average_gradient: gradient,
                        result,
                    }
                }).collect())
            }
            Err(e) => Err(e)
        }
    }
    
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Circuit;
    
    #[test]
    fn test_multi_region_detection() {
        // Test that we can identify multiple regions
        let circuit = Circuit::new();
        let solver = GlacierSolver::new(circuit);
        let mut multi_solver = MultiRegionSolver::new(solver);
        
        // This would test region detection
        // Placeholder for now
    }
}