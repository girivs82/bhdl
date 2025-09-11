# Safety Attribute Type System
## Compile-Time Validation for Safety Requirements

### Problem Statement

Currently, safety attributes are free-form, allowing typos and inconsistencies:
```bhdl
// These should be compile-time errors:
satisfies VoltageMonitoring {
    responce_time: 50µs;     // Typo: should be "response_time"
    max_tollerance: ±2%;     // Typo: should be "tolerance"
    coverge: 90%;            // Typo: should be "coverage"
    custom_param: 123;       // Unknown attribute
}
```

### Solution: Strongly-Typed Attribute System

## 1. Core Attribute Types

### Timing Attributes
```bhdl
enum TimingAttribute {
    response_time,        // Time to respond to condition
    detection_time,       // Time to detect fault
    reaction_time,        // Time to take action
    propagation_delay,    // Signal propagation time
    settling_time,        // Time to stabilize
    rise_time,           // Signal rise time
    fall_time,           // Signal fall time
    test_interval,       // Self-test period
    test_duration,       // Self-test execution time
    startup_time,        // Power-on to ready time
    shutdown_time,       // Time to safe state
    hold_time,           // Minimum hold duration
    timeout,             // Maximum wait time
    debounce_time,       // Input debouncing period
}
```

### Electrical Attributes
```bhdl
enum ElectricalAttribute {
    // Voltage
    voltage,             // Operating voltage
    voltage_range,       // Min..max voltage
    threshold_voltage,   // Trigger voltage
    overvoltage_limit,   // Maximum safe voltage
    undervoltage_limit,  // Minimum operating voltage
    voltage_tolerance,   // ± tolerance
    voltage_ripple,      // Peak-to-peak ripple
    
    // Current
    current,             // Operating current
    max_current,         // Maximum current
    min_current,         // Minimum current
    current_limit,       // Protection threshold
    inrush_current,      // Startup current
    quiescent_current,   // Idle current
    leakage_current,     // Maximum leakage
    
    // Power
    power_dissipation,   // Heat generation
    max_power,           // Maximum power rating
    efficiency,          // Power efficiency %
    
    // Impedance
    impedance,           // Input/output impedance
    resistance,          // DC resistance
    capacitance,         // Capacitance value
    inductance,          // Inductance value
    
    // Frequency
    frequency,           // Operating frequency
    frequency_range,     // Min..max frequency
    bandwidth,           // Signal bandwidth
    switching_frequency, // PWM/switching freq
}
```

### Coverage Attributes
```bhdl
enum CoverageAttribute {
    diagnostic_coverage,      // % of faults detected
    self_test_coverage,      // % covered by self-test
    latent_fault_coverage,   // % of latent faults detected
    safe_fault_coverage,     // % of safe faults
    detection_coverage,      // % of detectable conditions
    protection_coverage,     // % of protected scenarios
}
```

### Reliability Attributes
```bhdl
enum ReliabilityAttribute {
    failure_rate,        // FIT or failures/hour
    mtbf,               // Mean time between failures
    mttr,               // Mean time to repair
    availability,       // % uptime
    mission_time,       // Operating duration
    lifetime,           // Component lifetime
    cycles,             // Number of cycles
    endurance,          // Write/erase cycles
}
```

### Performance Attributes
```bhdl
enum PerformanceAttribute {
    accuracy,           // Measurement accuracy
    precision,          // Measurement precision
    resolution,         // Measurement resolution
    tolerance,          // General tolerance
    drift,              // Parameter drift over time
    stability,          // Long-term stability
    noise,              // Noise level
    snr,                // Signal-to-noise ratio
    thd,                // Total harmonic distortion
    crosstalk,          // Channel isolation
}
```

### Safety Attributes
```bhdl
enum SafetyAttribute {
    asil_level,         // ASIL A/B/C/D
    sil_level,          // SIL 1/2/3/4
    category,           // Safety category
    performance_level,  // PL a/b/c/d/e
    coverage_factor,    // DC/CC/CCF
    proof_test_interval,// Periodic test interval
    diagnostic_test_ratio, // DTR percentage
}
```

### Thermal Attributes
```bhdl
enum ThermalAttribute {
    operating_temp_range,    // Min..max temperature
    storage_temp_range,      // Storage temperature
    junction_temp_max,       // Maximum junction temp
    thermal_resistance,      // Thermal resistance
    power_derating,         // Derating curve
    temperature_coefficient, // Temp coefficient
}
```

### Mechanical Attributes
```bhdl
enum MechanicalAttribute {
    vibration_resistance,   // Vibration spec
    shock_resistance,       // Shock rating
    ingress_protection,     // IP rating
    altitude_max,           // Maximum altitude
    humidity_range,         // Operating humidity
    pressure_range,         // Pressure limits
}
```

