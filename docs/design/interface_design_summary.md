# Interface Design Summary

## Core Design Decisions

### 1. Interfaces are Contracts
- Define multi-signal bundles with electrical/protocol requirements
- Synthesize to nets and passive components only
- No active components or hidden behavior

### 2. Perspective-Based Directions
```bhdl
interface SPI {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
    }
    perspective slave {
        signal MOSI: in;
        signal MISO: out;
    }
}
```

### 3. Component-Declared Interfaces
```bhdl
component STM32F4 {
    interface SPI1: SPI.master;
    interface I2C1: I2C.master;
}

component W25Q128 {
    interface SPI: SPI.slave;
}
```

### 4. Uniform Connection Syntax
```bhdl
// Both masters and slaves use same syntax
mcu.SPI1 <=> spi_bus;   // Master
flash.SPI <=> spi_bus;  // Slave
```

### 5. Parameterized Interfaces
```bhdl
interface I2C(frequency: frequency = 100kHz) {
    signal SDA: inout;
    signal SCL: inout;
    
    // Parameter-dependent requirements
    require pullup(SDA, frequency <= 100kHz ? 4.7kΩ : 2.2kΩ);
}

// Usage
fast_i2c: I2C(frequency=400kHz);
```

### 6. Synthesis Model
Interfaces generate:
- **Nets**: One per signal
- **Pullup/Pulldown Resistors**: From `require pullup/pulldown`
- **Termination**: From `require termination`
- **No Logic**: Purely structural

### 7. Key Benefits
1. **Type Safety**: Can't connect incompatible interfaces
2. **Consistency**: Same syntax for all devices
3. **Clarity**: No manual pin mapping
4. **Reusability**: Define once, use many times
5. **Automatic Validation**: Electrical and protocol checks

## Implementation Priority

### Phase 1 (MVP) - 2-3 weeks
- Basic interface definitions with signals
- Simple instantiation and connections
- Pin-to-interface connections
- Basic perspective support

### Phase 2 - 2-3 weeks  
- Parameterized interfaces
- Interface-to-interface connections
- Electrical requirements (pullup, termination)
- Component interface declarations

### Phase 3 - 3-4 weeks
- Hierarchical interfaces
- Arrays of interfaces
- Complex parameter relationships
- Full validation

## Example: Complete System
```bhdl
// Parameterized interface
interface I2C(frequency: frequency = 100kHz) {
    signal SDA: inout;
    signal SCL: inout;
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
}

// Component with interface
component BME280 {
    interface I2C: I2C.slave;
    pin VDD: power in;
    pin GND: ground in;
}

// Usage
board WeatherStation {
    power VCC_3V3 = 3.3V @ 100mA;
    ground GND;
    
    // Interface instance
    sensor_bus: I2C(frequency=400kHz);
    
    // Uniform connections
    mcu: STM32L4() {
        I2C1 <=> sensor_bus;  // Master
    }
    
    sensor: BME280() {
        I2C <=> sensor_bus;   // Slave
        VDD <- VCC_3V3;
        GND <- GND;
    }
}
```

## Next Steps
1. Implement parser support for interface signals and perspectives
2. Add AST nodes for interface content
3. Create analyzer type system for interfaces
4. Implement synthesizer to generate nets and components
5. Write comprehensive test suite