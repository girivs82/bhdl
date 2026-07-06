//! Comprehensive tests for all stdlib intent functions
//!
//! Tests each intent function individually with valid and invalid parameters,
//! verifies correct SimMode selection, synthesis hints, and validation rules.

use bhdl_common::{
    IntentRegistry, IntentFunction, IntentParam, IntentValue, IntentResult,
    SimMode, SynthesisHint, ToolScope,
};
use bhdl_stdlib::intents;

/// Helper to create intent registry with all stdlib intents
fn create_registry() -> IntentRegistry {
    let mut registry = IntentRegistry::new();
    intents::register_stdlib_intents(&mut registry);
    registry
}

// ============================================================================
// TIMING INTENTS (3)
// ============================================================================

#[test]
fn test_delay_intent_milliseconds() {
    let registry = create_registry();
    let intent = registry.get("delay").expect("delay intent should be registered");

    let params = vec![
        IntentParam::Positional(IntentValue::Number(20.0, Some("ms".to_string())))
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    // 20ms should require analog simulation (> 1e-3)
    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.contains(&SynthesisHint::ActiveDelay));
    assert!(!result.validation_rules.is_empty());
}

#[test]
fn test_delay_intent_microseconds() {
    let registry = create_registry();
    let intent = registry.get("delay").expect("delay intent should be registered");

    let params = vec![
        IntentParam::Positional(IntentValue::Number(5.0, Some("us".to_string())))
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    // 5µs should use mixed signal (between 1e-6 and 1e-3)
    assert_eq!(result.sim_mode, SimMode::MixedSignal);
    assert!(result.synthesis_hints.contains(&SynthesisHint::RCNetwork));
}

#[test]
fn test_delay_intent_nanoseconds() {
    let registry = create_registry();
    let intent = registry.get("delay").expect("delay intent should be registered");

    let params = vec![
        IntentParam::Positional(IntentValue::Number(50.0, Some("ns".to_string())))
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    // 50ns should use buffer chain (< 100ns)
    assert_eq!(result.sim_mode, SimMode::DigitalWithTiming);
    assert!(result.synthesis_hints.contains(&SynthesisHint::BufferChain));
}

#[test]
fn test_delay_intent_missing_parameter() {
    let registry = create_registry();
    let intent = registry.get("delay").expect("delay intent should be registered");

    let params = vec![];
    let result = intent.resolve(&params);

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("requires a time parameter"));
}

#[test]
fn test_debounce_intent_default_time() {
    let registry = create_registry();
    let intent = registry.get("debounce").expect("debounce intent should be registered");

    // Debounce can work with just source (uses default 20ms)
    let params = vec![
        IntentParam::Positional(IntentValue::Identifier("button".to_string()))
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.contains(&SynthesisHint::RCNetwork));
    assert!(result.validation_rules.iter().any(|r| r.condition == "has_rc_network"));
}

#[test]
fn test_debounce_intent_custom_time() {
    let registry = create_registry();
    let intent = registry.get("debounce").expect("debounce intent should be registered");

    let params = vec![
        IntentParam::Positional(IntentValue::Identifier("button".to_string())),
        IntentParam::Named("time".to_string(), IntentValue::Number(50.0, Some("ms".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.validation_rules.iter().any(|r| r.error_message.contains("50ms")));
}

#[test]
fn test_pulse_stretch_intent() {
    let registry = create_registry();
    let intent = registry.get("pulse_stretch").expect("pulse_stretch intent should be registered");

    let params = vec![
        IntentParam::Named("duration".to_string(), IntentValue::Number(100.0, Some("us".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::MixedSignal);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("Pulse stretcher"))
    }));
    assert!(result.synthesis_hints.contains(&SynthesisHint::RCNetwork));
}

// ============================================================================
// PROTECTION INTENTS (2)
// ============================================================================

#[test]
fn test_input_protection_intent() {
    let registry = create_registry();
    let intent = registry.get("input_protection").expect("input_protection intent should be registered");

    let params = vec![
        IntentParam::Named("overvoltage".to_string(), IntentValue::Number(6.0, Some("V".to_string()))),
        IntentParam::Named("current_limit".to_string(), IntentValue::Number(5.0, Some("mA".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("TVS diode"))
    }));
    assert!(result.validation_rules.iter().any(|r| r.condition.contains("has_voltage_clamp") || r.condition.contains("has_current_limiting")));
}

#[test]
fn test_overvoltage_protection_intent() {
    let registry = create_registry();
    let intent = registry.get("overvoltage_protection").expect("overvoltage_protection intent should be registered");

    let params = vec![
        IntentParam::Positional(IntentValue::Number(15.0, Some("V".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("TVS diode"))
    }));
}

// ============================================================================
// SIGNAL PROCESSING INTENTS (3)
// ============================================================================

#[test]
fn test_anti_alias_intent() {
    let registry = create_registry();
    let intent = registry.get("anti_alias").expect("anti_alias intent should be registered");

    let params = vec![
        IntentParam::Named("before".to_string(), IntentValue::Identifier("adc".to_string())),
        IntentParam::Named("cutoff".to_string(), IntentValue::Number(10.0, Some("kHz".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.contains(&SynthesisHint::AnalogFilter));
    assert!(result.validation_rules.iter().any(|r| r.condition.contains("cutoff_below_nyquist")));
}

#[test]
fn test_low_noise_intent() {
    let registry = create_registry();
    let intent = registry.get("low_noise").expect("low_noise intent should be registered");

    let params = vec![
        IntentParam::Named("max_ripple".to_string(), IntentValue::Number(10.0, Some("mV".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("low-noise"))
    }));
    assert!(result.validation_rules.iter().any(|r| r.error_message.contains("10mV")));
}

#[test]
fn test_noise_filtering_intent() {
    let registry = create_registry();
    let intent = registry.get("noise_filtering").expect("noise_filtering intent should be registered");

    let params = vec![
        IntentParam::Positional(IntentValue::Number(1.0, Some("kHz".to_string()))),
        IntentParam::Named("attenuation".to_string(), IntentValue::Number(40.0, Some("dB".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.contains(&SynthesisHint::AnalogFilter));
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("1kHz") && s.contains("cutoff"))
    }));
}

#[test]
fn test_noise_filtering_intent_missing_cutoff() {
    let registry = create_registry();
    let intent = registry.get("noise_filtering").expect("noise_filtering intent should be registered");

    let params = vec![]; // Missing cutoff parameter
    let result = intent.resolve(&params);

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("requires cutoff frequency"));
}

// ============================================================================
// ANALOG INTENTS (4)
// ============================================================================

#[test]
fn test_current_limiting_intent() {
    let registry = create_registry();
    let intent = registry.get("current_limiting").expect("current_limiting intent should be registered");

    let params = vec![
        IntentParam::Named("max".to_string(), IntentValue::Number(20.0, Some("mA".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("current limiting resistor"))
    }));
    assert!(result.validation_rules.iter().any(|r| r.condition.contains("current_within_limit")));
}

#[test]
fn test_level_shifting_intent() {
    let registry = create_registry();
    let intent = registry.get("level_shifting").expect("level_shifting intent should be registered");

    let params = vec![
        IntentParam::Named("from".to_string(), IntentValue::Number(3.3, Some("V".to_string()))),
        IntentParam::Named("to".to_string(), IntentValue::Number(5.0, Some("V".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::MixedSignal);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("3.3V") && s.contains("5V"))
    }));
}

#[test]
fn test_voltage_division_intent() {
    let registry = create_registry();
    let intent = registry.get("voltage_division").expect("voltage_division intent should be registered");

    let params = vec![
        IntentParam::Named("ratio".to_string(), IntentValue::Number(0.5, Some("".to_string()))),
        IntentParam::Named("impedance".to_string(), IntentValue::Number(10.0, Some("k".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("0.5") && s.contains("ratio"))
    }));
}

#[test]
fn test_signal_amplification_intent() {
    let registry = create_registry();
    let intent = registry.get("signal_amplification").expect("signal_amplification intent should be registered");

    let params = vec![
        IntentParam::Positional(IntentValue::Number(10.0, Some("".to_string()))), // 10x gain
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("10") && s.contains("gain"))
    }));
}

