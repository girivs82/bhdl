//! Specialized intent functions
//!
//! This module provides intent functions for specialized applications including
//! voltage regulation, current sensing, communication interfaces, system monitoring,
//! power optimization, test/debug access, and redundancy/fault tolerance.

use bhdl_common::{
    IntentFunction, IntentParam, IntentResult, IntentValue,
    SimMode, SynthesisHint, ValidationRule, ToolScope,
    ParamMetadata, ParamType
};

/// Voltage regulation - ensures precise voltage regulation with specified requirements
///
/// Used for regulated power supplies with specific load regulation, line regulation,
/// and ripple requirements. Critical for precision analog circuits and sensitive loads.
///
/// # Parameters
/// - `output_voltage`: Required. Target output voltage in V/mV
/// - `load_regulation`: Optional. Maximum load regulation in mV or % (default: 1%)
/// - `line_regulation`: Optional. Maximum line regulation in mV or % (default: 0.5%)
/// - `ripple`: Optional. Maximum output ripple in mV (default: 50mV)
///
/// # Examples
/// ```bhdl
/// net regulated: @input -> regulator: LDO().IN -> regulator.OUT -> @vout
///     for voltage_regulation(output_voltage: 3.3V, load_regulation: 1%, ripple: 20mV);
/// ```
pub struct VoltageRegulationIntent;

impl IntentFunction for VoltageRegulationIntent {
    fn name(&self) -> &str {
        "voltage_regulation"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract output_voltage parameter (required)
        let output_voltage = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "output_voltage" => {
                        let volts = match unit.as_deref() {
                            Some("V") => *val,
                            Some("mV") => val / 1000.0,
                            _ => *val,
                        };
                        Some(volts)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let volts = match unit.as_deref() {
                            Some("V") => *val,
                            Some("mV") => val / 1000.0,
                            _ => *val,
                        };
                        Some(volts)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "voltage_regulation requires 'output_voltage' parameter".to_string())?;

        if output_voltage <= 0.0 {
            return Err("Output voltage must be positive".to_string());
        }

        // Extract load_regulation (can be % or absolute mV)
        let load_regulation_percent = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "load_regulation" => {
                        let percent = match unit.as_deref() {
                            Some("%") => *val,
                            Some("mV") => (val / (output_voltage * 1000.0)) * 100.0,
                            _ => *val, // Assume percent
                        };
                        Some(percent)
                    }
                    _ => None
                }
            })
            .unwrap_or(1.0); // Default 1%

        // Extract line_regulation
        let line_regulation_percent = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "line_regulation" => {
                        let percent = match unit.as_deref() {
                            Some("%") => *val,
                            Some("mV") => (val / (output_voltage * 1000.0)) * 100.0,
                            _ => *val,
                        };
                        Some(percent)
                    }
                    _ => None
                }
            })
            .unwrap_or(0.5); // Default 0.5%

        // Extract ripple in mV
        let ripple_mv = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "ripple" => {
                        let millivolts = match unit.as_deref() {
                            Some("mV") => *val,
                            Some("V") => val * 1000.0,
                            _ => *val,
                        };
                        Some(millivolts)
                    }
                    _ => None
                }
            })
            .unwrap_or(50.0); // Default 50mV

        // Tight regulation specs require analog analysis
        let sim_mode = if load_regulation_percent < 0.5 || line_regulation_percent < 0.1 || ripple_mv < 10.0 {
            SimMode::AnalogRequired
        } else {
            SimMode::MixedSignal
        };

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!(
                    "Regulated output: {:.2}V ±{:.1}%",
                    output_voltage, load_regulation_percent
                )),
                SynthesisHint::Custom(format!(
                    "Load regulation: {:.2}% max",
                    load_regulation_percent
                )),
                SynthesisHint::Custom(format!(
                    "Line regulation: {:.2}% max",
                    line_regulation_percent
                )),
                SynthesisHint::Custom(format!(
                    "Output ripple: {:.1}mV max",
                    ripple_mv
                )),
                SynthesisHint::Custom("Consider low-dropout (LDO) regulator for low ripple".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: format!("output_voltage_{:.2}V", output_voltage),
                    error_message: format!(
                        "Output voltage must be {:.2}V ±{:.1}%",
                        output_voltage, load_regulation_percent
                    ),
                },
                ValidationRule {
                    condition: format!("ripple_max_{:.1}mV", ripple_mv),
                    error_message: format!("Output ripple must not exceed {:.1}mV", ripple_mv),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "output_voltage".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "load_regulation".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(1.0, Some("%".to_string()))),
            },
            ParamMetadata {
                name: "line_regulation".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(0.5, Some("%".to_string()))),
            },
            ParamMetadata {
                name: "ripple".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(50.0, Some("mV".to_string()))),
            },
        ]
    }
}

