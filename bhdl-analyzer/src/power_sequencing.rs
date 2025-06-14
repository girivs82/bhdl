//! Power sequencing logic generator for BHDL circuit flow paradigm
//!
//! This module generates intelligent power-up and power-down sequences
//! based on power domain dependencies and timing constraints.

use crate::types::SourceLocation;
use std::collections::{HashMap, HashSet, VecDeque};
use std::fmt;

/// Power sequence step
#[derive(Debug, Clone, PartialEq)]
pub struct PowerSequenceStep {
    pub step_id: u32,
    pub domain_name: String,
    pub action: PowerAction,
    pub delay_ms: f64,
    pub condition: Option<PowerCondition>,
    pub timeout_ms: Option<f64>,
    pub error_action: Option<ErrorAction>,
}

/// Power control actions
#[derive(Debug, Clone, PartialEq)]
pub enum PowerAction {
    Enable,
    Disable,
    WaitForStable,
    CheckVoltage,
    SetVoltage(f64),
    RampVoltage { from: f64, to: f64, rate_v_per_ms: f64 },
    Delay(f64),
}

/// Power sequence conditions
#[derive(Debug, Clone, PartialEq)]
pub enum PowerCondition {
    VoltageStable { domain: String, tolerance: f64 },
    VoltageLevel { domain: String, min_voltage: f64 },
    CurrentLimit { domain: String, max_current: f64 },
    Temperature { max_temp: f64 },
    ExternalSignal { signal_name: String, level: bool },
    TimerExpired { duration_ms: f64 },
}

/// Error handling actions
#[derive(Debug, Clone, PartialEq)]
pub enum ErrorAction {
    Retry { max_attempts: u32, delay_ms: f64 },
    Shutdown,
    ContinueWithWarning,
    JumpToStep(u32),
}

/// Power domain for sequencing
#[derive(Debug, Clone)]
pub struct PowerDomain {
    pub name: String,
    pub voltage: f64,
    pub max_current: f64,
    pub enable_signal: Option<String>,
    pub good_signal: Option<String>,
    pub dependencies: Vec<String>,
    pub startup_delay_ms: f64,
    pub shutdown_delay_ms: f64,
    pub ramp_rate_v_per_ms: Option<f64>,
    pub sequence_priority: u32,
    pub critical: bool, // System cannot function without this domain
}

/// Power sequencing constraints
#[derive(Debug, Clone)]
pub struct SequencingConstraints {
    pub max_inrush_current: f64,
    pub max_total_power: f64,
    pub max_startup_time_ms: f64,
    pub temperature_monitoring: bool,
    pub redundancy_required: bool,
}

/// Power sequence generator
#[derive(Debug)]
pub struct PowerSequenceGenerator {
    pub domains: HashMap<String, PowerDomain>,
    pub constraints: SequencingConstraints,
    pub startup_sequence: Vec<PowerSequenceStep>,
    pub shutdown_sequence: Vec<PowerSequenceStep>,
    pub error_recovery_sequences: HashMap<String, Vec<PowerSequenceStep>>,
    pub warnings: Vec<String>,
}

impl PowerSequenceGenerator {
    /// Create a new power sequence generator
    pub fn new() -> Self {
        Self {
            domains: HashMap::new(),
            constraints: SequencingConstraints::default(),
            startup_sequence: Vec::new(),
            shutdown_sequence: Vec::new(),
            error_recovery_sequences: HashMap::new(),
            warnings: Vec::new(),
        }
    }

    /// Add a power domain
    pub fn add_domain(&mut self, domain: PowerDomain) {
        self.domains.insert(domain.name.clone(), domain);
    }

    /// Generate complete power sequences
    pub fn generate_sequences(&mut self) -> Result<(), SequencingError> {
        // Validate domain dependencies
        self.validate_dependencies()?;
        
        // Generate startup sequence
        self.generate_startup_sequence()?;
        
        // Generate shutdown sequence (reverse of startup)
        self.generate_shutdown_sequence()?;
        
        // Generate error recovery sequences
        self.generate_error_recovery_sequences()?;
        
        Ok(())
    }

    /// Validate power domain dependencies for circular references
    fn validate_dependencies(&self) -> Result<(), SequencingError> {
        for domain in self.domains.values() {
            let mut visited = HashSet::new();
            let mut path = Vec::new();
            if self.has_circular_dependency(&domain.name, &mut visited, &mut path) {
                return Err(SequencingError::CircularDependency {
                    domains: path,
                    location: SourceLocation::unknown(),
                });
            }
        }
        Ok(())
    }

