# Electrical Safety Analysis System - Design Plan

## Overview

The SPICE crate should provide comprehensive electrical safety analysis that detects dangerous conditions and either warns the user or automatically suggests/applies fixes. This system should be generic and extensible, not limited to specific components.

## Goals

1. **Detect dangerous electrical conditions** before they cause component damage
2. **Provide actionable warnings** with clear explanations
3. **Suggest or auto-apply fixes** when safe to do so
4. **Be extensible** for new safety rules
5. **Integrate seamlessly** with existing SPICE analysis

## Types of Electrical Hazards to Detect

### 1. Overcurrent Conditions
- Components without current limiting (LEDs, motors, etc.)
- Excessive current through any component beyond ratings
- Short circuits or near-short conditions
- Missing base resistors for transistors
- Unprotected MOSFET gates

### 2. Overvoltage Conditions
- Voltage exceeding component ratings
- Missing voltage regulation
- Reverse voltage without protection
- ESD-sensitive components without protection
- Logic level mismatches

### 3. Power Dissipation Issues
- Components exceeding power ratings
- Insufficient heatsinking
- Thermal runaway conditions
- Hot spots in PCB traces

### 4. Missing Protection
- No reverse polarity protection
- Missing flyback diodes for inductive loads
- No TVS/surge protection on inputs
- Unprotected power supply inputs
- Missing pull-up/pull-down resistors

### 5. Signal Integrity Issues
- Impedance mismatches
- Missing termination resistors
- Excessive rise/fall times
- Crosstalk risks

### 6. Power Supply Issues
- Missing decoupling capacitors
- Inadequate bulk capacitance
- Power sequencing violations
- Inrush current problems

## Proposed Architecture

### 1. Safety Rule Engine
```rust
pub trait SafetyRule {
    fn name(&self) -> &str;
    fn severity(&self) -> Severity;
    fn check(&self, circuit: &Circuit) -> Vec<SafetyViolation>;
    fn can_auto_fix(&self) -> bool;
    fn suggest_fix(&self, violation: &SafetyViolation) -> Option<CircuitModification>;
}

pub enum Severity {
    Info,
    Warning,
    Error,
    Critical, // Component damage likely
}

pub struct SafetyViolation {
    rule: String,
    severity: Severity,
    location: CircuitLocation,
    message: String,
    technical_details: String,
    user_impact: String, // What happens if not fixed
}
```

### 2. Circuit Modification System
```rust
pub enum CircuitModification {
    InsertComponent {
        component_type: String,
        value: Option<f64>,
        between: (NodeId, NodeId),
        reason: String,
    },
    ModifyComponent {
        instance: InstanceId,
        new_value: f64,
        reason: String,
    },
    AddProtection {
        protection_type: ProtectionType,
        target: ProtectionTarget,
        specifications: ProtectionSpec,
    },
}
```

### 3. Analysis Integration Points

#### Pre-Analysis Safety Check
- Detect obvious issues before running simulation
- Fast static analysis
- Structural checks (missing components, connections)

#### During-Analysis Monitoring
- Monitor convergence issues that indicate problems
- Detect numerical instabilities from bad circuits
- Track extreme values during simulation

#### Post-Analysis Validation
- Check all voltages/currents against limits
- Thermal analysis
- Transient behavior validation

## Implementation Strategy

### Phase 1: Core Framework
1. Define safety rule interface
2. Create modification system
3. Implement basic rule engine
4. Add integration points to SPICE analyzer

### Phase 2: Essential Safety Rules
1. **Current Limiting Rule**: Detect components needing current protection
2. **Voltage Rating Rule**: Check all components against voltage limits
3. **Power Dissipation Rule**: Calculate and validate power in all components
4. **Protection Circuit Rule**: Ensure proper protection circuits

### Phase 3: Advanced Rules
1. **Thermal Analysis Rule**: Estimate junction temperatures
2. **Signal Integrity Rule**: Check high-speed signals
3. **EMC Compliance Rule**: Basic EMC checks
4. **Power Sequencing Rule**: Validate startup/shutdown sequences

### Phase 4: Auto-Fix System
1. Component value calculation (resistors, capacitors)
2. Protection circuit insertion
3. Safe modification strategies
4. User approval workflow

## Example Use Cases

