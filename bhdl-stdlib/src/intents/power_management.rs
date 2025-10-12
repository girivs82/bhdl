//! Power management intent functions
//!
//! Intents for power sequencing, monitoring, and protection

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Power sequencing - ensures power rails turn on in correct order
pub struct PowerSequencingIntent;

impl IntentFunction for PowerSequencingIntent {
    fn name(&self) -> &str {
        "power_sequencing"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract order parameter (required)
        let order = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, _)) if name == "order" => Some(*val as i32),
                    IntentParam::Positional(IntentValue::Number(val, _)) => Some(*val as i32),
                    _ => None
                }
            })
            .ok_or_else(|| "power_sequencing requires 'order' parameter (integer sequence number)".to_string())?;

        if order < 1 {
            return Err("Power sequencing order must be >= 1".to_string());
        }

        // Extract delay parameter (optional)
        let delay_ms = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "delay" => {
                        let delay = match unit.as_deref() {
                            Some("ms") => *val,
                            Some("us") | Some("µs") => val / 1000.0,
                            Some("s") => val * 1000.0,
                            _ => *val, // Assume ms
                        };
                        Some(delay)
                    }
                    _ => None
                }
            })
            .unwrap_or(10.0); // Default 10ms delay between stages

        // Power sequencing requires timing analysis
        let sim_mode = if delay_ms < 1.0 {
            SimMode::DigitalWithTiming  // Fast sequencing
        } else {
            SimMode::MixedSignal        // Need timing and analog monitoring
        };

        let synthesis_hints = vec![
            SynthesisHint::Custom(format!("Power-up sequence stage {}, delay {:.1}ms", order, delay_ms)),
            SynthesisHint::Custom("Consider using power sequencer IC or microcontroller".to_string()),
        ];

        let validation_rules = vec![
            ValidationRule {
                condition: format!("power_sequence_order_{}", order),
                error_message: format!("Power rail must turn on in sequence position {}", order),
            },
            ValidationRule {
                condition: format!("power_delay_min_{}ms", delay_ms),
                error_message: format!("Minimum {:.1}ms delay required before next power stage", delay_ms),
            },
        ];

        Ok(IntentResult {
            sim_mode,
            synthesis_hints,
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "order".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "delay".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: Some(IntentValue::Number(10.0, Some("ms".to_string()))),
            },
        ]
    }
}

/// Voltage monitoring - monitors power rail voltage levels
pub struct VoltageMonitoringIntent;

impl IntentFunction for VoltageMonitoringIntent {
    fn name(&self) -> &str {
        "voltage_monitoring"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract threshold voltage (required)
        let threshold_v = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "threshold" => {
                        let volts = match unit.as_deref() {
                            Some("V") | None => *val,
                            Some("mV") => val / 1000.0,
                            _ => return None,
                        };
                        Some(volts)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let volts = match unit.as_deref() {
                            Some("V") | None => *val,
                            Some("mV") => val / 1000.0,
                            _ => return None,
                        };
                        Some(volts)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "voltage_monitoring requires 'threshold' parameter (voltage)".to_string())?;

        // Extract hysteresis (optional)
        let hysteresis_mv = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "hysteresis" => {
                        let millivolts = match unit.as_deref() {
                            Some("mV") => *val,
                            Some("V") => val * 1000.0,
                            _ => *val, // Assume mV
                        };
                        Some(millivolts)
                    }
                    _ => None
                }
            })
            .unwrap_or(100.0); // Default 100mV hysteresis

        // Voltage monitoring requires analog measurement
        let sim_mode = SimMode::AnalogRequired;

        let synthesis_hints = vec![
            SynthesisHint::Custom(format!(
                "Monitor voltage at {:.2}V threshold with {:.0}mV hysteresis",
                threshold_v,
                hysteresis_mv
            )),
            SynthesisHint::Custom("Consider using voltage supervisor IC or comparator".to_string()),
        ];

        let validation_rules = vec![
            ValidationRule {
                condition: format!("voltage_threshold_{:.2}V", threshold_v),
                error_message: format!("Voltage must stay above {:.2}V threshold", threshold_v),
            },
            ValidationRule {
                condition: format!("hysteresis_{}mV", hysteresis_mv),
                error_message: format!("Hysteresis of {:.0}mV required to prevent oscillation", hysteresis_mv),
            },
        ];

        Ok(IntentResult {
            sim_mode,
            synthesis_hints,
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "threshold".to_string(),
                param_type: ParamType::Voltage,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "hysteresis".to_string(),
                param_type: ParamType::Voltage,
                required: false,
                default_value: Some(IntentValue::Number(100.0, Some("mV".to_string()))),
            },
        ]
    }
}

