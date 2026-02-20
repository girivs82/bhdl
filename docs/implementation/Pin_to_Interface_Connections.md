# Pin-to-Interface Connection Implementation

## Overview

This document describes the implementation of pin-to-interface connections in the BHDL synthesizer, allowing component pins to connect directly to interface signals.

## Syntax

Pin-to-interface connections enable components to connect to interface signals using the following syntax:

```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: out;
}

board TestBoard {
    i2c_bus: I2C();
    
    mcu: STM32F4() {
        PA4 -> i2c_bus.SDA;  // Component pin to interface signal
        PA5 -> i2c_bus.SCL;  // Component pin to interface signal
    }
}
```

## Implementation Architecture

### 1. Synthesis Pipeline Order

The key insight was ensuring interface synthesis happens **before** connectivity extraction:

```rust
// Phase 2: Generate component instances
self.generate_database_component_instances(analysis).await?;

// Phase 3: Synthesize interface instances BEFORE connectivity
// This ensures interface signal nets exist before connections are processed
self.synthesize_interfaces(analysis)?;

// Phase 4: Extract connectivity and create nets
self.extract_connectivity_from_ast(ast, analysis)?;
```

### 2. Interface Signal Net Creation

Interface synthesis creates nets for each signal in the interface:

```rust
// In interface_synthesis.rs
for signal in &signals {
    let net_name = format!("{}_{}", instance.instance_name, signal.name);
    // Creates nets like "U1_SDA", "U1_SCL"
    let net_id = self.netlist.add_net(Some(net_name.clone()));
}
```

### 3. Smart Net Resolution

The `resolve_net` function in `hierarchical_connectivity.rs` was enhanced to detect interface signal references:

```rust
pub fn resolve_net(&mut self, net_name: &str, netlist: &mut Netlist) -> Result<NetId> {
    // Check for interface signal reference (e.g., i2c_bus.SDA)
    if net_name.contains('.') {
        let parts: Vec<&str> = net_name.split('.').collect();
        if parts.len() == 2 {
            let signal_name = parts[1];
            
            // Look for existing interface nets ending with _<signal_name>
            for (net_id, net) in netlist.nets.iter() {
                if let Some(existing_name) = &net.name {
                    if existing_name.ends_with(&format!("_{}", signal_name)) {
                        // Verify it's an interface net (U<number>_<signal> pattern)
                        let prefix_end = existing_name.len() - signal_name.len() - 1;
                        if prefix_end > 0 {
                            let prefix = &existing_name[..prefix_end];
                            if prefix.starts_with("U") && prefix[1..].chars().all(|c| c.is_digit(10)) {
                                return Ok(net_id);
                            }
                        }
                    }
                }
            }
        }
    }
    // ... rest of net resolution logic
}
```

### 4. Connection Processing

Port mappings like `PA4 -> i2c_bus.SDA` are processed through:

1. **Port Mapping Parsing**: The hierarchical connectivity extractor parses the connection
2. **Net Resolution**: `resolve_net` is called with "i2c_bus.SDA"
3. **Interface Net Lookup**: The function finds the existing "U1_SDA" net
4. **Pin Connection**: The component pin is connected to the interface net

## Key Design Decisions

### Interface Instance Naming

Interface instances are processed through component inference, which generates instance names like "U1", "U2". The original names (like "i2c_bus") are currently lost, but the system maps interface signals to the generated nets.

### Net Naming Convention

Interface signal nets follow the pattern: `<generated_instance>_<signal_name>`
- Example: `U1_SDA`, `U1_SCL` for an I2C interface instance

### Pattern Recognition

The system identifies interface nets by:
1. Net names ending with `_<signal_name>`
2. Prefixes matching `U<number>` pattern (interface instance convention)
3. Avoiding false matches with regular component nets

## Test Results

The implementation was verified with a comprehensive test:

```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: out;
}

entity STM32F4 {
    pin PA4: signal inout;
    pin PA5: signal inout;
}

board TestBoard {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    i2c_bus: I2C();
    
    mcu: STM32F4() {
        PA4 -> i2c_bus.SDA;
        PA5 -> i2c_bus.SCL;
    }
}
```

**Results:**
- ✅ Interface nets created: `U1_SDA`, `U1_SCL`
- ✅ MCU Pin PA4 connected to `U1_SDA`
- ✅ MCU Pin PA5 connected to `U1_SCL`
- ✅ No duplicate nets created
- ✅ Proper signal flow established

## Future Enhancements

1. **Original Name Preservation**: Preserve original interface instance names (i2c_bus) in addition to generated names
2. **Bidirectional Connections**: Support `<-` and `<->` operators for interface connections
3. **Interface-to-Interface**: Enable direct interface-to-interface connections using `<=>`
4. **Multi-Instance Interfaces**: Support connecting multiple components to the same interface signals

## Related Files

- `bhdl-synthesizer/src/interface_synthesis.rs` - Interface synthesis implementation
- `bhdl-synthesizer/src/hierarchical_connectivity.rs` - Net resolution and connection processing
- `bhdl-synthesizer/src/lib.rs` - Main synthesis pipeline
- `bhdl-synthesizer/src/bin/test_pin_interface_runner.rs` - Test runner
- `bhdl-synthesizer/tests/test_interface_pin_connections.rs` - Unit tests

## Dependencies

This implementation builds on:
- Interface definition parsing (Pass 2 analyzer)
- Interface instance synthesis
- Component inference system
- Hierarchical connectivity extraction
- Pin metadata system