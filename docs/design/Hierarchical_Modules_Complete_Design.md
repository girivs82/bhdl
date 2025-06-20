# BHDL Hierarchical Modules - Complete Design Specification

## Overview

This document specifies the complete design for hierarchical modules in BHDL, including:
- Module instantiation within modules
- Parameter passing (configuration)
- Port mapping (connections)
- Attribute scoping and inheritance

## Core Concepts

### 1. Parameters vs Attributes

**Parameters**: Configuration values passed during instantiation
- Defined in module header: `module Name(param: type = default)`
- Set during instantiation: `inst: Module(param = value)`
- Cross scope boundaries (parent → child)
- Immutable within the module

**Attributes**: Module properties and internal state
- Defined with `attribute` keyword
- Exist within module scope
- Can be static metadata or computed values
- Can be passed as parameter values

### 2. Hierarchical Structure

```bhdl
board TopLevel {
    module Container {
        module Nested {
            component Instance
        }
    }
}
```

## Syntax Specification

### Module Definition with Parameters

```bhdl
module ModuleName(
    param1: type1,
    param2: type2 = default_value,
    param3: type3
) {
    // Module body
}
```

### Module Instantiation

```bhdl
instance_name: ModuleType(param1=value1, param2=value2) {
    // Port mappings - module pins on LEFT, parent signals on RIGHT
    input_pin <- source_signal;      // Input mapping
    output_pin -> dest_signal;       // Output mapping
    inout_pin <-> bidirectional_signal;  // Bidirectional mapping
}
```

### Complete Example

```bhdl
// Module definition with parameters
module VoltageRegulator(
    vout_target: voltage,
    current_limit: current = 2A,
    switching_freq: frequency = 500kHz
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    pin PGOOD: digital out;
    
    // Internal attributes (not parameters!)
    attribute description = "Configurable buck regulator";
    attribute topology = (vout_target > VIN) ? "boost" : "buck";
    attribute inductor_value = calculate_inductor(vout_target, switching_freq);
    
    // Use parameters in implementation
    controller: ControllerIC {
        VCC <- VIN;
        FB <- feedback_point;
        SW -> switch_node;
        
        // Controller configuration from parameters
        attribute fsw = switching_freq;
        attribute ilim = current_limit;
    }
    
    // Feedback network using parameter
    feedback: FeedbackDivider(
        ratio = 0.8V / vout_target
    ) {
        TOP <- VOUT;
        TAP -> feedback_point;
        BOTTOM <- GND;
    }

}

// Parent module passing parameters
module PowerSupply {
    pin INPUT: power in;
    pin OUTPUT_5V: power out;
    pin OUTPUT_3V3: power out;
    
    // Module-level attributes
    attribute efficiency_mode = "high";
    attribute board_temp = 25C;
    
    // First regulator - using attributes as parameter values
    reg_5v: VoltageRegulator(
        vout_target = 5V,                    // Direct value
        current_limit = 10A,                 // Direct value
        switching_freq = efficiency_mode == "high" ? 300kHz : 500kHz
    ) {
        VIN <- INPUT;
        VOUT -> OUTPUT_5V;
        EN <- enable_5v;
        PGOOD -> pgood_5v;
    }
    
    // Second regulator - cascaded
    reg_3v3: VoltageRegulator(
        vout_target = 3.3V,
        current_limit = 5A
        // switching_freq uses default 500kHz
    ) {
        VIN <- OUTPUT_5V;    // Cascaded from first regulator
        VOUT -> OUTPUT_3V3;
        EN <- enable_3v3;
        PGOOD -> pgood_3v3;
    }
}
```

## Parameter Flow

### Direct Pass-Through

```bhdl
module Parent(base_voltage: voltage) {
    child: Child(
        operating_voltage = base_voltage  // Direct pass
    ) {
        // port mappings
    }
}
```

### Transformed Parameters

```bhdl
module Parent(
    input_voltage: voltage,
    num_outputs: int
) {
    // Mathematical transformation
    child1: Regulator(
        vout = input_voltage / 2,
        imax = 10A / num_outputs
    ) { 
        VIN <- power_rail;
        VOUT -> output1;
    }
    
    // Conditional transformation
    child2: Regulator(
        vout = 3.3V,
        switching_freq = (input_voltage > 24V) ? 200kHz : 500kHz
    ) { 
        VIN <- power_rail;
        VOUT -> output2;
    }
}
```

### Multi-Level Parameter Flow

```bhdl
module Level1(system_freq: frequency) {
    // Pass to Level2
    sub: Level2(
        base_freq = system_freq
    ) { }
}

module Level2(base_freq: frequency) {
    // Pass to Level3 with modification
    subsub: Level3(
        operating_freq = base_freq / 2
    ) { }
}

module Level3(operating_freq: frequency) {
    // Use the parameter
    attribute period = 1 / operating_freq;
}
```

