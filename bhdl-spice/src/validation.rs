//! SPICE validation mode for user-specified values
//! 
//! This module validates that user-specified component values meet
//! electrical constraints and safety requirements through simulation.

use std::collections::HashMap;
use anyhow::Result;
use log::{warn, info, error};

use crate::{
    Circuit, NonlinearDcAnalysis, AnalysisResult,
    SpiceError, Severity,
    ComponentModel, ElectricalLimits,
};

/// Validation constraint types
#[derive(Debug, Clone)]
pub enum ValidationConstraint {
    /// Maximum current through component
    MaxCurrent { limit: f64, derate: f64 },
    /// Maximum voltage across component
    MaxVoltage { limit: f64, derate: f64 },
    /// Maximum power dissipation
    MaxPower { limit: f64, derate: f64 },
    /// Maximum temperature rise
    MaxTemperatureRise { limit: f64, ambient: f64 },
    /// Minimum/maximum resistance range
    ResistanceRange { min: f64, max: f64 },
    /// Minimum/maximum capacitance range
    CapacitanceRange { min: f64, max: f64 },
    /// Operating point requirement
    OperatingPoint { parameter: String, target: f64, tolerance: f64 },
}

/// Validation result for a component
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub component: String,
    pub passed: bool,
    pub violations: Vec<ConstraintViolation>,
    pub warnings: Vec<ValidationWarning>,
    pub operating_point: OperatingPoint,
}

/// Constraint violation details
#[derive(Debug, Clone)]
pub struct ConstraintViolation {
    pub constraint_type: String,
    pub limit: f64,
    pub actual: f64,
    pub margin: f64,
    pub severity: Severity,
}

/// Validation warning
#[derive(Debug, Clone)]
pub struct ValidationWarning {
    pub message: String,
    pub severity: Severity,
}

/// Component operating point
#[derive(Debug, Clone)]
pub struct OperatingPoint {
    pub voltage: f64,
    pub current: f64,
    pub power: f64,
    pub temperature_rise: f64,
}

/// SPICE validation engine
pub struct ValidationEngine {
    constraints: HashMap<String, Vec<ValidationConstraint>>,
    models: HashMap<String, ComponentModel>,
    ambient_temp: f64,
}

impl ValidationEngine {
    pub fn new() -> Self {
        Self {
            constraints: HashMap::new(),
            models: HashMap::new(),
            ambient_temp: 25.0, // Default 25°C
        }
    }
    
    /// Set ambient temperature for thermal calculations
    pub fn set_ambient_temperature(&mut self, temp: f64) {
        self.ambient_temp = temp;
    }
    
    /// Add component model for validation
    pub fn add_model(&mut self, name: String, model: ComponentModel) {
        self.models.insert(name, model);
    }
    
    /// Add validation constraint for a component
    pub fn add_constraint(&mut self, component: &str, constraint: ValidationConstraint) {
        self.constraints
            .entry(component.to_string())
            .or_insert_with(Vec::new)
            .push(constraint);
    }
    