/// Current sensing - precision current measurement
///
/// Used for power monitoring, motor control, battery management, and overcurrent
/// protection. Provides accurate current measurement with specified accuracy and range.
///
/// # Parameters
/// - `max_current`: Required. Maximum measurable current in A/mA
/// - `accuracy`: Optional. Measurement accuracy in % (default: 1%)
/// - `sense_resistor`: Optional. Sense resistor value in Ω/mΩ
/// - `bandwidth`: Optional. Measurement bandwidth in Hz/kHz (default: 1kHz)
///
/// # Examples
/// ```bhdl
/// net current_monitor: @load -> sense_r: Res(0.1).1 -> sense_r.2 -> @gnd
///     for current_sensing(max_current: 5A, accuracy: 0.5%);
/// ```
pub struct CurrentSensingIntent;

impl IntentFunction for CurrentSensingIntent {
    fn name(&self) -> &str {
        "current_sensing"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract max_current parameter (required)
        let max_current = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max_current" => {
                        let amps = match unit.as_deref() {
                            Some("A") => *val,
                            Some("mA") => val / 1000.0,
                            _ => *val,
                        };
                        Some(amps)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let amps = match unit.as_deref() {
                            Some("A") => *val,
                            Some("mA") => val / 1000.0,
                            _ => *val,
                        };
                        Some(amps)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "current_sensing requires 'max_current' parameter".to_string())?;

        if max_current <= 0.0 {
            return Err("Maximum current must be positive".to_string());
        }

        // Extract accuracy
        let accuracy_percent = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "accuracy" => {
                        let percent = match unit.as_deref() {
                            Some("%") => *val,
                            _ => *val,
                        };
                        Some(percent)
                    }
                    _ => None
                }
            })
            .unwrap_or(1.0); // Default 1%

        // Extract sense_resistor
        let sense_resistor = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "sense_resistor" => {
                        let ohms = match unit.as_deref() {
                            Some("Ω") | Some("ohm") => *val,
                            Some("mΩ") | Some("mohm") => val / 1000.0,
                            _ => *val,
                        };
                        Some(ohms)
                    }
                    _ => None
                }
            });

        // Extract bandwidth
        let bandwidth_hz = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "bandwidth" => {
                        let hz = match unit.as_deref() {
                            Some("Hz") => *val,
                            Some("kHz") => val * 1000.0,
                            Some("MHz") => val * 1_000_000.0,
                            _ => *val,
                        };
                        Some(hz)
                    }
                    _ => None
                }
            })
            .unwrap_or(1000.0); // Default 1kHz

        // High accuracy or high current requires analog analysis
        let sim_mode = if accuracy_percent < 0.5 || max_current > 10.0 {
            SimMode::AnalogRequired
        } else {
            SimMode::MixedSignal
        };

        let mut hints = vec![
            SynthesisHint::Custom(format!(
                "Current sensing: 0-{:.2}A with {:.1}% accuracy",
                max_current, accuracy_percent
            )),
        ];

        if let Some(r_sense) = sense_resistor {
            hints.push(SynthesisHint::Custom(format!(
                "Sense resistor: {:.3}Ω ({:.2}mV drop at max current)",
                r_sense, r_sense * max_current * 1000.0
            )));
        } else {
            // Calculate recommended sense resistor (aim for ~50mV drop)
            let recommended_r = 0.05 / max_current;
            hints.push(SynthesisHint::Custom(format!(
                "Recommended sense resistor: {:.3}Ω (50mV drop at max current)",
                recommended_r
            )));
        }

        hints.push(SynthesisHint::Custom("Consider current sense amplifier IC (e.g., INA219, MAX4372)".to_string()));

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("current_accuracy_{:.1}percent", accuracy_percent),
                    error_message: format!(
                        "Current measurement accuracy must be within ±{:.1}%",
                        accuracy_percent
                    ),
                },
                ValidationRule {
                    condition: format!("current_range_0_to_{:.2}A", max_current),
                    error_message: format!(
                        "Current measurement range must support 0-{:.2}A",
                        max_current
                    ),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "max_current".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "accuracy".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(1.0, Some("%".to_string()))),
            },
            ParamMetadata {
                name: "sense_resistor".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "bandwidth".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(1000.0, Some("Hz".to_string()))),
            },
        ]
    }
}

