# Pin Metadata System for Component Role Detection

## Overview

The pin metadata system provides explicit functional identification of component pins without relying on naming conventions. This enables accurate component role detection in power circuits by using pin function declarations as the primary source of truth, with topology analysis as secondary confirmation.

## Motivation

As identified by the user: "for the switching node, shouldn't the component pin details specify that it is a switching node? we can use inductor etc. as further confirmation but the pin should be the first clue, correct?"

Previously, the system relied on:
- Node naming conventions (e.g., "SW" for switch node)
- Pure topology analysis

The new system uses:
- Explicit pin function metadata (primary)
- Topology analysis for confirmation (secondary)

## Implementation

### Pin Function Types

```rust
pub enum PinFunction {
    PowerIn,           // Power input pin
    PowerOut,          // Power output pin
    SwitchNode,        // High dV/dt switching node
    Bootstrap,         // Bootstrap capacitor connection
    Feedback,          // Feedback voltage sensing
    Compensation,      // Compensation network connection
    SoftStart,         // Soft-start capacitor connection
    Enable,            // Enable/shutdown control
    CurrentSense,      // Current sense input
    ErrorAmplifierOut, // Error amplifier output
    VoltageReference,  // Internal voltage reference
    Ground,            // Ground/reference
    Signal,            // General signal pin
    Passive,           // Passive component terminal
    Unknown,           // Unknown/unspecified function
}
```

### Pin Metadata Structure

```rust
pub struct PinMetadata {
    pub function: PinFunction,
    pub electrical: PinElectricalData,
    pub description: Option<String>,
}

pub struct PinElectricalData {
    pub voltage_range: Option<(f64, f64)>,  // (min, max) volts
    pub max_current: Option<f64>,           // amperes
    pub impedance: Option<f64>,             // ohms (for inputs)
    pub dv_dt_rating: Option<f64>,          // V/µs (for switch nodes)
    pub frequency_range: Option<(f64, f64)>, // Hz
}
```

### Component Role Detection Enhancement

The component role detector now uses a two-phase approach:

1. **Primary: Pin Metadata Check**
   - Check if component is connected to an IC pin with specific function
   - Example: Capacitor connected to BOOT pin → Bootstrap capacitor

2. **Secondary: Topology Analysis**
   - Confirm role using electrical connections and values
   - Example: 0.1-1µF capacitor between switch node and boot pin

### Example: Switch Node Detection

```rust
fn is_switch_node(&self, node_id: NodeId) -> bool {
    // Primary method: Check pin metadata
    for (comp_id, comp) in &connected_components {
        if matches!(comp.component_type(), 
            "BuckController" | "BoostController" | ...) {
            
            if self.pin_database.pin_has_function(
                comp.component_type(), "SW", &PinFunction::SwitchNode
            ) {
                return true; // Definitely a switch node
            }
        }
    }
    
    // Secondary method: Topology analysis
    let has_switch = /* check for MOSFET or controller */;
    let has_inductor = /* check for inductor */;
    let has_diode = /* check for catch diode */;
    
    has_switch && has_inductor && has_diode
}
```

## Usage Example

```rust
// Create pin database with defaults
let pin_db = ComponentPinDatabase::new_with_defaults();

// Check if a pin has specific function
if pin_db.pin_has_function("BuckController", "SW", &PinFunction::SwitchNode) {
    println!("SW is a switch node pin");
}

// Get full metadata for a pin
if let Some(metadata) = pin_db.get_pin_metadata("BuckController", "BOOT") {
    println!("BOOT pin function: {:?}", metadata.function);
    println!("Voltage range: {:?}", metadata.electrical.voltage_range);
}
```

## Benefits

1. **No Naming Dependencies**: Works regardless of pin naming conventions
2. **Explicit Declaration**: Pin functions are explicitly declared, not inferred
3. **Electrical Validation**: Includes electrical characteristics for validation
4. **Extensible**: Easy to add new component types and pin functions
5. **Documentation**: Pin descriptions provide clear documentation

## Integration with BHDL

Future integration with BHDL could allow pin metadata in component definitions:

```bhdl
module BuckController {
    pin VIN: power in @metadata(function: PowerIn, vmax: 40V);
    pin SW: power out @metadata(function: SwitchNode, dv_dt: 100V/us);
    pin BOOT: power in @metadata(function: Bootstrap);
    pin FB: signal in @metadata(function: Feedback, impedance: 1M);
    // ...
}
```

## Testing

The implementation includes:
- Unit tests for pin database functionality
- Integration test showing role detection improvement
- Visual demonstration of pin metadata usage

## Future Enhancements

1. **Dynamic Pin Discovery**: Query actual component database for pin metadata
2. **Multi-Pin Components**: Handle components with many pins (e.g., microcontrollers)
3. **Pin Constraints**: Add constraints like "must connect to ground"
4. **Automatic Database Population**: Extract pin metadata from datasheets
5. **BHDL Parser Integration**: Parse pin metadata from BHDL files