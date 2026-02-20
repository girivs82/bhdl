# BHDL Syntax Decisions Summary

## Final Syntax Rules

### 1. Net References - Always Use @

**All nets must be referenced with @ prefix**, regardless of how they were declared:

```bhdl
// Power/ground declarations create nets
power VCC = 5V @ 1A;        // Creates net named VCC
ground GND;                  // Creates net named GND

// ALL net references use @
@VCC -> Res(10k).1 -> led: LED(red).A;
led.K -> @GND;

// Named nets created inline
@VCC -> @filtered -> amp: OpAmp().IN+;
@filtered -> Cap(100n).1 -> @GND;
```

**Rationale**: 
- Consistent syntax: @ always means net
- No ambiguity between nets, components, and other entities
- Power/ground are just nets with special attributes

### 2. Component Handles - Use : Only

**The : operator is exclusively for component handles**:

```bhdl
// Create component with handle
@VCC -> r1: Res(10k).1;

// Reference component via handle  
r1.2 -> led: LED(red).A;

// Anonymous components (no handle)
@VCC -> Res(4.7k).1 -> LED(blue).A;
```

**Rationale**:
- Clear distinction: : means component handle
- No confusion with other uses of :

### 3. No Connection Labels

**Remove the label: syntax for connections**:

```bhdl
// DON'T DO THIS - confusing
protection: @VIN -> fuse.1 -> @protected;

// DO THIS - clear and simple
@VIN -> fuse.1 -> @protected for overvoltage_protection;
```

**Rationale**:
- Labels add confusion (is protection a component?)
- Intent clauses already document purpose
- Comments can provide additional documentation

### 4. Intent Syntax - No 'net' Keyword

**Attach intents using 'for' on flows and connections**:

```bhdl
// Intent on named flow (if we keep flow names, use different syntax)
flow power_path = @VIN |> protection |> regulation;

// Intent on direct connection
@VIN -> fuse: Fuse(2A).1 -> @protected for overvoltage_protection;

// Intent on anonymous flow
@VCC -> Res(10k).1 -> LED(red).A for power_indicator;
```

**Rationale**:
- Maintains flow-based philosophy
- No declarative 'net' keyword needed
- Natural left-to-right reading

### 5. Power/Ground Keywords Stay

**Keep 'power' and 'ground' keywords for declarations**:

```bhdl
board Example {
    // Clear power infrastructure
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    ground GND;
    
    // But reference with @
    @VIN -> @VCC;
    @VCC -> @GND;
}
```

**Rationale**:
- Self-documenting power infrastructure
- Clear board structure at a glance
- Power domains are nets with attributes

## Complete Example

```bhdl
board ClearSyntaxExample {
    // Power declarations (create nets with attributes)
    power VIN = 12V @ 2A;
    power VCC = 5V @ 1A;
    power VCC_3V3 = 3.3V @ 500mA;
    ground GND;
    ground AGND;
    
    // All nets use @ for reference
    @VIN -> fuse: Fuse(2A).1;
    fuse.2 -> @protected -> tvs: TVSDiode(15V).K;
    tvs.A -> @GND;
    
    // Intent without labels or 'net' keyword
    @protected -> reg: LM7805().IN for voltage_regulation;
    reg.OUT -> @VCC;
    reg.GND -> @GND;
    
    // Component handles use :
    @VCC -> r1: Res(330).1;
    r1.2 -> led: LED(green).A;
    led.K -> @GND;
    
    // Anonymous components
    @VCC_3V3 -> Res(10k).1 -> @pulled_up;
    
    // Multiple connections to same net
    @VCC -> Cap(100uF).+ -> @GND;  // Bulk cap
    @VCC -> Cap(0.1uF).1 -> @GND;  // Bypass cap
}
```

## Summary Table

| Syntax | Purpose | Example |
|--------|---------|---------|
| `@name` | Net reference | `@VCC`, `@filtered`, `@GND` |
| `handle:` | Component handle | `r1: Res(10k)`, `amp: OpAmp()` |
| `handle.pin` | Component pin | `r1.2`, `amp.OUT` |
| `for intent` | Intent clause | `for overvoltage_protection` |
| `power/ground` | Net declaration | `power VCC = 5V @ 1A` |

## Key Principles

1. **One symbol, one meaning**: @ for nets, : for components
2. **No ambiguity**: Every reference clearly shows its type
3. **Flow-based**: No declarative keywords beyond power/ground
4. **Consistent**: Same rules everywhere in the language

This creates a clean, intuitive syntax that's easy to learn and impossible to misuse.