//! Signal integrity checking

use crate::circuit::state::{PinValue, NetValue};
use crate::error::SimulationResult;
use bhdl_netlist::NetId;
use std::collections::HashMap;

/// Checks signal integrity violations
pub struct SignalIntegrityChecker {
    /// Voltage limits by domain
    voltage_limits: HashMap<String, VoltageLimit>,
    
    /// Slew rate limits
    max_slew_rate: f64,
    
    /// Reflection threshold
    reflection_threshold: f64,
    
    /// History for slew rate calculation
    pin_history: HashMap<String, Vec<(f64, f64)>>, // (time, voltage)
    
    /// Metrics
    metrics: IntegrityMetrics,
}

/// Voltage limits for a power domain
#[derive(Debug, Clone)]
pub struct VoltageLimit {
    pub min: f64,
    pub max: f64,
    pub nominal: f64,
}

/// Signal integrity violation
#[derive(Debug, Clone)]
pub struct IntegrityViolation {
    pub location: String,
    pub violation_type: ViolationType,
    pub severity: ViolationSeverity,
    pub value: f64,
    pub limit: f64,
}

/// Type of integrity violation
#[derive(Debug, Clone, PartialEq)]
pub enum ViolationType {
    Overvoltage,
    Undervoltage,
    ExcessiveSlew,
    Reflection,
    Ringing,
    Crosstalk,
}

/// Violation severity
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum ViolationSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Integrity checking metrics
#[derive(Debug, Default)]
struct IntegrityMetrics {
    checks_performed: usize,
    violations_found: usize,
    check_time_ms: f64,
}

impl SignalIntegrityChecker {
    /// Create a new signal integrity checker
    pub fn new() -> Self {
        let mut checker = Self {
            voltage_limits: HashMap::new(),
            max_slew_rate: 1e9, // 1V/ns default
            reflection_threshold: 0.1, // 10% reflection
            pin_history: HashMap::new(),
            metrics: IntegrityMetrics::default(),
        };
        
        // Add default voltage limits
        checker.add_default_limits();
        checker
    }
    
    /// Add default voltage limits
    fn add_default_limits(&mut self) {
        // 5V domain
        self.voltage_limits.insert("5V".to_string(), VoltageLimit {
            min: -0.3,
            max: 5.5,
            nominal: 5.0,
        });
        
        // 3.3V domain
        self.voltage_limits.insert("3.3V".to_string(), VoltageLimit {
            min: -0.3,
            max: 3.6,
            nominal: 3.3,
        });
        
        // 1.8V domain
        self.voltage_limits.insert("1.8V".to_string(), VoltageLimit {
            min: -0.3,
            max: 2.0,
            nominal: 1.8,
        });
    }
    
    /// Add custom voltage limit
    pub fn add_voltage_limit(&mut self, domain: String, limit: VoltageLimit) {
        self.voltage_limits.insert(domain, limit);
    }
    
    /// Set maximum slew rate
    pub fn set_max_slew_rate(&mut self, rate: f64) {
        self.max_slew_rate = rate;
    }
    
    /// Check all signals for integrity violations
    pub fn check_all_signals(
        &mut self,
        pins: &HashMap<String, PinValue>,
        nets: &HashMap<NetId, NetValue>,
        current_time: f64,
    ) -> SimulationResult<Vec<IntegrityViolation>> {
        let start = std::time::Instant::now();
        let mut violations = Vec::new();
        
        // Check pin voltages
        for (pin_name, pin_value) in pins {
            violations.extend(self.check_pin_voltage(pin_name, pin_value)?);
            violations.extend(self.check_slew_rate(pin_name, pin_value.voltage, current_time)?);
            self.metrics.checks_performed += 2;
        }
        
        // Check net voltages
        for (net_id, net_value) in nets {
            violations.extend(self.check_net_voltage(&format!("{:?}", net_id), net_value)?);
            self.metrics.checks_performed += 1;
        }
        
        self.metrics.violations_found += violations.len();
        self.metrics.check_time_ms += start.elapsed().as_secs_f64() * 1000.0;
        
        Ok(violations)
    }
    
    /// Check pin voltage limits
    fn check_pin_voltage(
        &self,
        pin_name: &str,
        pin_value: &PinValue,
    ) -> SimulationResult<Vec<IntegrityViolation>> {
        let mut violations = Vec::new();
        
        // Determine voltage domain (simplified - would need actual domain info)
        let domain = if pin_value.voltage > 4.0 {
            "5V"
        } else if pin_value.voltage > 2.5 {
            "3.3V"
        } else {
            "1.8V"
        };
        
        if let Some(limit) = self.voltage_limits.get(domain) {
            if pin_value.voltage > limit.max {
                violations.push(IntegrityViolation {
                    location: pin_name.to_string(),
                    violation_type: ViolationType::Overvoltage,
                    severity: if pin_value.voltage > limit.max + 1.0 {
                        ViolationSeverity::Critical
                    } else {
                        ViolationSeverity::Error
                    },
                    value: pin_value.voltage,
                    limit: limit.max,
                });
            }
            
            if pin_value.voltage < limit.min {
                violations.push(IntegrityViolation {
                    location: pin_name.to_string(),
                    violation_type: ViolationType::Undervoltage,
                    severity: ViolationSeverity::Error,
                    value: pin_value.voltage,
                    limit: limit.min,
                });
            }
        }
        
        Ok(violations)
    }
    
