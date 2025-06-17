# BHDL Examples

## Overview

This directory contains examples demonstrating BHDL v2.0 flow-based syntax. All examples are compatible with the current specification in `../spec/BHDL_Complete_Specification.md`.

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