# BHDL Attribute Type System
## Core Language Feature for Typed Attributes

### Overview

The BHDL attribute type system provides compile-time validation for all attributes used in component definitions, requirements, and board specifications. This is a **core language feature** that applies to:
- Safety attributes (`satisfies` blocks)
- Functional requirements
- Component specifications
- Module interfaces
- Design constraints
- Performance specifications

### Architecture

```
┌─────────────────────────────────────────────────┐
│                BHDL Compiler                     │
│  ┌───────────────────────────────────────────┐  │
│  │  Attribute Type Checker (Core Feature)    │  │
│  │  - Validates against stdlib definitions   │  │
│  │  - Provides autocomplete & error messages │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────┐
│              bhdl-stdlib/attributes             │
│  ┌───────────────────────────────────────────┐  │
│  │  Standard Attribute Definitions           │  │
│  │  - timing.bhdl                            │  │
│  │  - electrical.bhdl                        │  │
│  │  - performance.bhdl                       │  │
│  │  - mechanical.bhdl                        │  │
│  │  - ...                                    │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
                        ↓
┌─────────────────────────────────────────────────┐
│           User/Custom Libraries                 │
│  ┌───────────────────────────────────────────┐  │
│  │  Domain-Specific Attributes               │  │
│  │  - automotive_attributes.bhdl             │  │
│  │  - aerospace_attributes.bhdl              │  │
│  │  - medical_attributes.bhdl                │  │
│  └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────┘
```

## 1. Core Language Support

### Attribute Declaration Syntax
The compiler recognizes `attribute_type` as a first-class construct:

```bhdl
// Basic attribute type declaration
attribute_type response_time: time {
    description: "Time to respond to a stimulus";
    units: [ns, µs, ms, s];
    range: 0..∞;
    default_unit: µs;
}

// Enum attribute type
attribute_type protection_method: enum {
    description: "Method of circuit protection";
    values: [crowbar, clamp, shutdown, foldback, hiccup];
    default: shutdown;
}

// Structured attribute type
attribute_type voltage_spec: struct {
    nominal: voltage;
    tolerance: percentage;
    ripple: voltage;
    transients: voltage_range;
}
```

### Attribute Group Declaration
Groups organize related attributes:

```bhdl
attribute_group TimingAttributes {
    description: "Time-related characteristics";
    
    attributes {
        response_time: time {
            description: "Response to input change";
            typical_range: 1ns..1s;
        }
        
        propagation_delay: time {
            description: "Input to output delay";
            typical_range: 1ns..100ms;
        }
        
        rise_time: time {
            description: "Low to high transition";
            typical_range: 1ns..10ms;
        }
        
        setup_time: time {
            description: "Data stable before clock";
            typical_range: 0..100ns;
        }
    }
}
```

## 2. Standard Library Definitions

### File: `bhdl-stdlib/attributes/timing.bhdl`
```bhdl
// Standard timing attributes used across BHDL
namespace stdlib.attributes.timing {
    
    attribute_group TimingAttributes {
        description: "Standard timing-related attributes";
        
        // Response times
        attribute_type response_time: time {
            description: "Time to respond to input change";
            applications: [safety, control, interface];
        }
        
        attribute_type reaction_time: time {
            description: "Time from detection to action";
            applications: [safety, protection];
        }
        
        attribute_type detection_time: time {
            description: "Time to detect a condition";
            applications: [monitoring, safety];
        }
        
        // Delays and propagation
        attribute_type propagation_delay: time {
            description: "Signal propagation time";
            applications: [digital, analog, interface];
        }
        
        attribute_type turn_on_delay: time {
            description: "Enable to output valid";
            applications: [power, switching];
        }
        
        // Periodic timing
        attribute_type period: time {
            description: "Repetition period";
            applications: [clock, pwm, sampling];
        }
        
        attribute_type frequency: frequency {
            description: "Rate of repetition";
            related: period = 1/frequency;
        }
        
        // Test and diagnostic
        attribute_type test_interval: time {
            description: "Time between self-tests";
            applications: [safety, diagnostic];
            typical_range: 1ms..1hour;
        }
    }
}
```