## 2. Capability-Specific Attribute Sets

Each safety capability has a defined set of allowed attributes:

```bhdl
capability VoltageMonitoring {
    required_attributes: [
        ElectricalAttribute.voltage_range,
        TimingAttribute.response_time,
        CoverageAttribute.diagnostic_coverage
    ];
    
    optional_attributes: [
        ElectricalAttribute.threshold_voltage,
        ElectricalAttribute.voltage_tolerance,
        PerformanceAttribute.accuracy,
        TimingAttribute.detection_time,
        ReliabilityAttribute.failure_rate
    ];
    
    forbidden_attributes: [
        // Attributes that don't make sense for voltage monitoring
        ElectricalAttribute.inductance,
        MechanicalAttribute.vibration_resistance
    ];
}

capability CurrentLimiting {
    required_attributes: [
        ElectricalAttribute.current_limit,
        TimingAttribute.response_time
    ];
    
    optional_attributes: [
        ElectricalAttribute.max_current,
        PerformanceAttribute.accuracy,
        TimingAttribute.reaction_time,
        method: enum { foldback, hiccup, latching, current_mode }
    ];
}

capability SelfTestable {
    required_attributes: [
        TimingAttribute.test_interval,
        CoverageAttribute.self_test_coverage
    ];
    
    optional_attributes: [
        TimingAttribute.test_duration,
        TimingAttribute.startup_time,
        test_method: enum { 
            bist,                    // Built-in self-test
            functional_test,         // Functional verification
            diagnostic_test,         // Diagnostic coverage test
            proof_test,             // Proof test
            runtime_test,           // Continuous monitoring
            initiated_test,         // Externally triggered
            automatic_test          // Automatic periodic
        }
    ];
}

capability OvervoltageProtection {
    required_attributes: [
        ElectricalAttribute.overvoltage_limit,
        TimingAttribute.response_time
    ];
    
    optional_attributes: [
        protection_method: enum { 
            crowbar,                // SCR crowbar
            clamp,                  // Voltage clamping
            shutdown,               // Power shutdown
            foldback,              // Foldback limiting
            series_disconnect      // Series switch
        },
        ElectricalAttribute.voltage_tolerance,
        recovery_method: enum { 
            auto_retry,
            manual_reset,
            latch_off,
            time_delayed
        }
    ];
}
```

## 3. Type-Safe Syntax

### Component Definition with Typed Attributes
```bhdl
component LTC2954 {
    satisfies VoltageMonitoring {
        // Compiler validates these are valid VoltageMonitoring attributes
        voltage_range: 2.7V..28V;              // ✓ ElectricalAttribute
        response_time: 50µs;                   // ✓ TimingAttribute
        diagnostic_coverage: 99%;              // ✓ CoverageAttribute
        accuracy: ±1.5%;                       // ✓ PerformanceAttribute
        
        // Compile-time error examples:
        // responce_time: 50µs;                // ✗ Typo - not in enum
        // custom_param: 123;                  // ✗ Not in allowed attributes
        // inductance: 10µH;                   // ✗ Forbidden for VoltageMonitoring
    }
    
    satisfies SelfTestable {
        test_interval: 100ms;                  // ✓ Required attribute
        self_test_coverage: 87%;              // ✓ Required attribute
        test_method: bist;                     // ✓ Optional enum value
        test_duration: 5ms;                   // ✓ Optional attribute
    }
}
```

### Board-Level Requirements with Typed Attributes
```bhdl
board PowerSupply {
    satisfies {
        REQ_001: via monitor {
            // Tool validates these match requirement template
            response_time: 50µs;               // Validated against requirement
            diagnostic_coverage: 95%;          // Validated against requirement
        };
    }
}
```

## 4. Requirement Templates with Typed Attributes

```bhdl
requirement_template VoltageMonitoringRequirement {
    parameters: {
        monitored_voltage: ElectricalAttribute.voltage;
        threshold: ElectricalAttribute.threshold_voltage;
        response: TimingAttribute.response_time;
        coverage: CoverageAttribute.diagnostic_coverage;
    }
    
    // Type-safe template
    template: {
        description: "Monitor ${monitored_voltage} rail";
        
        functional_interface {
            monitored_signal: monitored_voltage;
            fault_threshold: threshold;
            response_time: ≤ response;
            diagnostic_coverage: ≥ coverage;
        }
    }
}

// Usage with compile-time validation
requirement REQ_PSU_001: VoltageMonitoringRequirement {
    monitored_voltage: 5V;
    threshold: 5.5V ± 2%;
    response: 100µs;
    coverage: 90%;
    
    // Compile error if wrong type:
    // response: 90%;           // ✗ Wrong type (percentage for time)
    // unknown_param: 123;      // ✗ Not in template
}
```

