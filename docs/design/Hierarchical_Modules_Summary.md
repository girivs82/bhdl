# Hierarchical Modules - Complete Design Summary

## Overview

This document summarizes the complete design for hierarchical modules in BHDL, consolidating all design decisions and specifications.

## Core Features

### 1. Module Definition with Parameters
```bhdl
module ModuleName(param1: type1, param2: type2 = default) {
    pin PIN_NAME: type direction;
    attribute internal_state = value;
    
    // Can contain other module instances
    child: ChildModule(param=value) {
        PIN <- signal;  // Port mapping
    }
}
```

### 2. Module Instantiation
```bhdl
instance_name: ModuleType(param1=value1, param2=value2) {
    // Port mappings - module pins LEFT, signals RIGHT
    INPUT_PIN <- source_signal;
    OUTPUT_PIN -> dest_signal;
    BIDIR_PIN <-> bidirectional_signal;
    
    // Scoped attributes
    attribute nested.attribute = value;
}
```

### 3. Key Design Decisions

#### No Dot Notation
- Module pins always on LEFT side of connections
- Parent signals always on RIGHT side
- Arrow direction indicates data flow
- Position determines context (no ambiguity)

#### Parameters vs Attributes
- **Parameters**: Passed during instantiation in parentheses, immutable
- **Attributes**: Internal state, can be set via scoped paths

#### Port Mapping Syntax
```bhdl
// Input: module pin receives from signal
PIN <- signal;

// Output: module pin sends to signal  
PIN -> signal;

// Bidirectional
PIN <-> signal;

// From another instance
PIN <- other_instance.PIN;
```

## Analysis Features

### 1. Connectivity Validation
- Pin direction checking (no output-to-output)
- Open-drain/collector special handling
- Required pull-up detection

### 2. Electrical Compatibility
- Voltage level checking (5V/3.3V/1.8V)
- Logic family compatibility (TTL/CMOS/LVTTL)
- Current capacity validation
- Signal integrity analysis

### 3. SPICE Integration
- DC operating point verification
- Actual voltage/current checking
- Drive strength analysis
- Rise/fall time calculation

### 4. Automated Fixes
- Level shifter insertion for voltage mismatches
- Pull-up resistor addition for open-drain
- Buffer suggestion for overloaded outputs

## Pipeline Optimizations

### 1. Reference Designator Intelligence
```
Top level: R1, R2, C1, C2
Instance 1: R1_1, R1_2, C1_1
Instance 2: R2_1, R2_2, C2_1
```

### 2. Module Deduplication
- Identical module instances share synthesized definition
- Based on module signature (name + parameters)
- Reduces SPICE analysis time

### 3. Hierarchical Net Naming
```
board.power_section.buck_controller.feedback_net
```

## Complete Example

```bhdl
// Reusable parameterized module
module BuckConverter(
    vout: voltage,
    imax: current = 2A,
    fsw: frequency = 500kHz
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    pin PGOOD: digital out;
    
    // Calculate component values from parameters
    attribute inductor_value = (VIN - vout) * vout / (VIN * fsw * imax * 0.3);
    
    controller: BuckControllerIC {
        VCC <- VIN;
        FB <- feedback_point;
        SW -> switch_node;
        
        attribute switching_freq = fsw;
        attribute current_limit = imax * 1.2;
    }
    
    feedback: ResistorDivider(ratio = 0.8V / vout) {
        TOP <- VOUT;
        TAP -> feedback_point;
        BOTTOM <- GND;
    }
    
    power_stage: LC_Filter {
        IN <- switch_node;
        OUT -> VOUT;
        
        attribute L = inductor_value;
        attribute C = 100µF * (imax / 1A);  // Scale with current
    }
}

// Board using multiple instances
board PowerSupply {
    power VIN_24V = 24V @ 5A;
    
    // 12V rail
    buck_12v: BuckConverter(vout=12V, imax=3A) {
        VIN <- VIN_24V;
        VOUT -> RAIL_12V;
        EN <- enable_12v;
        PGOOD -> pgood_12v;
    }
    
    // 5V rail  
    buck_5v: BuckConverter(vout=5V, imax=5A, fsw=300kHz) {
        VIN <- VIN_24V;
        VOUT -> RAIL_5V;
        EN <- enable_5v;
        PGOOD -> pgood_5v;
        
        // Override specific component
        attribute power_stage.C = 220µF;
    }
    
    // 3.3V from 5V (cascaded)
    ldo_3v3: LDO_Regulator(dropout=0.5V) {
        VIN <- RAIL_5V;
        VOUT -> RAIL_3V3;
        EN <- pgood_5v;  // Enable when 5V is good
    }
}
```

## Benefits

1. **Reusability** - Define once, instantiate many times with different parameters
2. **Clarity** - Hierarchical organization matches system architecture  
3. **Safety** - Comprehensive electrical validation prevents errors
4. **Efficiency** - Smart deduplication and caching
5. **Flexibility** - Parameters enable configuration without modification
6. **Correctness** - SPICE verification ensures electrical validity

## Implementation Status

- [x] Design specification complete
- [x] Syntax finalized (no dots, left-right convention)
- [x] Analysis requirements defined
- [x] Implementation plan created
- [ ] Parser implementation
- [ ] AST updates
- [ ] Analyzer passes
- [ ] Synthesizer updates
- [ ] Test suite
- [ ] Documentation

## Next Steps

1. Begin parser implementation following the plan
2. Create test circuits for validation
3. Update existing examples to use modules
4. Write migration guide for users