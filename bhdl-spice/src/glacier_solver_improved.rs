//! Improved GLACIER solver with MAESTRO integration for DC operating point selection

use crate::{
    Circuit, ComponentModel, SpiceError, Result, AnalysisResult,
    TransientResult, maestro_production::{MaestroOrchestrator, CircuitPattern},
    glacier_production::{GlacierSolver as ProductionGlacierSolver, Solution as GlacierSolution},
};
use std::collections::HashMap;

/// Improved GLACIER solver that uses MAESTRO for intelligent DC selection
pub struct ImprovedGlacierSolver {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    use_maestro: bool,
}

impl ImprovedGlacierSolver {
    /// Create a new improved solver
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
            use_maestro: true,
        }
    }
    
    /// Add a component model
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Enable/disable MAESTRO integration (default: enabled)
    pub fn set_use_maestro(&mut self, use_maestro: bool) {
        self.use_maestro = use_maestro;
    }
    
    /// Find DC operating point using MAESTRO for intelligent selection
    pub fn find_dc_operating_point(&self) -> Result<AnalysisResult> {
        // Use production GLACIER to find all solutions
        let mut glacier = ProductionGlacierSolver::new(self.circuit.clone());
        for (name, model) in &self.models {
            glacier.add_model(name.clone(), model.clone());
        }
        
        let all_solutions = glacier.solve()?;
        
        if all_solutions.is_empty() {
            return Err(SpiceError::ConvergenceFailed(0));
        }
        
        // Select best solution
        let selected_solution = if self.use_maestro && all_solutions.len() > 1 {
            // Use MAESTRO for intelligent selection
            let maestro = MaestroOrchestrator::new(self.circuit.clone(), self.models.clone());
            let pattern = maestro.detect_circuit_pattern()?;
            
            println!("MAESTRO: Detected circuit pattern: {:?}", pattern);
            println!("MAESTRO: Selecting from {} solutions", all_solutions.len());
            
            maestro.select_best_solution(all_solutions, pattern)?
        } else if all_solutions.len() == 1 {
            // Only one solution, use it
            all_solutions.into_iter().next().unwrap()
        } else {
            // Fallback to max power (old behavior) if MAESTRO disabled
            println!("Warning: MAESTRO disabled, using max power selection");
            all_solutions.into_iter()
                .max_by(|a, b| a.total_power.partial_cmp(&b.total_power).unwrap())
                .unwrap()
        };
        
        // Convert to index-based format
        self.convert_solution_to_indices(&selected_solution)
    }
    
    /// Run transient analysis with MAESTRO-selected DC operating point
    pub fn analyze_transient(
        &self,
        t_stop: f64,
        t_step: f64,
        initial_conditions: Option<AnalysisResult>,
    ) -> Result<TransientResult> {
        // Get DC operating point if not provided
        let dc_point = match initial_conditions {
            Some(ic) => ic,
            None => {
                println!("Finding DC operating point with MAESTRO...");
                self.find_dc_operating_point()?
            }
        };
        
        // Create base GLACIER solver for transient
        let mut base_solver = crate::glacier_solver::GlacierSolver::new(self.circuit.clone());
        for (name, model) in &self.models {
            base_solver.add_model(name.clone(), model.clone());
        }
        
        // Run transient with MAESTRO-selected DC point
        base_solver.analyze_transient(t_stop, t_step, Some(dc_point))
    }
    
    /// Convert string-based solution to index-based for transient solver
    fn convert_solution_to_indices(&self, solution: &GlacierSolution) -> Result<AnalysisResult> {
        let mut node_voltages = HashMap::new();
        let mut branch_currents = HashMap::new();
        
        // Convert node voltages
        for (node_name, voltage) in &solution.node_voltages {
            if let Some((idx, _)) = self.circuit.get_node(node_name) {
                node_voltages.insert(idx, *voltage);
            }
        }
        
        // Convert branch currents
        for (branch_name, current) in &solution.branch_currents {
            if let Some((idx, _)) = self.circuit.get_branch(branch_name) {
                branch_currents.insert(idx, *current);
            }
        }
        
        Ok(AnalysisResult {
            node_voltages,
            branch_currents,
            total_power: solution.total_power,
            iterations: solution.iterations,
        })
    }
}

/// Helper trait to add MAESTRO support to existing GLACIER solver
pub trait MaestroEnabled {
    /// Run transient analysis with MAESTRO DC selection
    fn analyze_transient_with_maestro(
        &self,
        t_stop: f64,
        t_step: f64,
        use_maestro: bool,
    ) -> Result<TransientResult>;
}

impl MaestroEnabled for crate::glacier_solver::GlacierSolver {
    fn analyze_transient_with_maestro(
        &self,
        t_stop: f64,
        t_step: f64,
        use_maestro: bool,
    ) -> Result<TransientResult> {
        if use_maestro {
            // Find all DC solutions
            let all_solutions = self.analyze()?;
            
            if all_solutions.is_empty() {
                return Err(SpiceError::ConvergenceFailed(0));
            }
            
            // Convert to MAESTRO format and select best
            let mut improved = ImprovedGlacierSolver::new(self.circuit.clone());
            for (name, model) in &self.models {
                improved.add_model(name.clone(), model.clone());
            }
            
            let dc_point = improved.find_dc_operating_point()?;
            
            // Run transient with selected DC point
            self.analyze_transient(t_stop, t_step, Some(dc_point))
        } else {
            // Use original behavior
            self.analyze_transient(t_stop, t_step, None)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_maestro_dc_selection() {
        // Create test circuit
        let mut circuit = Circuit::new();
        circuit.add_node("VDD".to_string(), None);
        circuit.add_node("N1".to_string(), None);
        circuit.add_node("GND".to_string(), None);
        
        circuit.add_branch("V1".to_string(), "VDD", "GND", "VoltageSource".to_string(), 5.0, None);
        circuit.add_branch("R1".to_string(), "VDD", "N1", "Resistor".to_string(), 220.0, None);
        circuit.add_branch("D1".to_string(), "N1", "GND", "LED".to_string(), 0.0, None);
        
        // Create solver
        let mut solver = ImprovedGlacierSolver::new(circuit);
        
        // Add models
        solver.add_model("V1".to_string(), ComponentModel::VoltageSource { voltage: 5.0 });
        solver.add_model("R1".to_string(), ComponentModel::Resistor { 
            resistance: 220.0, 
            limits: Default::default() 
        });
        solver.add_model("D1".to_string(), ComponentModel::LED {
            color: "red".to_string(),
            forward_voltage: 2.0,
            forward_current: 0.020,
            dynamic_resistance: 10.0,
            saturation_current: Some(1e-15),
            emission_coefficient: Some(1.8),
            thermal_voltage: Some(0.026),
            limits: Default::default(),
        });
        
        // Test DC operating point selection
        let dc_point = solver.find_dc_operating_point();
        assert!(dc_point.is_ok());
        
        let result = dc_point.unwrap();
        assert!(result.total_power > 0.0);
        assert!(result.total_power < 0.5); // Reasonable power
    }
}