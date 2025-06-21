# Interface Perspective Design - Addressing the Slave Interface Problem

## The Problem

In the current design, we have an inconsistency:
```bhdl
// Master uses interface
mcu.SPI1 <=> flash_spi;

// But slave uses individual pins - inconsistent!
flash: W25Q128() {
    SI <- flash_spi.MOSI;
    SO -> flash_spi.MISO;
    SCK <- flash_spi.SCK;
    CS <- flash_spi.CS;
}
```

This is problematic because:
1. It's inconsistent - why do masters get interfaces but slaves don't?
2. It requires users to remember pin mappings (SI vs MOSI)
3. It doesn't scale well for complex protocols

## Solution: Perspective-Aware Interfaces

### Option 1: Automatic Perspective Detection

```bhdl
interface SPI {
    // Define both perspectives
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out;
    }
    
    perspective slave {
        signal MOSI: in;   // Same names, flipped directions
        signal MISO: out;
        signal SCK: in;
        signal CS: in;
    }
}

// Usage - perspective inferred from component type
board SPIExample {
    spi_bus: SPI();
    
    // Master connection (inferred)
    mcu: STM32F4() {
        SPI1 <=> spi_bus;  // Uses master perspective
    }
    
    // Slave connection (inferred)
    flash: W25Q128() {
        SPI <=> spi_bus;   // Uses slave perspective
    }
}
```

### Option 2: Explicit Perspective Selection

```bhdl
// Usage with explicit perspective
mcu.SPI1 as master <=> spi_bus;
flash.SPI as slave <=> spi_bus;
```

### Option 3: Signal Mapping in Components

```bhdl
// Component defines its SPI interface mapping
component W25Q128 {
    // Component declares it has an SPI slave interface
    interface SPI: SPI.slave {
        SI => MOSI;   // Map internal names to interface names
        SO => MISO;
        CLK => SCK;
        CS => CS;
    }
}

// Then use it uniformly
flash: W25Q128() {
    SPI <=> spi_bus;  // Clean and consistent!
}
```

## Recommended Approach: Hybrid Solution

### 1. Components Declare Their Interfaces

```bhdl
// In component library
component STM32F4 {
    // MCU has multiple SPI interfaces as master
    interface SPI1: SPI.master;
    interface SPI2: SPI.master;
    interface I2C1: I2C.master;
    interface I2C2: I2C.master;
}

component W25Q128 {
    // Flash has SPI slave interface
    interface SPI: SPI.slave;
}

component BME280 {
    // Sensor can be I2C slave or SPI slave
    interface I2C: I2C.slave;
    interface SPI: SPI.slave optional;  // Mode-dependent
}
```

### 2. Automatic Perspective Resolution

```bhdl
board Example {
    // Bus instances are perspective-neutral
    main_spi: SPI();
    sensor_i2c: I2C();
    
    // Connections automatically use correct perspective
    mcu: STM32F4() {
        SPI1 <=> main_spi;      // Master perspective
        I2C1 <=> sensor_i2c;    // Master perspective
    }
    
    flash: W25Q128() {
        SPI <=> main_spi;       // Slave perspective
    }
    
    sensor: BME280() {
        I2C <=> sensor_i2c;     // Slave perspective
    }
}
```

### 3. Multi-Slave Support

```bhdl
interface SPI {
    perspective master {
        signal MOSI: out;
        signal MISO: in;    // Shared among slaves
        signal SCK: out;
        signal CS[n]: out;  // Array for multiple slaves
    }
    
    perspective slave {
        signal MOSI: in;
        signal MISO: out tristate;  // Hi-Z when not selected
        signal SCK: in;
        signal CS: in;
    }
}

// Usage with multiple slaves
board MultiSlaveSPI {
    spi_bus: SPI(slaves=3);
    
    mcu: STM32F4() {
        SPI1 <=> spi_bus as master;
    }
    
    flash: W25Q128() {
        SPI <=> spi_bus as slave[0];  // Uses CS[0]
    }
    
    adc: MAX11254() {
        SPI <=> spi_bus as slave[1];  // Uses CS[1]
    }
    
    dac: DAC8568() {
        SPI <=> spi_bus as slave[2];  // Uses CS[2]
    }
}
```

## Benefits of This Approach

1. **Consistency**: All devices use interfaces uniformly
2. **Type Safety**: Can't accidentally connect master-to-master
3. **Clarity**: No need to remember pin mappings
4. **Scalability**: Works for complex multi-device buses
5. **Reusability**: Components declare their interfaces once

## Implementation Notes

### For Component Libraries

Components should declare their interface capabilities:
```bhdl
component MAX3232 {
    // RS-232 transceiver has two channels
    interface CH1_TTL: UART.device;    // TTL side
    interface CH1_RS232: RS232.dte;   // RS-232 side
    interface CH2_TTL: UART.device;
    interface CH2_RS232: RS232.dte;
}
```

### For Protocol Bridges

```bhdl
component FT232R {
    interface USB: USB2.device;
    interface UART: UART.device;
    
    // Bridge connects USB to UART internally
}
```

### For Bidirectional Protocols

```bhdl
interface I2C {
    // No perspective needed - truly bidirectional
    signal SDA: inout;
    signal SCL: inout;  // Both master/slave can stretch
    
    // But we can still indicate role for clarity
    role master;  // Can initiate transactions
    role slave;   // Only responds
    role multi_master;  // Can be both
}
```

## Updated Example

```bhdl
interface SPI(frequency: frequency = 1MHz) {
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

board FlashStorage {
    power VCC_3V3 = 3.3V @ 100mA;
    ground GND;
    
    // SPI bus
    flash_bus: SPI(frequency=50MHz);
    
    // Clean, consistent connections
    mcu: STM32F4() {
        VDD <- VCC_3V3;
        VSS <- GND;
        SPI1 <=> flash_bus;  // As master (inferred)
    }
    
    flash: W25Q64() {
        VCC <- VCC_3V3;
        GND <- GND;
        SPI <=> flash_bus;   // As slave (inferred)
    }
}
```

## Conclusion

By having components declare their interface capabilities and using perspective-aware interfaces, we achieve:
- Consistent syntax for all connections
- Type-safe interface matching
- Clear, readable designs
- No manual pin mapping needed

This solves the asymmetry problem while maintaining the simplicity of BHDL's connection syntax.