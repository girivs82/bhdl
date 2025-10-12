//! Safety-critical system intent functions
//!
//! Intents for automotive, industrial, medical, and other safety-critical applications

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Automotive safety requirements (ISO 26262 ASIL levels)
pub struct AutomotiveSafetyIntent;

impl IntentFunction for AutomotiveSafetyIntent {
    fn name(&self) -> &str {
        "automotive_safety"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract ASIL level
        let asil_level = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "level" => Some(s.clone()),
                    IntentParam::Positional(IntentValue::String(s)) => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "automotive_safety requires 'level' parameter (ASIL_A/B/C/D)".to_string())?;

        // Validate ASIL level
        let valid_levels = ["ASIL_A", "ASIL_B", "ASIL_C", "ASIL_D"];
        if !valid_levels.contains(&asil_level.as_str()) {
            return Err(format!("Invalid ASIL level '{}'. Must be one of: ASIL_A, ASIL_B, ASIL_C, ASIL_D", asil_level));
        }

        // Extract optional redundancy parameter
        let redundancy = params.iter().any(|p| {
            matches!(p, IntentParam::Named(name, IntentValue::Boolean(true)) if name == "redundancy")
        });

        // ASIL-D and ASIL-C require AnalogRequired
        let sim_mode = match asil_level.as_str() {
            "ASIL_D" | "ASIL_C" => SimMode::AnalogRequired,
            "ASIL_B" | "ASIL_A" => SimMode::MixedSignal,
            _ => SimMode::AnalogRequired,
        };

        let mut synthesis_hints = vec![
            SynthesisHint::Custom(format!("ISO 26262 {} compliance required", asil_level)),
        ];

        if redundancy {
            synthesis_hints.push(SynthesisHint::Custom("Implement redundant signal paths".to_string()));
        }

        let validation_rules = vec![
            ValidationRule {
                condition: format!("automotive_asil_{}", asil_level.to_lowercase()),
                error_message: format!("Circuit must meet ISO 26262 {} requirements", asil_level),
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
                name: "level".to_string(),
                param_type: ParamType::String,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "redundancy".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                default_value: Some(IntentValue::Boolean(false)),
            },
        ]
    }
}

/// Industrial control safety requirements (IEC 61508 SIL / ISO 13849 PL)
pub struct IndustrialControlIntent;

impl IntentFunction for IndustrialControlIntent {
    fn name(&self) -> &str {
        "industrial_control"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract safety category
        let safety_category = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "safety_category" => Some(s.clone()),
                    IntentParam::Positional(IntentValue::String(s)) => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "industrial_control requires 'safety_category' parameter (CAT1/CAT2/CAT3/CAT4)".to_string())?;

        // Validate safety category
        let valid_categories = ["CAT1", "CAT2", "CAT3", "CAT4"];
        if !valid_categories.contains(&safety_category.as_str()) {
            return Err(format!("Invalid safety category '{}'. Must be one of: CAT1, CAT2, CAT3, CAT4", safety_category));
        }

        // Optional SIL level
        let sil_level = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::String(s)) if name == "sil_level" => Some(s.clone()),
                _ => None
            }
        });

        // Validate SIL level if provided
        if let Some(ref sil) = sil_level {
            let valid_sil = ["SIL1", "SIL2", "SIL3", "SIL4"];
            if !valid_sil.contains(&sil.as_str()) {
                return Err(format!("Invalid SIL level '{}'. Must be one of: SIL1, SIL2, SIL3, SIL4", sil));
            }
        }

        let emergency_stop = params.iter().any(|p| {
            matches!(p, IntentParam::Named(name, IntentValue::Boolean(true)) if name == "emergency_stop")
        });

        // CAT3 and CAT4 require full analog analysis
        let sim_mode = match safety_category.as_str() {
            "CAT4" | "CAT3" => SimMode::AnalogRequired,
            "CAT2" | "CAT1" => SimMode::MixedSignal,
            _ => SimMode::AnalogRequired,
        };

        let mut synthesis_hints = vec![
            SynthesisHint::Custom(format!("ISO 13849 {} compliance required", safety_category)),
        ];

        if let Some(ref sil) = sil_level {
            synthesis_hints.push(SynthesisHint::Custom(format!("IEC 61508 {} compliance required", sil)));
        }

        if emergency_stop {
            synthesis_hints.push(SynthesisHint::Custom("Emergency stop circuit - redundancy required".to_string()));
        }

        let mut validation_rules = vec![
            ValidationRule {
                condition: format!("industrial_safety_{}", safety_category.to_lowercase()),
                error_message: format!("Circuit must meet ISO 13849 {} requirements", safety_category),
            },
        ];

        if emergency_stop {
            validation_rules.push(ValidationRule {
                condition: "emergency_stop_redundant".to_string(),
                error_message: "Emergency stop circuits require dual-channel redundancy".to_string(),
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
                name: "safety_category".to_string(),
                param_type: ParamType::String,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "sil_level".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "emergency_stop".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                default_value: Some(IntentValue::Boolean(false)),
            },
        ]
    }
}

