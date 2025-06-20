# Port Mapping Modules in BHDL

## Overview

Port mapping modules provide flexible interface adaptation, pin remapping, and protocol bridging. They're essential for:
- Adapting between different component packages
- Creating reusable interface modules
- Handling pin multiplexing
- Protocol translation

## Design Approaches

### 1. Basic Port Mapping

```bhdl
// Simple 1:1 port mapping
module UARTAdapter {
    // Left side - MCU interface
    pin MCU_TX: signal out;
    pin MCU_RX: signal in;
    pin MCU_RTS: signal out;
    pin MCU_CTS: signal in;
    
    // Right side - External interface  
    pin EXT_TX: signal in;
    pin EXT_RX: signal out;
    pin EXT_RTS: signal in;
    pin EXT_CTS: signal out;
    
    // Direct connections (note direction swap)
    MCU_TX -> EXT_RX;
    EXT_TX -> MCU_RX;
    MCU_RTS -> EXT_CTS;
    EXT_RTS -> MCU_CTS;
}
```

### 2. Parameterized Pin Mapping

```bhdl
// Configurable GPIO mapping
module GPIOMapper(
    num_pins: int = 8,
    mapping: int[] = [0,1,2,3,4,5,6,7]  // Default 1:1
) {
    pin GPIO_A[num_pins]: signal inout;
    pin GPIO_B[num_pins]: signal inout;
    
    // Map according to parameter
    generate for i in 0..num_pins {
        GPIO_A[i] <-> GPIO_B[mapping[i]];
    }
}

// Usage with custom mapping
board System {
    // Swap pairs: 0<->1, 2<->3, etc
    mapper: GPIOMapper(num_pins=8, mapping=[1,0,3,2,5,4,7,6]) {
        MCU.GPIO[0..7] <-> .GPIO_A[0..7];
        CONNECTOR.PINS[0..7] <-> .GPIO_B[0..7];
    }
}
```

### 3. Interface Width Adaptation

```bhdl
// SPI with optional pins
module SPIInterface(
    use_cs: bool = true,
    num_cs: int = 1,
    use_interrupt: bool = false
) {
    // Core SPI pins (always present)
    pin MOSI: signal out;
    pin MISO: signal in;
    pin SCK: signal out;
    
    // Optional chip selects
    when (use_cs) {
        pin CS[num_cs]: signal out;
    }
    
    // Optional interrupt
    when (use_interrupt) {
        pin INT: signal in;
    }
}

// Adapter between different SPI variants
module SPIAdapter {
    // Full-featured side
    spi_full: SPIInterface(use_cs=true, num_cs=4, use_interrupt=true) {
        // External connections
    }
    
    // Basic side  
    spi_basic: SPIInterface(use_cs=true, num_cs=1, use_interrupt=false) {
        // MCU connections
    }
    
    // Core connections
    spi_basic.MOSI -> spi_full.MOSI;
    spi_full.MISO -> spi_basic.MISO;
    spi_basic.SCK -> spi_full.SCK;
    
    // Map first CS only
    spi_basic.CS[0] -> spi_full.CS[0];
    
    // Pull unused CS high
    VCC -> R1: Res(10k).1 -> spi_full.CS[1];
    VCC -> R2: Res(10k).1 -> spi_full.CS[2];
    VCC -> R3: Res(10k).1 -> spi_full.CS[3];
}
```

### 4. Protocol Bridge Mapping

```bhdl
// I2C to SPI bridge
module I2CToSPIBridge {
    // I2C slave interface
    pin SDA: signal inout;
    pin SCL: signal in;
    
    // SPI master interface
    pin MOSI: signal out;
    pin MISO: signal in;
    pin SCK: signal out;
    pin CS: signal out;
    
    // Bridge IC
    bridge: SC18IS602B {
        SDA <-> .SDA;
        SCL -> .SCL;
        .MOSI -> MOSI;
        MISO -> .MISO;
        .SPICLK -> SCK;
        .CS -> CS;
    }
    
    // Pull-ups for I2C
    VCC -> R1: Res(4.7k).1 -> SDA;
    VCC -> R2: Res(4.7k).1 -> SCL;
}
```

### 5. Differential to Single-Ended Mapping

```bhdl
module DifferentialAdapter(
    num_pairs: int = 4,
    termination: resistance = 100
) {
    // Differential side
    pin D_P[num_pairs]: signal in;
    pin D_N[num_pairs]: signal in;
    
    // Single-ended side
    pin SE[num_pairs]: signal out;
    
    generate for i in 0..num_pairs {
        // Termination resistors
        R_term[i]: Res(termination) {
            D_P[i] -> .1;
            D_N[i] -> .2;
        }
        
        // Differential receiver
        U[i]: DS90LV048A {
            D_P[i] -> .IN_P;
            D_N[i] -> .IN_N;
            .OUT -> SE[i];
            VCC -> .VCC;
            GND -> .GND;
        }
    }
}
```

