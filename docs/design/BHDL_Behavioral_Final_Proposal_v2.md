# BHDL Behavioral Modeling - Final Proposal v2

## Executive Summary

BHDL needs only **5 new concepts** to enable powerful behavioral modeling while maintaining a clean, intuitive design:

1. **Attributes** - Configuration and properties
2. **Signals** - All dynamic behavior (state, expressions, outputs)  
3. **External model decorator** - `@behavioral` for PLI
4. **Testbench co-simulation** - `@cosim` for external tests
5. **Built-in `dt` variable** - Simulation timestep

## The 5 New Concepts

### 1. Attributes for Configuration

```bhdl
module PIController {
    // Static configuration
    attribute description = "PI Controller";
    attribute kp = 0.1;
    attribute ki = 0.01;
    attribute vref = 3.3V;
    
    // Mutable attributes (can only reference other attributes)
    attribute enable_integral = true;
    
    when (some_attribute_condition) {
        enable_integral = false;  // Attributes can be modified based on other attributes
    }
}
```

**Key rules:**
- Can only reference other attributes
- Cannot reference signals or pins
- Represent module configuration/metadata
- Can be mutable but only based on attribute logic

### 2. Signals for Dynamic Behavior

```bhdl
module PowerController {
    pin ENABLE: digital in;
    pin FB: analog in;
    pin PWM: digital out;
    pin PGOOD: digital out;
    
    // Configuration
    attribute vref = 3.3V;
    attribute soft_start_time = 10ms;
    
    // Signals: Expression-based (continuously evaluated)
    signal error = vref - FB;
    signal in_regulation = abs(error) < 0.1V;
    
    // Signals: State-based (modified in when blocks)
    signal vref_ramped = 0V;
    signal pwm_duty = 0.0;
    signal power_good = false;
    
    // Time-based behavior
    when (ENABLE && vref_ramped < vref) {
        vref_ramped += vref / soft_start_time * dt;
    }
    
    when (!ENABLE) {
        vref_ramped = 0V;
        pwm_duty = 0.0;
    }
    
    when (in_regulation) {
        power_good = true;
        pwm_duty = 0.5 + error * 0.1;  // Simple P control
    }
    
    // Connect signals to pins
    PWM = pwm_duty;
    PGOOD = power_good;
}
```

**Key rules:**
- Can reference attributes, other signals, and pins
- Can be expressions (continuously evaluated)
- Can be state (modified in when blocks)
- Can drive output pins

### 3. External Model Decorator

```bhdl
module ComplexController {
    pin VIN: power in;
    pin VOUT: power out;
    pin I_SENSE: analog in;
    
    // Link to external behavioral model
    @behavioral(model="controllers.BuckController", language="python")
}
```

### 4. Testbench Co-simulation

```bhdl
testbench Validation for PowerSupply {
    stimulus {
        @0ms: VIN = 12V;
        @1ms: ENABLE = high;
        @10ms: LOAD = 1A;
    }
    
    // Link to external test harness
    @cosim(harness="test_power_supply.py", mode="batch")
}
```

### 5. Built-in `dt` Variable

```bhdl
// Available globally in when blocks
when (ramping) {
    voltage += slew_rate * dt;  // dt is timestep in seconds
}
```

## Complete Examples

### Example 1: LED Thermal Controller (Pure BHDL)

```bhdl
module ThermalLED {
    pin TEMP_SENSE: analog in;
    pin LED_DRIVE: current out;
    
    // Configuration
    attribute temp_threshold = 85;     // Celsius
    attribute temp_critical = 105;
    attribute i_nominal = 350mA;
    
    // Dynamic behavior
    signal temp_c = (TEMP_SENSE - 0.5V) / 10mV;
    signal derating = if (temp_c < temp_threshold) { 1.0 }
                     else if (temp_c < temp_critical) { 
                         (temp_critical - temp_c) / (temp_critical - temp_threshold)
                     }
                     else { 0.0 };
    
    signal i_limit = i_nominal * derating;
    
    LED_DRIVE = i_limit;
}
```

### Example 2: Power Sequencer