/// Medical device safety requirements (IEC 60601, FDA Class I/II/III)
pub struct MedicalSafetyIntent;

impl IntentFunction for MedicalSafetyIntent {
    fn name(&self) -> &str {
        "medical_safety"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract device class
        let device_class = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "class" => Some(s.clone()),
                    IntentParam::Positional(IntentValue::String(s)) => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "medical_safety requires 'class' parameter (I, II, or III)".to_string())?;

        // Validate device class
        let valid_classes = ["I", "II", "III"];
        if !valid_classes.contains(&device_class.as_str()) {
            return Err(format!("Invalid device class '{}'. Must be one of: I, II, III", device_class));
        }

        let patient_connected = params.iter().any(|p| {
            matches!(p, IntentParam::Named(name, IntentValue::Boolean(true)) if name == "patient_connected")
        });

        // Class III and patient-connected devices require full analog simulation
        let sim_mode = if device_class == "III" || patient_connected {
            SimMode::AnalogRequired
        } else if device_class == "II" {
            SimMode::MixedSignal
        } else {
            SimMode::MixedSignal
        };

        let mut synthesis_hints = vec![
            SynthesisHint::Custom(format!("FDA Class {} medical device compliance", device_class)),
            SynthesisHint::Custom("IEC 60601 medical electrical equipment standard".to_string()),
        ];

        if patient_connected {
            synthesis_hints.push(SynthesisHint::Custom("Patient isolation required (2.5kV minimum)".to_string()));
        }

        let mut validation_rules = vec![
            ValidationRule {
                condition: format!("medical_device_class_{}", device_class),
                error_message: format!("Circuit must meet FDA Class {} requirements", device_class),
            },
            ValidationRule {
                condition: "iec_60601_compliance".to_string(),
                error_message: "Circuit must comply with IEC 60601 medical device standards".to_string(),
            },
        ];

        if patient_connected {
            validation_rules.push(ValidationRule {
                condition: "patient_isolation >= 2500V".to_string(),
                error_message: "Patient-connected circuits require ≥2.5kV isolation".to_string(),
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
                name: "class".to_string(),
                param_type: ParamType::String,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "patient_connected".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                default_value: Some(IntentValue::Boolean(false)),
            },
        ]
    }
}

/// ESD (Electrostatic Discharge) protection requirements
pub struct EsdProtectionIntent;