### Case 1: LED Without Resistor
```bhdl
// User writes:
VCC -> LED(red).A;

// System detects:
CRITICAL: LED 'LED1' connected directly to 5V supply
  - Estimated current: 2.5A (vs 20mA max)
  - LED will be destroyed immediately
  - Suggested fix: Insert 150Ω resistor

// Auto-fix produces:
VCC -> R_auto1: Res(150Ω).1 -> LED(red).A;
```

### Case 2: MOSFET Gate Protection
```bhdl
// User writes:
GPIO -> Q1: NMOS().G;

// System detects:
WARNING: MOSFET gate 'Q1.G' directly connected to GPIO
  - Risk of ESD damage
  - Possible oscillation
  - Suggested fix: Add 100Ω gate resistor

// Auto-fix produces:
GPIO -> R_gate1: Res(100Ω).1 -> Q1: NMOS().G;
```

### Case 3: Inductive Load Protection
```bhdl
// User writes:
Q1: NMOS().D -> relay1: Relay().1;
relay1.2 -> VCC;

// System detects:
ERROR: Inductive load 'relay1' without flyback protection
  - Voltage spike on turn-off could exceed 100V
  - Will damage MOSFET Q1
  - Suggested fix: Add flyback diode

// Auto-fix produces:
Q1: NMOS().D -> relay1: Relay().1;
relay1.2 -> VCC;
D_fly1: Diode().K -> relay1.1;
D_fly1.A -> relay1.2;
```

### Case 4: Decoupling Capacitors
```bhdl
// User writes:
VCC -> U1: MCU().VDD;

// System detects:
WARNING: IC 'U1' power pin without local decoupling
  - Risk of noise and instability
  - Suggested fix: Add 0.1µF capacitor

// Auto-fix produces:
VCC -> U1: MCU().VDD;
C_decouple1: Cap(0.1µF).1 -> U1.VDD;
C_decouple1.2 -> GND;
```

## Safety Analysis Workflow

```
1. Parse Circuit
      ↓
2. Pre-Analysis Safety Check
   - Structural validation
   - Obvious hazards
      ↓
3. Run SPICE Analysis
   - Monitor for issues
   - Collect data
      ↓
4. Post-Analysis Validation
   - Check all limits
   - Thermal analysis
      ↓
5. Generate Safety Report
   - Categorized by severity
   - Clear explanations
   - Suggested fixes
      ↓
6. [Optional] Apply Auto-Fixes
   - User approval required
   - Show before/after
   - Explain changes
```

## Integration with Existing System

### 1. Extend ComponentInferenceContext
- Add safety analysis results
- Track suggested modifications
- Store auto-fix proposals

### 2. New Analyzer Pass
- Pass 8: Electrical Safety Analysis
- Runs after SPICE synthesis
- Before final output generation

### 3. SPICE Crate Extensions
```rust
pub struct SafetyAnalysis {
    rules: Vec<Box<dyn SafetyRule>>,
    violations: Vec<SafetyViolation>,
    modifications: Vec<CircuitModification>,
}

impl Circuit {
    pub fn run_safety_analysis(&self) -> SafetyAnalysis {
        // Run all safety rules
    }
    
    pub fn apply_safety_modifications(&mut self, mods: Vec<CircuitModification>) {
        // Apply approved fixes
    }
}
```

## Configuration and Customization

### 1. Safety Levels
```toml
[safety]
level = "strict"  # strict, normal, permissive
auto_fix = false  # require approval
exclude_rules = []
custom_rules = ["./my_rules.rs"]
```

### 2. Component Derating
```toml
[safety.derating]
voltage = 0.8  # Use 80% of max voltage
current = 0.7  # Use 70% of max current  
power = 0.5    # Use 50% of max power
temperature = 0.8  # 80% of max temp
```

### 3. Rule Priorities
- Some rules can override others
- User can set rule priorities
- Critical rules always run

## Benefits

1. **Prevents costly mistakes** - Catch issues before manufacturing
2. **Educational** - Teaches good design practices
3. **Time-saving** - Automatic fix suggestions
4. **Flexible** - Extensible rule system
5. **Comprehensive** - Covers all electrical hazards

## Next Steps

1. Review and refine this plan
2. Prototype the safety rule interface
3. Implement core framework
4. Add first set of critical rules
5. Test with real circuits
6. Iterate based on feedback