// Built-in variables for behavioral modeling
// Provides support for special variables like 'dt' (time step)

use std::collections::HashMap;
use bhdl_ast::semantic_analysis::BhdlType;

/// Built-in variable information
#[derive(Debug, Clone)]
pub struct BuiltinVariable {
    pub name: String,
    pub var_type: BhdlType,
    pub description: String,
    pub is_constant: bool,
}

/// Manager for built-in variables
#[derive(Debug, Clone)]
pub struct BuiltinVariableManager {
    variables: HashMap<String, BuiltinVariable>,
}

impl BuiltinVariableManager {
    pub fn new() -> Self {
        let mut manager = Self {
            variables: HashMap::new(),
        };
        manager.register_default_builtins();
        manager
    }
    
    /// Register default built-in variables
    fn register_default_builtins(&mut self) {
        // dt - simulation time step
        self.register_builtin(BuiltinVariable {
            name: "dt".to_string(),
            var_type: BhdlType::Real,
            description: "Simulation time step in seconds".to_string(),
            is_constant: true, // Constant during a simulation run
        });
        
        // t - current simulation time
        self.register_builtin(BuiltinVariable {
            name: "t".to_string(),
            var_type: BhdlType::Real,
            description: "Current simulation time in seconds".to_string(),
            is_constant: false, // Changes during simulation
        });
        
        // pi - mathematical constant
        self.register_builtin(BuiltinVariable {
            name: "pi".to_string(),
            var_type: BhdlType::Real,
            description: "Mathematical constant π (3.14159...)".to_string(),
            is_constant: true,
        });
        
        // e - mathematical constant
        self.register_builtin(BuiltinVariable {
            name: "e".to_string(),
            var_type: BhdlType::Real,
            description: "Mathematical constant e (2.71828...)".to_string(),
            is_constant: true,
        });
    }
    
    /// Register a built-in variable
    pub fn register_builtin(&mut self, var: BuiltinVariable) {
        self.variables.insert(var.name.clone(), var);
    }
    
    /// Check if a name is a built-in variable
    pub fn is_builtin(&self, name: &str) -> bool {
        self.variables.contains_key(name)
    }
    
    /// Get built-in variable information
    pub fn get_builtin(&self, name: &str) -> Option<&BuiltinVariable> {
        self.variables.get(name)
    }
    
    /// Get all built-in variable names
    pub fn builtin_names(&self) -> Vec<String> {
        self.variables.keys().cloned().collect()
    }
}

/// Simulation context containing runtime values for built-in variables
#[derive(Debug, Clone)]
pub struct SimulationContext {
    /// Current simulation time
    pub current_time: f64,
    /// Simulation time step
    pub time_step: f64,
    /// Custom built-in values
    pub custom_values: HashMap<String, f64>,
}

impl SimulationContext {
    pub fn new(time_step: f64) -> Self {
        Self {
            current_time: 0.0,
            time_step,
            custom_values: HashMap::new(),
        }
    }
    
    /// Advance simulation by one time step
    pub fn advance_time(&mut self) {
        self.current_time += self.time_step;
    }
    
    /// Get value of a built-in variable
    pub fn get_builtin_value(&self, name: &str) -> Option<f64> {
        match name {
            "dt" => Some(self.time_step),
            "t" => Some(self.current_time),
            "pi" => Some(std::f64::consts::PI),
            "e" => Some(std::f64::consts::E),
            _ => self.custom_values.get(name).copied(),
        }
    }
    
    /// Set a custom built-in value
    pub fn set_custom_value(&mut self, name: String, value: f64) {
        self.custom_values.insert(name, value);
    }
}

/// Check if a variable reference should be excluded from attribute dependencies
/// Built-in variables like 'dt' don't create dependencies
pub fn is_dependency_excluded(name: &str) -> bool {
    matches!(name, "dt" | "t" | "pi" | "e")
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_builtin_manager() {
        let manager = BuiltinVariableManager::new();
        
        assert!(manager.is_builtin("dt"));
        assert!(manager.is_builtin("t"));
        assert!(manager.is_builtin("pi"));
        assert!(manager.is_builtin("e"));
        assert!(!manager.is_builtin("foo"));
        
        let dt_var = manager.get_builtin("dt").unwrap();
        assert_eq!(dt_var.name, "dt");
        assert!(dt_var.is_constant);
        assert_eq!(dt_var.var_type, BhdlType::Real);
    }
    
    #[test]
    fn test_simulation_context() {
        let mut ctx = SimulationContext::new(0.001); // 1ms time step
        
        assert_eq!(ctx.get_builtin_value("dt"), Some(0.001));
        assert_eq!(ctx.get_builtin_value("t"), Some(0.0));
        assert_eq!(ctx.get_builtin_value("pi"), Some(std::f64::consts::PI));
        
        ctx.advance_time();
        assert_eq!(ctx.get_builtin_value("t"), Some(0.001));
        
        ctx.advance_time();
        assert_eq!(ctx.get_builtin_value("t"), Some(0.002));
    }
    
    #[test]
    fn test_dependency_exclusion() {
        assert!(is_dependency_excluded("dt"));
        assert!(is_dependency_excluded("t"));
        assert!(is_dependency_excluded("pi"));
        assert!(is_dependency_excluded("e"));
        assert!(!is_dependency_excluded("voltage"));
        assert!(!is_dependency_excluded("current"));
    }
}