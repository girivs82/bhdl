// Digital circuit intent functions

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Signal buffering intent for driving multiple loads
pub struct SignalBufferingIntent;

impl IntentFunction for SignalBufferingIntent {
    fn name(&self) -> &str {
        "signal_buffering"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract fanout parameter
        let fanout = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, _)) if name == "fanout" => {
                    Some(*val as u32)
                }
                IntentParam::Positional(IntentValue::Number(val, _)) => {
                    Some(*val as u32)
                }
                _ => None
            }
        });

        // Extract drive strength parameter
        let drive = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Identifier(level)) if name == "drive" => {
                    Some(level.clone())
                }
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "drive" => {
                    Some(format!("{}{}", val, unit.as_deref().unwrap_or("mA")))
                }
                _ => None
            }
        });

        let fanout_val = fanout.unwrap_or(1);

        let sim_mode = if fanout_val > 5 {
            // High fanout may need analog analysis for signal integrity
            SimMode::MixedSignal
        } else {
            SimMode::DigitalWithTiming
        };

        let mut synthesis_hints = vec![
            SynthesisHint::Custom(format!("Use buffer for {} fanout", fanout_val)),
        ];

        if let Some(drive_str) = drive {
            synthesis_hints.push(SynthesisHint::Custom(format!("Drive strength: {}", drive_str)));
        }

        if fanout_val > 10 {
            synthesis_hints.push(SynthesisHint::Custom("Consider using buffer tree".to_string()));
        }

        Ok(IntentResult {
            sim_mode,
            synthesis_hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("fanout_supported_{}", fanout_val),
                    error_message: format!("Buffer must support fanout of {}", fanout_val),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "fanout".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(1.0, None)),
            },
            ParamMetadata {
                name: "drive".to_string(),
                param_type: ParamType::Current,
                required: false,
                default_value: None,
            },
        ]
    }
}