/// Power good signal - indicates power rail is stable and within spec
pub struct PowerGoodSignalIntent;

impl IntentFunction for PowerGoodSignalIntent {
    fn name(&self) -> &str {
        "power_good_signal"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract delay parameter (optional)
        let delay_us = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "delay" => {
                        let microseconds = match unit.as_deref() {
                            Some("us") | Some("µs") => *val,
                            Some("ms") => val * 1000.0,
                            Some("ns") => val / 1000.0,
                            Some("s") => val * 1_000_000.0,
                            _ => *val, // Assume microseconds
                        };
                        Some(microseconds)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let microseconds = match unit.as_deref() {
                            Some("us") | Some("µs") => *val,
                            Some("ms") => val * 1000.0,
                            Some("ns") => val / 1000.0,
                            Some("s") => val * 1_000_000.0,
                            _ => *val,
                        };
                        Some(microseconds)
                    }
                    _ => None
                }
            })
            .unwrap_or(100.0); // Default 100µs delay

        // Extract tolerance parameter (optional)
        let tolerance_percent = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, _)) if name == "tolerance" => Some(*val),
                    _ => None
                }
            })
            .unwrap_or(5.0); // Default ±5%

        // Power good signals need timing analysis
        let sim_mode = if delay_us < 10.0 {
            SimMode::DigitalWithTiming  // Fast power good
        } else {
            SimMode::MixedSignal        // Slower, need analog verification
        };

        let synthesis_hints = vec![
            SynthesisHint::Custom(format!(
                "Power good signal with {:.1}µs delay, ±{:.1}% tolerance",
                delay_us,
                tolerance_percent
            )),
            SynthesisHint::Custom("Voltage must be stable before asserting power good".to_string()),
        ];

        let validation_rules = vec![
            ValidationRule {
                condition: format!("power_good_delay_{}us", delay_us),
                error_message: format!("Power good signal must wait {:.1}µs after power stable", delay_us),
            },
            ValidationRule {
                condition: format!("voltage_tolerance_{}%", tolerance_percent),
                error_message: format!("Voltage must be within ±{:.1}% before power good", tolerance_percent),
            },
        ];

        Ok(IntentResult {
            sim_mode,
            synthesis_hints,
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "delay".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: Some(IntentValue::Number(100.0, Some("us".to_string()))),
            },
            ParamMetadata {
                name: "tolerance".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(5.0, None)),
            },
        ]
    }
}

/// Inrush current limiting - limits current surge during power-on
pub struct InrushLimitingIntent;

impl IntentFunction for InrushLimitingIntent {
    fn name(&self) -> &str {
        "inrush_limiting"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract max current (required)
        let max_current_a = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max_current" => {
                        let amps = match unit.as_deref() {
                            Some("A") | None => *val,
                            Some("mA") => val / 1000.0,
                            _ => return None,
                        };
                        Some(amps)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let amps = match unit.as_deref() {
                            Some("A") | None => *val,
                            Some("mA") => val / 1000.0,
                            _ => return None,
                        };
                        Some(amps)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "inrush_limiting requires 'max_current' parameter (current)".to_string())?;

