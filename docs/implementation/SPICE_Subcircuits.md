# SPICE Subcircuits Implementation

This document describes the implementation of SPICE subcircuits in the BHDL toolchain.

## Overview

SPICE subcircuits allow hierarchical circuit definitions where complex components are represented as a collection of simpler SPICE elements. This is essential for modeling ICs, modules, and reusable circuit blocks.

## Implementation

### Core Components

1. **SubcircuitDefinition** - Defines a reusable subcircuit template
   - Name (e.g., "LM741", "555_TIMER")
   - Pin definitions mapping external to internal nodes
   - Internal circuit representation
   - Parameters and defaults

2. **SubcircuitModel** - Instance of a subcircuit
   - References a SubcircuitDefinition
   - Instance-specific parameter overrides
   - Pin connections to parent circuit nodes
   - Expansion method to integrate into parent circuit

3. **SubcircuitLibrary** - Storage for subcircuit definitions
   - Add/retrieve definitions
   - Instantiate subcircuits

4. **SpiceModelFactory Integration**
   - `add_subcircuit()` - Register custom subcircuits
   - `create_subcircuit()` - Create instances
   - `is_subcircuit()` - Check if name is a subcircuit
   - Built-in TL431 voltage reference subcircuit

### Key Features

- **Hierarchical Expansion**: Subcircuits expand into parent circuits with proper node mapping
- **Node Prefixing**: Internal nodes prefixed with instance name (e.g., "U1:INT")
- **Parameter Support**: Instance-specific parameter overrides (future: expression evaluation)
- **Pin Mapping**: External pins map to internal circuit nodes

### Example Usage

```rust
// Create voltage divider subcircuit
let mut internal = Circuit::new();
internal.add_node("IN", None);
internal.add_node("OUT", None);
internal.add_node("GND", None);
internal.add_branch("R1", "IN", "OUT", "Resistor", 10e3, None);
internal.add_branch("R2", "OUT", "GND", "Resistor", 10e3, None);

let pins = vec![
    SubcircuitPin {
        external_name: "VIN".to_string(),
        internal_node: "IN".to_string(),
        pin_type: "input".to_string(),
    },
    // ... more pins
];

let def = SubcircuitDefinition {
    name: "VDIV_2TO1".to_string(),
    pins,
    internal_circuit: internal,
    parameters: HashMap::new(),
    defaults: HashMap::new(),
};

// Use in model factory
factory.add_subcircuit(def);
let instance = factory.create_subcircuit("U1", "VDIV_2TO1");
```

## Integration with BHDL

Future work will integrate subcircuits with BHDL syntax:

```bhdl
// Define subcircuit in BHDL
subcircuit OpAmp741 {
    pin VCC: power in;
    pin VEE: power in;
    pin IN+: signal in;
    pin IN-: signal in;
    pin OUT: signal out;
    
    // Internal implementation
    ...
}

// Instantiate in board
board MyBoard {
    U1: OpAmp741();
    VCC -> U1.VCC;
    // ...
}
```

## Testing

The `test_subcircuit.rs` example demonstrates:
- Creating voltage divider and RC filter subcircuits
- Instantiating and connecting subcircuits
- Expanding into parent circuit
- Verifying correct node mapping and component naming

## Future Enhancements

1. **Expression Evaluation**: Support parameter expressions in component values
2. **BHDL Integration**: Parse subcircuit definitions from BHDL
3. **Library Management**: Import/export subcircuit libraries
4. **Nested Subcircuits**: Support subcircuits containing other subcircuits
5. **Model Parameters**: Pass model-specific parameters (e.g., transistor models)