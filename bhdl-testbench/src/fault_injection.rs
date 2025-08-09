//! Fault injection system for testbenches
//! 
//! Provides capabilities for injecting component failures and parameter drift
//! to test circuit robustness and safety mechanisms.

use std::collections::HashMap;
use anyhow::{Result, anyhow};
use bhdl_spice::{ComponentModel, ElectricalLimits};

/// Type of fault to inject
#[derive(Debug, Clone)]
pub enum FaultType {
    /// Component parameter override
    ParameterOverride {
        parameter: String,
        value: FaultValue,
    },
    /// Complete component failure
    ComponentFailure {
        mode: FailureMode,
    },
    /// Connection fault
    ConnectionFault {
        fault_type: ConnectionFaultType,
    },
}

/// Fault values for parameter overrides
#[derive(Debug, Clone)]
pub enum FaultValue {
    /// Absolute value
    Absolute(f64),
    /// Relative multiplier (1.0 = nominal)
    Relative(f64),
    /// Short circuit (~0 ohms)
    Short,
    /// Open circuit (~infinite ohms)
    Open,
    /// Percentage drift from nominal
    Drift(f64),
}

/// Component failure modes
#[derive(Debug, Clone)]
pub enum FailureMode {
    ShortCircuit,
    OpenCircuit,
    Degraded { factor: f64 },
}

/// Connection fault types
#[derive(Debug, Clone)]
pub enum ConnectionFaultType {
    Open,
    ShortToGround,
    ShortToSupply,
    Intermittent { probability: f64 },
}

/// Fault scenario definition
#[derive(Debug, Clone)]
pub struct FaultScenario {
    pub name: String,
    pub description: String,
    pub faults: HashMap<String, FaultType>,  // component_name -> fault
    pub trigger_time: Option<f64>,
    pub expected_behavior: Option<ExpectedBehavior>,
}

/// Expected behavior after fault injection
#[derive(Debug, Clone)]
pub struct ExpectedBehavior {
    pub should_fail_safe: bool,
    pub max_stress: HashMap<String, StressLimit>,
    pub protection_should_trigger: Vec<String>,
}

/// Stress limits for components
#[derive(Debug, Clone)]
pub struct StressLimit {
    pub max_current: Option<f64>,
    pub max_voltage: Option<f64>,
    pub max_power: Option<f64>,
    pub max_temperature: Option<f64>,
}

/// Fault injection engine
pub struct FaultInjector {
    pub(crate) scenarios: HashMap<String, FaultScenario>,
    active_faults: Vec<(String, FaultType)>,  // (component, fault)
}

impl FaultInjector {
    pub fn new() -> Self {
        Self {
            scenarios: HashMap::new(),
            active_faults: Vec::new(),
        }
    }
    
    /// Add a fault scenario
    pub fn add_scenario(&mut self, scenario: FaultScenario) {
        self.scenarios.insert(scenario.name.clone(), scenario);
    }
    
    /// Load a scenario and prepare faults
    pub fn load_scenario(&mut self, name: &str) -> Result<&FaultScenario> {
        let scenario = self.scenarios.get(name)
            .ok_or_else(|| anyhow!("Fault scenario '{}' not found", name))?;
        
        // Clear previous faults
        self.active_faults.clear();
        
        // Load new faults
        for (component, fault) in &scenario.faults {
            self.active_faults.push((component.clone(), fault.clone()));
        }
        
        Ok(scenario)
    }
    
    /// Apply faults to component models
    pub fn apply_to_component_model(
        &self,
        component_name: &str,
        model: &mut ComponentModel,
    ) -> Result<()> {
        // Check if this component has an active fault
        for (fault_component, fault_type) in &self.active_faults {
            if fault_component == component_name {
                Self::apply_fault_to_model(model, fault_type)?;
            }
        }
        Ok(())
    }
    
