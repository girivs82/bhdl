# Unified Attribute System for BHDL Behavioral Modeling

## The Problem

Currently BHDL has:
- `attribute` for static metadata (title, version, etc.)
- Proposed `param` for behavioral expressions

This is confusing! Let's unify them.

## Solution: Extend `attribute` for All Use Cases

### 1. Static Attributes (Current)
```bhdl
board PowerSupply {
    // Current usage - unchanged
    attribute title = "Buck Converter";
    attribute version = "1.0";
}
```

### 2. Behavioral Attributes (New)
```bhdl
module BuckController {
    pin FB: analog in;
    pin PWM: digital out;
    
    // NEW: Attributes can be expressions and reference pins
    attribute vref = 3.3V;
    attribute error = vref - FB;
    attribute duty = clamp(0.5 + error * 0.1, 0.1, 0.9);
    
    // NEW: Can assign attributes to pins
    PWM = duty;
}
```

### 3. Mutable Attributes (New)
```bhdl
module SoftStart {
    pin ENABLE: digital in;
    pin VREF_OUT: analog out;
    
    // NEW: Mutable attribute
    attribute var vref_internal = 0V;
    
    // Time-based updates
    when (ENABLE && vref_internal < 3.3V) {
        vref_internal += 3.3V / 10ms * dt;
    }
    
    VREF_OUT = vref_internal;
}
```

## Syntax Options

### Option A: Use `var` keyword for mutable
```bhdl
attribute title = "Static metadata";        // Immutable
attribute error = vref - FB;               // Immutable expression
attribute var vref = 0V;                   // Mutable
```

### Option B: Use different assignment operator
```bhdl
attribute title = "Static metadata";        // Immutable (=)
attribute error = vref - FB;               // Immutable expression (=)
attribute vref := 0V;                      // Mutable (:=)
```

### Option C: Infer from usage
```bhdl
attribute title = "Static metadata";        // Immutable (never modified)
attribute vref = 0V;                       // Becomes mutable when modified

when (condition) {
    vref += rate * dt;  // Makes vref mutable
}
```

## Benefits of Unified Approach

1. **Single keyword** - No param/attribute confusion
2. **Backward compatible** - Existing attributes work unchanged
3. **Clear semantics** - Attributes are "properties of this module"
4. **Natural extension** - From static to dynamic naturally

## Complete Minimal Set (Revised)

### 1. Extended Attributes
```bhdl
// Static
attribute version = "1.0";

// Expression-based
attribute error = target - actual;

// Mutable
attribute var state = 0;
```

### 2. Time-Based Updates
```bhdl
when (condition) {
    state += delta * dt;
}
```

### 3. Attribute-to-Pin Assignment
```bhdl
PIN_OUT = attribute_value;
```

### 4. External Model Decorator
```bhdl
@behavioral(model="controller", language="python")
```

### 5. Testbench Co-simulation
```bhdl
@cosim(harness="tests.py")
```

### 6. Built-in `dt`
```bhdl
value += rate * dt;
```

## Examples with Unified Syntax

### Thermal LED Controller
```bhdl
module ThermalLED {
    pin TEMP: analog in;
    pin LED_DRIVE: current out;
    
    // All using 'attribute'
    attribute temp_c = (TEMP - 0.5V) / 10mV;
    attribute i_limit = if (temp_c < 85) { 350mA }
                       else if (temp_c < 105) { 350mA * (125-temp_c)/40 }
                       else { 0mA };
    
    LED_DRIVE = i_limit;
}
```

### Buck with Soft Start
```bhdl
module SimpleBuck {
    pin ENABLE: digital in;
    pin FB: analog in;
    pin PWM: digital out;
    
    // Mutable attribute for soft start
    attribute var vref = 0V;
    
    // Expression attributes
    attribute error = vref - FB;
    attribute duty = clamp(0.3 + error * 0.1, 0.1, 0.9);
    
    when (ENABLE && vref < 3.3V) {
        vref += 3.3V / 10ms * dt;
    }
    
    PWM = duty;
}
```

## Recommendation

I recommend **Option C: Infer from usage**

Reasons:
1. Least syntax addition
2. Most intuitive - "if you modify it, it's mutable"
3. No new keywords beyond extending `attribute`
4. Clean, simple code

This gives us behavioral modeling with just ONE existing keyword extended!