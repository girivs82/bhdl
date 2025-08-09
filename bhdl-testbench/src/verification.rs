//! Verification engine for assertions and measurements

use std::collections::HashMap;
use crate::{Result, TestbenchError, SignalRef};
use crate::testbench::{
    Assertion, AssertionCondition, TimeConstraint, Severity,
    Measurement, MeasurementType,
};
use crate::coordinator::AssertionViolation;

/// Verification engine for checking assertions and computing measurements
pub struct VerificationEngine {
    assertions: Vec<Assertion>,
    measurements: HashMap<String, MeasurementState>,
    violations: Vec<AssertionViolation>,
}

struct MeasurementState {
    config: Measurement,
    accumulator: MeasurementAccumulator,
}

enum MeasurementAccumulator {
    Average { sum: f64, count: usize },
    RMS { sum_squares: f64, count: usize },
    PeakToPeak { min: f64, max: f64 },
    RiseTime { tracking: bool, start_time: Option<f64>, end_time: Option<f64> },
    Integral { sum: f64, last_time: Option<f64> },
}

impl VerificationEngine {
    pub fn new(
        assertions: &[Assertion],
        measurements: &HashMap<String, Measurement>,
    ) -> Result<Self> {
        let mut measurement_states = HashMap::new();
        
        for (name, measurement) in measurements {
            let accumulator = match &measurement.measurement_type {
                MeasurementType::Average { .. } => {
                    MeasurementAccumulator::Average { sum: 0.0, count: 0 }
                }
                MeasurementType::RMS { .. } => {
                    MeasurementAccumulator::RMS { sum_squares: 0.0, count: 0 }
                }
                MeasurementType::PeakToPeak { .. } => {
                    MeasurementAccumulator::PeakToPeak { 
                        min: f64::INFINITY, 
                        max: f64::NEG_INFINITY 
                    }
                }
                MeasurementType::RiseTime { .. } => {
                    MeasurementAccumulator::RiseTime {
                        tracking: false,
                        start_time: None,
                        end_time: None,
                    }
                }
                MeasurementType::Integral { .. } => {
                    MeasurementAccumulator::Integral { 
                        sum: 0.0, 
                        last_time: None 
                    }
                }
                _ => {
                    return Err(TestbenchError::ConfigError(
                        format!("Unsupported measurement type for {}", name)
                    ));
                }
            };
            
            measurement_states.insert(name.clone(), MeasurementState {
                config: measurement.clone(),
                accumulator,
            });
        }
        
        Ok(Self {
            assertions: assertions.to_vec(),
            measurements: measurement_states,
            violations: Vec::new(),
        })
    }
    
    pub fn check(
        &mut self, 
        time: f64, 
        values: &HashMap<SignalRef, f64>
    ) -> Result<Vec<AssertionViolation>> {
        let mut new_violations = Vec::new();
        
        for assertion in &self.assertions {
            if self.should_check_assertion(assertion, time, values) {
                if !self.evaluate_condition(&assertion.condition, values) {
                    new_violations.push(AssertionViolation {
                        time,
                        assertion_name: assertion.name.clone(),
                        message: assertion.message.clone(),
                        severity: assertion.severity,
                    });
                }
            }
        }
        
        self.violations.extend(new_violations.clone());
        Ok(new_violations)
    }
    
    pub fn update_measurements(
        &mut self,
        time: f64,
        values: &HashMap<SignalRef, f64>,
    ) -> Result<()> {
        for state in self.measurements.values_mut() {
            match &state.config.measurement_type {
                MeasurementType::Average { signal } => {
                    if let Some(value) = values.get(signal) {
                        if let MeasurementAccumulator::Average { sum, count } = &mut state.accumulator {
                            *sum += value;
                            *count += 1;
                        }
                    }
                }
                
                MeasurementType::RMS { signal } => {
                    if let Some(value) = values.get(signal) {
                        if let MeasurementAccumulator::RMS { sum_squares, count } = &mut state.accumulator {
                            *sum_squares += value * value;
                            *count += 1;
                        }
                    }
                }
                
                MeasurementType::PeakToPeak { signal, .. } => {
                    if let Some(value) = values.get(signal) {
                        if let MeasurementAccumulator::PeakToPeak { min, max } = &mut state.accumulator {
                            *min = min.min(*value);
                            *max = max.max(*value);
                        }
                    }
                }
                
                MeasurementType::Integral { .. } => {
                    if let MeasurementAccumulator::Integral { sum, last_time } = &mut state.accumulator {
                        if let Some(last_t) = last_time {
                            let dt = time - *last_t;
                            // TODO: Evaluate expression and integrate
                            *sum += 0.0 * dt; // Placeholder
                        }
                        *last_time = Some(time);
                    }
                }
                
                _ => {} // Other measurement types
            }
        }
        
        Ok(())
    }
    
    pub fn get_final_measurements(&self) -> HashMap<String, f64> {
        let mut results = HashMap::new();
        
        for (name, state) in &self.measurements {
            let value = match &state.accumulator {
                MeasurementAccumulator::Average { sum, count } => {
                    if *count > 0 { sum / (*count as f64) } else { 0.0 }
                }
                MeasurementAccumulator::RMS { sum_squares, count } => {
                    if *count > 0 { 
                        (sum_squares / (*count as f64)).sqrt() 
                    } else { 
                        0.0 
                    }
                }
                MeasurementAccumulator::PeakToPeak { min, max } => {
                    if max.is_finite() && min.is_finite() {
                        max - min
                    } else {
                        0.0
                    }
                }
                MeasurementAccumulator::Integral { sum, .. } => *sum,
                _ => 0.0, // Other types
            };
            
            results.insert(name.clone(), value);
        }
        
        results
    }
    
    fn should_check_assertion(
        &self,
        assertion: &Assertion,
        time: f64,
        _values: &HashMap<SignalRef, f64>,
    ) -> bool {
        match &assertion.time_constraint {
            TimeConstraint::Always => true,
            TimeConstraint::After(after_time) => time >= after_time.as_seconds(),
            TimeConstraint::Between { start, end } => {
                time >= start.as_seconds() && time <= end.as_seconds()
            }
            TimeConstraint::When { .. } => {
                // TODO: Evaluate when condition
                true
            }
        }
    }
    
    fn evaluate_condition(
        &self,
        condition: &AssertionCondition,
        values: &HashMap<SignalRef, f64>,
    ) -> bool {
        match condition {
            AssertionCondition::SignalInRange { signal, min, max } => {
                if let Some(value) = values.get(signal) {
                    *value >= *min && *value <= *max
                } else {
                    println!("DEBUG: Signal {:?} not found in values", signal);
                    println!("       Available signals: {:?}", values.keys().collect::<Vec<_>>());
                    false // Signal not found
                }
            }
            
            AssertionCondition::SignalEquals { signal, value: expected, tolerance } => {
                if let Some(value) = values.get(signal) {
                    (value - expected).abs() <= *tolerance
                } else {
                    println!("DEBUG: Signal {:?} not found in values", signal);
                    println!("       Available signals: {:?}", values.keys().collect::<Vec<_>>());
                    false
                }
            }
            
            AssertionCondition::Expression(_expr) => {
                // TODO: Parse and evaluate expression
                true
            }
        }
    }
}