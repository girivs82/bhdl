# BHDL Behavioral Modeling Quick Reference

## The 5 Concepts at a Glance

1. **Expression Attributes**: `attribute error = vref - FB;`
2. **Mutable Attributes**: Modified in `when` blocks
3. **External Models**: `@behavioral(model="name", language="python")`
4. **Testbench Co-sim**: `@cosim(harness="test.py")`
5. **Time Variable**: Use `dt` for timestep

## Simple Behavioral (No PLI)

### Basic Expression Attribute
```bhdl
entity Comparator {
    pin IN_P: analog in;
    pin IN_N: analog in;
    pin OUT: digital out;
    
    attribute diff = IN_P - IN_N;
    OUT = diff > 0;
}
```

### Thermal Derating
```bhdl
entity ThermalLED {
    pin TEMP: analog in;
    pin I_OUT: current out;
    
    attribute temp_c = (TEMP - 0.5V) / 10mV;
    attribute i_max = if (temp_c < 85) { 350mA }
                     else if (temp_c < 105) { 350mA * (125-temp_c)/40 }
                     else { 0mA };
    
    I_OUT = i_max;
}
```

### Time-Based Behavior
```bhdl
entity SoftStart {
    pin ENABLE: digital in;
    pin VREF: analog out;
    
    attribute vref_internal = 0V;  // Becomes mutable below
    
    when (ENABLE && vref_internal < 3.3V) {
        vref_internal += 3.3V / 10ms * dt;
    }
    
    when (!ENABLE) {
        vref_internal = 0V;  // Reset
    }
    
    VREF = vref_internal;
}
```

### Simple Controller
```bhdl
entity BuckControl {
    pin FB: analog in;
    pin PWM: digital out;
    
    attribute target = 3.3V;
    attribute error = target - FB;
    attribute duty = clamp(0.5 + error * 0.1, 0.1, 0.9);
    
    PWM = duty;  // Implicit ratio→PWM conversion
}
```

## Complex Behavioral (With PLI)

### Basic PLI Module
```bhdl
entity USBController {
    pin D_P, D_N: analog inout;
    pin VBUS: power out;
    
    @behavioral(model="usb.Controller", language="python")
}
```

### PLI with Parameters
```bhdl
entity MotorDrive(pwm_freq: frequency = 20kHz) {
    pin PHASE_A, PHASE_B, PHASE_C: current out;
    
    @behavioral(
        model="motor.FOCController",
        language="rust",
        params={frequency: pwm_freq}
    )
}
```

### Testbench with Co-simulation
```bhdl
testbench Validation for MyCircuit {
    stimulus {
        @0ms: VIN = 12V;
        @1ms: ENABLE = high;
    }
    
    @cosim(harness="validation_tests.py", mode="batch")
}
```

## Common Patterns

### Hysteresis
```bhdl
attribute threshold_high = 4.5V;
attribute threshold_low = 3.5V;
attribute state = false;  // Mutable

when (input > threshold_high) {
    state = true;
}

when (input < threshold_low) {
    state = false;
}
```

### Rate Limiting
```bhdl
attribute target = control_input;
attribute output = 0V;  // Mutable
attribute slew_rate = 1V/ms;

when (output < target) {
    output += min(slew_rate * dt, target - output);
}

when (output > target) {
    output -= min(slew_rate * dt, output - target);
}
```

### Integrator
```bhdl
attribute integral = 0.0;  // Mutable
attribute ki = 0.1;

when (enable) {
    integral += error * ki * dt;
    integral = clamp(integral, -10, 10);  // Anti-windup
}
```

### State Machine (Simple)
```bhdl
attribute state = 0;  // Mutable: 0=IDLE, 1=ACTIVE, 2=FAULT

when (state == 0 && start_signal) {
    state = 1;
}

when (state == 1 && fault_detected) {
    state = 2;
}

when (state == 2 && reset_signal) {
    state = 0;
}
```

## Python PLI Template

```python
from bhdl import BehavioralModel

class MyController(BehavioralModel):
    def __init__(self):
        super().__init__()
        self.state = "IDLE"
        
    def step(self, dt):
        # Read inputs
        vin = self.read_pin("VIN")
        enable = self.read_pin("ENABLE")
        
        # Control logic
        if enable > 0.5:
            vout = self.calculate_output(vin)
            self.write_pin("VOUT", vout)
        else:
            self.write_pin("VOUT", 0.0)
    
    def step_batch(self, dt, count):
        """Override for better performance"""
        results = []
        for _ in range(count):
            self.step(dt)
            results.append(self.capture_outputs())
        return results
```

## Type Conversions

| From | To | Example |
|------|-----|---------|
| `ratio` | `digital out` (PWM) | `PWM = duty_cycle` |
| `voltage` | `analog out` | `DAC_OUT = vref` |
| `boolean` | `digital out` | `LED = temp_ok` |
| `current` | `analog out` | `ISET = i_limit` |

## Built-in Functions

- `clamp(value, min, max)` - Limit value to range
- `min(a, b)` - Minimum of two values
- `max(a, b)` - Maximum of two values
- `abs(value)` - Absolute value

## Decision Guide

### Use Pure BHDL When:
- Logic is under 20 lines
- Only basic math needed (+, -, *, /)
- Simple conditionals (if/else)
- No complex state machines
- No iteration needed

### Use PLI When:
- Complex algorithms (PID, filters)
- State machines with 3+ states
- Need external libraries
- Matrix/vector operations
- File I/O required
- Network communication

## Common Gotchas

1. **Forgetting `dt` in time calculations**:
   ```bhdl
   // Wrong
   value += rate;
   
   // Correct
   value += rate * dt;
   ```

2. **Pin direction matters**:
   ```bhdl
   pin FB: analog in;   // Can read
   pin PWM: digital out; // Can write
   
   // This won't work:
   FB = 3.3V;  // Error: Can't write to input
   ```

3. **Attribute evaluation order**:
   ```bhdl
   // Correct: Dependencies work
   attribute a = pin1 + pin2;
   attribute b = a * 2;
   
   // Wrong: Circular dependency
   attribute x = y + 1;
   attribute y = x - 1;
   ```

4. **Mutability is inferred**:
   ```bhdl
   attribute count = 0;  // Looks immutable
   
   when (trigger) {
       count += 1;  // Now it's mutable!
   }
   ```

## Performance Tips

1. **Batch PLI calls**: Process 100-1000 timesteps at once
2. **Use shared memory**: For large waveform data
3. **Minimize pin reads**: Cache values when possible
4. **Profile first**: Don't optimize prematurely

## Debug Commands

```python
# In Python PLI models
self.log_debug(f"State: {self.state}, Error: {error}")
self.add_breakpoint("on_fault")
self.dump_waveform("debug.vcd")
```

## Next Steps

1. Start with simple expression attributes
2. Add time-based behavior with `when` blocks
3. Move complex logic to PLI when needed
4. Test thoroughly with realistic scenarios

Remember: Keep BHDL simple, put complexity in PLI!