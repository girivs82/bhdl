//! Evaluation subsystem for behavioral simulation

pub mod scheduler;
pub mod context;
pub mod evaluator;
pub mod when_processor;
pub mod error_recovery;
pub mod simple;

pub use scheduler::{EvaluationScheduler, AttributeId, DependencyChange};
pub use context::SimulationEvaluationContext;
pub use evaluator::{SimulationAttributeEvaluator, AttributeUpdateResult};
pub use when_processor::{WhenBlockProcessor, WhenProcessingResult};

/// Evaluation statistics
#[derive(Debug, Default)]
pub struct EvaluationStats {
    /// Total attributes evaluated
    pub attributes_evaluated: usize,
    
    /// Total when blocks processed
    pub when_blocks_processed: usize,
    
    /// Total evaluation time in milliseconds
    pub total_time_ms: f64,
    
    /// Number of evaluation cycles in current timestep
    pub cycles: usize,
}

/// Manages the complete evaluation process
pub struct EvaluationManager {
    scheduler: EvaluationScheduler,
    evaluator: SimulationAttributeEvaluator,
    when_processor: WhenBlockProcessor,
    stats: EvaluationStats,
}

impl EvaluationManager {
    /// Create a new evaluation manager
    pub fn new(
        scheduler: EvaluationScheduler,
        evaluator: SimulationAttributeEvaluator,
        when_processor: WhenBlockProcessor,
    ) -> Self {
        Self {
            scheduler,
            evaluator,
            when_processor,
            stats: EvaluationStats::default(),
        }
    }
    
    /// Perform complete evaluation for a timestep
    pub fn evaluate_timestep(
        &mut self,
        circuit_state: &mut crate::circuit::state::CircuitState,
        time_manager: &crate::engine::time::TimeManager,
    ) -> crate::error::SimulationResult<()> {
        let start = std::time::Instant::now();
        self.stats.cycles = 0;
        
        // Process when blocks first (they can update mutable attributes)
        let when_result = self.when_processor.process_all(circuit_state, time_manager)?;
        self.stats.when_blocks_processed += when_result.updated_attributes.len();
        
        // Mark updated attributes as dirty
        for attr in when_result.updated_attributes {
            self.scheduler.mark_dirty(AttributeId(attr));
        }
        
        // Evaluate attributes in dependency order
        while self.scheduler.has_dirty_attributes() {
            self.stats.cycles += 1;
            
            // Get next batch to evaluate
            let batch = self.scheduler.get_evaluation_batch();
            if batch.is_empty() {
                // No more attributes ready to evaluate
                // This could indicate circular dependencies
                if self.scheduler.has_dirty_attributes() {
                    return Err(crate::error::SimulationError::EvaluationError(
                        "Circular dependencies detected".to_string()
                    ));
                }
                break;
            }
            
            // Evaluate the batch
            let results = self.evaluator.evaluate_batch(&batch, circuit_state, time_manager)?;
            
            // Update statistics
            self.stats.attributes_evaluated += results.len();
            
            // Check for new dependencies (dynamic dependencies)
            // This would be needed for advanced features but skipped for now
        }
        
        self.stats.total_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        Ok(())
    }
    
    /// Get evaluation statistics
    pub fn stats(&self) -> &EvaluationStats {
        &self.stats
    }
    
    /// Reset statistics
    pub fn reset_stats(&mut self) {
        self.stats = EvaluationStats::default();
        self.evaluator.reset_metrics();
        self.when_processor.reset_metrics();
    }
}