    /// Check for circular dependencies using DFS
    fn has_circular_dependency(
        &self,
        domain_name: &str,
        visited: &mut HashSet<String>,
        path: &mut Vec<String>,
    ) -> bool {
        if path.contains(&domain_name.to_string()) {
            path.push(domain_name.to_string());
            return true;
        }

        if visited.contains(domain_name) {
            return false;
        }

        visited.insert(domain_name.to_string());
        path.push(domain_name.to_string());

        if let Some(domain) = self.domains.get(domain_name) {
            for dep in &domain.dependencies {
                if self.has_circular_dependency(dep, visited, path) {
                    return true;
                }
            }
        }

        path.pop();
        false
    }

    /// Generate startup sequence using topological sort
    fn generate_startup_sequence(&mut self) -> Result<(), SequencingError> {
        self.startup_sequence.clear();
        
        // Topological sort of domains based on dependencies
        let sorted_domains = self.topological_sort()?;
        
        let mut step_id = 1;
        let mut total_startup_time = 0.0;
        
        for domain_name in sorted_domains {
            if let Some(domain) = self.domains.get(&domain_name) {
                // Skip non-controllable domains
                if domain.enable_signal.is_none() {
                    continue;
                }
                
                // Add enable step
                self.startup_sequence.push(PowerSequenceStep {
                    step_id,
                    domain_name: domain.name.clone(),
                    action: PowerAction::Enable,
                    delay_ms: 0.0,
                    condition: None,
                    timeout_ms: Some(1000.0), // 1 second timeout
                    error_action: Some(ErrorAction::Retry { max_attempts: 3, delay_ms: 100.0 }),
                });
                step_id += 1;

                // Add ramp step if needed
                if let Some(ramp_rate) = domain.ramp_rate_v_per_ms {
                    let ramp_time = domain.voltage / ramp_rate;
                    self.startup_sequence.push(PowerSequenceStep {
                        step_id,
                        domain_name: domain.name.clone(),
                        action: PowerAction::RampVoltage {
                            from: 0.0,
                            to: domain.voltage,
                            rate_v_per_ms: ramp_rate,
                        },
                        delay_ms: ramp_time,
                        condition: None,
                        timeout_ms: Some(ramp_time * 2.0),
                        error_action: Some(ErrorAction::Shutdown),
                    });
                    step_id += 1;
                    total_startup_time += ramp_time;
                }

                // Add stability check
                if domain.good_signal.is_some() || domain.startup_delay_ms > 0.0 {
                    let condition = if let Some(good_signal) = &domain.good_signal {
                        Some(PowerCondition::ExternalSignal {
                            signal_name: good_signal.clone(),
                            level: true,
                        })
                    } else {
                        Some(PowerCondition::VoltageStable {
                            domain: domain.name.clone(),
                            tolerance: 0.05, // 5% tolerance
                        })
                    };

                    self.startup_sequence.push(PowerSequenceStep {
                        step_id,
                        domain_name: domain.name.clone(),
                        action: PowerAction::WaitForStable,
                        delay_ms: domain.startup_delay_ms,
                        condition,
                        timeout_ms: Some(domain.startup_delay_ms * 3.0),
                        error_action: if domain.critical {
                            Some(ErrorAction::Shutdown)
                        } else {
                            Some(ErrorAction::ContinueWithWarning)
                        },
                    });
                    step_id += 1;
                    total_startup_time += domain.startup_delay_ms;
                }

                // Add current limit check
                self.startup_sequence.push(PowerSequenceStep {
                    step_id,
                    domain_name: domain.name.clone(),
                    action: PowerAction::CheckVoltage,
                    delay_ms: 10.0, // 10ms check
                    condition: Some(PowerCondition::CurrentLimit {
                        domain: domain.name.clone(),
                        max_current: domain.max_current,
                    }),
                    timeout_ms: Some(100.0),
                    error_action: Some(ErrorAction::Shutdown),
                });
                step_id += 1;
                total_startup_time += 10.0;
            }
        }

        // Check if total startup time exceeds constraints
        if total_startup_time > self.constraints.max_startup_time_ms {
            self.warnings.push(format!(
                "Startup sequence time ({:.1}ms) exceeds constraint ({:.1}ms)",
                total_startup_time, self.constraints.max_startup_time_ms
            ));
        }

        Ok(())
    }

