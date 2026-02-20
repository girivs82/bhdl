# BHDL - Board Hardware Description Language

[![License](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE-APACHE)
[![Documentation](https://img.shields.io/badge/docs-latest-brightgreen.svg)](docs/)
[![Status](https://img.shields.io/badge/status-production--ready-success.svg)]()

**A modern hardware description language for electronic circuit design with natural flow-based syntax, intelligent analysis, and professional tooling.**

---

## 🎯 Overview

BHDL enables hardware designers to express circuit designs the way they think about them - as **flows of power and signals** between components. With automatic component inference, electrical validation, and comprehensive IDE integration, BHDL brings software-grade tooling to hardware design.

```bhdl
board PowerSupply {
    power VIN = 12V @ 2A;
    ground GND;

    // Flow-based syntax: intuitive and readable
    net regulated: VIN -> reg: LM7805().IN;
    reg.OUT -> @VCC;
    reg.GND -> @GND;

    // Design intent capture
    net filtered: @VCC -> cap: Cap(10µF).1 -> cap.2 -> @GND
        for noise_filtering(cutoff: 100Hz, attenuation: 40dB);
}
```

## ✨ Key Features

### 🌊 Natural Flow-Based Syntax
- **Express circuits as flows**: `VCC -> Res(1kΩ).1 -> LED(red).A -> GND`
- **Component instantiation in-line**: No separate declaration/instantiation
- **Multiple connection operators**: `->` (unidirectional), `<->` (bidirectional), `|>` (flow)
- **Net assignments**: `protected_vin: TVSDiode(15V).K` creates nets with implicit handles

### 🎨 Design Intent System
- **Capture design purpose**: 38 standard intent functions
- **Flow-based annotation**: `for current_limiting(max_current: 20mA)`
- **Automatic synthesis hints**: Tools understand design intent
- **Simulation mode determination**: PureDigital, DigitalWithTiming, MixedSignal, AnalogRequired
- **Validation rules**: Intent-specific checks and recommendations

### 🔬 Advanced Electrical Analysis
- **DC Operating Point**: Newton-Raphson nonlinear solver with component models
- **Safety Analysis**: Automatic derating, current limiting, protection recommendations
- **Component Role Detection**: Topology-based identification without naming conventions
- **Thermal Analysis**: Component temperature and hotspot detection
- **Power Domain Tracking**: Voltage propagation through complex hierarchies

### 📐 Power Domain Scalability
- **Wildcard expansion**: `sensor[*].VCC` → expands to all sensor instances
- **Range expansion**: `fpga.VCCO[0..7]` → 8 indexed pins
- **Automatic decoupling**: Capacitor generation with placement constraints
- **10-100x verbosity reduction** for large designs

### 💻 Professional Tooling

#### Command-Line Interface (CLI)
9 comprehensive commands for the complete workflow:
- `parse` - Syntax validation and AST inspection
- `analyze` - Full 11-pass semantic analysis
- `synthesize` - Netlist generation (JSON/SPICE formats)
- `visualize` - SVG schematic generation
- `spice` - Component role detection and analysis
- `pipeline` - End-to-end workflow automation
- `simulate` - Testbench-driven simulation
- `intents` - Flow tracking and intent analysis
- `doc` - Automatic documentation generation

```bash
# Complete workflow in one command
$ bhdl-cli circuit.bhdl pipeline -o ./build

# Analyze design intents
$ bhdl-cli circuit.bhdl intents

# Generate power domain docs
$ bhdl-cli circuit.bhdl doc -o power_analysis.md
```

#### Language Server Protocol (LSP)
22 major features for IDE integration:
- Real-time diagnostics (parse + semantic)
- Intelligent autocomplete (38 intent functions)
- Hover documentation
- Go to definition / Find references
- Symbol rename with conflict detection
- Document outline and workspace symbols
- Semantic syntax highlighting
- Code actions (quick fixes)
- Inlay hints (type/value display)
- Document formatting
- And 12 more features...

**Works with**: VSCode, Neovim, Emacs, Sublime Text, IntelliJ, and any LSP-compatible editor.

## 🚀 Quick Start

### Prerequisites
- Rust 1.70 or later
- 100MB disk space

### Installation

```bash
# Clone the repository
git clone https://github.com/[USERNAME]/bhdl.git
cd bhdl

# Build all components
cargo build --release

# Run tests
cargo test

# Build CLI
cargo build --release -p bhdl-cli

# Build LSP server
cargo build --release -p bhdl-lsp
```

### Your First Circuit

Create `led_blinker.bhdl`:

```bhdl
entity LED(color: string) {
    pin A: signal in;
    pin K: signal in;
}

entity Resistor(value: resistance) {
    pin 1: signal inout;
    pin 2: signal inout;
}

board LEDBlinker {
    power VCC = 5V @ 100mA;
    ground GND;

    // Current-limited LED with design intent
    net power_to_res: @VCC -> r1: Resistor(330).1;
    net res_to_led: r1.2 -> led1: LED("red").A
        for current_limiting(max_current: 20mA);
    net led_to_gnd: led1.K -> @GND;
}
```

Validate and analyze:

```bash
# Parse the circuit
$ bhdl-cli led_blinker.bhdl parse
✓ Parse successful

# Run analysis
$ bhdl-cli led_blinker.bhdl analyze
✓ Analysis successful - no issues found

# Generate visualization
$ bhdl-cli led_blinker.bhdl visualize -o led_blinker.svg
✓ Visualization generated
```

## 📚 Documentation

| Document | Description |
|----------|-------------|
| [Language Specification](docs/spec/BHDL_Complete_Specification.md) | Complete v2.0 language reference |
| [CLI Guide](docs/implementation/CLI_IMPLEMENTATION_GUIDE.md) | Command-line interface reference |
| [LSP Guide](docs/implementation/LSP_IMPLEMENTATION_SUMMARY.md) | IDE integration guide |
| [Developer Guide](CLAUDE.md) | Architecture and development instructions |
| [Examples](docs/examples/) | Sample circuits and tutorials |
| [Testing Guide](tests/TESTING.md) | Test organization and execution |

## 🏗️ Architecture

The BHDL toolchain follows a multi-stage pipeline:

```
┌──────────────┐    ┌─────────┐    ┌───────────┐    ┌──────────────┐    ┌─────────────┐
│ BHDL Source  │───▶│ Parser  │───▶│ Analyzer  │───▶│ Synthesizer  │───▶│ Visualizer  │
│  (.bhdl)     │    │ (CST)   │    │ (11-pass) │    │  (Netlist)   │    │    (SVG)    │
└──────────────┘    └─────────┘    └───────────┘    └──────────────┘    └─────────────┘
                                           │
                                           ▼
                                    ┌─────────────┐
                                    │ SPICE       │
                                    │ Analysis    │
                                    └─────────────┘
```

### Core Components

#### 1. **bhdl-parser** - Syntax Analysis
- Full v2.0 flow-based syntax support
- Rowan-based incremental parsing
- Comprehensive error recovery
- Electrical unit handling (kΩ, μF, MHz, etc.)

#### 2. **bhdl-analyzer** - Semantic Analysis
11-pass analysis pipeline:
1. **Scope Building**: Symbol tables and definition scopes
2. **Component Registry**: Early instance registration for scalability
3. **Power Domain Expansion**: Wildcards, ranges, and decoupling generation
4. **Reference Resolution**: Type checking and symbol binding
5. **Constant Evaluation**: Compile-time computation
6. **Bounds Checking**: Range validation
7. **Power Analysis**: Voltage propagation and domain tracking
8. **Component Inference**: Automatic component selection
9. **SPICE Synthesis**: Circuit model generation
10. **Attribute Analysis**: Custom attribute handling
11. **Flow Tracking**: Intent resolution and simulation mode determination

#### 3. **bhdl-synthesizer** - Netlist Generation
- Structural netlist output
- Component database integration
- Reference designator auto-generation (R1, D1, C1, etc.)
- Multiple output formats (JSON, SPICE)

#### 4. **bhdl-visualizer** - Schematic Generation
- Semantic layout algorithms
- Force-directed and analytical placement
- Intelligent routing with pathfinding
- Component symbol libraries
- SVG output with embedded metadata

#### 5. **bhdl-spice** - Electrical Analysis
- Newton-Raphson DC solver
- Nonlinear component models
- LED forward voltage modeling
- Diode Shockley equations
- Safety analysis with derating factors

#### 6. **bhdl-stdlib** - Component Library
- Standard component definitions
- Manufacturer datasheet parameters
- Intent function implementations (38 standard intents)
- Behavioral models

#### 7. **bhdl-testbench** - Simulation
- Testbench compiler
- Waveform generation (VCD, CSV, JSON)
- Assertion checking
- Measurement extraction

#### 8. **bhdl-cli** - Command-Line Interface
Production-ready CLI with 9 commands for complete workflow automation.

#### 9. **bhdl-lsp** - Language Server
Production-ready LSP server with 22 features for professional IDE integration.

## 🎓 Language Features

### Flow-Based Connections
```bhdl
// Direct flow syntax
VCC -> Res(1kΩ).1 -> LED(red).A -> GND

// Named nets
net signal_line: sensor.OUT -> filter.IN;

// Bidirectional
data_bus: controller <-> memory;

// With component instantiation
VCC -> r1: Resistor(330).1 -> r1.2 -> led: LED("red").A;
```

### Design Intents
```bhdl
// Capture design purpose
net debounced: button -> rc_filter
    for debounce(time: 50ms);

// Protection intent
net protected: input -> tvs: TVSDiode(6V).K
    for input_protection(max_voltage: 6V);

// Measurement intent
net monitored: @VCC -> sense_resistor -> load
    for current_sensing(max_current: 1A, accuracy: 1%);
```

### Power Domain Scalability
```bhdl
board SensorArray {
    power VCC = 3.3V @ 500mA;

    // Wildcard expansion - connects ALL sensor instances
    power_domain @VCC = 3.3V @ 500mA {
        distribution {
            sensor[*].VCC;  // Expands to sensor_0.VCC, sensor_1.VCC, ...
        }

        decoupling {
            // One capacitor near each sensor
            near sensor[*]: 100nF @ 1;
        }
    }

    // Range expansion - 8 indexed pins
    power_domain @VCCO = 2.5V @ 1A {
        distribution {
            fpga.VCCO[0..7];  // Expands to VCCO0..VCCO7
        }
    }
}
```

### Entity Definitions
```bhdl
entity VoltageRegulator(
    input_voltage: voltage,
    output_voltage: voltage,
    max_current: current
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground;
    pin EN: signal in when has_enable_pin;

    // Behavioral specification (optional)
    behavior {
        VOUT = output_voltage;
        efficiency = 0.85;
    }
}
```

### Conditional Logic
```bhdl
// Ternary operator
net regulated: vin -> (use_ldo ? ldo.IN : switcher.IN);

// Conditional pins
pin EN: signal in when has_enable_control;

// Conditional connections
if debug_mode {
    test_points: signals[*] -> TestPoint();
}
```

## 🔧 Development

### Building from Source

```bash
# Debug build (faster compilation)
cargo build

# Release build (optimized)
cargo build --release

# Build specific component
cargo build -p bhdl-analyzer
```

### Running Tests

```bash
# All tests
cargo test

# Specific crate
cargo test -p bhdl-analyzer

# With output
cargo test -- --nocapture

# Single test
cargo test test_name
```

### Code Quality

```bash
# Linting
cargo clippy

# Formatting
cargo fmt

# Check without building
cargo check
```

## 📊 Project Status

| Component | Status | Features | Tests | LOC |
|-----------|--------|----------|-------|-----|
| **Parser** | ✅ Production | Full v2.0 syntax | 150+ | 8,500 |
| **Analyzer** | ✅ Production | 11-pass analysis | 200+ | 15,000 |
| **Synthesizer** | ✅ Production | Multi-format output | 80+ | 5,000 |
| **Visualizer** | ✅ Production | Multiple layouts | 50+ | 6,000 |
| **SPICE** | ✅ Production | DC + safety | 100+ | 8,000 |
| **Stdlib** | ✅ Production | 38 intents | N/A | 3,000 |
| **Testbench** | ✅ Production | Full simulation | 40+ | 4,000 |
| **CLI** | ✅ Production | 9 commands | Validated | 1,050 |
| **LSP** | ✅ Production | 22 features | 92 | 6,117 |
| **Total** | - | - | **700+** | **56,667** |

### Recent Milestones (October 2025)

- ✅ **CLI Implementation**: Complete command-line interface with 9 commands
- ✅ **LSP Server**: Full IDE integration with 22 LSP features
- ✅ **Intent System**: Flow tracking and 38 standard intent functions
- ✅ **Power Domain Expansion**: Wildcards and ranges for scalability
- ✅ **Documentation Generation**: Automatic power domain docs
- ✅ **Unified Simulation**: DC + safety + thermal analysis

### Current Focus

- 🎯 Visualization refinements (symbol scaling, routing optimization)
- 🎯 Additional manufacturer datasheet integration
- 🎯 Community building and example collection
- 🎯 Performance optimization for large designs

## 🤝 Contributing

We welcome contributions! Areas where we'd love help:

- **Component Library**: Add more standard components and models
- **Examples**: Create example circuits and tutorials
- **Documentation**: Improve guides and references
- **Testing**: Add test cases and edge case coverage
- **IDE Extensions**: VSCode, Neovim, Emacs extensions
- **Visualization**: Improve layout algorithms and symbol libraries

See `CONTRIBUTING.md` (TODO) for guidelines.

## 📖 Learning Resources

### Examples

- [Simple LED Circuit](docs/examples/01_simple_led_board.bhdl) - Beginner introduction
- [Multi-LED with Wildcards](docs/examples/02_multi_led_wildcards.bhdl) - Scalability features
- [Sensor Array](docs/examples/03_sensor_array.bhdl) - Ranges and patterns
- [FPGA Dev Board](docs/examples/04_fpga_dev_board.bhdl) - Complex multi-domain design
- [Buck Converter](docs/examples/buck_converter_with_metadata.bhdl) - Advanced features
- [Power Domain Demo](docs/examples/comprehensive_power_demo.bhdl) - All power features

### Tutorials

- [Getting Started](docs/tutorials/getting_started.md) - TODO
- [Design Intents](docs/tutorials/design_intents.md) - TODO
- [Power Domains](docs/tutorials/power_domains.md) - TODO
- [Simulation](docs/tutorials/simulation.md) - TODO

## 🔗 Related Projects

- **KiCad**: PCB design suite (component import)
- **SPICE**: Circuit simulation (netlist export)
- **Verilator**: HDL simulator (digital co-simulation, planned)
- **ngspice**: Open-source SPICE simulator

## 📄 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🙏 Acknowledgments

BHDL builds on excellent open-source projects:

- **Rowan**: Rust implementation of Red-Green trees for incremental parsing
- **Tower-LSP**: Language Server Protocol framework for Rust
- **Clap**: Command-line argument parsing
- **Serde**: Serialization framework

## 📧 Contact

- **Issues**: [GitHub Issues](https://github.com/[USERNAME]/bhdl/issues)
- **Discussions**: [GitHub Discussions](https://github.com/[USERNAME]/bhdl/discussions)
- **Documentation**: [docs/](docs/)

---

**BHDL** - Hardware description language for the modern age.

*Express circuits naturally. Validate electrically. Design confidently.*
