//! Advanced digital timing intent functions
//!
//! Intents for clock distribution, reset generation, and boot sequencing

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Clock distribution - manages clock signal distribution with timing constraints
pub struct ClockDistributionIntent;

impl IntentFunction for ClockDistributionIntent {
    fn name(&self) -> &str {
        "clock_distribution"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract skew parameter (optional)
        let skew_ps = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "skew" => {
                        let picoseconds = match unit.as_deref() {
                            Some("ps") => *val,
                            Some("ns") => val * 1000.0,
                            Some("us") | Some("µs") => val * 1_000_000.0,
                            _ => *val, // Assume ps
                        };
                        Some(picoseconds)
                    }
                    _ => None
                }
            })
            .unwrap_or(100.0); // Default 100ps max skew

        // Extract jitter parameter (optional)
        let jitter_ps = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "jitter" => {
                        let picoseconds = match unit.as_deref() {
                            Some("ps") => *val,
                            Some("ns") => val * 1000.0,
                            _ => *val, // Assume ps
                        };
                        Some(picoseconds)
                    }
                    _ => None
                }
            })
            .unwrap_or(50.0); // Default 50ps max jitter

        // Extract fanout parameter (optional)
        let fanout = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, _)) if name == "fanout" => Some(*val as i32),
                    _ => None
                }
            })
            .unwrap_or(8); // Default fanout of 8

        // Clock distribution with tight timing always requires DigitalWithTiming
        let sim_mode = SimMode::DigitalWithTiming;

        let synthesis_hints = vec![
            SynthesisHint::Custom(format!(
                "Clock distribution: max skew {:.0}ps, max jitter {:.0}ps, fanout {}",
                skew_ps, jitter_ps, fanout
            )),
            SynthesisHint::BufferChain,
            SynthesisHint::Custom("Consider using clock buffer IC (e.g., SI5351, CDCE913)".to_string()),
        ];

        let validation_rules = vec![
            ValidationRule {
                condition: format!("clock_skew_max_{}ps", skew_ps),
                error_message: format!("Clock skew must not exceed {:.0}ps", skew_ps),
            },
            ValidationRule {
                condition: format!("clock_jitter_max_{}ps", jitter_ps),
                error_message: format!("Clock jitter must not exceed {:.0}ps", jitter_ps),
            },
            ValidationRule {
                condition: format!("clock_fanout_max_{}", fanout),
                error_message: format!("Clock fanout must not exceed {}", fanout),
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
                name: "skew".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: Some(IntentValue::Number(100.0, Some("ps".to_string()))),
            },
            ParamMetadata {
                name: "jitter".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: Some(IntentValue::Number(50.0, Some("ps".to_string()))),
            },
            ParamMetadata {
                name: "fanout".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(8.0, None)),
            },
        ]
    }
}

/// Reset generation - generates system reset signal with timing requirements
pub struct ResetGenerationIntent;

impl IntentFunction for ResetGenerationIntent {
    fn name(&self) -> &str {
        "reset_generation"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract duration parameter (required)
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
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let milliseconds = match unit.as_deref() {
                            Some("ms") => *val,
                            Some("us") | Some("µs") => val / 1000.0,
                            Some("s") => val * 1000.0,
                            _ => *val,
                        };
                        Some(milliseconds)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "reset_generation requires 'duration' parameter (time)".to_string())?;

        if duration_ms <= 0.0 {
            return Err("Reset duration must be positive".to_string());
        }

        // Extract assert level (optional)
        let assert_level = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "assert_level" => Some(s.clone()),
                    _ => None
                }
            })
            .unwrap_or_else(|| "low".to_string());

        // Validate assert level
        if assert_level != "low" && assert_level != "high" {
            return Err(format!("assert_level must be 'low' or 'high', got '{}'", assert_level));
        }

        // Reset generation requires timing analysis
        let sim_mode = if duration_ms < 1.0 {
            SimMode::DigitalWithTiming  // Sub-millisecond resets
        } else {
            SimMode::MixedSignal        // Longer resets may need RC timing
        };

        let synthesis_hints = vec![
            SynthesisHint::Custom(format!(
                "Reset signal: {:.1}ms duration, active {}",
                duration_ms, assert_level
            )),
            SynthesisHint::Custom("Consider using reset supervisor IC or RC delay circuit".to_string()),
        ];

        let validation_rules = vec![
            ValidationRule {
                condition: format!("reset_duration_min_{}ms", duration_ms),
                error_message: format!("Reset must be held for at least {:.1}ms", duration_ms),
            },
            ValidationRule {
                condition: format!("reset_active_{}", assert_level),
                error_message: format!("Reset must be active-{} to match system requirements", assert_level),
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
                name: "duration".to_string(),
                param_type: ParamType::Duration,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "assert_level".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: Some(IntentValue::String("low".to_string())),
            },
        ]
    }
}

/// Boot sequencing - manages multi-stage boot process with timeouts
pub struct BootSequencingIntent;

