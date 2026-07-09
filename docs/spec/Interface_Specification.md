> **SUPERSEDED by [Interfaces.md](Interfaces.md) (2026-07-09).** This earlier
> draft describes interface features that were redesigned or retired —
> `require pullup(...)`/`require esd_protection(...)` (replaced by vendor
> `expansion {}` + conditional gating), `~Interface` reversal (replaced by
> `:role` perspectives), `extends` inheritance, and inline `level_shift()`.
> Use Interfaces.md as the current interface reference.

# BHDL Interface Specification

## 1. Introduction

Interfaces in BHDL provide a mechanism to define and enforce contracts for multi-signal connections between modules. They encapsulate related signals, their directions, electrical characteristics, and protocol requirements into reusable, type-safe bundles.

## 2. Core Concepts

### 2.1 What is an Interface?

An interface is a named collection of signals with:
- Well-defined signal roles and directions
- Electrical constraints and requirements
- Protocol-level semantics
- Bidirectional connection capability

### 2.2 Interface vs Module

| Aspect | Interface | Module |
|--------|-----------|---------|
| Purpose | Define connection contracts | Implement functionality |
| Contains | Signal declarations, constraints | Components, connections |
| Instantiation | Creates connection points | Creates physical components |
| Direction | Bidirectional by nature | Fixed pin directions |
| Synthesis | Generates wires/nets | Generates components |

## 3. Interface Definition Syntax

### 3.1 Basic Interface

```bhdl
interface I2C {
    // Signal declarations with roles
    signal SDA: inout;  // Data line
    signal SCL: in;     // Clock line (master perspective)
    
    // Electrical requirements
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
    
    // Voltage domain
    domain: power;  // Inherits from connected power domain
}
```

### 3.2 Parameterized Interface

```bhdl
interface SPI(frequency: frequency = 1MHz, mode: int = 0) {
    // Master perspective signals
    signal MOSI: out;   // Master Out, Slave In
    signal MISO: in;    // Master In, Slave Out  
    signal SCK: out;    // Serial Clock
    signal CS: out;     // Chip Select (active low)
    
    // Constraints based on parameters
    constrain SCK.frequency <= frequency;
    constrain SCK.phase = (mode & 0x1);
    constrain SCK.polarity = (mode >> 1) & 0x1;
}
```

### 3.3 Hierarchical Interface

```bhdl
interface RGMII {
    // Transmit interface
    interface TX {
        signal TXD[4]: out;      // Data lines
        signal TX_CTL: out;      // Control
        signal TXC: out;         // Clock
    }
    
    // Receive interface  
    interface RX {
        signal RXD[4]: in;       // Data lines
        signal RX_CTL: in;       // Control
        signal RXC: in;          // Clock
    }
    
    // Management interface
    signal MDC: out;             // Management clock
    signal MDIO: inout;          // Management data
    
    // Constraints
    constrain TXC.frequency == 125MHz;
    constrain RXC.frequency == 125MHz;
}
```

## 4. Interface Instantiation

### 4.1 Simple Instantiation

```bhdl
board I2CDemo {
    power VCC_3V3 = 3.3V @ 100mA;
    ground GND;
    
    // Interface instantiation creates connection point
    bus: I2C();  // Uses defaults
    
    // Connect to interface signals
    VCC_3V3 -> bus.pullup_rail;  // Implicit pullup power
    
    // Component connections to interface
    mcu: STM32F4() {
        I2C1_SDA <-> bus.SDA;
        I2C1_SCL <-> bus.SCL;
    }
    
    sensor: BME280() {
        SDA <-> bus.SDA;
        SCL <-> bus.SCL;
    }
}
```

### 4.2 Parameterized Instantiation

```bhdl
// High-speed SPI interface
flash_spi: SPI(frequency=50MHz, mode=3);

// Components declare their interface capabilities
component W25Q128 {
    interface SPI: SPI.slave;  // Has SPI slave interface
}

// Both master and slave use consistent interface connections
mcu: STM32F4() {
    SPI1 <=> flash_spi;  // As master (inferred from component)
}

flash: W25Q128() {
    SPI <=> flash_spi;   // As slave (inferred from component)
}
```

## 5. Interface Connections

### 5.1 Interface-to-Pins Connection

```bhdl
// Explicit pin connections
mcu.I2C1_SDA <-> i2c_bus.SDA;
mcu.I2C1_SCL <-> i2c_bus.SCL;

// Bundled connection (when pin names match)
sensor.{SDA, SCL} <-> i2c_bus.{SDA, SCL};
```

### 5.2 Interface-to-Interface Connection

```bhdl
// Direct interface connection
mcu.I2C1 <=> sensor_i2c;

// Interface with transformation
mcu.I2C1 <=> level_shift(3.3V, 1.8V) <=> sensor.I2C;

// Protocol bridging
uart_debug <=> uart_to_usb() <=> usb_connector;
```

### 5.3 Partial Interface Connection

```bhdl
// Connect only some signals
interface JTAG {
    signal TCK: in;
    signal TMS: in;
    signal TDI: in;
    signal TDO: out;
    signal TRST: in optional;  // Optional signal
}

// Partial connection (TRST not connected)
debug: JTAG();
mcu.{TCK, TMS, TDI, TDO} <-> debug.{TCK, TMS, TDI, TDO};
```

