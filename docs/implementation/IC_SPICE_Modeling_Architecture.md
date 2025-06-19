# IC SPICE Modeling Architecture

## Overview

This document describes the sustainable architecture for modeling integrated circuits (ICs) in BHDL's SPICE integration. The approach prioritizes maintainability and extensibility by keeping component-specific knowledge in BHDL files rather than hardcoding it in Rust.

## Design Principles

### 1. Behavioral Modeling
- ICs are modeled as black boxes with observable pin behavior
- No need for transistor-level details
- Focus on datasheet specifications and characteristic curves

### 2. BHDL-Driven Parameters
- All SPICE parameters come from BHDL component attributes
- No hardcoded IC variants in Rust code
- Model factory remains generic and extensible

### 3. Separation of Concerns
- **BHDL**: Component specifications, parameters, variants
- **Rust**: Generic model implementation, simulation algorithms
- **SPICE Models**: Mathematical behavior at pins

## Implementation Example: Voltage Regulators

### BHDL Component Definition
```bhdl
module VReg78xx(voltage: voltage = 5V, package: string = "TO-220") {
    pin IN: power in;
    pin OUT: power out;
    pin GND: ground;
    
    // Component-specific parameters
    const VREG_7805: VoltageRegulatorParams = {
        type: "fixed",
        vout_nominal: 5V,
        dropout: 2V,
        iout_max: 1A,
        iq: 5mA,
        load_regulation: 0.5%,
        line_regulation: 0.01%,
        rout: 0.017,
        // ... more parameters
    };
    
    // Select parameters based on voltage
    const params = voltage == 5V ? VREG_7805 : /* other variants */;
    
    // Export ALL parameters as SPICE attributes
    attribute spice_model = "voltage_regulator";
    attribute spice_type = params.type;
    attribute spice_vout_nom = params.vout_nominal;
    attribute spice_dropout = params.dropout;
    // ... all other parameters
}
```

### Rust Model Factory (Generic)
```rust
// No hardcoded IC knowledge!
"voltage_regulator" => {
    let mut params = VoltageRegulatorParams::default();
    
    // Extract ALL parameters from attributes
    if let Some(vout) = attributes.get("spice_vout_nom").and_then(parse) {
        params.vout_nom = vout;
    }
    if let Some(dropout) = attributes.get("spice_dropout").and_then(parse) {
        params.dropout = dropout;
    }
    // ... extract all parameters
    
    Some(Box::new(VoltageRegulatorModel::new(name, params)))
}
```

### SPICE Model Implementation
```rust
impl VoltageRegulatorModel {
    fn regulated_voltage(&self, vin: f64, iout: f64, temp: f64) -> f64 {
        // Behavioral model equations
        let vout_ideal = self.params.vout_nom;
        let vout_dropout = vin - self.params.dropout;
        let vout = vout_ideal.min(vout_dropout);
        
        // Apply load regulation
        vout * (1.0 - self.params.load_reg * iout)
    }
}
```

## Benefits

### 1. Maintainability
- Adding new IC variants requires only BHDL changes
- No need to modify and recompile Rust code
- All IC knowledge centralized in stdlib

### 2. Extensibility
- Easy to add new IC types following the pattern
- Model factory remains generic
- New parameters can be added without breaking existing code

### 3. Type Safety
- BHDL's type system ensures correct units
- Parameters validated at compile time
- Clear separation between electrical and SPICE parameters

### 4. Documentation
- IC parameters documented in BHDL files
- Self-documenting component interfaces
- Easy to see all available variants

## Adding New IC Types

To add a new IC type:

1. **Create BHDL Component Module**
   ```bhdl
   module NewIC(...) {
       // Define pins
       // Define parameter types
       // Create parameter constants for variants
       // Export all as spice attributes
   }
   ```

2. **Create Rust Model**
   ```rust
   pub struct NewICModel {
       params: NewICParams,
   }
   impl SpiceModel for NewICModel {
       // Implement behavioral equations
   }
   ```

3. **Register in Model Factory**
   ```rust
   "new_ic" => {
       // Generic parameter extraction
       // No hardcoded variants!
   }
   ```

## IC Types Roadmap

### Implemented
- ✅ Voltage Regulators (fixed & adjustable)

### Planned
- 🔲 Digital Logic Gates
- 🔲 Timers (555, etc.)
- 🔲 Comparators
- 🔲 Voltage References
- 🔲 Op-Amps (enhanced)
- 🔲 Power Management ICs
- 🔲 Interface ICs

## Best Practices

1. **Always export all parameters** - Even if not used initially
2. **Use meaningful units** - Leverage BHDL's unit system
3. **Document assumptions** - Behavioral models make simplifications
4. **Test edge cases** - Dropout, current limits, temperature
5. **Follow datasheet** - Parameters should match manufacturer specs

## Conclusion

This architecture ensures that BHDL remains the single source of truth for component specifications while keeping the simulation engine generic and maintainable. The approach scales well from simple voltage regulators to complex mixed-signal ICs.