// ============================================================================
// DIGITAL INTENTS (1)
// ============================================================================

#[test]
fn test_signal_buffering_intent() {
    let registry = create_registry();
    let intent = registry.get("signal_buffering").expect("signal_buffering intent should be registered");

    let params = vec![
        IntentParam::Named("fanout".to_string(), IntentValue::Number(8.0, Some("".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    // Digital buffering with fanout 8 should use MixedSignal (> 5)
    assert_eq!(result.sim_mode, SimMode::MixedSignal);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("buffer") && s.contains("fanout"))
    }));
}

// ============================================================================
// MEASUREMENT INTENTS (2)
// ============================================================================

#[test]
fn test_precision_measurement_intent() {
    let registry = create_registry();
    let intent = registry.get("precision_measurement").expect("precision_measurement intent should be registered");

    let params = vec![
        IntentParam::Named("bandwidth".to_string(), IntentValue::Number(1000.0, Some("Hz".to_string()))),
        IntentParam::Named("noise_floor".to_string(), IntentValue::Number(-90.0, Some("dB".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("precision") || s.contains("ADC"))
    }));
}

#[test]
fn test_control_loop_intent() {
    let registry = create_registry();
    let intent = registry.get("control_loop").expect("control_loop intent should be registered");

    let params = vec![
        IntentParam::Named("bandwidth".to_string(), IntentValue::Number(100.0, Some("Hz".to_string()))),
        IntentParam::Named("stability_margin".to_string(), IntentValue::Number(45.0, Some("deg".to_string()))),
    ];

    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::AnalogRequired);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("100Hz") && s.contains("bandwidth"))
    }));
}

