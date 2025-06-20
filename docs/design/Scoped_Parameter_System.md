# Scoped Parameter System for BHDL

## Overview

Enable fine-grained control over deeply nested module parameters from higher levels using scoped references. This allows configuring identical module instances differently based on their role in the system.

## Motivation Example

```bhdl
// Problem: Two identical buck converters need different output voltages
board PowerSupply {
    // Traditional approach - must expose every parameter
    module BuckWithFeedback(vout: voltage) {
        buck: BuckConverter {
            // How to set feedback resistors for vout?
        }
    }
    
    // Want to do this:
    buck_3v3: BuckConverter {
        // Override deeply nested parameters
        @override feedback.r_top = 10k;
        @override feedback.r_bottom = 3.3k;
    }
    
    buck_5v: BuckConverter {
        @override feedback.r_top = 10k;
        @override feedback.r_bottom = 2.2k;
    }
}
```

## Design Approaches

### Approach 1: Scoped Parameter Paths

```bhdl
module BuckConverter {
    pin VIN: power in;
    pin VOUT: power out;
    
    controller: ControllerIC {
        VIN -> .VIN;
        .FB -> feedback.center;
    }
    
    // Feedback network as submodule
    feedback: FeedbackDivider {
        VOUT -> .IN;
        .OUT -> controller.FB;
        .GND -> GND;
        
        // These can be overridden
        @param r_top = 10k;
        @param r_bottom = 10k;
    }
}

// Usage with scoped overrides
board System {
    buck_3v3: BuckConverter {
        VIN -> .VIN;
        
        // Scoped parameter override
        @params {
            feedback.r_top = 10k;
            feedback.r_bottom = 3.3k;  // For 3.3V output
        }
    }
    
    buck_5v: BuckConverter {
        VIN -> .VIN;
        
        @params {
            feedback.r_top = 10k;
            feedback.r_bottom = 2.2k;  // For 5V output
        }
    }
}
```

### Approach 2: Parameter Propagation

```bhdl
module BuckConverter(
    target_vout: voltage = 3.3V,
    // Allow override of nested params
    feedback_r_top: resistance = auto,
    feedback_r_bottom: resistance = auto
) {
    // Calculate if not overridden
    attribute r_top_calc = feedback_r_top ?? 10k;
    attribute r_bottom_calc = feedback_r_bottom ?? (r_top_calc * 0.8V / (target_vout - 0.8V));
    
    feedback: FeedbackDivider(
        r_top = r_top_calc,
        r_bottom = r_bottom_calc
    ) {
        // connections
    }
}
```

### Approach 3: Constraint-Based Configuration

```bhdl
module BuckConverter {
    pin VOUT: power out;
    
    // Declare configurable parameters
    @configurable feedback.ratio: real;
    
    feedback: FeedbackDivider {
        @constraint ratio = r_bottom / (r_top + r_bottom);
        @constraint r_top + r_bottom <= 50k;  // Max current
    }
}

board System {
    buck_3v3: BuckConverter {
        // Specify constraint, let system solve
        @configure feedback.ratio = 0.8V / 3.3V;
    }
    
    buck_5v: BuckConverter {
        @configure feedback.ratio = 0.8V / 5.0V;
    }
}
```

## Recommended Design: Hierarchical Parameter System

### 1. Parameter Declaration with Paths

```bhdl
module PowerSupply {
    // Module-level parameters
    @param vin_nominal: voltage = 12V;
    @param efficiency_target: ratio = 0.9;
    
    // Nested module with its own params
    input_filter: EMIFilter {
        @param cutoff: frequency = 100kHz;
        @param stages: int = 2;
    }
    
    // Multiple identical converters
    conv_3v3: BuckConverter {
        @param vout: voltage = 3.3V;
        @param imax: current = 5A;
        
        // Nested params
        @param controller.fsw: frequency = 500kHz;
        @param feedback.r_top: resistance = 10k;
        @param feedback.r_bottom: resistance = 3.3k;
    }
    
    conv_1v8: BuckConverter {
        @param vout: voltage = 1.8V;
        @param imax: current = 3A;
        @param controller.fsw: frequency = 1MHz;  // Higher freq for lower voltage
        @param feedback.r_top: resistance = 10k;
        @param feedback.r_bottom: resistance = 8.2k;
    }
}
```

