# BHDL Clean-Room Review Synthesis
## Feedback from Three Independent Perspectives

**Date:** December 2024  
**Reviewers:** Three independent clean-room analyses focusing on practical board design workflow  
**Philosophy:** Make BHDL feel like "sketching with superpowers" - a natural extension of engineer thinking

---

## Executive Summary

Three independent reviews converged on the same core insight: **BHDL should bridge the natural progression from napkin sketch → schematic → PCB** that engineers already follow, rather than requiring adoption of a programmer's mindset.

The most impactful improvements focus on reducing friction during the initial "sketch" phase while maintaining the language's sophisticated electrical analysis capabilities.

---

## Priority 1: "Sketch Mode" Ergonomics (High Impact, Medium Effort)

### 1.1 Automatic Pin Resolution
**Problem:** During initial sketching, engineers think "resistor connects LED to ground," not about specific pin numbers.

**Proposed Solution:** Allow simplified connection syntax for obvious 2-pin components:
```bhdl
// Current (verbose):
@VCC -> Res(330Ω).1 -> LED(red).A;
LED(red).K -> @GND;

// Proposed "sketch mode":
@VCC -> Res(330Ω): -> LED(red): -> @GND;
// OR
@VCC -> Res(330Ω) -> LED(red) -> @GND;  // Auto-chain through 2-pin components
```

**Implementation:** The `:` suffix or auto-chaining triggers pin pair resolution (1↔2, A↔K) without explicit naming.

### 1.2 Pin-Bundle Connections
**Problem:** Related pins (like differential pairs) require multiple lines even when conceptually grouped.

**Proposed Solution:**
```bhdl
// USB differential pair in one line
usb_conn.(DP,DM) <-> mcu.(USB_DP, USB_DM);

// Multiple power pins
pmic.(VCC1,VCC2,VCC3) -> @VCC_3V3;
```

### 1.3 Informal Engineering Notation
**Problem:** Engineers use shorthand in emails/chat that should be recognized by the parser.

**Proposed Solution:** Accept common engineering abbreviations:
```bhdl
R(4k7)    → Res(4.7kΩ)     // Standard resistor notation
C(100n)   → Cap(100nF)     // Capacitor shorthand  
C(10u)    → Cap(10µF)      // Micro notation
3v3       → 3.3V           // Voltage shorthand
```

---

## Priority 2: Progressive Refinement (High Impact, High Effort)

### 2.1 Three-Stage Design Maturity
**Problem:** Real designs evolve from concept to production. The tool should support this progression.

**Proposed Solution:** CLI validation levels:
```bash
bhdl check --level=draft    # Syntax + gross safety, allows ? and TODO
bhdl check --level=review   # Full electrical rules, design review ready
bhdl check --level=locked   # Production-ready, no unresolved elements
```

### 2.2 Enhanced `?` Syntax for Design Intent
**Problem:** Often engineers know requirements but want the tool to calculate specific values.

**Proposed Solution:** Expressive constraint specification:
```bhdl
// Calculate resistance for specific current
@5V -> R(?for_current=20mA) -> LED(red, Vf=2.1V) -> @GND;

// Voltage divider with ratio constraint
@3V3 -> R1(?ratio=1.8/3.3) -> @1V8_REF -> R2(?) -> @GND;

// Component with requirements, specific part selected later
amp: OpAmp(PLACEHOLDER) {
    requirements: {
        bandwidth > 1MHz;
        supply_voltage = 5V;
        package = "SOIC8";
    }
}
```

### 2.3 Declarative TODOs and Design Questions
**Problem:** Design process involves questions and pending tasks that should be tracked formally.

**Proposed Solution:** Tool-aware comment system:
```bhdl
//? Why is this resistor 4.7kΩ?           // Design question
// TODO: Select footprint for connector    // Pending task  
//! This section is critical for EMI      // Important note
TODO("Pick inductor value for 100kHz")    // Explicit placeholder

// Linter can enforce: bhdl check --fail-on-unanswered-questions
```

---

## Priority 3: CAD Integration (High Impact, Medium Effort)

### 3.1 First-Class Net Classes
**Problem:** PCB net classes are critical for layout but awkward to specify in current constraint syntax.

**Proposed Solution:** Direct net class mapping:
```bhdl
netclass DDR_BUS {
    impedance = 50Ω ± 10%;
    matched_length = ±0.1mm to (CPU.DDR_CLK);
    diff_pair_gap = 0.15mm;
    via_count_max = 2;
}

// Apply to specific nets
mcu.ddr_bus in DDR_BUS;
[DQ0, DQ1, DQ2, DQ3] in DDR_BUS;
```

### 3.2 Visual Diff for Design Reviews
**Problem:** Design reviews need to see what changed between versions.

**Proposed Solution:**
```bash
bhdl visualize --diff previous.bhdl current.bhdl --output changes.svg
# Generates schematic-style diagram:
# - Green: Added components
# - Red: Removed components  
# - Yellow: Changed constraints
```

### 3.3 Schematic Import/Scaffold
**Problem:** Starting from scratch is harder than starting from existing design.

