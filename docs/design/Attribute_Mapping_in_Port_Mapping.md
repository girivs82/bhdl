# Attribute Mapping During Entity Instantiation

## Overview

When instantiating an entity, we need to map both pins (port mapping) and attributes (configuration). This allows parameterizing each instance differently.

## Syntax Design

### Basic Attribute Mapping

```bhdl
entity VoltageRegulator {
    pin VIN: power in;
    pin VOUT: power out;
    
    // Module attributes that can be overridden
    attribute vout_target = 3.3V;
    attribute switching_freq = 500kHz;
    attribute soft_start_time = 10ms;
}

board PowerSupply {
    // Instance with both port and attribute mapping
    reg_5v: VoltageRegulator {
        // Port mappings
        VIN_12V -> .VIN;
        .VOUT -> RAIL_5V;
        
        // Attribute mappings
        attribute vout_target = 5V;
        attribute switching_freq = 300kHz;
        // soft_start_time uses default 10ms
    }
    
    reg_3v3: VoltageRegulator {
        VIN_12V -> .VIN;
        .VOUT -> RAIL_3V3;
        
        // Uses default attributes (3.3V, 500kHz, 10ms)
    }
}
```

### Computed Attribute Mapping

```bhdl
board AdaptiveSystem {
    // Board-level attributes
    attribute system_voltage = 3.3V;
    attribute low_power_mode = true;
    
    controller: PowerController {
        // Port mappings
        VIN -> .POWER;
        .OUT -> controlled_rail;
        
        // Attribute mappings with expressions
        attribute target_voltage = system_voltage;
        attribute max_current = low_power_mode ? 1A : 5A;
        attribute efficiency_mode = low_power_mode ? "light" : "heavy";
    }
}
```

### Nested Attribute Mapping

```bhdl
entity ComplexRegulator {
    // Nested entities with their own attributes
    controller: ControlIC {
        attribute fsw = 500kHz;
        attribute compensation = "type3";
    }
    
    feedback: FeedbackNetwork {
        attribute r_top = 10k;
        attribute r_bottom = 10k;
    }
}

board System {
    supply: ComplexRegulator {
        // Pin mappings
        VIN -> .IN;
        .OUT -> VOUT;
        
        // Direct nested attribute mapping
        attribute controller.fsw = 1MHz;
        attribute feedback.r_bottom = 3.3k;  // For different voltage
    }
}
```

### Array Attribute Mapping

```bhdl
entity MultiChannelDriver(channels: int = 4) {
    generate for i in 0..channels {
        driver[i]: ChannelDriver {
            attribute max_current = 350mA;
            attribute thermal_limit = 85C;
        }
    }
}

board LEDSystem {
    drivers: MultiChannelDriver(channels=8) {
        // Pin mappings
        VIN -> .POWER;
        
        // Array attribute mappings
        attribute driver[0].max_current = 250mA;  // Red LED
        attribute driver[1].max_current = 250mA;
        attribute driver[2].max_current = 300mA;  // Green LED
        attribute driver[3].max_current = 300mA;
        attribute driver[4..7].max_current = 400mA;  // Blue LEDs
        
        // Wildcard for all
        attribute driver[*].thermal_limit = 90C;
    }
}
```

## Advanced Features

### 1. Attribute Inheritance

```bhdl
entity Parent {
    attribute base_frequency = 1MHz;
    
    child: Child {
        // Child can reference parent attributes
        attribute operating_freq = base_frequency / 2;
    }
}
```

### 2. Conditional Attribute Mapping

```bhdl
board FlexibleSystem {
    attribute high_performance = true;
    
    processor: CPU {
        // Conditional attribute values
        attribute clock_speed = high_performance ? 3GHz : 1.5GHz;
        attribute voltage = high_performance ? 1.2V : 0.9V;
        attribute cache_size = high_performance ? "8MB" : "2MB";
    }
}
```

### 3. Type-Checked Attribute Mapping

```bhdl
entity TypedModule {
    attribute<voltage> vref = 1.2V;
    attribute<frequency> fsw = 500kHz;
    attribute<int> divider_ratio = 2;
}

board Usage {
    inst: TypedModule {
        attribute vref = 2.5V;        // OK: voltage type
        attribute fsw = 1MHz;         // OK: frequency type
        // attribute vref = 1MHz;     // ERROR: frequency != voltage
        attribute divider_ratio = 4;  // OK: int type
    }
}
```

### 4. Entity Parameter vs Attribute Mapping

```bhdl
// Entity with both parameters and attributes
entity FlexRegulator(
    topology: string = "buck"  // Parameter: fixed at instantiation
) {
    attribute vout = 3.3V;     // Attribute: can be overridden
    attribute imax = 2A;       // Attribute: can be overridden
    
    when (topology == "buck") {
        // Buck implementation
    } else when (topology == "boost") {
        // Boost implementation
    }
}

board Power {
    // Parameter goes in parentheses, attributes in body
    buck_reg: FlexRegulator(topology="buck") {
        VIN -> .IN;
        .OUT -> VOUT_5V;
        
        attribute vout = 5V;    // Override attribute
        attribute imax = 3A;    // Override attribute
        // topology is fixed as "buck"
    }
}
```

