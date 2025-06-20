# BHDL Behavioral Modeling - Final Proposal (Unified)

## Executive Summary

BHDL needs only **5 new concepts** (not 6!) by extending the existing `attribute` keyword rather than adding a new `param` keyword.

## The 5 New Concepts

### 1. Extended Attributes (Expressions & Pin References)

```bhdl
module Controller {
    pin FB: analog in;
    pin PWM: digital out;
    
    // Existing: static attributes
    attribute description = "Buck controller";
    
    // NEW: Attributes can be expressions referencing pins
    attribute error = 3.3V - FB;
    attribute duty = clamp(0.5 + error * 0.1, 0.1, 0.9);
    
    // NEW: Can assign attributes to pins
    PWM = duty;
}
```

### 2. Mutable Attributes (Inferred from Usage)

```bhdl
module SoftStart {
    pin ENABLE: digital in;
    
    // Looks like regular attribute
    attribute vref = 0V;
    
    // But becomes mutable when modified
    when (ENABLE && vref < 3.3V) {
        vref += 3.3V / 10ms * dt;  // Makes vref mutable
    }
}
```

### 3. External Model Decorator

```bhdl
module ComplexController {
    pin VIN: power in;
    pin VOUT: power out;
    
    // Link to external behavioral model
    @behavioral(model="buck_controller", language="python")
}
```

### 4. Testbench Co-simulation Decorator

```bhdl
testbench Validation for Controller {
    stimulus {
        @0ms: VIN = 12V;
    }
    
    // Link to external test harness
    @cosim(harness="tests.py", mode="batch")
}
```

### 5. Built-in `dt` Variable

```bhdl
// Global timestep variable
when (ramping) {
    value += rate * dt;  // dt in seconds
}
```

## That's It! Only 5 Concepts

By reusing `attribute` instead of adding `param`, we get one less keyword to learn!

## What This Enables

### Simple Behavioral (No PLI)
```bhdl
module ThermalLED {
    pin TEMP_SENSE: analog in;
    pin LED_DRIVE: current out;
    
    // Temperature calculation
    attribute temp_c = (TEMP_SENSE - 0.5V) / 10mV;
    
    // Derating logic
    attribute i_max = if (temp_c < 85) { 350mA }
                     else if (temp_c < 105) { 350mA * (125-temp_c)/40 }
                     else { 0mA };
    
    LED_DRIVE = i_max;
}
```

### Time-Based Behavioral
```bhdl
module BuckWithSoftStart {
    pin ENABLE: digital in;
    pin FB: analog in;
    pin PWM: digital out;
    
    // State that evolves over time
    attribute vref = 0V;  // Becomes mutable due to modification below
    
    // Control calculations
    attribute error = vref - FB;
    attribute duty = clamp(0.3 + error * 0.1, 0.1, 0.9);
    
    // Soft start ramp
    when (ENABLE && vref < 3.3V) {
        vref += 3.3V / 10ms * dt;
    }
    
    PWM = duty;
}
```

### Complex Behavioral (PLI)
```bhdl
module USBPDController {
    pin CC1, CC2: analog inout;
    pin VBUS: power out;
    
    // Complex protocol in external model
    @behavioral(model="usb_pd.PDController", language="python")
}
```

## Why Unifying with `attribute` is Better

1. **No new keywords** - Reuses existing `attribute`
2. **Conceptually cleaner** - Attributes are "properties" (static or dynamic)
3. **Natural progression**:
   - Static: `attribute version = "1.0"`
   - Computed: `attribute error = target - actual`
   - Time-varying: `attribute vref = 0V` + modifications
4. **Backward compatible** - Existing attributes work unchanged
5. **Less to learn** - One concept instead of two

## Implementation Notes

### Parser Rules
```
attribute_decl := 'attribute' IDENT '=' expression ';'
expression := literal | pin_ref | binary_op | conditional | function_call
```

### Mutability Detection
```rust
// During semantic analysis
if attribute_modified_in_when_block(attr) {
    mark_as_mutable(attr);
}
```

### Type Inference
```rust
// Infer types from usage
match expression {
    BinaryOp(left, op, right) => infer_numeric_type(left, right),
    PinRef(pin) => get_pin_type(pin),
    // ...
}
```

## Migration Guide

### Before (Proposed with param)
```bhdl
param vref = 0V;
param error = vref - FB;
```

### After (Unified with attribute)
```bhdl
attribute vref = 0V;
attribute error = vref - FB;
```

### Existing Code (Unchanged)
```bhdl
attribute title = "My Board";
attribute version = "1.0";
```

## Summary

This unified approach gives us:
- **Fewer keywords** (5 concepts vs 6)
- **Cleaner mental model** (attributes for all properties)
- **Same power** (simple behavioral + PLI)
- **Better consistency** with existing BHDL

The key insight: `attribute` already means "property of this module" - whether that property is static metadata, a computed value, or time-varying state!