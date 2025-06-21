# Parameterized Interfaces Design Document

## Use Cases for Interface Parameters

### 1. Configuration Parameters

#### Speed/Frequency Configuration
```bhdl
interface I2C(frequency: frequency = 100kHz) {
    signal SDA: inout;
    signal SCL: inout;
    
    // Constraints based on frequency
    require pullup(SDA, frequency <= 100kHz ? 4.7kΩ : 2.2kΩ);
    require pullup(SCL, frequency <= 100kHz ? 4.7kΩ : 2.2kΩ);
    
    // Timing constraints
    constrain SCL.frequency <= frequency;
    constrain SDA.setup_time >= (frequency <= 100kHz ? 250ns : 100ns);
}

// Usage
slow_i2c: I2C();                    // 100kHz default
fast_i2c: I2C(frequency=400kHz);    // Fast mode
fm_plus_i2c: I2C(frequency=1MHz);   // Fast mode plus
```

#### Protocol Modes
```bhdl
interface SPI(mode: int = 0, frequency: frequency = 1MHz) {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS: out;
    }
    
    // Mode determines clock polarity and phase
    constrain SCK.idle_state = (mode >= 2) ? high : low;  // CPOL
    constrain SCK.sample_edge = (mode & 1) ? falling : rising;  // CPHA
    constrain SCK.frequency <= frequency;
}

// Different modes for different devices
flash_spi: SPI(mode=0, frequency=50MHz);   // Mode 0 for flash
adc_spi: SPI(mode=3, frequency=20MHz);     // Mode 3 for ADC
```

### 2. Structural Parameters

#### Bus Width
```bhdl
interface ParallelBus(width: int = 8) {
    signal DATA[width]: inout;
    signal ADDR[width]: out;
    signal RD: out;
    signal WR: out;
    signal CS: out;
    
    // Width-dependent timing
    constrain DATA.setup_time >= width * 0.5ns;  // More bits = more skew
}

// Usage
narrow_bus: ParallelBus(width=8);   // 8-bit bus
wide_bus: ParallelBus(width=32);    // 32-bit bus
```

#### Number of Slaves/Channels
```bhdl
interface SPIMultiSlave(slaves: int = 1, frequency: frequency = 1MHz) {
    perspective master {
        signal MOSI: out;
        signal MISO: in;
        signal SCK: out;
        signal CS[slaves]: out;  // Array of chip selects
    }
    
    // Per-slave perspective
    perspective slave[n] {
        signal MOSI: in;
        signal MISO: out;
        signal SCK: in;
        signal CS: in = CS[n];  // Maps to specific CS
    }
}

// Usage
multi_spi: SPIMultiSlave(slaves=4, frequency=10MHz);

// Connect slaves
flash.SPI <=> multi_spi as slave[0];
adc.SPI <=> multi_spi as slave[1];
dac.SPI <=> multi_spi as slave[2];
eeprom.SPI <=> multi_spi as slave[3];
```

### 3. Electrical Parameters

#### Voltage Levels
```bhdl
interface LVDS(voltage: voltage = 1.2V, swing: voltage = 350mV) {
    signal P: out;
    signal N: out;
    
    require differential(P, N);
    require common_mode(voltage);
    require differential_swing(swing);
    require termination(100Ω);
}

// Different voltage standards
lvds_1v2: LVDS();                              // Standard 1.2V
lvds_2v5: LVDS(voltage=2.5V, swing=450mV);    // 2.5V LVDS
```

#### Drive Strength
```bhdl
interface GPIO(drive: current = 4mA, slew: string = "slow") {
    signal IO: inout;
    
    require drive_strength(drive);
    require slew_rate(slew);
    
    // Different pullup based on drive strength
    require pullup(IO, drive <= 4mA ? 10kΩ : 4.7kΩ) when pullup_enabled;
}

// Usage
weak_gpio: GPIO(drive=2mA, slew="slow");      // Low power
strong_gpio: GPIO(drive=12mA, slew="fast");   // High speed
```

### 4. Protocol Variants

#### UART with/without Flow Control
```bhdl
interface UART(
    baud_rate: frequency = 9600,
    flow_control: bool = false,
    parity: string = "none"
) {
    signal TX: out;
    signal RX: in;
    
    // Conditional signals based on parameters
    signal RTS: out when flow_control;
    signal CTS: in when flow_control;
    
    // Parity affects data format
    constrain data_bits = (parity == "none") ? 8 : 7;
    constrain stop_bits = (baud_rate > 115200) ? 2 : 1;
}

// Usage
simple_uart: UART();  // No flow control
full_uart: UART(baud_rate=115200, flow_control=true);
```

