//! MAESTRO: Multi-strategy Adaptive Engine for Smart Topology-driven Resolution and Orchestration
//! 
//! MAESTRO acts as an intelligent orchestrator that:
//! 1. First tries GLACIER which returns multiple solutions from different regions
//! 2. Selects the physically meaningful solution based on circuit knowledge
//! 3. Falls back to topology-aware strategies if GLACIER fails completely

use crate::{
    Circuit, AnalysisResult, ComponentModel, GlacierSolver,
    topology::{TopologyAnalyzer, CircuitPattern},
    strategies::{ProgressiveActivation, SymmetryExploitation, CurrentSharing, HierarchicalDecomposition},
};
use nalgebra::DVector;
use std::collections::HashMap;
use anyhow::{Result, anyhow};

/// MAESTRO orchestrator that coordinates between GLACIER and specialized strategies
pub struct MaestroOrchestrator {
    circuit: Circuit,
    models: HashMap<String, ComponentModel>,
    topology_analyzer: TopologyAnalyzer,
}

impl MaestroOrchestrator {
    pub fn new(circuit: Circuit) -> Self {
        let topology_analyzer = TopologyAnalyzer::new(&circuit);
        Self {
            circuit,
            models: HashMap::new(),
            topology_analyzer,
        }
    }
    
    pub fn add_model(&mut self, component_name: String, model: ComponentModel) {
        self.models.insert(component_name, model);
    }
    
    /// Main solving entry point
    pub fn solve(&mut self) -> Result<AnalysisResult> {
        println!("MAESTRO: Starting circuit analysis...");
        
        // Step 1: Try GLACIER first - it returns multiple solutions
        match self.try_glacier() {
            Ok(result) => {
                println!("MAESTRO: GLACIER succeeded, selected physically meaningful solution");
                return Ok(result);
            }
            Err(e) => {
                println!("MAESTRO: GLACIER failed: {}, trying topology-aware strategies", e);
            }
        }
        
        // Step 2: Analyze circuit topology
        let patterns = self.topology_analyzer.detect_patterns();
        println!("MAESTRO: Detected {} circuit patterns", patterns.len());
        
        // Step 3: Try topology-specific strategies
        for pattern in patterns {
            match self.apply_strategy(&pattern) {
                Ok(result) => {
                    println!("MAESTRO: Strategy {:?} succeeded", pattern);
                    return Ok(result);
                }
                Err(e) => {
                    println!("MAESTRO: Strategy {:?} failed: {}", pattern, e);
                }
            }
        }
        
        // Step 4: Try combined approach - use GLACIER with topology-informed starting points
        self.try_combined_approach()
    }
    
    /// Try GLACIER and select the best solution from multiple regions
    fn try_glacier(&self) -> Result<AnalysisResult> {
        let mut glacier = GlacierSolver::new(self.circuit.clone());
        
        // Add all models
        for (name, model) in &self.models {
            glacier.add_model(name.clone(), model.clone());
        }
        
        // GLACIER now returns multiple solutions from different regions
        let solutions = glacier.analyze()?;
        
        if solutions.is_empty() {
            return Err(anyhow!("GLACIER returned no solutions"));
        }
        
        println!("MAESTRO: GLACIER returned {} solutions from different regions", solutions.len());
        
        // Select the physically meaningful solution
        self.select_best_solution(solutions)
    }
    
    /// Select the most physically meaningful solution from GLACIER's multiple results
    fn select_best_solution(&self, solutions: Vec<(f64, f64, f64, AnalysisResult)>) -> Result<AnalysisResult> {
        // For each solution, evaluate physical meaningfulness
        let mut best_solution = None;
        let mut best_score = f64::NEG_INFINITY;
        
        for (start_ramp, end_ramp, gradient, result) in solutions {
            let score = self.evaluate_solution_quality(&result, start_ramp, end_ramp, gradient);
            
            println!("  Solution from region {:.1}%-{:.1}%: score = {:.3}", 
                     start_ramp * 100.0, end_ramp * 100.0, score);
            
            if score > best_score {
                best_score = score;
                best_solution = Some(result);
            }
        }
        
        best_solution.ok_or_else(|| anyhow!("No physically meaningful solution found"))
    }
    
    /// Evaluate the quality/physical meaningfulness of a solution
    fn evaluate_solution_quality(&self, result: &AnalysisResult, start_ramp: f64, end_ramp: f64, gradient: f64) -> f64 {
        let mut score = 0.0;
        
        // 1. Prefer solutions from higher ramp regions (components more likely to be "on")
        score += (start_ramp + end_ramp) / 2.0 * 10.0;
        
        // 2. Prefer stable regions (lower gradient)
        if gradient < 100.0 {
            score += 5.0;
        }
        
        // 3. Check physical constraints
        for (edge_idx, current) in &result.branch_currents {
            // Get branch name from circuit
            if let Some((_, branch)) = self.circuit.branches().find(|(idx, _)| idx == edge_idx) {
                if let Some(model) = self.models.get(&branch.name) {
                match model {
                    ComponentModel::LED { forward_current, .. } => {
                        // Prefer solutions where LEDs are conducting reasonably
                        if current.abs() > 0.1e-3 && current.abs() < forward_current * 2.0 {
                            score += 2.0;
                        }
                    }
                    ComponentModel::Resistor { .. } => {
                        // Check power dissipation is reasonable
                        if let Some(voltage) = self.get_branch_voltage(result, &branch.name) {
                            let power = voltage * current;
                            if power.abs() < 0.5 { // Less than 0.5W
                                score += 1.0;
                            }
                        }
                    }
                    _ => {}
                }
            }
            }
        }
        
        // 4. Prefer solutions with reasonable node voltages
        for voltage in result.node_voltages.values() {
            if voltage.abs() < 100.0 { // Reasonable voltage range
                score += 0.1;
            }
        }
        
        score
    }
    
