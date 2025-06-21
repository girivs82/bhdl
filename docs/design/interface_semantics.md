# BHDL Interface Semantics Design Document

## Core Semantic Decisions

### 1. What is an Interface?

**Definition**: An interface is a contract that defines a bundle of related signals with their electrical and protocol requirements.

**Key Properties**:
- **Not a Component**: Interfaces don't create physical components, only connection contracts
- **Bidirectional**: All interface connections use `<->` or `<=>` operators
- **Type-Safe**: Enforces signal compatibility and electrical requirements
- **Synthesizable**: Generates nets and required passive components (pullups, termination)

### 2. Interface vs Module

```bhdl
// MODULE: Creates components and internal connections
module UARTTransceiver() {
    pin TX: signal out;
    pin RX: signal in;
    // Contains implementation with components
}

// INTERFACE: Defines connection contract
interface UART {
    signal TX: out;
    signal RX: in;
    // Contains only signal definitions and requirements
}
```

### 3. Signal Direction Semantics

**Problem**: Who defines signal directions in an interface?

**Solution**: Perspective-based directions with component-declared interfaces

```bhdl
interface SPI {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out;
    }
    
    perspective slave {
        signal MOSI: in;
        signal MISO: out;
        signal SCK: in;
        signal CS: in;
    }
}

// Components declare their interface capabilities
component STM32F4 {
    interface SPI1: SPI.master;
    interface SPI2: SPI.master;
}

component W25Q128 {
    interface SPI: SPI.slave;
}

// Usage - perspective automatically resolved
mcu.SPI1 <=> spi_bus;   // Uses master perspective
flash.SPI <=> spi_bus;  // Uses slave perspective
```

### 4. Connection Semantics

#### Interface Instantiation
```bhdl
// Creates a bundle of nets with the interface contract
i2c_bus: I2C();
```

#### Pin-to-Interface Connection
```bhdl
// Individual connections
mcu.SDA <-> i2c_bus.SDA;
mcu.SCL <-> i2c_bus.SCL;

// Bundle connection (when names match)
mcu.{SDA, SCL} <-> i2c_bus.{SDA, SCL};
```

#### Interface-to-Interface Connection
```bhdl
// Direct connection (all signals)
mcu.I2C1 <=> sensor_i2c;

// With transformation
mcu.SPI1 <=> level_shift(3.3V, 1.8V) <=> flash.SPI;
```

### 5. Synthesis Semantics

Interfaces synthesize to:

1. **Nets**: Each signal becomes a net
2. **Passive Components**: From requirements
   ```bhdl
   require pullup(SDA, 4.7kΩ);  // Generates resistor
   require termination(D+, D-, 90Ω);  // Generates termination
   ```
3. **No Active Components**: Interfaces cannot contain active components

### 6. Electrical Requirements

```bhdl
interface LVDS {
    signal P: out;
    signal N: out;
    
    // These generate validation rules and/or components
    require differential(P, N);
    require impedance(100Ω);
    require voltage_swing(350mV);
}
```

### 7. Type Compatibility

Interfaces are compatible when:
1. Signal names match (or are explicitly mapped)
2. Signal directions are compatible
3. Electrical requirements don't conflict
4. Required signals are all present

### 8. Hierarchical Interfaces

```bhdl
interface RGMII {
    // Sub-interface for organization
    interface TX {
        signal TXD[4]: out;
        signal TX_CTL: out;
    }
    
    interface RX {
        signal RXD[4]: in;
        signal RX_CTL: in;
    }
}

// Usage
eth.TX <=> phy.TX;  // Connect sub-interfaces
```

### 9. Optional Signals

```bhdl
interface UART {
    signal TX: out;
    signal RX: in;
    signal RTS: out optional;  // May not be connected
    signal CTS: in optional;
}

// OK to omit optional signals
uart1.{TX, RX} <-> uart_bus.{TX, RX};
```

### 10. Key Design Principles

1. **Simplicity First**: Start with basic signal bundles
2. **No Hidden Behavior**: Interfaces are purely declarative
3. **Explicit Requirements**: All constraints must be stated
4. **Composition Over Complexity**: Build complex from simple
5. **Clear Synthesis Model**: Users must understand what hardware is generated

## Implementation Priority

### Phase 1 (MVP)
- Basic interface definition with signals
- Simple instantiation
- Pin-to-interface connections
- Basic synthesis (nets only)

### Phase 2
- Electrical requirements (pullups, termination)
- Interface-to-interface connections
- Perspective-based directions
- Optional signals

### Phase 3
- Hierarchical interfaces
- Interface inheritance
- Advanced requirements
- Protocol validation

## Interface Parameters

### Why Parameters are Essential

1. **Protocol Configuration**: Different speeds, modes, and options
   ```bhdl
   i2c_slow: I2C(frequency=100kHz);   // Standard mode
   i2c_fast: I2C(frequency=400kHz);   // Fast mode
   ```

2. **Structural Adaptation**: Variable bus widths and channel counts
   ```bhdl
   narrow: ParallelBus(width=8);   // 8-bit bus
   wide: ParallelBus(width=32);    // 32-bit bus
   ```

3. **Electrical Requirements**: Voltage-dependent configurations
   ```bhdl
   interface GPIO(voltage: voltage = 3.3V, drive: current = 4mA) {
       signal IO: inout;
       require drive_strength(drive);
       require pullup(IO, voltage > 2.5V ? 10kΩ : 47kΩ) when pulled_up;
   }
   ```

### Parameter Design Principles

1. **Sensible Defaults**: Most common configuration as default
2. **Type Safety**: Use appropriate types (frequency, voltage, not just numbers)
3. **Validation**: Constrain parameters to valid ranges
4. **Conditional Features**: Use parameters to enable/disable signals

## Open Questions

1. **How to handle voltage domains?**
   - Interfaces inherit domain from connected power
   - Explicit domain specification allowed

2. **Can interfaces contain logic?**
   - No, only declarative requirements
   - Logic belongs in modules

3. **How to handle protocol state?**
   - Future: State machine declarations
   - Current: Documentation only

## Example: Complete I2C Interface

```bhdl
// Minimal viable interface definition
interface I2C(frequency: frequency = 100kHz) {
    // Signals
    signal SDA: inout;
    signal SCL: inout;  // Direction context-dependent
    
    // Electrical requirements
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
    require open_drain(SDA, SCL);
    
    // Timing constraints
    constrain SCL.frequency <= frequency;
}

// Usage
board I2CDevice {
    power VCC_3V3 = 3.3V @ 100mA;
    ground GND;
    
    // Creates nets + pullup resistors
    bus: I2C(frequency=400kHz);
    
    // Connections
    mcu.I2C1 <=> bus;
    sensor.I2C <=> bus;
}
```

## Summary

BHDL interfaces provide a clean, declarative way to define multi-signal connection contracts. They synthesize to nets and passive components only, with no hidden behavior. The design prioritizes simplicity, explicitness, and a clear synthesis model that users can reason about.