## 5. Validation Rules

### Compile-Time Checks
```bhdl
compiler_rules {
    // 1. Attribute name validation
    validate_attribute_exists: {
        for_each attribute_name in satisfies_block {
            assert attribute_name in AttributeEnum;
        }
    }
    
    // 2. Attribute type validation
    validate_attribute_type: {
        for_each attribute in satisfies_block {
            assert typeof(attribute.value) matches attribute.expected_type;
        }
    }
    
    // 3. Required attributes check
    validate_required_attributes: {
        for_each capability in satisfies {
            assert all required_attributes are present;
        }
    }
    
    // 4. Forbidden attributes check
    validate_no_forbidden: {
        for_each capability in satisfies {
            assert no forbidden_attributes are used;
        }
    }
    
    // 5. Value range validation
    validate_value_ranges: {
        response_time: must_be > 0;
        coverage: must_be 0%..100%;
        efficiency: must_be 0%..100%;
        voltage_tolerance: must_be ± value;
    }
}
```

### Runtime Validation
```bhdl
runtime_validation {
    // Validate calculated values
    validate_derived_attributes: {
        if voltage_range defined and threshold_voltage defined {
            assert threshold_voltage within voltage_range;
        }
    }
    
    // Cross-reference validation
    validate_consistency: {
        if component.satisfies includes VoltageMonitoring {
            assert component.electrical_model.monitoring_range exists;
            assert satisfies.voltage_range ⊆ electrical_model.monitoring_range;
        }
    }
}
```

## 6. Benefits of Typed Attribute System

1. **Compile-Time Safety**
   - Catch typos before runtime
   - Ensure all required attributes present
   - Prevent invalid attribute combinations

2. **IDE Support**
   - Autocomplete for attribute names
   - Type checking for values
   - Documentation on hover

3. **Consistency**
   - Standard attribute names across projects
   - Consistent units and formats
   - Clear semantics

4. **Maintainability**
   - Central definition of attributes
   - Easy to add new attributes
   - Clear deprecation path

5. **Tool Integration**
   - Tools know exactly what to expect
   - Automated validation possible
   - Better error messages

## 7. Migration Strategy

### Phase 1: Define Core Attributes
```bhdl
// Start with most common attributes
core_attributes: [
    response_time,
    voltage_range,
    current_limit,
    diagnostic_coverage,
    test_interval
];
```

### Phase 2: Add Validation Warnings
```bhdl
// Warn about unknown attributes
warning: "Unknown attribute 'responce_time', did you mean 'response_time'?"
```

### Phase 3: Enforce Strict Mode
```bhdl
// Make unknown attributes an error
pragma strict_attributes;  // Enables compile-time enforcement
```

## 8. Example with Full Type Safety

```bhdl
// With typed attributes - compile-time validated
component LM2596 {
    satisfies VoltageRegulation {
        voltage_range: 4.5V..40V;        // ✓ Valid ElectricalAttribute
        efficiency: 85%;                 // ✓ Valid PerformanceAttribute
        switching_frequency: 150kHz;     // ✓ Valid ElectricalAttribute
        
        // response_time not allowed for VoltageRegulation
        // response_time: 10µs;          // ✗ Compile error
    }
    
    satisfies CurrentLimiting {
        current_limit: 3A;               // ✓ Required attribute
        response_time: 10µs;             // ✓ Required attribute
        method: cycle_by_cycle;          // ✓ Valid enum value
        
        // test_interval not relevant here
        // test_interval: 100ms;         // ✗ Compile error
    }
}

// Tool provides clear error messages
error[E001]: Unknown attribute 'responce_time'
  --> line 3:9
  |
3 |         responce_time: 50µs;
  |         ^^^^^^^^^^^^^ did you mean 'response_time'?
  |
  = help: Valid attributes for VoltageMonitoring are:
          - response_time (TimingAttribute)
          - voltage_range (ElectricalAttribute)
          - diagnostic_coverage (CoverageAttribute)

error[E002]: Missing required attribute
  --> line 10:5
  |
10|     satisfies SelfTestable {
  |     ^^^^^^^^^^^^^^^^^^^^^^
  |
  = help: SelfTestable requires:
          - test_interval (TimingAttribute)
          - self_test_coverage (CoverageAttribute)
```

## Conclusion

A strongly-typed attribute system provides:
- **Compile-time validation** of all safety attributes
- **Clear contracts** for what each capability requires
- **Better tooling** with IDE support and error messages
- **Consistency** across projects and teams
- **Evolution path** for adding new attributes safely

This system ensures safety-critical attributes are correct by construction, catching errors early in the development cycle.