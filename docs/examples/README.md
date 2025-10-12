# BHDL Examples

## Overview

This directory contains examples demonstrating BHDL v2.0 features:
- **Power Domain Example Gallery**: Progressive tutorial series teaching power domain concepts
- **General Examples**: Flow-based syntax and component usage
- **Component Libraries**: Standard component definitions

All examples are compatible with the current specification in `../spec/BHDL_Complete_Specification.md`.

---

## 🎓 Power Domain Example Gallery

Learn BHDL power domain features through a carefully designed progression from beginner to expert.

### Learning Path

```
01_simple_led_board.bhdl          ⭐ Beginner (10-15 min)
          ↓
02_multi_led_wildcards.bhdl       ⭐⭐ Intermediate (15-20 min)
          ↓
03_sensor_array.bhdl              ⭐⭐⭐ Advanced (20-30 min)
          ↓
04_fpga_dev_board.bhdl            ⭐⭐⭐⭐ Expert (30-45 min)
          ↓
comprehensive_power_demo.bhdl     ⭐⭐⭐⭐⭐ Master (45-60 min)
```

### Example 1: Simple LED Board (Beginner ⭐)
**Features**: Basic power domains, direct connections, decoupling capacitors
```bhdl
power_domain @VCC_5V = 5V @ 500mA {
    distribution {
        power_supply.VOUT;
        led_red.VCC;
    }
    decoupling {
        near power_supply: 10µF @ 1;
        distributed: 100nF @ 3;
    }
}
```
**Circuit**: 3 LEDs, 1 power domain, 4 connections

### Example 2: Multi-LED with Wildcards (Intermediate ⭐⭐)
**Features**: Wildcard pattern matching, array naming conventions
```bhdl
distribution {
    led[*].A;      // Expands to all 8 LEDs
    res[*].VCC;    // Expands to all 8 resistors
}
```
**Circuit**: 8 LEDs, 8 resistors, 17 connections

### Example 3: Sensor Array (Advanced ⭐⭐⭐)
**Features**: Range patterns, explicit lists, multiple voltage domains
```bhdl
sensor[0..7].VCC;         // Range: first 8 sensors
sensor[0,5,10,15].VREF;   // List: specific sensors
```
**Circuit**: 16 sensors, 3 power domains (digital/precision/monitoring), 24 connections

### Example 4: FPGA Development Board (Expert ⭐⭐⭐⭐)
**Features**: Multi-voltage domains, bank-based I/O organization, sophisticated decoupling
```bhdl
// Four different voltage levels for different FPGA subsystems
power_domain @VCCINT = 1.0V @ 30A {
    distribution { fpga.VCCINT[0..11]; }  // Core logic
}

power_domain @VCCAUX = 1.8V @ 3A {
    distribution { fpga.VCCAUX[0..3]; }   // PLLs and analog
}

power_domain @VCCO_0 = 2.5V @ 2A {
    distribution { fpga.VCCO[0..15]; }    // I/O banks 0-1 (DDR3)
}

power_domain @VCCO_1 = 3.3V @ 2A {
    distribution {
        fpga.VCCO[16..31];                // I/O banks 2-3
        led[*].A;                          // LEDs
    }
}
```
**Circuit**: FPGA with 5 voltage domains, 67 connections, 101 decoupling capacitors
**Design Pattern**: Multi-voltage domain architecture with bank-based I/O organization

### Example 5: Comprehensive Demo (Master ⭐⭐⭐⭐⭐)
**Features**: ALL power domain features including hierarchical wildcards, even/odd patterns, stepped ranges
```bhdl
sensor_board[*].sensor.VCC;  // Hierarchical
adc[even].AVCC;              // Even indices
mem[0..11:3].VCC;            // Stepped range: 0, 3, 6, 9
```
**Circuit**: 45 instances, 7 power domains, 59 connections, 90 decoupling capacitors
**Status**: ✅ All tests passing (59/59 connections)
**Documentation**: See `README_COMPREHENSIVE_POWER_DEMO.md` for complete details

### Quick Reference: Pattern Types