**Proposed Solution:** Lossy import that creates BHDL skeleton:
```bash
bhdl import --scaffold schematic.kicad_sch --output draft.bhdl
# Creates BHDL with TODO markers where semantics are unclear
# TODO("Could not determine if this is pullup or voltage divider")
# TODO("Found net 'I2C_SDA'. Is this part of an I2C interface?")
```

---

## Priority 4: Simulation & Validation (Medium Impact, High Effort)

### 4.1 Fast "Sim-Lite" Checking
**Problem:** Full SPICE simulation is too slow for iterative design during sketch phase.

**Proposed Solution:** Lightweight simulation for quick validation:
```bhdl
sim_lite {
    nodes = [@VCC_5V, @VCC_3V3, mcu.VDD];
    duration = 1ms;
    mode = "rc_approx";  // Simplified models
    run_on_save = true;  // IDE integration
}
```

### 4.2 Enhanced Physical Validation
**Problem:** Pin number vs. pin name confusion is common source of errors.

**Proposed Solution:** Smart linting with footprint awareness:
```
[WARNING] Pin Mapping Check
You connected LM7805.GND (logical pin) which maps to physical pin 2.
Standard KiCad footprint uses pin 3 for GND. 
Are you using a non-standard footprint or is this an error?
```

---

## Priority 5: Common Patterns (Medium Impact, Low Effort)

### 5.1 Built-in Circuit Pattern Functions
**Problem:** Same basic circuits (decoupling, LED indicators, pullups) are repeated constantly.

**Proposed Solution:** First-class pattern functions:
```bhdl
// Expand to standard decoupling (10µF + 0.1µF with appropriate placement)
mcu.VDD <- decouple_standard();

// Expand to current-limited LED with ground connection
@VCC -> led_indicator(green, current=2mA);

// Add pullups to I2C bus
i2c_bus <- pullup_standard(@VCC, 4.7kΩ);

// Voltage divider pattern
@5V -> voltage_divider(output=@3V3, current=1mA);
```

### 5.2 Personal Component Libraries
**Problem:** Engineers have preferred parts and default parameters.

**Proposed Solution:** Personal component shortcuts:
```bhdl
// In personal library file
alias MyResistor = Res(package="0603", tolerance=1%, power=0.1W);
alias MyLED = LED(package="0805", current=2mA);
alias MyDecoupling = [Cap(10µF, package="1206"), Cap(0.1µF, package="0603")];

// Usage in designs
@VCC -> MyResistor(4.7kΩ) -> MyLED(green) -> @GND;
```

---

## Priority 6: Syntax & Style Improvements (High Impact, Low Effort)

### 6.1 Semicolon Policy Clarification
**Current Issue:** Mixed examples in specification create confusion.

**Recommendation:** Establish firm policy:
- **Option A:** Semicolons required (recommended for cut-paste robustness)
- **Option B:** Semicolons optional but barred from all official examples

### 6.2 Ground Connection Shorthand
**Problem:** Explicit `-> @GND` is verbose for simple ground connections.

**Proposed Solution:**
```bhdl
LED.K -> ⏚;      // Ground symbol (Alt+23)
LED.K -> GND;    // Simple GND reference without @
```

### 6.3 Sheet/Grouping Construct
**Problem:** No lightweight way to group related circuits (schematic sheets).

**Proposed Solution:**
```bhdl
sheet PowerInput {
    // Input protection and filtering
    @12V_IN -> F1: Fuse(2A) -> @12V_PROT;
    @12V_PROT -> D1: TVS(15V) -> @GND;
}

sheet Regulation {
    // LDO circuit  
    @12V_PROT -> U1: LM7805() -> @5V_OUT;
}
```

---

## Implementation Roadmap

### Phase 1: Quick Wins (1-2 months)
- Informal unit notation (`4k7`, `10u`, `3v3`)
- Declarative comments (`//? TODO //!`)
- Semicolon policy clarification
- Ground symbol shorthand

### Phase 2: Core Ergonomics (3-6 months)
- Automatic pin resolution (`:` syntax)
- Pin bundle connections
- Enhanced `?` syntax for design intent
- Common pattern functions

### Phase 3: Advanced Features (6-12 months)
- Three-stage validation levels
- Net class system
- Sim-lite infrastructure
- Visual diff tools
- Schematic import/scaffold

### Phase 4: Polish (Ongoing)
- Personal component libraries
- Sheet grouping constructs
- Advanced linting and validation

---

## Success Metrics

1. **Time to First Circuit:** How long from installation to working LED blink example
2. **Sketch-to-Schematic Time:** Time to go from BHDL concept to importable netlist
3. **Error Resolution Time:** How quickly users can understand and fix common errors
4. **Design Review Efficiency:** How easily reviewers can understand changes and provide feedback

---

## Conclusion

The convergent feedback from three independent reviews confirms that BHDL's core architecture is sound. The proposed enhancements focus on reducing friction in the natural design workflow while preserving the language's analytical power.

The key insight is that BHDL should feel like an extension of how engineers already think about circuits, not a replacement for their mental model. Success will be measured by how quickly engineers can go from idea to working design, and how naturally the tool fits into their existing workflow. 