// ============================================================================
// DEVELOPMENT INTENTS (1)
// ============================================================================

#[test]
fn test_debug_only_intent() {
    let registry = create_registry();
    let intent = registry.get("debug_only").expect("debug_only intent should be registered");

    let params = vec![];
    let result = intent.resolve(&params).expect("Should resolve");

    assert_eq!(result.sim_mode, SimMode::PureDigital);
    assert_eq!(result.tool_scope, ToolScope::SimulationOnly);
    assert!(result.synthesis_hints.iter().any(|h| {
        matches!(h, SynthesisHint::Custom(s) if s.contains("DEBUG ONLY"))
    }));
    assert!(result.validation_rules.iter().any(|r| {
        r.condition.contains("not_in_production_build")
    }));
}

// ============================================================================
// INTENT REGISTRY TESTS
// ============================================================================

#[test]
fn test_all_intents_registered() {
    let registry = create_registry();

    // Verify every stdlib intent is registered
    let expected_intents = vec![
        // Timing (3)
        "delay", "debounce", "pulse_stretch",
        // Protection (2)
        "input_protection", "overvoltage_protection",
        // Signal Processing (3)
        "anti_alias", "low_noise", "noise_filtering",
        // Analog (7)
        "current_limiting", "level_shifting", "voltage_division", "signal_amplification",
        "amplifier", "current_source", "digital_switch",
        // Digital (1)
        "signal_buffering",
        // Measurement (2)
        "precision_measurement", "control_loop",
        // Development (1)
        "debug_only",
        // Safety (4)
        "automotive_safety", "industrial_control", "medical_safety", "esd_protection",
        // Power Management (4)
        "power_sequencing", "voltage_monitoring", "power_good_signal", "inrush_limiting",
        // Digital Timing (3)
        "clock_distribution", "reset_generation", "boot_sequencing",
        // Advanced (4)
        "signal_integrity", "emi_filtering", "isolation", "thermal_management",
        // Core filtering/regulation (bhdl-common) (4)
        "output_filtering", "input_filtering", "regulation", "loading",
        // Specialized (7)
        "voltage_regulation", "current_sensing", "communication_interface",
        "watchdog_monitoring", "power_optimization", "test_point", "redundancy",
    ];

    for intent_name in &expected_intents {
        assert!(registry.get(intent_name).is_some(),
                "Intent '{}' should be registered", intent_name);
    }

    // Verify count against the enumerated list so this test fails loudly —
    // with the name of the change, not a bare number — whenever an intent is
    // added or removed without updating the enumeration above.
    assert_eq!(registry.registered_intents().len(), expected_intents.len(),
               "Registry has intents not in this test's enumeration (or vice versa): {:?}",
               registry.registered_intents());
}

#[test]
fn test_intent_param_metadata() {
    let registry = create_registry();

    // Test that intents expose their parameter metadata
    let delay_intent = registry.get("delay").expect("delay intent should exist");
    let metadata = delay_intent.param_metadata();

    assert_eq!(metadata.len(), 1);
    assert_eq!(metadata[0].name, "time");
    assert!(metadata[0].required);
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[test]
fn test_invalid_unit_handling() {
    let registry = create_registry();
    let intent = registry.get("delay").expect("delay intent should be registered");

    // Invalid unit for delay
    let params = vec![
        IntentParam::Positional(IntentValue::Number(10.0, Some("meters".to_string())))
    ];

    let result = intent.resolve(&params);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Invalid time unit"));
}

#[test]
fn test_wrong_parameter_type() {
    let registry = create_registry();
    let intent = registry.get("delay").expect("delay intent should be registered");

    // String instead of number
    let params = vec![
        IntentParam::Positional(IntentValue::String("invalid".to_string()))
    ];

    let result = intent.resolve(&params);
    assert!(result.is_err());
}

#[test]
fn test_missing_required_parameter() {
    let registry = create_registry();
    let intent = registry.get("current_limiting").expect("current_limiting intent should be registered");

    // Missing max current parameter
    let params = vec![];
    let result = intent.resolve(&params);

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("requires max current parameter"));
}