impl IntentFunction for BootSequencingIntent {
    fn name(&self) -> &str {
        "boot_sequencing"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract stage parameter (required)
        let stage = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, _)) if name == "stage" => Some(*val as i32),
                    IntentParam::Positional(IntentValue::Number(val, _)) => Some(*val as i32),
                    _ => None
                }
            })
            .ok_or_else(|| "boot_sequencing requires 'stage' parameter (integer stage number)".to_string())?;

        if stage < 1 {
            return Err("Boot stage must be >= 1".to_string());
        }

        // Extract timeout parameter (optional)
        let timeout_s = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "timeout" => {
                        let seconds = match unit.as_deref() {
                            Some("s") => *val,
                            Some("ms") => val / 1000.0,
                            Some("m") | Some("min") => val * 60.0,
                            _ => *val, // Assume seconds
                        };
                        Some(seconds)
                    }
                    _ => None
                }
            })
            .unwrap_or(5.0); // Default 5 second timeout

        // Extract dependencies (optional)
        let has_dependencies = params.iter().any(|p| {
            matches!(p, IntentParam::Named(name, _) if name == "depends_on")
        });

        // Boot sequencing needs timing for timeouts
        let sim_mode = if timeout_s < 0.1 {
            SimMode::DigitalWithTiming  // Fast boot stages
        } else {
            SimMode::MixedSignal        // Longer stages may need monitoring
        };

        let mut synthesis_hints = vec![
            SynthesisHint::Custom(format!(
                "Boot stage {}: {:.1}s timeout",
                stage, timeout_s
            )),
        ];

        if has_dependencies {
            synthesis_hints.push(SynthesisHint::Custom("Stage has dependencies - wait for completion before starting".to_string()));
        }

        if stage == 1 {
            synthesis_hints.push(SynthesisHint::Custom("Initial boot stage - runs immediately after reset".to_string()));
        }

        let mut validation_rules = vec![
            ValidationRule {
                condition: format!("boot_stage_{}_complete", stage),
                error_message: format!("Boot stage {} must complete successfully", stage),
            },
            ValidationRule {
                condition: format!("boot_timeout_{}s", timeout_s),
                error_message: format!("Boot stage must complete within {:.1}s or timeout", timeout_s),
            },
        ];

        if has_dependencies {
            validation_rules.push(ValidationRule {
                condition: format!("boot_dependencies_stage_{}", stage),
                error_message: format!("All dependencies must complete before stage {}", stage),
            });
        }

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
                name: "stage".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "timeout".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: Some(IntentValue::Number(5.0, Some("s".to_string()))),
            },
            ParamMetadata {
                name: "depends_on".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: None,
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clock_distribution_basic() {
        let intent = ClockDistributionIntent;
        let params = vec![
            IntentParam::Named("skew".to_string(), IntentValue::Number(50.0, Some("ps".to_string()))),
            IntentParam::Named("jitter".to_string(), IntentValue::Number(25.0, Some("ps".to_string()))),
            IntentParam::Named("fanout".to_string(), IntentValue::Number(16.0, None)),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::BufferChain)
        }));
        assert_eq!(result.validation_rules.len(), 3);
    }

    #[test]
    fn test_clock_distribution_defaults() {
        let intent = ClockDistributionIntent;
        let params = vec![];

        let result = intent.resolve(&params).unwrap();

        // Should use default values: 100ps skew, 50ps jitter, fanout 8
        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("100ps") && s.contains("50ps"))
        }));
    }

    #[test]
    fn test_reset_generation_basic() {
        let intent = ResetGenerationIntent;
        let params = vec![
            IntentParam::Named("duration".to_string(), IntentValue::Number(100.0, Some("ms".to_string()))),
            IntentParam::Named("assert_level".to_string(), IntentValue::String("low".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("100") && s.contains("low"))
        }));
    }

    #[test]
    fn test_reset_generation_fast() {
        let intent = ResetGenerationIntent;
        let params = vec![
            IntentParam::Named("duration".to_string(), IntentValue::Number(500.0, Some("us".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        // Fast reset uses DigitalWithTiming
        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming);
    }

    #[test]
    fn test_reset_generation_invalid_duration() {
        let intent = ResetGenerationIntent;
        let params = vec![
            IntentParam::Named("duration".to_string(), IntentValue::Number(-10.0, Some("ms".to_string()))),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be positive"));
    }

    #[test]
    fn test_reset_generation_invalid_level() {
        let intent = ResetGenerationIntent;
        let params = vec![
            IntentParam::Named("duration".to_string(), IntentValue::Number(100.0, Some("ms".to_string()))),
            IntentParam::Named("assert_level".to_string(), IntentValue::String("invalid".to_string())),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be 'low' or 'high'"));
    }

    #[test]
    fn test_boot_sequencing_basic() {
        let intent = BootSequencingIntent;
        let params = vec![
            IntentParam::Named("stage".to_string(), IntentValue::Number(2.0, None)),
            IntentParam::Named("timeout".to_string(), IntentValue::Number(10.0, Some("s".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("stage 2"))
        }));
    }

    #[test]
    fn test_boot_sequencing_fast() {
        let intent = BootSequencingIntent;
        let params = vec![
            IntentParam::Named("stage".to_string(), IntentValue::Number(1.0, None)),
            IntentParam::Named("timeout".to_string(), IntentValue::Number(50.0, Some("ms".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();

        // Fast boot stage uses DigitalWithTiming
        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("Initial boot stage"))
        }));
    }

    #[test]
    fn test_boot_sequencing_with_dependencies() {
        let intent = BootSequencingIntent;
        let params = vec![
            IntentParam::Named("stage".to_string(), IntentValue::Number(3.0, None)),
            IntentParam::Named("depends_on".to_string(), IntentValue::String("stage2".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();

        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("dependencies"))
        }));
        assert!(result.validation_rules.iter().any(|r| {
            r.condition.contains("dependencies")
        }));
    }

    #[test]
    fn test_boot_sequencing_invalid_stage() {
        let intent = BootSequencingIntent;
        let params = vec![
            IntentParam::Named("stage".to_string(), IntentValue::Number(0.0, None)),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be >= 1"));
    }
}
