# Interface Implementation Plan

## Overview

This document outlines the concrete steps to implement interface support in BHDL, with specific code examples and test cases.

## Phase 1: Basic Interface Support (MVP)

### 1.1 Parser Updates

**Add interface body parsing:**
```rust
// In parser/src/top_level.rs
fn parse_interface_contents(&mut self) {
    while !self.at_end() && !self.at(R_BRACE) {
        match self.current() {
            SIGNAL_KW => self.parse_interface_signal(),
            REQUIRE_KW => self.parse_interface_requirement(),
            COMMENT => self.advance(),
            _ => {
                self.error("Expected 'signal' or 'require' in interface");
                self.recover();
            }
        }
    }
}

fn parse_interface_signal(&mut self) {
    self.start_node(INTERFACE_SIGNAL);
    self.expect(SIGNAL_KW);
    self.expect(IDENT);  // Signal name
    self.expect(COLON);
    self.parse_signal_direction();  // in, out, inout
    if self.at(OPTIONAL_KW) {
        self.advance();
    }
    self.expect(SEMICOLON);
    self.finish_node();
}
```

### 1.2 AST Updates

**Add interface content access methods:**
```rust
// In ast/src/items.rs
impl InterfaceDef {
    pub fn signals(&self) -> impl Iterator<Item = InterfaceSignal> {
        self.syntax()
            .children()
            .filter_map(InterfaceSignal::cast)
    }
    
    pub fn requirements(&self) -> impl Iterator<Item = InterfaceRequirement> {
        self.syntax()
            .children()
            .filter_map(InterfaceRequirement::cast)
    }
    
    pub fn params(&self) -> Option<ParamList> {
        self.syntax()
            .children()
            .find_map(ParamList::cast)
    }
}

// New AST node for interface signals
pub struct InterfaceSignal(SyntaxNode);

impl InterfaceSignal {
    pub fn name(&self) -> Option<Ident> {
        self.syntax()
            .children()
            .find_map(Ident::cast)
    }
    
    pub fn direction(&self) -> SignalDirection {
        // Parse in/out/inout
    }
    
    pub fn is_optional(&self) -> bool {
        self.syntax()
            .children_with_tokens()
            .any(|it| it.kind() == OPTIONAL_KW)
    }
}
```

### 1.3 Analyzer Support

**Add interface analysis in Pass 2:**
```rust
// In analyzer/src/lib.rs
fn analyze_interface_def(&mut self, interface: &InterfaceDef) {
    let name = interface.name();
    
    // Create interface type
    let mut signals = HashMap::new();
    for signal in interface.signals() {
        let sig_name = signal.name();
        let sig_type = InterfaceSignalType {
            direction: signal.direction(),
            optional: signal.is_optional(),
            electrical_type: self.resolve_signal_type(&signal),
        };
        signals.insert(sig_name, sig_type);
    }
    
    // Store interface type
    self.interface_types.insert(name, InterfaceType {
        signals,
        requirements: self.analyze_requirements(interface),
    });
}
```

### 1.4 Synthesizer Support

**Generate nets for interface instances:**
```rust
// In synthesizer/src/lib.rs
fn synthesize_interface_instance(&mut self, inst: &InterfaceInst) -> Result<()> {
    let interface_name = inst.interface_type();
    let instance_name = inst.name();
    
    // Look up interface definition
    let interface_type = self.analysis.interface_types.get(&interface_name)
        .ok_or("Unknown interface")?;
    
    // Create nets for each signal
    for (signal_name, signal_type) in &interface_type.signals {
        let net_name = format!("{}.{}", instance_name, signal_name);
        let net_id = self.netlist.add_net(Some(net_name));
        
        // Store mapping for connections
        self.interface_signals.insert(
            (instance_name.clone(), signal_name.clone()),
            net_id
        );
    }
    
    // Generate required components (e.g., pullups)
    for requirement in &interface_type.requirements {
        self.synthesize_requirement(requirement, instance_name)?;
    }
    
    Ok(())
}
```

## Phase 1 Test Cases

### Test 1: Basic I2C Interface
```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
}

board Test {
    ground GND;
    
    bus: I2C();
    
    // Should generate:
    // - Net: bus.SDA
    // - Net: bus.SCL
}
```

