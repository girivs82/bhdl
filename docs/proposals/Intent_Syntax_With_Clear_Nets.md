# Intent Syntax with Clarified Net System

## Overview

With the clarified net syntax (@ for nets, : for components), intent attachment becomes much clearer and more intuitive.

## Intent Attachment Points

### 1. On Named Flows (Labels)

```bhdl
// Flow labels use : but they're not components - they're labels
critical_path: VCC -> @protected -> LED(red).A for timing_critical;
power_distribution: VIN -> @main -> regulators for safety_critical;
```

### 2. On Anonymous Flows

```bhdl
// Direct connection with intent
VCC -> Res(10k).1 -> LED(red).A for indicator(purpose: power);

// Multi-segment flow with intent  
sensor -> OpAmp().+ -> Filter().OUT for low_noise(bandwidth: 10kHz);
```

### 3. On Net Creation

```bhdl
// Intent when creating net
VCC -> @filtered for anti_alias(before: adc) -> amp.IN;

// Or split across lines for readability
VCC -> @main_power for safety_critical(sil: 2);
@main_power -> reg1.IN;
@main_power -> reg2.IN;
```

### 4. NOT on Net References

```bhdl
// WRONG - can't attach intent to net reference
@filtered for something -> amp.IN;  // ERROR!

// RIGHT - intent only at creation or on flows
VCC -> @filtered for noise_immunity -> amp.IN;
```

## Complete Examples

### Example 1: Power Supply with Clear Intent

```bhdl
board SafePowerSupply {
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    ground GND;
    
    // Input protection flow with intent
    protection: VIN -> fuse: Fuse(2A).1 -> @protected 
        for input_protection(overvoltage: 15V, current_limit: 2A);
    
    // Protected net to TVS diode
    @protected -> tvs: TVSDiode(15V).K;
    tvs.A -> GND;
    
    // Regulation with safety intent
    main_regulation: @protected -> reg: LM7805().IN 
        for safety_critical(redundancy: parallel);
    
    // Output filtering with performance intent
    reg.OUT -> @raw_5v -> filter: LCFilter().IN
        for noise_filtering(ripple: 10mV, bandwidth: 100kHz);
    
    // Clean power distribution
    filter.OUT -> @VCC for power_distribution(star_topology);
    
    // Indicator with simple intent
    @VCC -> Res(330).1 -> LED(green).A for indicator;
    LED(green).K -> GND;
}
```

### Example 2: Sensor Interface with Intent

```bhdl
entity SensorInterface {
    pin SENSOR_IN: signal in;
    pin ADC_OUT: signal out;
    pin VCC: power in;
    pin GND: ground in;
    
    // Input conditioning with measurement intent
    sensor_path: SENSOR_IN -> @raw_signal 
        for signal_conditioning(impedance: high);
    
    // Protection stage
    @raw_signal -> protection: CrowbarClamp(3.6V).IN
        for overvoltage_protection(clamp: 3.6V);
    protection.OUT -> @protected_signal;
    
    // Amplification with precision intent
    gain_stage: @protected_signal -> amp: InstrumentationAmp(gain: 100).IN+
        for precision_measurement(offset: 1mV, drift: 10ppm);
    amp.IN- -> GND;
    amp.OUT -> @amplified;
    
    // Filtering with anti-aliasing intent  
    @amplified -> filter: Butterworth4(fc: 10kHz).IN
        for anti_alias(before: adc, margin: 2x);
    
    // Final output
    filter.OUT -> ADC_OUT;
}
```

### Example 3: Mixed Intents - Board and Flow Level

```bhdl
// Board-level intent
board CriticalController for automotive_safety(asil: B) {
    power VBAT = 12V @ 5A;
    ground GND;
    
    // This flow inherits automotive_safety intent
    // but adds specific power intent
    power_input: VBAT -> @protected for reverse_polarity_protection;
    
    // Critical path overrides with higher safety level
    critical_sensors: @protected -> sensor_supply: PowerReg(3.3V).IN
        for safety_critical(asil: D);  // Overrides board's ASIL-B
    
    // Non-critical path opts out
    debug_leds: @protected -> Res(1k).1 -> LED(red).A
        for debug_only;  // Opts out of safety requirements
}
```

## Key Principles

1. **Intent follows flow**: Attach intent where the flow is defined
2. **Creation not reference**: Intent only when creating nets/flows, not when referencing
3. **Clear precedence**: More specific intent overrides general
4. **Natural reading**: "Create @filtered for anti_alias"
5. **No ambiguity**: @ = net, : = component/label, for = intent

## Benefits of Clear Syntax

1. **Intuitive**: "VCC -> @filtered for noise_immunity" reads naturally
2. **Unambiguous**: Always clear what entity the intent applies to
3. **Flexible**: Attach intent at natural points in the flow
4. **Hierarchical**: Board/module/flow/net intent precedence is clear
5. **Tool-friendly**: Parser knows exactly what each symbol means

## Summary

With the clarified @ for nets and : for components rule:
- Intent attachment points are obvious
- The syntax reads naturally left-to-right
- No confusion about what entity receives the intent
- Maintains BHDL's flow-based philosophy

This creates a clean, intuitive system for expressing design intent while maintaining the natural flow-based syntax that makes BHDL unique.