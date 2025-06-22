// Protection-related intent functions

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Input protection intent - comprehensive protection for inputs
pub struct InputProtectionIntent;

impl IntentFunction for InputProtectionIntent {
    fn name(&self) -> &str {
        "input_protection"
    }
    
    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract overvoltage and current limit parameters
        let mut overvoltage = None;
        let mut current_limit = None;
        
        for param in params {
            match param {
                IntentParam::Named(name, IntentValue::Number(val, unit)) => {
                    match name.as_str() {
                        "overvoltage" => overvoltage = Some((*val, unit.clone())),
                        "current_limit" => current_limit = Some((*val, unit.clone())),
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        
        // Protection circuits need analog simulation to verify effectiveness
        let mut validation_rules = vec![];
        
        if let Some((voltage, _)) = overvoltage {
            validation_rules.push(ValidationRule {
                condition: "has_voltage_clamp".to_string(),
                error_message: format!("Input protection requires voltage clamping for {}V", voltage),
            });
        }
        
        if let Some((current, unit)) = current_limit {
            let current_str = format!("{}{}", current, unit.as_deref().unwrap_or("A"));
            validation_rules.push(ValidationRule {
                condition: "has_current_limiting".to_string(),
                error_message: format!("Input protection requires current limiting to {}", current_str),
            });
        }
        
        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom("TVS diode or zener".to_string()),
                SynthesisHint::Custom("Current limiting resistor".to_string()),
            ],
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }
    
    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "overvoltage".to_string(),
                param_type: ParamType::Voltage,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "current_limit".to_string(),
                param_type: ParamType::Current,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// Overvoltage protection intent - simple voltage clamping
pub struct OvervoltageProtectionIntent;

impl IntentFunction for OvervoltageProtectionIntent {
    fn name(&self) -> &str {
        "overvoltage_protection"
    }
    
    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract voltage parameter
        let clamp_voltage = match params.get(0) {
            Some(IntentParam::Positional(IntentValue::Number(val, _))) => *val,
            _ => return Err("overvoltage_protection() requires voltage parameter".to_string()),
        };
        
        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!("TVS diode rated > {}V", clamp_voltage)),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "clamp_voltage_adequate".to_string(),
                    error_message: format!("Protection device must clamp below {}V", clamp_voltage * 1.2),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }
    
    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "voltage".to_string(),
                param_type: ParamType::Voltage,
                required: true,
                default_value: None,
            }
        ]
    }
}