    /// Generate shutdown sequence (reverse order of startup)
    fn generate_shutdown_sequence(&mut self) -> Result<(), SequencingError> {
        self.shutdown_sequence.clear();
        
        // Get domains in reverse startup order
        let mut domain_order: Vec<String> = self.startup_sequence.iter()
            .filter(|step| step.action == PowerAction::Enable)
            .map(|step| step.domain_name.clone())
            .collect();
        domain_order.reverse();

        let mut step_id = 1;
        
        for domain_name in domain_order {
            if let Some(domain) = self.domains.get(&domain_name) {
                // Add disable step
                self.shutdown_sequence.push(PowerSequenceStep {
                    step_id,
                    domain_name: domain.name.clone(),
                    action: PowerAction::Disable,
                    delay_ms: domain.shutdown_delay_ms,
                    condition: None,
                    timeout_ms: Some(domain.shutdown_delay_ms * 2.0),
                    error_action: Some(ErrorAction::ContinueWithWarning),
                });
                step_id += 1;
            }
        }

        Ok(())
    }

    /// Generate error recovery sequences
    fn generate_error_recovery_sequences(&mut self) -> Result<(), SequencingError> {
        self.error_recovery_sequences.clear();

        // Generate recovery for each critical domain
        for domain in self.domains.values() {
            if domain.critical {
                let mut recovery_sequence = Vec::new();
                
                // Shutdown everything
                recovery_sequence.push(PowerSequenceStep {
                    step_id: 1,
                    domain_name: "ALL".to_string(),
                    action: PowerAction::Disable,
                    delay_ms: 100.0,
                    condition: None,
                    timeout_ms: Some(1000.0),
                    error_action: None,
                });

                // Wait for stabilization
                recovery_sequence.push(PowerSequenceStep {
                    step_id: 2,
                    domain_name: "SYSTEM".to_string(),
                    action: PowerAction::Delay(500.0),
                    delay_ms: 500.0,
                    condition: None,
                    timeout_ms: None,
                    error_action: None,
                });

                // Restart sequence
                recovery_sequence.push(PowerSequenceStep {
                    step_id: 3,
                    domain_name: "SYSTEM".to_string(),
                    action: PowerAction::Enable,
                    delay_ms: 0.0,
                    condition: None,
                    timeout_ms: Some(5000.0),
                    error_action: Some(ErrorAction::Shutdown),
                });

                self.error_recovery_sequences.insert(
                    format!("{}_RECOVERY", domain.name),
                    recovery_sequence
                );
            }
        }

        Ok(())
    }

    /// Topological sort of domains based on dependencies
    fn topological_sort(&self) -> Result<Vec<String>, SequencingError> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        let mut graph: HashMap<String, Vec<String>> = HashMap::new();

        // Initialize
        for domain_name in self.domains.keys() {
            in_degree.insert(domain_name.clone(), 0);
            graph.insert(domain_name.clone(), Vec::new());
        }

        // Build graph and calculate in-degrees
        for domain in self.domains.values() {
            for dep in &domain.dependencies {
                if let Some(dep_edges) = graph.get_mut(dep) {
                    dep_edges.push(domain.name.clone());
                    *in_degree.get_mut(&domain.name).unwrap() += 1;
                }
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<String> = VecDeque::new();
        for (domain, &degree) in &in_degree {
            if degree == 0 {
                queue.push_back(domain.clone());
            }
        }

        let mut result = Vec::new();
        while let Some(domain) = queue.pop_front() {
            result.push(domain.clone());

            if let Some(neighbors) = graph.get(&domain) {
                for neighbor in neighbors {
                    let degree = in_degree.get_mut(neighbor).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push_back(neighbor.clone());
                    }
                }
            }
        }

        if result.len() != self.domains.len() {
            return Err(SequencingError::CircularDependency {
                domains: vec!["Unknown".to_string()],
                location: SourceLocation::unknown(),
            });
        }

        Ok(result)
    }

