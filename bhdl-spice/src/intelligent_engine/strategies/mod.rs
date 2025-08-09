//! Solving strategies for different circuit patterns

pub mod progressive;
pub mod current_sharing;
pub mod symmetry;
pub mod state_enum;

use crate::{GlacierSolver, Result, AnalysisResult};
use crate::intelligent_engine::{patterns::CircuitPattern, SynthesizerContext};
use std::collections::HashMap;

/// Context passed to strategies during solving
#[derive(Debug, Clone)]
pub struct SolverContext {
    /// Previous solutions for warm starting
    pub previous_solutions: Vec<AnalysisResult>,
    
    /// Temperature for thermal analysis
    pub temperature: f64,
    
    /// Convergence history
    pub convergence_history: Vec<ConvergenceMetric>,
    
    /// User-provided hints
    pub user_hints: HashMap<String, Hint>,
    
    /// Synthesizer context if available
    pub synthesizer_context: Option<SynthesizerContext>,
}

/// Convergence metrics for tracking
#[derive(Debug, Clone)]
pub struct ConvergenceMetric {
    pub iteration: usize,
    pub error: f64,
    pub time_ms: f64,
}

/// User hints for guiding solution
#[derive(Debug, Clone)]
pub enum Hint {
    ExpectedCurrent(f64),
    ExpectedVoltage(String, f64),
    SymmetricSolution,
    InitialGuess(Vec<f64>),
    PreferredStrategy(String),
}

/// Core trait that all solving strategies implement
pub trait SolvingStrategy: Send + Sync {
    /// Strategy name for identification
    fn name(&self) -> &str;
    
    /// Check if this strategy is applicable to the given pattern
    fn applicable(&self, pattern: &CircuitPattern) -> bool;
    
    /// Confidence level for this pattern (0.0 to 1.0)
    fn confidence(&self, pattern: &CircuitPattern) -> f64;
    
    /// Execute the strategy
    fn solve(
        &self,
        solver: &mut GlacierSolver,
        pattern: &CircuitPattern,
        context: &SolverContext,
    ) -> Result<Vec<AnalysisResult>>;
    
    /// Check if strategy matches a specific intent
    fn matches_intent(&self, intent_name: &str, params: &HashMap<String, String>) -> bool {
        // Default: no intent matching
        false
    }
    
    /// Generate initial guess based on strategy knowledge
    fn generate_initial_guess(
        &self,
        pattern: &CircuitPattern,
        context: &SolverContext,
    ) -> Option<InitialGuess> {
        None
    }
}

/// Types of initial guesses
#[derive(Debug, Clone)]
pub enum InitialGuess {
    /// Use default solver initialization
    Default,
    
    /// Start from specific values
    Values(Vec<f64>),
    
    /// Ramp from start to end
    Ramped {
        start: f64,
        end: f64,
        stages: usize,
    },
    
    /// Enforce symmetric solution
    Symmetric,
    
    /// Current-limited initialization
    CurrentLimited(f64),
}

/// Default strategy when no special pattern is detected
pub struct DefaultStrategy;

impl DefaultStrategy {
    pub fn new() -> Self {
        Self
    }
}

impl SolvingStrategy for DefaultStrategy {
    fn name(&self) -> &str {
        "Default"
    }
    
    fn applicable(&self, _pattern: &CircuitPattern) -> bool {
        // Always applicable as fallback
        true
    }
    
    fn confidence(&self, _pattern: &CircuitPattern) -> f64 {
        0.1 // Low confidence, just a fallback
    }
    
    fn solve(
        &self,
        solver: &mut GlacierSolver,
        _pattern: &CircuitPattern,
        _context: &SolverContext,
    ) -> Result<Vec<AnalysisResult>> {
        // Just use the standard solver
        solver.analyze_simple()
    }
}

/// Helper functions for strategies
pub mod helpers {
    use super::*;
    use crate::GlacierSolver;
    
    /// Restore original component models
    pub fn restore_components(
        solver: &mut GlacierSolver,
        original_models: HashMap<String, crate::ComponentModel>,
    ) {
        for (comp, model) in original_models {
            solver.update_component_model(&comp, model);
        }
    }
    
    /// Check if solution is symmetric within tolerance
    pub fn is_symmetric_solution(
        voltages: &[(String, f64)],
        components: &[String],
        tolerance: f64,
    ) -> bool {
        if components.len() < 2 {
            return true;
        }
        
        // Get voltages for specified components
        let comp_voltages: Vec<f64> = components.iter()
            .filter_map(|comp| {
                voltages.iter()
                    .find(|(name, _)| name.contains(comp))
                    .map(|(_, v)| *v)
            })
            .collect();
        
        if comp_voltages.len() < 2 {
            return false;
        }
        
        // Check if all voltages are within tolerance
        let avg = comp_voltages.iter().sum::<f64>() / comp_voltages.len() as f64;
        comp_voltages.iter().all(|v| (v - avg).abs() < tolerance)
    }
}