/// Communication interface - serial/parallel communication protocol support
///
/// Used for UART, SPI, I2C, CAN, and other communication interfaces. Ensures
/// proper signal levels, timing, and bus loading.
///
/// # Parameters
/// - `protocol`: Required. Communication protocol (uart, spi, i2c, can, rs232, rs485)
/// - `speed`: Required. Communication speed (baud rate or bus speed)
/// - `voltage`: Optional. Signal voltage level in V (default: 3.3V)
/// - `bus_loading`: Optional. Maximum capacitive load in pF (default: 400pF for I2C)
///
/// # Examples
/// ```bhdl
/// net i2c_bus: @master -> pullup: Res(4.7k).1 -> slave.SDA
///     for communication_interface(protocol: "i2c", speed: 400kHz, voltage: 3.3V);
/// ```
pub struct CommunicationInterfaceIntent;

impl IntentFunction for CommunicationInterfaceIntent {
    fn name(&self) -> &str {
        "communication_interface"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract protocol parameter (required)
        let protocol = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "protocol" => Some(s.clone()),
                    IntentParam::Positional(IntentValue::String(s)) => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "communication_interface requires 'protocol' parameter (uart, spi, i2c, can, rs232, rs485)".to_string())?;

        // Validate protocol
        let valid_protocols = ["uart", "spi", "i2c", "can", "rs232", "rs485", "usb", "ethernet"];
        if !valid_protocols.contains(&protocol.to_lowercase().as_str()) {
            return Err(format!(
                "Invalid protocol '{}'. Valid options: {}",
                protocol,
                valid_protocols.join(", ")
            ));
        }

        // Extract speed parameter (required)
        let speed_hz = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "speed" => {
                        let hz = match unit.as_deref() {
                            Some("Hz") | Some("baud") => *val,
                            Some("kHz") | Some("kbaud") => val * 1000.0,
                            Some("MHz") | Some("Mbaud") => val * 1_000_000.0,
                            Some("Gbaud") => val * 1_000_000_000.0,
                            _ => *val,
                        };
                        Some(hz)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "communication_interface requires 'speed' parameter".to_string())?;

        if speed_hz <= 0.0 {
            return Err("Communication speed must be positive".to_string());
        }

        // Extract voltage
        let voltage = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "voltage" => {
                        let volts = match unit.as_deref() {
                            Some("V") => *val,
                            Some("mV") => val / 1000.0,
                            _ => *val,
                        };
                        Some(volts)
                    }
                    _ => None
                }
            })
            .unwrap_or(3.3); // Default 3.3V

        // High-speed interfaces require timing analysis
        let sim_mode = if speed_hz >= 1_000_000.0 {
            SimMode::DigitalWithTiming
        } else {
            SimMode::PureDigital
        };

        let protocol_lower = protocol.to_lowercase();
        let mut hints = vec![
            SynthesisHint::Custom(format!(
                "{} interface: {:.0} baud at {:.1}V",
                protocol.to_uppercase(), speed_hz, voltage
            )),
        ];

        // Protocol-specific recommendations
        match protocol_lower.as_str() {
            "i2c" => {
                hints.push(SynthesisHint::Custom("Use 4.7kΩ pull-up resistors for standard mode".to_string()));
                hints.push(SynthesisHint::Custom("Use 2.2kΩ pull-up resistors for fast mode".to_string()));
            }
            "spi" => {
                hints.push(SynthesisHint::Custom("Keep traces short for high-speed SPI".to_string()));
                hints.push(SynthesisHint::Custom("Consider series resistors on MOSI/MISO for signal integrity".to_string()));
            }
            "can" => {
                hints.push(SynthesisHint::Custom("Use 120Ω termination resistors at both ends".to_string()));
                hints.push(SynthesisHint::Custom("Twisted pair cable recommended for EMI immunity".to_string()));
            }
            "rs485" => {
                hints.push(SynthesisHint::Custom("Use 120Ω termination resistors at both ends".to_string()));
                hints.push(SynthesisHint::Custom("Maintain differential impedance of 120Ω".to_string()));
            }
            _ => {}
        }

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("{}_{:.0}baud", protocol_lower, speed_hz),
                    error_message: format!(
                        "{} interface must support {:.0} baud",
                        protocol.to_uppercase(), speed_hz
                    ),
                },
                ValidationRule {
                    condition: format!("signal_level_{:.1}V", voltage),
                    error_message: format!(
                        "Signal levels must be compatible with {:.1}V logic",
                        voltage
                    ),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "protocol".to_string(),
                param_type: ParamType::String,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "speed".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "voltage".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(3.3, Some("V".to_string()))),
            },
            ParamMetadata {
                name: "bus_loading".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
        ]
    }
}

