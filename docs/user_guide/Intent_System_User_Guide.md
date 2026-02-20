# BHDL Intent System User Guide

## Overview

The BHDL Intent System allows you to declare the **purpose** of signal flows in your circuit designs using the `for` keyword. This captures your design intent explicitly, enabling intelligent tool automation, simulation optimization, and design validation.

### Key Principle: "One Flow, One Intent"

Intent applies to entire signal flow paths, not individual nets. When a net branches, each branch can have its own intent. This captures design purpose explicitly and enables intelligent tool automation.

```bhdl
// Intent applies to entire flow path
net protection: sensor -> tvs: TVSDiode(6V).cathode -> tvs.anode -> r: Res(1k).1 -> r.2 -> @protected
    for input_protection(overvoltage: 6V, current_limit: 5mA);

// Different intents on branches
net monitor: @protected -> buf: Buffer().A -> buf.Y -> status_out
    for fault_detection(response: 10ns);

net measure: @protected -> filter -> adc
    for precision_measurement(accuracy: 0.1%);
```

## Basic Syntax

Intent annotations use the `for` keyword after flow statements:

```bhdl
net name: source -> component -> destination
    for intent_function(parameter: value, ...);
```

### Multiple Intents

You can apply multiple intents to a single flow by using multiple `for` clauses:

```bhdl
net feedback: @output -> R1: Res(10k).1 -> R1.2 -> ctrl.FB
    for control_loop(bandwidth: 10kHz, stability_margin: 45deg)
    for precision_measurement(accuracy: 1%, resolution: 12bit);
```

### Intent on Power Domains

Power domains can also have intents:

```bhdl
power VCC = 5V @ 1A for low_noise(max_ripple: 50mV);
```

## Intent Categories

### 1. Timing Intents

Control temporal behavior of signals.

#### `delay(time)`
Specifies required signal propagation delay.

```bhdl
net fast_path: @input -> buffer -> output
    for delay(10ns);
```

**Parameters:**
- `time`: Duration - Required delay (ns, us, ms)

**SimMode:** DigitalWithTiming

**Use Cases:**
- Synchronization paths
- Setup/hold time requirements
- Deliberate signal delays

#### `debounce(time)`
Removes mechanical switch bounce.

```bhdl
net button_debounced: @button_raw -> R1: Res(10k).1 -> C1: Cap(1u).1
    for debounce(time: 20ms);
```

**Parameters:**
- `time`: Duration - Debounce window

**SimMode:** DigitalWithTiming

**Use Cases:**
- Mechanical button inputs
- Switch contacts
- Relay contacts

#### `pulse_stretch(min_width)`
Extends pulse width to minimum duration.

```bhdl
net stretched: @trigger -> monostable -> output
    for pulse_stretch(min_width: 100us);
```

**Parameters:**
- `min_width`: Duration - Minimum pulse width

**SimMode:** DigitalWithTiming

#### `stable_for(duration)`
Requires signal stability for specified time.

```bhdl
net stable_signal: @input -> filter -> validated
    for stable_for(duration: 50ms);
```

**Parameters:**
- `duration`: Duration - Required stability time

**SimMode:** DigitalWithTiming

### 2. Signal Processing Intents

Shape and condition signals.

#### `noise_filtering(cutoff, attenuation?)`
Low-pass filtering to remove high-frequency noise.

```bhdl
net filtered: @noisy -> C1: Cap(100n).1 -> C1.2 -> @GND
    for noise_filtering(cutoff: 100kHz, attenuation: 40dB);
```

**Parameters:**
- `cutoff`: Frequency - -3dB cutoff frequency
- `attenuation`: Float (optional) - Required attenuation in dB at high frequencies

**SimMode:** MixedSignal or AnalogRequired (if attenuation specified)

**Use Cases:**
- Power supply filtering
- Sensor signal conditioning
- EMI reduction

#### `anti_alias(before, cutoff)`
Anti-aliasing filter before ADC.

```bhdl
net sensor_filtered: sensor.OUT -> Rf: Res(1k).1 -> Cf: Cap(100n).1
    for anti_alias(before: adc, cutoff: 1kHz);
```

**Parameters:**
- `before`: Component name - ADC being protected
- `cutoff`: Frequency - Filter cutoff (typically Nyquist/2)

**SimMode:** MixedSignal

**Use Cases:**
- Before any ADC
- Sample-and-hold circuits
- Data acquisition systems

#### `fast_response(risetime)`
Specifies required signal response time.

```bhdl
net fast_output: @input -> high_speed_buffer -> output
    for fast_response(risetime: 5ns);
```