### File: `bhdl-stdlib/attributes/electrical.bhdl`
```bhdl
namespace stdlib.attributes.electrical {
    
    attribute_group VoltageAttributes {
        attribute_type voltage: voltage {
            description: "Electric potential";
            units: [µV, mV, V, kV];
            default_unit: V;
        }
        
        attribute_type voltage_range: range<voltage> {
            description: "Operating voltage range";
            format: "min..max";
        }
        
        attribute_type voltage_tolerance: tolerance {
            description: "Voltage variation tolerance";
            format: "±percentage" | "±absolute";
        }
        
        attribute_type ripple: voltage {
            description: "Peak-to-peak voltage variation";
            measurement: peak_to_peak;
        }
    }
    
    attribute_group CurrentAttributes {
        attribute_type current: current {
            description: "Electric current";
            units: [nA, µA, mA, A];
            default_unit: mA;
        }
        
        attribute_type current_limit: current {
            description: "Maximum allowed current";
            applications: [protection, specification];
        }
        
        attribute_type inrush_current: current {
            description: "Startup current spike";
            measurement: peak;
        }
    }
    
    attribute_group PowerAttributes {
        attribute_type power: power {
            description: "Electric power";
            units: [µW, mW, W, kW];
            default_unit: W;
        }
        
        attribute_type efficiency: percentage {
            description: "Power conversion efficiency";
            range: 0%..100%;
        }
    }
}
```

### File: `bhdl-stdlib/attributes/performance.bhdl`
```bhdl
namespace stdlib.attributes.performance {
    
    attribute_type accuracy: percentage | absolute {
        description: "Measurement accuracy";
        format: "±percentage" | "±absolute_value";
    }
    
    attribute_type resolution: quantity {
        description: "Smallest detectable change";
        applications: [adc, dac, sensor];
    }
    
    attribute_type bandwidth: frequency {
        description: "Frequency range of operation";
        measurement: -3dB_points;
    }
    
    attribute_type noise: voltage | current {
        description: "Random signal variations";
        measurement: rms | peak_to_peak;
    }
    
    attribute_type thd: percentage {
        description: "Total harmonic distortion";
        range: 0%..100%;
        typical: <1%;
    }
}
```

## 3. Capability Definitions Using Typed Attributes

### File: `bhdl-stdlib/capabilities/monitoring.bhdl`
```bhdl
import stdlib.attributes.*;

capability VoltageMonitoring {
    description: "Ability to monitor voltage levels";
    
    required_attributes: [
        electrical.voltage_range,
        timing.response_time,
    ];
    
    optional_attributes: [
        electrical.voltage_tolerance,
        performance.accuracy,
        coverage.diagnostic_coverage,
    ];
    
    constraints {
        response_time: <1s;  // Must respond within 1 second
        accuracy: better_than(±5%);  // If specified, must be ±5% or better
    }
}

capability TemperatureMonitoring {
    description: "Ability to monitor temperature";
    
    required_attributes: [
        thermal.temperature_range,
        performance.accuracy,
    ];
    
    optional_attributes: [
        timing.response_time,
        performance.resolution,
        timing.sample_rate,
    ];
}
```

## 4. User-Defined Custom Attributes

### Custom Domain Library: `automotive_lib/attributes/functional_safety.bhdl`
```bhdl
import stdlib.attributes.*;

namespace automotive.attributes {
    
    // Extend standard attributes with domain-specific ones
    attribute_type asil_level: enum {
        description: "ISO 26262 ASIL level";
        values: [QM, ASIL_A, ASIL_B, ASIL_C, ASIL_D];
        ordering: QM < ASIL_A < ASIL_B < ASIL_C < ASIL_D;
    }
    
    attribute_type diagnostic_coverage: percentage {
        description: "Percentage of faults detected";
        iso26262_thresholds: {
            low: <60%;
            medium: 60%..90%;
            high: ≥90%;
        }
    }
    
    attribute_type fault_reaction_time: time {
        description: "Time from fault detection to safe state";
        extends: stdlib.attributes.timing.reaction_time;
        typical_range: 1µs..100ms;
        safety_critical: true;
    }
}
```

## 5. Using Attributes in BHDL Code

### Component Definition
```bhdl
import stdlib.attributes.*;

component LM2596 {
    // Functional specifications use typed attributes
    specifications {
        electrical.voltage_range: 4.5V..40V;
        electrical.efficiency: 85% @ 3A;
        electrical.switching_frequency: 150kHz ± 10%;
        timing.soft_start_time: 10ms typical;
        thermal.operating_temp_range: -40°C..125°C;
    }
    
    // Safety capabilities use the same attribute system
    satisfies VoltageRegulation {
        electrical.voltage_range: 4.5V..40V;
        electrical.efficiency: 85%;
        performance.line_regulation: 0.2%;
        performance.load_regulation: 0.5%;
    }
    
    satisfies CurrentLimiting {
        electrical.current_limit: 3A;
        timing.response_time: 10µs;
        protection.method: cycle_by_cycle;  // Enum type
    }
}
```