    /// Add standard constraints based on component type
    pub fn add_standard_constraints(&mut self, component: &str, component_type: &str, value: f64) {
        match component_type.to_lowercase().as_str() {
            "resistor" | "res" => {
                // Standard resistor constraints
                self.add_constraint(component, ValidationConstraint::MaxPower { 
                    limit: self.infer_power_rating(value), 
                    derate: 0.7  // 70% derating for reliability
                });
                self.add_constraint(component, ValidationConstraint::MaxTemperatureRise { 
                    limit: 50.0,  // 50°C rise max
                    ambient: self.ambient_temp 
                });
            }
            
            "capacitor" | "cap" => {
                // Standard capacitor constraints
                self.add_constraint(component, ValidationConstraint::MaxVoltage { 
                    limit: self.infer_voltage_rating(value), 
                    derate: 0.8  // 80% voltage derating
                });
            }
            
            "led" => {
                // LED constraints
                self.add_constraint(component, ValidationConstraint::MaxCurrent { 
                    limit: 0.030,  // 30mA typical max
                    derate: 0.8 
                });
                self.add_constraint(component, ValidationConstraint::MaxPower { 
                    limit: 0.1,  // 100mW typical
                    derate: 0.7 
                });
            }
            
            "diode" => {
                // Diode constraints
                let (current_limit, voltage_limit) = if let Some(model) = self.models.get(component) {
                    if let ComponentModel::Diode { limits, .. } = model {
                        (limits.max_current, limits.max_voltage)
                    } else {
                        (None, None)
                    }
                } else {
                    (None, None)
                };
                
                if let Some(current_max) = current_limit {
                    self.add_constraint(component, ValidationConstraint::MaxCurrent { 
                        limit: current_max, 
                        derate: 0.8 
                    });
                }
                if let Some(voltage_max) = voltage_limit {
                    self.add_constraint(component, ValidationConstraint::MaxVoltage { 
                        limit: voltage_max, 
                        derate: 0.8 
                    });
                }
            }
            
            _ => {
                // Generic constraints
                info!("No standard constraints for component type: {}", component_type);
            }
        }
    }
    
    /// Validate all components in circuit
    pub fn validate(&self, circuit: &Circuit) -> Result<Vec<ValidationResult>> {
        info!("Starting SPICE validation of user-specified values");
        
        // Run circuit analysis
        let mut analysis = NonlinearDcAnalysis::new(circuit.clone());
        let result = analysis.analyze()?;
        
        let mut validation_results = Vec::new();
        
        // Validate each component with constraints
        for (component, constraints) in &self.constraints {
            let validation = self.validate_component(circuit, &result, component, constraints)?;
            validation_results.push(validation);
        }
        
        // Check components without explicit constraints
        for (_, branch) in circuit.branches() {
            if !self.constraints.contains_key(&branch.name) {
                let validation = self.validate_unconstrained(circuit, &result, &branch.name)?;
                validation_results.push(validation);
            }
        }
        
        // Summary
        let failed_count = validation_results.iter()
            .filter(|v| !v.passed)
            .count();
        
        if failed_count > 0 {
            error!("{} components failed validation", failed_count);
        } else {
            info!("All components passed validation");
        }
        
        Ok(validation_results)
    }
    
    /// Validate a single component
    fn validate_component(
        &self,
        circuit: &Circuit,
        result: &AnalysisResult,
        component: &str,
        constraints: &[ValidationConstraint],
    ) -> Result<ValidationResult> {
        let mut violations = Vec::new();
        let mut warnings = Vec::new();
        
        // Get operating point
        let op = self.get_operating_point(circuit, result, component)?;
        
        // Check each constraint
        for constraint in constraints {
            match constraint {
                ValidationConstraint::MaxCurrent { limit, derate } => {
                    let derated_limit = limit * derate;
                    if op.current > derated_limit {
                        violations.push(ConstraintViolation {
                            constraint_type: "MaxCurrent".to_string(),
                            limit: derated_limit,
                            actual: op.current,
                            margin: (op.current - derated_limit) / derated_limit * 100.0,
                            severity: if op.current > *limit { 
                                Severity::Critical 
                            } else { 
                                Severity::Error 
                            },
                        });
                    } else if op.current > derated_limit * 0.9 {
                        warnings.push(ValidationWarning {
                            message: format!("Current {:.1}mA approaching limit {:.1}mA", 
                                op.current * 1000.0, derated_limit * 1000.0),
                            severity: Severity::Warning,
                        });
                    }
                }
                
                ValidationConstraint::MaxVoltage { limit, derate } => {
                    let derated_limit = limit * derate;
                    if op.voltage > derated_limit {
                        violations.push(ConstraintViolation {
                            constraint_type: "MaxVoltage".to_string(),
                            limit: derated_limit,
                            actual: op.voltage,
                            margin: (op.voltage - derated_limit) / derated_limit * 100.0,
                            severity: if op.voltage > *limit { 
                                Severity::Critical 
                            } else { 
                                Severity::Error 
                            },
                        });
                    }
                }
                
                ValidationConstraint::MaxPower { limit, derate } => {
                    let derated_limit = limit * derate;
                    if op.power > derated_limit {
                        violations.push(ConstraintViolation {
                            constraint_type: "MaxPower".to_string(),
                            limit: derated_limit,
                            actual: op.power,
                            margin: (op.power - derated_limit) / derated_limit * 100.0,
                            severity: if op.power > *limit { 
                                Severity::Critical 
                            } else { 
                                Severity::Error 
                            },
                        });
                    } else if op.power > derated_limit * 0.9 {
                        warnings.push(ValidationWarning {
                            message: format!("Power {:.3}W approaching limit {:.3}W", 
                                op.power, derated_limit),
                            severity: Severity::Warning,
                        });
                    }
                }
                
                ValidationConstraint::MaxTemperatureRise { limit, ambient } => {
                    if op.temperature_rise > *limit {
                        violations.push(ConstraintViolation {
                            constraint_type: "MaxTemperatureRise".to_string(),
                            limit: *limit,
                            actual: op.temperature_rise,
                            margin: (op.temperature_rise - limit) / limit * 100.0,
                            severity: Severity::Error,
                        });
                        
                        let junction_temp = ambient + op.temperature_rise;
                        if junction_temp > 125.0 {
                            warnings.push(ValidationWarning {
                                message: format!("Junction temperature {:.0}°C exceeds safe operating area", 
                                    junction_temp),
                                severity: Severity::Critical,
                            });
                        }
                    }
                }
                
                _ => {
                    // Other constraints not implemented yet
                }
            }
        }
        
        Ok(ValidationResult {
            component: component.to_string(),
            passed: violations.is_empty(),
            violations,
            warnings,
            operating_point: op,
        })
    }
    
