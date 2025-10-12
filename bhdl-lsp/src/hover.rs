//! Hover documentation for intents and components

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind, Position};
use bhdl_common::IntentRegistry;

/// Provide hover documentation
pub fn provide_hover(
    text: &str,
    position: Position,
    registry: &IntentRegistry,
) -> Option<Hover> {
    // Get the word at the cursor position
    let lines: Vec<&str> = text.lines().collect();
    if position.line as usize >= lines.len() {
        return None;
    }

    let line = lines[position.line as usize];
    let word = extract_word_at_position(line, position.character as usize)?;

    // Check if it's an intent function
    if let Some(_intent) = registry.get(&word) {
        let documentation = get_intent_documentation(&word);
        return Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: documentation,
            }),
            range: None,
        });
    }

    // Check for common BHDL keywords
    let keyword_doc = match word.as_str() {
        "board" => Some("**board** - Defines a complete circuit board\n\nExample:\n```bhdl\nboard MyBoard {\n  power VCC = 5V @ 1A;\n  ground GND;\n}\n```"),
        "module" => Some("**module** - Defines a reusable circuit component\n\nExample:\n```bhdl\nmodule Resistor(value: resistance) {\n  pin 1: signal inout;\n  pin 2: signal inout;\n}\n```"),
        "power" => Some("**power** - Declares a power domain\n\nExample:\n```bhdl\npower VCC = 5V @ 1A;\n```"),
        "ground" => Some("**ground** - Declares a ground reference\n\nExample:\n```bhdl\nground GND;\n```"),
        "net" => Some("**net** - Declares a named signal net\n\nExample:\n```bhdl\nnet signal_path: @VCC -> Res(10k).1 -> Cap(100nF).1;\n```"),
        "for" => Some("**for** - Attaches design intent to signal flow\n\nExample:\n```bhdl\nnet filtered: input -> filter for noise_filtering(10kHz, 40dB);\n```\n\nSee intent function completions for available intents."),
        _ => None,
    };

    keyword_doc.map(|doc| Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: doc.to_string(),
        }),
        range: None,
    })
}

fn extract_word_at_position(line: &str, character: usize) -> Option<String> {
    if character >= line.len() {
        return None;
    }

    // Find word boundaries
    let chars: Vec<char> = line.chars().collect();
    let mut start = character;
    let mut end = character;

    // Move start backwards to word boundary
    while start > 0 && (chars[start - 1].is_alphanumeric() || chars[start - 1] == '_') {
        start -= 1;
    }

    // Move end forwards to word boundary
    while end < chars.len() && (chars[end].is_alphanumeric() || chars[end] == '_') {
        end += 1;
    }

    if start < end {
        Some(chars[start..end].iter().collect())
    } else {
        None
    }
}

