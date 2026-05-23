// Analog circuit intent functions

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Current limiting intent for safety and protection
pub struct CurrentLimitingIntent;

impl IntentFunction for CurrentLimitingIntent {
    fn name(&self) -> &str {
        "current_limiting"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract max current parameter
        let max_current = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max" => {
                    Some((*val, unit.clone()))
                }
                IntentParam::Positional(IntentValue::Number(val, unit)) => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });

        let (current_val, current_unit) = max_current
            .ok_or_else(|| "current_limiting() requires max current parameter".to_string())?;

        let current_str = format!("{}{}", current_val, current_unit.as_deref().unwrap_or("A"));

        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!("Add current limiting resistor for max {}", current_str)),
                SynthesisHint::Custom("Consider current sense resistor".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "current_within_limit".to_string(),
                    error_message: format!("Current must not exceed {}", current_str),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "max".to_string(),
                param_type: ParamType::Current,
                required: true,
                default_value: None,
            },
        ]
    }
}

/// Level shifting intent for voltage domain translation
pub struct LevelShiftingIntent;

impl IntentFunction for LevelShiftingIntent {
    fn name(&self) -> &str {
        "level_shifting"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract from and to voltage levels
        let from_voltage = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "from" => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });

        let to_voltage = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "to" => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        });

        let from_v = from_voltage.ok_or_else(|| "level_shifting() requires 'from' voltage".to_string())?;
        let to_v = to_voltage.ok_or_else(|| "level_shifting() requires 'to' voltage".to_string())?;

        let from_str = format!("{}{}", from_v.0, from_v.1.as_deref().unwrap_or("V"));
        let to_str = format!("{}{}", to_v.0, to_v.1.as_deref().unwrap_or("V"));

        // Determine if we need active or passive level shifting
        let needs_active = from_v.0 > to_v.0 + 0.7; // voltage drop > 0.7V

        let hint = if needs_active {
            SynthesisHint::Custom(format!("Use level shifter IC for {} to {}", from_str, to_str))
        } else {
            SynthesisHint::Custom(format!("Use voltage divider for {} to {}", from_str, to_str))
        };

        Ok(IntentResult {
            sim_mode: SimMode::MixedSignal,
            synthesis_hints: vec![
                hint,
                SynthesisHint::Custom("Verify logic thresholds match".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: format!("input_voltage_is_{}", from_str),
                    error_message: format!("Input must be at {}", from_str),
                },
                ValidationRule {
                    condition: format!("output_voltage_is_{}", to_str),
                    error_message: format!("Output must be at {}", to_str),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "from".to_string(),
                param_type: ParamType::Voltage,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "to".to_string(),
                param_type: ParamType::Voltage,
                required: true,
                default_value: None,
            },
        ]
    }
}

/// Voltage division intent for resistive dividers
pub struct VoltageDivisionIntent;

impl IntentFunction for VoltageDivisionIntent {
    fn name(&self) -> &str {
        "voltage_division"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract division ratio
        let ratio = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::Number(val, _)) if name == "ratio" => {
                    Some(*val)
                }
                IntentParam::Positional(IntentValue::Number(val, _)) => {
                    Some(*val)
                }
                _ => None
            }
        });

        let ratio_val = ratio.ok_or_else(|| "voltage_division() requires ratio parameter".to_string())?;

        if ratio_val <= 0.0 || ratio_val > 1.0 {
            return Err("Voltage division ratio must be between 0 and 1".to_string());
        }

        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!("Use resistor divider with ratio {:.3}", ratio_val)),
                SynthesisHint::Custom("Consider load impedance effects".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: format!("output_is_{}x_input", ratio_val),
                    error_message: format!("Output should be {:.1}% of input", ratio_val * 100.0),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "ratio".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
        ]
    }
}

/// Signal amplification intent for gain stages
pub struct SignalAmplificationIntent;

