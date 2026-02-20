# Consistent Port Mapping Syntax - Alternative to Dot Notation

## The Proposal

Always put entity pins on the LEFT side of connections, parent signals/pins on the RIGHT:

```bhdl
instance_name: ModuleType {
    PIN -> signal;      // Module input pin receives from parent signal
    PIN <- signal;      // Module output pin sends to parent signal  
    PIN <-> signal;     // Module bidirectional pin connects to parent signal
}
```

## Why This Works

### 1. Complete Disambiguation
```bhdl
entity PowerSystem {
    signal VIN;
    signal VOUT;
    
    reg1: VoltageRegulator {
        VIN <- VIN;      // Clear: module's VIN pin gets parent's VIN signal
        VOUT -> VOUT;    // Clear: module's VOUT pin sends to parent's VOUT signal
    }
}
```

### 2. Consistent Mental Model
- Module pins ALWAYS on left
- Parent context ALWAYS on right
- Arrow shows data flow direction

### 3. Natural Reading
```bhdl
buck: BuckConverter {
    VIN <- input_12v;       // "VIN receives from input_12v"
    VOUT -> regulated_5v;   // "VOUT sends to regulated_5v"
    EN <- enable_signal;    // "EN receives from enable_signal"
    PGOOD -> power_good;    // "PGOOD sends to power_good"
}
```

## Comparison with Dot Notation

### Current (Dot Notation):
```bhdl
buck: BuckConverter {
    input_12v -> .VIN;      // Parent to module
    .VOUT -> regulated_5v;  // Module to parent
    enable_signal -> .EN;   // Parent to module
    .PGOOD -> power_good;   // Module to parent
}
```

### Proposed (Consistent Order):
```bhdl
buck: BuckConverter {
    VIN <- input_12v;       // Module pin <- parent signal
    VOUT -> regulated_5v;   // Module pin -> parent signal
    EN <- enable_signal;    // Module pin <- parent signal
    PGOOD -> power_good;    // Module pin -> parent signal
}
```

## Advantages

1. **No Special Syntax**: No dots or other markers needed
2. **Visual Consistency**: All pins aligned on left
3. **Clear Direction**: Arrow always shows data flow
4. **Simpler Parser**: No need to handle dot prefix
5. **Easy to Scan**: Can quickly see all entity pins

## Handling Edge Cases

### 1. Bidirectional Pins
```bhdl
i2c_device: I2CPeripheral {
    SDA <-> i2c_sda_net;   // Clear: bidirectional
    SCL <- i2c_scl_net;    // Clear: input only
}
```

### 2. Power/Ground
```bhdl
amp: OpAmp {
    VCC <- VCC_5V;         // Power in
    VEE <- VEE_NEG5V;      // Power in
    GND <- GND;            // Ground connection
}
```

### 3. Arrays
```bhdl
mux: Multiplexer {
    IN[0..7] <- data_bus[0..7];    // Array input
    OUT[0..3] -> output[0..3];     // Array output
    SEL[0..1] <- select_bits;      // Selection inputs
}
```

### 4. Inter-Instance Connections
```bhdl
// Still need qualified names for instance-to-instance
stage2: Processor {
    DIN <- stage1.DOUT;    // From another instance
    CLK <- system_clock;   // From parent signal
}
```

## Why Arrows Make Sense

The arrow direction naturally indicates data flow:
- `<-` : Into the entity (input)
- `->` : Out of the entity (output)
- `<->` : Bidirectional

This matches how we think about entity interfaces!

## Potential Issues

### 1. Breaking Existing Convention
Current BHDL already uses both forms:
```bhdl
// Current mixed style
source -> .dest;    // When source is parent
.source -> dest;    // When source is module
```

### 2. Output Driving Seems Backward?
```bhdl
VOUT -> regulated_5v;  // Might read as "VOUT receives regulated_5v"?
```
But with context it's clear: "VOUT drives regulated_5v"

### 3. Need More Arrow Types?
What about weak pull-ups, tri-state, etc? Current arrows suffice.

## Migration Path

Could support both syntaxes during transition:
```bhdl
// Both work initially
amp: OpAmp {
    VCC <- VCC_5V;        // New style
    signal -> .OUT;       // Old style still works
}
```

## Complete Example

```bhdl
entity PowerSupply {
    pin VIN: power in;
    pin VOUT_5V: power out;
    pin VOUT_3V3: power out;
    pin ENABLE: digital in;
    
    // First stage - buck to 5V
    buck_5v: BuckConverter(vout=5V) {
        VIN <- VIN;              // Module VIN from parent VIN
        VOUT -> rail_5v;         // Module VOUT to internal rail
        EN <- ENABLE;            // Module EN from parent ENABLE
        FB <- feedback_5v;       // Feedback input
        
        // Scoped attributes
        attribute controller.fsw = 300kHz;
    }
    
    // Feedback network for 5V
    fb_5v: FeedbackDivider(ratio=0.8/5) {
        TOP <- rail_5v;          // Input from 5V rail
        BOTTOM <- GND;           // Ground reference
        TAP -> feedback_5v;      // Output to feedback net
    }
    
    // Output to parent
    rail_5v -> VOUT_5V;          // Internal signal to parent pin
    
    // Second stage - LDO to 3.3V
    ldo_3v3: LinearRegulator(vout=3.3V) {
        VIN <- rail_5v;          // From 5V rail
        VOUT -> VOUT_3V3;        // Direct to parent output
        EN <- ENABLE;            // Same enable
    }
}

board System {
    supply: PowerSupply {
        VIN <- VIN_12V;          // Board's 12V to module
        VOUT_5V -> SYS_5V;       // Module's 5V output
        VOUT_3V3 -> SYS_3V3;     // Module's 3.3V output
        ENABLE <- power_enable;   // Control signal
    }
}
```

## Recommendation

This consistent left-hand pin approach is **cleaner** than dot notation:

1. **Simpler syntax** - no special markers
2. **Consistent layout** - pins always on left
3. **Natural arrows** - show actual data flow
4. **Easier to read** - vertical alignment of pins

The only downside is breaking compatibility with current syntax, but the improvement in clarity may be worth it!