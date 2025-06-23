//! Evaluation context bridge for simulation
//! Connects the simulation state to the expression evaluator

use crate::circuit::state::{CircuitState, PinValue};
use crate::engine::time::TimeManager;
use bhdl_analyzer::expression_evaluator::{EvaluationContext, RuntimeValue};
use bhdl_analyzer::builtin_variables::SimulationContext;
use std::collections::HashMap;

/// Bridge between simulation state and expression evaluation
pub struct SimulationEvaluationContext<'a> {
    circuit_state: &'a CircuitState,
    time_manager: &'a TimeManager,
    builtin_manager: BuiltinVariableManager,
}

impl<'a> SimulationEvaluationContext<'a> {
    /// Create a new evaluation context
    pub fn new(circuit_state: &'a CircuitState, time_manager: &'a TimeManager) -> Self {
        Self {
            circuit_state,
            time_manager,
            builtin_manager: BuiltinVariableManager::new(),
        }
    }
    
    /// Build an evaluation context for the expression evaluator
    /// Note: This creates a SimulationContext that must outlive the returned EvaluationContext
    pub fn build_context_with_sim<'b>(&self, sim_context: &'b SimulationContext) -> EvaluationContext<'b> {
        let mut eval_context = EvaluationContext::new(sim_context);
        
        // Copy all attributes from circuit state
        for (name, value) in self.collect_attributes() {
            eval_context.set_attribute(name, value);
        }
        
        // Copy all pin values
        for (name, value) in self.collect_pins() {
            eval_context.set_pin(name, value);
        }
        
        eval_context
    }
    
    /// Create a simulation context with current time values
    pub fn create_sim_context(&self) -> SimulationContext {
        SimulationContext {
            current_time: self.time_manager.current_time(),
            time_step: self.time_manager.time_step(),
            custom_values: HashMap::new(),
        }
    }
    
    /// Collect all attributes as runtime values
    fn collect_attributes(&self) -> HashMap<String, RuntimeValue> {
        self.circuit_state.get_all_attributes()
    }
    
    /// Collect all pin values as runtime values
    fn collect_pins(&self) -> HashMap<String, RuntimeValue> {
        let mut pins = HashMap::new();
        
        // Get all pins from circuit state
        for (pin_name, pin_value) in self.circuit_state.get_all_pins() {
            pins.insert(pin_name, Self::pin_to_runtime_value(&pin_value));
        }
        
        pins
    }
    
    /// Convert a pin value to runtime value (voltage)
    pub fn pin_to_runtime_value(pin: &PinValue) -> RuntimeValue {
        RuntimeValue::Real(pin.voltage)
    }
}

/// Manages built-in simulation variables
#[derive(Debug)]
pub struct BuiltinVariableManager {
    // Cache for computed values
    cache: HashMap<String, f64>,
}

impl BuiltinVariableManager {
    /// Create a new builtin variable manager
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }
    
    /// Clear the cache
    pub fn clear_cache(&mut self) {
        self.cache.clear();
    }
}

impl Default for BuiltinVariableManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::state::CircuitTopology;
    
    #[test]
    fn test_context_creation() {
        let topology = CircuitTopology {
            instance_modules: HashMap::new(),
            net_connections: HashMap::new(),
        };
        
        let circuit_state = CircuitState::new(topology);
        let time_manager = TimeManager::new(1e-6);
        
        let sim_context = SimulationEvaluationContext::new(&circuit_state, &time_manager);
        let sim_ctx = sim_context.create_sim_context();
        let eval_context = sim_context.build_context_with_sim(&sim_ctx);
        
        // Check built-in variables are accessible
        assert_eq!(eval_context.simulation.current_time, 0.0);
        assert_eq!(eval_context.simulation.time_step, 1e-6);
    }
}