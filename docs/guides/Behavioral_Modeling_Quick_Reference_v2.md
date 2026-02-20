# BHDL Behavioral Modeling Quick Reference v2

## The 5 Concepts

1. **Attributes**: Configuration and properties
2. **Signals**: All dynamic behavior (state & expressions)
3. **@behavioral**: External model decorator
4. **@cosim**: Testbench co-simulation
5. **dt**: Built-in timestep variable

## Key Rules

### Attributes
- Configuration/metadata only
- Can reference other attributes ONLY
- Cannot reference signals or pins
- Can be modified based on attribute logic

### Signals  
- Handle ALL dynamic behavior
- Can reference attributes, signals, and pins
- Can be modified in `when` blocks (state)
- Can be expressions (continuously evaluated)
- Can drive output pins

## Common Patterns

### Basic Comparator
```bhdl
entity Comparator {
    pin IN_P, IN_N: analog in;
    pin OUT: digital out;
    
    signal diff = IN_P - IN_N;
    OUT = diff > 0;
}
```

### Thermal Derating
```bhdl
entity ThermalLED {
    pin TEMP: analog in;
    pin I_OUT: current out;
    
    attribute i_max_nom = 350mA;
    attribute t_threshold = 85;
    
    signal temp_c = (TEMP - 0.5V) / 10mV;
    signal i_max = if (temp_c < t_threshold) { i_max_nom }
                   else if (temp_c < 105) { i_max_nom * (105-temp_c)/20 }
                   else { 0mA };
    
    I_OUT = i_max;
}
```

### Soft-Start
```bhdl
entity SoftStart {
    pin ENABLE: digital in;
    pin VREF: analog out;
    
    attribute target = 3.3V;
    attribute ramp_time = 10ms;
    
    signal vref_internal = 0V;
    
    when (ENABLE && vref_internal < target) {
        vref_internal += target / ramp_time * dt;
    }
    
    when (!ENABLE) {
        vref_internal = 0V;
    }
    
    VREF = vref_internal;
}
```

### Power Sequencer
```bhdl
entity Sequencer {
    pin ENABLE: digital in;
    pin EN0, EN1, EN2: digital out;
    
    attribute delay = 1ms;
    
    signal timer = 0ms;
    signal en0_state = low;
    signal en1_state = low; 
    signal en2_state = low;
    
    when (ENABLE) {
        timer += dt;
        
        when (timer > 0ms) { en0_state = high; }
        when (timer > delay) { en1_state = high; }
        when (timer > 2*delay) { en2_state = high; }
    }
    
    when (!ENABLE) {
        timer = 0ms;
        en0_state = low;
        en1_state = low;
        en2_state = low;
    }
    
    EN0 = en0_state;
    EN1 = en1_state;
    EN2 = en2_state;
}
```

### PI Controller
```bhdl
entity PIController {
    pin FB: analog in;
    pin PWM: digital out;
    
    attribute vref = 3.3V;
    attribute kp = 0.1;
    attribute ki = 0.01;
    
    signal error = vref - FB;
    signal integrator = 0.0;
    signal output = error * kp + integrator * ki;
    
    when (true) {  // Always
        integrator += error * dt;
        integrator = clamp(integrator, -10, 10);
    }
    
    PWM = clamp(output, 0, 1);
}
```

### State Machine
```bhdl
entity SimpleStateMachine {
    pin START, STOP, FAULT: digital in;
    pin RUNNING, ERROR: digital out;
    
    signal state = 0;  // 0=IDLE, 1=RUNNING, 2=ERROR
    signal running_out = false;
    signal error_out = false;
    
    // State transitions
    when (state == 0 && START) { state = 1; }
    when (state == 1 && STOP) { state = 0; }
    when (state == 1 && FAULT) { state = 2; }
    when (state == 2 && START) { state = 0; }
    
    // Output logic
    when (state == 1) {
        running_out = true;
        error_out = false;
    }
    
    when (state == 2) {
        running_out = false;
        error_out = true;
    }
    
    when (state == 0) {
        running_out = false;
        error_out = false;
    }
    
    RUNNING = running_out;
    ERROR = error_out;
}
```

## External Models (PLI)

### Basic External Model
```bhdl
entity ComplexController {
    pin VIN: power in;
    pin VOUT: power out;
    
    @behavioral(model="controllers.BuckController", language="python")
}
```

### With Parameters
```bhdl
entity MotorController(pwm_freq: frequency = 20kHz) {
    pin PHASE_A, PHASE_B, PHASE_C: current out;
    
    attribute max_current = 10A;
    
    @behavioral(
        model="motor.FOCController",
        language="rust",
        params={frequency: pwm_freq, i_limit: max_current}
    )
}
```

## Python PLI Template

```python
from bhdl import BehavioralModel

class MyController(BehavioralModel):
    def __init__(self, params):
        super().__init__()
        self.i_limit = params.get('i_limit', 10.0)
        
    def step(self, dt):
        # Read inputs
        vin = self.read_pin("VIN")
        enable = self.read_pin("ENABLE") > 0.5
        
        if enable:
            # Your control algorithm
            vout = self.calculate_output(vin)
            self.write_pin("VOUT", vout)
        else:
            self.write_pin("VOUT", 0.0)
```

## Decision Guide

### Use BHDL Behavioral When:
- Simple math expressions
- Basic state machines (<3 states)
- Simple time-based behavior
- Thermal derating/protection
- Power sequencing

### Use PLI When:
- Complex algorithms (PID, filters)
- Communication protocols
- Matrix/vector math
- Need external libraries
- >50 lines of behavioral code

## Common Mistakes

### ❌ Trying to reference signals in attributes
```bhdl
attribute error = vref - FB;  // WRONG! FB is a pin
```

### ✅ Use signals for pin references
```bhdl
signal error = vref - FB;  // Correct
```

### ❌ Forgetting dt in time calculations
```bhdl
when (ramping) {
    voltage += 0.1V;  // Wrong - how fast?
}
```

### ✅ Always use dt for time-based changes
```bhdl
when (ramping) {
    voltage += 0.1V/ms * dt;  // Correct
}
```

### ❌ Modifying expression signals
```bhdl
signal error = vref - FB;
when (condition) {
    error = 0;  // Error! Can't modify expression
}
```

### ✅ Use separate state signal
```bhdl
signal error_calc = vref - FB;
signal error = 0.0;

when (condition) {
    error = error_calc;  // OK - error is state
}
```

## Tips

1. **Start simple**: Attributes → Expression signals → State signals → PLI
2. **Name clearly**: `_state` suffix for state signals helps readability
3. **Comment when blocks**: Explain the condition's purpose
4. **Test edge cases**: Power-up, enable/disable, fault conditions
5. **Use PLI early**: Don't force complex logic into BHDL