    /// Generate BHDL code for power sequences
    pub fn generate_bhdl_code(&self) -> String {
        let mut code = String::new();

        // Generate startup sequence
        if !self.startup_sequence.is_empty() {
            code.push_str("// Auto-generated power startup sequence\n");
            code.push_str("power_startup_sequence {\n");
            
            for step in &self.startup_sequence {
                code.push_str(&format!("  // Step {}: {}\n", step.step_id, step.domain_name));
                
                match &step.action {
                    PowerAction::Enable => {
                        code.push_str(&format!("  {}.enable();\n", step.domain_name));
                    }
                    PowerAction::WaitForStable => {
                        if let Some(condition) = &step.condition {
                            match condition {
                                PowerCondition::VoltageStable { domain, tolerance } => {
                                    code.push_str(&format!("  wait_for({}.voltage_stable({:.3}));\n", 
                                                         domain, tolerance));
                                }
                                PowerCondition::ExternalSignal { signal_name, level } => {
                                    code.push_str(&format!("  wait_for({} == {});\n", 
                                                         signal_name, level));
                                }
                                _ => {
                                    code.push_str(&format!("  delay({}ms);\n", step.delay_ms));
                                }
                            }
                        } else {
                            code.push_str(&format!("  delay({}ms);\n", step.delay_ms));
                        }
                    }
                    PowerAction::CheckVoltage => {
                        code.push_str(&format!("  check({}.voltage_ok());\n", step.domain_name));
                    }
                    PowerAction::RampVoltage { from, to, rate_v_per_ms } => {
                        code.push_str(&format!("  {}.ramp_voltage({}V, {}V, {}V/ms);\n", 
                                             step.domain_name, from, to, rate_v_per_ms));
                    }
                    PowerAction::Delay(ms) => {
                        code.push_str(&format!("  delay({}ms);\n", ms));
                    }
                    _ => {}
                }

                if step.timeout_ms.is_some() || step.error_action.is_some() {
                    code.push_str("  // Error handling: ");
                    if let Some(timeout) = step.timeout_ms {
                        code.push_str(&format!("timeout {}ms", timeout));
                    }
                    if let Some(error_action) = &step.error_action {
                        match error_action {
                            ErrorAction::Retry { max_attempts, delay_ms } => {
                                code.push_str(&format!(", retry {} times with {}ms delay", 
                                                     max_attempts, delay_ms));
                            }
                            ErrorAction::Shutdown => {
                                code.push_str(", shutdown on error");
                            }
                            ErrorAction::ContinueWithWarning => {
                                code.push_str(", continue with warning");
                            }
                            _ => {}
                        }
                    }
                    code.push('\n');
                }
                
                code.push('\n');
            }
            
            code.push_str("}\n\n");
        }

        // Generate shutdown sequence
        if !self.shutdown_sequence.is_empty() {
            code.push_str("// Auto-generated power shutdown sequence\n");
            code.push_str("power_shutdown_sequence {\n");
            
            for step in &self.shutdown_sequence {
                code.push_str(&format!("  {}.disable();\n", step.domain_name));
                if step.delay_ms > 0.0 {
                    code.push_str(&format!("  delay({}ms);\n", step.delay_ms));
                }
            }
            
            code.push_str("}\n\n");
        }

        code
    }
}

/// Sequencing error types
#[derive(Debug, Clone)]
pub enum SequencingError {
    CircularDependency {
        domains: Vec<String>,
        location: SourceLocation,
    },
    InvalidConstraint {
        constraint: String,
        message: String,
        location: SourceLocation,
    },
    TimingViolation {
        domain: String,
        required_time: f64,
        available_time: f64,
        location: SourceLocation,
    },
}

impl fmt::Display for SequencingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SequencingError::CircularDependency { domains, location } => {
                write!(f, "Circular power dependency: {} at {}:{}", 
                       domains.join(" -> "), location.line, location.column)
            }
            SequencingError::InvalidConstraint { constraint, message, location } => {
                write!(f, "Invalid constraint '{}': {} at {}:{}", 
                       constraint, message, location.line, location.column)
            }
            SequencingError::TimingViolation { domain, required_time, available_time, location } => {
                write!(f, "Timing violation in domain '{}': required {:.1}ms, available {:.1}ms at {}:{}", 
                       domain, required_time, available_time, location.line, location.column)
            }
        }
    }
}

impl std::error::Error for SequencingError {}