#### I2C vs I3C
```bhdl
interface I2C_I3C(version: string = "I2C", speed_class: string = "standard") {
    signal SDA: inout;
    signal SCL: inout;
    
    // I3C additions
    require high_keeper(SDA) when version == "I3C";
    require push_pull_capable(SDA, SCL) when version == "I3C";
    
    // Speed-dependent requirements
    require pullup(SDA, 
        speed_class == "standard" ? 4.7kΩ :
        speed_class == "fast" ? 2.2kΩ :
        speed_class == "fast_plus" ? 1kΩ : 1kΩ
    ) when version == "I2C";
}
```

### 5. Complex Protocol Configuration

#### USB with Different Speeds
```bhdl
interface USB(speed: string = "high_speed") {
    // Common signals
    interface Power {
        signal VBUS: power;
        signal GND: ground;
    }
    
    // Speed-dependent signals
    generate if (speed == "low_speed" || speed == "full_speed") {
        signal DP: inout;
        signal DN: inout;
    } else if (speed == "high_speed") {
        signal DP: inout;
        signal DN: inout;
        // Same signals but different electrical requirements
        require impedance(DP, DN, 90Ω ± 10%);
    } else if (speed == "super_speed") {
        // USB 3.0 adds SuperSpeed pairs
        interface SS_TX {
            signal TXP: out;
            signal TXN: out;
        }
        interface SS_RX {
            signal RXP: in;
            signal RXN: in;
        }
        // Still has USB 2.0 for backwards compatibility
        signal DP: inout;
        signal DN: inout;
    }
}

// Usage
usb2: USB(speed="high_speed");
usb3: USB(speed="super_speed");
```

#### Memory Interfaces
```bhdl
interface DDR(
    version: int = 3,
    width: int = 16,
    ranks: int = 1
) {
    // Address/command
    signal ADDR[version >= 4 ? 17 : 16]: out;
    signal BA[version >= 4 ? 2 : 3]: out;  // Bank address
    signal RAS, CAS, WE: out;
    
    // Data
    signal DQ[width]: inout;
    signal DQS[width/8]: inout;  // Data strobe
    signal DM[width/8]: out;     // Data mask
    
    // Clocking
    signal CK_P, CK_N: out;
    
    // Per-rank signals
    signal CS[ranks]: out;
    signal ODT[ranks]: out when version >= 2;
    
    // Timing parameters based on version
    constrain CK.frequency = 
        version == 1 ? 200MHz :
        version == 2 ? 400MHz :
        version == 3 ? 800MHz :
        version == 4 ? 1600MHz : 1600MHz;
}

// Usage
ddr3_x16: DDR(version=3, width=16, ranks=1);
ddr4_x64: DDR(version=4, width=64, ranks=2);
```

## Design Considerations

### 1. Parameter Types
- **Electrical**: voltage, current, frequency, resistance
- **Structural**: int (for widths, counts)
- **Configuration**: string (for modes), bool (for options)
- **Timing**: time units (ns, ps)

### 2. Parameter Validation
```bhdl
interface I2C(frequency: frequency = 100kHz) {
    // Validate parameter ranges
    constrain frequency in [10kHz, 1MHz];
    
    // Error on invalid configurations
    assert frequency == 100kHz || 
           frequency == 400kHz || 
           frequency == 1MHz
        : "I2C only supports standard (100kHz), fast (400kHz), or fast-plus (1MHz) modes";
}
```

### 3. Parameter Dependencies
```bhdl
interface SerialProtocol(
    protocol: string = "UART",
    speed: frequency = 9600
) {
    // Speed limits depend on protocol
    constrain speed <= match protocol {
        "UART" => 1MHz,
        "RS232" => 115200,
        "RS485" => 10MHz,
        _ => 1MHz
    };
}
```

### 4. Default Behaviors
```bhdl
interface FlexibleBus(
    mode: string = "auto"  // auto, parallel, serial
) {
    generate if (mode == "auto") {
        // Determine mode from connections
        // This is complex - maybe not for MVP
    } else if (mode == "parallel") {
        signal DATA[8]: inout;
        signal ADDR[16]: out;
    } else if (mode == "serial") {
        signal SDATA: inout;
        signal SCLK: out;
    }
}
```

## Implementation Guidelines

### Phase 1: Basic Parameters
- Support for primitive types (int, frequency, voltage, string, bool)
- Simple conditional signals (`when` clause)
- Basic validation

### Phase 2: Advanced Features
- Arrays sized by parameters
- Complex conditional generation
- Parameter-based calculations
- Cross-parameter validation

### Phase 3: Full Support
- Auto-detection of optimal parameters
- Parameter inference from connections
- Complex parameter relationships

## Conclusion

Parameterized interfaces are essential for:
1. **Protocol Configuration** - Speed, mode, options
2. **Structural Flexibility** - Bus width, channel count
3. **Electrical Adaptation** - Voltage levels, drive strength
4. **Standard Compliance** - Different versions of protocols
5. **Reusability** - One interface definition, many configurations

The parameter system should be powerful enough to express real-world requirements while remaining intuitive and predictable.