### 2. Scoped Override Syntax

```bhdl
board MainBoard {
    psu: PowerSupply {
        // Override top-level param
        @param vin_nominal = 24V;
        
        // Override nested params with paths
        @param conv_3v3.imax = 8A;  // Need more current
        @param conv_3v3.controller.fsw = 300kHz;  // Lower freq for higher current
        
        // Deep override
        @param conv_1v8.feedback.r_bottom = 10k;  // Adjust output voltage
    }
}
```

### 3. Parameter Inheritance and Computation

```bhdl
module BuckConverter(base_fsw: frequency = 500kHz) {
    // Parameters can reference parent params
    @param controller.fsw: frequency = base_fsw;
    @param controller.deadtime: time = 100ns / (controller.fsw / 500kHz);
    
    controller: PWMController {
        // Inherits scoped parameters
        @param fsw;  // Links to controller.fsw above
        @param deadtime;
    }
}
```

### 4. Array Parameter Scoping

```bhdl
module MultiPhaseConverter(phases: int = 4) {
    @param base_current: current = 25A;
    
    generate for i in 0..phases {
        phase[i]: PhaseController {
            // Array element parameters
            @param imax: current = base_current / phases;
            @param phase_shift: angle = 360deg * i / phases;
            
            // Can be individually overridden
            @param current_limit.threshold = imax * 1.2;
        }
    }
}

board System {
    converter: MultiPhaseConverter {
        // Override specific array elements
        @param phase[0].imax = 8A;  // First phase handles more
        @param phase[1].imax = 6A;
        @param phase[2].imax = 6A;
        @param phase[3].imax = 5A;
        
        // Or override all
        @param phase[*].current_limit.threshold = 10A;
    }
}
```

## Implementation Design

### 1. Parameter Path Resolution

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ParameterPath {
    segments: Vec<PathSegment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    Module(String),
    Array(String, usize),
    ArrayAll(String),  // [*] syntax
    Parameter(String),
}

impl ParameterPath {
    pub fn parse(s: &str) -> Result<Self, ParseError> {
        // Parse "conv_3v3.controller.fsw"
        // Parse "phase[0].imax"
        // Parse "phase[*].current_limit"
    }
}
```

### 2. Parameter Registry

```rust
pub struct ParameterRegistry {
    // Full path -> parameter definition
    parameters: HashMap<ParameterPath, Parameter>,
    // Override values by instance path
    overrides: HashMap<InstancePath, HashMap<ParameterPath, Value>>,
}

impl ParameterRegistry {
    pub fn get_value(
        &self,
        instance_path: &InstancePath,
        param_path: &ParameterPath,
    ) -> Option<Value> {
        // 1. Check for instance-specific override
        if let Some(overrides) = self.overrides.get(instance_path) {
            if let Some(value) = overrides.get(param_path) {
                return Some(value.clone());
            }
        }
        
        // 2. Check for wildcard overrides (phase[*])
        if let Some(value) = self.check_wildcard_override(instance_path, param_path) {
            return Some(value);
        }
        
        // 3. Use default from parameter definition
        self.parameters.get(param_path)
            .and_then(|p| p.default_value.clone())
    }
}
```

### 3. Module Instantiation with Overrides

```rust
impl ModuleInstantiator {
    pub fn instantiate(
        &mut self,
        module: &Module,
        instance_path: &InstancePath,
        overrides: &HashMap<ParameterPath, Value>,
    ) -> Result<InstantiatedModule> {
        // Create parameter context with overrides
        let mut param_context = ParameterContext::new();
        
        // Apply module defaults
        for (path, param) in module.parameters() {
            param_context.set(path, param.default_value);
        }
        
        // Apply instance overrides
        for (path, value) in overrides {
            param_context.override_param(path, value);
        }
        
        // Instantiate with context
        self.instantiate_with_context(module, param_context)
    }
}
```

## Advanced Features

### 1. Parameter Constraints with Scopes

```bhdl
module RegulatedSupply {
    conv1: BuckConverter { @param vout = 5V; }
    conv2: BuckConverter { @param vout = 3.3V; }
    