**Parameters:**
- `risetime`: Duration - Maximum acceptable rise/fall time

**SimMode:** DigitalWithTiming or MixedSignal

### 3. Protection Intents

Safeguard circuits from damage.

#### `input_protection(overvoltage, current_limit?)`
Protects input from overvoltage and overcurrent.

```bhdl
net protected_input: VIN -> tvs: TVSDiode(15V).cathode -> tvs.anode -> @GND
    for input_protection(overvoltage: 15V, current_limit: 2A);
```

**Parameters:**
- `overvoltage`: Voltage - Maximum safe voltage
- `current_limit`: Current (optional) - Maximum safe current

**SimMode:** AnalogRequired

**Validation Rules:**
- Verifies TVS/Zener diode voltage rating
- Checks current limiting resistor if specified

#### `overvoltage_clamp(max_voltage)`
Clamps voltage to maximum level.

```bhdl
net clamped: @input -> clamp: Zener(5.1V).cathode -> clamp.anode -> @GND
    for overvoltage_clamp(max_voltage: 5.5V);
```

**Parameters:**
- `max_voltage`: Voltage - Clamp threshold

**SimMode:** AnalogRequired

#### `current_limiting(max)`
Limits current through path.

```bhdl
net led_current: @VCC -> R1: Res(330).1 -> led: LED(red).A
    for current_limiting(max: 15mA);
```

**Parameters:**
- `max`: Current - Maximum allowed current

**SimMode:** AnalogRequired

**Validation Rules:**
- Verifies current limiting resistor value
- Checks component power dissipation ratings

### 4. Power and Analog Intents

Manage power delivery and analog signal quality.

#### `low_noise(max_ripple)`
Requires low noise/ripple on power or analog signals.

```bhdl
power VCC = 5V @ 1A for low_noise(max_ripple: 10mV);
```

**Parameters:**
- `max_ripple`: Voltage - Maximum acceptable ripple

**SimMode:** AnalogRequired

**Use Cases:**
- Precision analog circuits
- ADC/DAC supplies
- Low-jitter clock generation

#### `signal_amplification(gain, bandwidth?)`
Amplifies signal with specified gain.

```bhdl
net amplified: @sensor -> opamp: OpAmp().IN -> opamp.OUT
    for signal_amplification(gain: 10, bandwidth: 100kHz);
```

**Parameters:**
- `gain`: Float - Voltage gain (V/V)
- `bandwidth`: Frequency (optional) - Required bandwidth

**SimMode:** AnalogRequired

#### `level_shifting(from, to)`
Shifts signal between voltage levels.

```bhdl
net shifted: @input_3v3 -> level_shifter -> output_5v
    for level_shifting(from: 3.3V, to: 5V);
```

**Parameters:**
- `from`: Voltage - Input level
- `to`: Voltage - Output level

**SimMode:** MixedSignal

### 5. Digital Intents

Optimize digital signal distribution.

#### `signal_buffering(fanout, drive?)`
Buffers signal for multiple loads.

```bhdl
net buffered: @source -> buf: Buffer().IN -> buf.OUT
    for signal_buffering(fanout: 8, drive: high);
```

**Parameters:**
- `fanout`: Integer - Number of loads to drive
- `drive`: String (optional) - Drive strength ("low", "medium", "high")

**SimMode:** PureDigital or DigitalWithTiming

**Synthesis Hints:**
- Recommends buffer type based on fanout
- Suggests driver sizing

#### `output_buffering(capacitive_load)`
Buffers output for capacitive loads.

```bhdl
net buffered_out: @signal -> driver -> output_pin
    for output_buffering(capacitive_load: 100pF);
```

**Parameters:**
- `capacitive_load`: Capacitance - Load capacitance

**SimMode:** DigitalWithTiming

#### `signal_distribution(fanout, balanced?)`
Distributes signal to multiple destinations.

```bhdl
net distributed: @clock -> fanout_buffer -> outputs
    for signal_distribution(fanout: 16, balanced: true);
```

**Parameters:**
- `fanout`: Integer - Number of destinations
- `balanced`: Boolean (optional) - Require matched delays

**SimMode:** DigitalWithTiming (if balanced), else PureDigital

### 6. Measurement and Control Intents

Support feedback and monitoring.

#### `precision_measurement(accuracy, resolution?)`
High-precision measurement requirements.

```bhdl
net measured: @voltage_divider -> adc.IN
    for precision_measurement(accuracy: 0.5%, resolution: 12bit);
```

