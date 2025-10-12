//! Advanced feature intent functions
//!
//! This module provides intent functions for advanced electrical engineering
//! concerns including signal integrity, EMI/EMC compliance, isolation, and
//! thermal management.

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Signal integrity - ensures proper impedance matching and minimal reflections
///
/// Used for high-speed digital signals, RF paths, and transmission lines.
/// Requires precise impedance control and minimal signal reflections.
///
/// # Parameters
/// - `impedance`: Required. Characteristic impedance in Ω (e.g., 50Ω, 75Ω, 100Ω)
/// - `max_reflection`: Optional. Maximum reflection coefficient in dB (default: -20dB)
/// - `max_crosstalk`: Optional. Maximum crosstalk in dB (default: -40dB)
///
/// # Examples
/// ```bhdl
/// net high_speed: @source -> trace -> @destination
///     for signal_integrity(impedance: 50, max_reflection: -20dB);
/// ```
pub struct SignalIntegrityIntent;

impl IntentFunction for SignalIntegrityIntent {
    fn name(&self) -> &str {
        "signal_integrity"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract impedance parameter (required)
        let impedance = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, _)) if name == "impedance" => Some(*val),
                    IntentParam::Positional(IntentValue::Number(val, _)) => Some(*val),
                    _ => None
                }
            })
            .ok_or_else(|| "signal_integrity requires 'impedance' parameter (e.g., 50Ω, 75Ω)".to_string())?;

        // Validate impedance is positive
        if impedance <= 0.0 {
            return Err(format!("Impedance must be positive, got {}", impedance));
        }

        // Extract max_reflection with unit conversion (dB)
        let max_reflection_db = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max_reflection" => {
                        let db = match unit.as_deref() {
                            Some("dB") => *val,
                            _ => *val, // Assume dB if no unit
                        };
                        Some(db)
                    }
                    _ => None
                }
            })
            .unwrap_or(-20.0); // Default -20dB

        // Extract max_crosstalk
        let max_crosstalk_db = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max_crosstalk" => {
                        let db = match unit.as_deref() {
                            Some("dB") => *val,
                            _ => *val,
                        };
                        Some(db)
                    }
                    _ => None
                }
            })
            .unwrap_or(-40.0); // Default -40dB

        // SimMode based on impedance requirements
        let sim_mode = if impedance == 50.0 || impedance == 75.0 {
            SimMode::MixedSignal // Standard RF/high-speed impedances
        } else {
            SimMode::AnalogRequired // Non-standard impedances need detailed analysis
        };

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!(
                    "Controlled impedance: {:.0}Ω trace, max reflection {:.1}dB",
                    impedance, max_reflection_db
                )),
                SynthesisHint::Custom(format!(
                    "PCB trace width and spacing for {:.0}Ω characteristic impedance",
                    impedance
                )),
                SynthesisHint::Custom("Consider series termination resistors at source".to_string()),
                SynthesisHint::Custom("Consider parallel termination at destination".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: format!("impedance_match_{}ohm", impedance),
                    error_message: format!(
                        "Trace impedance must match {:.0}Ω ±10%",
                        impedance
                    ),
                },
                ValidationRule {
                    condition: format!("reflection_max_{}dB", max_reflection_db),
                    error_message: format!(
                        "Signal reflection must be below {:.1}dB",
                        max_reflection_db
                    ),
                },
                ValidationRule {
                    condition: format!("crosstalk_max_{}dB", max_crosstalk_db),
                    error_message: format!(
                        "Crosstalk between adjacent signals must be below {:.1}dB",
                        max_crosstalk_db
                    ),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "impedance".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "max_reflection".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(-20.0, Some("dB".to_string()))),
            },
            ParamMetadata {
                name: "max_crosstalk".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(-40.0, Some("dB".to_string()))),
            },
        ]
    }
}

/// EMI filtering - provides electromagnetic interference filtering
///
/// Used for EMC compliance, ensures the design meets EMI/RFI emission standards.
/// Critical for CE, FCC, and other regulatory compliance.
///
/// # Parameters
/// - `class`: Required. EMC class (e.g., "CISPR11_ClassB", "FCC_ClassB", "EN55011_ClassA")
/// - `frequency`: Optional. Target frequency range in Hz/MHz/GHz
/// - `attenuation`: Optional. Required attenuation in dB (default: -40dB)
///
/// # Examples
/// ```bhdl
/// net power_filtered: @input -> filter: EMIFilter().IN -> filter.OUT -> @clean_power
///     for emi_filtering(class: "CISPR11_ClassB", attenuation: -40dB);
/// ```
pub struct EmiFilteringIntent;

