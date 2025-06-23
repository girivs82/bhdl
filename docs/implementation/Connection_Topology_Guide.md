# Connection Topology Guide

## Overview

BHDL v2.0 supports two fundamental connection paradigms:
1. **Net-based connections** - for logical equivalence
2. **Pin-to-pin connections** - for physical topology

This guide explains when to use each approach and how to apply physical constraints.

## Net-Based Connections

Use net-based connections when you need to represent true equipotential nodes where the exact topology doesn't matter.

### When to Use Nets

1. **Power and Ground Distribution**
```bhdl
@VCC -> U1.VDD;
@VCC -> U2.VDD;
@VCC -> U3.VDD;
@GND -> C1.2;
@GND -> C2.2;
```

2. **Multi-drop Digital Buses**
```bhdl
@I2C_SCL -> U1.SCL;
@I2C_SCL -> U2.SCL;
@I2C_SCL -> EEPROM.SCL;
@I2C_SCL -> R_PULLUP.1;
```

3. **Global Signals**
```bhdl
@RESET_N -> MCU.RST;
@RESET_N -> FPGA.RST_N;
@RESET_N -> ETHERNET_PHY.RST;
```

### Net Creation Rules

- Nets are created implicitly when first referenced with `@`
- No explicit `net` declarations needed in v2.0
- Power/ground are declared with keywords but referenced with `@`

## Pin-to-Pin Connections

Use pin-to-pin connections when the physical topology matters for signal integrity, current flow, or layout.

### When to Use Pin-to-Pin

1. **Analog Signal Paths**
```bhdl
// Op-amp feedback network - topology matters!
OPAMP.out -> R1.1;
R1.2 -> C1.1;      // Compensation
C1.1 -> R2.1;
R2.1 -> OPAMP.inv; // Feedback
R2.2 -> @GND;
```

2. **Power Delivery Networks**
```bhdl
// Buck converter - feedback point critical
L1.2 -> C1.1;           // First cap at inductor
C1.1 -> R_FB_TOP.1;     // Feedback taps HERE
C1.1 -> C2.1;           // Bulk caps downstream
C2.1 -> LOAD.power;
```

3. **High-Speed Differential Pairs**
```bhdl
// Maintains pair routing
DRIVER.TX_P -> RECEIVER.RX_P;
DRIVER.TX_N -> RECEIVER.RX_N;
```

4. **Current Sensing**
```bhdl
// Current flows through sense resistor
MOSFET.source -> R_SENSE.1;
R_SENSE.2 -> @GND;
R_SENSE.1 -> CURRENT_AMP.IN_P;  // Kelvin connection
R_SENSE.2 -> CURRENT_AMP.IN_N;  // Kelvin connection
```

## Physical Constraints

### Individual Connection Constraints (`where`)

Use `where` to constrain a single connection:

```bhdl
// Length constraints
C1.1 -> FB.top where trace_length < 10mm;
XTAL.out -> MCU.OSC where length = 15mm ± 0.5mm;

// Impedance control
TX.out -> RX.in where impedance = 50Ω;

// Current handling
@VCC -> MOTOR.pwr where current >= 5A, width >= 2mm;

// Special routing
OPAMP.out -> ADC.in where shielded, guard_ring;
```

### Grouped Constraints (`with`)

Use `with` to apply constraints to multiple connections:

```bhdl
// Matched impedance
with routing(impedance = 50Ω, matched_length) {
    CPU.D0 -> RAM.D0;
    CPU.D1 -> RAM.D1;
    CPU.D2 -> RAM.D2;
}

// Power distribution
with power(min_width = 1mm) {
    @VCC -> U1.VDD where bypass = C1;
    @VCC -> U2.VDD where bypass = C2;
}
```

## Best Practices

### 1. Choose the Right Connection Type

| Scenario | Use Nets | Use Pin-to-Pin |
|----------|----------|----------------|
| Power distribution | ✓ | Sometimes |
| Ground connections | ✓ | Rarely |
| Digital multi-drop | ✓ | No |
| Analog signals | Rarely | ✓ |
| Feedback paths | No | ✓ |
| High-speed paths | No | ✓ |
| Current sensing | No | ✓ |

### 2. Apply Constraints at the Right Level

- **Individual constraints**: For specific requirements
- **Group constraints**: For related signals
- **Nested groups**: For hierarchical organization

### 3. Document Critical Paths

```bhdl
// Buck converter feedback - critical for stability
L1.2 -> C1.1;
C1.1 -> R_FB.1 where trace_length < 10mm;  // Critical!
```

### 4. Consider Current Flow

```bhdl
// High current path
MOSFET.drain -> L1.1 where current_rating = 5A;
L1.2 -> C1.1 where trace_width >= 2mm;
C1.1 -> LOAD.power;

// Separate sense path
C1.1 -> R_SENSE.1 where trace_width = 0.2mm;  // Light current
```

## Examples

### Example 1: Precision Analog Circuit

```bhdl
module PrecisionAnalog {
    // Input path with shielding
    @INPUT -> R1.1 where trace_length < 20mm;
    R1.2 -> OPAMP.in_p where shielded;
    
    // Feedback network - pin-to-pin critical
    OPAMP.out -> R_FB.1;
    R_FB.2 -> C_COMP.1;
    C_COMP.1 -> OPAMP.in_n where trace_length < 15mm;
    C_COMP.2 -> @AGND;
    
    // Star ground for analog
    with routing(star_point = "OPAMP.gnd") {
        C_COMP.2 -> @AGND;
        R_GAIN.2 -> @AGND;
        C_BYPASS.2 -> @AGND;
    }
}
```

### Example 2: High-Speed Digital

```bhdl
module DDR3Interface {
    // Matched length byte lanes
    with routing(impedance = 50Ω, matched_length) {
        generate for i in 0..7 {
            CPU.DQ[i] -> RAM.DQ[i];
        }
        
        // Differential strobe
        with differential(100Ω) {
            CPU.DQS_P -> RAM.DQS_P;
            CPU.DQS_N -> RAM.DQS_N;
        }
    }
}
```

### Example 3: Power Management

```bhdl
module PowerPath {
    // Main power distribution
    @VIN -> FUSE.1 where current_rating = 10A;
    FUSE.2 -> MOSFET.drain;
    
    // Switch node - critical routing
    MOSFET.source -> L1.1 where trace_width >= 3mm, length < 20mm;
    L1.2 -> C_OUT1.1;
    
    // Feedback taps at first cap
    C_OUT1.1 -> R_FB_TOP.1 where trace_length < 10mm;
    
    // Bulk storage downstream
    C_OUT1.1 -> C_OUT2.1 where trace_width >= 2mm;
    C_OUT2.1 -> C_OUT3.1;
}
```

## Conclusion

The dual connection paradigm in BHDL v2.0 provides the flexibility to express both logical connectivity and physical topology. By choosing the appropriate connection type and applying constraints judiciously, designers can create boards that are both functionally correct and physically realizable with good signal integrity.

Key takeaways:
- Use nets for true equipotential nodes
- Use pin-to-pin for topology-critical paths
- Apply constraints close to their requirement source
- Group related connections for consistency
- Document critical paths and constraints