**Parameters:**
- `accuracy`: Float - Required measurement accuracy (%)
- `resolution`: Integer (optional) - ADC resolution in bits

**SimMode:** AnalogRequired

**Use Cases:**
- Voltage/current monitoring
- Temperature sensing
- Closed-loop control

#### `control_loop(bandwidth, stability_margin?)`
Feedback control loop requirements.

```bhdl
net feedback: @output -> R1: Res(10k).1 -> R1.2 -> ctrl.FB
    for control_loop(bandwidth: 10kHz, stability_margin: 45deg);
```

**Parameters:**
- `bandwidth`: Frequency - Loop bandwidth
- `stability_margin`: Float (optional) - Phase margin in degrees

**SimMode:** AnalogRequired

**Validation Rules:**
- Verifies loop stability
- Checks compensation network

#### `data_logging(sample_rate)`
Data acquisition and logging.

```bhdl
net logged_data: @sensor -> adc -> microcontroller.ADC_IN
    for data_logging(sample_rate: 1kHz);
```

**Parameters:**
- `sample_rate`: Frequency - Required sampling rate

**SimMode:** MixedSignal

### 7. Safety Intents

Critical system requirements.

#### `debug_only()`
Marks signal as debug-only (not for production).

```bhdl
net debug_output: internal_signal -> test_point
    for debug_only();
```

**Parameters:** None

**SimMode:** Inherits from parent flow

**Use Cases:**
- Test points
- Debug interfaces
- Development-only features

#### `glitch_immunity(min_pulse)`
Ignores glitches shorter than minimum pulse width.

```bhdl
net glitch_free: @input -> filter -> output
    for glitch_immunity(min_pulse: 100ns);
```

**Parameters:**
- `min_pulse`: Duration - Minimum valid pulse width

**SimMode:** DigitalWithTiming

#### `fault_detection(response)`
Detects and responds to fault conditions.

```bhdl
net fault_monitor: @critical_signal -> comparator -> fault_flag
    for fault_detection(response: 1us);
```

**Parameters:**
- `response`: Duration - Maximum fault detection time

**SimMode:** MixedSignal

## Simulation Modes

Intent functions determine the simulation requirements for each flow path:

| SimMode | Description | When Used |
|---------|-------------|-----------|
| **PureDigital** | Simple boolean logic | No timing constraints, pure logic |
| **DigitalWithTiming** | Digital with timing | Delay, debounce, timing requirements |
| **MixedSignal** | Mixed analog/digital | Level shifting, ADC/DAC interfaces |
| **AnalogRequired** | Full analog simulation | Precision, noise, control loops |

The BHDL toolchain uses SimMode to optimize:
- **SPICE Analysis Scope**: Only analog-critical components simulated
- **Synthesis Strategy**: Component selection based on requirements
- **Validation Depth**: Appropriate checks for each domain

## Complete Examples

### Example 1: 7805 Voltage Regulator

```bhdl
board LM7805_Regulator {
    // Power supply with noise requirements
    power VIN = 12V @ 1A for low_noise(max_ripple: 50mV);
    ground GND;

    // Input protection circuit
    net protected_input: VIN -> tvs: TVSDiode(15V).cathode -> tvs.anode -> @GND
        for input_protection(overvoltage: 15V, current_limit: 2A);

    // Input filtering with anti-noise requirements
    net filtered_input: @protected_input -> C1: Cap(100n).1 -> C1.2 -> @GND
        for noise_filtering(cutoff: 100kHz, attenuation: 40dB);

    // Voltage regulation
    entity LM7805 {
        pin VIN: power in;
        pin VOUT: power out;
        pin GND: ground inout;
    }

    @filtered_input -> reg: LM7805().VIN;
    reg.GND -> @GND;

    // Output capacitor for stability
    net regulated_output: reg.VOUT -> C2: Cap(10u).1 -> C2.2 -> @GND
        for low_noise(max_ripple: 10mV);

    // Current-limited LED indicator
    power VOUT = 5V @ 1A;
    @regulated_output -> @VOUT;

    @VOUT -> R1: Res(330).1 -> R1.2 -> led: LED(green).A -> led.K -> @GND
        for current_limiting(max: 15mA);
}
```

**Intent Analysis:**
- **low_noise** on VIN and regulated_output → AnalogRequired simulation for ripple analysis
- **input_protection** → Validates TVS diode rating and current limiting
- **noise_filtering** → Verifies filter cutoff frequency and attenuation
- **current_limiting** → Checks resistor value for 15mA LED current

### Example 2: Buck Converter with Control Loop

