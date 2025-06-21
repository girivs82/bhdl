# BHDL: Board Hardware Description Language
## Complete Specification v2.0

### Table of Contents
1. [Introduction](#1-introduction)
2. [Design Philosophy](#2-design-philosophy)
3. [Core Language Constructs](#3-core-language-constructs)
4. [Type System](#4-type-system)
5. [Component System](#5-component-system)
   - [Component Handles and Net Naming](#55-component-handles-and-net-naming)
   - [Dual-Role Component Syntax](#56-dual-role-component-syntax)
6. [Interface System](#6-interface-system)
7. [Power Management](#7-power-management)
8. [Level Shifting](#8-level-shifting)
9. [Physical Constraints](#9-physical-constraints)
10. [Multi-File Team Workflow](#10-multi-file-team-workflow)
11. [Standard Library](#11-standard-library)
12. [Complete Working Example](#12-complete-working-example)
13. [Advanced Power Sequencing](#13-advanced-power-sequencing)
14. [Advanced Level Shifting](#14-advanced-level-shifting)
15. [Team Workflow and Multi-File Support](#15-team-workflow-and-multi-file-support)
16. [Language Reference](#16-language-reference)
17. [Design Benefits and Advantages](#17-design-benefits-and-advantages)
18. [Electrical Safety Analysis](#18-electrical-safety-analysis)

---

## 1. Introduction

### 1.1 Purpose and Scope

BHDL (Board Hardware Description Language) is a domain-specific language for describing electronic circuit boards using a **circuit flow paradigm**. Unlike traditional HDLs designed for digital logic, BHDL captures the natural way board designers think about power distribution, signal flow, and component interconnection.

### 1.2 Key Innovations

- **Circuit Flow Paradigm**: Express designs as power/signal flows rather than structural hierarchies
- **Clear Net Disambiguation**: @ prefix clearly distinguishes nets from component handles
- **Component Inference**: Automatic component instantiation from connection patterns
- **Domain-Aware Signals**: Automatic level shifting and power domain management
- **Unified Syntax**: Seven core constructs handle everything from simple LEDs to complex SoCs
- **Team Workflow**: Multi-file architecture supporting concurrent design by different specialists

### 1.3 Target Users

- **System Architects**: High-level functional specification and requirements
- **Board Designers**: Circuit implementation and component selection
- **Layout Engineers**: Physical constraints and manufacturing requirements

---

## 2. Design Philosophy

### 2.1 Core Principles

1. **Natural Thinking**: Syntax matches how designers think about circuits
2. **Minimal Cognitive Load**: Seven core constructs cover entire language
3. **Connection-First Workflow**: Sketch connections, refine components later
4. **Automatic Intelligence**: Tool handles routine tasks (level shifting, power sequencing, constraints)
5. **Progressive Refinement**: Start simple, add detail where needed
6. **Team Collaboration**: Clear separation of concerns between team members

### 2.2 Paradigm Shift: Flow vs Structure

**Traditional HDL Approach (Structural)**:
```vhdl
-- Define modules, then connect
entity regulator is
  port (VIN: in std_logic; VOUT: out std_logic);
end entity;

-- Complex instantiation and wiring
```

**BHDL Approach (Flow-Based)**:
```bhdl
// Express intent directly
power_flow: USB_5V |> regulation(3.3V) |> distribution |> loads;

// Tool handles implementation details
```

---

## 3. Core Language Constructs

BHDL has exactly **7 core constructs** that compose to handle any complexity:

### 3.1 Component Instantiation and Net Naming

#### Anonymous Nets
```bhdl
// Universal pattern: source -> component(parameters) -> destination
VCC -> Res(4.7kΩ).1 -> LED(red).A;  // Creates anonymous nets
USB_5V -> regulator: LinearReg(3.3V, 1A).IN;
```

#### Named Nets with @ Syntax
```bhdl
// Named nets use @ prefix for creation and reference
VCC @FILTERED-> r1: Res(4.7kΩ).1;      // Creates net @FILTERED
@FILTERED -> c1: Cap(100nF).1;          // References net @FILTERED
@FILTERED -> c2: Cap(10µF).1;           // Multiple connections to same net

// Component handles (r1, c1, c2) are separate from net names
r1.2 -> LED(red).A;  // r1 is component handle, not a net
```

### 3.2 Net Naming and References

#### The @ Syntax for Nets

BHDL v2.0 uses the `@` prefix to clearly distinguish nets from component handles:

```bhdl
// Anonymous nets (no explicit name)
VCC -> r1: Res(10k).1;        // Creates anonymous net
r1.2 -> led: LED(red).A;      // Another anonymous net

// Named nets with @ syntax
VCC @FILTERED-> r1: Res(10k).1;    // Creates net @FILTERED
@FILTERED -> c1: Cap(100n).1;      // References net @FILTERED
@FILTERED -> c2: Cap(10µ).1;       // Multiple connections to same net

// Clear distinction
r1.2 -> led.A;          // r1, led are component handles
@FILTERED -> r1.1;      // FILTERED is a net
fuse.2 -> @PROTECTED;   // fuse is component, PROTECTED is net
```

#### Key Rules:
1. **Net Creation**: Use `@NAME->` to create a named net
2. **Net Reference**: Always use `@NAME` when referencing a net
3. **Component Handles**: Use `:` to create, no prefix to reference
4. **Anonymous Nets**: Use `->` without `@NAME`
5. **Disambiguation**: `@` always indicates a net, never a component

### 3.3 Flow Specification
```bhdl
// Universal flow operator |> for any domain
power_flow: USB_5V |> protection |> regulation |> distribution;
signal_flow: INPUT |> amplify(10x) |> filter(1kHz) |> OUTPUT;
data_flow: sensors |> i2c_bus |> mcu |> processing;
```

### 3.4 Interface Declaration
```bhdl
// Bus interfaces as first-class objects
main_i2c: I2C(voltage=3.3V, frequency=400kHz);
ddr_bus: DDR3(width=16bit, speed=800MHz);
expansion: GPIO_Header(pins=40, pitch=2.54mm);
```

### 3.5 Generate Constructs
```bhdl
// Universal repetition pattern
generate for i in 0..7 {
  GPIO[i] -> LED(colors[i]).A;
  LED.K -> GND;
}

generate for rail in [VCC_3V3, VCC_1V8] {
  rail -> Cap(10µF).+ -> Cap(0.1µF).+ -> load;
}
```

### 3.6 Conditional Logic
```bhdl
// Universal conditional construct
if (condition) { actions } else { alternatives }

// Power sequencing
if (VCC_3V3.stable) { VCC_1V8.enable(); }

// Component selection
if (high_speed) { 
  level_shift using TXS0108E; 
} else { 
  level_shift using 74LVC1T45; 
}
```

### 3.7 Module Definition

Modules enable hierarchical design and code reuse by encapsulating functionality into reusable components with well-defined interfaces.

#### Basic Module Syntax
```bhdl
// Module with parameters and pins
module RC_Filter(R_value: resistance = 1kΩ, C_value: capacitance = 100nF) {
    // Pin declarations with types and directions
    pin IN: signal in;      // Input signal pin
    pin OUT: signal out;    // Output signal pin  
    pin GND: ground in;     // Ground reference
    
    // Internal connections
    IN -> Res(R_value).1;
    Res(R_value).2 -> OUT;
    OUT -> Cap(C_value).1;
    Cap(C_value).2 -> GND;
}

// Simple module without parameters
module PowerIndicator() {
    pin VCC: power in;
    pin GND: ground in;
    
    // Status LED with current limiting
    VCC -> Res(1kΩ).1 -> LED(green).A;
    LED(green).K -> GND;
}
```

#### Module Instantiation
```bhdl
board AudioAmplifier {
    power VCC_12V = 12V @ 2A;
    ground GND;
    
    // Instance with custom parameters
    input_filter: RC_Filter(R_value=10kΩ, C_value=47nF) {
        IN <- audio_input;
        OUT -> amplifier_input;
        GND <- GND;
    }
    
    // Instance with default parameters
    output_filter: RC_Filter() {
        IN <- amplifier_output;
        OUT -> speaker_output;
        GND <- GND;
    }
    
    // Multiple instances create unique components
    power_indicator: PowerIndicator() {
        VCC <- VCC_12V;
        GND <- GND;
    }
}
```

#### Hierarchical Reference Designators
Components within module instances receive hierarchical names:
```bhdl
// In the above example, components are named:
// - input_filter.R1 (10kΩ resistor)
// - input_filter.C1 (47nF capacitor)
// - output_filter.R1 (1kΩ resistor - default)
// - output_filter.C1 (100nF capacitor - default)
// - power_indicator.R1 (1kΩ resistor)
// - power_indicator.D1 (green LED)
```

#### Module Pin Types
```bhdl
module ComplexInterface() {
    // Power pins
    pin VCC: power in;          // Power input
    pin VOUT: power out;        // Power output
    pin GND: ground in;         // Ground reference
    
    // Signal pins  
    pin CLK: signal in;         // Input signal
    pin DATA: signal inout;     // Bidirectional signal
    pin STATUS: signal out;     // Output signal
    
    // Protocol-specific pins
    pin SDA: signal(i2c) inout; // I2C data
    pin SCL: signal(i2c) in;    // I2C clock
}
```

#### Parameterized Modules
```bhdl
// Parameters with types and constraints
module VoltageRegulator(
    Vin: voltage,                    // Required parameter
    Vout: voltage,                   // Required parameter
    Imax: current = 1A,              // Optional with default
    topology: string = "linear"      // String parameter
) {
    pin IN: power in;
    pin OUT: power out;
    pin GND: ground in;
    pin EN: signal in when topology == "switching";  // Conditional pin
    
    // Implementation based on parameters
    generate if (topology == "linear") {
        IN -> LinearReg(Vout, Imax).IN;
        LinearReg(Vout, Imax).OUT -> OUT;
        LinearReg(Vout, Imax).GND -> GND;
    } else if (topology == "switching") {
        IN -> BuckConverter(Vin, Vout, Imax).VIN;
        BuckConverter(Vin, Vout, Imax).VOUT -> OUT;
        BuckConverter(Vin, Vout, Imax).GND -> GND;
        EN -> BuckConverter(Vin, Vout, Imax).EN;
    }
}
```

#### Module Arrays and Generate
```bhdl
module LEDArray(count: int = 8) {
    pin VCC: power in;
    pin GND: ground in;
    pin[count] CTRL: signal in;  // Pin array
    
    // Generate multiple components
    generate for i in 0..count {
        CTRL[i] -> Res(330Ω).1 -> LED(red).A;
        LED(red).K -> GND;
    }
}

// Usage
board LEDPanel {
    power VCC_5V = 5V @ 500mA;
    ground GND;
    
    // Creates 16 LEDs with individual control
    display: LEDArray(count=16) {
        VCC <- VCC_5V;
        GND <- GND;
        CTRL <- gpio_bus[0..15];
    }
}
```

#### Module Composition
```bhdl
// Modules can instantiate other modules
module PowerManagement() {
    pin VIN: power in;
    pin VOUT_3V3: power out;
    pin VOUT_1V8: power out;
    pin GND: ground in;
    
    // First stage: 12V to 5V
    stage1: VoltageRegulator(Vin=12V, Vout=5V, Imax=2A) {
        IN <- VIN;
        OUT -> intermediate_5V;
        GND <- GND;
    }
    
    // Second stage: 5V to 3.3V
    stage2: VoltageRegulator(Vin=5V, Vout=3.3V) {
        IN <- intermediate_5V;
        OUT -> VOUT_3V3;
        GND <- GND;
    }
    
    // Third stage: 5V to 1.8V
    stage3: VoltageRegulator(Vin=5V, Vout=1.8V) {
        IN <- intermediate_5V;
        OUT -> VOUT_1V8;
        GND <- GND;
    }
}
```

#### Module Variants and Deduplication
The BHDL toolchain automatically deduplicates module instances with identical parameters:
```bhdl
// These create a single module definition
filter1: RC_Filter(1kΩ, 100nF) { ... }
filter2: RC_Filter(1kΩ, 100nF) { ... }  // Reuses same module

// This creates a new variant
filter3: RC_Filter(10kΩ, 10nF) { ... }  // New module variant
```

#### Module Imports and Multi-File Support
```bhdl
// Import all public modules from a file
import "common/filters.bhdl";
import "power/regulators.bhdl";

// Import specific modules (destructuring)
import { RC_Filter, LC_Filter } from "common/filters.bhdl";
import { LinearReg, BuckConverter } from "power/regulators.bhdl";

// Relative imports
import "../shared/connectors.bhdl";
import "./local_modules.bhdl";
```

#### Module Aliases
```bhdl
// Create shorter names for frequently used modules
alias LDO = LinearDropoutRegulator;
alias Buck = BuckConverter;
alias TVS = TransientVoltageSuppressor;

// Usage
reg1: LDO(3.3V) { ... }      // Same as LinearDropoutRegulator
conv1: Buck(12V, 5V) { ... } // Same as BuckConverter
```

#### Best Practices
1. **Clear Interfaces**: Define all pins with explicit types and directions
2. **Meaningful Parameters**: Use typed parameters with sensible defaults
3. **Hierarchical Organization**: Build complex systems from simple modules
4. **Consistent Naming**: Use descriptive names for modules and instances
5. **Documentation**: Add comments explaining module purpose and usage
6. **File Organization**: Group related modules in separate files
7. **Namespace Management**: Use clear import paths to avoid conflicts

### 3.8 Constraint Declaration
```bhdl
// Physical and electrical constraints
constrain component_placement {
  place crystal near mcu within 5mm;
  place power_components at edge(left);
}

constrain routing {
  route ddr_signals { length_match=±0.1mm, impedance=50Ω };
  route power_rails { min_width=0.5mm, thermal_vias=true };
}
```

---

## 4. Type System

### 4.1 Electrical Types

```bhdl
// Base electrical types
signal_types {
  signal: electrical_signal {
    voltage_min: voltage;
    voltage_max: voltage;
    current_max: current;
    domain: power_domain;
  };
  
  power: power_rail {
    voltage: voltage;
    current_max: current;
    ripple_max: voltage;
    domain: power_domain;
  };
  
  ground: reference_potential {
    domain: power_domain;
  };
}
```

### 4.2 Domain-Qualified Signals

```bhdl
// Signals are automatically qualified by their power domain
mcu_gpio: signal(domain=VCC_3V3, levels=[0V, 3.3V]);
sensor_int: signal(domain=VCC_1V8, levels=[0V, 1.8V]);

// Cross-domain connections automatically insert level shifters
mcu_gpio -> sensor_int;  // Auto-inserts 3.3V-to-1.8V level shifter
```

### 4.3 Protocol-Specific Types

```bhdl
// Specialized signal types for common protocols
i2c_signal: signal {
  drive_type = open_drain;
  pullup_required = true;
  max_frequency = 400kHz;
};

spi_signal: signal {
  drive_type = cmos;
  max_frequency = 50MHz;
};

ddr_signal: signal {
  impedance = 50Ω ± 10%;
  slew_rate = controlled;
  termination = required;
};
```

### 4.4 Units and Physical Values

```bhdl
// Comprehensive unit system with automatic conversion
electrical_units {
  // Voltage
  3.3Vdc, 230Vac, 120Vrms
  
  // Current  
  100mA, 2A, 50µA
  
  // Resistance
  4.7kΩ, 1MΩ, 0.1Ω
  
  // Capacitance
  10µF, 100nF, 1pF
  
  // Frequency
  16MHz, 400kHz, 50Hz
  
  // Time
  10ns, 1µs, 100ms
  
  // Temperature
  85°C, -40°C
  
  // Percentages
  5%, 85%
}
```

---

## 5. Component System

### 5.1 Component Inference Pattern

```bhdl
// Natural component instantiation - no pre-declaration needed
VCC -> Res(4.7kΩ).1 -> LED(red, 20mA).A;
LED.K -> GND;

// Tool automatically creates:
// R1: Resistor(value=4.7kΩ, tolerance=5%, power=0.25W)
// LED1: LED(color=red, current=20mA, package=auto_select)
```

### 5.2 Component Refinement

```bhdl
// Override auto-inferred properties when needed
components {
  R1: Resistor(4.7kΩ, tolerance=1%, power=0.5W, package="0603");
  LED1: LED(red, current=20mA, package="0805", luminosity=high);
}
```

### 5.3 Standard Component Patterns

```bhdl
// Common component shortcuts
VCC -> pullup_bank(4.7kΩ) -> [SCL, SDA];  // I2C pullups
VCC -> decoupling(10µF + 0.1µF) -> mcu.VDD;  // Power decoupling
INPUT -> lowpass_filter(1kHz) -> OUTPUT;  // RC filter
```

### 5.4 Component Handles

```bhdl
// Explicit handles for multiple references
VCC -> current_sense: Res(0.1Ω).1 -> VOUT;
current_sense.2 -> current_monitor.INPUT;
current_sense.voltage_drop -> power_calculation;
```

### 5.5 Component Handles and Net Naming

#### Component Handles
```bhdl
// Component handle syntax: name: Component(...).pin
VCC -> r1: Res(10kΩ).1;  // Creates component with handle "r1"
r1.2 -> led: LED(red).A;  // Reference component pins via handle
led.K -> GND;

// Handles are ONLY component references, not net names
```

#### Named Nets with @ Syntax
```bhdl
// Create and reference named nets with @ prefix
VIN @RAW-> fuse: Fuse(1A).1;
fuse.2 @PROTECTED-> tvs: TVSDiode(15V).1;
tvs.2 -> GND;

// Reference named nets - ALWAYS with @
@PROTECTED -> bulk_cap: ElectrolyticCap(100µF, 25V).+;
@PROTECTED -> ceramic_cap: Cap(0.1µF).1;
bulk_cap.- -> GND;
ceramic_cap.2 -> GND;
```

#### Key Points:
- Component handles use `:` syntax and create ONLY component references
- Named nets use `@` prefix for both creation (`@NAME->`) and reference (`@NAME`)
- Anonymous nets are created by `->` without `@NAME`
- Handles and nets are in separate namespaces - no ambiguity
- Reference designators (R1, D1, C1) are auto-generated by the toolchain

#### Examples:
```bhdl
// Power supply with clear net/component distinction
VIN @RAW-> fuse: Fuse(1A).1;           // @RAW is net, fuse is component
fuse.2 @FUSED-> tvs: TVSDiode(15V).1;  // @FUSED is net, tvs is component
@FUSED -> reg: LM7805.IN;              // Reference @FUSED net
reg.OUT @5V-> c_out: Cap(100µF).+;    // Create @5V net
reg.GND -> GND;
c_out.- -> GND;

// Multiple connections to named net
@5V -> r1: Res(330Ω).1;    // Power indicator
r1.2 -> led: LED(green).A;
led.K -> GND;
@5V -> conn: Header_1x3.1;  // Power output
GND -> conn.2;
@5V -> conn.3;              // Second power pin
```

### 5.6 Dual-Role Component Syntax

BHDL supports a revolutionary dual-role syntax where component parameters can serve as both **values** and **constraints**, with the toolchain using electrical simulation to determine appropriate values when constraints are specified.

#### Value Specification (Traditional)
```bhdl
// Direct value specification - traditional approach
VCC -> Res(4.7kΩ).1 -> LED(red).A;     // Explicit 4.7kΩ value
VCC -> Res(330Ω).1 -> LED(green).A;    // Explicit 330Ω value
```

#### Constraint Specification (Revolutionary)
```bhdl
// Constraint-based specification - BHDL innovation
VCC -> Res(?, current=20mA).1 -> LED(red).A;    // Infer resistance for 20mA
VCC -> Res(?, power=0.5W).1 -> load;            // Infer value within power rating
VCC -> Cap(?, ripple<50mV).1 -> load;           // Infer capacitance for ripple spec
```

#### Mixed Specification
```bhdl
// Combine explicit values with constraints
VCC -> Res(10kΩ, power=0.5W).1 -> load;         // 10kΩ with power validation
VCC -> Cap(100µF, voltage=25V).+ -> load;       // 100µF rated for 25V
```

#### How It Works

1. **Value Mode**: When numeric values are provided, they're used directly
2. **Constraint Mode**: When `?` is used, SPICE simulation determines the value
3. **Validation Mode**: When both are provided, SPICE validates the choice
4. **Safety Analysis**: All modes undergo electrical safety verification

#### Examples

```bhdl
// LED current limiting - let SPICE calculate resistance
power VCC = 5V;
VCC -> r1: Res(?, current=20mA).1 -> led: LED(red, Vf=2.0V).A;
led.K -> GND;
// SPICE calculates: R = (5V - 2.0V) / 20mA = 150Ω

// Power dissipation constraint
high_current_path -> Res(?, power=2W, tolerance=5%).1 -> load;
// SPICE selects appropriate value within 2W rating

// Filtering capacitor selection
noisy_rail -> Cap(?, ripple<100mV, esr<0.1Ω).+ -> clean_rail;
// SPICE determines capacitance for ripple requirement

// Voltage divider with ratio constraint
VIN -> R1: Res(?, ratio=2:1).1 -> @VOUT -> R2: Res(?).1 -> GND;
// SPICE calculates R1 and R2 to achieve 2:1 ratio
```

#### Advanced Constraint Types

```bhdl
// Thermal constraints
power_path -> Res(?, temperature_rise<10°C).1 -> load;

// Frequency response constraints
signal -> Cap(?, cutoff_freq=1kHz).1 -> filtered_signal;

// Impedance matching
source -> Res(?, impedance_match=50Ω).1 -> transmission_line;

// Current sharing in parallel paths
VCC -> [
  Res(?, current_share=50%).1,
  Res(?, current_share=50%).1
] -> load;
```

#### Benefits

1. **Design Intent Capture**: Express what you want, not just what you calculated
2. **Automatic Optimization**: SPICE finds optimal component values
3. **Safety Verification**: All solutions verified for electrical safety
4. **Design Space Exploration**: Quickly evaluate different constraints
5. **Documentation**: Constraints document design requirements inline

This dual-role syntax represents a paradigm shift in hardware description, moving from prescriptive component selection to constraint-based design with automatic optimization and verification.

---

## 6. Interface System

### 6.1 Interface Definition

Interfaces define standardized communication protocols with signals, requirements, and perspectives.

```bhdl
// Basic interface definition
interface I2C {
    signal SDA: inout;  // Bidirectional data
    signal SCL: out;    // Clock (master drives)
}

// Parameterized interface with defaults
interface SPI(width: int = 8, frequency: frequency = 1MHz) {
    signal MOSI: out;
    signal MISO: in;
    signal SCK: out;
    signal CS: out optional;  // Optional chip select
}

// Interface with requirements
interface USB2 {
    signal DP: inout;
    signal DM: inout;
    signal VBUS: power in;
    signal GND: ground;
    
    // Electrical requirements
    require pullup(DP, 1.5kΩ);
    require termination(DP, 27Ω);
    require termination(DM, 27Ω);
}
```

### 6.2 Interface Perspectives

Perspectives define different signal directions for master/slave or DTE/DCE modes.

```bhdl
interface UART(baudrate: int = 9600) {
    signal TX: out;  // Default: DTE perspective
    signal RX: in;
    signal RTS: out optional;
    signal CTS: in optional;
    
    // DCE perspective (modem side)
    perspective dce {
        signal TX: in;   // Swapped for DCE
        signal RX: out;
        signal RTS: in optional;
        signal CTS: out optional;
    }
}

interface SPI(width: int = 8) {
    signal MOSI: out;  // Master Out, Slave In
    signal MISO: in;   // Master In, Slave Out
    signal SCK: out;   // Master drives clock
    signal CS: out optional;
    
    perspective slave {
        signal MOSI: in;   // Slave receives
        signal MISO: out;  // Slave transmits
        signal SCK: in;    // Slave receives clock
        signal CS: in optional;
    }
}
```

### 6.3 Interface Instantiation

```bhdl
board Example {
    power VCC = 3.3V @ 1A;
    ground GND;
    
    // Basic instantiation
    i2c_bus: I2C;
    
    // With parameter overrides
    spi_bus: SPI(width=16, frequency=10MHz);
    
    // With explicit perspective
    uart_dte: UART(mode="dte", baudrate=115200);
    uart_dce: UART(mode="dce", baudrate=115200);
    
    // Direct instantiation in connections
    sensor: I2CSensor;
    sensor.i2c <=> I2C();  // Anonymous interface
}
```

### 6.4 Interface Connections

```bhdl
// Pin-to-interface connections
module MCU {
    interface I2C i2c;
    interface SPI spi_master;
    pin VDD: power in;
    pin GND: ground;
}

board System {
    mcu: MCU;
    i2c_bus: I2C;
    
    // Connect MCU's I2C to bus (merges all signals)
    mcu.i2c <=> i2c_bus;
    
    // Direct signal access
    VCC -> Res(4.7kΩ).1 -> i2c_bus.SDA;
    VCC -> Res(4.7kΩ).1 -> i2c_bus.SCL;
}

// Interface-to-interface connections
spi_master: SPI(mode="master");
spi_slave: SPI(mode="slave");

// Connect master to slave (signal mapping handled automatically)
spi_master <=> spi_slave;

// Multiple devices on same interface
i2c_bus <=> [sensor1.i2c, sensor2.i2c, eeprom.i2c];
```

### 6.5 Interface Requirements

```bhdl
interface I2C {
    signal SDA: inout;
    signal SCL: out;
    
    // Pullup requirements
    require pullup(SDA, 4.7kΩ);
    require pullup(SCL, 4.7kΩ);
}

interface LVDS {
    signal P: out;
    signal N: out;
    
    // Differential termination
    require termination(P, N, 100Ω);
}

interface CAN {
    signal CANH: inout;
    signal CANL: inout;
    
    // Bus termination at endpoints
    require termination(CANH, CANL, 120Ω) when is_endpoint;
}
```

### 6.6 Advanced Interface Features

```bhdl
// Nested interfaces
interface RMII {
    signal TX_CLK: out;
    signal TX_EN: out;
    signal TXD[2]: out;
    
    // Management interface
    interface MDIO mdio;
}

// Interface arrays
module AudioCodec {
    interface I2S[4] channels;  // 4 I2S interfaces
    pin VDD: power in;
    pin GND: ground;
}

// Conditional signals
interface FlexibleSPI {
    signal MOSI: out;
    signal MISO: in;
    signal SCK: out;
    signal CS: out when !single_device;
    signal WP: out when flash_mode;
    signal HOLD: out when flash_mode;
}

// Interface with timing constraints
interface DDR3 {
    signal CK: out;
    signal DQS: inout;
    signal DQ[8]: inout;
    
    constrain timing {
        setup(DQ, DQS) >= 0.5ns;
        hold(DQ, DQS) >= 0.5ns;
        skew(CK, DQS) <= 0.2ns;
    }
}
```

### 6.7 Interface Generation

```bhdl
// Generate interface connections
board MultiSensor {
    i2c_bus: I2C;
    
    generate for i in 0..7 {
        sensor[i]: TempSensor;
        sensor[i].i2c <=> i2c_bus;
        sensor[i].ADDR -> (i & 0x07);  // Address pins
    }
}

// Generate chained interfaces
uart_chain[4]: UART;
generate for i in 0..3 {
    uart_chain[i].TX -> uart_chain[i+1].RX;
}
```

---

## 7. Power Management

### 7.1 Power Domain Declaration

```bhdl
power_domains {
  USB_5V: input_power {
    voltage = 5V ± 5%;
    current_max = 2A;
    source = USB_CONNECTOR.VBUS;
  };
  
  VCC_3V3: regulated_power {
    voltage = 3.3V ± 3%;
    current_max = 1A;
    efficiency_min = 85%;
  };
  
  VCC_1V8_CORE: core_power {
    voltage = 1.8V ± 2%;
    current_max = 2A;
    ripple_max = 50mVpp;
  };
}
```

### 7.2 Power Flow Specification

```bhdl
// Declarative power flow
power_flows {
  main_power: USB_5V |> 
              protection(overvoltage=5.5V, overcurrent=2.1A) |>
              regulation(3.3V, efficiency=85%) |>
              distribution |> 
              [digital_loads, analog_loads, io_loads];
  
  core_power: VCC_3V3 |>
              switching_regulation(1.8V, efficiency=90%) |>
              low_noise_filtering |>
              core_loads;
}
```

### 7.3 Power Sequencing

```bhdl
// Simple power sequencing using flow + conditionals
power_sequence {
  startup: USB_5V |> 
           delay(10ms) |> VCC_3V3.enable() |>
           if (VCC_3V3.stable) { 
             [VCC_1V8.enable(), VCC_1V2.enable()] 
           } |>
           if (all_stable) { RESET.release(); };
  
  shutdown: RESET.assert() |> 
            VCC_1V2.disable() |> VCC_1V8.disable() |>
            delay(50ms) |> VCC_3V3.disable();
}
```

### 7.4 Low-Power Modes

```bhdl
// Power state management
power_modes {
  ACTIVE: all_rails_full_power;
  
  SLEEP: {
    VCC_1V8.reduce_to(1.5V);  // Lower core voltage
    VCC_DDR.self_refresh_mode();
    unused_peripherals.power_gate();
  };
  
  HIBERNATION: {
    save_state_to(external_flash);
    all_rails.disable();
    backup_domain.maintain(1µA);
  };
}
```

---

## 8. Level Shifting

### 8.1 Automatic Level Shifting

```bhdl
// Automatic insertion based on voltage domains
mcu.GPIO(3.3V) -> sensor.INT(1.8V);  // Auto level shift
sensor.DATA(1.8V) -> mcu.ADC(3.3V);  // Auto level shift

// Tool automatically selects and inserts appropriate level shifters
```

### 8.2 Manual Level Shifter Control

```bhdl
// Override automatic selection when needed
mcu.SPI_MOSI(3.3V) -> level_shift(type=TXS0108E, channel=1) -> 
                      sensor.SPI_MOSI(1.8V);

// Conditional level shifter selection
mcu.GPIO(3.3V) -> level_shift(
  if (high_speed) { TXS0108E } else { 74LVC1T45 }
) -> external_device(1.8V);
```

### 8.3 Bidirectional Level Shifting

```bhdl
// Bidirectional interfaces with auto-direction sensing
cross_domain_i2c: I2C(from=3.3V, to=1.8V);
mcu.i2c1 <-> cross_domain_i2c <-> low_voltage_sensors;

// Tool automatically handles:
// - Bidirectional level shifter selection (PCA9306, TXS0108E)
// - Auto-direction sensing
// - Back-drive protection
// - Pullup resistor management
```

### 8.4 Power Sequence Integration

```bhdl
// Level shifters integrate with power sequencing
power_sequence {
  stage1: VCC_3V3.enable();
  stage2: VCC_1V8.enable();
  stage3: if (both_domains_stable) { level_shifters.enable(); };
  stage4: communication_interfaces.enable();
}
```

---

## 9. Physical Constraints

### 9.1 Component Placement

```bhdl
constrain placement {
  // Proximity constraints
  place crystal near mcu within 5mm;
  place decoupling_caps near mcu within 2mm;
  
  // Area constraints
  place power_management {
    area = rectangle(20mm, 15mm);
    location = edge(left, margin=5mm);
  };
  
  // Orientation constraints
  place connectors at edges;
  orient high_power_components for thermal_relief;
}
```

### 9.2 Routing Constraints

```bhdl
constrain routing {
  // High-speed digital
  route ddr_signals {
    impedance = 50Ω ± 10%;
    length_match = ±0.1mm;
    reference_plane = solid_ground;
    via_count_max = 2;
  };
  
  // Power distribution
  route power_rails {
    min_width = 0.5mm;
    thermal_vias = enabled;
    plane_connection = direct;
  };
  
  // Differential pairs
  route_diff usb_signals {
    impedance = 90Ω ± 10%;
    length_match = ±0.05mm;
    spacing = 0.2mm;
  };
}
```

### 9.3 Layer Stackup

```bhdl
constrain stackup {
  layers = 4;
  thickness = 1.6mm;
  
  layer1: signal + components;
  layer2: ground_plane(continuous);
  layer3: power_planes {
    VCC_3V3: area(60%);
    VCC_1V8: area(40%);
  };
  layer4: signal + components;
}
```

---

## 10. Multi-File Team Workflow

### 10.1 File Structure

```
project/
├── system/
│   └── requirements.bhdl          # System architect
├── circuit/
│   ├── power_management.bhdl      # Board designer
│   ├── communication.bhdl
│   └── support_circuits.bhdl
├── layout/
│   ├── constraints.bhdl           # Layout engineer
│   └── stackup.bhdl
└── integration/
    └── main_board.bhdl            # Integration file
```

### 10.2 System Level (System Architect)

```bhdl
// requirements.bhdl
system STM32H7_DevBoard {
  metadata {
    performance_target = "480MHz ARM Cortex-M7";
    memory_requirement = "512MB DDR3";
    power_budget = "2W maximum";
    target_cost = "$25";
  };
  
  functional_blocks {
    processing: ARM_Cortex_M7(480MHz);
    memory: DDR3_External(512MB);
    connectivity: [USB2, Debug_SWD, I2C, SPI, GPIO_Expansion];
    power: USB_Powered(efficiency_min=85%);
  };
  
  system_flows {
    data_flow: External_Memory <-> Processing_Core <-> Peripherals;
    power_flow: USB_Input |> Power_Management |> Rail_Distribution;
  };
  
  requirements {
    boot_time < 2s;
    operating_temp = -20°C to +70°C;
    emc_compliance = FCC_ClassB;
  };
}
```

### 10.3 Circuit Level (Board Designer)

```bhdl
// power_management.bhdl
import "../system/requirements.bhdl";

circuit_implementation PowerManagement {
  implements STM32H7_DevBoard.power;
  
  power_flows {
    primary_rail: USB_5V |>
                  protection(overvoltage=5.5V) |>
                  filtering(emi_filter) |>
                  main_5v_rail;
    
    digital_rail: main_5v_rail |>
                  switching_regulation(3.3V, efficiency=88%) |>
                  decoupling(bulk=22µF, ceramic=0.1µF) |>
                  VCC_3V3;
    
    core_rail: VCC_3V3 |>
               ldo_regulation(1.8V, low_noise=true) |>
               local_decoupling(10µF + 0.1µF) |>
               VCC_1V8_CORE;
  };
  
  protection {
    input_protection: overvoltage + overcurrent + reverse_polarity;
    thermal_management: temperature_monitoring + thermal_shutdown;
  };
}
```

### 10.4 Layout Level (Layout Engineer)

```bhdl
// constraints.bhdl
import "../circuit/power_management.bhdl";

physical_implementation Layout_Constraints {
  board_specifications {
    size = 100mm x 80mm;
    layers = 4;
    material = FR4;
    thickness = 1.6mm;
  };
  
  component_placement {
    place mcu at center(50mm, 40mm);
    place ddr_ram near mcu within 15mm;
    place PowerManagement.components {
      area = rectangle(25mm, 20mm);
      location = corner(bottom_left, margin=5mm);
    };
  };
  
  routing_constraints {
    route_group ddr_interface {
      impedance = 50Ω ± 10%;
      length_match = ±0.1mm;
      layer_preference = [1, 4];
      reference_plane = layer2;
    };
    
    route_power VCC_3V3 {
      min_width = 0.5mm;
      via_size = 0.3mm;
      thermal_relief = disabled;
    };
  };
}
```

---

## 11. Standard Library

### 11.1 Component Library

```bhdl
// std.components.*
Resistor(value, tolerance=5%, power=0.25W, package=auto);
Capacitor(value, voltage, dielectric="X7R", package=auto);
Inductor(value, current, dcr, package=auto);

LED(color, current=20mA, package="0805");
Diode(type, voltage, current, package=auto);

OpAmp(part_number, package=auto);
Comparator(part_number, package=auto);
LinearRegulator(output_voltage, current, package=auto);
```

### 11.2 Interface Library

```bhdl
// std.interfaces.*
I2C(voltage, frequency=400kHz, pullups=4.7kΩ);
SPI(voltage, frequency=1MHz, mode=0);
UART(voltage, baud=115200, flow_control=false);
USB2(speed=full_speed, power=100mA);
DDR3(width=16bit, speed=800MHz, voltage=1.5V);
```

### 11.3 Pattern Library

```bhdl
// std.patterns.*
voltage_divider(input, output, ratio, accuracy=5%);
rc_filter(input, output, cutoff_frequency);
crystal_oscillator(frequency, load_capacitance);
power_on_reset(delay, threshold);
linear_regulator_circuit(input_v, output_v, current);
```

---

## 12. Complete Working Example

### 12.1 Realistic 7805 Linear Regulator Circuit

This example demonstrates all key BHDL v2.0 features in a real circuit:

```bhdl
// Realistic 7805 Linear Regulator Circuit - BHDL v2.0
// A complete 12V to 5V power supply with proper filtering and protection
// Updated with explicit net naming using @ syntax

board PowerSupply_7805 {
    // Power domains
    power VIN = 12V @ 1A;      // Input voltage from DC adapter
    power VCC = 5V @ 1A;       // Regulated output
    ground GND;
    
    // Input protection and filtering with named nets
    VIN @RAW-> fuse: Fuse(1A).1;
    fuse.2 @PROTECTED-> tvs: TVSDiode(15V).1;
    tvs.2 -> GND;
    
    // Input filtering capacitors on protected net
    @PROTECTED -> c_in1: ElectrolyticCap(100µF, 25V).+;
    @PROTECTED -> c_in2: Cap(0.1µF).1;
    c_in1.- -> GND;
    c_in2.2 -> GND;
    
    // Linear regulator circuit
    @PROTECTED -> reg: LM7805().IN;
    reg.GND -> GND;
    reg.OUT @5V-> c_out1: ElectrolyticCap(10µF, 10V).+;
    
    // Output filtering
    @5V -> c_out2: Cap(0.1µF).1;
    c_out1.- -> GND;
    c_out2.2 -> GND;
    
    // LED power indicator
    @5V -> r_led: Res(330Ω).1;
    r_led.2 @LED_DRIVE-> led: LED(green).A;
    led.K -> GND;
    
    // Test points for measurement
    @PROTECTED -> tp_vin: TestPoint().1;
    @5V -> tp_vout: TestPoint().1;
    GND -> tp_gnd: TestPoint().1;
    
    // Output header
    @5V -> conn: Header_1x3.1;   // Power out
    GND -> conn.2;               // Ground
    @5V -> conn.3;               // Second power pin
    
    // Board metadata
    attribute title = "7805 Linear Regulator Power Supply";
    attribute version = "2.0";
    attribute author = "BHDL Test Suite";
    attribute description = "12V to 5V linear regulator with protection and filtering";
}
```

### Key Features Demonstrated:

1. **Power Domain Declaration**
   - `power VIN = 12V @ 1A` - Input power specification
   - `power VCC = 5V @ 1A` - Output power specification
   - `ground GND` - Ground reference

2. **Component Handles and Named Nets (@)**
   - Component handles: `fuse:`, `tvs:`, `reg:` create component references
   - Named nets: `@RAW->`, `@PROTECTED->`, `@5V->` create explicit nets
   - Net references: `@PROTECTED`, `@5V` always use @ prefix
   - Clear distinction: `fuse` is component, `@PROTECTED` is net

3. **Component Instantiation**
   - Direct instantiation: `Fuse(1A)`, `LM7805()`, `LED(green)`
   - Parameter specification: `ElectrolyticCap(100µF, 25V)`
   - Pin access: `.1`, `.2`, `.+`, `.-`, `.A`, `.K`

4. **Net Organization**
   - `@RAW` - Input after fuse
   - `@PROTECTED` - After TVS diode protection
   - `@5V` - Regulated output
   - `@LED_DRIVE` - Current-limited LED drive
   - Anonymous nets used where naming adds no value

4. **Automatic Reference Designators**
   - The toolchain assigns: F1, D1, C1-C4, U1, R1, LED1, TP1-TP3
   - Users work with meaningful names via handles

5. **Attributes**
   - Board-level metadata for documentation and tool processing

---

---

## 13. Advanced Power Sequencing

### 13.1 Declarative Power Sequences

```bhdl
power_up_sequence {
  // Stage-based sequencing with timing control
  stage1: {
    USB_5V.enable();
    wait_for USB_5V.power_good(timeout=100ms);
    delay(10ms);
  };
  
  stage2: {
    VCC_3V3.enable();
    wait_for VCC_3V3.power_good(timeout=50ms);
    delay(5ms);
  };
  
  stage3_parallel: {
    branch_A: {
      VCC_1V2_CORE.enable();
      wait_for VCC_1V2_CORE.power_good(timeout=20ms);
    };
    
    branch_B: {
      delay(2ms);
      VCC_1V8_IO.enable();
      wait_for VCC_1V8_IO.power_good(timeout=30ms);
    };
    
    sync_point;
  };
  
  stage4: {
    depends_on [VCC_1V2_CORE.power_good, VCC_1V8_IO.power_good];
    VCC_DDR.enable();
    wait_for VCC_DDR.power_good(timeout=20ms);
    
    delay(5ms);
    VCC_ANALOG.enable();
    wait_for VCC_ANALOG.power_good(timeout=10ms);
  };
  
  stage5: {
    delay(10ms);
    SYSTEM_RESET.deassert();
    POWER_LED.enable();
    system_state = SYSTEM_ON;
  };
}
```

### 13.2 Low-Power Mode Management

```bhdl
low_power_modes {
  LIGHT_SLEEP: {
    VCC_1V2_CORE.reduce_to(0.8V);
    VCC_DDR.enter_self_refresh();
    VCC_ANALOG.maintain();
    
    wake_sources = [GPIO_interrupt, UART_activity, Timer];
    wake_time_typical = 10µs;
  };
  
  DEEP_SLEEP: {
    VCC_1V2_CORE.reduce_to(0.6V);
    VCC_1V8_IO.gate_unused_banks();
    VCC_DDR.disable();
    VCC_ANALOG.disable();
    
    VCC_RTC.maintain(source=battery_backup);
    
    wake_sources = [RTC_alarm, GPIO_wakeup];
    wake_time_typical = 100ms;
  };
  
  HIBERNATION: {
    system_state.save_to(external_flash);
    [VCC_1V2_CORE, VCC_1V8_IO, VCC_DDR, VCC_ANALOG].disable();
    VCC_3V3.disable();
    
    VCC_BACKUP.maintain(source=coin_cell, current=1µA);
    
    wake_sources = [RTC_alarm, power_button];
    wake_time_typical = 2s;
  };
}
```

---

## 14. Advanced Level Shifting

### 14.1 Automatic Level Shifter Insertion

```bhdl
// Automatic level shifting based on voltage domains
mcu.GPIO(3.3V) -> sensor.INT(1.8V);  // Auto-inserts 3.3V-to-1.8V shifter
sensor.DATA(1.8V) -> mcu.ADC(3.3V);  // Auto-inserts 1.8V-to-3.3V shifter

// Manual override when needed
mcu.SPI_MOSI(3.3V) -> level_shift(type=TXS0108E, channel=1) -> 
                      sensor.SPI_MOSI(1.8V);

// Conditional level shifter selection
mcu.GPIO(3.3V) -> level_shift(
  if (high_speed) { TXS0108E } else { 74LVC1T45 }
) -> external_device(1.8V);
```

### 14.2 Bidirectional Level Shifting

```bhdl
// I2C bus crossing domains with auto-direction sensing
cross_domain_i2c: I2C(from=3.3V, to=1.8V);
mcu.i2c1 <-> cross_domain_i2c <-> low_voltage_sensors;

// Tool automatically handles:
// - Bidirectional level shifter selection (PCA9306, TXS0108E)
// - Auto-direction sensing
// - Back-drive protection
// - Pullup resistor management
```

### 14.3 Back-Drive Protection

```bhdl
back_drive_protection {
  cross_domain_uart: {
    transmitter: mcu.UART_TX(domain=VCC_3V3);
    receiver: module.UART_RX(domain=VCC_1V8);
    
    power_dependencies {
      VCC_3V3.power_up_time = 50ms;
      VCC_1V8.power_up_time = 30ms;
      back_drive_risk = high;
    };
    
    level_shifter = auto_select BackDriveProtectedLevelShifter {
      features = [
        auto_direction_sensing,
        power_down_protection,
        output_disable_when_vcc_low
      ];
      
      part = "TXS0108E" {
        channels_used = 1;
        auto_direction = true;
        partial_power_down = supported;
      };
    };
  };
}
```

---

## 15. Team Workflow and Multi-File Support

### 15.1 File Structure for Team Collaboration

```
project/
├── system/
│   ├── system_architecture.bhdl     # System architect's domain
│   ├── power_budget.bhdl           # Power requirements
│   └── interface_definitions.bhdl   # External interfaces
├── circuit/
│   ├── power_management.bhdl       # Board designer's domain
│   ├── signal_processing.bhdl      # Circuit implementations
│   └── support_circuits.bhdl       # Reset, clocks, etc.
├── layout/
│   ├── physical_constraints.bhdl   # Layout engineer's domain
│   ├── layer_stackup.bhdl         # PCB stackup definition
│   └── component_placement.bhdl    # Placement constraints
└── integration/
    └── main_board.bhdl             # Integrates all files
```

### 15.2 System Architecture Level

```bhdl
// system_architecture.bhdl - System Architect's specification
system_spec STM32H7_System {
  metadata {
    author = "System Architecture Team";
    version = "1.0";
    target_cost = $25;
    target_power = 2W;
  }
  
  functional_blocks {
    processing: ARM_Cortex_M7 {
      frequency = 480MHz;
      memory_external = DDR3_512MB;
      storage = QSPI_Flash_32MB;
    };
    
    power_management: USB_Powered {
      input = USB_TypeC;
      efficiency_target = 85%;
      rails = [5V, 3.3V, 1.8V, 1.2V];
    };
    
    connectivity: {
      usb_device: USB2_FullSpeed;
      debug: SWD_Interface;
      expansion: GPIO_Header_40pin;
      serial: UART_Console;
    };
  }
  
  system_flows {
    power_distribution:
      USB_Input |> PowerManagement |> [ProcessingCore, Memory, Peripherals];
    
    data_flows:
      ExternalMemory <-> ProcessingCore <-> Peripherals;
  }
  
  requirements {
    boot_time < 2s;
    power_consumption < 2W;
    operating_temp = -40°C to +85°C;
    emc_compliance = FCC_ClassB + CE;
  }
}
```

### 15.3 Common Pattern Shortcuts

```bhdl
// Decoupling capacitor patterns
mcu.VDD <- standard_decoupling <- VCC;  // Uses 10µF + 0.1µF default
mcu.VDDA <- low_noise_decoupling <- VCC;  // Uses 10µF + 1µF + 0.1µF + 10nF

// Location-aware decoupling
mcu.VDD <- local_decoupling(0.1µF within 2mm) + 
           bulk_decoupling(10µF within 10mm) <- VCC;

// Pull-up/pull-down resistor banks
i2c_bus.SCL, i2c_bus.SDA <- pullup_bank(4.7kΩ) <- VCC;
gpio_inputs[0:7] <- pulldown_bank(10kΩ) <- GND;

// LED indicator arrays
status_pins[0:3] -> led_array(colors=[red,green,blue,yellow], current=2mA) -> GND;

// Crystal oscillator with load caps
mcu.OSC_IN, mcu.OSC_OUT <-> crystal_circuit(25MHz, load=18pF);

// Voltage divider shortcuts
vref_2v5 <- voltage_divider(5V, ratio=0.5, accuracy=1%) <- VCC;

// Filter patterns
audio_in -> lowpass_filter(cutoff=20kHz, order=2) -> preamp_in;
switching_node -> emi_filter(common_mode + differential) -> clean_output;

// ESD protection patterns
usb_connector.DP, usb_connector.DN <- esd_protection(type=TVS, clamp=5.5V);
```

---

## 16. Language Reference

### 16.1 Core Language Constructs (7 Total)

1. **Component Instantiation**: `VCC -> Res(4.7kΩ).1 -> LED(red).A;`
2. **Flow Specification**: `INPUT |> amplify(10x) |> filter |> OUTPUT;`
3. **Interface Declaration**: `main_i2c: I2C(3.3V, 400kHz);`
4. **Generate Loops**: `generate for i in 0..7 { GPIO[i] -> LED[i]; }`
5. **Conditional Logic**: `if (condition) { action } else { alternative }`
6. **Module Definition**: `module Name(params) { implementation }`
7. **Constraint Declaration**: `constrain { placement, routing, timing }`

### 16.2 Operators

```bhdl
->    // Unidirectional connection
<->   // Bidirectional connection  
<=>   // Interface connection
|>    // Flow operator
[]    // Grouping/arrays
{}    // Code blocks
()    // Parameters
```

### 16.3 Keywords

```bhdl
// Core constructs
if else when generate for in module constrain

// Declarations  
board system circuit interface power_domain

// Interface-specific
interface signal perspective require

// Types
signal power ground voltage current

// Modifiers
input output inout optional extends implements

// Interface directions
in out inout
```

### 16.4 Built-in Functions

```bhdl
// Timing
delay(time)
wait_for(condition, timeout)

// Conditions
stable(signal)
power_good(rail)
all_stable(rail_list)

// Component selection
auto_select(requirements)
optimize_for(criteria)

// Level shifting
level_shift(from_domain, to_domain, signal_type, speed_class)
i2c_level_shift(from_voltage, to_voltage, frequency_max)
spi_level_shift(from_voltage, to_voltage, frequency_max, channels)
```

---

## 17. Design Benefits and Advantages

### 17.1 Memorability and Learning
- **7 core constructs** instead of 20+ specialized ones
- **Consistent syntax** across all language areas
- **Single reference card** covers entire language
- **Natural progression** from simple to complex designs

### 17.2 Designer Productivity
- **Zero manual effort** for level shifting and power sequencing
- **Automatic component inference** from connection patterns
- **Pattern-based shortcuts** for common circuit idioms
- **Progressive refinement** - start simple, add detail where needed

### 17.3 Team Collaboration
- **Clear separation of concerns** between team members
- **Interface contracts** ensure requirements flow down correctly
- **Concurrent development** at appropriate abstraction levels
- **Automatic validation** of cross-team dependencies

### 17.4 Correctness by Construction
- **No forgotten level shifters** or wrong voltage connections
- **Automatic back-drive protection** during power sequencing
- **Power domain awareness** prevents electrical violations
- **Timing and signal integrity** automatically managed

### 17.5 Implementation Flexibility
- **Multiple abstraction levels** in same design
- **Tool optimization** for routine implementations
- **Manual override** available when needed
- **Standard library** hides implementation complexity

---

## 18. Electrical Safety Analysis

### 18.1 Overview

BHDL includes comprehensive electrical safety analysis that automatically detects dangerous conditions in circuits. The system is:
- **Data-driven**: Uses actual component specifications, not hardcoded values
- **Analysis-based**: Checks real currents and voltages from DC analysis
- **Generic**: Works for any component type (resistors, ICs, LEDs, etc.)
- **Actionable**: Suggests specific fixes when violations are found

### 18.2 Safety Checks

The analyzer performs the following safety checks in Pass 8:

```bhdl
// Example: LED without current limiting - DANGEROUS!
board UnsafeLED {
    power VCC = 5V @ 500mA;
    ground GND;
    
    VCC -> led1: LED(red).A;  // Will detect overcurrent!
    led1.K -> GND;
}
```

#### Current Limiting Check
- Detects components exceeding their maximum current ratings
- Applies derating factors (70% for conservative design)
- Severity levels: Critical (>100%), Warning (>70%)

#### Overvoltage Protection
- Checks components against voltage ratings
- Considers transients and supply variations
- Suggests protection circuits (TVS diodes, regulators)

#### Short Circuit Detection
- Identifies low-resistance paths between power and ground
- Detects abnormally high currents indicating shorts
- Highest priority to prevent fire hazards

### 18.3 Component Limits

Component limits come from the component database or explicit declarations:

```bhdl
// Component with explicit limits
module LED(color: string) {
    pin A: signal in;
    pin K: signal out;
    
    // Electrical limits
    max_current = 30mA;
    max_voltage = 3.3V;
    max_power = 100mW;
    forward_voltage = 2.0V @ 20mA;
}
```

### 18.4 Safety Violations

Violations are reported with:
- **Severity**: Info, Warning, Error, Critical
- **Location**: Specific components and nets
- **Technical details**: Measured vs. allowed values
- **User impact**: Plain language explanation
- **Estimated damage**: Time to failure and cost

Example output:
```
[CRITICAL] Current Limiting Check
  LED 'D1' current 500.0mA exceeds absolute maximum 30.0mA
  Technical: Measured: 0.500A, Max: 0.030A, Overcurrent ratio: 16.7x
  Impact: Component will fail immediately
  Damage: Overcurrent failure - 10ms to failure, $0.10 replacement cost
```

### 18.5 Automatic Fixes

The system can suggest automatic fixes:

```bhdl
// Suggested fix for overcurrent LED
CircuitModification::InsertComponent {
    component_type: Resistor,
    value: 180Ω,
    location: between VCC and led1.A,
    reason: "Add current limiting resistor for LED protection"
}
```

### 18.6 Derating Policy

Conservative derating factors ensure reliability:
- **Voltage**: 80% of maximum rating
- **Current**: 70% of maximum rating  
- **Power**: 50% of maximum rating
- **Temperature**: 80% of maximum range

### 18.7 Integration with Toolchain

Safety analysis runs automatically:
1. Parser → AST → Analyzer (Passes 1-7)
2. Synthesizer generates netlist
3. DC analysis computes voltages/currents
4. Safety analysis checks all components
5. Violations reported as diagnostics

No dangerous circuits pass through to PCB layout!

This completes the comprehensive BHDL v2.0 specification. The language provides a complete framework for modern board design while maintaining simplicity through its seven core constructs and flow-based paradigm.