    /// Apply a specific fault to a component model
    fn apply_fault_to_model(model: &mut ComponentModel, fault: &FaultType) -> Result<()> {
        match fault {
            FaultType::ParameterOverride { parameter, value } => {
                match (model, parameter.as_str(), value) {
                    // Resistor faults
                    (ComponentModel::Resistor { resistance, .. }, "resistance", FaultValue::Short) => {
                        *resistance = 1e-3;  // 1 milliohm
                    }
                    (ComponentModel::Resistor { resistance, .. }, "resistance", FaultValue::Open) => {
                        *resistance = 1e12;  // 1 teraohm
                    }
                    (ComponentModel::Resistor { resistance, .. }, "resistance", FaultValue::Drift(pct)) => {
                        *resistance *= 1.0 + (pct / 100.0);
                    }
                    (ComponentModel::Resistor { resistance, .. }, "resistance", FaultValue::Absolute(val)) => {
                        *resistance = *val;
                    }
                    
                    // Capacitor faults
                    (ComponentModel::Capacitor { capacitance, esr, .. }, "capacitance", FaultValue::Short) => {
                        // Model as very large capacitance with low ESR
                        *capacitance = 1.0;  // 1F
                        *esr = Some(1e-3);   // 1 milliohm
                    }
                    (ComponentModel::Capacitor { capacitance, .. }, "capacitance", FaultValue::Open) => {
                        // Model as very small capacitance
                        *capacitance = 1e-15;  // 1fF
                    }
                    (ComponentModel::Capacitor { esr, .. }, "esr", FaultValue::Absolute(val)) => {
                        *esr = Some(*val);
                    }
                    
                    // LED faults
                    (ComponentModel::LED { forward_voltage, .. }, "forward_voltage", FaultValue::Drift(pct)) => {
                        *forward_voltage *= 1.0 + (pct / 100.0);
                    }
                    
                    _ => {
                        return Err(anyhow!("Unsupported fault combination for parameter '{}' on component type", parameter));
                    }
                }
            }
            
            FaultType::ComponentFailure { mode } => {
                match mode {
                    FailureMode::ShortCircuit => {
                        // Replace with low resistance
                        *model = ComponentModel::Resistor {
                            resistance: 1e-3,
                            tolerance: 0.0,
                            limits: ElectricalLimits::default(),
                        };
                    }
                    FailureMode::OpenCircuit => {
                        // Replace with high resistance
                        *model = ComponentModel::Resistor {
                            resistance: 1e12,
                            tolerance: 0.0,
                            limits: ElectricalLimits::default(),
                        };
                    }
                    FailureMode::Degraded { factor } => {
                        // Degrade key parameters
                        match model {
                            ComponentModel::Resistor { resistance, .. } => {
                                *resistance *= factor;
                            }
                            ComponentModel::Capacitor { capacitance, .. } => {
                                *capacitance *= factor;
                            }
                            ComponentModel::LED { forward_voltage, dynamic_resistance, .. } => {
                                *forward_voltage *= factor;
                                *dynamic_resistance *= factor;
                            }
                            _ => {}
                        }
                    }
                }
            }
            
            FaultType::ConnectionFault { .. } => {
                // Connection faults need to be handled at circuit level
                return Err(anyhow!("Connection faults must be handled at circuit level"));
            }
        }
        
        Ok(())
    }
    