## Port Mapping Specification

### Basic Port Mapping

Port mapping connects signals between module boundaries using consistent syntax:
- Module pins always on LEFT side
- Parent signals/pins on RIGHT side
- Arrow shows data flow direction

```bhdl
module Container {
    signal internal_net;
    
    child: ChildModule {
        // Input mapping
        input_pin <- internal_net;
        
        // Output mapping
        output_pin -> internal_net;
        
        // Bidirectional mapping
        bidir_pin <-> internal_net;
    }
}
```

### Pin Reference Syntax

**No dot notation needed - position determines context:**
```bhdl
PIN <- signal;    // Module pin receives from parent signal
PIN -> signal;    // Module pin sends to parent signal
PIN <-> signal;   // Bidirectional connection
```

### Array Pin Mapping

```bhdl
module ArrayExample {
    pin DATA_IN[8]: signal in;
    pin DATA_OUT[8]: signal out;
    
    processor: DataProcessor {
        INPUT[0..7] <- DATA_IN[0..7];
        OUTPUT[0..7] -> DATA_OUT[0..7];
    }
}
```

### Instance-to-Instance Connections

```bhdl
module Pipeline {
    stage1: ProcessorA {
        IN <- input;
        OUT -> intermediate;  // To net
    }
    
    stage2: ProcessorB {
        IN <- intermediate;   // From net
        OUT -> output;
    }
    
    // Direct connection requires qualified name
    // stage2: ProcessorB {
    //     IN <- stage1.OUT;
    // }
}
```

## Scoped Attributes

### Basic Scoped Attributes

```bhdl
module Parent {
    child: Child {
        // Port mappings
        input <- IN;
        
        // Scoped attribute settings
        attribute feedback.r_top = 10k;
        attribute feedback.r_bottom = 3.3k;
        attribute controller.mode = "pwm";
    }
}
```

### Array Element Attributes

```bhdl
module MultiChannel {
    drivers: DriverBank(channels=8) {
        // Port mappings for array
        IN[0..7] <- input_signals[0..7];
        OUT[0..7] -> output_signals[0..7];
        
        // Set individual elements
        attribute driver[0].current = 250mA;
        attribute driver[1].current = 250mA;
        
        // Set range
        attribute driver[2..5].current = 350mA;
        
        // Set all
        attribute driver[*].frequency = 1kHz;
    }
}
```

## Complete Integration Example

```bhdl
module PowerManagementSystem(
    input_voltage: voltage = 24V,
    low_power_mode: bool = false
) {
    pin VIN: power in;
    pin VOUT_5V: power out;
    pin VOUT_3V3: power out;
    pin VOUT_1V8: power out;
    
    // System attributes
    attribute description = "Multi-rail power system";
    attribute efficiency_target = low_power_mode ? 0.85 : 0.92;
    
    // Input protection
    protection: InputProtection(
        max_voltage = input_voltage * 1.2,
        clamp_voltage = input_voltage * 1.5
    ) {
        INPUT <- VIN;
        OUTPUT -> protected_rail;
    }
    
    // Main 5V rail
    buck_5v: BuckConverter(
        vin = input_voltage,
        vout = 5V,
        imax = 10A,
        fsw = low_power_mode ? 200kHz : 500kHz
    ) {
        VIN <- protected_rail;
        VOUT -> VOUT_5V;
        EN <- enable_5v;
        
        // Configure internals
        attribute controller.compensation = "type3";
        attribute power_stage.mosfet_count = 2;
    }
    
    // 3.3V from 5V
    ldo_3v3: LDO_Regulator(
        dropout = 0.5V,
        current_max = 3A
    ) {
        VIN <- VOUT_5V;
        VOUT -> VOUT_3V3;
        
        attribute thermal.max_temp = 125C;
    }
    
    // 1.8V from 3.3V
    ldo_1v8: LDO_Regulator(
        dropout = 0.3V,
        current_max = 1A
    ) {
        VIN <- VOUT_3V3;
        VOUT -> VOUT_1V8;
        
        attribute thermal.max_temp = 105C;
    }
}
```

## Implementation Priorities

1. **Basic Instantiation**: Module-in-module with port mapping
2. **Parameter System**: Module parameters with defaults
3. **Parameter Flow**: Passing parameters through hierarchy
4. **Scoped Attributes**: Setting nested module attributes
5. **Arrays and Generation**: Parameterized module arrays

## Design Decisions Summary

1. **No `param` keyword** - Parameters are implicit in module header
2. **Parameters vs Attributes** - Clear distinction by context
3. **Unified instantiation syntax** - Parameters in parens, ports in braces
4. **Attribute keyword for scoping** - Clear nested configuration
5. **Parameter flow** - Can be passed down and transformed
6. **No dot notation** - Module pins always on left, parent signals on right
7. **Consistent arrow direction** - Arrows show actual data flow