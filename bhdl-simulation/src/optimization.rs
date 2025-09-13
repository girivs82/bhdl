// Optimization Algorithms for Simulation-Driven Synthesis

use std::collections::HashMap;
use std::time::Instant;
use crate::engine::{SimulationEngine, ModelMetadata, DesignParameters, SimulationResult, OptimizationResult};
use crate::engine::{Result, SimulationError};
use rayon::prelude::*;

/// Optimization objective
#[derive(Debug, Clone)]
pub struct Objective {
    pub metric: String,
    pub goal: OptimizationGoal,
    pub target_value: Option<f64>,
    pub weight: f64,
}

#[derive(Debug, Clone)]
pub enum OptimizationGoal {
    Minimize,
    Maximize,
    Target(f64),
}

/// Optimization constraint
#[derive(Debug, Clone)]
pub struct Constraint {
    pub metric: String,
    pub condition: ConstraintCondition,
    pub value: f64,
    pub hard: bool, // Hard constraint vs soft preference
}

#[derive(Debug, Clone)]
pub enum ConstraintCondition {
    GreaterThan,
    LessThan,
    Equal,
    InRange(f64, f64),
}

/// Optimization configuration
#[derive(Debug, Clone)]
pub struct OptimizationConfig {
    pub max_iterations: usize,
    pub convergence_tolerance: f64,
    pub parallel: bool,
    pub early_termination: bool,
}

impl Default for OptimizationConfig {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            convergence_tolerance: 1e-6,
            parallel: true,
            early_termination: true,
        }
    }
}

/// Grid Search Optimizer
pub struct GridSearchOptimizer {
    engine: SimulationEngine,
    config: OptimizationConfig,
}

impl GridSearchOptimizer {
    pub fn new(engine: SimulationEngine, config: OptimizationConfig) -> Self {
        Self { engine, config }
    }
    
