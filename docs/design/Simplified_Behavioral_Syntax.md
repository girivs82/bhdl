# Simplified BHDL Behavioral Syntax

## Goal: Minimal Keywords, Maximum Power

Based on the PLI discussion, here's a simplified approach that keeps BHDL clean while enabling behavioral modeling.

## Core Principle: Reuse Existing Constructs

### 1. Simple Behavioral Equations (No New Keywords)

```bhdl
// Instead of complex "behavioral" blocks, use module parameters
module BuckController(vout_target: voltage = 3.3V) {
    pin FB: analog in;
    pin PWM: digital out;
    
    // Simple behavioral as parameter expressions
    param error = vout_target - FB;
    param duty_cycle = clamp(0.5 + error * 10, 0.1, 0.9);
    
    // Connect behavioral to physical
    PWM = duty_cycle;  // Automatic D/A conversion
}
```

### 2. Time-Based Behavior with "when" (Already Exists)

```bhdl
module SoftStartController {
    pin ENABLE: digital in;
    pin VREF: analog out;
    
    // Use existing "when" syntax
    param vref_internal: voltage = 0V;
    
    // Soft start ramp
    when (ENABLE == high) {
        vref_internal = min(vref_internal + 3.3V/10ms * dt, 3.3V);
    }
    
    VREF = vref_internal;
}
```

### 3. External Models for Complex Behavior

```bhdl
// For complex state machines, use external model
module USBPDController {
    pin CC1, CC2: analog inout;
    pin VBUS_EN: digital out;
    
    // Link to external implementation
    @external("usb_pd_controller")
    @interface("bhdl.pli.v1")
}
```

### 4. Testbench Stays Focused

```bhdl
testbench BasicValidation for BuckConverter {
    // Simple time-based stimulus
    stimulus {
        @0ms: VIN = 12V;
        @1ms: LOAD = 0.5A;
        @2ms: LOAD = 1.5A;
    }
    
    // Built-in measurements
    measure {
        RIPPLE: peak_to_peak(VOUT);
        EFFICIENCY: power(out) / power(in) * 100;
    }
    
    // Simple assertions
    assert {
        VOUT in range(3.2V, 3.4V) after 1ms;
        RIPPLE < 50mV;
    }
}
```

### 5. Co-simulation for Advanced Scenarios

```bhdl
testbench ClosedLoopControl for MotorDriver {
    // Link external control algorithm
    cosim {
        controller: "python:motor_control.py";
        sample_rate: 20kHz;
    }
    
    // BHDL handles analog/power
    // Python handles control algorithm
}
```

## Minimal Additions to BHDL

### Only 3 New Concepts:

1. **Parameter expressions with time**
   ```bhdl
   param duty = clamp(error * kp, 0, 1);
   when (condition) { param = expression; }
   ```

2. **External model declaration**
   ```bhdl
   @external("model_name")
   ```

3. **Cosim in testbench**
   ```bhdl
   cosim { model: "language:file"; }
   ```

## Examples Without Keyword Explosion

### Buck with Soft Start (No Behavioral Keywords)
```bhdl
module BuckWithSoftStart {
    pin VIN: power in;
    pin VOUT: power out @ 3.3V;
    pin ENABLE: digital in;
    
    // Internal parameters act as state
    param vref: voltage = 0V;
    param soft_start_rate = 3.3V / 10ms;
    
    // Time-based update
    when (ENABLE && vref < 3.3V) {
        vref = vref + soft_start_rate * dt;
    }
    
    // Use vref in regulation
    internal_buck: BuckController(vout_target = vref);
}
```

### LED with Thermal Derating (Simple Equations)
```bhdl
module ThermallyDeratedLED {
    pin TEMP_SENSE: analog in;
    pin LED_ANODE: current out;
    
    // Temperature from NTC
    param temp_c = ntc_to_temp(TEMP_SENSE, R25=10k, B=3950);
    
    // Derating curve as expression
    param current_limit = if (temp_c < 85) {
        350mA
    } else if (temp_c < 105) {
        350mA * (125 - temp_c) / 40
    } else {
        0mA  // Shutdown
    };
    
    LED_ANODE = current_limit;
}
```

### Complex USB-PD via External
```bhdl
module USBC_PowerDelivery {
    // Simple BHDL interface
    pin CC1, CC2: analog inout;
    pin VBUS: power out;
    
    // Complex protocol in Python/Rust/C++
    @external("usb_pd_protocol")
    @ports({
        "cc1": CC1,
        "cc2": CC2,
        "vbus_en": VBUS.enable,
        "vbus_voltage": VBUS.voltage_setting
    })
}
```

## Benefits of This Approach

1. **BHDL stays simple** - Only 3 new concepts
2. **No keyword explosion** - Reuse existing syntax
3. **Clear separation** - Electrical in BHDL, algorithms external
4. **Flexible** - Use any language for complex behavior
5. **Performant** - Simple stuff inline, complex stuff optimized

## Migration Path

### Start Simple
```bhdl
// Just equations
param duty = 0.5 + error * 0.1;
```

### Add Time Behavior
```bhdl
when (startup) {
    vref = min(vref + rate * dt, target);
}
```

### Go External When Needed
```bhdl
@external("complex_controller")
```

## Comparison

### Original Proposal (Too Many Keywords)
```bhdl
behavioral module Controller {
    state IDLE {
        when (start) -> ACTIVE;
    }
    state ACTIVE {
        continuously {
            output = calculate();
        }
    }
    behavior {
        // More keywords...
    }
}
```

### New Proposal (Minimal)
```bhdl
module Controller {
    param state = "IDLE";
    
    when (start && state == "IDLE") {
        state = "ACTIVE";
    }
    
    param output = (state == "ACTIVE") ? calculate() : 0;
}

// Or go external for truly complex:
@external("controller_fsm")
```

This keeps BHDL focused on board description while enabling all the behavioral capabilities through either simple expressions or external models.