impl IntentFunction for EsdProtectionIntent {
    fn name(&self) -> &str {
        "esd_protection"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract ESD protection level (voltage)
        let esd_level = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "level" => {
                        // Convert to volts
                        let volts = match unit.as_deref() {
                            Some("kV") => val * 1000.0,
                            Some("V") | None => *val,
                            _ => return None,
                        };
                        Some(volts)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let volts = match unit.as_deref() {
                            Some("kV") => val * 1000.0,
                            Some("V") | None => *val,
                            _ => return None,
                        };
                        Some(volts)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "esd_protection requires 'level' parameter (voltage)".to_string())?;

        // Validate reasonable range (1kV to 30kV)
        if esd_level < 1000.0 || esd_level > 30000.0 {
            return Err(format!("ESD level {:.1}kV is outside typical range (1kV to 30kV)", esd_level / 1000.0));
        }

        // Optional standard
        let standard = params.iter().find_map(|p| {
            match p {
                IntentParam::Named(name, IntentValue::String(s)) if name == "standard" => Some(s.clone()),
                _ => None
            }
        }).unwrap_or_else(|| "IEC_61000_4_2".to_string());

        // Validate standard
        let valid_standards = ["IEC_61000_4_2", "HBM", "CDM"];
        if !valid_standards.contains(&standard.as_str()) {
            return Err(format!("Invalid ESD standard '{}'. Must be one of: IEC_61000_4_2, HBM, CDM", standard));
        }

        let contact_discharge = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Boolean(b)) if name == "contact_discharge" => Some(*b),
                    _ => None
                }
            })
            .unwrap_or(true);

        // High ESD levels (>4kV) require analog simulation to verify protection
        let sim_mode = if esd_level >= 4000.0 {
            SimMode::AnalogRequired
        } else {
            SimMode::MixedSignal
        };

        let discharge_type = if contact_discharge { "contact" } else { "air" };

        let synthesis_hints = vec![
            SynthesisHint::Custom(format!(
                "ESD protection to {:.1}kV ({} discharge, {} standard)",
                esd_level / 1000.0,
                discharge_type,
                standard
            )),
            SynthesisHint::Custom("Use TVS diodes or ESD protection diodes on exposed pins".to_string()),
        ];

        let validation_rules = vec![
            ValidationRule {
                condition: format!("esd_protection >= {}V", esd_level),
                error_message: format!("Circuit must withstand {:.1}kV ESD events", esd_level / 1000.0),
            },
            ValidationRule {
                condition: format!("esd_standard_{}", standard),
                error_message: format!("Circuit must pass {} ESD testing", standard),
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
                name: "level".to_string(),
                param_type: ParamType::Voltage,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "standard".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: Some(IntentValue::String("IEC_61000_4_2".to_string())),
            },
            ParamMetadata {
                name: "contact_discharge".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                default_value: Some(IntentValue::Boolean(true)),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_automotive_safety_asil_d() {
        let intent = AutomotiveSafetyIntent;
        let params = vec![
            IntentParam::Named("level".to_string(), IntentValue::String("ASIL_D".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::AnalogRequired);
        assert!(!result.synthesis_hints.is_empty());
        assert!(!result.validation_rules.is_empty());
    }

    #[test]
    fn test_automotive_safety_with_redundancy() {
        let intent = AutomotiveSafetyIntent;
        let params = vec![
            IntentParam::Named("level".to_string(), IntentValue::String("ASIL_C".to_string())),
            IntentParam::Named("redundancy".to_string(), IntentValue::Boolean(true)),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::AnalogRequired);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("redundant"))
        }));
    }

    #[test]
    fn test_automotive_safety_invalid_level() {
        let intent = AutomotiveSafetyIntent;
        let params = vec![
            IntentParam::Named("level".to_string(), IntentValue::String("ASIL_E".to_string())),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid ASIL level"));
    }

    #[test]
    fn test_industrial_control_cat4() {
        let intent = IndustrialControlIntent;
        let params = vec![
            IntentParam::Named("safety_category".to_string(), IntentValue::String("CAT4".to_string())),
            IntentParam::Named("sil_level".to_string(), IntentValue::String("SIL3".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::AnalogRequired);
        assert!(result.synthesis_hints.len() >= 2); // Both ISO 13849 and IEC 61508
    }

    #[test]
    fn test_industrial_control_emergency_stop() {
        let intent = IndustrialControlIntent;
        let params = vec![
            IntentParam::Named("safety_category".to_string(), IntentValue::String("CAT3".to_string())),
            IntentParam::Named("emergency_stop".to_string(), IntentValue::Boolean(true)),
        ];

        let result = intent.resolve(&params).unwrap();

        assert!(result.validation_rules.iter().any(|r| {
            r.condition.contains("emergency_stop")
        }));
    }

    #[test]
    fn test_medical_safety_class_iii() {
        let intent = MedicalSafetyIntent;
        let params = vec![
            IntentParam::Named("class".to_string(), IntentValue::String("III".to_string())),
            IntentParam::Named("patient_connected".to_string(), IntentValue::Boolean(true)),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::AnalogRequired);
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("isolation"))
        }));
    }

    #[test]
    fn test_esd_protection_8kv() {
        let intent = EsdProtectionIntent;
        let params = vec![
            IntentParam::Named("level".to_string(), IntentValue::Number(8.0, Some("kV".to_string()))),
            IntentParam::Named("standard".to_string(), IntentValue::String("IEC_61000_4_2".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::AnalogRequired);
        assert!(!result.validation_rules.is_empty());
    }

    #[test]
    fn test_esd_protection_hbm() {
        let intent = EsdProtectionIntent;
        let params = vec![
            IntentParam::Named("level".to_string(), IntentValue::Number(2000.0, Some("V".to_string()))),
            IntentParam::Named("standard".to_string(), IntentValue::String("HBM".to_string())),
            IntentParam::Named("contact_discharge".to_string(), IntentValue::Boolean(false)),
        ];

        let result = intent.resolve(&params).unwrap();

        assert_eq!(result.sim_mode, SimMode::MixedSignal); // <4kV
        assert!(result.synthesis_hints.iter().any(|h| {
            matches!(h, SynthesisHint::Custom(s) if s.contains("air discharge"))
        }));
    }

    #[test]
    fn test_esd_protection_invalid_level() {
        let intent = EsdProtectionIntent;
        let params = vec![
            IntentParam::Named("level".to_string(), IntentValue::Number(100.0, Some("V".to_string()))),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("outside typical range"));
    }
}