    /// Validate component without explicit constraints
    fn validate_unconstrained(
        &self,
        circuit: &Circuit,
        result: &AnalysisResult,
        component: &str,
    ) -> Result<ValidationResult> {
        let op = self.get_operating_point(circuit, result, component)?;
        let mut warnings = Vec::new();
        
        // Basic sanity checks
        if op.power > 1.0 {
            warnings.push(ValidationWarning {
                message: format!("High power dissipation: {:.2}W", op.power),
                severity: Severity::Warning,
            });
        }
        
        if op.current > 1.0 {
            warnings.push(ValidationWarning {
                message: format!("High current: {:.2}A", op.current),
                severity: Severity::Warning,
            });
        }
        
        Ok(ValidationResult {
            component: component.to_string(),
            passed: true,
            violations: Vec::new(),
            warnings,
            operating_point: op,
        })
    }
    
    /// Get component operating point
    fn get_operating_point(
        &self,
        circuit: &Circuit,
        result: &AnalysisResult,
        component: &str,
    ) -> Result<OperatingPoint> {
        let current = circuit.branch_current(component, result)?;
        let voltage = self.get_component_voltage(circuit, result, component)?;
        let power = voltage * current.abs();
        
        // Estimate temperature rise based on power and thermal resistance
        let thermal_resistance = self.estimate_thermal_resistance(component);
        let temperature_rise = power * thermal_resistance;
        
        Ok(OperatingPoint {
            voltage,
            current: current.abs(),
            power,
            temperature_rise,
        })
    }
    
    /// Get voltage across component
    fn get_component_voltage(
        &self,
        circuit: &Circuit,
        result: &AnalysisResult,
        component: &str,
    ) -> Result<f64> {
        let (edge_idx, _) = circuit.get_branch(component)
            .ok_or_else(|| anyhow::anyhow!("Component {} not found", component))?;
        
        let (n1, n2) = circuit.branch_nodes(edge_idx)
            .ok_or_else(|| anyhow::anyhow!("Invalid branch nodes"))?;
        
        let v1 = result.node_voltages.get(&n1).copied().unwrap_or(0.0);
        let v2 = result.node_voltages.get(&n2).copied().unwrap_or(0.0);
        
        Ok((v1 - v2).abs())
    }
    