## Implementation Approach

### 1. Parser Extension

```rust
// In instance declaration parsing
fn parse_instance_body(p: &mut Parser) {
    p.expect(T!['{']);
    
    while !p.at(T!['}']) {
        if p.at(T![attribute]) {
            parse_attribute_mapping(p);  // NEW
        } else {
            parse_port_mapping(p);       // Existing
        }
    }
    
    p.expect(T!['}']);
}

fn parse_attribute_mapping(p: &mut Parser) {
    p.expect(T![attribute]);
    parse_scoped_path(p);  // e.g., controller.fsw
    p.expect(T![=]);
    parse_expression(p);
    p.expect(T![;]);
}
```

### 2. AST Representation

```rust
#[derive(Debug, Clone)]
pub struct InstanceDecl {
    pub name: String,
    pub entity_type: String,
    pub params: Option<ParamList>,
    pub port_mappings: Vec<PortMapping>,
    pub attribute_mappings: Vec<AttributeMapping>,  // NEW
}

#[derive(Debug, Clone)]
pub struct AttributeMapping {
    pub path: AttributePath,  // Can be nested: feedback.r_top
    pub value: Expr,
}
```

### 3. Analyzer Resolution

```rust
impl Analyzer {
    fn resolve_instance_attributes(
        &mut self,
        instance: &InstanceDecl,
        entity_def: &Entity,
    ) -> AttributeContext {
        let mut context = AttributeContext::new();
        
        // 1. Start with entity defaults
        for attr in entity_def.attributes() {
            context.set(&attr.path, attr.default_value);
        }
        
        // 2. Apply instance overrides
        for mapping in &instance.attribute_mappings {
            let value = self.evaluate_expr(&mapping.value);
            context.override_attr(&mapping.path, value);
        }
        
        // 3. Validate overrides exist
        for mapping in &instance.attribute_mappings {
            if !entity_def.has_attribute_path(&mapping.path) {
                self.error(format!(
                    "Entity '{}' has no attribute '{}'",
                    entity_def.name,
                    mapping.path
                ));
            }
        }
        
        context
    }
}
```

## Complete Example

```bhdl
entity UniversalBuckConverter {
    pin VIN: power in;
    pin VOUT: power out;
    pin EN: digital in;
    
    // Configurable attributes
    attribute vout_target = 3.3V;
    attribute imax = 2A;
    attribute fsw = 500kHz;
    
    // Computed internal values
    attribute ripple_current = imax * 0.3;
    attribute inductor_value = (VIN - vout_target) / (fsw * ripple_current);
    
    controller: BuckControllerIC {
        attribute switching_freq = fsw;
        attribute current_limit = imax * 1.2;
    }
    
    feedback: FeedbackDivider {
        attribute r_top = 10k;
        attribute r_bottom = 10k * 0.8V / (vout_target - 0.8V);
    }
    
    power_stage: PowerStage {
        attribute inductor = inductor_value;
        attribute num_output_caps = ceil(imax / 5A);
    }
}

board PowerDistribution {
    // System configuration
    attribute ambient_temp = 50C;
    attribute high_efficiency_mode = true;
    
    // 12V to 5V, 10A
    main_5v: UniversalBuckConverter {
        // Port mappings
        VIN_12V -> .VIN;
        .VOUT -> RAIL_5V;
        main_enable -> .EN;
        
        // Attribute mappings
        attribute vout_target = 5V;
        attribute imax = 10A;
        attribute fsw = high_efficiency_mode ? 300kHz : 500kHz;
        
        // Override nested attributes
        attribute controller.current_limit = 12A;  // Extra margin
        attribute power_stage.num_output_caps = 3; // Override calculation
    }
    
    // 5V to 3.3V, 5A
    secondary_3v3: UniversalBuckConverter {
        RAIL_5V -> .VIN;
        .VOUT -> RAIL_3V3;
        secondary_enable -> .EN;
        
        attribute vout_target = 3.3V;
        attribute imax = 5A;
        // Use default 500kHz switching frequency
        
        attribute feedback.r_bottom = 3.16k;  // Fine tune output
    }
    
    // 5V to 1.2V, 20A for FPGA
    fpga_core: UniversalBuckConverter {
        RAIL_5V -> .VIN;
        .VOUT -> RAIL_1V2_CORE;
        fpga_enable -> .EN;
        
        attribute vout_target = 1.2V;
        attribute imax = 20A;
        attribute fsw = 1MHz;  // Higher frequency for fast transients
        
        // Lots of customization for FPGA requirements
        attribute controller.switching_freq = 1MHz;
        attribute controller.transient_response = "fast";
        attribute power_stage.num_output_caps = 8;
        attribute power_stage.use_polymer_caps = true;
    }
}
```

## Benefits

1. **Configuration Flexibility**: Each instance can be configured differently
2. **Computed Values**: Attributes can be expressions, not just literals
3. **Deep Configuration**: Can reach into nested modules
4. **Type Safety**: Attribute types are checked
5. **Clear Separation**: Parameters (structural) vs attributes (configuration)
6. **Inheritance**: Child entities can reference parent attributes

This makes entities truly reusable - same entity definition, different configurations per instance!