impl Default for SequencingConstraints {
    fn default() -> Self {
        Self {
            max_inrush_current: 2.0, // 2A max inrush
            max_total_power: 10.0,   // 10W max power
            max_startup_time_ms: 5000.0, // 5 second max startup
            temperature_monitoring: true,
            redundancy_required: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_sequence_generator() {
        let mut generator = PowerSequenceGenerator::new();
        
        // Add test domains
        let mut domain1 = PowerDomain {
            name: "VCC_3V3".to_string(),
            voltage: 3.3,
            max_current: 1.0,
            enable_signal: Some("VCC_3V3_EN".to_string()),
            good_signal: Some("VCC_3V3_GOOD".to_string()),
            dependencies: vec!["USB_5V".to_string()],
            startup_delay_ms: 10.0,
            shutdown_delay_ms: 5.0,
            ramp_rate_v_per_ms: None,
            sequence_priority: 2,
            critical: true,
        };
        
        let mut domain2 = PowerDomain {
            name: "USB_5V".to_string(),
            voltage: 5.0,
            max_current: 0.5,
            enable_signal: None, // Always on
            good_signal: None,
            dependencies: vec![],
            startup_delay_ms: 0.0,
            shutdown_delay_ms: 0.0,
            ramp_rate_v_per_ms: None,
            sequence_priority: 1,
            critical: true,
        };
        
        generator.add_domain(domain1);
        generator.add_domain(domain2);
        
        // Generate sequences
        let result = generator.generate_sequences();
        assert!(result.is_ok(), "Sequence generation should succeed");
        
        // Check that startup sequence was generated
        assert!(!generator.startup_sequence.is_empty(), "Startup sequence should not be empty");
        
        // Check that shutdown sequence was generated
        assert!(!generator.shutdown_sequence.is_empty(), "Shutdown sequence should not be empty");
    }

    #[test]
    fn test_circular_dependency_detection() {
        let mut generator = PowerSequenceGenerator::new();
        
        // Create circular dependency: A -> B -> A
        let domain_a = PowerDomain {
            name: "A".to_string(),
            voltage: 3.3,
            max_current: 1.0,
            enable_signal: Some("A_EN".to_string()),
            good_signal: None,
            dependencies: vec!["B".to_string()],
            startup_delay_ms: 1.0,
            shutdown_delay_ms: 1.0,
            ramp_rate_v_per_ms: None,
            sequence_priority: 1,
            critical: false,
        };
        
        let domain_b = PowerDomain {
            name: "B".to_string(),
            voltage: 1.8,
            max_current: 0.5,
            enable_signal: Some("B_EN".to_string()),
            good_signal: None,
            dependencies: vec!["A".to_string()],
            startup_delay_ms: 1.0,
            shutdown_delay_ms: 1.0,
            ramp_rate_v_per_ms: None,
            sequence_priority: 2,
            critical: false,
        };
        
        generator.add_domain(domain_a);
        generator.add_domain(domain_b);
        
        // Should detect circular dependency
        let result = generator.generate_sequences();
        assert!(result.is_err(), "Should detect circular dependency");
    }

    #[test]
    fn test_topological_sort() {
        let mut generator = PowerSequenceGenerator::new();
        
        // A -> C, B -> C (C depends on both A and B)
        let domain_a = PowerDomain {
            name: "A".to_string(),
            voltage: 5.0,
            max_current: 1.0,
            enable_signal: Some("A_EN".to_string()),
            good_signal: None,
            dependencies: vec![],
            startup_delay_ms: 1.0,
            shutdown_delay_ms: 1.0,
            ramp_rate_v_per_ms: None,
            sequence_priority: 1,
            critical: false,
        };
        
        let domain_b = PowerDomain {
            name: "B".to_string(),
            voltage: 3.3,
            max_current: 1.0,
            enable_signal: Some("B_EN".to_string()),
            good_signal: None,
            dependencies: vec![],
            startup_delay_ms: 1.0,
            shutdown_delay_ms: 1.0,
            ramp_rate_v_per_ms: None,
            sequence_priority: 2,
            critical: false,
        };
        
        let domain_c = PowerDomain {
            name: "C".to_string(),
            voltage: 1.8,
            max_current: 0.5,
            enable_signal: Some("C_EN".to_string()),
            good_signal: None,
            dependencies: vec!["A".to_string(), "B".to_string()],
            startup_delay_ms: 1.0,
            shutdown_delay_ms: 1.0,
            ramp_rate_v_per_ms: None,
            sequence_priority: 3,
            critical: false,
        };
        
        generator.add_domain(domain_a);
        generator.add_domain(domain_b);
        generator.add_domain(domain_c);
        
        let sorted = generator.topological_sort();
        assert!(sorted.is_ok(), "Topological sort should succeed");
        
        let order = sorted.unwrap();
        let c_index = order.iter().position(|x| x == "C").unwrap();
        let a_index = order.iter().position(|x| x == "A").unwrap();
        let b_index = order.iter().position(|x| x == "B").unwrap();
        
        // C should come after both A and B
        assert!(c_index > a_index, "C should come after A");
        assert!(c_index > b_index, "C should come after B");
    }
}