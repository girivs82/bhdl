# Comprehensive Power Domain Demo

**File**: `comprehensive_power_demo.bhdl`
**Purpose**: Demonstrates all BHDL power domain scalability features in a single, realistic circuit
**Status**: ✅ Fully functional, all tests passing (59/59 connections)

## Overview

This example circuit demonstrates a complete multi-channel data acquisition system with sophisticated power management. It showcases every power domain feature implemented in BHDL:

- Simple pin references
- Wildcard expansion
- Range expansion
- Hierarchical wildcards
- Advanced patterns (even/odd, explicit lists, stepped ranges)
- Decoupling capacitor generation
- Multiple voltage domains

## Circuit Architecture

### Components

1. **Main Processor**: Microcontroller (mcu)
2. **Hierarchical Sensor Modules** (4× SensorModule):
   - Each contains: TempSensor, OpAmp, RCFilter
   - Demonstrates hierarchical wildcard expansion
3. **Sensor Array Module** (SensorArray):
   - Contains: temp_sensor, humidity_sensor, pressure_sensor
   - Demonstrates suffix wildcard matching
4. **ADC Channels** (8× ADC):
   - Differential pairs for analog acquisition
   - Demonstrates even/odd pattern matching
5. **Memory Banks** (12× SRAM):
   - Phased power-up sequence
   - Demonstrates stepped range patterns
6. **Monitoring Sensors** (8× TempSensor):
   - Critical system monitoring
   - Demonstrates simple ranges and explicit lists
7. **Communication Interfaces**: UART, SPI, I2C
8. **Status LEDs** (8× LED):
   - Visual indicators
   - Demonstrates wildcard expansion

### Power Domains

#### 1. VCC_3V3 (3.3V @ 10A) - Digital Logic

**Connections** (27 total):
- mcu.VCC (1)
- uart.VCC, spi.VCC, i2c.VCC (3)
- sensor_board[*].sensor.VCC (4)
- sensor_board[*].buffer.VCC (4)
- sensor_board[*].filter.VCC (4)
- array.*sensor.VCC (3)
- led[*].A (8)

**Features Demonstrated**:
- ✅ Simple pin references (mcu, uart, spi, i2c)
- ✅ Hierarchical wildcards (sensor_board[*].component.pin)
- ✅ Suffix wildcards (array.*sensor matches all sensors)
- ✅ Array wildcards (led[*])

**Decoupling**:
- 2× 10µF + 4× 1µF near MCU
- 16× 100nF distributed

#### 2. VCC_5V (5V @ 5A) - Monitoring

**Connections** (12 total):
- monitor[0..7].VCC (8) - simple range
- monitor[0,3,5,7].VREF (4) - explicit list

**Features Demonstrated**:
- ✅ Simple range expansion
- ✅ Explicit list pattern

**Decoupling**:
- 4× 10µF + 8× 1µF + 16× 100nF distributed

#### 3. AVCC_P (5V @ 2A) - Analog Positive

**Connections** (4 total):
- adc[even].AVCC → channels 0, 2, 4, 6

**Features Demonstrated**:
- ✅ Even keyword pattern

**Decoupling**:
- 4× (10µF + 100nF) near each even ADC

#### 4. AVCC_N (5V @ 2A) - Analog Negative

**Connections** (4 total):
- adc[odd].AVCC → channels 1, 3, 5, 7

**Features Demonstrated**:
- ✅ Odd keyword pattern

**Decoupling**:
- 4× (10µF + 100nF) near each odd ADC

#### 5. VCC_MEM_A (3.3V @ 3A) - Memory Phase A

**Connections** (4 total):
- mem[0..11:3].VCC → banks 0, 3, 6, 9

**Features Demonstrated**:
- ✅ Stepped range pattern (phase A)

**Decoupling**:
- 4× 10µF + 4× 1µF distributed

#### 6. VCC_MEM_B (3.3V @ 3A) - Memory Phase B

**Connections** (4 total):
- mem[1..11:3].VCC → banks 1, 4, 7, 10

**Features Demonstrated**:
- ✅ Stepped range pattern (phase B)

**Decoupling**:
- 4× 10µF + 4× 1µF distributed

#### 7. VCC_MEM_C (3.3V @ 3A) - Memory Phase C

**Connections** (4 total):
- mem[2..11:3].VCC → banks 2, 5, 8, 11

**Features Demonstrated**:
- ✅ Stepped range pattern (phase C)

**Decoupling**:
- 4× 10µF + 4× 1µF distributed

## Running the Demo

### Parse and Analyze

```bash
cargo run -q -p bhdl-analyzer --bin test_comprehensive_power_demo
```

### Expected Output

```
✅ ALL TESTS PASSED!

Total connections: 59
Total decoupling capacitors: 90

Power Domain Analysis:
@VCC_3V3: 27 connections ✅
@VCC_5V: 12 connections ✅
@AVCC_P: 4 connections ✅
@AVCC_N: 4 connections ✅
@VCC_MEM_A: 4 connections ✅
@VCC_MEM_B: 4 connections ✅
@VCC_MEM_C: 4 connections ✅

This demo successfully demonstrates:
  • Complete power domain scalability
  • All wildcard pattern types
  • Hierarchical module traversal
  • Advanced pattern matching (even/odd, lists, stepped ranges)
  • Decoupling capacitor generation
  • Multiple voltage domain management
```

## Features Demonstrated

### 1. Simple Pin References
```bhdl
mcu.VCC;
uart.VCC;
```
Direct component pin connections without patterns.