## 6. Interface Roles and Perspectives

### 6.1 Role-Based Signal Direction

```bhdl
interface UART {
    // Role-relative directions
    role transmitter {
        signal TX: out;
        signal RTS: out optional;
        signal CTS: in optional;
    }
    
    role receiver {
        signal RX: in;
    }
}

// Usage with role
mcu.UART1 as transmitter <=> gps.UART as receiver;
```

### 6.2 Automatic Role Inference

```bhdl
// Interface automatically flips directions based on connection context
interface I2C {
    perspective master {
        signal SDA: inout;
        signal SCL: out;  // Master drives clock
    }
    
    perspective slave {
        signal SDA: inout;  
        signal SCL: in;   // Slave receives clock
    }
}

// Automatic perspective selection
mcu.I2C1 as master <=> sensor as slave;  // Explicit
mcu.I2C2 <=> eeprom;  // Inferred from component types
```

## 7. Interface Constraints and Requirements

### 7.1 Electrical Requirements

```bhdl
interface LVDS {
    signal P: out;  // Positive
    signal N: out;  // Negative
    
    // Electrical constraints
    require differential(P, N);
    require impedance(100Ω ± 10%);
    require common_mode(1.2V ± 0.1V);
    require swing(350mV ± 50mV);
}
```

### 7.2 Timing Constraints

```bhdl
interface DDR3 {
    signal CK_P, CK_N: out;     // Differential clock
    signal DQ[8]: inout;        // Data
    signal DQS_P, DQS_N: inout; // Data strobe
    
    // Timing requirements
    constrain DQ.setup_time >= 35ps;
    constrain DQ.hold_time >= 65ps;
    constrain DQS.edge aligned_with DQ.edge ± 10ps;
    constrain CK.frequency == 800MHz;
}
```

### 7.3 Protocol Requirements

```bhdl
interface I3C {
    extends I2C;  // Inherit from I2C
    
    signal SDA: inout;
    signal SCL: inout;
    
    // I3C-specific requirements
    require high_keeper(SDA);  // Bus keeper for high state
    require open_drain(SDA, SCL) when speed <= 400kHz;
    require push_pull(SDA, SCL) when speed > 400kHz;
    
    // Dynamic address assignment support
    capability dynamic_address;
    capability in_band_interrupt;
}
```

## 8. Interface Composition

### 8.1 Interface Extension

```bhdl
// Base interface
interface SerialBase {
    signal TX: out;
    signal RX: in;
}

// Extended interface
interface SerialWithFlow extends SerialBase {
    signal RTS: out;  // Request to send
    signal CTS: in;   // Clear to send
}

// Further extension
interface SerialFull extends SerialWithFlow {
    signal DTR: out;  // Data terminal ready
    signal DSR: in;   // Data set ready
    signal DCD: in;   // Data carrier detect
    signal RI: in;    // Ring indicator
}
```

### 8.2 Interface Aggregation

```bhdl
interface USB_TypeC {
    // USB 2.0
    interface USB2 {
        signal DP: inout;
        signal DN: inout;
    }
    
    // USB 3.2 (dual simplex)
    interface USB3_TX {
        signal TXP1, TXN1: out;
        signal TXP2, TXN2: out;
    }
    
    interface USB3_RX {
        signal RXP1, RXN1: in;
        signal RXP2, RXN2: in;
    }
    
    // Power delivery
    signal VBUS: power;
    signal CC1, CC2: inout;  // Configuration channel
    
    // Alternate modes
    capability DisplayPort;
    capability Thunderbolt;
}
```

## 9. Interface Implementation

### 9.1 Interface Synthesis

Interfaces synthesize to:
1. **Nets/Wires** - Each signal becomes a net
2. **Pullup/Pulldown Resistors** - As specified by requirements
3. **Termination Components** - For impedance matching
4. **Protection Components** - If required by constraints

```bhdl
interface I2C_Protected {
    signal SDA: inout;
    signal SCL: inout;
    
    // These requirements generate components
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
    require esd_protection(SDA, SCL, 5V);
    
    // Synthesizes to:
    // - 2 nets (SDA, SCL)
    // - 2 pullup resistors
    // - 2 ESD protection diodes
}
```

### 9.2 Interface Validation

The analyzer validates:
1. **Signal Compatibility** - Matching signal names and types
2. **Direction Compatibility** - No output-to-output connections
3. **Electrical Compatibility** - Voltage levels, drive strength
4. **Protocol Compatibility** - Required signals present

## 10. Advanced Interface Features

### 10.1 Dynamic Interfaces

```bhdl
interface GPIOBank(width: int = 8) {
    signal GPIO[width]: inout;
    
    // Per-pin configuration
    for i in 0..width {
        capability GPIO[i].pullup;
        capability GPIO[i].pulldown;
        capability GPIO[i].open_drain;
    }
}

// Usage
gpio: GPIOBank(width=16);
```

### 10.2 Protocol State Machines