/// Watchdog monitoring - system health monitoring and recovery
///
/// Used for embedded systems requiring automatic recovery from software failures.
/// Ensures system remains operational through watchdog timer monitoring.
///
/// # Parameters
/// - `timeout`: Required. Watchdog timeout period in ms/s
/// - `reset_type`: Optional. Reset type on timeout (hard, soft) default: hard
/// - `window`: Optional. Window watchdog mode with minimum refresh time
///
/// # Examples
/// ```bhdl
/// net watchdog: @mcu_wdt_out -> supervisor: WatchdogIC().WDI -> supervisor.RST -> @system_reset
///     for watchdog_monitoring(timeout: 1s, reset_type: "hard");
/// ```
pub struct WatchdogMonitoringIntent;

impl IntentFunction for WatchdogMonitoringIntent {
    fn name(&self) -> &str {
        "watchdog_monitoring"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract timeout parameter (required)
        let timeout_ms = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "timeout" => {
                        let milliseconds = match unit.as_deref() {
                            Some("ms") => *val,
                            Some("s") => val * 1000.0,
                            Some("us") | Some("µs") => val / 1000.0,
                            _ => *val,
                        };
                        Some(milliseconds)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let milliseconds = match unit.as_deref() {
                            Some("ms") => *val,
                            Some("s") => val * 1000.0,
                            Some("us") | Some("µs") => val / 1000.0,
                            _ => *val,
                        };
                        Some(milliseconds)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "watchdog_monitoring requires 'timeout' parameter".to_string())?;

        if timeout_ms <= 0.0 {
            return Err("Watchdog timeout must be positive".to_string());
        }

        // Extract reset_type
        let reset_type = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "reset_type" => Some(s.clone()),
                    _ => None
                }
            })
            .unwrap_or_else(|| "hard".to_string());

        // Validate reset type
        if reset_type != "hard" && reset_type != "soft" {
            return Err(format!("reset_type must be 'hard' or 'soft', got '{}'", reset_type));
        }

        // Extract window mode
        let window_mode = params.iter()
            .any(|p| matches!(p, IntentParam::Named(name, _) if name == "window"));

        // Fast watchdog timeouts need timing analysis
        let sim_mode = if timeout_ms < 100.0 {
            SimMode::DigitalWithTiming
        } else {
            SimMode::MixedSignal
        };

        let mut hints = vec![
            SynthesisHint::Custom(format!(
                "Watchdog timeout: {:.0}ms with {} reset",
                timeout_ms, reset_type
            )),
        ];

        if window_mode {
            hints.push(SynthesisHint::Custom("Window watchdog mode enabled".to_string()));
        }

        hints.push(SynthesisHint::Custom("Consider external watchdog IC for critical systems".to_string()));

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("watchdog_timeout_{:.0}ms", timeout_ms),
                    error_message: format!(
                        "Watchdog must be refreshed within {:.0}ms",
                        timeout_ms
                    ),
                },
                ValidationRule {
                    condition: "watchdog_coverage".to_string(),
                    error_message: "Watchdog must monitor all critical system functions".to_string(),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "timeout".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "reset_type".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: Some(IntentValue::String("hard".to_string())),
            },
            ParamMetadata {
                name: "window".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                default_value: Some(IntentValue::Boolean(false)),
            },
        ]
    }
}

