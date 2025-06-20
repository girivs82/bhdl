# Minimal BHDL Constructs for Behavioral Modeling with PLI

## Goal: Minimal Syntax, Maximum Power, Optimized PLI

### 1. Simple Parameter-Based Behavioral (No PLI Needed)

```bhdl
module SimpleBuckController {
    pin FB: analog in;
    pin PWM: digital out;
    
    // Static behavioral - just expressions
    param vref: voltage = 3.3V;
    param error: voltage = vref - FB;
    param duty: ratio = clamp(0.5 + error * 0.1, 0.1, 0.9);
    
    // Direct assignment (D/A conversion implicit)
    PWM = duty;
}
```

**New constructs needed:**
- Parameter expressions that reference pins
- Implicit type conversions (ratio → PWM)

### 2. Time-Dependent Behavioral (Still No PLI)

```bhdl
module SoftStartController {
    pin ENABLE: digital in;
    pin VREF_OUT: analog out;
    
    // State that changes over time
    param vref_internal: voltage = 0V;
    
    // Time-based behavioral using existing 'when'
    when (ENABLE == high && vref_internal < 3.3V) {
        vref_internal += 3.3V / 10ms * dt;  // dt is timestep
    }
    
    VREF_OUT = vref_internal;
}
```

**New constructs needed:**
- Mutable parameters with `+=`, `-=`, etc.
- Built-in `dt` (simulation timestep)
- Parameters persist across time

### 3. External Model Declaration (PLI)

```bhdl
module ComplexBuckController {
    // Pin interface - this is all BHDL needs to know
    pin VIN: power in;
    pin VOUT: power out;
    pin ENABLE: digital in;
    pin PGOOD: digital out;
    
    // Link to external model - minimal syntax
    @behavioral(model="buck_controller", language="python")
}
```

**New constructs needed:**
- Single `@behavioral` decorator with named arguments
- That's it!

### 4. Enhanced External Model (Optional Parameters)

```bhdl
module ParametricBuckController(
    vout_target: voltage = 3.3V,
    fsw: frequency = 500kHz
) {
    pin VIN: power in;
    pin VOUT: power out;
    
    // Pass parameters to external model
    @behavioral(
        model="buck_controller.BuckModel",
        language="python",
        params={vout: vout_target, fsw: fsw}
    )
}
```

**New constructs needed:**
- `params` argument in @behavioral

### 5. Testbench PLI Integration

```bhdl
testbench BuckValidation for BuckController {
    // Standard BHDL stimulus
    stimulus {
        @0ms: VIN = 12V;
        @1ms: ENABLE = high;
    }
    
    // Link external test code - minimal syntax
    @cosim(harness="buck_tests.py", mode="lockstep")
    
    // Standard BHDL measurements still work
    measure {
        VOUT_AVG: mean(VOUT);
    }
}
```

**New constructs needed:**
- Single `@cosim` decorator for testbenches

## Complete Minimal Set of New Constructs

### 1. For Simple Behavioral (No PLI):
- **Mutable parameters**: Allow `param` to be modified with `+=`, `-=`, `=`
- **Pin references in params**: `param error = vref - FB`
- **Built-in `dt`**: Simulation timestep available globally
- **Implicit conversions**: `ratio` → `digital out` PWM

### 2. For PLI Integration:
- **`@behavioral` decorator**: Links module to external model
- **`@cosim` decorator**: Links testbench to external harness

That's it! Just 6 concepts total.

## PLI Design to Minimize Cons

### 1. Performance Optimization

```python
# Python side - batch operations
class BuckModel(bhdl.BehavioralModel):
    def configure(self):
        # Declare batch size for efficiency
        self.batch_size = 1000  # Process 1000 timesteps at once
        
    def step_batch(self, dt, count):
        # Process multiple timesteps in one call
        # Reduces IPC overhead by 1000x
        states = np.zeros((count, 4))  # Pre-allocate
        for i in range(count):
            states[i] = self.calculate_step(dt)
        return states
```

### 2. Debugging Support

```bhdl
module DebugableBuck {
    @behavioral(
        model="buck_controller",
        debug_port=12345,  // Enable debug connection
        breakpoints=["on_fault", "on_transition"]
    )
}
```

### 3. Zero-Copy Data Transfer

```python
# Use shared memory for waveform data
class BuckModel(bhdl.BehavioralModel):
    def setup_shared_memory(self):
        # Allocate shared memory buffer
        self.shm = SharedMemory(create=True, size=10*1024*1024)
        self.waveforms = np.ndarray((1000000, 6), 
                                   dtype=np.float64, 
                                   buffer=self.shm.buf)
```

### 4. Simplified Deployment

```toml
# bhdl_project.toml
[behavioral_models]
buck_controller = { type = "python", path = "models/buck.py" }

[deployment]
bundle_runtime = true  # Include Python runtime
package_dependencies = ["numpy", "scipy"]
```

### 5. Hybrid Execution Modes

```bhdl
testbench FlexibleTest for BuckController {
    @cosim(
        harness="buck_tests.py",
        mode="async",  // Run ahead for performance
        sync_points=["@1ms", "@5ms", "@10ms"]  // Sync at specific times
    )
}
```

## Examples Using Minimal Syntax

### Example 1: Thermal Derating (No PLI)

```bhdl
module ThermalLED {
    pin TEMP: analog in;
    pin LED_DRIVE: current out;
    
    // Simple behavioral
    param temp_c = (TEMP - 0.5V) / 10mV;  // TMP36 sensor
    param i_limit = if (temp_c < 85) { 350mA }
                   else if (temp_c < 105) { 350mA * (125 - temp_c) / 40 }
                   else { 0mA };
    
    LED_DRIVE = i_limit;
}
```

### Example 2: Complex USB PD (PLI)

```bhdl
module USBPD_Controller {
    pin CC1, CC2: analog inout;
    pin VBUS_EN: digital out;
    pin VBUS_PROG: analog out;
    
    // Complex protocol in external model
    @behavioral(model="usb_pd.PDController", language="python")
}
```

### Example 3: Motor Control (PLI)

```bhdl
module FOC_Controller {
    pin PHASE_A, PHASE_B, PHASE_C: analog in;  // Current sense
    pin PWM_A, PWM_B, PWM_C: digital out;      // Gate drives
    pin ENCODER: digital in;
    
    @behavioral(
        model="motor_control.FieldOrientedControl",
        language="rust",  // For performance
        params={pwm_freq: 20kHz}
    )
}
```

## Benefits of This Minimal Approach

1. **BHDL stays simple**: Only 6 new concepts total
2. **Clear separation**: Electrical in BHDL, algorithms in code
3. **Performance options**: Batching, shared memory, async modes
4. **Easy debugging**: Debug ports, breakpoints
5. **Flexible deployment**: Bundle runtime or use system
6. **Language choice**: Python for prototyping, Rust for production

## Migration Path

1. **Start with parameter expressions**
   ```bhdl
   param duty = 0.5 + error * 0.1;
   ```

2. **Add time-based behavior if needed**
   ```bhdl
   when (condition) { param += rate * dt; }
   ```

3. **Move to PLI when complexity warrants**
   ```bhdl
   @behavioral(model="complex_controller")
   ```

This minimal approach gives us maximum flexibility with minimum language complexity!