fn get_intent_documentation(intent_name: &str) -> String {
    match intent_name {
        // Timing
        "delay" => format!(
            "# delay\n\n**Category**: Timing\n\n\
            Adds propagation delay to a signal path.\n\n\
            **Parameters**:\n\
            - `time` - Delay duration (e.g., 5ms, 100ns)\n\n\
            **Example**:\n\
            ```bhdl\nnet delayed: input -> output for delay(5ms);\n```\n\n\
            **SimMode**: AnalogRequired for accurate timing"
        ),
        "debounce" => format!(
            "# debounce\n\n**Category**: Timing\n\n\
            Removes glitches from digital signals (mechanical switch bounce).\n\n\
            **Parameters**:\n\
            - `time` - Minimum stable duration (e.g., 50ms)\n\n\
            **Example**:\n\
            ```bhdl\nnet button: switch_pin -> debounced for debounce(50ms);\n```\n\n\
            **SimMode**: DigitalWithTiming"
        ),

        // Signal Processing
        "noise_filtering" => format!(
            "# noise_filtering\n\n**Category**: Signal Processing\n\n\
            Low-pass filter to attenuate high-frequency noise.\n\n\
            **Parameters**:\n\
            - `cutoff` - Cutoff frequency (e.g., 10kHz)\n\
            - `attenuation` - Attenuation in dB (e.g., 40dB)\n\n\
            **Example**:\n\
            ```bhdl\nnet filtered: sensor -> adc for noise_filtering(10kHz, 40dB);\n```\n\n\
            **SimMode**: AnalogRequired"
        ),
        "anti_alias" => format!(
            "# anti_alias\n\n**Category**: Signal Processing\n\n\
            Anti-aliasing filter for ADC inputs to prevent frequency folding.\n\n\
            **Parameters**:\n\
            - `cutoff` - Filter cutoff (typically < Nyquist frequency)\n\n\
            **Example**:\n\
            ```bhdl\nnet adc_input: signal -> adc.IN for anti_alias(20kHz);\n```\n\n\
            **SimMode**: AnalogRequired"
        ),

        // Protection
        "input_protection" => format!(
            "# input_protection\n\n**Category**: Protection\n\n\
            Protects inputs from overvoltage and overcurrent.\n\n\
            **Parameters**:\n\
            - `max_voltage` - Maximum voltage limit (e.g., 15V)\n\
            - `max_current` - Maximum current limit (e.g., 100mA)\n\n\
            **Example**:\n\
            ```bhdl\nnet protected: external -> tvs -> circuit for input_protection(15V, 100mA);\n```\n\n\
            **SimMode**: AnalogRequired"
        ),
        "current_limiting" => format!(
            "# current_limiting\n\n**Category**: Protection\n\n\
            Limits current to prevent damage.\n\n\
            **Parameters**:\n\
            - `max_current` - Maximum current (e.g., 2A)\n\n\
            **Example**:\n\
            ```bhdl\nnet limited: source -> load for current_limiting(2A);\n```\n\n\
            **SimMode**: AnalogRequired"
        ),

        // Power/Analog
        "low_noise" => format!(
            "# low_noise\n\n**Category**: Power/Analog\n\n\
            Minimizes noise in sensitive analog circuits.\n\n\
            **Parameters**:\n\
            - `noise_floor` - Target noise level (e.g., 10µV)\n\n\
            **Example**:\n\
            ```bhdl\nnet analog: source -> opamp for low_noise(10µV);\n```\n\n\
            **SimMode**: AnalogRequired"
        ),
        "level_shifting" => format!(
            "# level_shifting\n\n**Category**: Power/Analog\n\n\
            Shifts voltage levels between different logic families.\n\n\
            **Parameters**:\n\
            - `from_voltage` - Source voltage level (e.g., 3.3V)\n\
            - `to_voltage` - Target voltage level (e.g., 5V)\n\n\
            **Example**:\n\
            ```bhdl\nnet shifted: mcu_3v3 -> 5v_device for level_shifting(3.3V, 5V);\n```\n\n\
            **SimMode**: MixedSignal"
        ),

        // Specialized
        "voltage_regulation" => format!(
            "# voltage_regulation\n\n**Category**: Specialized\n\n\
            Precise voltage regulation with tight specifications.\n\n\
            **Parameters**:\n\
            - `output_voltage` - Regulated output (e.g., 5V)\n\
            - `load_regulation` - Load regulation spec (e.g., 0.5%)\n\
            - `ripple` - Maximum ripple (e.g., 20mV)\n\n\
            **Example**:\n\
            ```bhdl\nnet regulated: @VIN -> reg -> @VOUT for voltage_regulation(5V, 0.5%, 20mV);\n```\n\n\
            **SimMode**: AnalogRequired for tight specs"
        ),
        "current_sensing" => format!(
            "# current_sensing\n\n**Category**: Specialized\n\n\
            High-accuracy current measurement.\n\n\
            **Parameters**:\n\
            - `max_current` - Maximum current range (e.g., 5A)\n\
            - `accuracy` - Measurement accuracy (e.g., 1%)\n\n\
            **Example**:\n\
            ```bhdl\nnet monitored: source -> sense -> load for current_sensing(5A, 1%);\n```\n\n\
            **SimMode**: AnalogRequired for <0.5% accuracy"
        ),
        "communication_interface" => format!(
            "# communication_interface\n\n**Category**: Specialized\n\n\
            Serial/parallel communication protocol specification.\n\n\
            **Parameters**:\n\
            - `protocol` - Protocol name (\"uart\", \"spi\", \"i2c\", \"can\", etc.)\n\
            - `speed` - Communication speed (e.g., 400kHz)\n\
            - `voltage` - Logic level voltage (optional)\n\n\
            **Example**:\n\
            ```bhdl\nnet i2c_bus: mcu.SDA -> sensor.SDA for communication_interface(\"i2c\", 400kHz);\n```\n\n\
            **SimMode**: DigitalWithTiming for high speeds"
        ),

        // Safety
        "automotive_safety" => format!(
            "# automotive_safety\n\n**Category**: Safety\n\n\
            ISO 26262 Automotive Safety Integrity Level compliance.\n\n\
            **Parameters**:\n\
            - `level` - ASIL level (\"QM\", \"ASIL_A\", \"ASIL_B\", \"ASIL_C\", \"ASIL_D\")\n\n\
            **Example**:\n\
            ```bhdl\nnet safety_critical: sensor -> ecu for automotive_safety(\"ASIL_D\");\n```\n\n\
            **SimMode**: AnalogRequired for ASIL-D"
        ),

        // Default for unrecognized intents
        _ => format!(
            "# {}\n\n**Intent Function**\n\n\
            Part of the BHDL Intent System (38 functions available).\n\n\
            Use autocomplete (Ctrl+Space) to see all available intents.",
            intent_name
        ),
    }
}
