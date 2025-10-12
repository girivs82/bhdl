// Signal processing intent functions

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Anti-aliasing filter intent
pub struct AntiAliasIntent;

impl IntentFunction for AntiAliasIntent {
    fn name(&self) -> &str {
        "anti_alias"
    }
    
    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract the ADC component reference
        let adc_component = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Identifier(comp)) if name == "before" => {
                    Some(comp.clone())
                }
                _ => None
            }
        });
        
        // Extract cutoff frequency if specified
        let cutoff_freq = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "cutoff" => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });
        
        let mut validation_rules = vec![];
        
        if let Some(adc) = adc_component {
            validation_rules.push(ValidationRule {
                condition: format!("cutoff_below_nyquist({})", adc),
                error_message: "Anti-alias filter cutoff must be < ADC_sample_rate / 2".to_string(),
            });
        }
        
        // Anti-aliasing always requires analog simulation
        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::AnalogFilter,
                SynthesisHint::Custom("Low-pass filter required".to_string()),
            ],
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }
    
    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "before".to_string(),
                param_type: ParamType::Component,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "cutoff".to_string(),
                param_type: ParamType::Frequency,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// Low noise intent for sensitive analog signals
pub struct LowNoiseIntent;

impl IntentFunction for LowNoiseIntent {
    fn name(&self) -> &str {
        "low_noise"
    }
    
    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract noise parameters
        let max_ripple = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max_ripple" => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });
        
        let mut validation_rules = vec![];
        
        if let Some((ripple, unit)) = max_ripple {
            let ripple_str = format!("{}{}", ripple, unit.as_deref().unwrap_or("V"));
            validation_rules.push(ValidationRule {
                condition: "ripple_within_spec".to_string(),
                error_message: format!("Signal ripple must be < {}", ripple_str),
            });
        }
        
        // Low noise circuits need careful analog simulation
        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom("Use low-noise components".to_string()),
                SynthesisHint::Custom("Consider shielding".to_string()),
                SynthesisHint::Custom("Star grounding recommended".to_string()),
            ],
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }
    
    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "max_ripple".to_string(),
                param_type: ParamType::Voltage,
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

/// Noise filtering intent for removing unwanted signals
pub struct NoiseFilteringIntent;

impl IntentFunction for NoiseFilteringIntent {
    fn name(&self) -> &str {
        "noise_filtering"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract cutoff frequency parameter (first positional or named)
        let cutoff = params.get(0).and_then(|p| {
            match p {
                IntentParam::Positional(IntentValue::Number(val, unit)) => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        }).or_else(|| {
            params.iter().find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "cutoff" => {
                        Some((*val, unit.clone()))
                    }
                    _ => None
                }
            })
        });

        // Extract attenuation parameter (second positional or named)
        let attenuation = params.get(1).and_then(|p| {
            match p {
                IntentParam::Positional(IntentValue::Number(val, unit)) => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        }).or_else(|| {
            params.iter().find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "attenuation" => {
                        Some((*val, unit.clone()))
                    }
                    _ => None
                }
            })
        });

        let cutoff_val = cutoff.ok_or_else(|| "noise_filtering() requires cutoff frequency".to_string())?;
        let cutoff_str = format!("{}{}", cutoff_val.0, cutoff_val.1.as_deref().unwrap_or("Hz"));

        let mut synthesis_hints = vec![
            SynthesisHint::AnalogFilter,
            SynthesisHint::Custom(format!("Low-pass filter with {} cutoff", cutoff_str)),
        ];

        if let Some((atten, unit)) = attenuation {
            let atten_str = format!("{}{}", atten, unit.as_deref().unwrap_or("dB"));
            synthesis_hints.push(SynthesisHint::Custom(format!("Attenuation: {} @ stopband", atten_str)));
        }

        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("filter_cutoff_at_{}", cutoff_str),
                    error_message: format!("Filter cutoff frequency must be {}", cutoff_str),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "cutoff".to_string(),
                param_type: ParamType::Frequency,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "attenuation".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
        ]
    }
}