### Functional Requirements
```bhdl
// Functional requirements use the same attribute system
functional_requirement PowerEfficiency {
    description: "Power supply efficiency requirements";
    
    constraints {
        electrical.efficiency: ≥80% @ full_load;
        electrical.efficiency: ≥85% @ half_load;
        thermal.max_dissipation: ≤5W;
    }
}

functional_requirement SignalIntegrity {
    description: "High-speed signal requirements";
    
    constraints {
        timing.rise_time: ≤2ns;
        timing.fall_time: ≤2ns;
        performance.overshoot: ≤10%;
        electrical.impedance: 50Ω ± 5%;
    }
}
```

### Board-Level Specifications
```bhdl
board HighSpeedProcessor {
    // Board-level constraints use typed attributes
    constraints {
        timing.max_trace_delay: 100ps/inch;
        electrical.impedance_tolerance: ±10%;
        thermal.max_junction_temp: 85°C;
        mechanical.pcb_thickness: 1.6mm ± 10%;
    }
    
    // Functional requirements
    satisfies {
        PowerEfficiency: {
            electrical.efficiency: 87% measured;  // Actual value
        }
        
        SignalIntegrity: {
            timing.rise_time: 1.8ns measured;
            timing.fall_time: 1.7ns measured;
        }
    }
}
```

## 6. Compiler Integration

The compiler provides:

### Autocomplete
```bhdl
component MyComponent {
    specifications {
        electrical.  // <- IDE shows: voltage, current, power, efficiency...
    }
}
```

### Type Checking
```bhdl
// Compiler errors:
electrical.voltage: "5 volts";     // Error: Expected voltage type, got string
timing.frequency: 100V;            // Error: Expected frequency, got voltage
thermal.temperature: 85;           // Error: Missing unit (°C, °F, K)
```

### Import Resolution
```bhdl
import stdlib.attributes.timing.*;      // Standard library
import automotive.attributes.*;         // Domain library
import myproject.custom_attributes.*;   // Project-specific

// All attributes are now available with full type checking
```

## 7. Benefits of Library-Based Approach

### 1. **Flexibility**
- Add new attributes without compiler changes
- Domain-specific libraries can extend standard attributes
- Projects can define custom attributes

### 2. **Versioning**
```bhdl
import stdlib.attributes@v2.0.*;  // Specific version
import stdlib.attributes.*;       // Latest version
```

### 3. **Backward Compatibility**
```bhdl
// Old attribute names can be aliased
attribute_type reaction_time = response_time {
    deprecated: "Use response_time instead";
    since: "v2.0";
}
```

### 4. **Documentation**
```bhdl
attribute_type test_coverage: percentage {
    description: "Percentage of functionality tested";
    see_also: [diagnostic_coverage, fault_coverage];
    examples: [
        "test_coverage: 95%",
        "test_coverage: ≥90%"
    ];
}
```

### 5. **Validation Rules**
```bhdl
attribute_type efficiency: percentage {
    range: 0%..100%;
    
    validation_rules {
        warn_if: <50% { message: "Unusually low efficiency" }
        error_if: >100% { message: "Efficiency cannot exceed 100%" }
    }
}
```

## 8. Migration from Hardcoded to Library-Based

### Phase 1: Define Core Attributes in stdlib
```bash
bhdl-stdlib/
├── attributes/
│   ├── index.bhdl       # Main entry point
│   ├── timing.bhdl      # Timing attributes
│   ├── electrical.bhdl  # Electrical attributes
│   ├── thermal.bhdl     # Thermal attributes
│   ├── mechanical.bhdl  # Mechanical attributes
│   └── performance.bhdl # Performance attributes
```

### Phase 2: Compiler References stdlib
```rust
// In BHDL compiler
fn validate_attribute(name: &str, value: &Value) -> Result<(), Error> {
    // Look up attribute definition from loaded stdlib
    let attr_def = stdlib.lookup_attribute(name)?;
    attr_def.validate(value)
}
```

### Phase 3: Support Custom Libraries
```bhdl
// Project configuration
project MyProject {
    dependencies {
        stdlib: "1.0.0";
        automotive_lib: "2.1.0";
        my_custom_lib: "./lib/custom_attributes";
    }
}
```

## Conclusion

By making the attribute type system:
1. **A core language feature** - Available everywhere in BHDL
2. **Library-defined** - Flexible and extensible
3. **Compile-time validated** - Catch errors early
4. **Domain-extensible** - Custom attributes for specific industries

We get a powerful, flexible system that provides safety without sacrificing extensibility. The stdlib provides common attributes everyone needs, while allowing complete customization for specific domains or projects.