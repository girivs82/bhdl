# BHDL - Board Hardware Description Language

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)
[![CI](https://github.com/[USERNAME]/bhdl/workflows/CI/badge.svg)](https://github.com/[USERNAME]/bhdl/actions)
[![Documentation](https://img.shields.io/badge/docs-latest-brightgreen.svg)](docs/)

A domain-specific language for describing electronic circuit boards using a natural flow-based syntax.

## Overview

BHDL (Board Hardware Description Language) enables hardware designers to express circuit designs the way they think about them - as flows of power and signals between components.

## Key Features

- **Flow-based syntax**: Express circuits as natural flows (`VCC -> Res(1kΩ).1 -> LED(red).A`)
- **Automatic inference**: Component selection, level shifting, power sequencing handled automatically
- **Multi-level abstraction**: From high-level system architecture to detailed component connections
- **Team workflow**: Support for concurrent development by system architects, board designers, and layout engineers

## Advanced Analysis Capabilities

- **SPICE-based electrical validation**: Newton-Raphson DC analysis for accurate circuit simulation
- **Intelligent safety analysis**: Component derating, current limiting, and protection recommendations
- **Component role detection**: Topology-based identification without naming conventions
- **Stability analysis**: Loop stability, impedance measurement, and resonance detection
- **Unified data architecture**: Single netlist augmented with analysis results (no lossy conversions)

## Repository Structure

- **`/bhdl-*`**: Rust crates implementing the BHDL toolchain
  - `bhdl-parser` - Language parser and CST generation
  - `bhdl-ast` - Abstract syntax tree and semantic nodes
  - `bhdl-analyzer` - Multi-pass semantic analysis and type checking
  - `bhdl-synthesizer` - Circuit synthesis and netlist generation
  - `bhdl-visualizer` - Circuit layout and SVG visualization
  - `bhdl-spice` - SPICE-like electrical analysis
  - `bhdl-stdlib` - Standard component library
  - `bhdl-components` - Component database and KiCad integration
- **`/docs/spec`**: Language specification
  - `BHDL_Complete_Specification.md` - Authoritative v2.0 specification
- **`/docs/examples`**: Example circuits demonstrating v2.0 syntax
- **`/tests`**: Organized test infrastructure
  - See `tests/TESTING.md` for testing documentation

## Quick Start

```bash
# Build the project
cargo build

# Run tests
./tests/run_tests.sh all

# Parse an example circuit
cargo run -p bhdl-parser --bin test_v2_parser docs/examples/simple_led_circuit.bhdl

# See CLAUDE.md for detailed development instructions
```

## Language Example

```bhdl
board PowerSupply {
    // Power domains
    power VIN = 12V @ 1A;
    power VCC = 5V @ 1A;
    ground GND;
    
    // Circuit flow
    VIN -> fuse: Fuse(1A).1;
    fuse.2 -> protected_vin: TVSDiode(15V).1;
    protected_vin -> reg: LM7805().IN;
    reg.OUT -> VCC;
    reg.GND -> GND;
    
    // Decoupling
    VCC -> Cap(10µF).+ -> Cap(0.1µF).+ -> loads;
    
    // Status LED
    VCC -> Res(330Ω).1 -> LED(green).A;
    LED.K -> GND;
}
```

## Documentation

- **Development Guide**: See `CLAUDE.md` for architecture and development instructions
- **Language Specification**: See `docs/spec/BHDL_Complete_Specification.md`
- **Examples**: Browse `docs/examples/` for sample circuits
- **Testing**: See `tests/TESTING.md` for test organization

## Status

BHDL v2.0 is under active development with the following components functional:

- ✅ **Parser**: Full v2.0 flow-based syntax support
- ✅ **Analyzer**: 8-pass semantic analysis including electrical safety
- ✅ **Synthesizer**: Netlist generation with component database integration
- ✅ **SPICE Engine**: Nonlinear DC analysis with component models
- ✅ **Safety Analysis**: Data-driven validation with fix recommendations
- ✅ **Role Detection**: Pin metadata-based component classification
- ✅ **Stability Analysis**: AC-integrated power converter validation
- 🚧 **Visualizer**: Layout generation (component scaling improvements ongoing)
- 🚧 **CLI**: Basic commands (full integration in progress) 