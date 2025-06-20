# BHDL Behavioral Modeling - Final Minimal Proposal

## Executive Summary

Based on our analysis, BHDL needs only **6 new constructs** to enable powerful behavioral modeling while keeping the language simple and focused on board description.

## The 6 New Constructs

### 1. Mutable Parameters with Expressions

```bhdl
module Controller {
    pin FB: analog in;
    pin PWM: digital out;
    
    // NEW: Parameters can reference pins and be expressions
    param error: voltage = 3.3V - FB;
    param duty: ratio = clamp(0.5 + error * 0.1, 0.1, 0.9);
    
    // NEW: Direct assignment from param to pin
    PWM = duty;  // Implicit ratio→PWM conversion
}
```

### 2. Time-Based Parameter Updates

```bhdl
module SoftStart {
    pin ENABLE: digital in;
    
    // NEW: Mutable parameter
    param vref: voltage = 0V;
    
    // NEW: Can modify params in 'when' blocks with 'dt'
    when (ENABLE && vref < 3.3V) {
        vref += 3.3V / 10ms * dt;  // dt is timestep
    }
}
```

### 3. External Model Decorator

```bhdl
module ComplexController {
    pin VIN: power in;
    pin VOUT: power out;
    
    // NEW: Single decorator for external models
    @behavioral(model="buck_controller", language="python")
}
```

### 4. Parameter Passing to External Models

```bhdl
module ParametricController(fsw: frequency = 500kHz) {
    pin VIN: power in;
    pin VOUT: power out;
    
    // NEW: Pass BHDL params to external model
    @behavioral(
        model="controller.BuckModel",
        params={switching_freq: fsw}
    )
}
```

### 5. Testbench Co-simulation

```bhdl
testbench Validation for Controller {
    // Standard BHDL stimulus
    stimulus {
        @0ms: VIN = 12V;
    }
    
    // NEW: Link to external test harness
    @cosim(harness="tests.py", mode="batch")
}
```

### 6. Built-in `dt` Variable

```bhdl
// NEW: 'dt' available globally as simulation timestep
when (condition) {
    value += rate * dt;  // dt in seconds
}
```

## That's It! Just 6 Concepts

### What We DON'T Need to Add:
- ❌ State machines
- ❌ Complex control flow  
- ❌ Function definitions
- ❌ Arrays/matrices
- ❌ String manipulation
- ❌ File I/O
- ❌ Advanced math functions
- ❌ Object-oriented features

All complex features are handled by external models in Python/Rust/C++.

## Examples Showing the Power

### Example 1: Thermal LED (Pure BHDL)
```bhdl
module ThermalLED {
    pin TEMP_SENSE: analog in;
    pin LED_DRIVE: current out;
    
    // Simple behavioral - no PLI needed
    param temp_c = (TEMP_SENSE - 0.5V) / 10mV;
    param i_max = if (temp_c < 85) { 350mA }
                  else if (temp_c < 105) { 350mA * (125-temp_c)/40 }
                  else { 0mA };
    
    LED_DRIVE = min(i_max, 350mA);
}
```

### Example 2: Buck with Soft Start (Pure BHDL)
```bhdl
module SimpleBuck {
    pin ENABLE: digital in;
    pin FB: analog in;
    pin PWM: digital out;
    
    param vref: voltage = 0V;
    param error: voltage = vref - FB;
    param duty: ratio = clamp(0.3 + error * 0.1, 0.1, 0.9);
    
    // Soft start
    when (ENABLE && vref < 3.3V) {
        vref += 3.3V / 10ms * dt;
    }
    
    PWM = duty;
}
```

### Example 3: Complex USB PD (PLI)
```bhdl
module USBPD {
    pin CC1, CC2: analog inout;
    pin VBUS: power out;
    
    // Complex protocol in Python
    @behavioral(model="usb_pd.Controller", language="python")
}
```

### Example 4: Motor FOC (PLI)
```bhdl
module MotorController {
    pin PHASE_A, PHASE_B, PHASE_C: current in;
    pin PWM_A, PWM_B, PWM_C: digital out;
    
    // High-performance FOC in Rust
    @behavioral(model="libfoc_control.so", language="rust")
}
```

## Implementation Priority

### Phase 1: Pure BHDL Behavioral (Months 1-2)
1. Mutable parameters
2. Parameter expressions with pin references
3. Time-based updates with `dt`
4. Implicit conversions

### Phase 2: Basic PLI (Months 3-4)
1. `@behavioral` decorator
2. Python bindings first
3. Shared memory for performance

### Phase 3: Advanced PLI (Months 5-6)
1. Batch mode
2. Rust/C++ bindings
3. Debugging support
4. `@cosim` for testbenches

## Why This Approach Wins

### 1. BHDL Stays Simple
- Only 6 new concepts
- No complex programming constructs
- Clear focus on board description

### 2. Unlimited Power via PLI
- Use any language
- Access all libraries
- Team collaboration

### 3. Performance Options
- Simple behavioral is native BHDL (fast)
- Batch mode for PLI (1000x overhead reduction)
- Shared memory (zero copy)

### 4. Easy Migration Path
- Start with parameter expressions
- Add time-based behavior
- Move to PLI when needed

### 5. Clear Guidelines

| Use Pure BHDL When | Use PLI When |
|--------------------|--------------|
| < 10 lines of behavioral | > 10 lines of behavioral |
| Simple math (+, -, *, /) | Complex math (FFT, matrices) |
| Basic conditionals | State machines > 3 states |
| Linear equations | Nonlinear algorithms |
| No loops needed | Iteration required |
| No external data | File I/O needed |

## Conclusion

This minimal approach gives BHDL powerful behavioral modeling capabilities while maintaining its identity as a board description language. The 6 new constructs are easy to learn, implement, and use, while the PLI provides unlimited extensibility for complex scenarios.