impl IntentFunction for SignalAmplificationIntent {
    fn name(&self) -> &str {
        "signal_amplification"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract gain parameter (first positional or named "gain")
        let gain = params.get(0).and_then(|p| {
            match p {
                IntentParam::Positional(IntentValue::Number(val, unit)) => {
                    Some((*val, unit.clone()))
                }
                _ => None
            }
        }).or_else(|| {
            params.iter().find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "gain" => {
                        Some((*val, unit.clone()))
                    }
                    _ => None
                }
            })
        });

        // Extract bandwidth parameter (second positional or named "bandwidth")
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

        let gain_val = gain.ok_or_else(|| "signal_amplification() requires gain parameter".to_string())?;

        let gain_str = if gain_val.1.as_deref() == Some("dB") {
            format!("{}dB", gain_val.0)
        } else {
            format!("{}x", gain_val.0)
        };

        let mut synthesis_hints = vec![
            SynthesisHint::Custom(format!("Use amplifier with {} gain", gain_str)),
        ];

        if let Some((bw, unit)) = bandwidth {
            let bw_str = format!("{}{}", bw, unit.as_deref().unwrap_or("Hz"));
            synthesis_hints.push(SynthesisHint::Custom(format!("Bandwidth: {}", bw_str)));
        }

        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints,
            validation_rules: vec![
                ValidationRule {
                    condition: "gain_within_spec".to_string(),
                    error_message: format!("Amplifier gain must achieve {}", gain_str),
                }
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "gain".to_string(),
                param_type: ParamType::Number,
                required: true,
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

/// Amplifier intent for a gain stage.
///
/// `for amplifier(gain: N)` declares that a stage must deliver a small-signal
/// voltage gain of `N`. It carries no component values — the synthesizer's
/// operating-point designer (see `bhdl-spice/src/tube_bias.rs`) computes the
/// bias network from the intent. The grammar surface for the
/// simulate → parameterize → finalize loop.
pub struct AmplifierIntent;

impl IntentFunction for AmplifierIntent {
    fn name(&self) -> &str {
        "amplifier"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Gain — named `gain` or the first positional argument.
        let gain = params.iter().find_map(|p| match p {
            IntentParam::Named(name, IntentValue::Number(v, _)) if name == "gain" => Some(*v),
            _ => None,
        }).or_else(|| params.first().and_then(|p| match p {
            IntentParam::Positional(IntentValue::Number(v, _)) => Some(*v),
            _ => None,
        }));

        let gain = gain.ok_or_else(||
            "amplifier() requires a gain parameter, e.g. amplifier(gain: 15)".to_string())?;
        if gain <= 0.0 {
            return Err(format!("amplifier() gain must be positive, got {gain}"));
        }

        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!(
                    "Design the stage operating point for a voltage gain of {gain}")),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "gain_within_spec".to_string(),
                    error_message: format!("Stage must achieve a voltage gain of {gain}"),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "gain".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
        ]
    }
}

/// Current-source intent for a constant-current tail/load.
///
/// `for current_source(current: I)` declares that a stage must sink a
/// constant current `I` largely independent of the voltage across it. The
/// designer (see `bhdl-spice/src/tube_bias.rs`) sizes the cathode
/// degeneration resistor so the tube draws the requested plate current.
pub struct CurrentSourceIntent;

impl IntentFunction for CurrentSourceIntent {
    fn name(&self) -> &str {
        "current_source"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        let current = params.iter().find_map(|p| match p {
            IntentParam::Named(name, IntentValue::Number(v, unit)) if name == "current" =>
                Some((*v, unit.clone())),
            _ => None,
        }).or_else(|| params.first().and_then(|p| match p {
            IntentParam::Positional(IntentValue::Number(v, unit)) => Some((*v, unit.clone())),
            _ => None,
        }));

        let (i, unit) = current.ok_or_else(||
            "current_source() requires a current parameter, e.g. current_source(current: 5mA)".to_string())?;
        if i <= 0.0 {
            return Err(format!("current_source() current must be positive, got {i}"));
        }

        let pretty = format!("{}{}", i, unit.as_deref().unwrap_or("A"));
        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!(
                    "Size the cathode degeneration resistor for I_p = {pretty}")),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "current_within_spec".to_string(),
                    error_message: format!("Stage must sink {pretty}"),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "current".to_string(),
                param_type: ParamType::Current,
                required: true,
                default_value: None,
            },
        ]
    }
}

/// Switch intent — drive a triode hard between cutoff and saturation.
///
/// `for digital_switch()` declares that a stage is operated as a digital
/// switch: at cutoff the plate sits at V_bb (off), at saturation it pulls
/// down to near ground (on). The designer (see `bhdl-spice/src/tube_bias.rs`)
/// sizes the plate-load R_p so the saturated state lands at a clean low
/// rail. (The intent name avoids `switch`, which is a reserved BHDL keyword
/// used for `switch out` pin types.)
pub struct SwitchIntent;

impl IntentFunction for SwitchIntent {
    fn name(&self) -> &str {
        "digital_switch"
    }

    fn resolve(&self, _params: &[IntentParam]) -> Result<IntentResult, String> {
        Ok(IntentResult {
            sim_mode: SimMode::AnalogRequired,
            synthesis_hints: vec![
                SynthesisHint::Custom(
                    "Size the plate-load R_p so saturation lands at a clean \
                     low rail".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: "rails_separated".to_string(),
                    error_message: "Saturation and cutoff plate voltages must \
                                    leave usable headroom".to_string(),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        // No required parameters; the topology + V_bb determine the design.
        vec![]
    }
}