/// Power optimization - low-power design optimization
///
/// Used for battery-powered devices and energy-efficient designs. Provides
/// guidance for minimizing power consumption in various operating modes.
///
/// # Parameters
/// - `target_power`: Required. Target average power in W/mW/µW
/// - `sleep_current`: Optional. Maximum sleep mode current in µA/mA
/// - `active_duty_cycle`: Optional. Active duty cycle in % (default: 10%)
/// - `voltage_scaling`: Optional. Enable dynamic voltage scaling (boolean)
///
/// # Examples
/// ```bhdl
/// net battery_powered: @battery -> regulator -> @system_vdd
///     for power_optimization(target_power: 100µW, sleep_current: 10µA, active_duty_cycle: 5%);
/// ```
pub struct PowerOptimizationIntent;

impl IntentFunction for PowerOptimizationIntent {
    fn name(&self) -> &str {
        "power_optimization"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract target_power parameter (required)
        let target_power_w = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "target_power" => {
                        let watts = match unit.as_deref() {
                            Some("W") => *val,
                            Some("mW") => val / 1000.0,
                            Some("µW") | Some("uW") => val / 1_000_000.0,
                            _ => *val,
                        };
                        Some(watts)
                    }
                    IntentParam::Positional(IntentValue::Number(val, unit)) => {
                        let watts = match unit.as_deref() {
                            Some("W") => *val,
                            Some("mW") => val / 1000.0,
                            Some("µW") | Some("uW") => val / 1_000_000.0,
                            _ => *val,
                        };
                        Some(watts)
                    }
                    _ => None
                }
            })
            .ok_or_else(|| "power_optimization requires 'target_power' parameter".to_string())?;

        if target_power_w <= 0.0 {
            return Err("Target power must be positive".to_string());
        }

        // Extract sleep_current
        let sleep_current_a = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "sleep_current" => {
                        let amps = match unit.as_deref() {
                            Some("A") => *val,
                            Some("mA") => val / 1000.0,
                            Some("µA") | Some("uA") => val / 1_000_000.0,
                            _ => *val,
                        };
                        Some(amps)
                    }
                    _ => None
                }
            });

        // Extract active_duty_cycle
        let active_duty_cycle = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "active_duty_cycle" => {
                        let percent = match unit.as_deref() {
                            Some("%") => *val,
                            _ => *val,
                        };
                        Some(percent)
                    }
                    _ => None
                }
            })
            .unwrap_or(10.0); // Default 10%

        // Check voltage scaling
        let voltage_scaling = params.iter()
            .any(|p| matches!(p, IntentParam::Named(name, IntentValue::Boolean(true)) if name == "voltage_scaling"));

        // Ultra-low power requires detailed power analysis
        let sim_mode = if target_power_w < 0.001 {
            SimMode::AnalogRequired
        } else {
            SimMode::MixedSignal
        };

        let mut hints = vec![
            SynthesisHint::Custom(format!(
                "Target average power: {:.0}µW",
                target_power_w * 1_000_000.0
            )),
            SynthesisHint::Custom(format!(
                "Active duty cycle: {:.1}%",
                active_duty_cycle
            )),
        ];

        if let Some(i_sleep) = sleep_current_a {
            hints.push(SynthesisHint::Custom(format!(
                "Maximum sleep current: {:.0}µA",
                i_sleep * 1_000_000.0
            )));
        }

        if voltage_scaling {
            hints.push(SynthesisHint::Custom("Dynamic voltage scaling enabled".to_string()));
        }

        hints.push(SynthesisHint::Custom("Consider low-quiescent current regulators".to_string()));
        hints.push(SynthesisHint::Custom("Minimize pull-up/pull-down resistor power".to_string()));

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("average_power_max_{:.0}uW", target_power_w * 1_000_000.0),
                    error_message: format!(
                        "Average power must not exceed {:.0}µW",
                        target_power_w * 1_000_000.0
                    ),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "target_power".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "sleep_current".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "active_duty_cycle".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(10.0, Some("%".to_string()))),
            },
            ParamMetadata {
                name: "voltage_scaling".to_string(),
                param_type: ParamType::Boolean,
                required: false,
                default_value: Some(IntentValue::Boolean(false)),
            },
        ]
    }
}

