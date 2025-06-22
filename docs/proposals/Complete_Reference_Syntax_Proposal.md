# Complete Reference Syntax Proposal for BHDL

## The Problem

BHDL currently has inconsistent syntax for referencing different entities:

1. **Power/Ground domains**: `VCC`, `GND` (no special syntax)
2. **Named nets**: Sometimes `@name`, sometimes just `name`  
3. **Components**: `handle.pin`
4. **Ambiguity**: Is `VCC` a power domain or a net named VCC?

This creates confusion:
```bhdl
power VCC = 5V @ 1A;
VCC -> Res(10k).1;        // VCC is power domain
reg.OUT -> VCC;           // VCC is... what exactly?
```

## Proposed Solution: Consistent Prefixes

### Core Principle: Every Reference Has a Clear Type

1. **Power/Ground domains**: Use `$` prefix
2. **Named nets**: Use `@` prefix  
3. **Components**: Use handle.pin (no prefix)

### Syntax Rules

```bhdl
// Declarations
power VCC = 5V @ 1A;      // Declare power domain VCC
ground GND;               // Declare ground domain GND

// References - ALWAYS use prefix
$VCC -> Res(10k).1;       // Reference power domain
reg.OUT -> $VCC;          // Connect to power domain
led.K -> $GND;            // Connect to ground domain

// Named nets - ALWAYS use @
$VCC -> @filtered -> amp.IN;    // Create net @filtered
@filtered -> Cap(100n).1;       // Reference net @filtered

// Components - no prefix
r1: Res(10k)              // Create component handle
r1.2 -> led.A;            // Reference component pin
```

## Complete Example

```bhdl
board ClearSyntaxExample {
    // Declarations
    power VCC = 5V @ 1A;
    power VCC_3V3 = 3.3V @ 500mA;
    ground GND;
    
    // Input protection - clear what's what
    $VCC -> fuse: Fuse(1A).1;
    fuse.2 -> @protected -> tvs: TVSDiode(5.5V).1;
    tvs.2 -> $GND;
    
    // Voltage regulation - no ambiguity
    @protected -> reg: LM7805().IN;
    reg.GND -> $GND;              // GND pin to ground domain
    reg.OUT -> $VCC;              // Output to power domain
    
    // Filtering with clear net creation
    $VCC -> @main_rail -> bulk: Cap(100uF).+;
    @main_rail -> ceramic: Cap(0.1uF).1;
    bulk.- -> $GND;
    ceramic.2 -> $GND;
    
    // Multiple power domains - crystal clear
    $VCC -> reg_3v3: Regulator(3.3V).IN;
    reg_3v3.OUT -> $VCC_3V3;
    reg_3v3.GND -> $GND;
    
    // LED indicator - obvious what connects to what
    $VCC_3V3 -> r_led: Res(330).1;
    r_led.2 -> led: LED(green).A;
    led.K -> $GND;
}
```

## Alternative: Keyword-Based (More Verbose)

```bhdl
// If $ seems too cryptic, use keywords
power(VCC) -> Res(10k).1;
net(@filtered) -> amp.IN;
ground(GND) -> cap.2;
```

But this is more verbose and less flow-like.

## Benefits of $ Prefix

1. **Unambiguous**: You always know if it's a power domain, net, or component
2. **Searchable**: Easy to find all power/ground connections with grep
3. **Visual**: Power rails stand out in the code
4. **Consistent**: Every entity type has its own clear syntax
5. **No conflicts**: Can have net @VCC and power $VCC without confusion

## Migration Path

### Phase 1: Accept Both
```bhdl
VCC -> ...;      // Deprecated warning
$VCC -> ...;     // Preferred
```

### Phase 2: Require $ for Power/Ground
```bhdl
VCC -> ...;      // Error: Did you mean $VCC?
$VCC -> ...;     // Required
```

## Complete Reference Guide

| Entity Type | Declaration | Reference | Example |
|------------|-------------|-----------|----------|
| Power domain | `power VCC = 5V @ 1A` | `$VCC` | `$VCC -> Res(1k).1` |
| Ground domain | `ground GND` | `$GND` | `cap.2 -> $GND` |
| Named net | Created inline | `@name` | `@filtered -> amp.IN` |
| Component | `handle: Type()` | `handle.pin` | `r1.2 -> led.A` |
| Anonymous net | None (implicit) | N/A | `-> Res(1k).1 ->` |

## Intent Integration

With clear syntax, intent attachment is natural:

```bhdl
// Intent on flows with clear entity types
protection: $VIN -> fuse.1 -> @protected for overvoltage_protection;

// No ambiguity about what receives the intent
filtering: $VCC -> @filtered for noise_immunity -> load;
```

## Summary

- **$** for power/ground domains: `$VCC`, `$GND`, `$VIN`
- **@** for named nets: `@filtered`, `@protected`
- **No prefix** for components: `r1.2`, `amp.OUT`

This creates a consistent, unambiguous syntax where every reference clearly indicates its type.