```bhdl
board BuckConverter {
    power VIN = 12V @ 2A;
    ground GND;

    // Input protection
    net protected_vin: @VIN -> tvs: TVSDiode(18V).cathode -> tvs.anode -> @GND
        for input_protection(overvoltage: 18V, current_limit: 3A);

    // Input bulk capacitor
    net vin_filtered: @protected_vin -> Cin: Cap(100u).1 -> Cin.2 -> @GND
        for noise_filtering(cutoff: 10kHz);

    // Buck controller
    entity TPS54302 {
        pin VIN: power in;
        pin SW: signal out;
        pin FB: signal in;
        pin GND: ground inout;
    }

    @vin_filtered -> ctrl: TPS54302().VIN;
    ctrl.GND -> @GND;

    // LC filter
    net switch_node: ctrl.SW -> L1: Inductor(10u).1;
    net output_unfiltered: L1.2 -> Cout: Cap(47u).1
        for noise_filtering(cutoff: 100kHz, attenuation: 60dB);
    Cout.2 -> @GND;

    // Feedback with precision requirements
    net feedback: @output_unfiltered -> R1: Res(10k).1 -> R1.2 -> @GND
        for control_loop(bandwidth: 10kHz, stability_margin: 45deg)
        for precision_measurement(accuracy: 1%, resolution: 12bit);

    R1.2 -> ctrl.FB;

    power VOUT = 5V @ 1.5A;
    @output_unfiltered -> @VOUT;

    // Load with current monitoring
    net load_current: @VOUT -> Rload: Res(3.3).1 -> Rload.2 -> @GND
        for current_limiting(max: 1.5A)
        for precision_measurement(accuracy: 0.5%);
}
```

**Intent Analysis:**
- **control_loop** → Enables stability analysis with phase margin verification
- **precision_measurement** on feedback → Requires accurate resistor values
- **noise_filtering** with attenuation → Full AC analysis for filter performance
- **current_limiting** on load → Validates safe operating area

### Example 3: Mixed-Signal Circuit

```bhdl
board MixedSignalBoard {
    power VCC = 5V @ 500mA;
    ground GND;

    entity Button {
        pin OUT: signal out;
        pin GND: ground inout;
    }

    // Button with debouncing
    net button_raw: btn: Button().OUT;
    net button_debounced: @button_raw -> R1: Res(10k).1 -> C1: Cap(1u).1
        for debounce(time: 20ms);

    // Signal buffering for multiple loads
    net buffered_signal: @button_debounced -> buf: Buffer().IN -> buf.OUT
        for signal_buffering(fanout: 8, drive: high);

    // Fast digital path
    net fast_path: @buffered_signal -> R2: Res(100).1 -> led1: LED(red).A
        for delay(10ns)
        for current_limiting(max: 20mA);

    // Slow path with delay
    net slow_path: @buffered_signal -> R3: Res(1k).1 -> C2: Cap(1u).1
        for delay(1ms)
        for current_limiting(max: 10mA);

    // Analog sensor with anti-aliasing
    entity AnalogSensor {
        pin OUT: signal out;
        pin VCC: power in;
        pin GND: ground inout;
    }

    @VCC -> sensor: AnalogSensor().VCC;
    sensor.GND -> @GND;

    entity ADC {
        pin IN: signal in;
        pin CLK: clock in;
        pin DATA: signal out;
        pin VCC: power in;
        pin GND: ground inout;
    }

    net sensor_filtered: sensor.OUT -> Rf: Res(1k).1 -> Cf: Cap(100n).1
        for anti_alias(before: adc, cutoff: 1kHz);

    @sensor_filtered -> adc: ADC().IN;
    @VCC -> adc.VCC;
    adc.GND -> @GND;

    // Debug output
    net adc_data: adc.DATA -> R4: Res(1k).1
        for debug_only();
}
```

**Intent Analysis:**
- **debounce** → DigitalWithTiming for button stabilization
- **signal_buffering** → Recommends high-drive buffer for 8 loads
- **delay** intents → Two different timing paths with explicit delays
- **anti_alias** → MixedSignal analysis before ADC
- **debug_only** → Marks test point as non-production

## Best Practices

### 1. Be Specific

Provide exact requirements rather than general intents:

```bhdl
// ❌ Vague
net filtered: @input -> C1: Cap(100n).1
    for noise_filtering(cutoff: 1MHz);

// ✅ Specific
net filtered: @input -> C1: Cap(100n).1
    for noise_filtering(cutoff: 1MHz, attenuation: 60dB);
```

### 2. Intent at the Right Level

Apply intent where it matters most:

