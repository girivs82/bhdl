// Measurement and instrumentation intent functions

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Precision measurement intent for high-accuracy sensing
pub struct PrecisionMeasurementIntent;

impl IntentFunction for PrecisionMeasurementIntent {
    fn name(&self) -> &str {
        "precision_measurement"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract bandwidth parameter
        let bandwidth = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "bandwidth" => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });

        // Extract noise floor parameter
        let noise_floor = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "noise_floor" => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });

        let mut synthesis_hints = vec![
            SynthesisHint::Custom("Use precision ADC".to_string()),
            SynthesisHint::Custom("Minimize noise coupling".to_string()),
            SynthesisHint::Custom("Use star grounding".to_string()),
        ];

        if let Some((bw, unit)) = bandwidth {
            let bw_str = format!("{}{}", bw, unit.as_deref().unwrap_or("Hz"));
            synthesis_hints.push(SynthesisHint::Custom(format!("Bandwidth: {}", bw_str)));
        }

        if let Some((nf, unit)) = noise_floor {
            let nf_str = format!("{}{}", nf, unit.as_deref().unwrap_or("dB"));
            synthesis_hints.push(SynthesisHint::Custom(format!("Max noise floor: {}", nf_str)));
        }

        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints,
            validation_rules: vec![
                ValidationRule {
                    condition: "measurement_accuracy_verified".to_string(),
                    error_message: "Verify measurement chain meets accuracy requirements".to_string(),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "bandwidth".to_string(),
                param_type: ParamType::Frequency,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "noise_floor".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// Control loop intent for feedback systems
pub struct ControlLoopIntent;

impl IntentFunction for ControlLoopIntent {
    fn name(&self) -> &str {
        "control_loop"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract response time parameter (first positional or named)
        let response_time = params.get(0).and_then(|p| {
            match p {
                IntentParam::Positional(IntentValue::Number(val, unit)) => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        }).or_else(|| {
            params.iter().find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "response_time" => {
                        Some((*val, unit.clone()))
                    }
                    _ => None
                }
            })
        });

        // Extract bandwidth parameter (second positional or named)
        let bandwidth = params.get(1).and_then(|p| {
            match p {
                IntentParam::Positional(IntentValue::Number(val, unit)) => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        }).or_else(|| {
            params.iter().find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "bandwidth" => {
                        Some((*val, unit.clone()))
                    }
                    _ => None
                }
            })
        });

        let mut synthesis_hints = vec![
            SynthesisHint::Custom("Design feedback compensation network".to_string()),
            SynthesisHint::Custom("Verify loop stability (phase margin > 45°)".to_string()),
        ];

        if let Some((rt, unit)) = response_time {
            let rt_str = format!("{}{}", rt, unit.as_deref().unwrap_or("s"));
            synthesis_hints.push(SynthesisHint::Custom(format!("Target response time: {}", rt_str)));
        }

        if let Some((bw, unit)) = bandwidth {
            let bw_str = format!("{}{}", bw, unit.as_deref().unwrap_or("Hz"));
            synthesis_hints.push(SynthesisHint::Custom(format!("Loop bandwidth: {}", bw_str)));
        }

        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints,
            validation_rules: vec![
                ValidationRule {
                    condition: "loop_stable".to_string(),
                    error_message: "Control loop must be stable with adequate phase margin".to_string(),
                },
                ValidationRule {
                    condition: "response_time_met".to_string(),
                    error_message: "Control loop must meet response time requirement".to_string(),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "response_time".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "bandwidth".to_string(),
                param_type: ParamType::Frequency,
                required: false,
                default_value: None,
            },
        ]
    }
}
