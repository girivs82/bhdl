# Realistic Requirement Generation Approach
## Semi-Automated Requirements with Safety Engineer Refinement

### The Problem with Full Automation

**From this high-level safety function:**
```bhdl
FSR1: safety_function {
    id: "FSR_PSU_001";
    description: "Power supply output monitoring and fault detection";
    implements: SG1;
    asil: ASIL_B;
}
```

**Tool CANNOT automatically determine:**
- What voltage to monitor (5V? 3.3V? 12V?)
- What constitutes a "fault" (overvoltage? undervoltage? both?)
- What threshold values are appropriate
- How fast detection needs to be
- What type of fault indication is needed

### Realistic Approach: Template Generation + Manual Refinement

## 1. Tool-Generated Template Structure

The tool generates **requirement shells** with placeholders based on safety function type:

```bhdl
// TOOL-GENERATED TEMPLATE (incomplete)
generated_requirement_template {
    REQ_PSU_001: requirement {
        source: "FSR_PSU_001";
        type: safety;
        asil: ASIL_B;  // Inherited from safety function
        
        description: "[SAFETY_ENGINEER: Specify what to monitor and how]";
        
        // Template based on "monitoring and fault detection" pattern
        functional_interface {
            monitored_signal: "[SPECIFY: voltage/current/temperature]";
            monitoring_range: "[SPECIFY: min..max with units]";
            fault_conditions: "[SPECIFY: overvoltage, undervoltage, out_of_range]";
            fault_indication: "[SPECIFY: signal type and polarity]";
        }
        
        performance_constraints {
            detection_time: "[SPECIFY: time requirement]";
            accuracy: "[SPECIFY: ± percentage or absolute]";
            false_positive_rate: "[SPECIFY: if applicable]";
        }
        
        // Placeholders for common monitoring requirements
        implementation_hints {
            typical_solutions: [voltage_supervisor, window_comparator, ADC_monitoring];
            consider: "Self-test capability may be needed for ASIL B";
        }
    }
}
```

## 2. Safety Engineer Completes the Template

The safety engineer fills in the domain-specific details:

```bhdl
// SAFETY ENGINEER COMPLETED
requirements {
    REQ_PSU_001: requirement {
        source: "FSR_PSU_001";
        type: safety;
        asil: ASIL_B;
        
        description: "Monitor 5V output for undervoltage and overvoltage conditions";
        
        functional_interface {
            monitored_signal: "5V power rail";
            monitoring_range: 0V..6V;
            fault_conditions: {
                undervoltage: <4.5V;  // 10% below nominal
                overvoltage: >5.5V;   // 10% above nominal
            };
            fault_indication: active_low_signal_to_mcu;
        }
        
        performance_constraints {
            detection_time: ≤100µs;  // Before damage can occur
            accuracy: ±2%;            // Threshold accuracy
            false_positive_rate: <0.1%;  // Avoid nuisance trips
        }
        
        rationale: "100µs response prevents damage to 5V-powered components";
    }
}
```

## 3. Pattern-Based Template Generation

The tool recognizes common safety function patterns and generates appropriate templates:

### Pattern: "voltage monitoring"
```bhdl
template_pattern: voltage_monitoring {
    generates: {
        monitored_signal: "[voltage rail name]";
        fault_conditions: "[overvoltage/undervoltage thresholds]";
        fault_indication: "[signal type]";
        detection_time: "[response time]";
    }
}
```

### Pattern: "self-test"
```bhdl
template_pattern: self_test {
    generates: {
        test_interval: "[frequency]";
        test_coverage: "[percentage based on ASIL]";
        test_indication: "[pass/fail signal]";
        test_duration: "[max time]";
    }
}
```

### Pattern: "overcurrent protection"
```bhdl
template_pattern: overcurrent_protection {
    generates: {
        current_limit: "[threshold]";
        response_action: "[shutdown/foldback/limit]";
        recovery_method: "[auto/manual]";
        response_time: "[time to action]";
    }
}
```

## 4. Improved Workflow

### Step 1: Safety Engineer Defines Safety Functions
```bhdl
safety_functions {
    FSR1: {
        description: "Power supply output monitoring";
        implements: SG1;
        asil: ASIL_B;
        
        // NEW: Safety engineer provides hints
        monitoring_targets: {
            signal: VCC_5V;
            nominal: 5.0V;
            critical_deviation: ±10%;  // When it becomes a fault
        }
    }
}
```