    /// Create common fault scenarios
    pub fn create_standard_scenarios() -> Vec<FaultScenario> {
        vec![
            // Resistor short circuit
            FaultScenario {
                name: "resistor_short".to_string(),
                description: "Current limiting resistor fails short".to_string(),
                faults: {
                    let mut faults = HashMap::new();
                    faults.insert("R1".to_string(), FaultType::ParameterOverride {
                        parameter: "resistance".to_string(),
                        value: FaultValue::Short,
                    });
                    faults
                },
                trigger_time: Some(0.01),  // 10ms
                expected_behavior: Some(ExpectedBehavior {
                    should_fail_safe: true,
                    max_stress: {
                        let mut stress = HashMap::new();
                        stress.insert("LED1".to_string(), StressLimit {
                            max_current: Some(0.1),  // 100mA max
                            max_voltage: None,
                            max_power: Some(0.5),    // 500mW max
                            max_temperature: None,
                        });
                        stress
                    },
                    protection_should_trigger: vec![],
                }),
            },
            
            // Resistor drift
            FaultScenario {
                name: "resistor_drift_high".to_string(),
                description: "Resistor value drifts +10%".to_string(),
                faults: {
                    let mut faults = HashMap::new();
                    faults.insert("R1".to_string(), FaultType::ParameterOverride {
                        parameter: "resistance".to_string(),
                        value: FaultValue::Drift(10.0),
                    });
                    faults
                },
                trigger_time: None,
                expected_behavior: None,
            },
            
            // LED open circuit
            FaultScenario {
                name: "led_open".to_string(),
                description: "LED fails open circuit".to_string(),
                faults: {
                    let mut faults = HashMap::new();
                    faults.insert("LED1".to_string(), FaultType::ComponentFailure {
                        mode: FailureMode::OpenCircuit,
                    });
                    faults
                },
                trigger_time: Some(0.02),  // 20ms
                expected_behavior: Some(ExpectedBehavior {
                    should_fail_safe: true,
                    max_stress: HashMap::new(),
                    protection_should_trigger: vec![],
                }),
            },
            
            // Multiple component degradation
            FaultScenario {
                name: "aging_simulation".to_string(),
                description: "Simulate component aging".to_string(),
                faults: {
                    let mut faults = HashMap::new();
                    faults.insert("R1".to_string(), FaultType::ParameterOverride {
                        parameter: "resistance".to_string(),
                        value: FaultValue::Drift(5.0),  // +5%
                    });
                    faults.insert("LED1".to_string(), FaultType::ParameterOverride {
                        parameter: "forward_voltage".to_string(),
                        value: FaultValue::Drift(-5.0),  // -5%
                    });
                    faults
                },
                trigger_time: None,
                expected_behavior: None,
            },
        ]
    }
}

/// Results from fault analysis
#[derive(Debug)]
pub struct FaultAnalysisResult {
    pub scenario_name: String,
    pub baseline_values: HashMap<crate::SignalRef, f64>,
    pub faulted_values: HashMap<crate::SignalRef, f64>,
    pub stress_violations: Vec<StressViolation>,
    pub cascade_failures: Vec<CascadeFailure>,
    pub protection_triggered: Vec<String>,
    pub safety_passed: bool,
}

#[derive(Debug)]
pub struct StressViolation {
    pub component: String,
    pub stress_type: String,
    pub actual_value: f64,
    pub limit_value: f64,
    pub severity: String,
}

#[derive(Debug)]
pub struct CascadeFailure {
    pub initial_fault: String,
    pub affected_component: String,
    pub failure_mechanism: String,
    pub time_to_failure: Option<f64>,
}

impl FaultAnalysisResult {
    pub fn generate_report(&self) -> String {
        let mut report = String::new();
        
        report.push_str(&format!("=== Fault Analysis Report: {} ===\n\n", self.scenario_name));
        
        report.push_str("Baseline vs Faulted Values:\n");
        for (signal, baseline) in &self.baseline_values {
            if let Some(faulted) = self.faulted_values.get(signal) {
                let change = (faulted - baseline) / baseline * 100.0;
                report.push_str(&format!("  {}: {:.3} -> {:.3} ({:+.1}%)\n", 
                    signal.to_string(), baseline, faulted, change));
            }
        }
        
        if !self.stress_violations.is_empty() {
            report.push_str("\nStress Violations:\n");
            for violation in &self.stress_violations {
                report.push_str(&format!("  [{}] {} {}: {:.3} (limit: {:.3})\n",
                    violation.severity,
                    violation.component,
                    violation.stress_type,
                    violation.actual_value,
                    violation.limit_value
                ));
            }
        }
        
        if !self.cascade_failures.is_empty() {
            report.push_str("\nCascade Failures Detected:\n");
            for cascade in &self.cascade_failures {
                report.push_str(&format!("  {} -> {} ({})\n",
                    cascade.initial_fault,
                    cascade.affected_component,
                    cascade.failure_mechanism
                ));
            }
        }
        
        report.push_str(&format!("\nSafety Status: {}\n", 
            if self.safety_passed { "PASSED" } else { "FAILED" }));
        
        report
    }
}