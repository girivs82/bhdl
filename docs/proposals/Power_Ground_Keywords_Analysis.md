# Power/Ground Keywords: Keep or Remove?

## Current Syntax
```bhdl
power VCC = 5V @ 1A;
ground GND;
```

## Option 1: Remove Keywords (Pure Nets)

```bhdl
@VCC = 5V @ 1A;           // Net with power attributes
@GND = ground;            // Net with ground attribute

// Or more explicit
@VCC: power(5V, 1A);
@GND: ground;
```

### Pros
- **Ultimate consistency**: Everything is just nets with attributes
- **One declaration syntax**: `@name = attributes`
- **Simpler parser**: No special keywords to handle

### Cons  
- **Less readable**: Loses the immediate clarity of "power VCC"
- **Not self-documenting**: Have to infer it's a power rail from attributes
- **More typing**: @ prefix even in declarations

## Option 2: Keep Keywords (Current)

```bhdl
power VCC = 5V @ 1A;      // Clear power declaration
ground GND;               // Clear ground declaration

// But reference with @
@VCC -> Res(10k).1;
@GND -> cap.2;
```

### Pros
- **Self-documenting**: `power` and `ground` immediately convey intent
- **Natural reading**: "power VCC equals 5 volts at 1 amp"
- **Clear board structure**: Power section stands out visually
- **Matches mental model**: Designers think "I need to declare my power rails"

### Cons
- **Slight inconsistency**: Declaration doesn't use @, reference does
- **Two concepts**: "Power declaration" vs "net with power attributes"

## Option 3: Hybrid Clarity

```bhdl
power @VCC = 5V @ 1A;     // Explicit: power net named VCC
ground @GND;              // Explicit: ground net named GND

// Reference consistently
@VCC -> Res(10k).1;
@GND -> cap.2;
```

### Pros
- **Best of both**: Clear intent AND consistent @ usage
- **No ambiguity**: Obviously creates a net named VCC
- **Teaching advantage**: "power @VCC creates a power net"

### Cons
- **Redundant?**: The @ might feel redundant with the keyword

## Recommendation: Keep Keywords (Option 2)

The `power` and `ground` keywords should stay because:

1. **Board-Level Clarity**: When you open a BHDL file, you immediately see:
   ```bhdl
   board PowerSupply {
       power VIN = 12V @ 2A;
       power VCC = 5V @ 1A;
       power VCC_3V3 = 3.3V @ 500mA;
       ground GND;
       ground AGND;
   ```
   This is incredibly clear and matches how engineers think.

2. **Semantic Meaning**: They're not just nets - they're THE power infrastructure
   
3. **Tool Intelligence**: Tools can immediately identify power structure for:
   - DRC checks
   - Power analysis
   - Visualization (power rails shown differently)
   - Documentation generation

4. **Natural Language**: "Declare power VCC at 5 volts" is clearer than "Create net VCC with 5V attribute"

## Implementation Approach

```bhdl
// Declaration - keywords for clarity
power VCC = 5V @ 1A;      // Creates net @VCC with power attributes
ground GND;               // Creates net @GND with ground attribute

// Reference - @ for consistency
@VCC -> reg.IN;           // Clear: referencing the VCC net
reg.OUT -> @VCC_3V3;      // Clear: referencing the VCC_3V3 net
transistor.E -> @GND;     // Clear: referencing the GND net
```

## Syntax Rule Summary

1. **Declarations use keywords**: `power NAME = spec;` and `ground NAME;`
2. **References use @**: `@NAME` everywhere
3. **Mental model**: "power VCC" declares a net named VCC with power attributes

This gives us:
- Clear, self-documenting board structure
- Consistent @ usage for all net references  
- Simple rule: "Declare with keywords, reference with @"

## Example

```bhdl
board ClearExample {
    // Power infrastructure - immediately visible
    power VIN = 12V @ 2A;
    power VCC_5V = 5V @ 1A;  
    power VCC_3V3 = 3.3V @ 500mA;
    ground GND;
    ground AGND;
    
    // Connections - all nets use @
    @VIN -> fuse: Fuse(2A).1;
    fuse.2 -> @protected -> reg: LM7805().IN;
    reg.OUT -> @VCC_5V;
    reg.GND -> @GND;
    
    // Clear what's a net vs component
    @VCC_5V -> r1: Res(10k).1;  // @VCC_5V is net, r1 is component
    r1.2 -> led: LED(red).A;    // r1 is component
    led.K -> @GND;              // @GND is net
}
```

The keywords make the code more readable and self-documenting while @ references maintain consistency.