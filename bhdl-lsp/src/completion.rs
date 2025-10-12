//! Intent function autocomplete

use tower_lsp::lsp_types::{
    CompletionItem, CompletionItemKind, Documentation, InsertTextFormat,
    MarkupContent, MarkupKind, Position,
};
use bhdl_common::IntentRegistry;

/// Provide intent function completions
pub fn provide_completions(
    text: &str,
    position: Position,
    _registry: &IntentRegistry,
) -> Vec<CompletionItem> {
    // Get the line where cursor is
    let lines: Vec<&str> = text.lines().collect();
    if position.line as usize >= lines.len() {
        return Vec::new();
    }

    let current_line = lines[position.line as usize];

    // Check if we're after "for " keyword
    if !current_line.contains("for ") {
        // Not in intent context, return empty
        return Vec::new();
    }

    // Generate completion items for all 38 intents
    let mut completions = Vec::new();

    // Timing intents
    add_intent_completion(&mut completions, "delay", "Timing",
        "delay(time)", "Add signal delay",
        "delay(5ms) - Adds 5ms delay to signal path");
    add_intent_completion(&mut completions, "debounce", "Timing",
        "debounce(time)", "Debounce digital signal",
        "debounce(50ms) - Removes glitches shorter than 50ms");
    add_intent_completion(&mut completions, "pulse_stretch", "Timing",
        "pulse_stretch(duration)", "Stretch pulse width",
        "pulse_stretch(100µs) - Ensures pulses are at least 100µs wide");
    add_intent_completion(&mut completions, "stable_for", "Timing",
        "stable_for(duration)", "Require signal stability",
        "stable_for(10ms) - Signal must be stable for 10ms");

    // Signal Processing
    add_intent_completion(&mut completions, "noise_filtering", "Signal Processing",
        "noise_filtering(cutoff, attenuation)", "Low-pass filter",
        "noise_filtering(10kHz, 40dB) - Attenuates noise above 10kHz by 40dB");
    add_intent_completion(&mut completions, "anti_alias", "Signal Processing",
        "anti_alias(cutoff)", "Anti-aliasing filter",
        "anti_alias(20kHz) - Prevents aliasing in ADC sampling");
    add_intent_completion(&mut completions, "fast_response", "Signal Processing",
        "fast_response(bandwidth)", "Fast transient response",
        "fast_response(1MHz) - Maintains bandwidth up to 1MHz");

    // Protection
    add_intent_completion(&mut completions, "input_protection", "Protection",
        "input_protection(max_voltage, max_current)", "Protect input from overvoltage/overcurrent",
        "input_protection(15V, 100mA) - Limits input to 15V and 100mA");
    add_intent_completion(&mut completions, "overvoltage_clamp", "Protection",
        "overvoltage_clamp(clamp_voltage)", "Clamp voltage transients",
        "overvoltage_clamp(6V) - Clamps voltage spikes above 6V");
    add_intent_completion(&mut completions, "current_limiting", "Protection",
        "current_limiting(max_current)", "Limit current draw",
        "current_limiting(2A) - Limits current to 2A maximum");

    // Power/Analog
    add_intent_completion(&mut completions, "low_noise", "Power/Analog",
        "low_noise(noise_floor)", "Minimize noise",
        "low_noise(10µV) - Keeps noise below 10µV RMS");
    add_intent_completion(&mut completions, "signal_amplification", "Power/Analog",
        "signal_amplification(gain)", "Amplify signal",
        "signal_amplification(10) - Provides 10x (20dB) gain");
    add_intent_completion(&mut completions, "level_shifting", "Power/Analog",
        "level_shifting(from_voltage, to_voltage)", "Shift voltage levels",
        "level_shifting(3.3V, 5V) - Converts 3.3V logic to 5V logic");

    // Digital
    add_intent_completion(&mut completions, "signal_buffering", "Digital",
        "signal_buffering()", "Buffer digital signal",
        "signal_buffering() - Adds buffer to improve drive strength");
    add_intent_completion(&mut completions, "output_buffering", "Digital",
        "output_buffering(drive_strength)", "Buffer output with drive strength",
        "output_buffering(8mA) - Buffers output with 8mA drive");
    add_intent_completion(&mut completions, "signal_distribution", "Digital",
        "signal_distribution(fanout)", "Distribute signal to multiple loads",
        "signal_distribution(8) - Distributes to 8 loads");

    // Safety
    add_intent_completion(&mut completions, "automotive_safety", "Safety",
        "automotive_safety(level)", "ISO 26262 ASIL compliance",
        "automotive_safety(\"ASIL_D\") - Highest automotive safety level");
    add_intent_completion(&mut completions, "medical_safety", "Safety",
        "medical_safety(class)", "IEC 60601 medical compliance",
        "medical_safety(\"ClassB\") - Class B medical device");
    add_intent_completion(&mut completions, "esd_protection", "Safety",
        "esd_protection(level)", "ESD protection level",
        "esd_protection(8kV) - Protects against 8kV ESD events");

    // Specialized
    add_intent_completion(&mut completions, "voltage_regulation", "Specialized",
        "voltage_regulation(output_voltage, load_regulation, ripple)", "Precise voltage regulation",
        "voltage_regulation(5V, 0.5%, 20mV) - Tight voltage regulation specs");
    add_intent_completion(&mut completions, "current_sensing", "Specialized",
        "current_sensing(max_current, accuracy)", "High-accuracy current measurement",
        "current_sensing(5A, 1%) - Measures up to 5A with 1% accuracy");
    add_intent_completion(&mut completions, "communication_interface", "Specialized",
        "communication_interface(protocol, speed)", "Serial/parallel communication",
        "communication_interface(\"i2c\", 400kHz) - I2C at 400kHz (Fast Mode)");
    add_intent_completion(&mut completions, "watchdog_monitoring", "Specialized",
        "watchdog_monitoring(timeout)", "System health monitoring",
        "watchdog_monitoring(1s) - Resets if not serviced within 1s");
    add_intent_completion(&mut completions, "power_optimization", "Specialized",
        "power_optimization(target_power)", "Low-power design",
        "power_optimization(500mW) - Optimizes for 500mW total power");

    completions
}

fn add_intent_completion(
    completions: &mut Vec<CompletionItem>,
    label: &str,
    category: &str,
    insert_text: &str,
    detail: &str,
    documentation: &str,
) {
    completions.push(CompletionItem {
        label: label.to_string(),
        kind: Some(CompletionItemKind::FUNCTION),
        detail: Some(format!("[{}] {}", category, detail)),
        documentation: Some(Documentation::MarkupContent(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```bhdl\nfor {}\n```\n\n{}", insert_text, documentation),
        })),
        insert_text: Some(insert_text.to_string()),
        insert_text_format: Some(InsertTextFormat::PLAIN_TEXT),
        ..Default::default()
    });
}
