# Simplified Net Syntax Proposal for BHDL

## Key Insight

Power domains are just nets with attributes. They're not a separate entity type!

```bhdl
power VCC = 5V @ 1A;     // This declares a net named VCC with attributes
ground GND;              // This declares a net named GND with ground attribute
```

## Simplified Syntax Proposal

Since power domains are just nets, we should use consistent @ syntax:

### Option 1: Always Use @ for All Nets

```bhdl
board SimplifiedExample {
    power VCC = 5V @ 1A;        // Declares net VCC
    ground GND;                 // Declares net GND
    
    // Always use @ to reference ANY net
    @VCC -> Res(10k).1 -> led: LED(red).A;
    led.K -> @GND;
    
    // Create other nets inline
    @VCC -> @filtered -> amp: OpAmp().IN+;
    amp.V- -> @GND;
    
    // Everything is consistent!
    @filtered -> Cap(100n).1 -> @GND;
}
```

### Option 2: Special Declaration, Normal Reference

```bhdl
board AlternativeExample {
    power VCC = 5V @ 1A;        // Special declaration
    ground GND;                 // Special declaration
    
    // But reference without @ since they're pre-declared
    VCC -> Res(10k).1;          // OK - VCC was declared
    
    // Inline nets need @ to create
    VCC -> @filtered -> amp.IN;  // Creates net 'filtered'
    
    // But this is inconsistent...
    @filtered -> Cap(100n).1;    // Needs @ because not pre-declared
    GND -> Cap(100n).2;         // No @ because pre-declared
}
```

## Recommendation: Option 1 - Always Use @

**All nets use @ prefix, regardless of how they're declared:**

```bhdl
board ConsistentExample {
    // Declarations
    power VCC = 5V @ 1A;        // Declares net with power attributes
    power VCC_3V3 = 3.3V;       // Another power net
    ground GND;                 // Declares net with ground attribute
    
    // ALL net references use @
    @VCC -> fuse: Fuse(1A).1;
    fuse.2 -> @protected -> tvs: TVSDiode(5.5V).K;
    tvs.A -> @GND;
    
    // Clean and consistent
    @protected -> reg: LM7805().IN;
    reg.OUT -> @VCC_3V3;
    reg.GND -> @GND;
    
    // No ambiguity anywhere
    @VCC_3V3 -> @clean -> load;
    @clean -> Cap(0.1uF).1 -> @GND;
}
```

## Benefits

1. **Conceptual Clarity**: Power domains are just nets with metadata
2. **Syntax Consistency**: @ always means net, everywhere
3. **No Special Cases**: Don't need different syntax for power vs other nets
4. **Easier to Learn**: One rule: @ for nets, : for components
5. **Tooling Simplicity**: Parser doesn't need special power/ground handling

## Examples Showing Consistency

### Mixed Nets and Components
```bhdl
@VCC -> r1: Res(10k).1;        // @ for net, : for component
r1.2 -> @signal;               // Component pin to net
@signal -> amp: OpAmp().IN+;   // Net to component
amp.OUT -> @output;            // Component to net
@output -> @filtered;          // Net to net
```

### Power Distribution
```bhdl
// All power rails are just nets
@VIN -> @protected -> reg: Regulator().IN;
reg.OUT -> @VCC_5V;
@VCC_5V -> @VCC_3V3 -> @VCC_1V8;  // Chained power nets
```

### With Intent
```bhdl
// Intent on any net creation or flow
main_power: @VIN -> @protected for input_protection(15V);
@protected -> reg.IN for safety_critical;
```

## Counter-argument: Pre-declared Nets

One might argue that pre-declared nets (power/ground) shouldn't need @:

```bhdl
power VCC = 5V;
VCC -> Res(10k).1;      // VCC is pre-declared, so no @ needed?
```

But this creates two classes of nets:
- Pre-declared: No @ needed
- Inline-created: @ required

This inconsistency is confusing. Better to have one rule: **all nets use @**.

## Implementation

1. Parser treats `power` and `ground` as net declarations with attributes
2. All net references require @ prefix
3. Symbol table stores power/ground attributes on net symbols
4. No separate "power domain" concept needed

## Summary

- Power domains ARE nets, not a separate entity type
- Use @ consistently for ALL net references
- Simplifies mental model and syntax
- Makes BHDL easier to learn and use

This proposal eliminates an entire category of entities (power domains) by recognizing they're just nets with attributes, leading to a simpler, more consistent language.