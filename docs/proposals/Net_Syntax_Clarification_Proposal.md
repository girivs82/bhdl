# BHDL Net Syntax Clarification Proposal

## Problem Statement

The current BHDL syntax has multiple, inconsistent ways to create and reference nets:

1. **@ prefix syntax**: `VCC @FILTERED-> r1: Res(10k).1`
2. **Net assignment pattern**: `fuse.2 -> protected_vin: TVSDiode(15V).K`
3. **Component handles**: `r1: Res(10k)` 
4. **Anonymous nets**: `VCC -> Res(10k).1`

This creates confusion because:
- The `:` operator is used for BOTH component handles AND net names (context-dependent)
- Pattern #2 isn't even documented in the specification
- It's unclear when `name:` creates a component vs a net
- Users (and implementers) get confused about what creates what

## Proposed Solution: Clear, Consistent Syntax

### Core Principle: One Symbol, One Meaning

**Component handles use `:`**
```bhdl
VCC -> r1: Res(10k).1;        // Creates component handle "r1"
r1.2 -> led: LED(red).A;      // Reference component via handle
```

**Named nets use `@` exclusively**
```bhdl
VCC -> @filtered -> r1: Res(10k).1;     // Creates net @filtered in flow
@filtered -> c1: Cap(100n).1;           // Reference net with @
```

**Anonymous nets need no syntax**
```bhdl
VCC -> Res(10k).1 -> LED(red).A;       // Direct connections
```

### Eliminate Confusing Pattern

Remove the ambiguous pattern:
```bhdl
// REMOVE THIS PATTERN - it's confusing!
fuse.2 -> protected_vin: TVSDiode(15V).K;
```

Replace with clear @ syntax:
```bhdl
// CLEAR: @ means net
fuse.2 -> @protected_vin -> TVSDiode(15V).K;
```

## Complete Syntax Rules

### 1. Component Instantiation and Handles

```bhdl
// Anonymous component (no handle)
VCC -> Res(10k).1;                      

// Named component with handle
VCC -> r1: Res(10k).1;                  // r1 is component handle
r1.2 -> LED(red).A;                     // Reference via handle

// Multiple components with handles
sensor -> amp: OpAmp().IN+;
amp.OUT -> filter: LowPass(1kHz).IN;
filter.OUT -> adc: ADC().IN;
```

### 2. Net Creation and Reference

```bhdl
// Create net in flow with @name
VCC -> @main_rail -> Res(10k).1;       // Creates net @main_rail

// Reference net with @name
@main_rail -> Cap(100n).1;             // Must use @ for nets
@main_rail -> Cap(10uF).+;             // Multiple connections

// Nets at any point in flow
VCC -> fuse: Fuse(1A).1;
fuse.2 -> @protected -> reg: LM7805().IN;
reg.OUT -> @regulated_5v -> load;
```

### 3. Component Pins vs Nets

```bhdl
// Component.pin references
r1.2        // Pin 2 of component r1
led.A       // Anode of component led
amp.OUT     // Output pin of component amp

// Net references (always with @)
@filtered   // Net named filtered
@VCC_3V3    // Net named VCC_3V3
@protected  // Net named protected
```

## Intent Integration

With clear syntax, intent attachment becomes natural:

### Intent on Named Flows
```bhdl
// Flow name uses : (it's like a label, not a component)
power_path: VCC -> @protected -> reg.IN for safety_critical;
```

### Intent on Direct Connections  
```bhdl
// Anonymous flow with intent
VCC -> Res(10k).1 -> LED(red).A for indicator(purpose: power_on);
```

### Intent on Named Nets
```bhdl
// Net creation with intent
VCC -> @filtered for noise_rejection(20dB) -> amp.IN;

// Or at the end of net usage
sensor -> @signal -> filter.IN for anti_alias(before: adc);
```

## Benefits

1. **Clarity**: @ always means net, : always means component handle
2. **Consistency**: One symbol, one meaning throughout
3. **Readability**: `@protected` clearly reads as "the protected net"
4. **No Ambiguity**: You can never confuse nets and components
5. **Flow-Based**: Still natural left-to-right reading

## Migration Examples

### Before (Confusing)
```bhdl
// Is protected_vin a net or component? Unclear!
fuse.2 -> protected_vin: TVSDiode(15V).K;
protected_vin -> Cap(100uF).+;  // This suggests it's a net
// But TVSDiode.A is disconnected - can't reference it!
```

### After (Clear)
```bhdl
// Clearly a net with @
fuse.2 -> @protected_vin -> tvs: TVSDiode(15V).K;
tvs.A -> GND;                    // Can reference component
@protected_vin -> Cap(100uF).+;  // Clearly referencing net
```

## Implementation Changes

1. **Parser**: Remove the confusing net assignment pattern
2. **Documentation**: Update spec to clearly explain @ for nets only
3. **Examples**: Update all examples to use consistent syntax
4. **Error Messages**: Clear errors when @ is missing for nets

## Summary

- **Component handles**: `name: Component()` (uses `:`)
- **Net names**: `@name` (always uses `@`)
- **No ambiguity**: You always know what you're creating/referencing
- **Flow-based**: Natural left-to-right reading preserved
- **Intent-ready**: Clear syntax makes intent attachment obvious

This proposal eliminates confusion while maintaining BHDL's flow-based philosophy.