    // Cross-module constraint
    @constraint conv2.vin < conv1.vout;  // conv2 fed from conv1
    @constraint conv1.imax >= conv2.imax * 1.2;  // Margin
}
```

### 2. Computed Overrides

```bhdl
board System {
    attribute system_voltage = 3.3V;
    
    psu: PowerSupply {
        // Compute nested param from board-level attribute
        @param conv_digital.vout = system_voltage;
        @param conv_analog.vout = system_voltage;
        @param conv_analog.filter.cutoff = 10Hz;  // Analog needs filtering
    }
}
```

### 3. Type-Safe Parameter Paths

```bhdl
module TypedParams {
    // Declare parameter types
    @param<voltage> vref = 1.2V;
    @param<resistance> r_series = 100;
    
    // Type-checked at compile time
    submodule: Child {
        @param vref = vref;  // OK: voltage -> voltage
        // @param r_value = vref;  // ERROR: voltage -> resistance
    }
}
```

### 4. Parameter Templates

```bhdl
// Define parameter sets
@param_template HighPower {
    imax = 10A;
    thermal.max_temp = 125C;
    protection.overcurrent = true;
}

@param_template LowNoise {
    filter.stages = 3;
    layout.separation = 5mm;
    shield.required = true;
}

module ConfigurableSupply {
    // Apply template
    @apply_params HighPower;
    @apply_params LowNoise;
    
    // Override specific values
    @param imax = 8A;  // Override template
}
```

## Benefits

1. **Fine Control**: Configure any parameter at any depth
2. **Type Safety**: Paths are checked at compile time
3. **Reusability**: Same module, different configurations
4. **Clarity**: Override paths show exactly what's changing
5. **Maintainability**: Changes in one place affect all instances

## Example: Complex Power System

```bhdl
board ServerPower {
    input_voltage: power = 48V;
    
    // Main power supply with many rails
    main_psu: PowerSupplyUnit {
        @param input.filter.stages = 3;  // Extra filtering
        
        // CPU power (high current, tight regulation)
        @param vcore.phases = 8;
        @param vcore.vout = 1.0V;
        @param vcore.imax = 150A;
        @param vcore.regulation = 0.5%;  // Tight
        @param vcore.phase[*].switching_freq = 500kHz;
        
        // Memory power (low noise)
        @param vmem.vout = 1.2V;
        @param vmem.imax = 30A;
        @param vmem.filter.cutoff = 1kHz;
        @param vmem.layout.trace_width = 5mm;
        
        // Standby power (high efficiency)
        @param vstby.vout = 3.3V;
        @param vstby.imax = 2A;
        @param vstby.mode = "burst";  // For light load efficiency
    }
    
    // Redundant supply with different config
    backup_psu: PowerSupplyUnit {
        @param input.filter.stages = 2;  // Less filtering OK
        @param vcore.phases = 4;  // Less phases OK for backup
        @param vcore.phase[*].switching_freq = 300kHz;  // Lower freq
        
        // Same voltages but different implementation
        @param vmem.topology = "ldo";  // Simple for backup
        @param vstby.always_on = true;  // Different behavior
    }
}
```

This scoped parameter system makes it possible to create truly flexible, reusable modules while maintaining precise control over every aspect of the design!