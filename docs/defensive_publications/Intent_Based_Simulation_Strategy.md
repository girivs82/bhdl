# Defensive Publication: Intent-Based Simulation Strategy Selection

**Publication Date**: [DATE]  
**Authors**: [Your Name]  
**Contact**: [Your Email]

## Abstract

This publication discloses a novel method for automatically selecting simulation strategies (digital, analog, or mixed-signal) based on explicitly declared design intent rather than attempting to automatically detect analog/digital boundaries. The system uses a flow-based intent declaration that applies to entire signal paths.

## Problem Statement

Traditional mixed-signal simulation requires:
- Manual partitioning of analog and digital regions
- Complex boundary detection algorithms
- Conservative over-simulation due to uncertainty

These approaches fail because:
- Analog/digital boundaries are context-dependent
- Components like MOSFETs can operate in multiple modes
- No clean boundaries exist in many real circuits

## Innovation

### 1. Flow-Based Intent Declaration

Intent is attached to signal flow paths using a `for` keyword:
```bhdl
net protection: sensor -> TVS(6V).A -> Res(1k).1 -> @protected
    for input_protection(overvoltage: 6V, current_limit: 5mA)
```

### 2. Intent Applies to Entire Flows

Unlike net-based annotations, intent covers the complete signal path:
- Multiple nets in sequence
- All pins along the path
- All components involved

### 3. Branch-Specific Intents

When signals branch, each branch can have different intent:
```bhdl
net fast_monitor: @protected -> Buffer().A -> monitor_out
    for fault_detection(response: 10ns)
    
net precise_measure: @protected -> Filter() -> ADC
    for precision_measurement(accuracy: 0.1%)
```

### 4. Extensible Intent Library

Intents are not keywords but library functions:
```bhdl
// In stdlib
intent noise_filtering(cutoff: frequency, attenuation: dB) {
    simulation_mode = SimMode::AnalogRequired
    synthesis_hint = FilterImplementation
    validation_rule = CheckCutoffFrequency
}
```

### 5. Hierarchical Intent Propagation

Intents flow through design hierarchy:
- Board-level intents apply to all contents
- Module-level intents override board level
- Flow-level intents are most specific

## Implementation Architecture

### Core Types
```rust
enum SimMode {
    PureDigital,
    DigitalWithTiming,
    MixedSignal,
    AnalogRequired
}

struct IntentResult {
    simulation_mode: SimMode,
    synthesis_hint: Option<SynthHint>,
    validation_rules: Vec<Rule>,
    propagation: IntentPropagation
}
```

### Flow Tracking Algorithm

1. Parse intent declaration with flow
2. Track all nets/pins in flow sequence
3. Detect branch points
4. Apply appropriate simulation mode to each segment
5. Handle mode transitions at boundaries

### Intent Categories

**Timing**: delay(), pulse_stretch(), debounce()
**Signal Processing**: noise_filtering(), anti_alias()  
**Protection**: input_protection(), overvoltage_clamp()
**Power**: signal_amplification(), level_shifting()
**Digital**: signal_buffering(), signal_distribution()
**Measurement**: precision_measurement(), data_logging()
**Safety**: automotive_safety(), medical_safety()

## Novel Aspects

1. **Explicit over Implicit**: Designers declare intent, tools don't guess
2. **Flow-Based Scope**: Intent covers entire signal paths
3. **Branch Awareness**: Different strategies for different branches
4. **Library Extensibility**: New intents without language changes
5. **Hierarchical Composition**: Natural intent inheritance

## Example: Mixed-Signal Circuit

```bhdl
board SensorInterface for industrial_monitoring {
    // Board-level intent applies to all
    
    // Analog processing path
    net sensor_input: sensor -> amp -> filter -> ADC
        for precision_sensing(accuracy: 0.1%, bandwidth: 1kHz)
    // Maps to: AnalogRequired
    
    // Digital monitoring path  
    net status: sensor -> comparator -> interrupt
        for fault_detection(response: 1us)
    // Maps to: DigitalWithTiming
    
    // Power path
    net power: VCC -> reg -> sensor
        for power_delivery(ripple: <50mV)
    // Maps to: MixedSignal
}
```

## Advantages Over Prior Art

1. **No Boundary Detection**: Eliminates flawed automatic detection
2. **Designer Control**: Explicit intent captures design knowledge
3. **Tool Optimization**: Each path gets optimal simulation
4. **Documentation**: Intent serves as inline documentation
5. **Extensibility**: Domain-specific intents possible

## Industrial Applicability

- EDA tools for mixed-signal simulation
- Circuit synthesis and optimization
- Design rule checking
- Automated documentation generation
- Design review and validation tools

## Conclusion

This intent-based approach solves the fundamental challenge of mixed-signal simulation by leveraging designer knowledge rather than attempting error-prone automatic detection. The system is extensible, intuitive, and provides better results than traditional approaches.

---

*This publication establishes prior art for these innovations to ensure they remain freely available for community use.*