        // Extract duration (optional)
        let duration_ms = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "duration" => {
                        let milliseconds = match unit.as_deref() {
                            Some("ms") => *val,
                            Some("us") | Some("µs") => val / 1000.0,
                            Some("s") => val * 1000.0,
                            _ => *val, // Assume ms
                        };
                        Some(milliseconds)
                    }
                    _ => None
                }
            })
            .unwrap_or(10.0); // Default 10ms inrush period

        // Inrush limiting requires analog simulation
        let sim_mode = SimMode::AnalogRequired;

        let synthesis_hints = vec![
            SynthesisHint::Custom(format!(
                "Limit inrush current to {:.2}A over {:.1}ms period",
                max_current_a,
                duration_ms
            )),
            SynthesisHint::Custom("Consider using NTC thermistor, active current limiter, or soft-start circuit".to_string()),
        ];

        let validation_rules = vec![
            ValidationRule {
                condition: format!("inrush_current_max_{}A", max_current_a),
                error_message: format!("Inrush current must not exceed {:.2}A", max_current_a),
            },
            ValidationRule {
                condition: format!("inrush_duration_{}ms", duration_ms),
                error_message: format!("Current limiting active for first {:.1}ms", duration_ms),
            },
        ];

        Ok(IntentResult {
            sim_mode,
            synthesis_hints,
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "max_current".to_string(),
                param_type: ParamType::Current,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "duration".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: Some(IntentValue::Number(10.0, Some("ms".to_string()))),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_sequencing_basic() {
        let intent = PowerSequencingIntent;
        let params = vec![
            IntentParam::Named("order".to_string(), IntentValue::Number(2.0, None)),
            IntentParam::Named("delay".to_string(), IntentValue::Number(15.0, Some("ms".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("stage 2"))
        }));
        assert!(!result.validation_rules.is_empty());
    }

    #[test]
    fn test_power_sequencing_fast() {
        let intent = PowerSequencingIntent;
        let params = vec![
            IntentParam::Named("order".to_string(), IntentValue::Number(1.0, None)),
            IntentParam::Named("delay".to_string(), IntentValue::Number(500.0, Some("us".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        // Fast sequencing uses DigitalWithTiming
        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming);
    }

    #[test]
    fn test_power_sequencing_invalid_order() {
        let intent = PowerSequencingIntent;
        let params = vec![
            IntentParam::Named("order".to_string(), IntentValue::Number(0.0, None)),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be >= 1"));
    }

    #[test]
    fn test_voltage_monitoring() {
        let intent = VoltageMonitoringIntent;
        let params = vec![
            IntentParam::Named("threshold".to_string(), IntentValue::Number(3.3, Some("V".to_string()))),
            IntentParam::Named("hysteresis".to_string(), IntentValue::Number(50.0, Some("mV".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::AnalogRequired);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("3.3") && s.contains("50"))
        }));
    }

    #[test]
    fn test_power_good_signal() {
        let intent = PowerGoodSignalIntent;
        let params = vec![
            IntentParam::Named("delay".to_string(), IntentValue::Number(200.0, Some("us".to_string()))),
            IntentParam::Named("tolerance".to_string(), IntentValue::Number(2.0, None)),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.validation_rules.iter().any(|r| {
            r.condition.contains("200") && r.condition.contains("us")
        }));
    }

    #[test]
    fn test_power_good_fast() {
        let intent = PowerGoodSignalIntent;
        let params = vec![
            IntentParam::Named("delay".to_string(), IntentValue::Number(5.0, Some("us".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        // Very fast power good uses DigitalWithTiming
        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming);
    }

    #[test]
    fn test_inrush_limiting() {
        let intent = InrushLimitingIntent;
        let params = vec![
            IntentParam::Named("max_current".to_string(), IntentValue::Number(2.5, Some("A".to_string()))),
            IntentParam::Named("duration".to_string(), IntentValue::Number(20.0, Some("ms".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::AnalogRequired);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("2.5") && s.contains("20"))
        }));
    }

    #[test]
    fn test_inrush_limiting_milliamps() {
        let intent = InrushLimitingIntent;
        let params = vec![
            IntentParam::Named("max_current".to_string(), IntentValue::Number(500.0, Some("mA".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        // Should convert 500mA to 0.5A
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("0.5") || s.contains("0.50"))
        }));
    }
}
