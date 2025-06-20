# Port Mapping Implementation for Hierarchical Modules

## What Port Mapping Actually Is

Port mapping is the connection syntax inside module instantiation blocks:

```bhdl
instance_name: ModuleType {
    pin_name <- signal_name;    // Module pin receives from signal
    pin_name -> signal_name;    // Module pin sends to signal
    pin_name <-> signal_name;   // Bidirectional connection
}
```

## Parser Requirements

### 1. Module Instantiation Block
```bhdl
module Container {
    // Instance with port mapping block
    child: ChildModule {
        // Port mappings go here
    }
}
```

### 2. Connection Statements in Instance Blocks
```
InstanceDecl = IDENT ':' IDENT ParamList? '{' ConnectionList '}'
ConnectionList = Connection*
Connection = FlowConnection | Assignment | ...
```

### 3. Pin Reference Syntax
- Left side: Module pin being connected
- Right side: Parent signal or qualified instance.pin
- Arrow direction: Shows data flow

## AST Representation

```rust
#[derive(Debug, Clone)]
pub struct InstanceDecl {
    pub name: String,
    pub module_type: String,
    pub params: Option<ParamList>,
    pub connections: Vec<PortMapping>,
}

#[derive(Debug, Clone)]
pub struct PortMapping {
    pub kind: MappingKind,
    pub source: ConnectionEndpoint,
    pub target: ConnectionEndpoint,
}

#[derive(Debug, Clone)]
pub enum MappingKind {
    Unidirectional,  // ->
    Bidirectional,   // <->
}

#[derive(Debug, Clone)]
pub enum ConnectionEndpoint {
    Signal(String),           // signal_name
    Pin(String),             // pin_name (always module pin on left)
    QualifiedPin(String, String), // other_instance.pin
    Power(String),           // VCC, GND, etc.
}
```

## Analyzer Validation

### 1. Pin Existence
```rust
fn validate_port_mapping(
    instance: &InstanceDecl,
    module_def: &Module,
) -> Result<()> {
    for mapping in &instance.connections {
        // Check pins exist in module definition (always on left)
        if let ConnectionEndpoint::Pin(pin) = &mapping.source {
            if !module_def.has_pin(pin) {
                return Err(format!(
                    "Module '{}' has no pin '{}'", 
                    module_def.name, pin
                ));
            }
        }
    }
    Ok(())
}
```

### 2. Direction Compatibility
```rust
fn check_direction_compatibility(
    source: &PinDirection,
    target: &PinDirection,
) -> Result<()> {
    match (source, target) {
        (Out, In) => Ok(()),
        (InOut, InOut) => Ok(()),
        (In, Out) => Err("Cannot connect input to output"),
        // ... other cases
    }
}
```

### 3. Type Compatibility
```rust
fn check_type_compatibility(
    source_type: &SignalType,
    target_type: &SignalType,
) -> Result<()> {
    if source_type != target_type {
        return Err(format!(
            "Type mismatch: {} != {}", 
            source_type, target_type
        ));
    }
    Ok(())
}
```

## Synthesizer Handling

### 1. Net Creation
```rust
impl NetlistBuilder {
    fn process_port_mapping(
        &mut self,
        instance_id: InstanceId,
        mapping: &PortMapping,
    ) {
        // Get or create net for the connection
        let net_id = match &mapping.source {
            ConnectionEndpoint::Signal(name) => {
                self.get_or_create_net(name)
            }
            ConnectionEndpoint::QualifiedPin(inst, pin) => {
                self.get_instance_pin_net(inst, pin)
            }
            // ... other cases
        };
        
        // Connect instance pin to net (pin is always on left)
        if let ConnectionEndpoint::Pin(pin) = &mapping.source {
            self.connect_pin_to_net(instance_id, pin, net_id);
        }
    }
}
```

### 2. Hierarchical Net Names
```rust
// Generate unique net names for hierarchy
fn hierarchical_net_name(path: &[String], local_name: &str) -> String {
    let mut full_path = path.to_vec();
    full_path.push(local_name.to_string());
    full_path.join(".")
}

// Examples:
// "top.power_supply.intermediate_12v"
// "top.controller.feedback_net"
```

## Common Patterns

### 1. Power Distribution
```bhdl
module System {
    power VCC = 5V;
    
    sub1: SubModule {
        POWER <- VCC;  // Distribute power
    }
    
    sub2: SubModule {
        POWER <- VCC;  // Same power to multiple modules
    }
}
```

### 2. Signal Chaining
```bhdl
module Pipeline {
    stage1: Process {
        IN <- input;
        OUT -> intermediate;
    }
    
    stage2: Process {
        IN <- intermediate;
        OUT -> output;
    }
}
```

### 3. Bus Connections
```bhdl
module BusSystem {
    master: BusMaster {
        ADDR[0..7] -> addr_bus[0..7];
        DATA[0..7] <-> data_bus[0..7];
    }
    
    slave: BusSlave {
        ADDR[0..7] <- addr_bus[0..7];
        DATA[0..7] <-> data_bus[0..7];
    }
}
```

## Implementation Checklist

- [ ] Parse instance declaration blocks
- [ ] Parse connection statements inside blocks
- [ ] Enforce left-side pin, right-side signal syntax
- [ ] Create PortMapping AST nodes
- [ ] Validate pin existence
- [ ] Check direction compatibility
- [ ] Check type compatibility
- [ ] Generate hierarchical nets
- [ ] Handle array pin mappings
- [ ] Support all connection operators (←, →, ↔)

This is the foundation that makes modules composable!