### 6. Voltage Level Translation

```bhdl
module LevelShifter(
    channels: int = 8,
    vcc_a: voltage = 3.3V,
    vcc_b: voltage = 5V,
    bidirectional: bool = true
) {
    // Power pins
    pin VCCA: power in;
    pin VCCB: power in;
    
    // I/O pins
    pin A[channels]: signal inout;
    pin B[channels]: signal inout;
    
    when (bidirectional) {
        // Auto-direction sensing level shifter
        translator: TXB0108 {
            VCCA -> .VCCA;
            VCCB -> .VCCB;
            A[0..7] <-> .A[1..8];
            B[0..7] <-> .B[1..8];
            VCCA -> .OE;  // Always enabled
        }
    } else {
        // Unidirectional level shifters
        generate for i in 0..channels {
            trans[i]: SN74LVC1T45 {
                VCCA -> .VCCA;
                VCCB -> .VCCB;
                A[i] -> .A;
                .B -> B[i];
                VCCA -> .DIR;  // A to B
            }
        }
    }
}
```

### 7. Connector Pin Mapping

```bhdl
// Map logical functions to physical connector
module ConnectorMapping {
    // Logical interface
    pin POWER: power in;
    pin GND: ground;
    pin USB_DP: signal inout;
    pin USB_DN: signal inout;
    pin I2C_SDA: signal inout;
    pin I2C_SCL: signal out;
    pin SPI_MOSI: signal out;
    pin SPI_MISO: signal in;
    pin SPI_SCK: signal out;
    pin SPI_CS: signal out;
    
    // Physical connector
    connector: Connector_2x10 {
        // Map by function and signal integrity needs
        POWER    -> .1;   // Pin 1 - Power
        GND      -> .2;   // Pin 2 - Ground
        POWER    -> .3;   // Pin 3 - Power (parallel)
        GND      -> .4;   // Pin 4 - Ground (parallel)
        
        USB_DP   -> .5;   // Pins 5,6 - Differential pair
        USB_DN   -> .6;
        
        I2C_SDA  -> .7;   // Pins 7,8 - I2C pair
        I2C_SCL  -> .8;
        
        SPI_MOSI -> .9;   // Pins 9-12 - SPI group
        SPI_MISO -> .10;
        SPI_SCK  -> .11;
        SPI_CS   -> .12;
        
        GND      -> .13;  // More grounds for signal integrity
        GND      -> .14;
        
        // Pins 15-20 unused
        NC       -> .15;
        NC       -> .16;
        NC       -> .17;
        NC       -> .18;
        NC       -> .19;
        NC       -> .20;
    }
}
```

### 8. Dynamic Pin Multiplexing

```bhdl
// Multiplexed pin configuration
module PinMux(
    mode: string = "uart"  // "uart", "spi", "i2c", "gpio"
) {
    // Shared physical pins
    pin PIN_1: signal inout;
    pin PIN_2: signal inout;
    pin PIN_3: signal inout;
    pin PIN_4: signal inout;
    
    // Function-specific pins
    when (mode == "uart") {
        pin UART_TX: signal out;
        pin UART_RX: signal in;
        pin UART_RTS: signal out;
        pin UART_CTS: signal in;
        
        UART_TX -> PIN_1;
        PIN_2 -> UART_RX;
        UART_RTS -> PIN_3;
        PIN_4 -> UART_CTS;
    }
    
    when (mode == "spi") {
        pin SPI_MOSI: signal out;
        pin SPI_MISO: signal in;
        pin SPI_SCK: signal out;
        pin SPI_CS: signal out;
        
        SPI_MOSI -> PIN_1;
        PIN_2 -> SPI_MISO;
        SPI_SCK -> PIN_3;
        SPI_CS -> PIN_4;
    }
    
    when (mode == "i2c") {
        pin I2C_SDA: signal inout;
        pin I2C_SCL: signal out;
        
        I2C_SDA <-> PIN_1;
        I2C_SCL -> PIN_2;
        
        // Pins 3,4 unused - add pull-downs
        PIN_3 -> R1: Res(10k).1 -> GND;
        PIN_4 -> R2: Res(10k).1 -> GND;
    }
    
    when (mode == "gpio") {
        pin GPIO[4]: signal inout;
        
        GPIO[0] <-> PIN_1;
        GPIO[1] <-> PIN_2;
        GPIO[2] <-> PIN_3;
        GPIO[3] <-> PIN_4;
    }
}
```