### Step 2: Tool Generates Smarter Templates
```bhdl
// Tool uses hints to generate better template
generated_template {
    REQ_PSU_001: {
        // Pre-filled from hints
        monitored_signal: "VCC_5V rail";
        nominal_value: 5.0V;
        suggested_thresholds: {
            undervoltage: 4.5V;  // -10%
            overvoltage: 5.5V;   // +10%
        }
        
        // Still needs safety engineer input
        detection_time: "[SPECIFY: Based on damage analysis]";
        fault_indication: "[SPECIFY: Interface to safety controller]";
    }
}
```

### Step 3: Safety Engineer Reviews and Refines
```bhdl
// Safety engineer adjusts based on system analysis
refined_requirement {
    REQ_PSU_001: {
        monitored_signal: "VCC_5V rail";
        thresholds: {
            undervoltage: 4.75V;  // Tighter for critical components
            overvoltage: 5.5V;    // Kept at 10%
        }
        detection_time: ≤50µs;  // Based on component damage threshold
        fault_indication: active_low_open_drain_to_mcu;
    }
}
```

## 5. What the Tool CAN Generate Automatically

### Structural Elements
- Requirement ID and numbering
- Traceability links (source safety function)
- ASIL level inheritance
- Requirement categorization

### Standard Constraints (based on ASIL)
```bhdl
asil_based_constraints {
    ASIL_B: {
        diagnostic_coverage: ≥90%;
        latent_fault_coverage: ≥60%;
        self_test_interval: ≤100ms;  // Typical
    }
}
```

### Documentation Structure
```bhdl
requirement_documentation {
    id: auto_generated;
    source: traced_from_safety_function;
    asil: inherited;
    verification_method: [test, analysis, inspection];  // Template
    acceptance_criteria: "[TO BE SPECIFIED]";
}
```

## 6. What Requires Human Expertise

### Domain-Specific Values
- Voltage/current thresholds
- Timing requirements
- Accuracy specifications
- Interface details

### Safety Analysis Decisions
- What constitutes a "fault"
- How fast detection must be
- What safety action to take
- Recovery strategies

### System Context
- Interface to other systems
- Environmental considerations
- Operational constraints
- Degraded mode behavior

## Example: Complete Semi-Automated Flow

### Input: Safety Function
```bhdl
FSR_002: safety_function {
    description: "Overcurrent protection for critical loads";
    implements: SG2;
    asil: ASIL_B;
}
```

### Tool Output: Template
```bhdl
// GENERATED TEMPLATE - REQUIRES COMPLETION
REQ_PSU_002_TEMPLATE: requirement {
    source: "FSR_002";
    type: safety;
    asil: ASIL_B;
    
    description: "[COMPLETE: Specify overcurrent protection details]";
    
    // Pattern recognized: "overcurrent protection"
    current_protection {
        protected_circuit: "[SPECIFY: which circuit/component]";
        current_limit: "[SPECIFY: threshold in A]";
        response_time: "[SPECIFY: max time to protect]";
        protection_action: "[SELECT: shutdown/current_limit/foldback]";
        recovery: "[SELECT: auto_retry/manual_reset/latch_off]";
    }
    
    // Standard ASIL B requirements
    diagnostic: {
        self_test_capability: required;  // Auto-added for ASIL B
        diagnostic_coverage: ≥90%;        // Auto-added for ASIL B
    }
}
```

### Safety Engineer Completion
```bhdl
REQ_PSU_002: requirement {
    source: "FSR_002";
    type: safety;
    asil: ASIL_B;
    
    description: "Provide overcurrent protection for 5V rail at 3.5A with latching shutdown";
    
    current_protection {
        protected_circuit: "5V power rail to safety ECU";
        current_limit: 3.5A;  // 117% of 3A nominal
        response_time: ≤1ms;  // Before trace damage
        protection_action: shutdown;
        recovery: manual_reset;  // Requires investigation
    }
    
    diagnostic {
        self_test_capability: required;
        diagnostic_coverage: ≥90%;
        test_method: "Simulated overcurrent via test resistor";
    }
    
    rationale: "3.5A allows for inrush current while protecting against shorts";
}
```

## Benefits of Semi-Automated Approach

1. **Consistency**: All requirements follow standard templates
2. **Completeness**: Templates ensure nothing is forgotten
3. **Traceability**: Automatic linking to safety functions
4. **Efficiency**: Safety engineer focuses on values, not structure
5. **Quality**: Templates include best practices and reminders
6. **Flexibility**: Human expertise for critical decisions

## Conclusion

The realistic approach is:
- **Tool generates**: Structure, templates, traceability, standard constraints
- **Safety engineer specifies**: Values, thresholds, timing, interfaces
- **Result**: Complete, traceable, implementable requirements

This semi-automated approach provides the benefits of automation while preserving the critical role of safety engineering expertise in defining specific safety requirements.