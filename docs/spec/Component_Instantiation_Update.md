# BHDL Component Instantiation Specification Update

## Accepted Change: Type-Based Component Instantiation

### Problem with Refdes-First Approach

The original syntax required designers to invent reference designators during the connection phase:

```bhdl
// Problematic: Forces premature naming
VCC -> R1(4.7kΩ).1 -> LED1(red, 20mA).A;
VCC -> R2(10kΩ).1 -> C1(1µF).+ -> OUTPUT;
```

**Issues:**
- Cognitive overhead of inventing arbitrary labels (R1, R2, LED1...)
- Mental tracking of used designators
- Not how designers naturally think about circuits
- Waste of mental energy on naming rather than circuit design

### New Specification: Type-Based Instantiation

Components are instantiated by type with automatic refdes generation:

```bhdl
connections {
  // Natural component instantiation - no refdes needed
  VCC -> Res(4.7kΩ).1 -> LED(red, 20mA).A;
  LED.K -> GND;
  
  // Multiple components of same type
  VCC -> Res(10kΩ).1 -> Cap(1µF).+ -> OUTPUT;
  Res.2, Cap.- -> GND;
  
  // Component handles when multiple references needed
  VCC -> filter_res: Res(1kΩ).1 -> filter_cap: Cap(1µF).+ -> OUTPUT;
  filter_res.2, filter_cap.- -> GND;
}
```

### Automatic Refdes Generation

The toolchain automatically assigns reference designators:

1. **Component type mapping**: `Res` → R, `Cap` → C, `LED` → D, `OpAmp` → U
2. **Sequential numbering**: First resistor = R1, second = R2, etc.
3. **Handle preservation**: Named handles become the refdes when provided

**Internal representation:**
```bhdl
// Generated automatically by toolchain
components {
  R1: Resistor(4.7kΩ);           // From Res(4.7kΩ)
  D1: LED(red, 20mA);            // From LED(red, 20mA)
  R2: Resistor(10kΩ);            // From second Res(10kΩ)
  C1: Capacitor(1µF);            // From Cap(1µF)
  filter_res: Resistor(1kΩ);     // From handle
  filter_cap: Capacitor(1µF);    // From handle
}
```

### Standard Component Type Names

```bhdl
// Passive Components
Res(value, tolerance=5%, power=0.25W)
Cap(value, voltage, dielectric="X7R")
Ind(value, current, dcr)

// Semiconductors  
LED(color, current=20mA, voltage=auto)
Diode(type, voltage, current)
BJT_NPN(part_number)
MOSFET_N(part_number)

// Integrated Circuits
OpAmp(part_number)
Comparator(part_number)
Regulator(output_voltage, current)

// Connectors
Header(pins, rows=1, pitch=2.54mm)
Terminal(type, rating)
```

### Benefits

1. **Natural workflow**: Think in component functions, not arbitrary names
2. **Faster design entry**: No mental overhead for naming
3. **Fewer errors**: No duplicate refdes or numbering conflicts  
4. **Self-documenting**: Component types immediately visible in connections
5. **Progressive refinement**: Add handles only when multiple references needed

This change makes BHDL significantly more designer-friendly by eliminating unnecessary cognitive load during the creative circuit design phase.