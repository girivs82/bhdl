# Scoped Attributes for Hierarchical Parameter Control

## Overview

Use the existing `attribute` keyword with scoped paths to configure nested entity parameters, avoiding the need for new syntax.

## Design: Extending Attributes with Scopes

### Basic Scoped Attributes

```bhdl
entity AdjustableRegulator {
    pin VIN: power in;
    pin VOUT: power out;
    
    // Module-level attributes
    attribute description = "Adjustable voltage regulator";
    attribute default_vout = 3.3V;
    
    // Nested module
    feedback: VoltageDivider {
        VOUT -> .TOP;
        .TAP -> feedback_point;
        
        // These can be overridden from parent
        attribute r_top = 10k;
        attribute r_bottom = 10k;
    }
}

// Usage: Override nested attributes
board System {
    reg_5v: AdjustableRegulator {
        VIN -> .VIN;
        
        // Override nested entity attributes using dot notation
        attribute feedback.r_top = 10k;
        attribute feedback.r_bottom = 1.91k;  // For 5V output
    }
    
    reg_3v3: AdjustableRegulator {
        VIN -> .VIN;
        
        attribute feedback.r_top = 10k;
        attribute feedback.r_bottom = 3.16k;  // For 3.3V output
    }
}
```

### Array Element Attributes

```bhdl
entity MultiChannelDriver(channels: int = 8) {
    generate for i in 0..channels {
        ch[i]: ChannelDriver {
            attribute max_current = 350mA;  // Default
            attribute thermal_limit = 85C;
        }
    }
}

board LEDPanel {
    driver: MultiChannelDriver(channels=16) {
        // Override specific channels
        attribute ch[0].max_current = 250mA;    // Single element
        attribute ch[1].max_current = 250mA;
        attribute ch[2].max_current = 250mA;
        attribute ch[3].max_current = 250mA;
        
        // Range syntax (future enhancement)
        attribute ch[4..7].max_current = 300mA;
        
        // Wildcard for all elements
        attribute ch[*].thermal_limit = 90C;
    }
}
```

### Computed Scoped Attributes

```bhdl
board PowerSystem {
    // Board-level configuration
    attribute system_voltage = 3.3V;
    attribute high_current_mode = true;
    
    supply: PowerSupply {
        // Pass down computed values
        attribute output.target_voltage = system_voltage;
        attribute output.current_limit = high_current_mode ? 10A : 5A;
        
        // Deep attribute override
        attribute output.protection.overcurrent_threshold = output.current_limit * 1.2;
        attribute output.filter.capacitor_count = high_current_mode ? 6 : 3;
    }
}
```

## Implementation Approach

### 1. Parser Extension

```rust
// In grammar.rs - extend attribute parsing
fn parse_attribute(p: &mut Parser) {
    p.expect(T![attribute]);
    
    // Parse potentially dotted identifier
    parse_scoped_identifier(p);  // e.g., feedback.r_top or ch[0].current
    
    p.expect(T![=]);
    parse_expression(p);
    p.expect(T![;]);
}

fn parse_scoped_identifier(p: &mut Parser) {
    // identifier(.identifier | [index])*
    p.expect(T![ident]);
    
    while p.at(T![.]) || p.at(T!['[']) {
        if p.at(T![.]) {
            p.expect(T![.]);
            p.expect(T![ident]);
        } else {
            parse_array_index(p);
        }
    }
}
```

### 2. AST Representation

```rust
#[derive(Debug, Clone)]
pub struct ScopedAttribute {
    pub path: AttributePath,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub struct AttributePath {
    pub segments: Vec<PathSegment>,
}

#[derive(Debug, Clone)]
pub enum PathSegment {
    Identifier(String),
    ArrayIndex(usize),
    ArrayRange(usize, usize),
    ArrayWildcard,
}
```

### 3. Attribute Resolution

```rust
impl AttributeResolver {
    pub fn resolve_scoped_attribute(
        &self,
        instance_path: &InstancePath,
        attr_path: &AttributePath,
    ) -> Option<Value> {
        // Walk the instance hierarchy
        let mut current = self.get_instance(instance_path)?;
        
        for segment in &attr_path.segments {
            match segment {
                PathSegment::Identifier(name) => {
                    current = current.get_child_instance(name)?;
                }
                PathSegment::ArrayIndex(idx) => {
                    current = current.get_array_element(*idx)?;
                }
                // ... handle other cases
            }
        }
        
        current.get_attribute_value()
    }
}
```

## Advantages of Using `attribute`

1. **No new keywords** - Keeps language minimal
2. **Familiar syntax** - Users already know attributes
3. **Natural extension** - Dot notation feels intuitive
4. **Consistent semantics** - Attributes are entity properties