```bhdl
// ❌ Redundant - intent on every segment
net path1: @VCC -> R1: Res(330).1 for current_limiting(max: 15mA);
net path2: R1.2 -> led: LED(red).A for current_limiting(max: 15mA);

// ✅ Concise - intent on complete flow
net led_path: @VCC -> R1: Res(330).1 -> R1.2 -> led: LED(red).A -> led.K -> @GND
    for current_limiting(max: 15mA);
```

### 3. Combine Related Intents

Use multiple `for` clauses for comprehensive requirements:

```bhdl
net critical_signal: @sensor -> filter -> adc.IN
    for precision_measurement(accuracy: 0.1%)
    for anti_alias(before: adc, cutoff: 10kHz)
    for low_noise(max_ripple: 1mV);
```

### 4. Document Intent Rationale

Use comments to explain design decisions:

```bhdl
// Audio ADC requires anti-aliasing at 20kHz for 44.1kHz sampling
net audio_filtered: @mic_preamp -> Rf: Res(1k).1 -> Cf: Cap(8.2n).1
    for anti_alias(before: audio_adc, cutoff: 20kHz);
```

### 5. Trust the Tools

Don't over-specify implementation details. Let the tools choose appropriate components:

```bhdl
// ❌ Over-specified - limits tool flexibility
net filtered: @input -> R1: Res(1k).1 -> C1: Cap(100n).1
    for noise_filtering(cutoff: 1.6kHz, attenuation: 40dB);  // Exact RC values

// ✅ Specify requirements, not implementation
net filtered: @input -> filter
    for noise_filtering(cutoff: 1.5kHz, attenuation: 40dB);  // Let synthesizer choose
```

## Tool Integration

### For Circuit Designers

The Intent System automates tedious verification tasks:

1. **Component Selection**: Tools recommend appropriate components based on intent
2. **Value Calculation**: Resistor/capacitor values calculated from requirements
3. **Safety Validation**: Automatic checks for overcurrent, overvoltage, thermal issues
4. **Simulation Optimization**: Only critical paths get full analog simulation

### For Tool Developers

Intent information is available throughout the pipeline:

```rust
// Access flow tracker from analysis result
let flow_tracker = &analysis_result.flow_tracker;
let flow_paths = flow_tracker.get_flow_paths();

for flow_path in flow_paths {
    if let Some(intent_result) = &flow_path.intent_result {
        // Simulation mode
        let sim_mode = intent_result.sim_mode;

        // Synthesis hints
        for hint in &intent_result.synthesis_hints {
            match hint {
                SynthesisHint::RCNetwork { r_range, c_range, .. } => {
                    // Use for component selection
                }
                SynthesisHint::CurrentLimiting { max_current, .. } => {
                    // Calculate resistor value
                }
                _ => {}
            }
        }

        // Validation rules
        for rule in &intent_result.validation_rules {
            // Check conditions: rule.condition
            // Report errors: rule.error_message
        }
    }
}
```

## Troubleshooting

### Intent Not Recognized

**Problem**: `Unknown intent function: my_intent`

**Solution**: Ensure you're using a stdlib intent function. See the Intent Categories section for all available intents.

### Type Mismatch Errors

**Problem**: `Expected Voltage, found Duration`

**Solution**: Check parameter types in the intent function definition. Use correct units:

```bhdl
// ❌ Wrong type
for delay(1V)  // Should be duration, not voltage

// ✅ Correct type
for delay(1ms)
```

### Multiple Conflicting Intents

**Problem**: `Conflicting intents: fast_response and debounce`

**Solution**: Some intents are inherently contradictory. Choose the primary requirement:

```bhdl
// ❌ Conflict - can't be both fast and debounced
net signal: @input -> output
    for fast_response(risetime: 1ns)
    for debounce(time: 50ms);

// ✅ Choose the dominant requirement
net signal: @input -> output
    for debounce(time: 50ms);  // Debouncing implies slower response
```

## Summary

The BHDL Intent System transforms circuit design by making your intentions explicit. This enables:

- **Intelligent Component Selection**: Tools choose appropriate parts for your requirements
- **Automated Validation**: Safety and correctness checks happen automatically
- **Optimized Simulation**: Only critical paths get expensive analog simulation
- **Design Documentation**: Intent serves as self-documenting specification

Start by adding intents to critical paths (protection, precision, safety), then expand to other signals as you see the benefits.

For more information:
- **Implementation Details**: `docs/implementation/Intent_and_Flow_System.md`
- **Standard Library Reference**: `bhdl-stdlib/src/intents/`
- **Example Circuits**: `tests/circuits/realistic/`