impl IntentFunction for EmiFilteringIntent {
    fn name(&self) -> &str {
        "emi_filtering"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract EMC class parameter (required)
        let emc_class = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "class" => Some(s.clone()),
                    IntentParam::Positional(IntentValue::String(s)) => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "emi_filtering requires 'class' parameter (e.g., CISPR11_ClassB, FCC_ClassB)".to_string())?;

        // Validate EMC class
        let valid_classes = [
            "CISPR11_ClassA", "CISPR11_ClassB",
            "FCC_ClassA", "FCC_ClassB",
            "EN55011_ClassA", "EN55011_ClassB",
            "EN55022_ClassA", "EN55022_ClassB",
        ];

        if !valid_classes.contains(&emc_class.as_str()) {
            return Err(format!(
                "Invalid EMC class '{}'. Valid options: {}",
                emc_class,
                valid_classes.join(", ")
            ));
        }

        // Extract frequency with unit conversion
        let frequency_hz = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "frequency" => {
                        let hz = match unit.as_deref() {
                            Some("Hz") => *val,
                            Some("kHz") => val * 1_000.0,
                            Some("MHz") => val * 1_000_000.0,
                            Some("GHz") => val * 1_000_000_000.0,
                            _ => *val,
                        };
                        Some(hz)
                    }
                    _ => None
                }
            });

        // Extract attenuation
        let attenuation_db = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "attenuation" => {
                        let db = match unit.as_deref() {
                            Some("dB") => *val,
                            _ => *val,
                        };
                        Some(db)
                    }
                    _ => None
                }
            })
            .unwrap_or(-40.0); // Default -40dB

        // Class B is more stringent than Class A
        let sim_mode = if emc_class.contains("ClassB") {
            SimMode::AnalogRequired // Strict requirements need detailed analysis
        } else {
            SimMode::MixedSignal
        };

        let mut hints = vec![
            SynthesisHint::AnalogFilter,
            SynthesisHint::Custom(format!("EMI filter for {} compliance", emc_class)),
            SynthesisHint::Custom("Consider common-mode and differential-mode filtering".to_string()),
        ];

        if let Some(freq) = frequency_hz {
            hints.push(SynthesisHint::Custom(format!(
                "Target filtering at {:.0} Hz",
                freq
            )));
        }

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("emc_compliance_{}", emc_class),
                    error_message: format!("Must meet {} EMC standard", emc_class),
                },
                ValidationRule {
                    condition: format!("emi_attenuation_{}dB", attenuation_db.abs()),
                    error_message: format!(
                        "EMI filter must provide at least {:.1}dB attenuation",
                        attenuation_db.abs()
                    ),
                },
            ],
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
                name: "frequency".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "attenuation".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(-40.0, Some("dB".to_string()))),
            },
        ]
    }
}

/// Isolation - provides electrical isolation between circuits
///
/// Used for safety-critical applications, high-voltage protection, and
/// ground loop elimination. Supports galvanic, capacitive, and optical isolation.
///
/// # Parameters
/// - `voltage`: Required. Isolation voltage rating in V/kV
/// - `type`: Required. Isolation type ("galvanic", "capacitive", "optical", "magnetic")
/// - `test_voltage`: Optional. Test voltage for certification (default: 2x rated)
///
/// # Examples
/// ```bhdl
/// net isolated: @high_voltage -> isolator: Optocoupler().IN -> isolator.OUT -> @low_voltage
///     for isolation(voltage: 2500V, type: "galvanic");
/// ```
pub struct IsolationIntent;

impl IntentFunction for IsolationIntent {
    fn name(&self) -> &str {
        "isolation"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract voltage parameter (required)
        let voltage = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "voltage" => {
                        let volts = match unit.as_deref() {
                            Some("kV") => val * 1000.0,
                            Some("V") => *val,
                            _ => *val,
                        };
                        Some(volts)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let volts = match unit.as_deref() {
                            Some("kV") => val * 1000.0,
                            Some("V") => *val,
                            _ => *val,
                        };
                        Some(volts)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "isolation requires 'voltage' parameter (e.g., 2500V, 5kV)".to_string())?;

        if voltage <= 0.0 {
            return Err("Isolation voltage must be positive".to_string());
        }

        // Extract isolation type (required)
        let isolation_type = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "type" => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "isolation requires 'type' parameter (galvanic, capacitive, optical, magnetic)".to_string())?;

        // Validate isolation type
        let valid_types = ["galvanic", "capacitive", "optical", "magnetic"];
        if !valid_types.contains(&isolation_type.as_str()) {
            return Err(format!(
                "Invalid isolation type '{}'. Valid options: {}",
                isolation_type,
                valid_types.join(", ")
            ));
        }