## Complete Example: Multi-Rail Power Supply

```bhdl
// Reusable voltage regulator
entity VoltageRegulator {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    
    controller: RegulatorIC {
        VIN -> .VIN;
        .SW -> inductor.1;
        .FB -> feedback.tap;
        .EN -> EN;
    }
    
    inductor: Inductor {
        attribute value = 10uH;
        attribute isat = 5A;
    }
    
    feedback: FeedbackNetwork {
        VOUT -> .in;
        .tap -> controller.FB;
        
        // Default values - meant to be overridden
        attribute r_top = 10k;
        attribute r_bottom = 10k;
        attribute c_ff = 0pF;  // Optional feedforward cap
    }
    
    output_filter: OutputFilter {
        inductor.2 -> .in;
        .out -> VOUT;
        
        attribute num_caps = 2;
        attribute cap_value = 100uF;
        attribute cap_voltage = 16V;
    }
}

// Feedback network submodule
entity FeedbackNetwork {
    pin in: signal in;
    pin tap: signal out;
    
    attribute r_top = 10k;
    attribute r_bottom = 10k;
    attribute c_ff = 0pF;
    
    R_top: Res(r_top, 1%) {
        in -> .1;
        tap -> .2;
    }
    
    R_bottom: Res(r_bottom, 1%) {
        tap -> .1;
        GND -> .2;
    }
    
    // Conditional feedforward cap
    when (c_ff > 0pF) {
        C_ff: Cap(c_ff) {
            in -> .1;
            tap -> .2;
        }
    }
}

// Main board using scoped attributes
board ServerPowerSupply {
    power VIN_12V = 12V @ 50A;
    ground GND;
    
    // 5V rail - High current
    rail_5v: VoltageRegulator {
        VIN_12V -> .VIN;
        .VOUT -> V5V_BUS;
        
        // Configure via scoped attributes
        attribute feedback.r_top = 10k;
        attribute feedback.r_bottom = 1.91k;     // 5V output
        attribute feedback.c_ff = 22pF;          // Add feedforward
        
        attribute inductor.value = 4.7uH;        // Lower for high current
        attribute inductor.isat = 20A;
        
        attribute output_filter.num_caps = 6;    // More caps
        attribute output_filter.cap_value = 220uF;
    }
    
    // 3.3V rail - Standard
    rail_3v3: VoltageRegulator {
        VIN_12V -> .VIN;
        .VOUT -> V3V3_BUS;
        
        attribute feedback.r_top = 10k;
        attribute feedback.r_bottom = 3.16k;     // 3.3V output
        
        attribute output_filter.num_caps = 3;
        attribute output_filter.cap_value = 100uF;
    }
    
    // 1.2V rail - Low voltage, high current
    rail_1v2: VoltageRegulator {
        V5V_BUS -> .VIN;  // Fed from 5V
        .VOUT -> V1V2_CORE;
        
        attribute feedback.r_top = 10k;
        attribute feedback.r_bottom = 20k;       // 1.2V output
        attribute feedback.c_ff = 47pF;          // More feedforward
        
        attribute inductor.value = 2.2uH;        // Very low inductance
        attribute inductor.isat = 30A;           // High current
        
        attribute output_filter.num_caps = 10;   // Maximum filtering
        attribute output_filter.cap_value = 330uF;
        attribute output_filter.cap_voltage = 6.3V;  // Can use lower voltage
    }
}

// Example with conditional attributes
board AdaptableSystem {
    // System configuration
    attribute low_power_mode = false;
    attribute target_efficiency = 0.92;
    
    converter: BuckConverter {
        // Conditional nested attributes
        attribute controller.fsw = low_power_mode ? 100kHz : 500kHz;
        attribute controller.mode = (target_efficiency > 0.9) ? "sync" : "diode";
        
        // Computed nested values
        attribute inductor.value = 10uH / (controller.fsw / 500kHz);
        attribute output.ripple_target = low_power_mode ? 50mV : 20mV;
    }
}
```

## Syntax Summary

Using existing `attribute` keyword with scoped paths:

```bhdl
// Basic scoped attribute
attribute feedback.r_top = 10k;

// Array element
attribute channel[0].current = 250mA;

// Array range (future)
attribute channel[0..3].current = 250mA;

// Array wildcard
attribute channel[*].frequency = 2MHz;

// Deep nesting
attribute output.filter.caps[*].voltage = 16V;

// Computed values
attribute feedback.ratio = vout / vref;

// Conditional
attribute controller.mode = high_current ? "pwm" : "pfm";
```

This approach is cleaner and more consistent with BHDL's existing syntax!