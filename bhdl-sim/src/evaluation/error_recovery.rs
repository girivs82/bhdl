//! Error recovery mechanisms for evaluation failures

use crate::error::{SimulationError, SimulationResult};
use bhdl_analyzer::expression_evaluator::RuntimeValue;
use std::collections::HashMap;

/// Handles error recovery during attribute evaluation
#[derive(Debug)]
pub struct ErrorRecoveryHandler {
    /// Strategy for handling errors
    strategy: RecoveryStrategy,
    
    /// Fallback values for attributes
    fallback_values: HashMap<String, RuntimeValue>,
    
    /// Error log
    error_log: Vec<RecoveryEvent>,
    
    /// Maximum errors before failing
    max_errors: usize,
}

/// Recovery strategy for evaluation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryStrategy {
    /// Use last known good value
    UseLastValue,
    /// Use default/fallback value
    UseFallback,
    /// Interpolate from previous values
    Interpolate,
    /// Fail immediately
    FailFast,
}

/// Event logged during error recovery
#[derive(Debug)]
pub struct RecoveryEvent {
    pub time: f64,
    pub attribute: String,
    pub error: String,
    pub action: RecoveryAction,
}

/// Action taken during recovery
#[derive(Debug, Clone)]
pub enum RecoveryAction {
    UsedLastValue(RuntimeValue),
    UsedFallback(RuntimeValue),
    Interpolated(RuntimeValue),
    Failed,
}

impl ErrorRecoveryHandler {
    /// Create a new error recovery handler
    pub fn new(strategy: RecoveryStrategy) -> Self {
        Self {
            strategy,
            fallback_values: HashMap::new(),
            error_log: Vec::new(),
            max_errors: 100,
        }
    }
    
    /// Set recovery strategy
    pub fn set_strategy(&mut self, strategy: RecoveryStrategy) {
        self.strategy = strategy;
    }
    
    /// Set fallback value for an attribute
    pub fn set_fallback(&mut self, attribute: String, value: RuntimeValue) {
        self.fallback_values.insert(attribute, value);
    }
    
    /// Set maximum errors before failing
    pub fn set_max_errors(&mut self, max: usize) {
        self.max_errors = max;
    }
    
    /// Handle an evaluation error
    pub fn handle_error(
        &mut self,
        time: f64,
        attribute: &str,
        error: SimulationError,
        last_value: Option<&RuntimeValue>,
    ) -> SimulationResult<RuntimeValue> {
        // Check if we've exceeded error limit
        if self.error_log.len() >= self.max_errors {
            return Err(SimulationError::EvaluationError(
                format!("Maximum error count ({}) exceeded", self.max_errors)
            ));
        }
        
        // Apply recovery strategy
        let (value, action) = match self.strategy {
            RecoveryStrategy::UseLastValue => {
                if let Some(last) = last_value {
                    (last.clone(), RecoveryAction::UsedLastValue(last.clone()))
                } else {
                    // No last value, use fallback
                    self.apply_fallback(attribute)?
                }
            }
            RecoveryStrategy::UseFallback => {
                self.apply_fallback(attribute)?
            }
            RecoveryStrategy::Interpolate => {
                // TODO: Implement interpolation from history
                // For now, fall back to last value
                if let Some(last) = last_value {
                    (last.clone(), RecoveryAction::Interpolated(last.clone()))
                } else {
                    self.apply_fallback(attribute)?
                }
            }
            RecoveryStrategy::FailFast => {
                return Err(error);
            }
        };
        
        // Log the recovery event
        self.error_log.push(RecoveryEvent {
            time,
            attribute: attribute.to_string(),
            error: error.to_string(),
            action,
        });
        
        Ok(value)
    }
    
    /// Apply fallback value
    fn apply_fallback(&self, attribute: &str) -> SimulationResult<(RuntimeValue, RecoveryAction)> {
        if let Some(fallback) = self.fallback_values.get(attribute) {
            Ok((fallback.clone(), RecoveryAction::UsedFallback(fallback.clone())))
        } else {
            // Default fallback is 0.0
            let default = RuntimeValue::Real(0.0);
            Ok((default.clone(), RecoveryAction::UsedFallback(default)))
        }
    }
    
    /// Get error log
    pub fn error_log(&self) -> &[RecoveryEvent] {
        &self.error_log
    }
    
    /// Clear error log
    pub fn clear_log(&mut self) {
        self.error_log.clear();
    }
    
    /// Get error count
    pub fn error_count(&self) -> usize {
        self.error_log.len()
    }
}

impl Default for ErrorRecoveryHandler {
    fn default() -> Self {
        Self::new(RecoveryStrategy::UseLastValue)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_last_value_recovery() {
        let mut handler = ErrorRecoveryHandler::new(RecoveryStrategy::UseLastValue);
        
        let last_value = RuntimeValue::Real(5.0);
        let error = SimulationError::EvaluationError("Test error".to_string());
        
        let result = handler.handle_error(
            1.0,
            "test_attr",
            error,
            Some(&last_value),
        ).unwrap();
        
        assert_eq!(result, RuntimeValue::Real(5.0));
        assert_eq!(handler.error_count(), 1);
    }
    
    #[test]
    fn test_fallback_recovery() {
        let mut handler = ErrorRecoveryHandler::new(RecoveryStrategy::UseFallback);
        handler.set_fallback("test_attr".to_string(), RuntimeValue::Real(10.0));
        
        let error = SimulationError::EvaluationError("Test error".to_string());
        
        let result = handler.handle_error(
            1.0,
            "test_attr",
            error,
            None,
        ).unwrap();
        
        assert_eq!(result, RuntimeValue::Real(10.0));
    }
    
    #[test]
    fn test_fail_fast() {
        let mut handler = ErrorRecoveryHandler::new(RecoveryStrategy::FailFast);
        
        let error = SimulationError::EvaluationError("Test error".to_string());
        
        let result = handler.handle_error(
            1.0,
            "test_attr",
            error.clone(),
            None,
        );
        
        assert!(result.is_err());
        assert_eq!(handler.error_count(), 0); // No recovery attempted
    }
    
    #[test]
    fn test_max_errors() {
        let mut handler = ErrorRecoveryHandler::new(RecoveryStrategy::UseFallback);
        handler.set_max_errors(2);
        
        let error = SimulationError::EvaluationError("Test error".to_string());
        
        // First two errors should succeed
        for _ in 0..2 {
            let result = handler.handle_error(1.0, "test_attr", error.clone(), None);
            assert!(result.is_ok());
        }
        
        // Third error should fail
        let result = handler.handle_error(1.0, "test_attr", error, None);
        assert!(result.is_err());
    }
}