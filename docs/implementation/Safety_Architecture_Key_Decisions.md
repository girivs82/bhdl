# Safety Architecture Key Decisions
## Summary of Design Choices and Rationale

### 1. Keyword Choice: `satisfies` vs `implements`

**Decision**: Use `satisfies` for compliance declarations

**Rationale**:
- Aligns with ISO 26262 terminology ("requirements are satisfied")
- More natural for requirements: "component satisfies requirement"
- Works for both functional and safety requirements
- Fits BHDL's declarative paradigm better than programming-oriented "implements"

**Example**:
```bhdl
component LTC2954 {
    satisfies [VoltageMonitoring, SelfTestable] {
        // Declaration of what this component satisfies
    }
}

board PowerSupply {
    satisfies {
        REQ_001: via monitor;  // Board satisfies requirement via component
    }
}
```

### 2. Separation of Intent vs Compliance

**Decision**: Keep `for` and `satisfies` completely separate

**Rationale**:
- `for` = operational intent (runtime behavior)
- `satisfies` = compliance declaration (design-time verification)
- Different purposes, scopes, and analysis methods
- Prevents confusion between what circuit does vs what it complies with

**Example**:
```bhdl
// Intent: What the signal path does during operation
net power: @12V -> reg -> @5V
    for voltage_regulation(efficiency: 85%);

// Compliance: What requirements are met
component LM2596 {
    satisfies VoltageRegulation;  // Separate from operational intent
}
```

### 3. Multi-Phase Architecture

**Decision**: Three-phase parallel development workflow

**Phases**:
1. **System Safety Analysis** (Day 1, no board needed)
2. **Board Implementation** (Parallel with Phase 1)
3. **Automatic Validation** (Continuous)

**Rationale**:
- Eliminates sequential bottleneck
- Safety work starts immediately
- Board designers have freedom
- Tool validates continuously

### 4. Component Libraries: Modes Only, No Effects

**Decision**: Libraries contain failure modes but NOT effects

**Rationale**:
- Effects are context-dependent
- Same component, different circuit = different effects
- Prevents library maintenance burden
- Enables accurate context-sensitive analysis

**Example**:
```bhdl
// Library: Only failure mode
failure_modes {
    no_switching: {
        rate: 6FIT;
        description: "PWM controller failure";
        observable_symptom: "0V output";
        // NO effect like "system shutdown"
    }
}

// Tool generates context-specific effect
context_safety_ecu: {
    generated_effect: "Safety ECU loses power";  // Critical
}
context_led_driver: {
    generated_effect: "LEDs turn off";  // Minor
}
```

### 5. SEooC FIT Rate Decomposition

**Decision**: Tool decomposes vendor aggregate FIT rates

**Approach**:
- Vendor provides total FIT (e.g., 23FIT)
- Tool decomposes into die/package/transient
- Uses industry ratios when vendor doesn't specify
- Enables detailed failure mode analysis

### 6. Semi-Automated Requirement Generation

**Decision**: Templates + manual refinement, not full automation

**Rationale**:
- Tool cannot determine domain-specific values
- Safety engineer expertise is essential
- Templates ensure consistency and completeness
- Manual refinement adds critical details

**Example**:
```bhdl
// Tool generates template
template: {
    monitoring {
        signal: "[SPECIFY: voltage/current/temp]";
        threshold: "[SPECIFY: value and tolerance]";
    }
}

// Engineer completes
completed: {
    monitoring {
        signal: "5V power rail";
        threshold: 5.5V ± 2%;
    }
}
```

### 7. External Safety Mechanisms

**Decision**: Circuits (not just components) can satisfy requirements

**Rationale**:
- Many components lack built-in safety
- External circuits can add safety features
- Enables cost optimization
- Supports legacy component reuse

**Example**:
```bhdl
circuit_fragment MonitoringCircuit {
    components { LM393, TL431, resistors }
    connections { /* voltage divider + comparator */ }
    
    // Circuit satisfies capability
    satisfies VoltageMonitoring {
        threshold: 5.5V;
        response_time: 10µs;
    }
}
```

### 8. Minimal Interface Requirements

**Decision**: Trust board designers, don't over-specify

**What NOT to specify**:
- Drive capability
- Logic levels
- Pull-up/pull-down values
- Exact electrical characteristics

**What TO specify**:
- Functional requirements only
- Response times
- Coverage targets
- Interface type (digital/analog)

### 9. Validation Through Declaration

**Decision**: Board declares what it satisfies, tool validates

**Process**:
1. Board declares: `satisfies { REQ_001: via monitor; }`
2. Tool checks: Does monitor actually satisfy REQ_001?
3. Tool validates: Component capabilities vs requirements
4. Tool reports: PASS/FAIL with specific gaps

### 10. No Mixed Semantics

**Decision**: Don't mix safety attributes with operational code

**Keep Separate**:
- Board design (components, connections, intents)
- Safety declarations (satisfies blocks)
- Component definitions (electrical, behavioral, failure modes)
- Safety analysis (generated by tool)

## Summary

These decisions create a clean, maintainable architecture that:
- Separates concerns properly
- Enables parallel development
- Provides clear semantics
- Supports flexible implementation strategies
- Maintains ISO 26262 compliance
- Scales to complex systems

The `satisfies` keyword provides the critical link between requirements and implementation, while maintaining clear separation from operational intent captured by `for`.