```bhdl
module PowerSequencer {
    pin ENABLE: digital in;
    pin EN0, EN1, EN2, EN3: digital out;
    pin ALL_GOOD: digital out;
    
    // Configuration
    attribute t_delay = 1ms;
    attribute t_timeout = 10ms;
    
    // State signals
    signal timer = 0ms;
    signal en0_state = low;
    signal en1_state = low;
    signal en2_state = low;
    signal en3_state = low;
    
    when (ENABLE) {
        timer += dt;
        
        when (timer > 0ms) {
            en0_state = high;
        }
        
        when (timer > 1 * t_delay) {
            en1_state = high;
        }
        
        when (timer > 2 * t_delay) {
            en2_state = high;
        }
        
        when (timer > 3 * t_delay) {
            en3_state = high;
        }
    }
    
    when (!ENABLE) {
        timer = 0ms;
        en0_state = low;
        en1_state = low;
        en2_state = low;
        en3_state = low;
    }
    
    // Expression signal
    signal all_enabled = en0_state && en1_state && en2_state && en3_state;
    
    // Connect to pins
    EN0 = en0_state;
    EN1 = en1_state;
    EN2 = en2_state;
    EN3 = en3_state;
    ALL_GOOD = all_enabled;
}
```

### Example 3: Buck with Soft-Start

```bhdl
module BuckController {
    pin ENABLE: digital in;
    pin VIN: power in;
    pin FB: analog in;
    pin PWM: digital out;
    pin PGOOD: digital out;
    
    // Configuration
    attribute vout_target = 3.3V;
    attribute soft_start_time = 10ms;
    attribute kp = 0.1;
    
    // State signals
    signal vref = 0V;
    signal pwm_duty = 0.0;
    signal timer = 0ms;
    
    // Expression signals
    signal error = vref - FB;
    signal in_regulation = abs(error) < 0.1V;
    
    // Soft-start ramp
    when (ENABLE && vref < vout_target) {
        vref += vout_target / soft_start_time * dt;
    }
    
    when (!ENABLE) {
        vref = 0V;
        timer = 0ms;
        pwm_duty = 0.0;
    }
    
    // Simple proportional control
    when (ENABLE) {
        timer += dt;
        pwm_duty = clamp(0.5 + error * kp, 0.1, 0.9);
    }
    
    // Power good after soft-start + in regulation
    signal power_good = (timer > soft_start_time) && in_regulation;
    
    PWM = pwm_duty;
    PGOOD = power_good;
}
```

### Example 4: Complex USB PD (PLI)

```bhdl
module USBPDController {
    pin CC1, CC2: analog inout;
    pin VBUS_EN: digital out;
    pin VBUS_PROG: analog out;
    
    // All complex protocol logic in external model
    @behavioral(model="usb_pd.PDController", language="python")
}
```

## Why This Design Works

### 1. Clear Separation of Concerns
- **Attributes**: What the module IS (configuration)
- **Signals**: What the module DOES (behavior)
- **Pins**: How the module CONNECTS (interface)

### 2. Natural Mental Model
- "Attributes" naturally mean properties/configuration
- "Signals" naturally mean things that change/flow
- No artificial restrictions that users need to remember

### 3. Flexible Yet Simple
- Only 2 main keywords for behavioral: `attribute` and `signal`
- Compiler infers intent from usage
- Covers all use cases elegantly

### 4. Clean Data Flow
```
Attributes → Signals → Pins
    ↓          ↓
(config)  (behavior)
```

### 5. Easy to Learn
- Start with attributes for configuration
- Add signals for behavior
- Graduate to PLI for complex models

## Implementation Notes

### Compiler Rules

1. **Attribute validation**:
   - Can only reference other attributes
   - Detect circular dependencies

2. **Signal classification**:
   - Modified in `when` → State signal
   - Never modified → Expression signal
   - Both → Compiler error

3. **Type inference**:
   - From literals: `3.3V` → voltage
   - From operations: voltage - voltage → voltage
   - From context: signal assigned to `current out` → current

### Performance Considerations

1. **Expression signals**: Evaluated each timestep
2. **State signals**: Only updated when when-blocks execute
3. **Dependency tracking**: Avoid redundant evaluations

## Migration Path

1. **Phase 1**: Basic behavioral (attributes + signals)
2. **Phase 2**: Time-based (`dt` and when blocks)
3. **Phase 3**: PLI for complex models
4. **Phase 4**: Advanced optimizations

## Conclusion

This refined design gives BHDL powerful behavioral modeling with just 5 intuitive concepts. The separation between attributes (configuration) and signals (behavior) creates a clean mental model that's easy to learn and use.