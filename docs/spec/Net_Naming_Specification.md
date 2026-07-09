> **Historical (2026-02) — core doctrine current, verify specifics against
> the main spec (2026-07-09).** The handle/net separation described here (`@` =
> net, `:` = handle, distinct namespaces) is correct and shipped, and the
> `@NAME->` named-net form still parses. But the `@` prefix is **optional** on
> a declared rail (the corpus writes bare `VIN -> …`), and named nets are more
> commonly introduced by a bare identifier (`R1.2 -> fb_node`). The current
> reference is [BHDL_Complete_Specification.md](BHDL_Complete_Specification.md) §3.2.

# BHDL v2.0 Net Naming Specification

## Overview

This document specifies the net naming syntax for BHDL v2.0, introducing explicit disambiguation between component handles and net names using the `@` prefix.

## Motivation

In the original BHDL v2.0 specification, the syntax `handle: Component().pin` created both:
1. A component instance with handle "handle"
2. A net named "handle"

This dual purpose created ambiguity and implementation complexity. The new syntax clearly separates these concepts.

## Syntax Rules

### 1. Anonymous Nets

Connections without explicit net names create anonymous nets:

```bhdl
VCC -> r1: Res(10k).1;
r1.2 -> led: LED(red).A;
led.K -> GND;
```

The synthesizer assigns unique internal names to these nets (e.g., `$net_1`, `$net_2`).

### 2. Named Nets

Named nets are created and referenced using the `@` prefix:

```bhdl
// Create a named net
VCC @FILTERED_5V-> r1: Res(10k).1;

// Reference a named net (ALWAYS with @)
@FILTERED_5V -> c1: Cap(100n).1;
@FILTERED_5V -> c2: Cap(10u).1;
```

### 3. Component Handles

Component handles use the `:` syntax and are ONLY component references:

```bhdl
// r1 is ONLY a component handle, not a net name
VCC -> r1: Res(10k).1;

// Reference component pins
r1.2 -> led.A;  // Using component handles
```

### 4. Disambiguation Examples

```bhdl
// Clear distinction between nets and components
r1.2 -> led.A;        // r1, led = component handles
@FILTERED_5V -> r1.1; // FILTERED_5V = net, r1 = component
fuse.2 -> @PROTECTED; // fuse = component, PROTECTED = net
@RAW -> @FILTERED;    // Both are nets
```

## Grammar Changes

### Current Grammar (to be deprecated)
```
connection_stmt = flow_expr ";"
flow_expr = flow_element ("->" flow_element)*
flow_element = net_ref | pin_ref | component_instantiation

// Creates both component handle and net
component_instantiation = ident ":" component_inst
```

### New Grammar
```
connection_stmt = flow_expr ";"
flow_expr = flow_element (arrow flow_element)*
arrow = "->" | named_arrow
named_arrow = "@" ident "->"
flow_element = net_ref | pin_ref | component_instantiation

// Net references MUST have @ prefix
net_ref = "@" ident

// Component instantiation ONLY creates handle
component_instantiation = ident ":" component_inst
```

## Semantic Rules

1. **Net Creation**: A net is created when first referenced with `@NAME->`
2. **Net Reference**: All subsequent uses must include `@` prefix
3. **Component Handles**: Created with `:` syntax, referenced without prefix
4. **Anonymous Nets**: Created by `->` without `@NAME`
5. **Net Names**: Must be valid identifiers (alphanumeric + underscore)
6. **Case Sensitivity**: Net names are case-sensitive

## Examples

### Basic Circuit
```bhdl
board SimpleLED {
    power VCC = 5V @ 100mA;
    ground GND;
    
    // Anonymous net from VCC to resistor
    VCC -> r1: Res(330Ω).1;
    
    // Named net for LED drive
    r1.2 @LED_DRIVE-> led: LED(red).A;
    
    // Can add test point to named net
    @LED_DRIVE -> tp: TestPoint.1;
    
    led.K -> GND;
}
```

### Power Supply with Named Nets
```bhdl
board PowerSupply {
    power VIN = 12V @ 2A;
    ground GND;
    
    // Protection stage
    VIN @RAW-> fuse: Fuse(2A).1;
    fuse.2 @FUSED-> tvs: TVSDiode(15V).K;
    tvs.A -> GND;
    
    // Regulation stage
    @FUSED -> c_in: Cap(10uF).pos;
    c_in.neg -> GND;
    @FUSED -> reg: LM7805.IN;
    reg.GND -> GND;
    reg.OUT @5V-> c_out: Cap(100uF).pos;
    c_out.neg -> GND;
    
    // Distribution
    @5V -> conn: Header_1x3.1;  // Power out
    GND -> conn.2;              // Ground out
    @5V -> conn.3;              // Second power pin
}
```

### Mixed Named and Anonymous
```bhdl
board MixedNets {
    power VCC = 3.3V @ 500mA;
    ground GND;
    
    // Anonymous net for simple connection
    VCC -> r1: Res(10k).1;
    
    // Named net where we need multiple connections
    r1.2 @PULLUP-> btn: Button.1;
    @PULLUP -> mcu: MCU.GPIO1;
    btn.2 -> GND;
    
    // Anonymous for direct connections
    mcu.GPIO2 -> led: LED(blue).A;
    led.K -> r2: Res(220Ω).1;
    r2.2 -> GND;
}
```

## Migration Guide

### Old Syntax
```bhdl
// Component handle 'r1' also creates net 'r1'
VCC -> r1: Res(10k).1;
r1.2 -> led: LED(red).A;

// Ambiguous: is FILTERED a net or component?
FILTERED -> cap: Cap(100n).1;
```

### New Syntax
```bhdl
// Component handle 'r1', anonymous net
VCC -> r1: Res(10k).1;
r1.2 -> led: LED(red).A;

// Clear: @FILTERED is a net
@FILTERED -> cap: Cap(100n).1;
```

## Benefits

1. **Unambiguous**: Always clear what is a net vs component
2. **Explicit**: Net names are intentionally chosen, not implicit
3. **Searchable**: Can grep for `@NETNAME` to find all net uses
4. **Compatible**: Doesn't break component handle syntax
5. **Readable**: `@` reads as "at" - "VCC at RAW net connects to..."

## Implementation Notes

1. Parser must recognize `@ident` as net reference
2. Parser must recognize `@ident->` as named arrow
3. Analyzer must track nets separately from component handles
4. Synthesizer must not create nets from component handles
5. Net names and component handles are in separate namespaces