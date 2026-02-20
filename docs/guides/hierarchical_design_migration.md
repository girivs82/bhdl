# Hierarchical Design Migration Guide

This guide helps you migrate from flat BHDL designs to hierarchical entity-based designs, enabling better code organization, reusability, and team collaboration.

## Table of Contents
1. [Why Use Hierarchical Design?](#why-use-hierarchical-design)
2. [Basic Migration Steps](#basic-migration-steps)
3. [Common Patterns](#common-patterns)
4. [Before and After Examples](#before-and-after-examples)
5. [Best Practices](#best-practices)
6. [Troubleshooting](#troubleshooting)

## Why Use Hierarchical Design?

### Benefits
- **Code Reuse**: Define once, use many times
- **Team Collaboration**: Different engineers can work on separate entities
- **Better Organization**: Logical grouping of functionality
- **Easier Testing**: Test entities in isolation
- **Clear Interfaces**: Well-defined entity boundaries
- **Automatic Component Naming**: Hierarchical reference designators (e.g., `filter1.R1`)

### When to Use Entities
- Repeated circuit patterns (filters, regulators, indicators)
- Functional blocks (power management, sensor interfaces)
- Complex subsystems (communication interfaces, analog front-ends)
- Team boundaries (different engineers owning different parts)

## Basic Migration Steps

### Step 1: Identify Repeated Patterns
Look for circuit patterns that appear multiple times:
```bhdl
// Before: Repeated LED circuits
VCC -> Res(1kΩ).1 -> LED(red).A;
LED(red).K -> GND;

VCC -> Res(1kΩ).1 -> LED(green).A;
LED(green).K -> GND;

VCC -> Res(1kΩ).1 -> LED(blue).A;
LED(blue).K -> GND;
```

### Step 2: Extract into Entity
Create an entity for the repeated pattern:
```bhdl
entity StatusLED(color: string = "red", R_value: resistance = 1kΩ) {
    pin VCC: power in;
    pin GND: ground in;
    pin EN: signal in;  // Optional enable
    
    EN -> Res(R_value).1 -> LED(color).A;
    LED(color).K -> GND;
}
```

### Step 3: Replace with Entity Instances
```bhdl
// After: Using entity instances
board StatusPanel {
    power VCC = 3.3V @ 100mA;
    ground GND;
    
    power_led: StatusLED(color="green") {
        VCC <- VCC;
        GND <- GND;
        EN <- power_good;
    }
    
    error_led: StatusLED(color="red") {
        VCC <- VCC;
        GND <- GND;
        EN <- error_signal;
    }
    
    comm_led: StatusLED(color="blue", R_value=470Ω) {
        VCC <- VCC;
        GND <- GND;
        EN <- comm_active;
    }
}
```

## Common Patterns

### 1. Power Supply Entities
```bhdl
// Reusable power regulator entity
entity PowerRail(Vin: voltage, Vout: voltage, Imax: current = 1A) {
    pin IN: power in;
    pin OUT: power out;
    pin GND: ground in;
    pin EN: signal in;
    
    // Input protection
    IN -> tvs: TVSDiode(Vin * 1.2).K;
    tvs.A -> GND;
    
    // Regulation
    IN -> reg: LinearReg(Vout, Imax).IN;
    reg.OUT -> OUT;
    reg.GND -> GND;
    EN -> reg.EN;
    
    // Output filtering
    OUT -> Cap(10μF).1 -> GND;
    OUT -> Cap(100nF).1 -> GND;
}
```

### 2. Filter Entities
```bhdl
// Configurable filter entity
entity Filter(type: string = "RC", fc: frequency = 1kHz) {
    pin IN: signal in;
    pin OUT: signal out;
    pin GND: ground in;
    
    generate if (type == "RC") {
        // RC low-pass filter
        const R = 1 / (2 * π * fc * 100nF);
        IN -> Res(R).1;
        Res(R).2 -> OUT;
        OUT -> Cap(100nF).1 -> GND;
    } else if (type == "LC") {
        // LC low-pass filter
        const L = 50Ω / (2 * π * fc);
        const C = 1 / (2 * π * fc * 50Ω);
        IN -> Ind(L).1;
        Ind(L).2 -> OUT;
        OUT -> Cap(C).1 -> GND;
    }
}
```

### 3. Interface Entities
```bhdl
// I2C interface with protection
entity I2CInterface(voltage: voltage = 3.3V) {
    pin SDA_EXT: signal inout;  // External connection
    pin SCL_EXT: signal inout;  // External connection
    pin SDA_INT: signal inout;  // Internal connection
    pin SCL_INT: signal inout;  // Internal connection
    pin VCC: power in;
    pin GND: ground in;
    
    // Pull-up resistors
    VCC -> Res(4.7kΩ).1 -> SDA_EXT;
    VCC -> Res(4.7kΩ).1 -> SCL_EXT;
    
    // ESD protection
    SDA_EXT -> TVSDiode(voltage * 1.2).K -> GND;
    SCL_EXT -> TVSDiode(voltage * 1.2).K -> GND;
    
    // Series resistors for protection
    SDA_EXT -> Res(100Ω).1;
    Res(100Ω).2 -> SDA_INT;
    
    SCL_EXT -> Res(100Ω).1;
    Res(100Ω).2 -> SCL_INT;
}
```

## Before and After Examples

### Example 1: Sensor Board

**Before (Flat Design):**
```bhdl
board SensorBoard {
    power VCC_3V3 = 3.3V @ 500mA;
    ground GND;
    
    // Sensor 1 connections
    VCC_3V3 -> Cap(100nF).1 -> GND;  // Decoupling
    VCC_3V3 -> sensor1: BME280().VDD;
    sensor1.GND -> GND;
    sensor1.SDA -> Res(100Ω).1;      // Protection
    Res(100Ω).2 -> mcu.I2C1_SDA;
    sensor1.SCL -> Res(100Ω).1;      // Protection
    Res(100Ω).2 -> mcu.I2C1_SCL;
    VCC_3V3 -> Res(4.7kΩ).1 -> mcu.I2C1_SDA;  // Pull-up
    VCC_3V3 -> Res(4.7kΩ).1 -> mcu.I2C1_SCL;  // Pull-up
    
    // Sensor 2 connections (same pattern)
    VCC_3V3 -> Cap(100nF).1 -> GND;  // Decoupling
    VCC_3V3 -> sensor2: BME280().VDD;
    sensor2.GND -> GND;
    sensor2.SDA -> Res(100Ω).1;      // Protection
    Res(100Ω).2 -> mcu.I2C2_SDA;
    sensor2.SCL -> Res(100Ω).1;      // Protection
    Res(100Ω).2 -> mcu.I2C2_SCL;
    VCC_3V3 -> Res(4.7kΩ).1 -> mcu.I2C2_SDA;  // Pull-up
    VCC_3V3 -> Res(4.7kΩ).1 -> mcu.I2C2_SCL;  // Pull-up
}
```

**After (Hierarchical Design):**
```bhdl
entity SensorInterface() {
    pin VCC: power in;
    pin GND: ground in;
    pin SDA: signal inout;
    pin SCL: signal inout;
    
    // Local decoupling
    VCC -> Cap(100nF).1 -> GND;
    
    // Sensor with built-in protection
    sensor: BME280() {
        VDD <- VCC;
        GND <- GND;
        SDA <-> protection_sda;
        SCL <-> protection_scl;
    }
    
    // I2C protection and pull-ups
    i2c: I2CInterface(voltage=3.3V) {
        VCC <- VCC;
        GND <- GND;
        SDA_EXT <-> protection_sda;
        SCL_EXT <-> protection_scl;
        SDA_INT <-> SDA;
        SCL_INT <-> SCL;
    }
}

board SensorBoard {
    power VCC_3V3 = 3.3V @ 500mA;
    ground GND;
    
    // Clean, modular sensor connections
    sensor1: SensorInterface() {
        VCC <- VCC_3V3;
        GND <- GND;
        SDA <-> mcu.I2C1_SDA;
        SCL <-> mcu.I2C1_SCL;
    }
    
    sensor2: SensorInterface() {
        VCC <- VCC_3V3;
        GND <- GND;
        SDA <-> mcu.I2C2_SDA;
        SCL <-> mcu.I2C2_SCL;
    }
}
```

### Example 2: Power Distribution

**Before (Flat Design):**
```bhdl
board PowerBoard {
    power VIN_12V = 12V @ 5A;
    ground GND;
    
    // 5V rail
    VIN_12V -> buck1: BuckConverter(5V, 3A).VIN;
    buck1.VOUT -> VCC_5V;
    buck1.GND -> GND;
    VCC_5V -> Cap(100μF).1 -> GND;
    VCC_5V -> Cap(10μF).1 -> GND;
    VCC_5V -> Cap(100nF).1 -> GND;
    
    // 3.3V rail  
    VCC_5V -> ldo1: LinearReg(3.3V, 1A).IN;
    ldo1.OUT -> VCC_3V3;
    ldo1.GND -> GND;
    VCC_3V3 -> Cap(10μF).1 -> GND;
    VCC_3V3 -> Cap(100nF).1 -> GND;
    
    // 1.8V rail
    VCC_5V -> ldo2: LinearReg(1.8V, 500mA).IN;
    ldo2.OUT -> VCC_1V8;
    ldo2.GND -> GND;
    VCC_1V8 -> Cap(10μF).1 -> GND;
    VCC_1V8 -> Cap(100nF).1 -> GND;
}
```

**After (Hierarchical Design):**
```bhdl
entity PowerStage(Vin: voltage, Vout: voltage, Imax: current,
                  topology: string = "linear") {
    pin IN: power in;
    pin OUT: power out;
    pin GND: ground in;
    pin EN: signal in;
    pin PGOOD: signal out;
    
    generate if (topology == "buck") {
        IN -> conv: BuckConverter(Vout, Imax).VIN;
        conv.VOUT -> OUT;
        conv.GND -> GND;
        EN -> conv.EN;
        conv.PGOOD -> PGOOD;
    } else {
        IN -> reg: LinearReg(Vout, Imax).IN;
        reg.OUT -> OUT;
        reg.GND -> GND;
        EN -> reg.EN;
        // Simple power good (could be enhanced)
        OUT -> comp: Comparator(Vout * 0.9).IN;
        comp.OUT -> PGOOD;
    }
    
    // Output filtering based on current
    generate if (Imax >= 1A) {
        OUT -> Cap(100μF).1 -> GND;
        OUT -> Cap(10μF).1 -> GND;
        OUT -> Cap(100nF).1 -> GND;
    } else {
        OUT -> Cap(10μF).1 -> GND;
        OUT -> Cap(100nF).1 -> GND;
    }
}

board PowerBoard {
    power VIN_12V = 12V @ 5A;
    ground GND;
    
    // Clean power architecture
    stage_5v: PowerStage(Vin=12V, Vout=5V, Imax=3A, topology="buck") {
        IN <- VIN_12V;
        OUT -> VCC_5V;
        GND <- GND;
        EN <- power_enable;
        PGOOD -> pgood_5v;
    }
    
    stage_3v3: PowerStage(Vin=5V, Vout=3.3V, Imax=1A) {
        IN <- VCC_5V;
        OUT -> VCC_3V3;
        GND <- GND;
        EN <- pgood_5v;  // Sequencing
        PGOOD -> pgood_3v3;
    }
    
    stage_1v8: PowerStage(Vin=5V, Vout=1.8V, Imax=500mA) {
        IN <- VCC_5V;
        OUT -> VCC_1V8;
        GND <- GND;
        EN <- pgood_3v3;  // Sequencing
        PGOOD -> pgood_1v8;
    }
}
```

## Best Practices

### 1. Entity Naming
- Use descriptive names that indicate function
- Include key parameters in entity names if helpful
- Follow consistent naming conventions

```bhdl
// Good entity names
entity PowerRegulator() { ... }
entity I2CInterface() { ... }
entity AnalogInputFilter() { ... }

// Avoid generic names
entity Entity1() { ... }  // Bad
entity Thing() { ... }    // Bad
```

### 2. Parameter Design
- Use typed parameters with meaningful defaults
- Put most commonly changed parameters first
- Document parameter ranges and constraints

```bhdl
entity Filter(
    fc: frequency = 1kHz,        // Cutoff frequency
    type: string = "lowpass",    // "lowpass", "highpass", "bandpass"
    order: int = 2               // Filter order (1-4)
) {
    // Validate parameters
    constrain fc > 0Hz && fc < 1MHz;
    constrain order >= 1 && order <= 4;
    ...
}
```

### 3. Pin Organization
- Group related pins together
- Use consistent naming conventions
- Document pin purposes

```bhdl
entity MCUInterface() {
    // Power pins
    pin VCC: power in;
    pin GND: ground in;
    
    // Communication pins
    pin SPI_MOSI: signal out;
    pin SPI_MISO: signal in;
    pin SPI_SCK: signal out;
    pin SPI_CS: signal in;
    
    // Control pins
    pin RESET: signal in;
    pin IRQ: signal out;
}
```

### 4. Entity Boundaries
- Create entities at natural functional boundaries
- Keep entities focused on a single responsibility
- Avoid entities that are too large or too small

```bhdl
// Good: Clear functional boundary
entity USBPowerInput() {
    pin VBUS: power in;
    pin D_P: signal inout;
    pin D_N: signal inout;
    pin GND: ground in;
    // USB protection and filtering
}

// Too large: Should be split
entity EntireAnalogSection() {
    // Hundreds of lines...
}

// Too small: Not worth making an entity
entity SingleResistor() {
    pin A: signal in;
    pin B: signal out;
    A -> Res(1kΩ).1 -> B;
}
```

### 5. Testing Entities
Create test boards for individual entities:

```bhdl
// test_filter_entity.bhdl
board TestFilter {
    power VCC = 3.3V @ 100mA;
    ground GND;
    
    // Signal generator connection
    test_input: Connector_SMA() {
        signal -> filter_in;
        shield -> GND;
    }
    
    // Module under test
    dut: Filter(fc=10kHz, type="lowpass") {
        IN <- filter_in;
        OUT -> filter_out;
        GND <- GND;
    }
    
    // Output measurement
    test_output: Connector_SMA() {
        signal <- filter_out;
        shield <- GND;
    }
}
```

## Troubleshooting

### Common Issues

1. **Circular Dependencies**
   - Problem: Entity A uses Entity B, which uses Entity A
   - Solution: Refactor to break the cycle, extract common functionality

2. **Missing Pins**
   - Problem: "Pin X not found on entity Y"
   - Solution: Ensure all pins are declared in the entity definition

3. **Parameter Type Mismatches**
   - Problem: "Cannot convert string to resistance"
   - Solution: Use correct units and types for parameters

4. **Hierarchical Name Conflicts**
   - Problem: Multiple instances creating same component names
   - Solution: Hierarchical naming handles this automatically

### Migration Checklist

- [ ] Identify repeated circuit patterns
- [ ] Create entity definitions with clear interfaces
- [ ] Replace repeated circuits with entity instances
- [ ] Test each entity in isolation
- [ ] Update documentation and schematics
- [ ] Review generated hierarchical component names
- [ ] Verify electrical connectivity is preserved

## Summary

Hierarchical design in BHDL provides powerful abstraction capabilities while maintaining the simplicity of the flow-based syntax. By following these migration guidelines, you can transform flat designs into well-organized, reusable, and maintainable hierarchical designs that scale with your project complexity.

Key benefits of migration:
- **Reduced code duplication**
- **Improved readability**
- **Better team collaboration**
- **Easier testing and validation**
- **Automatic component organization**
- **Clear design intent**