    /// Check slew rate
    fn check_slew_rate(
        &mut self,
        pin_name: &str,
        voltage: f64,
        time: f64,
    ) -> SimulationResult<Vec<IntegrityViolation>> {
        let mut violations = Vec::new();
        
        // Get or create history
        let history = self.pin_history.entry(pin_name.to_string()).or_default();
        
        // Calculate slew rate if we have history
        if let Some((last_time, last_voltage)) = history.last() {
            let dt = time - last_time;
            if dt > 0.0 {
                let slew_rate = (voltage - last_voltage).abs() / dt;
                
                if slew_rate > self.max_slew_rate {
                    violations.push(IntegrityViolation {
                        location: pin_name.to_string(),
                        violation_type: ViolationType::ExcessiveSlew,
                        severity: if slew_rate > self.max_slew_rate * 2.0 {
                            ViolationSeverity::Error
                        } else {
                            ViolationSeverity::Warning
                        },
                        value: slew_rate,
                        limit: self.max_slew_rate,
                    });
                }
            }
        }
        
        // Update history
        history.push((time, voltage));
        
        // Keep history bounded
        if history.len() > 100 {
            history.remove(0);
        }
        
        Ok(violations)
    }
    
    /// Check net voltage
    fn check_net_voltage(
        &self,
        net_name: &str,
        net_value: &NetValue,
    ) -> SimulationResult<Vec<IntegrityViolation>> {
        let mut violations = Vec::new();
        
        // Similar to pin voltage check but for nets
        let domain = if net_value.voltage > 4.0 {
            "5V"
        } else if net_value.voltage > 2.5 {
            "3.3V"
        } else {
            "1.8V"
        };
        
        if let Some(limit) = self.voltage_limits.get(domain) {
            if net_value.voltage > limit.max {
                violations.push(IntegrityViolation {
                    location: format!("Net:{}", net_name),
                    violation_type: ViolationType::Overvoltage,
                    severity: ViolationSeverity::Error,
                    value: net_value.voltage,
                    limit: limit.max,
                });
            }
        }
        
        Ok(violations)
    }
    
    /// Check for reflections (simplified)
    pub fn check_reflections(
        &self,
        source_impedance: f64,
        load_impedance: f64,
        pin_name: &str,
    ) -> Option<IntegrityViolation> {
        let reflection_coeff = (load_impedance - source_impedance) / (load_impedance + source_impedance);
        
        if reflection_coeff.abs() > self.reflection_threshold {
            Some(IntegrityViolation {
                location: pin_name.to_string(),
                violation_type: ViolationType::Reflection,
                severity: if reflection_coeff.abs() > 0.3 {
                    ViolationSeverity::Error
                } else {
                    ViolationSeverity::Warning
                },
                value: reflection_coeff.abs(),
                limit: self.reflection_threshold,
            })
        } else {
            None
        }
    }
    
    /// Get metrics
    pub fn metrics(&self) -> &IntegrityMetrics {
        &self.metrics
    }
    
    /// Reset metrics
    pub fn reset_metrics(&mut self) {
        self.metrics = IntegrityMetrics::default();
    }
}

impl Default for SignalIntegrityChecker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::circuit::state::{DriveStrength, LogicLevel};
    
    #[test]
    fn test_overvoltage_detection() {
        let checker = SignalIntegrityChecker::new();
        
        let pin_value = PinValue {
            voltage: 6.0, // Over 5V limit
            current: 0.0,
            impedance: 50.0,
            drive_strength: DriveStrength::Strong,
            logic_level: Some(LogicLevel::High),
        };
        
        let violations = checker.check_pin_voltage("U1.VCC", &pin_value).unwrap();
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].violation_type, ViolationType::Overvoltage);
        assert_eq!(violations[0].severity, ViolationSeverity::Error);
    }
    
    #[test]
    fn test_slew_rate_detection() {
        let mut checker = SignalIntegrityChecker::new();
        checker.set_max_slew_rate(1e9); // 1V/ns
        
        // First sample
        checker.check_slew_rate("CLK", 0.0, 0.0).unwrap();
        
        // Second sample with excessive slew
        let violations = checker.check_slew_rate("CLK", 5.0, 1e-9).unwrap(); // 5V in 1ns
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].violation_type, ViolationType::ExcessiveSlew);
    }
    
    #[test]
    fn test_reflection_calculation() {
        let checker = SignalIntegrityChecker::new();
        
        // 50Ω source, 150Ω load = 0.5 reflection coefficient
        let violation = checker.check_reflections(50.0, 150.0, "TX_LINE");
        assert!(violation.is_some());
        
        if let Some(v) = violation {
            assert_eq!(v.violation_type, ViolationType::Reflection);
            assert!((v.value - 0.5).abs() < 0.001);
        }
    }
}