/// Test point - debugging and measurement access point
///
/// Used for providing test/debug access to internal signals. Ensures proper
/// loading and accessibility without affecting normal circuit operation.
///
/// # Parameters
/// - `purpose`: Required. Test point purpose (debug, calibration, production_test, field_service)
/// - `max_loading`: Optional. Maximum capacitive loading in pF (default: 10pF)
/// - `access`: Optional. Access method (probe, connector, via) default: probe
///
/// # Examples
/// ```bhdl
/// net debug_signal: @internal_node -> tp: TestPoint().SIG
///     for test_point(purpose: "debug", max_loading: 5pF);
/// ```
pub struct TestPointIntent;

impl IntentFunction for TestPointIntent {
    fn name(&self) -> &str {
        "test_point"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract purpose parameter (required)
        let purpose = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "purpose" => Some(s.clone()),
                    IntentParam::Positional(IntentValue::String(s)) => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "test_point requires 'purpose' parameter (debug, calibration, production_test, field_service)".to_string())?;

        // Validate purpose
        let valid_purposes = ["debug", "calibration", "production_test", "field_service"];
        if !valid_purposes.contains(&purpose.as_str()) {
            return Err(format!(
                "Invalid purpose '{}'. Valid options: {}",
                purpose,
                valid_purposes.join(", ")
            ));
        }

        // Extract max_loading
        let max_loading_pf = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "max_loading" => {
                        let picofarads = match unit.as_deref() {
                            Some("pF") => *val,
                            Some("nF") => val * 1000.0,
                            _ => *val,
                        };
                        Some(picofarads)
                    }
                    _ => None
                }
            })
            .unwrap_or(10.0); // Default 10pF

        // Extract access method
        let access = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "access" => Some(s.clone()),
                    _ => None
                }
            })
            .unwrap_or_else(|| "probe".to_string());

        // Test points generally don't affect simulation (passive observation)
        let sim_mode = SimMode::PureDigital;

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: vec![
                SynthesisHint::Custom(format!(
                    "Test point for {}: max loading {:.1}pF",
                    purpose, max_loading_pf
                )),
                SynthesisHint::Custom(format!("Access method: {}", access)),
                SynthesisHint::Custom("Place test point for easy probe access".to_string()),
            ],
            validation_rules: vec![
                ValidationRule {
                    condition: format!("testpoint_loading_max_{:.1}pF", max_loading_pf),
                    error_message: format!(
                        "Test point loading must not exceed {:.1}pF",
                        max_loading_pf
                    ),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "purpose".to_string(),
                param_type: ParamType::String,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "max_loading".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: Some(IntentValue::Number(10.0, Some("pF".to_string()))),
            },
            ParamMetadata {
                name: "access".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: Some(IntentValue::String("probe".to_string())),
            },
        ]
    }
}

/// Redundancy - fault-tolerant design with redundancy
///
/// Used for mission-critical applications requiring continued operation after
/// component failure. Supports various redundancy schemes (active, standby, TMR).
///
/// # Parameters
/// - `scheme`: Required. Redundancy scheme (active, standby, tmr, nmr)
/// - `fault_tolerance`: Required. Number of tolerable failures
/// - `switchover_time`: Optional. Maximum switchover time in ms (for standby)
/// - `voting`: Optional. Voting logic (majority, unanimous) for TMR/NMR
///
/// # Examples
/// ```bhdl
/// net redundant_power: @primary -> switcher: PowerMux().A -> switcher.OUT -> @vout
///                       @backup  -> switcher.B
///     for redundancy(scheme: "standby", fault_tolerance: 1, switchover_time: 10ms);
/// ```
pub struct RedundancyIntent;

impl IntentFunction for RedundancyIntent {
    fn name(&self) -> &str {
        "redundancy"
    }