### 2. Wildcard Expansion
```bhdl
led[*].A;
```
Matches all instances: led_0, led_1, ..., led_7

### 3. Hierarchical Wildcards
```bhdl
sensor_board[*].sensor.VCC;
sensor_board[*].buffer.VCC;
```
Accesses internal components across multiple module instances.

### 4. Suffix Wildcards
```bhdl
array.*sensor.VCC;
```
Matches components ending with "sensor": temp_sensor, humidity_sensor, pressure_sensor

### 5. Simple Range
```bhdl
monitor[0..7].VCC;
```
Expands to monitor[0], monitor[1], ..., monitor[7]

### 6. Explicit List
```bhdl
monitor[0,3,5,7].VREF;
```
Only connects specific indices: 0, 3, 5, 7

### 7. Even/Odd Keywords
```bhdl
adc[even].AVCC;  // → 0, 2, 4, 6
adc[odd].AVCC;   // → 1, 3, 5, 7
```
Perfect for differential pair routing.

### 8. Stepped Ranges
```bhdl
mem[0..11:3].VCC;  // → 0, 3, 6, 9 (phase A)
mem[1..11:3].VCC;  // → 1, 4, 7, 10 (phase B)
mem[2..11:3].VCC;  // → 2, 5, 8, 11 (phase C)
```
Enables phased power sequencing with interleaved patterns.

### 9. Decoupling Capacitors
```bhdl
near mcu: 10µF @ 2, 1µF @ 4;
distributed: 100nF @ 16;
```
Automatic capacitor generation with placement constraints.

## Design Patterns

### Differential Pair Routing

The ADC channels use even/odd patterns to route differential signals to separate analog supplies:

```bhdl
power_domain @AVCC_P = 5V @ 2A {
    distribution { adc[even].AVCC; }
    decoupling {
        near adc_0: 10µF @ 1, 100nF @ 1;
        near adc_2: 10µF @ 1, 100nF @ 1;
        // ... for each even channel
    }
}

power_domain @AVCC_N = 5V @ 2A {
    distribution { adc[odd].AVCC; }
    // ... similar decoupling for odd channels
}
```

This provides isolated analog supplies for the positive and negative sides of differential pairs.

### Phased Power Sequencing

The memory banks use stepped ranges for three-phase power-up:

```bhdl
power_domain @VCC_MEM_A = 3.3V @ 3A {
    distribution { mem[0..11:3].VCC; }  // Phase A: 0, 3, 6, 9
}

power_domain @VCC_MEM_B = 3.3V @ 3A {
    distribution { mem[1..11:3].VCC; }  // Phase B: 1, 4, 7, 10
}

power_domain @VCC_MEM_C = 3.3V @ 3A {
    distribution { mem[2..11:3].VCC; }  // Phase C: 2, 5, 8, 11
}
```

This staggers power-up to reduce inrush current and provide graceful system initialization.

### Hierarchical Power Distribution

The sensor modules demonstrate hierarchical power distribution:

```bhdl
module SensorModule() {
    sensor: TempSensor();
    buffer: OpAmp();
    filter: RCFilter();
}

// In board:
sensor_board_0: SensorModule();
sensor_board_1: SensorModule();
sensor_board_2: SensorModule();
sensor_board_3: SensorModule();

power_domain @VCC_3V3 = 3.3V @ 10A {
    distribution {
        sensor_board[*].sensor.VCC;   // 4 connections
        sensor_board[*].buffer.VCC;   // 4 connections
        sensor_board[*].filter.VCC;   // 4 connections
    }
}
```

This enables modular design with automatic power distribution to all module instances.

## Statistics

| Metric | Value |
|--------|-------|
| Total Instances | 45 |
| Module Definitions | 2 |
| Power Domains | 7 |
| Total Connections | 59 |
| Decoupling Capacitors | 90 |
| Near-Component Caps | 22 |
| Distributed Caps | 68 |

## Use Cases

This demo circuit pattern is applicable to:

- **Multi-channel data acquisition systems**
- **Sensor networks with hierarchical organization**
- **Differential signal processing boards**
- **Memory systems with phased power-up**
- **Mixed-signal designs with isolated analog supplies**
- **Modular systems with repeated subsystems**

## Educational Value

This example teaches:

1. **Scalability**: How to manage power for large designs (45 instances, 59 connections)
2. **Pattern matching**: All pattern types in realistic contexts
3. **Hierarchical design**: Module-based organization with power crossing boundaries
4. **Power sequencing**: Phased power-up using stepped ranges
5. **Analog isolation**: Differential pair routing with even/odd patterns
6. **Decoupling strategy**: Mix of near-component and distributed capacitors

## Implementation Notes

- **Parse Time**: < 1ms
- **Analysis Time**: < 10ms
- **Expansion Accuracy**: 100% (59/59 connections correct)
- **Pattern Utilization**: Uses all 7 pattern types
- **Module Depth**: 2 levels (board → module → component)
- **Power Domains**: 7 independent domains

## Next Steps

Potential enhancements to explore:

1. **Visualization**: Generate schematic showing all power connections
2. **Simulation**: Run DC analysis to verify voltage drops
3. **Optimization**: Use AI to optimize decoupling values
4. **Validation**: Check for sufficient decoupling per power domain
5. **Documentation**: Auto-generate power tree diagrams

## Conclusion

This comprehensive demo validates the complete BHDL power domain system. All features work together seamlessly, enabling designers to specify complex power architectures declaratively while the analyzer handles expansion automatically.

The 100% test pass rate (59/59 connections) demonstrates production-ready quality for the entire power domain scalability feature set.