    /// Infer power rating from resistance value
    fn infer_power_rating(&self, resistance: f64) -> f64 {
        // Common power ratings based on resistance ranges
        if resistance < 1.0 {
            2.0  // Low value resistors often need higher power
        } else if resistance < 100.0 {
            1.0
        } else if resistance < 10_000.0 {
            0.5
        } else if resistance < 100_000.0 {
            0.25
        } else {
            0.125  // High value resistors typically low power
        }
    }
    
    /// Infer voltage rating from capacitance value
    fn infer_voltage_rating(&self, capacitance: f64) -> f64 {
        // Common voltage ratings based on capacitance
        if capacitance > 100e-6 {
            25.0  // Large electrolytics
        } else if capacitance > 10e-6 {
            50.0
        } else if capacitance > 1e-6 {
            50.0
        } else {
            100.0  // Small ceramics can handle higher voltage
        }
    }
    
    /// Estimate thermal resistance for component
    fn estimate_thermal_resistance(&self, component: &str) -> f64 {
        // Rough estimates in °C/W
        if component.starts_with("R") {
            150.0  // Typical SMD resistor
        } else if component.starts_with("D") {
            100.0  // Typical diode
        } else if component.starts_with("Q") {
            50.0   // Transistor with some heatsinking
        } else if component.starts_with("U") {
            30.0   // IC package
        } else {
            100.0  // Default
        }
    }
}

/// Validation report formatter
pub struct ValidationReport;

impl ValidationReport {
    /// Generate human-readable validation report
    pub fn format(results: &[ValidationResult]) -> String {
        let mut report = String::new();
        report.push_str("=== SPICE Validation Report ===\n\n");
        
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed).count();
        let failed = total - passed;
        
        report.push_str(&format!("Total components: {}\n", total));
        report.push_str(&format!("Passed: {} ({:.1}%)\n", passed, passed as f64 / total as f64 * 100.0));
        report.push_str(&format!("Failed: {} ({:.1}%)\n\n", failed, failed as f64 / total as f64 * 100.0));
        
        // Failed components first
        if failed > 0 {
            report.push_str("FAILED COMPONENTS:\n");
            report.push_str("-----------------\n");
            for result in results.iter().filter(|r| !r.passed) {
                report.push_str(&format!("\n{}: FAILED\n", result.component));
                report.push_str(&format!("  Operating point: {:.2}V, {:.2}mA, {:.3}W\n",
                    result.operating_point.voltage,
                    result.operating_point.current * 1000.0,
                    result.operating_point.power));
                
                for violation in &result.violations {
                    report.push_str(&format!("  ❌ {}: {:.3} exceeds limit {:.3} by {:.1}%\n",
                        violation.constraint_type,
                        violation.actual,
                        violation.limit,
                        violation.margin));
                }
            }
            report.push_str("\n");
        }
        
        // Components with warnings
        let warnings_count = results.iter()
            .filter(|r| !r.warnings.is_empty())
            .count();
        
        if warnings_count > 0 {
            report.push_str("COMPONENTS WITH WARNINGS:\n");
            report.push_str("------------------------\n");
            for result in results.iter().filter(|r| !r.warnings.is_empty()) {
                report.push_str(&format!("\n{}: {}\n", 
                    result.component,
                    if result.passed { "PASSED with warnings" } else { "FAILED" }
                ));
                
                for warning in &result.warnings {
                    report.push_str(&format!("  ⚠️  {}\n", warning.message));
                }
            }
            report.push_str("\n");
        }
        
        // Summary recommendations
        if failed > 0 || warnings_count > 0 {
            report.push_str("RECOMMENDATIONS:\n");
            report.push_str("---------------\n");
            
            if failed > 0 {
                report.push_str("• Review failed components and adjust values or ratings\n");
                report.push_str("• Consider using constraint-based specification for automatic sizing\n");
            }
            
            if warnings_count > 0 {
                report.push_str("• Components with warnings are operating near limits\n");
                report.push_str("• Consider increasing safety margins for reliability\n");
            }
        } else {
            report.push_str("✅ All components operating within safe limits!\n");
        }
        
        report
    }
}