### 9. Port Expansion

```bhdl
// I2C GPIO expander for more pins
module GPIOExpander(
    base_addr: int = 0x20,
    num_ports: int = 2  // 1 or 2
) {
    // I2C interface
    pin SDA: signal inout;
    pin SCL: signal in;
    pin INT: signal out;
    
    // Expanded GPIO
    pin GPIO_A[8]: signal inout;
    when (num_ports == 2) {
        pin GPIO_B[8]: signal inout;
    }
    
    // Expander IC selection
    when (num_ports == 1) {
        expander: PCF8574 {
            SDA <-> .SDA;
            SCL -> .SCL;
            .INT -> INT;
            GPIO_A[0..7] <-> .P[0..7];
            
            // Address configuration
            attribute i2c_addr = base_addr;
        }
    } else {
        expander: MCP23017 {
            SDA <-> .SDA;
            SCL -> .SCL;
            .INTA -> INT;  // Combine interrupts
            .INTB -> INT;
            
            GPIO_A[0..7] <-> .GPA[0..7];
            GPIO_B[0..7] <-> .GPB[0..7];
            
            // Address pins
            signal addr = base_addr & 0x07;
            when (addr & 0x01) { VCC -> .A0; } else { GND -> .A0; }
            when (addr & 0x02) { VCC -> .A1; } else { GND -> .A1; }
            when (addr & 0x04) { VCC -> .A2; } else { GND -> .A2; }
        }
    }
}
```

### 10. Complex Interface Adaptation

```bhdl
// Memory interface width adapter
module MemoryAdapter(
    width_in: int = 16,
    width_out: int = 8
) {
    // Wide side
    pin ADDR_W[20]: signal in;
    pin DATA_W[width_in]: signal inout;
    pin CS_W: signal in;
    pin OE_W: signal in;
    pin WE_W: signal in;
    
    // Narrow side
    pin ADDR_N[21]: signal out;  // One extra address bit
    pin DATA_N[width_out]: signal inout;
    pin CS_N: signal out;
    pin OE_N: signal out;
    pin WE_N: signal out;
    
    // Control logic
    logic: CPLD {
        // Address mapping
        ADDR_W[0..19] -> .ADDR_IN[0..19];
        .ADDR_OUT[0..19] -> ADDR_N[0..19];
        
        // Byte select from control signals
        .ADDR_OUT[20] -> ADDR_N[20];
        
        // Data path multiplexing
        when (width_in == 16 && width_out == 8) {
            // 16 to 8 bit conversion
            DATA_W[0..7] <-> .DATA_HIGH[0..7];
            DATA_W[8..15] <-> .DATA_LOW[0..7];
            DATA_N[0..7] <-> .DATA_OUT[0..7];
        }
        
        // Control signal adaptation
        CS_W -> .CS_IN;
        OE_W -> .OE_IN;
        WE_W -> .WE_IN;
        .CS_OUT -> CS_N;
        .OE_OUT -> OE_N;
        .WE_OUT -> WE_N;
    }
}
```

## Implementation Considerations

### 1. Pin Direction Validation
```rust
// Analyzer should verify compatible directions
fn validate_port_connection(from_pin: &Pin, to_pin: &Pin) -> Result<()> {
    match (from_pin.direction, to_pin.direction) {
        (Out, In) => Ok(()),
        (InOut, InOut) => Ok(()),
        (In, Out) => Err("Cannot connect input to output"),
        // ... other cases
    }
}
```

### 2. Width Matching
```rust
// Check array dimensions match
fn validate_array_connection(from_array: &PinArray, to_array: &PinArray) -> Result<()> {
    if from_array.size != to_array.size {
        return Err(format!(
            "Array size mismatch: {} != {}", 
            from_array.size, 
            to_array.size
        ));
    }
    Ok(())
}
```

### 3. Mapping Tables
```bhdl
// Support mapping tables as attributes
module FlexibleMapper {
    // Mapping as attribute array
    attribute pin_map = [
        (0, 7),  // A[0] -> B[7]
        (1, 6),  // A[1] -> B[6]
        (2, 5),  // A[2] -> B[5]
        (3, 4),  // A[3] -> B[4]
    ];
    
    generate for (from, to) in pin_map {
        A[from] <-> B[to];
    }
}
```

## Benefits

1. **Flexibility**: Adapt between different interfaces
2. **Reusability**: Create universal adapter modules
3. **Clarity**: Port mapping logic is explicit
4. **Validation**: Type system ensures correct connections
5. **Parameterization**: Configure mappings without new modules

This completes our hierarchical module system with port mapping capabilities!