```bhdl
interface I2C_Stateful {
    signal SDA: inout;
    signal SCL: inout;
    
    // Protocol states
    state IDLE;
    state START;
    state ADDRESS;
    state DATA;
    state ACK;
    state STOP;
    
    // State transitions
    transition IDLE -> START when SDA.falling && SCL.high;
    transition START -> ADDRESS;
    transition ADDRESS -> ACK after 8 clocks;
    transition ACK -> DATA when SDA.low;
    transition ACK -> STOP when SDA.high;
    transition DATA -> ACK after 8 clocks;
    transition ANY -> IDLE when SDA.rising && SCL.high;
}
```

### 10.3 Interface Arrays

```bhdl
interface MemoryBus {
    signal ADDR[32]: out;
    signal DATA[32]: inout;
    signal CS: out;
    signal WE: out;
    signal OE: out;
}

// Array of memory interfaces
board MultiChannelMemory {
    // Four independent memory channels
    mem_channel[4]: MemoryBus();
    
    // Connect each to controller
    for i in 0..4 {
        controller.CH[i] <=> mem_channel[i];
        memory[i] <=> mem_channel[i];
    }
}
```

## 11. Interface Best Practices

### 11.1 Naming Conventions
- Use standard protocol names (I2C, SPI, UART)
- Include version/variant in name (USB2, USB3)
- Use descriptive signal names within interface

### 11.2 Reusability
- Make interfaces as generic as possible
- Use parameters for configuration
- Avoid module-specific assumptions

### 11.3 Documentation
- Document signal roles and timing
- Specify electrical requirements clearly
- Include protocol references

### 11.4 Validation
- Define all required signals
- Specify optional signals explicitly
- Include electrical and timing constraints

## 12. Implementation Roadmap

### Phase 1: Basic Interface Support
- Interface definition parsing
- Signal declarations
- Simple instantiation
- Basic pin-to-interface connections

### Phase 2: Advanced Features
- Parameterized interfaces
- Interface-to-interface connections
- Role-based perspectives
- Electrical requirements

### Phase 3: Full Protocol Support
- Interface inheritance/extension
- State machines
- Dynamic interfaces
- Arrays of interfaces

## 13. Examples

### 13.1 Simple I2C Temperature Sensor

```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: inout;
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
}

board TempSensor {
    power VCC_3V3 = 3.3V @ 50mA;
    ground GND;
    
    // I2C bus instance
    sensor_bus: I2C();
    
    // Microcontroller
    mcu: ATmega328() {
        VCC <- VCC_3V3;
        GND <- GND;
        PC4 <-> sensor_bus.SDA;  // A4/SDA
        PC5 <-> sensor_bus.SCL;  // A5/SCL
    }
    
    // Temperature sensor
    temp: TMP102() {
        VDD <- VCC_3V3;
        GND <- GND;
        SDA <-> sensor_bus.SDA;
        SCL <-> sensor_bus.SCL;
        ADD0 <- GND;  // I2C address select
    }
}
```

### 13.2 SPI Flash Memory

```bhdl
interface SPI(frequency: frequency = 10MHz) {
    signal MOSI: out;
    signal MISO: in;
    signal SCK: out;
    signal CS: out;
    
    constrain SCK.frequency <= frequency;
}

board FlashStorage {
    power VCC_3V3 = 3.3V @ 100mA;
    ground GND;
    
    // High-speed SPI bus
    flash_bus: SPI(frequency=50MHz);
    
    // Microcontroller
    mcu: STM32F4() {
        VDD <- VCC_3V3;
        VSS <- GND;
        SPI1 <=> flash_bus;  // Interface-to-interface
    }
    
    // Flash memory
    flash: W25Q64() {
        VCC <- VCC_3V3;
        GND <- GND;
        DI <- flash_bus.MOSI;
        DO -> flash_bus.MISO;
        CLK <- flash_bus.SCK;
        CS <- flash_bus.CS;
    }
}
```

### 13.3 Multi-Slave I2C System

```bhdl
board I2CNetwork {
    power VCC_3V3 = 3.3V @ 200mA;
    power VCC_1V8 = 1.8V @ 100mA;
    ground GND;
    
    // Main I2C bus at 3.3V
    main_bus: I2C();
    
    // Secondary bus at 1.8V
    sensor_bus: I2C();
    
    // Level shifter between buses
    main_bus <=> level_shift(3.3V, 1.8V) <=> sensor_bus;
    
    // 3.3V devices
    mcu: STM32L4() {
        I2C1 <=> main_bus;
    }
    
    eeprom: AT24C256() {
        {SDA, SCL} <-> main_bus.{SDA, SCL};
    }
    
    // 1.8V devices
    accel: LIS3DH() {
        {SDA, SCL} <-> sensor_bus.{SDA, SCL};
    }
    
    gyro: L3GD20() {
        {SDA, SCL} <-> sensor_bus.{SDA, SCL};
    }
}
```

## 14. Conclusion

Interfaces in BHDL provide a powerful abstraction for managing multi-signal connections with proper typing, electrical requirements, and protocol semantics. They enable cleaner designs, better reusability, and automatic validation of connection compatibility.