| Pattern | Syntax | Example | Expands To |
|---------|--------|---------|------------|
| Simple | `component.pin` | `mcu.VCC` | Single connection |
| Wildcard | `[*]` | `led[*].A` | All instances |
| Range | `[start..end]` | `sensor[0..7].VCC` | 0, 1, 2, ..., 7 |
| List | `[a,b,c]` | `sensor[0,5,10].VCC` | Only 0, 5, 10 |
| Even | `[even]` | `adc[even].AVCC` | 0, 2, 4, 6, ... |
| Odd | `[odd]` | `adc[odd].AVCC` | 1, 3, 5, 7, ... |
| Stepped | `[start..end:step]` | `mem[0..11:3].VCC` | 0, 3, 6, 9 |
| Hierarchical | `[*].component.pin` | `module[*].sensor.VCC` | Into sub-modules |
| Suffix | `*name.pin` | `array.*sensor.VCC` | Ends with "sensor" |

### Running Power Domain Examples

```bash
# Test individual examples
cargo run -p bhdl-analyzer --bin test_simple_led_board          # Example 1
cargo run -p bhdl-analyzer --bin test_multi_led_wildcards       # Example 2
cargo run -p bhdl-analyzer --bin test_sensor_array              # Example 3
cargo run -p bhdl-analyzer --bin test_fpga_dev_board            # Example 4

# Run comprehensive demo (all features)
cargo run -q -p bhdl-analyzer --bin test_comprehensive_power_demo
```

---

## Directory Structure

```
examples/
├── simple_led_circuit.bhdl     # Basic LED circuit example
├── linear_regulator.bhdl       # Power supply with flow syntax
├── libraries/                  # Component library modules
│   ├── passives.bhdl          # Resistors, capacitors, inductors
│   ├── power.bhdl             # Power components (regulators, etc.)
│   ├── ics.bhdl               # Integrated circuits
│   ├── protection.bhdl        # Protection components
│   ├── audio.bhdl             # Audio components
│   └── ...                    # Additional specialized libraries
├── modules/                    # Reusable circuit modules
│   └── audio_modules.bhdl     # Audio circuit patterns
└── old_syntax/                # Deprecated v1.0 examples (for reference)
```

## Key v2.0 Syntax Examples

### Basic Component Instantiation
```bhdl
// Direct flow connection
VCC -> Res(4.7kΩ).1 -> LED(red).A;
LED.K -> GND;

// Named handles for multiple references
USB_5V -> regulator: LinearReg(3.3V, 1A).IN;
regulator.OUT -> Cap(10µF).+ -> VOUT;
```

### Net Assignment with Implicit Handles
```bhdl
// Creates both a net and component handle
fuse.2 -> protected_vin: TVSDiode(15V).1;
protected_vin.2 -> GND;  // Use handle to reference other pins
```

### Flow Specifications
```bhdl
// Power flow using |> operator
power_flow: USB_5V |> protection |> regulation |> distribution;

// Signal processing flow
signal_flow: INPUT |> amplify(10x) |> filter(1kHz) |> OUTPUT;
```

### Generate Constructs
```bhdl
// Generate multiple connections
generate for i in 0..7 {
  GPIO[i] -> LED(colors[i]).A;
  LED.K -> GND;
}
```

## Component Libraries

The `libraries/` directory contains standard component definitions using v2.0 module syntax:

- **passives.bhdl**: Basic passive components with intelligent defaults
- **power.bhdl**: Linear and switching regulators, voltage references
- **ics.bhdl**: Common integrated circuits (op-amps, MCUs, etc.)
- **protection.bhdl**: ESD, overvoltage, and overcurrent protection

## Running Examples

These examples are designed to work with the BHDL toolchain:

```bash
# Parse and analyze an example
cargo run -p bhdl-analyzer -- docs/examples/simple_led_circuit.bhdl

# Generate netlist from example
cargo run -p bhdl-synthesizer -- docs/examples/linear_regulator.bhdl

# Visualize circuit (when fully implemented)
cargo run -p bhdl-visualizer -- docs/examples/simple_led_circuit.bhdl
```

## Notes

- All examples use BHDL v2.0 flow-based syntax
- Old v1.0 examples are preserved in `old_syntax/` for reference only
- See `../spec/BHDL_Complete_Specification.md` for complete language reference