    /// Run grid search optimization
    pub fn optimize(
        &mut self,
        model: &ModelMetadata,
        parameter_ranges: HashMap<String, Vec<f64>>,
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Result<OptimizationResult> {
        let start_time = Instant::now();
        
        // Generate all parameter combinations
        let combinations = self.generate_combinations(&parameter_ranges);
        let num_combinations = combinations.len();
        
        println!("Grid search: evaluating {} combinations", num_combinations);
        
        // Evaluate all combinations
        let results = if self.config.parallel {
            self.evaluate_parallel(model, &combinations, objectives, constraints)?
        } else {
            self.evaluate_sequential(model, &combinations, objectives, constraints)?
        };
        
        // Find best design
        let best = results
            .into_iter()
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
            .ok_or_else(|| SimulationError::OptimizationFailed("No valid designs found".to_string()))?;
        
        Ok(OptimizationResult {
            final_design: best.0,
            best_score: best.1,
            iterations: num_combinations,
            total_runtime: start_time.elapsed(),
            convergence_reason: "Grid search complete".to_string(),
        })
    }
    
    /// Generate all parameter combinations
    fn generate_combinations(&self, ranges: &HashMap<String, Vec<f64>>) -> Vec<DesignParameters> {
        let mut combinations = vec![DesignParameters::new()];
        
        for (param_name, values) in ranges {
            let mut new_combinations = Vec::new();
            
            for value in values {
                for combo in &combinations {
                    let mut new_combo = combo.clone();
                    new_combo.set(param_name, *value);
                    new_combinations.push(new_combo);
                }
            }
            
            combinations = new_combinations;
        }
        
        combinations
    }
    
    /// Evaluate combinations in parallel
    fn evaluate_parallel(
        &mut self,
        model: &ModelMetadata,
        combinations: &[DesignParameters],
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Result<Vec<(DesignParameters, f64)>> {
        // Note: In real implementation, we'd need thread-safe engine
        // For now, we'll simulate sequential evaluation
        self.evaluate_sequential(model, combinations, objectives, constraints)
    }
    
    /// Evaluate combinations sequentially
    fn evaluate_sequential(
        &mut self,
        model: &ModelMetadata,
        combinations: &[DesignParameters],
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Result<Vec<(DesignParameters, f64)>> {
        let mut results = Vec::new();
        
        for params in combinations {
            if let Ok(score) = self.evaluate_design(model, params, objectives, constraints) {
                results.push((params.clone(), score));
                
                // Early termination if perfect score found
                if self.config.early_termination && score >= 0.99 {
                    break;
                }
            }
        }
        
        Ok(results)
    }
    
    /// Evaluate a single design
    fn evaluate_design(
        &mut self,
        model: &ModelMetadata,
        parameters: &DesignParameters,
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Result<f64> {
        // Run simulation
        let sim_result = self.engine.simulate(model, parameters)?;
        
        // Check constraints
        let constraint_penalty = self.calculate_constraint_penalty(&sim_result, constraints);
        
        // Calculate objective score
        let objective_score = self.calculate_objective_score(&sim_result, objectives);
        
        // Final score with constraint penalty
        Ok((objective_score - constraint_penalty).max(0.0))
    }
    
    /// Calculate constraint penalty
    fn calculate_constraint_penalty(&self, result: &SimulationResult, constraints: &[Constraint]) -> f64 {
        let mut penalty = 0.0;
        
        for constraint in constraints {
            if let Some(&value) = result.metrics.get(&constraint.metric) {
                let violation = match constraint.condition {
                    ConstraintCondition::GreaterThan => {
                        if value < constraint.value {
                            constraint.value - value
                        } else {
                            0.0
                        }
                    }
                    ConstraintCondition::LessThan => {
                        if value > constraint.value {
                            value - constraint.value
                        } else {
                            0.0
                        }
                    }
                    ConstraintCondition::Equal => {
                        (value - constraint.value).abs()
                    }
                    ConstraintCondition::InRange(min, max) => {
                        if value < min {
                            min - value
                        } else if value > max {
                            value - max
                        } else {
                            0.0
                        }
                    }
                };
                
                // Apply penalty
                if violation > 0.0 {
                    if constraint.hard {
                        penalty += 1000.0 * violation; // Heavy penalty for hard constraints
                    } else {
                        penalty += 10.0 * violation; // Mild penalty for soft constraints
                    }
                }
            }
        }
        
        penalty
    }
    
    /// Calculate objective score
    fn calculate_objective_score(&self, result: &SimulationResult, objectives: &[Objective]) -> f64 {
        let mut total_score = 0.0;
        let mut total_weight = 0.0;
        
        for objective in objectives {
            if let Some(&value) = result.metrics.get(&objective.metric) {
                let score = match objective.goal {
                    OptimizationGoal::Minimize => {
                        // Normalize: lower is better
                        1.0 / (1.0 + value)
                    }
                    OptimizationGoal::Maximize => {
                        // Normalize: higher is better
                        value / (1.0 + value)
                    }
                    OptimizationGoal::Target(target) => {
                        // Normalize: closer to target is better
                        1.0 / (1.0 + (value - target).abs())
                    }
                };
                
                total_score += score * objective.weight;
                total_weight += objective.weight;
            }
        }
        
        if total_weight > 0.0 {
            total_score / total_weight
        } else {
            0.0
        }
    }
}

/// Nelder-Mead Simplex Optimizer
pub struct NelderMeadOptimizer {
    engine: SimulationEngine,
    config: OptimizationConfig,
    alpha: f64,  // Reflection coefficient
    gamma: f64,  // Expansion coefficient
    rho: f64,    // Contraction coefficient
    sigma: f64,  // Shrink coefficient
}

impl NelderMeadOptimizer {
    pub fn new(engine: SimulationEngine, config: OptimizationConfig) -> Self {
        Self {
            engine,
            config,
            alpha: 1.0,
            gamma: 2.0,
            rho: 0.5,
            sigma: 0.5,
        }
    }
    
    /// Run Nelder-Mead optimization
    pub fn optimize(
        &mut self,
        model: &ModelMetadata,
        initial_design: DesignParameters,
        parameter_names: Vec<String>,
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Result<OptimizationResult> {
        let start_time = Instant::now();
        let n = parameter_names.len();
        
        // Initialize simplex
        let mut simplex = self.initialize_simplex(&initial_design, &parameter_names);
        
        // Evaluate initial simplex
        let mut scores = Vec::new();
        for point in &simplex {
            let score = self.evaluate_design(model, point, objectives, constraints)?;
            scores.push(score);
        }
        
        let mut iteration = 0;
        let mut best_score = 0.0;
        
        while iteration < self.config.max_iterations {
            // Sort simplex by score (best first)
            let mut indexed: Vec<(usize, f64)> = scores.iter().enumerate()
                .map(|(i, &s)| (i, s))
                .collect();
            indexed.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
            
            // Reorder simplex and scores
            let sorted_simplex: Vec<_> = indexed.iter()
                .map(|(i, _)| simplex[*i].clone())
                .collect();
            let sorted_scores: Vec<_> = indexed.iter()
                .map(|(_, s)| *s)
                .collect();
            
            simplex = sorted_simplex;
            scores = sorted_scores;
            
            best_score = scores[0];
            
            // Check convergence
            if self.is_converged(&scores) {
                break;
            }
            
            // Calculate centroid (excluding worst point)
            let centroid = self.calculate_centroid(&simplex[..n], &parameter_names);
            
            // Reflection
            let reflected = self.reflect_point(&simplex[n], &centroid, &parameter_names);
            let reflected_score = self.evaluate_design(model, &reflected, objectives, constraints)?;
            
            if reflected_score > scores[n-1] && reflected_score < scores[0] {
                // Accept reflection
                simplex[n] = reflected;
                scores[n] = reflected_score;
            } else if reflected_score > scores[0] {
                // Try expansion
                let expanded = self.expand_point(&reflected, &centroid, &parameter_names);
                let expanded_score = self.evaluate_design(model, &expanded, objectives, constraints)?;
                
                if expanded_score > reflected_score {
                    simplex[n] = expanded;
                    scores[n] = expanded_score;
                } else {
                    simplex[n] = reflected;
                    scores[n] = reflected_score;
                }
            } else {
                // Try contraction
                let contracted = self.contract_point(&simplex[n], &centroid, &parameter_names);
                let contracted_score = self.evaluate_design(model, &contracted, objectives, constraints)?;
                
                if contracted_score > scores[n] {
                    simplex[n] = contracted;
                    scores[n] = contracted_score;
                } else {
                    // Shrink simplex
                    for i in 1..=n {
                        simplex[i] = self.shrink_point(&simplex[i], &simplex[0], &parameter_names);
                        scores[i] = self.evaluate_design(model, &simplex[i], objectives, constraints)?;
                    }
                }
            }
            
            iteration += 1;
        }
        
        Ok(OptimizationResult {
            final_design: simplex[0].clone(),
            best_score,
            iterations: iteration,
            total_runtime: start_time.elapsed(),
            convergence_reason: if iteration >= self.config.max_iterations {
                "Maximum iterations reached".to_string()
            } else {
                "Converged".to_string()
            },
        })
    }
    
    /// Initialize simplex around initial point
    fn initialize_simplex(&self, initial: &DesignParameters, params: &[String]) -> Vec<DesignParameters> {
        let n = params.len();
        let mut simplex = vec![initial.clone()];
        
        // Create n+1 points
        for i in 0..n {
            let mut point = initial.clone();
            if let Some(value) = point.get(&params[i]) {
                // Perturb by 10%
                point.set(&params[i], value * 1.1);
                simplex.push(point);
            }
        }
        
        simplex
    }
    
    /// Calculate centroid of points
    fn calculate_centroid(&self, points: &[DesignParameters], params: &[String]) -> DesignParameters {
        let mut centroid = DesignParameters::new();
        let n = points.len() as f64;
        
        for param in params {
            let sum: f64 = points.iter()
                .filter_map(|p| p.get(param))
                .sum();
            centroid.set(param, sum / n);
        }
        
        centroid
    }
    
    /// Reflect point through centroid
    fn reflect_point(&self, point: &DesignParameters, centroid: &DesignParameters, params: &[String]) -> DesignParameters {
        let mut reflected = DesignParameters::new();
        
        for param in params {
            if let (Some(p), Some(c)) = (point.get(param), centroid.get(param)) {
                reflected.set(param, c + self.alpha * (c - p));
            }
        }
        
        reflected
    }
    
    /// Expand point away from centroid
    fn expand_point(&self, point: &DesignParameters, centroid: &DesignParameters, params: &[String]) -> DesignParameters {
        let mut expanded = DesignParameters::new();
        
        for param in params {
            if let (Some(p), Some(c)) = (point.get(param), centroid.get(param)) {
                expanded.set(param, c + self.gamma * (p - c));
            }
        }
        
        expanded
    }
    
    /// Contract point toward centroid
    fn contract_point(&self, point: &DesignParameters, centroid: &DesignParameters, params: &[String]) -> DesignParameters {
        let mut contracted = DesignParameters::new();
        
        for param in params {
            if let (Some(p), Some(c)) = (point.get(param), centroid.get(param)) {
                contracted.set(param, c + self.rho * (p - c));
            }
        }
        
        contracted
    }
    
    /// Shrink point toward best point
    fn shrink_point(&self, point: &DesignParameters, best: &DesignParameters, params: &[String]) -> DesignParameters {
        let mut shrunk = DesignParameters::new();
        
        for param in params {
            if let (Some(p), Some(b)) = (point.get(param), best.get(param)) {
                shrunk.set(param, b + self.sigma * (p - b));
            }
        }
        
        shrunk
    }
    
    /// Check if simplex has converged
    fn is_converged(&self, scores: &[f64]) -> bool {
        if scores.len() < 2 {
            return true;
        }
        
        let best = scores[0];
        let worst = scores[scores.len() - 1];
        
        (best - worst).abs() < self.config.convergence_tolerance
    }
    
    /// Evaluate a single design
    fn evaluate_design(
        &mut self,
        model: &ModelMetadata,
        parameters: &DesignParameters,
        objectives: &[Objective],
        constraints: &[Constraint],
    ) -> Result<f64> {
        // Run simulation
        let sim_result = self.engine.simulate(model, parameters)?;
        
        // Check constraints
        let constraint_penalty = self.calculate_constraint_penalty(&sim_result, constraints);
        
        // Calculate objective score
        let objective_score = self.calculate_objective_score(&sim_result, objectives);
        
        // Final score with constraint penalty
        Ok((objective_score - constraint_penalty).max(0.0))
    }
    
    /// Calculate constraint penalty (same as GridSearch)
    fn calculate_constraint_penalty(&self, result: &SimulationResult, constraints: &[Constraint]) -> f64 {
        let mut penalty = 0.0;
        
        for constraint in constraints {
            if let Some(&value) = result.metrics.get(&constraint.metric) {
                let violation = match constraint.condition {
                    ConstraintCondition::GreaterThan => {
                        if value < constraint.value {
                            constraint.value - value
                        } else {
                            0.0
                        }
                    }
                    ConstraintCondition::LessThan => {
                        if value > constraint.value {
                            value - constraint.value
                        } else {
                            0.0
                        }
                    }
                    ConstraintCondition::Equal => {
                        (value - constraint.value).abs()
                    }
                    ConstraintCondition::InRange(min, max) => {
                        if value < min {
                            min - value
                        } else if value > max {
                            value - max
                        } else {
                            0.0
                        }
                    }
                };
                
                if violation > 0.0 {
                    if constraint.hard {
                        penalty += 1000.0 * violation;
                    } else {
                        penalty += 10.0 * violation;
                    }
                }
            }
        }
        
        penalty
    }
    
    /// Calculate objective score (same as GridSearch)
    fn calculate_objective_score(&self, result: &SimulationResult, objectives: &[Objective]) -> f64 {
        let mut total_score = 0.0;
        let mut total_weight = 0.0;
        
        for objective in objectives {
            if let Some(&value) = result.metrics.get(&objective.metric) {
                let score = match objective.goal {
                    OptimizationGoal::Minimize => {
                        1.0 / (1.0 + value)
                    }
                    OptimizationGoal::Maximize => {
                        value / (1.0 + value)
                    }
                    OptimizationGoal::Target(target) => {
                        1.0 / (1.0 + (value - target).abs())
                    }
                };
                
                total_score += score * objective.weight;
                total_weight += objective.weight;
            }
        }
        
        if total_weight > 0.0 {
            total_score / total_weight
        } else {
            0.0
        }
    }
}