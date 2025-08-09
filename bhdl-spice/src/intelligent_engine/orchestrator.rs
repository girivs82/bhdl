//! Strategy orchestration for intelligent solving

use super::{patterns::CircuitPattern, strategies::{SolvingStrategy, SolverContext}, SynthesizerContext};
use crate::{GlacierSolver, Result, AnalysisResult};
use std::time::{Duration, Instant};

/// Orchestrates execution of solving strategies
pub struct Orchestrator {
    /// Execution mode
    mode: ExecutionMode,
    
    /// Timeout for individual strategies
    strategy_timeout: Duration,
}

/// How to execute multiple strategies
#[derive(Debug, Clone, Copy)]
pub enum ExecutionMode {
    /// Try strategies sequentially until one succeeds
    Sequential,
    
    /// Run strategies in parallel, first to succeed wins
    Parallel,
    
    /// Run all strategies and combine results
    Ensemble,
}

impl Orchestrator {
    pub fn new() -> Self {
        Self {
            mode: ExecutionMode::Sequential,
            strategy_timeout: Duration::from_secs(30),
        }
    }
    
    /// Set execution mode
    pub fn set_mode(&mut self, mode: ExecutionMode) {
        self.mode = mode;
    }
    
    /// Execute strategies for the given patterns
    pub fn execute(
        &self,
        solver: &mut GlacierSolver,
        strategies: &[&dyn SolvingStrategy],
        patterns: &[CircuitPattern],
        context: Option<&SynthesizerContext>,
    ) -> Result<Vec<AnalysisResult>> {
        if strategies.is_empty() || patterns.is_empty() {
            return solver.analyze_simple();
        }
        
        // Build solver context
        let solver_context = self.build_solver_context(context);
        
        match self.mode {
            ExecutionMode::Sequential => {
                self.execute_sequential(solver, strategies, patterns, &solver_context)
            },
            ExecutionMode::Parallel => {
                // For now, fall back to sequential
                // TODO: Implement true parallel execution
                self.execute_sequential(solver, strategies, patterns, &solver_context)
            },
            ExecutionMode::Ensemble => {
                // For now, fall back to sequential
                // TODO: Implement ensemble voting
                self.execute_sequential(solver, strategies, patterns, &solver_context)
            },
        }
    }
    
    /// Execute strategies sequentially
    fn execute_sequential(
        &self,
        solver: &mut GlacierSolver,
        strategies: &[&dyn SolvingStrategy],
        patterns: &[CircuitPattern],
        context: &SolverContext,
    ) -> Result<Vec<AnalysisResult>> {
        // Try each pattern-strategy pair
        for (pattern, strategy) in patterns.iter().zip(strategies.iter()) {
            eprintln!("Attempting {} strategy for {} pattern", 
                strategy.name(), pattern.name());
            
            let start = Instant::now();
            
            match strategy.solve(solver, pattern, context) {
                Ok(results) => {
                    let elapsed = start.elapsed();
                    eprintln!("Strategy {} succeeded in {:?}", strategy.name(), elapsed);
                    return Ok(results);
                },
                Err(e) => {
                    let elapsed = start.elapsed();
                    eprintln!("Strategy {} failed after {:?}: {}", 
                        strategy.name(), elapsed, e);
                    
                    // Continue to next strategy
                    continue;
                }
            }
        }
        
        // All strategies failed, try default solver as last resort
        eprintln!("All strategies failed, falling back to default solver");
        solver.analyze_simple()
    }
    
    /// Build solver context from synthesizer context
    fn build_solver_context(&self, synth_context: Option<&SynthesizerContext>) -> SolverContext {
        SolverContext {
            previous_solutions: Vec::new(),
            temperature: 25.0,
            convergence_history: Vec::new(),
            user_hints: std::collections::HashMap::new(),
            synthesizer_context: synth_context.cloned(),
        }
    }
}