    /// Apply a specific topology-aware strategy
    fn apply_strategy(&self, pattern: &CircuitPattern) -> Result<AnalysisResult> {
        match pattern {
            CircuitPattern::SeriesNonlinear { components, count } => {
                println!("MAESTRO: Applying Progressive Activation for {} series components", count);
                let mut strategy = ProgressiveActivation::new(self.circuit.clone());
                for (name, model) in &self.models {
                    strategy.add_model(name.clone(), model.clone());
                }
                strategy.solve(components)
            }
            
            CircuitPattern::ParallelArray { components, matched } => {
                println!("MAESTRO: Applying Current Sharing for {} parallel components (matched: {})", 
                         components.len(), matched);
                let mut strategy = CurrentSharing::new(self.circuit.clone());
                for (name, model) in &self.models {
                    strategy.add_model(name.clone(), model.clone());
                }
                strategy.solve(components)
            }
            
            CircuitPattern::Symmetric { groups } => {
                println!("MAESTRO: Applying Symmetry Exploitation for {} symmetric groups", groups.len());
                let mut strategy = SymmetryExploitation::new(self.circuit.clone());
                for (name, model) in &self.models {
                    strategy.add_model(name.clone(), model.clone());
                }
                strategy.solve(groups)
            }
            
            CircuitPattern::Hierarchical { blocks } => {
                println!("MAESTRO: Applying Hierarchical Decomposition for {} blocks", blocks.len());
                let mut strategy = HierarchicalDecomposition::new(self.circuit.clone());
                for (name, model) in &self.models {
                    strategy.add_model(name.clone(), model.clone());
                }
                strategy.solve(blocks)
            }
        }
    }
    
    /// Combined approach: Use topology knowledge to provide starting points to GLACIER
    fn try_combined_approach(&self) -> Result<AnalysisResult> {
        println!("MAESTRO: Trying combined approach with topology-informed starting points");
        
        // Get patterns
        let patterns = self.topology_analyzer.detect_patterns();
        
        // For each pattern, generate a smart initial guess
        for pattern in patterns {
            if let Some(voltage_hint) = self.generate_pattern_based_guess(&pattern) {
                // Try GLACIER with pattern-specific guidance
                let mut glacier = GlacierSolver::new(self.circuit.clone());
                for (name, model) in &self.models {
                    glacier.add_model(name.clone(), model.clone());
                }
                
                println!("MAESTRO: Trying GLACIER with voltage hint {:.1}V for pattern {:?}", voltage_hint, pattern);
                
                // Use analyze_with_guidance to provide circuit-specific hint
                match glacier.analyze_with_guidance(1.0, Some(voltage_hint)) {
                    Ok(result) => {
                        println!("MAESTRO: Combined approach succeeded with pattern {:?}", pattern);
                        return Ok(result);
                    }
                    Err(e) => {
                        println!("MAESTRO: Combined approach failed for pattern {:?}: {}", pattern, e);
                    }
                }
            }
        }
        
        Err(anyhow!("All solving strategies failed"))
    }
    
    /// Generate an intelligent initial guess based on circuit pattern
    fn generate_pattern_based_guess(&self, pattern: &CircuitPattern) -> Option<f64> {
        match pattern {
            CircuitPattern::SeriesNonlinear { components, .. } => {
                // For series nonlinear components (e.g., LEDs), provide voltage hint
                println!("MAESTRO: Generating guess for series nonlinear circuit");
                
                // Count nonlinear components
                let nonlinear_count = components.iter()
                    .filter(|name| {
                        self.models.get(*name)
                            .map(|m| matches!(m, ComponentModel::LED { .. } | ComponentModel::Diode { .. }))
                            .unwrap_or(false)
                    })
                    .count();
                
                if nonlinear_count > 0 {
                    // For LEDs/diodes, suggest ~2V per device as initial guess
                    Some(2.0)
                } else {
                    None
                }
            }
            CircuitPattern::ParallelArray { .. } => {
                // For parallel arrays, voltage is same across all branches
                println!("MAESTRO: Generating guess for parallel array");
                Some(1.0) // Start with moderate voltage
            }
            _ => None,
        }
    }
    
    /// Helper to get branch voltage from node voltages
    fn get_branch_voltage(&self, result: &AnalysisResult, branch_name: &str) -> Option<f64> {
        // This would look up the branch endpoints and compute voltage difference
        // Placeholder for now
        None
    }
}

/// Public API for MAESTRO
pub fn solve_with_maestro(circuit: Circuit, models: HashMap<String, ComponentModel>) -> Result<AnalysisResult> {
    let mut maestro = MaestroOrchestrator::new(circuit);
    
    for (name, model) in models {
        maestro.add_model(name, model);
    }
    
    maestro.solve()
}