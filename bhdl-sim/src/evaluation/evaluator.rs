//! Attribute evaluator for simulation
//! Evaluates behavioral attribute expressions during simulation

use crate::circuit::state::CircuitState;
use crate::engine::time::TimeManager;
use crate::error::{SimulationError, SimulationResult};
use crate::evaluation::scheduler::AttributeId;
use bhdl_analyzer::{
    expression_evaluator::RuntimeValue,
    attribute_analysis::AttributeAnalysisResult,
};
use bhdl_ast::expr::Expr;
use std::collections::HashMap;

/// Evaluates attribute expressions during simulation
pub struct SimulationAttributeEvaluator {
    /// Attribute analysis from the analyzer
    attribute_analysis: AttributeAnalysisResult,
    
    /// Cached parsed expressions
    expression_cache: HashMap<String, Expr>,
    
    /// Performance metrics
    metrics: EvaluationMetrics,
}

/// Performance metrics for evaluation
#[derive(Debug, Default)]
struct EvaluationMetrics {
    total_evaluations: usize,
    cache_hits: usize,
    evaluation_time_ms: f64,
}

impl SimulationAttributeEvaluator {
    /// Create a new attribute evaluator
    pub fn new(attribute_analysis: AttributeAnalysisResult) -> Self {
        Self {
            attribute_analysis,
            expression_cache: HashMap::new(),
            metrics: EvaluationMetrics::default(),
        }
    }
    
    /// Evaluate a batch of attributes
    pub fn evaluate_batch(
        &mut self,
        attributes: &[AttributeId],
        circuit_state: &mut CircuitState,
        time_manager: &TimeManager,
    ) -> SimulationResult<Vec<AttributeUpdateResult>> {
        let mut results = Vec::new();
        let mut updates = Vec::new();
        
        // First pass: evaluate all attributes without modifying state
        for attr_id in attributes {
            let (result, new_value) = self.evaluate_single(
                &attr_id.0,
                circuit_state as &CircuitState,  // Pass as immutable
                time_manager,
            )?;
            updates.push((attr_id.0.clone(), new_value));
            results.push(result);
        }
        
        // Second pass: apply all updates
        for (attr_name, new_value) in updates {
            circuit_state.update_attribute(&attr_name, new_value);
        }
        
        Ok(results)
    }
    
    /// Evaluate a single attribute (returns result without modifying state)
    fn evaluate_single(
        &mut self,
        attr_name: &str,
        circuit_state: &CircuitState,
        _time_manager: &TimeManager,
    ) -> SimulationResult<(AttributeUpdateResult, RuntimeValue)> {
        let start = std::time::Instant::now();
        
        // Get attribute info
        let attr_info = self.attribute_analysis.attributes.get(attr_name)
            .ok_or_else(|| SimulationError::EvaluationError(
                format!("Unknown attribute: {}", attr_name)
            ))?;
        
        // Check if it's an expression attribute
        let new_value = match &attr_info.attribute_type {
            bhdl_ast::attributes::AttributeType::Expression(_dependencies) => {
                // For now, we need to get the actual expression from the AST
                // This is a limitation - we'd need the actual expression text or AST node
                // For now, just return the current value
                circuit_state.get_attribute(attr_name)
                    .cloned()
                    .unwrap_or(RuntimeValue::Real(0.0))
            }
            bhdl_ast::attributes::AttributeType::Static(_value) => {
                // Static attributes don't need evaluation
                // Just return the current value
                circuit_state.get_attribute(attr_name)
                    .cloned()
                    .unwrap_or(RuntimeValue::Real(0.0))
            }
            bhdl_ast::attributes::AttributeType::Mutable => {
                // Mutable attributes keep their current value unless updated by when blocks
                circuit_state.get_attribute(attr_name)
                    .cloned()
                    .unwrap_or(RuntimeValue::Real(0.0))
            }
        };
        
        // Get old value
        let old_value = circuit_state.get_attribute(attr_name).cloned();
        
        // Update metrics
        self.metrics.total_evaluations += 1;
        self.metrics.evaluation_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        let changed = old_value.as_ref() != Some(&new_value);
        let result = AttributeUpdateResult {
            attribute: attr_name.to_string(),
            old_value,
            new_value: new_value.clone(),
            changed,
        };
        
        Ok((result, new_value))
    }
    
    /// Get or parse an expression from cache
    fn get_or_parse_expression(
        &mut self,
        attr_name: &str,
        _expr_node: &str,
    ) -> SimulationResult<Expr> {
        if let Some(expr) = self.expression_cache.get(attr_name) {
            self.metrics.cache_hits += 1;
            return Ok(expr.clone());
        }
        
        // TODO: Parse the expression from the AST node
        // For now, return an error as we need the actual AST node
        Err(SimulationError::EvaluationError(
            "Expression parsing not yet implemented".to_string()
        ))
    }
    
    /// Get evaluation metrics
    pub fn metrics(&self) -> &EvaluationMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = EvaluationMetrics::default();
    }
}

/// Result of evaluating an attribute
#[derive(Debug)]
pub struct AttributeUpdateResult {
    /// Attribute name
    pub attribute: String,
    /// Old value (if any)
    pub old_value: Option<RuntimeValue>,
    /// New value
    pub new_value: RuntimeValue,
    /// Whether the value changed
    pub changed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::state::CircuitTopology;
    use crate::evaluation::SimulationEvaluationContext;
    
    #[test]
    fn test_static_attribute_evaluation() {
        let attr_analysis = AttributeAnalysisResult {
            attributes: HashMap::new(),
            dependencies: HashMap::new(),
            evaluation_order: vec![],
            circular_dependencies: vec![],
            mutable_attributes: std::collections::HashSet::new(),
        };
        
        let mut evaluator = SimulationAttributeEvaluator::new(attr_analysis);
        
        let topology = CircuitTopology {
            instance_modules: HashMap::new(),
            net_connections: HashMap::new(),
        };
        
        let mut circuit_state = CircuitState::new(topology);
        let time_manager = TimeManager::new(1e-6);
        
        // Set a static attribute
        circuit_state.update_attribute("test_attr", RuntimeValue::Real(5.0));
        
        // Evaluation should preserve static values
        let sim_context = SimulationEvaluationContext::new(&circuit_state, &time_manager);
        
        // Note: This test is limited because we need actual attribute info
        // Full testing will be done once expression parsing is implemented
        assert_eq!(evaluator.metrics().total_evaluations, 0);
    }
}