    fn resolve(&self, params: &[IntentParam]) -> Result<IntentResult, String> {
        // Extract scheme parameter (required)
        let scheme = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "scheme" => Some(s.clone()),
                    IntentParam::Positional(IntentValue::String(s)) => Some(s.clone()),
                    _ => None
                }
            })
            .ok_or_else(|| "redundancy requires 'scheme' parameter (active, standby, tmr, nmr)".to_string())?;

        // Validate scheme
        let valid_schemes = ["active", "standby", "tmr", "nmr"];
        if !valid_schemes.contains(&scheme.as_str()) {
            return Err(format!(
                "Invalid scheme '{}'. Valid options: {}",
                scheme,
                valid_schemes.join(", ")
            ));
        }

        // Extract fault_tolerance parameter (required)
        let fault_tolerance = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, _)) if name == "fault_tolerance" => Some(*val as i32),
                    _ => None
                }
            })
            .ok_or_else(|| "redundancy requires 'fault_tolerance' parameter".to_string())?;

        if fault_tolerance < 1 {
            return Err("fault_tolerance must be at least 1".to_string());
        }

        // Extract switchover_time
        let switchover_time_ms = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::Number(val, unit)) if name == "switchover_time" => {
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
            });

        // Extract voting logic
        let voting = params.iter()
            .find_map(|p| {
                match p {
                    IntentParam::Named(name, IntentValue::String(s)) if name == "voting" => Some(s.clone()),
                    _ => None
                }
            })
            .unwrap_or_else(|| "majority".to_string());

        // Fast switchover requires timing analysis
        let sim_mode = if let Some(switchover) = switchover_time_ms {
            if switchover < 1.0 {
                SimMode::DigitalWithTiming
            } else {
                SimMode::MixedSignal
            }
        } else {
            SimMode::MixedSignal
        };

        let mut hints = vec![
            SynthesisHint::Custom(format!(
                "{} redundancy: {} fault tolerance",
                scheme.to_uppercase(), fault_tolerance
            )),
        ];

        match scheme.as_str() {
            "active" => {
                hints.push(SynthesisHint::Custom("All redundant channels active simultaneously".to_string()));
            }
            "standby" => {
                if let Some(switchover) = switchover_time_ms {
                    hints.push(SynthesisHint::Custom(format!(
                        "Standby switchover time: {:.1}ms",
                        switchover
                    )));
                }
                hints.push(SynthesisHint::Custom("Consider load switch or multiplexer for switchover".to_string()));
            }
            "tmr" | "nmr" => {
                hints.push(SynthesisHint::Custom(format!("Voting logic: {}", voting)));
                hints.push(SynthesisHint::Custom("Implement voter circuit for fault masking".to_string()));
            }
            _ => {}
        }

        Ok(IntentResult {
            sim_mode,
            synthesis_hints: hints,
            validation_rules: vec![
                ValidationRule {
                    condition: format!("redundancy_{}_fault_tolerant_{}", scheme, fault_tolerance),
                    error_message: format!(
                        "System must tolerate {} component failure(s)",
                        fault_tolerance
                    ),
                },
                ValidationRule {
                    condition: "no_single_point_of_failure".to_string(),
                    error_message: "Design must not have single points of failure".to_string(),
                },
            ],
            tool_scope: ToolScope::All,
        })
    }

    fn param_metadata(&self) -> Vec<ParamMetadata> {
        vec![
            ParamMetadata {
                name: "scheme".to_string(),
                param_type: ParamType::String,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "fault_tolerance".to_string(),
                param_type: ParamType::Number,
                required: true,
                default_value: None,
            },
            ParamMetadata {
                name: "switchover_time".to_string(),
                param_type: ParamType::Number,
                required: false,
                default_value: None,
            },
            ParamMetadata {
                name: "voting".to_string(),
                param_type: ParamType::String,
                required: false,
                default_value: Some(IntentValue::String("majority".to_string())),
            },
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_voltage_regulation_basic() {
        let intent = VoltageRegulationIntent;
        let params = vec![
            IntentParam::Named("output_voltage".to_string(), IntentValue::Number(3.3, Some("V".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.synthesis_hints.len() >= 4);
    }

    #[test]
    fn test_voltage_regulation_tight_specs() {
        let intent = VoltageRegulationIntent;
        let params = vec![
            IntentParam::Named("output_voltage".to_string(), IntentValue::Number(5.0, Some("V".to_string()))),
            IntentParam::Named("load_regulation".to_string(), IntentValue::Number(0.1, Some("%".to_string()))),
            IntentParam::Named("ripple".to_string(), IntentValue::Number(5.0, Some("mV".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::AnalogRequired); // Tight specs
    }

    #[test]
    fn test_current_sensing_basic() {
        let intent = CurrentSensingIntent;
        let params = vec![
            IntentParam::Named("max_current".to_string(), IntentValue::Number(5.0, Some("A".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert!(result.synthesis_hints.len() >= 2);
    }

    #[test]
    fn test_current_sensing_high_accuracy() {
        let intent = CurrentSensingIntent;
        let params = vec![
            IntentParam::Named("max_current".to_string(), IntentValue::Number(1.0, Some("A".to_string()))),
            IntentParam::Named("accuracy".to_string(), IntentValue::Number(0.1, Some("%".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::AnalogRequired); // High accuracy
    }

    #[test]
    fn test_communication_interface_i2c() {
        let intent = CommunicationInterfaceIntent;
        let params = vec![
            IntentParam::Named("protocol".to_string(), IntentValue::String("i2c".to_string())),
            IntentParam::Named("speed".to_string(), IntentValue::Number(400.0, Some("kHz".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::PureDigital);
        assert!(result.synthesis_hints.iter().any(|h| {
            if let SynthesisHint::Custom(s) = h {
                s.contains("pull-up")
            } else {
                false
            }
        }));
    }

    #[test]
    fn test_communication_interface_high_speed() {
        let intent = CommunicationInterfaceIntent;
        let params = vec![
            IntentParam::Named("protocol".to_string(), IntentValue::String("spi".to_string())),
            IntentParam::Named("speed".to_string(), IntentValue::Number(10.0, Some("MHz".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming); // High speed
    }

    #[test]
    fn test_watchdog_monitoring_basic() {
        let intent = WatchdogMonitoringIntent;
        let params = vec![
            IntentParam::Named("timeout".to_string(), IntentValue::Number(1000.0, Some("ms".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal);
    }

    #[test]
    fn test_watchdog_monitoring_fast() {
        let intent = WatchdogMonitoringIntent;
        let params = vec![
            IntentParam::Named("timeout".to_string(), IntentValue::Number(50.0, Some("ms".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::DigitalWithTiming); // Fast timeout
    }

    #[test]
    fn test_power_optimization_basic() {
        let intent = PowerOptimizationIntent;
        let params = vec![
            IntentParam::Named("target_power".to_string(), IntentValue::Number(100.0, Some("µW".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::AnalogRequired); // Ultra low power
    }

    #[test]
    fn test_test_point_basic() {
        let intent = TestPointIntent;
        let params = vec![
            IntentParam::Named("purpose".to_string(), IntentValue::String("debug".to_string())),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::PureDigital);
    }

    #[test]
    fn test_redundancy_standby() {
        let intent = RedundancyIntent;
        let params = vec![
            IntentParam::Named("scheme".to_string(), IntentValue::String("standby".to_string())),
            IntentParam::Named("fault_tolerance".to_string(), IntentValue::Number(1.0, None)),
            IntentParam::Named("switchover_time".to_string(), IntentValue::Number(10.0, Some("ms".to_string()))),
        ];

        let result = intent.resolve(&params).unwrap();
        assert_eq!(result.sim_mode, SimMode::MixedSignal);
        assert_eq!(result.validation_rules.len(), 2);
    }

    #[test]
    fn test_redundancy_tmr() {
        let intent = RedundancyIntent;
        let params = vec![
            IntentParam::Named("scheme".to_string(), IntentValue::String("tmr".to_string())),
            IntentParam::Named("fault_tolerance".to_string(), IntentValue::Number(1.0, None)),
        ];

        let result = intent.resolve(&params).unwrap();
        assert!(result.synthesis_hints.iter().any(|h| {
            if let SynthesisHint::Custom(s) = h {
                s.contains("voter")
            } else {
                false
            }
        }));
    }
}