### Test 2: Interface with Requirements
```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
}

board Test {
    power VCC = 3.3V;
    ground GND;
    
    bus: I2C();
    
    // Should generate:
    // - Net: bus.SDA
    // - Net: bus.SCL
    // - Resistor: R1 (4.7kΩ) from VCC to bus.SDA
    // - Resistor: R2 (4.7kΩ) from VCC to bus.SCL
}
```

### Test 3: Pin-to-Interface Connection
```bhdl
interface UART {
    signal TX: out;
    signal RX: in;
}

board Test {
    uart_bus: UART();
    
    mcu: MCU() {
        TX -> uart_bus.TX;
        RX <- uart_bus.RX;
    }
    
    // Should generate:
    // - Connection: mcu.TX to uart_bus.TX net
    // - Connection: mcu.RX to uart_bus.RX net
}
```

## Phase 2: Advanced Features

### 2.1 Interface-to-Interface Connections

**Add operator support:**
```rust
// Handle <=> operator
fn analyze_interface_connection(&mut self, conn: &InterfaceConnection) {
    let left = self.resolve_interface(&conn.left());
    let right = self.resolve_interface(&conn.right());
    
    // Check compatibility
    for (sig_name, left_sig) in &left.signals {
        if let Some(right_sig) = right.signals.get(sig_name) {
            self.check_signal_compatibility(left_sig, right_sig)?;
        } else if !left_sig.optional {
            self.error("Required signal missing in connection");
        }
    }
    
    // Record connection for synthesis
    self.interface_connections.push((left, right));
}
```

### 2.2 Parameterized Interfaces

```bhdl
interface SPI(frequency: frequency = 1MHz, mode: int = 0) {
    signal MOSI: out;
    signal MISO: in;
    signal SCK: out;
    signal CS: out;
    
    constrain SCK.frequency <= frequency;
}

// Usage
spi_fast: SPI(frequency=50MHz, mode=3);
```

### 2.3 Hierarchical Interfaces

```bhdl
interface USB3 {
    interface SS {  // SuperSpeed
        signal TXP: out;
        signal TXN: out;
        signal RXP: in;
        signal RXN: in;
    }
    
    interface USB2 {  // High/Full/Low speed
        signal DP: inout;
        signal DN: inout;
    }
}

// Usage
usb: USB3();
phy.SSTXP -> usb.SS.TXP;
```

## Phase 3: Full Implementation

### 3.1 Interface Inheritance
```bhdl
interface I2CBase {
    signal SDA: inout;
    signal SCL: inout;
}

interface I2CWithSMBus extends I2CBase {
    signal SMBALERT: in optional;
    signal SMBCLK: out optional;
}
```

### 3.2 Voltage Domain Handling
```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
    domain: power;  // Inherits from connection context
}

// Automatic level shifting
mcu.I2C1 @3.3V <=> sensor.I2C @1.8V;  // Inserts level shifter
```

### 3.3 Interface Arrays
```bhdl
interface GPIO {
    signal IO: inout;
}

// Array of interfaces
gpio[8]: GPIO();

// Connect to pins
for i in 0..8 {
    mcu.GPIO[i] <-> gpio[i].IO;
}
```

## Validation Test Suite

### Positive Tests
1. Basic interface definition
2. Interface with all signal directions
3. Optional signals
4. Interface requirements (pullup, termination)
5. Parameterized interfaces
6. Interface-to-interface connections
7. Hierarchical interfaces
8. Interface arrays

### Negative Tests
1. Missing required signals in connection
2. Direction conflicts (out to out)
3. Type mismatches
4. Unknown interface types
5. Circular interface dependencies

### Integration Tests
1. I2C sensor network
2. SPI flash with multiple devices
3. USB with power delivery
4. Memory interface with timing

## Success Metrics

1. **Parser**: All interface syntax parses correctly
2. **Analyzer**: Type checking catches all errors
3. **Synthesizer**: Generates correct nets and components
4. **Examples**: All example circuits work correctly
5. **Documentation**: Clear specification and examples

## Timeline Estimate

- **Phase 1**: 2-3 weeks (Basic MVP)
- **Phase 2**: 2-3 weeks (Advanced features)
- **Phase 3**: 3-4 weeks (Full implementation)
- **Testing**: 1-2 weeks (Throughout)

Total: 8-12 weeks for complete implementation

## Next Steps

1. Implement basic parser support for interface signals
2. Add AST methods for interface content access
3. Create analyzer support for interface types
4. Implement basic synthesizer support
5. Write comprehensive test suite
6. Document interface usage patterns