        // Extract test voltage (default 2x rated)
        let test_voltage = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "test_voltage" => {
                        let volts = match unit.as_deref() {
                            Some("kV") => val * 1000.0,
                            Some("V") => *val,
                            _ => *val,
                        };
                        Some(volts)
                    }
                    _ => None
                }
            })
            .unwrap_or(voltage * 2.0);

        // High voltage isolation always requires analog analysis
        let sim_mode = if voltage >= 1000.0 {
            SimMode::AnalogRequired
        } else {
            SimMode::MixedSignal
        };

        let component_hint = match isolation_type.as_str() {
            "galvanic" => "transformer or relay",
            "capacitive" => "capacitive isolator IC (e.g., Si86xx)",
            "optical" => "optocoupler or fiber optic transceiver",
            "magnetic" => "transformer or isolated DC-DC converter",
            _ => "isolation component",
        };

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!(
                    "{} isolation rated for {:.0}V",
                    isolation_type, voltage
                )),
                SynthesisHint::Custom(format!("Consider {} for isolation", component_hint)),
                SynthesisHint::Custom(format!(
                    "Test at {:.0}V for safety certification",
                    test_voltage
                )),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: format!("isolation_voltage_{:.0}V", voltage),
                    error_message: format!(
                        "Isolation barrier must be rated for minimum {:.0}V",
                        voltage
                    ),
                },
                ValidationRule {
                    condition: format!("isolation_test_{:.0}V", test_voltage),
                    error_message: format!(
                        "Isolation must pass {:.0}V test voltage",
                        test_voltage
                    ),
                },
                ValidationRule {
                    condition: "creepage_clearance".to_string(),
                    error_message: "PCB creepage and clearance distances must meet safety standards (IEC 60664)".to_string(),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "voltage".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "type".to_string(),
                param_type: ParamType::String,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "test_voltage".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// Thermal management - ensures proper thermal design
///
/// Used for power electronics, high-current paths, and temperature-sensitive
/// components. Ensures junction temperatures stay within safe limits.
///
/// # Parameters
/// - `max_temp`: Required. Maximum junction/ambient temperature in °C
/// - `power`: Optional. Maximum power dissipation in W/mW
/// - `thermal_resistance`: Optional. Maximum thermal resistance in °C/W
///
/// # Examples
/// ```bhdl
/// net power_path: @input -> mosfet: MOSFET().D -> mosfet.S -> @load
///     for thermal_management(max_temp: 85C, power: 5W);
/// ```
pub struct ThermalManagementIntent;

impl IntentFunction for ThermalManagementIntent {
    fn name(&self) -> &str {
        "thermal_management"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract max_temp parameter (required)
        let max_temp_c = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max_temp" => {
                        let celsius = match unit.as_deref() {
                            Some("C") | Some("°C") => *val,
                            Some("K") => val - 273.15,
                            Some("F") | Some("°F") => (val - 32.0) * 5.0 / 9.0,
                            _ => *val, // Assume Celsius
                        };
                        Some(celsius)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let celsius = match unit.as_deref() {
                            Some("C") | Some("°C") => *val,
                            Some("K") => val - 273.15,
                            Some("F") | Some("°F") => (val - 32.0) * 5.0 / 9.0,
                            _ => *val,
                        };
                        Some(celsius)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "thermal_management requires 'max_temp' parameter (e.g., 85C, 125C)".to_string())?;

        if max_temp_c < -273.15 {
            return Err("Temperature cannot be below absolute zero".to_string());
        }

        // Extract power with unit conversion
        let power_w = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "power" => {
                        let watts = match unit.as_deref() {
                            Some("W") => *val,
                            Some("mW") => val / 1000.0,
                            Some("kW") => val * 1000.0,
                            _ => *val,
                        };
                        Some(watts)
                    }
                    _ => None
                }
            });

        // Extract thermal resistance
        let thermal_resistance = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, _)) if name == "thermal_resistance" => Some(*val),
                    _ => None
                }
            });

        // SimMode based on power level
        let sim_mode = if let Some(power) = power_w {
            if power > 1.0 {
                SimMode::AnalogRequired // High power needs detailed thermal analysis
            } else {
                SimMode::MixedSignal
            }
        } else {
            SimMode::MixedSignal
        };

        let mut hints = vec![
            SynthesisHint::Custom(format!("Maximum temperature: {:.1}°C", max_temp_c)),
        ];

        if let Some(power) = power_w {
            hints.push(SynthesisHint::Custom(format!(
                "Maximum power dissipation: {:.2}W",
                power
            )));

            if power > 1.0 {
                hints.push(SynthesisHint::Custom("Consider heatsink or thermal pad".to_string()));
            }
        }

        if let Some(r_th) = thermal_resistance {
            hints.push(SynthesisHint::Custom(format!(
                "Thermal resistance target: {:.1}°C/W",
                r_th
            )));
        }

        hints.push(SynthesisHint::Custom("Ensure adequate PCB copper pour for heat spreading".to_string()));

        let mut validation_rules = vec![
            ValidationRule {
                condition: format!("junction_temp_max_{:.0}C", max_temp_c),
                error_message: format!(
                    "Junction temperature must not exceed {:.1}°C",
                    max_temp_c
                ),
            },
        ];

        if let Some(power) = power_w {
            validation_rules.push(ValidationRule {
                condition: format!("power_dissipation_max_{:.2}W", power),
                error_message: format!(
                    "Power dissipation must not exceed {:.2}W",
                    power
                ),
            });
        }

        if let Some(r_th) = thermal_resistance {
            validation_rules.push(ValidationRule {
                condition: format!("thermal_resistance_max_{:.1}", r_th),
                error_message: format!(
                    "Thermal resistance must be below {:.1}°C/W",
                    r_th
                ),
            });
        }

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: hints,
            validation_rules,
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "max_temp".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "power".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "thermal_resistance".to_string(),
                param_type: ParamType::Number,
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
    fn test_signal_integrity_basic() {
        let intent = SignalIntegrityIntent;
        let params = vec![
            IntentParam::Named("impedance".to_string(), IntentValue::Number(50.0, None)),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.synthesis_hints.len() >= 3);
        assert_eq!(result.validation_rules.len(), 3);
    }

    #[test]
    fn test_signal_integrity_with_reflection() {
        let intent = SignalIntegrityIntent;
        let params = vec![
            IntentParam::Named("impedance".to_string(), IntentValue::Number(75.0, None)),
            IntentParam::Named("max_reflection".to_string(), IntentValue::Number(-25.0, Some("dB".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal);
    }

    #[test]
    fn test_signal_integrity_invalid_impedance() {
        let intent = SignalIntegrityIntent;
        let params = vec![
            IntentParam::Named("impedance".to_string(), IntentValue::Number(-50.0, None)),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("must be positive"));
    }

    #[test]
    fn test_emi_filtering_basic() {
        let intent = EmiFilteringIntent;
        let params = vec![
            IntentParam::Named("class".to_string(), IntentValue::String("CISPR11_ClassB".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::AnalogRequired); // ClassB is strict
        assert!(result.synthesis_hints.len() >= 2);
    }

    #[test]
    fn test_emi_filtering_class_a() {
        let intent = EmiFilteringIntent;
        let params = vec![
            IntentParam::Named("class".to_string(), IntentValue::String("FCC_ClassA".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal); // ClassA is less strict
    }

    #[test]
    fn test_emi_filtering_invalid_class() {
        let intent = EmiFilteringIntent;
        let params = vec![
            IntentParam::Named("class".to_string(), IntentValue::String("InvalidClass".to_string())),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid EMC class"));
    }

    #[test]
    fn test_isolation_basic() {
        let intent = IsolationIntent;
        let params = vec![
            IntentParam::Named("voltage".to_string(), IntentValue::Number(2500.0, Some("V".to_string()))),
            IntentParam::Named("type".to_string(), IntentValue::String("galvanic".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::AnalogRequired); // High voltage
        assert_eq!(result.validation_rules.len(), 3);
    }

    #[test]
    fn test_isolation_optical() {
        let intent = IsolationIntent;
        let params = vec![
            IntentParam::Named("voltage".to_string(), IntentValue::Number(500.0, Some("V".to_string()))),
            IntentParam::Named("type".to_string(), IntentValue::String("optical".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal); // Lower voltage
    }

    #[test]
    fn test_isolation_invalid_type() {
        let intent = IsolationIntent;
        let params = vec![
            IntentParam::Named("voltage".to_string(), IntentValue::Number(2500.0, Some("V".to_string()))),
            IntentParam::Named("type".to_string(), IntentValue::String("invalid".to_string())),
        ];

        let result = intent.resolve(&params);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Invalid isolation type"));
    }

    #[test]
    fn test_thermal_management_basic() {
        let intent = ThermalManagementIntent;
        let params = vec![
            IntentParam::Named("max_temp".to_string(), IntentValue::Number(85.0, Some("C".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.synthesis_hints.len() >= 2);
    }

    #[test]
    fn test_thermal_management_high_power() {
        let intent = ThermalManagementIntent;
        let params = vec![
            IntentParam::Named("max_temp".to_string(), IntentValue::Number(125.0, Some("C".to_string()))),
            IntentParam::Named("power".to_string(), IntentValue::Number(5.0, Some("W".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::AnalogRequired); // High power
        assert!(result.validation_rules.len() >= 2);
    }

    #[test]
    fn test_thermal_management_with_resistance() {
        let intent = ThermalManagementIntent;
        let params = vec![
            IntentParam::Named("max_temp".to_string(), IntentValue::Number(85.0, Some("C".to_string()))),
            IntentParam::Named("thermal_resistance".to_string(), IntentValue::Number(10.0, None)),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.validation_rules.len(), 2);
    }
}
