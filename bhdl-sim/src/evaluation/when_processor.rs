//! When block processor for conditional behavioral updates
//! Processes when blocks during simulation to update mutable attributes

use crate::circuit::state::CircuitState;
use crate::engine::time::TimeManager;
use crate::error::{SimulationResult, SimulationError};
use crate::evaluation::context::SimulationEvaluationContext;
use crate::evaluation::expression_parser::ExpressionParser;
use bhdl_analyzer::attribute_analysis::WhenBlockInfo;
use bhdl_analyzer::expression_evaluator::{ExpressionEvaluator, RuntimeValue};

/// Processes when blocks during simulation
pub struct WhenBlockProcessor {
    /// When blocks from attribute analysis
    when_blocks: Vec<WhenBlockInfo>,
    
    /// Expression parser
    expression_parser: ExpressionParser,
    
    /// Performance metrics
    metrics: WhenProcessorMetrics,
}

/// Performance metrics for when block processing
#[derive(Debug, Default)]
struct WhenProcessorMetrics {
    total_evaluations: usize,
    conditions_true: usize,
    attributes_updated: usize,
    processing_time_ms: f64,
}

impl WhenBlockProcessor {
    /// Create a new when block processor
    pub fn new(when_blocks: Vec<WhenBlockInfo>) -> Self {
        Self {
            when_blocks,
            expression_parser: ExpressionParser::new(),
            metrics: WhenProcessorMetrics::default(),
        }
    }
    
    /// Process all when blocks
    pub fn process_all(
        &mut self,
        circuit_state: &mut CircuitState,
        time_manager: &TimeManager,
    ) -> SimulationResult<WhenProcessingResult> {
        let start = std::time::Instant::now();
        let mut result = WhenProcessingResult::default();
        
        // Create evaluation context
        let sim_context = SimulationEvaluationContext::new(circuit_state, time_manager);
        let sim_ctx = sim_context.create_sim_context();
        let eval_context = sim_context.build_context_with_sim(&sim_ctx);
        
        // Process each when block
        for when_block in &self.when_blocks {
            self.metrics.total_evaluations += 1;
            
            // Evaluate the condition (need to handle mutability)
            let condition_result = {
                // Parse the condition expression
                let expr = self.expression_parser.parse(&when_block.condition)?;
                
                // Evaluate the expression
                let result = ExpressionEvaluator::evaluate(&expr, &eval_context)
                    .map_err(|e| SimulationError::EvaluationError(format!("Condition evaluation failed: {}", e)))?;
                
                // Convert to boolean
                result.to_bool()
                    .map_err(|e| SimulationError::EvaluationError(format!("Condition must evaluate to boolean: {}", e)))?
            };
            
            if condition_result {
                self.metrics.conditions_true += 1;
                
                // Process assignments in the when block
                for (attr_name, expr_text) in &when_block.assignments {
                    // Parse and evaluate the assignment expression
                    match self.expression_parser.parse(expr_text) {
                        Ok(expr) => {
                            match ExpressionEvaluator::evaluate(&expr, &eval_context) {
                                Ok(value) => {
                                    // Update the attribute in circuit state
                                    circuit_state.update_attribute(attr_name, value);
                                    result.updated_attributes.insert(attr_name.clone());
                                    self.metrics.attributes_updated += 1;
                                }
                                Err(e) => {
                                    result.errors.push(format!("Failed to evaluate expression for {}: {}", attr_name, e));
                                }
                            }
                        }
                        Err(e) => {
                            result.errors.push(format!("Failed to parse expression for {}: {}", attr_name, e));
                        }
                    }
                }
            }
        }
        
        self.metrics.processing_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        Ok(result)
    }
    
    /// Evaluate a when block condition
    fn evaluate_condition(
        &mut self,
        condition: &str,
        context: &bhdl_analyzer::expression_evaluator::EvaluationContext,
    ) -> SimulationResult<bool> {
        // Parse the condition expression
        let expr = self.expression_parser.parse(condition)?;
        
        // Evaluate the expression
        let result = ExpressionEvaluator::evaluate(&expr, context)
            .map_err(|e| SimulationError::EvaluationError(format!("Condition evaluation failed: {}", e)))?;
        
        // Convert to boolean
        result.to_bool()
            .map_err(|e| SimulationError::EvaluationError(format!("Condition must evaluate to boolean: {}", e)))
    }
    
    /// Get processing metrics
    pub fn metrics(&self) -> &WhenProcessorMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = WhenProcessorMetrics::default();
    }
}

/// Result of processing when blocks
#[derive(Debug, Default)]
pub struct WhenProcessingResult {
    /// Attributes that were updated
    pub updated_attributes: std::collections::HashSet<String>,
    
    /// Any errors encountered (non-fatal)
    pub errors: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::state::CircuitTopology;
    
    #[test]
    fn test_empty_when_blocks() {
        let mut processor = WhenBlockProcessor::new(vec![]);
        
        let topology = CircuitTopology {
            instance_modules: std::collections::HashMap::new(),
            net_connections: std::collections::HashMap::new(),
        };
        
        let mut circuit_state = CircuitState::new(topology);
        let time_manager = TimeManager::new(1e-6);
        
        let result = processor.process_all(&mut circuit_state, &time_manager).unwrap();
        
        assert!(result.updated_attributes.is_empty());
        assert_eq!(processor.metrics().total_evaluations, 0);
    }
    
    #[test]
    fn test_when_block_metrics() {
        let when_blocks = vec![
            WhenBlockInfo {
                condition: "t > 1.0".to_string(),
                assignments: std::collections::HashMap::new(),
            },
        ];
        
        let mut processor = WhenBlockProcessor::new(when_blocks);
        
        let topology = CircuitTopology {
            instance_modules: std::collections::HashMap::new(),
            net_connections: std::collections::HashMap::new(),
        };
        
        let mut circuit_state = CircuitState::new(topology);
        let time_manager = TimeManager::new(1e-6);
        
        processor.process_all(&mut circuit_state, &time_manager).unwrap();
        
        assert_eq!(processor.metrics().total_evaluations, 1);
        // Condition evaluates to false in our stub implementation
        assert_eq!(processor.metrics().conditions_true, 0);
    }
}