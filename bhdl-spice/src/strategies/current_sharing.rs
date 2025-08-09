//! Current sharing strategy for parallel arrays

use crate::{Circuit, AnalysisResult, ComponentModel, GlacierSolver};
use std::collections::HashMap;
use anyhow::{Result, anyhow};

pub struct CurrentSharing {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
}

impl CurrentSharing {
    pub fn new(circuit: Circuit) -> Self {
        Self {
            circuit,
            models: HashMap::new(),
        }
    }
    
    pub fn add_model(&mut self, component_name: String, model: ComponentModel) {
        self.models.insert(component_name, model);
    }
    
    pub fn solve(&self, components: &[String]) -> Result<AnalysisResult> {
        println!("Current Sharing: Solving {} parallel components", components.len());
        
        // For parallel arrays, we can leverage the fact that voltage is the same
        // Sort components by their parameters (e.g., LED Is values)
        // Activate strongest first, then add weaker ones
        
        // For now, just use GLACIER directly
        let mut glacier = GlacierSolver::new(self.circuit.clone());
        for (name, model) in &self.models {
            glacier.add_model(name.clone(), model.clone());
        }
        
        let solutions = glacier.analyze()?;
        if solutions.is_empty() {
            return Err(anyhow!("Current sharing strategy failed"));
        }
        
        // Select solution with best current distribution
        Ok(solutions.into_iter().next().unwrap().3)
    }
}