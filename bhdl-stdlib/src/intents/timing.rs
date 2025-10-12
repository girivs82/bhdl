// Timing-related intent functions

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Delay intent - specifies signal delay requirements
pub struct DelayIntent;

impl IntentFunction for DelayIntent {
    fn name(&self) -> &str {
        "delay"
    }
    
    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract delay time parameter
        let delay_time = match params.get(0) {
            Some(IntentParam::Positional(IntentValue::Number(val, unit))) => {
                // Convert to seconds based on unit
                let delay_seconds = match unit.as_deref() {
                    Some("s") => *val,
                    Some("ms") => val / 1000.0,
                    Some("us") | Some("µs") => val / 1_000_000.0,
                    Some("ns") => val / 1_000_000_000.0,
                    _ => return Err("Invalid time unit for delay".to_string()),
                };
                delay_seconds
            }
            _ => return Err("delay() requires a time parameter (e.g., 3ms)".to_string()),
        };
        
        // Determine simulation mode based on delay magnitude
        let sim_mode = if delay_time < 1e-6 {
            SimMode::DigitalWithTiming  // Sub-microsecond delays
        } else if delay_time < 1e-3 {
            SimMode::MixedSignal        // Microsecond to millisecond
        } else {
            SimMode::AnalogRequired     // Large delays need RC modeling
        };
        
        // Synthesis hints based on delay
        let synthesis_hints = if delay_time < 100e-9 {
            vec![SynthesisHint::BufferChain]
        } else if delay_time < 10e-6 {
            vec![SynthesisHint::RCNetwork]
        } else {
            vec![SynthesisHint::ActiveDelay]
        };
        
        // Validation rules
        let validation_rules = vec![
            ValidationRule {
                condition: "has_timing_element".to_string(),
                error_message: "Delay intent requires RC network or delay element".to_string(),
            }
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
                name: "time".to_string(),
                param_type: ParamType::Duration,
                required: true,
                default_value: None,
            }
        ]
    }
}

/// Debounce intent - for mechanical switch debouncing
pub struct DebounceIntent;

impl IntentFunction for DebounceIntent {
    fn name(&self) -> &str {
        "debounce"
    }
    
    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract parameters
        let bounce_time = match params.iter().find(|p| {
            matches!(p, IntentParam::Named(name, _) if name == "time")
        }).or_else(|| params.get(1)) {
            Some(IntentParam::Positional(IntentValue::Number(val, unit))) |
            Some(IntentParam::Named(_, IntentValue::Number(val, unit))) => {
                // Convert to seconds
                match unit.as_deref() {
                    Some("ms") => val / 1000.0,
                    Some("s") => *val,
                    _ => 0.020, // Default 20ms
                }
            }
            _ => 0.020, // Default 20ms debounce time
        };
        
        // Debouncing always requires analog simulation for RC filtering
        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![SynthesisHint::RCNetwork],
            validation_rules: vec![
                ValidationRule {
                    condition: "has_rc_network".to_string(),
                    error_message: format!("Debounce requires RC network for {}ms delay", bounce_time * 1000.0),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }
    
    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "source".to_string(),
                param_type: ParamType::Component,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "time".to_string(),
                param_type: ParamType::Duration,
                required: false,
                default_value: Some(IntentValue::Number(20.0, Some("ms".to_string()))),
            }
        ]
    }
}

/// Pulse stretch intent - extends short pulses to minimum duration
pub struct PulseStretchIntent;

impl IntentFunction for PulseStretchIntent {
    fn name(&self) -> &str {
        "pulse_stretch"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract stretch duration parameter
        let duration = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "duration" => {
                    Some((*val, unit.clone()))
                }
                IntentParam::Positional(IntentValue::Number(val, unit)) => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });

        let (duration_val, duration_unit) = duration
            .ok_or_else(|| "pulse_stretch() requires duration parameter".to_string())?;

        let duration_str = format!("{}{}", duration_val, duration_unit.as_deref().unwrap_or("s"));

        // Convert to seconds for mode determination
        let duration_seconds = match duration_unit.as_deref() {
            Some("s") => duration_val,
            Some("ms") => duration_val / 1000.0,
            Some("us") | Some("µs") => duration_val / 1_000_000.0,
            Some("ns") => duration_val / 1_000_000_000.0,
            _ => duration_val, // Assume seconds
        };

        // Determine simulation mode based on duration
        let sim_mode = if duration_seconds < 1e-6 {
            SimMode::DigitalWithTiming  // Sub-microsecond
        } else {
            SimMode::MixedSignal        // Longer durations need timing analysis
        };

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!("Pulse stretcher for min {}", duration_str)),
                SynthesisHint::RCNetwork,
                SynthesisHint::Custom("Consider monostable multivibrator".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: format!("pulse_min_width_{}", duration_str),
                    error_message: format!("Output pulse must be at least {}", duration_str),
                }
            ],
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
        ]
    }
}