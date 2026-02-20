# BHDL: Board Hardware Description Language
## Complete Specification v2.0

### Table of Contents
1. [Introduction](#1-introduction)
2. [Design Philosophy](#2-design-philosophy)
3. [Hello World Example](#3-hello-world-example)
4. [Complete Working Example](#4-complete-working-example)
5. [Core Language Constructs](#5-core-language-constructs)
   - [Connection Syntax and Physical Constraints](#53-connection-syntax-and-physical-constraints)
6. [Type System](#6-type-system)
7. [Component System](#7-component-system)
   - [Component Handles and Net Naming](#75-component-handles-and-net-naming)
   - [Dual-Role Component Syntax](#76-dual-role-component-syntax)
8. [Interface System](#8-interface-system)
9. [Power Management](#9-power-management)
10. [Level Shifting](#10-level-shifting)
11. [Physical Constraints](#11-physical-constraints)
12. [Toolchain and Integration](#12-toolchain-and-integration)
13. [Multi-File Team Workflow](#13-multi-file-team-workflow)
14. [Standard Library](#14-standard-library)
15. [Advanced Power Sequencing](#15-advanced-power-sequencing)
16. [Advanced Level Shifting](#16-advanced-level-shifting)
17. [Language Reference](#17-language-reference)
18. [Design Benefits and Advantages](#18-design-benefits-and-advantages)
19. [Electrical Safety Analysis](#19-electrical-safety-analysis)
20. [Formal Grammar](#20-formal-grammar)

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

## 3. Hello World Example

### 3.1 Minimal Blinking LED Circuit

Before diving into the full specification, let's see BHDL in action with the simplest possible circuit: a blinking LED.

```bhdl
// Hello World: Blinking LED - 5 lines of BHDL
board BlinkingLED {
    power VCC = 5V @ 100mA;           // Power source
    ground GND;                       // Ground reference
    
    @VCC -> Res(330Ω).1 -> LED(red).A;  // Current-limited LED
    LED.K -> @GND;                    // LED cathode to ground
}
```

**What this does:**
- Creates a simple circuit with a 5V power supply
- Connects a current-limiting resistor (330Ω) in series with a red LED
- The toolchain automatically assigns reference designators (R1, D1)
- Safety analysis ensures the LED won't be damaged

**Key concepts demonstrated:**
1. **Power domains**: `power VCC = 5V @ 100mA` declares a 5V rail with 100mA capacity
2. **Component instantiation**: `Res(330Ω)` creates a 330-ohm resistor
3. **Net naming**: `@VCC` and `@GND` reference power and ground nets
4. **Component handles**: `LED` becomes a handle to reference the LED's pins
5. **Pin connections**: `.A` (anode) and `.K` (cathode) access LED pins

### 3.2 Adding a Microcontroller

Let's expand to show GPIO control:

```bhdl
board MCU_LED {
    power VCC = 3.3V @ 500mA;
    ground GND;
    
    // Microcontroller
    mcu: STM32F103 {
        VDD <- @VCC;
        GND <- @GND;
    }
    
    // GPIO-controlled LED
    mcu.PA0 -> Res(1kΩ).1 -> LED(blue).A;
    LED.K -> @GND;
}
```

**New concepts:**
- **Component handles**: `mcu:` creates a named reference to the microcontroller
- **Pin mapping**: `mcu.PA0` accesses specific GPIO pins
- **Power connections**: `VDD <- @VCC` connects power (note the `<-` direction)

This 8-line example shows how BHDL naturally scales from simple to complex circuits while maintaining the same core syntax patterns.

---

## 4. Complete Working Example

### 4.1 Realistic 7805 Linear Regulator Circuit

This example demonstrates key BHDL v2.0 features in a practical circuit that many engineers will recognize:

```bhdl
// Realistic 7805 Linear Regulator Circuit - BHDL v2.0
// A complete 12V to 5V power supply with proper filtering and protection

board PowerSupply_7805 {
    // Power domains
    power VIN = 12V @ 1A;      // Input voltage from DC adapter
    power VCC = 5V @ 1A;       // Regulated output
    ground GND;
    
    // Input protection and filtering with named nets
    @VIN @RAW-> fuse: Fuse(1A).1;
    fuse.2 @PROTECTED-> tvs: TVSDiode(15V).1;
    tvs.2 -> @GND;
    
    // Input filtering capacitors on protected net
    @PROTECTED -> c_in1: ElectrolyticCap(100µF, 25V).+;
    @PROTECTED -> c_in2: Cap(0.1µF).1;
    c_in1.- -> @GND;
    c_in2.2 -> @GND;
    
    // Linear regulator circuit
    @PROTECTED -> reg: LM7805().IN;
    reg.GND -> @GND;
    reg.OUT @5V-> c_out1: ElectrolyticCap(10µF, 10V).+;
    
    // Output filtering
    @5V -> c_out2: Cap(0.1µF).1;
    c_out1.- -> @GND;
    c_out2.2 -> @GND;
    
    // LED power indicator
    @5V -> r_led: Res(330Ω).1;
    r_led.2 @LED_DRIVE-> led: LED(green).A;
    led.K -> @GND;
    
    // Test points for measurement
    @PROTECTED -> tp_vin: TestPoint().1;
    @5V -> tp_vout: TestPoint().1;
    @GND -> tp_gnd: TestPoint().1;
    
    // Output header
    @5V -> conn: Header_1x3.1;   // Power out
    @GND -> conn.2;               // Ground
    @5V -> conn.3;               // Second power pin
    
    // Board metadata
    attribute title = "7805 Linear Regulator Power Supply";
    attribute version = "2.0";
    attribute author = "BHDL Test Suite";
    attribute description = "12V to 5V linear regulator with protection and filtering";
}
```

**Key Features Demonstrated:**

1. **Power Domain Declaration**
   - `power VIN = 12V @ 1A` - Input power specification with current rating
   - `power VCC = 5V @ 1A` - Output power specification
   - `ground GND` - Ground reference declaration

2. **Component Handles and Named Nets (@)**
   - Component handles: `fuse:`, `tvs:`, `reg:` create component references
   - Named nets: `@RAW->`, `@PROTECTED->`, `@5V->` create explicit nets
   - Net references: `@PROTECTED`, `@5V`, `@VCC`, `@GND` always use @ prefix
   - Clear distinction: `fuse` is component handle, `@PROTECTED` is net name

3. **Component Instantiation**
   - Direct instantiation: `Fuse(1A)`, `LM7805()`, `LED(green)`
   - Parameter specification: `ElectrolyticCap(100µF, 25V)`
   - Pin access: `.1`, `.2`, `.+`, `.-`, `.A`, `.K`

4. **Net Organization**
   - `@RAW` - Input after fuse protection
   - `@PROTECTED` - After TVS diode protection  
   - `@5V` - Regulated output rail
   - `@LED_DRIVE` - Current-limited LED drive signal
   - Anonymous nets used where explicit naming adds no value

5. **Automatic Reference Designators**
   - The toolchain automatically assigns: F1, D1, C1-C4, U1, R1, LED1, TP1-TP3
   - Users work with meaningful semantic names via handles

6. **Board Metadata**
   - Attributes provide documentation and tool processing information

This example bridges from simple LED circuits to real-world power supply design, showing how BHDL scales naturally while maintaining readability.

---

## 5. Core Language Constructs

BHDL has exactly **7 core constructs** that compose to handle any complexity:

### 3.1 Component Instantiation and Net Naming

#### Anonymous Nets
```bhdl
// Universal pattern: source -> component(parameters) -> destination
@VCC -> Res(4.7kΩ).1 -> LED(red).A;  // Creates anonymous nets
@USB_5V -> regulator: LinearReg(3.3V, 1A).IN;
```

#### Named Nets with @ Syntax
```bhdl
// Named nets use @ prefix for creation and reference
@VCC @FILTERED-> r1: Res(4.7kΩ).1;      // Creates net @FILTERED
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
@VCC -> r1: Res(10k).1;        // Creates anonymous net
r1.2 -> led: LED(red).A;      // Another anonymous net

// Named nets with @ syntax
@VCC @FILTERED-> r1: Res(10k).1;    // Creates net @FILTERED
@FILTERED -> c1: Cap(100n).1;      // References net @FILTERED
@FILTERED -> c2: Cap(10µ).1;       // Multiple connections to same net

// Clear distinction
r1.2 -> led.A;          // r1, led are component handles
@FILTERED -> r1.1;      // FILTERED is a net
fuse.2 -> @PROTECTED;   // fuse is component, PROTECTED is net
```

#### Key Rules:
1. **Net Creation**: Use `@NAME->` to create a named net
2. **Net Reference**: Always use `@NAME` when referencing a net (including power/ground)
3. **Component Handles**: Use `:` exclusively for component handles, no prefix to reference
4. **Anonymous Nets**: Use `->` without `@NAME`
5. **Disambiguation**: `@` always indicates a net, never a component
6. **Power/Ground**: Declared with keywords but referenced with `@` (e.g., `@VCC`, `@GND`)

### 3.3 Connection Syntax and Physical Constraints

#### Basic Connections
BHDL v2.0 supports two connection paradigms: net-based and pin-to-pin connections.

```bhdl
// Net-based connections (for logical equivalence)
@VCC -> U1.VDD;
@VCC -> U2.VDD;
@GND -> C1.2;

// Pin-to-pin connections (preserves physical topology)
L1.2 -> C1.1;          // Inductor output to capacitor
C1.1 -> FB_DIV.top;    // Feedback taps at C1, not elsewhere
C1.1 -> C2.1;          // Bulk cap further downstream
```

#### Connection Constraints with 'where'
Use the `where` keyword to specify physical constraints on individual connections:

```bhdl
// Trace length constraints
C1.1 -> FB.top where trace_length < 10mm;
XTAL.out -> MCU.OSC_IN where length = 15mm ± 0.5mm, no_vias;

// Impedance control
TX.out -> RX.in where impedance = 50Ω, matched_length;
CPU.CLK -> RAM.CLK where impedance = 50Ω, max_vias = 2;

// Current and power constraints
@VCC -> MOTOR.power where current_rating >= 5A, trace_width >= 2mm;
L1.2 -> C1.1 where current_rating = 3A;

// Special routing requirements
OPAMP.out -> ADC.in where shielded, guard_ring;
SENSOR.out -> AMP.in where differential(AMP.in_n), spacing = 0.2mm;
```

#### Constraint Resolution Policy

When multiple constraints apply to the same connection, BHDL follows a priority-based resolution:

1. **Most Specific Wins**: Direct `where` clauses override broader `with` blocks
2. **Additive Constraints**: Non-conflicting constraints are combined
3. **Error on Conflicts**: Contradictory constraints generate compilation errors

```bhdl
// Example: Constraint resolution
with routing(impedance = 90Ω) {
    // This generates an ERROR - conflicting impedance values
    USB_DP -> CONN.DP where impedance = 50Ω;  // Conflicts with 90Ω above
}

// Correct approach: Specify which constraint to use
with routing(default_impedance = 90Ω) {
    USB_DP -> CONN.DP where impedance = 50Ω;  // Explicit override
    USB_DM -> CONN.DM;  // Uses default 90Ω
}

// Additive constraints work fine
CPU.CLK -> RAM.CLK where impedance = 50Ω, max_vias = 2, length_match = true;
```

**Resolution Rules:**
- **Compatible**: Different constraint types are combined
- **Conflicting**: Same constraint type with different values causes error
- **Override**: Use `override` keyword to explicitly replace inherited constraints
- **Inheritance**: Child entities inherit parent constraints unless overridden

#### Connection Groups with 'with'
Use the `with` keyword to apply shared constraints to multiple connections:

```bhdl
// Matched impedance group
with routing(impedance = 50Ω, matched_length) {
    CPU.D0 -> RAM.D0;
    CPU.D1 -> RAM.D1;
    CPU.D2 -> RAM.D2;
    CPU.D3 -> RAM.D3;
}

// Differential pairs
with routing(differential = true, impedance = 100Ω) {
    PHY.TX_P -> CONN.TX_P;
    PHY.TX_N -> CONN.TX_N;
}

// Power distribution
with power(min_width = 1mm, max_voltage_drop = 50mV) {
    @VCC -> U1.VDD where bypass = C1;
    @VCC -> U2.VDD where bypass = C2;
    @VCC -> U3.VDD where bypass = C3;
}

// High-speed buses
with routing(impedance = 50Ω ± 5%, matched_length = true) {
    // DDR3 data lines
    generate for i in 0..15 {
        CPU.DDR_D[i] -> RAM.D[i];
    }
}
```

#### Nested Groups
Groups can be nested for hierarchical constraint application:

```bhdl
with routing(layer = "top", reference = "ground_plane") {
    // All connections in this block are on top layer
    
    with impedance(50Ω ± 10%) {
        // These also have impedance control
        CPU.ADDR[0] -> RAM.A0;
        CPU.ADDR[1] -> RAM.A1;
    }
    
    with power(width >= 0.5mm) {
        // Power connections on top layer
        @VCC -> U1.VDD;
        @VCC -> U2.VDD;
    }
}
```

#### Topology-Aware Connections
Pin-to-pin connections naturally express circuit topology, critical for analog circuits:

```bhdl
// Buck converter with feedback tap point
entity BuckConverter {
    // Power path with explicit topology
    L1.2 -> C1.1;              // Inductor to first cap
    C1.1 -> R_TOP.1;           // Feedback divider taps HERE
    R_TOP.2 -> R_BOT.1;        // Divider middle
    R_BOT.2 -> @GND;           
    R_TOP.2 -> CONTROLLER.FB;  // Feedback to controller
    
    // Bulk capacitance further away
    C1.1 -> C2.1 where trace_width >= 2mm;
    C2.1 -> C3.1;
    C3.1 -> OUTPUT_CONN.1;
}

// Op-amp with gain-setting resistors
OPAMP.out -> R1.1;
R1.2 -> C1.1;              // Compensation cap
C1.1 -> R2.1;              
R2.1 -> OPAMP.inv;         // Feedback path
R2.2 -> @GND;
```

#### Best Practices
1. **Use nets for**: Power/ground distribution, multi-drop digital signals, true equipotential nodes
2. **Use pin-to-pin for**: Analog signal paths, feedback networks, topology-critical connections
3. **Apply constraints**: Close to the source of the requirement
4. **Group related connections**: For maintainability and consistency

### 3.4 Flow Specification
```bhdl
// Universal flow operator |> for any domain
power_flow: USB_5V |> protection |> regulation |> distribution;
signal_flow: INPUT |> amplify(10x) |> filter(1kHz) |> OUTPUT;
data_flow: sensors |> i2c_bus |> mcu |> processing;
```

### 3.5 Interface Declaration
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

// Array shortcut for simple fan-out patterns
@VCC -> Res(330Ω)[8] -> LED(red)[8].A;   // Creates 8 identical resistor-LED pairs
LED[all].K -> @GND;                      // Connect all LED cathodes

// Parameterized arrays  
@VCC -> Res(values=[1kΩ, 2kΩ, 4kΩ])[3] -> loads[3];

// Named array elements
generate for i in 0..3 {
  gpio_bank[i]: GPIO_Expander(channels=8) {
    VCC <- @VCC_3V3;
    GND <- @GND;
  }
}
```

#### Array Syntax Benefits

The array shortcut syntax provides concise notation for common patterns:

```bhdl
// Traditional generate (verbose but explicit)
generate for i in 0..7 {
  @VCC -> r[i]: Res(330Ω).1 -> led[i]: LED(red).A;
  led[i].K -> @GND;
}

// Array shortcut (concise for identical elements)  
@VCC -> Res(330Ω)[8] -> LED(red)[8].A -> @GND;

// Mixed approach (flexibility when needed)
@VCC -> Res(330Ω)[4] -> LED(red)[4].A;     // First 4 LEDs red
@VCC -> Res(680Ω)[4] -> LED(blue)[4].A;    // Next 4 LEDs blue, different resistance
LED[all].K -> @GND;                        // All cathodes to ground
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

### 3.7 Entity Definition

Entities enable hierarchical design and code reuse by encapsulating functionality into reusable components with well-defined interfaces.

#### Basic Entity Syntax
```bhdl
// Entity with parameters and pins
entity RC_Filter(R_value: resistance = 1kΩ, C_value: capacitance = 100nF) {
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

// Simple entity without parameters
entity PowerIndicator() {
    pin VCC: power in;
    pin GND: ground in;
    
    // Status LED with current limiting
    VCC -> Res(1kΩ).1 -> LED(green).A;
    LED(green).K -> GND;
}
```

#### Entity Instantiation
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
        VCC <- @VCC_12V;
        GND <- @GND;
    }
}
```

#### Hierarchical Reference Designators
Components within entity instances receive hierarchical names:
```bhdl
// In the above example, components are named:
// - input_filter.R1 (10kΩ resistor)
// - input_filter.C1 (47nF capacitor)
// - output_filter.R1 (1kΩ resistor - default)
// - output_filter.C1 (100nF capacitor - default)
// - power_indicator.R1 (1kΩ resistor)
// - power_indicator.D1 (green LED)
```

#### Entity Pin Types
```bhdl
entity ComplexInterface() {
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

#### Parameterized Entities
```bhdl
// Parameters with types and constraints
entity VoltageRegulator(
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

#### Entity Arrays and Generate
```bhdl
entity LEDArray(count: int = 8) {
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
        VCC <- @VCC_5V;
        GND <- @GND;
        CTRL <- gpio_bus[0..15];
    }
}
```

#### Entity Composition
```bhdl
// Entities can instantiate other entities
entity PowerManagement() {
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

#### Entity Variants and Deduplication
The BHDL toolchain automatically deduplicates entity instances with identical parameters:
```bhdl
// These create a single entity definition
filter1: RC_Filter(1kΩ, 100nF) { ... }
filter2: RC_Filter(1kΩ, 100nF) { ... }  // Reuses same entity

// This creates a new variant
filter3: RC_Filter(10kΩ, 10nF) { ... }  // New entity variant
```

#### Entity Imports and Multi-File Support
```bhdl
// Import all public entities from a file
import "common/filters.bhdl";
import "power/regulators.bhdl";

// Import specific entities (destructuring)
import { RC_Filter, LC_Filter } from "common/filters.bhdl";
import { LinearReg, BuckConverter } from "power/regulators.bhdl";

#### Parameter Override Semantics

Entity parameters can be overridden during instantiation with specific precedence rules:

```bhdl
// Entity definition with parameters
entity VoltageRegulator(
    input_voltage: voltage = 12V,          // Required parameter
    output_voltage: voltage = 5V,          // Required parameter
    max_current: current = 1A,             // Optional with default
    efficiency: percentage = 85%,          // Optional with default
    topology: string = "linear"            // String parameter with default
) {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground in;
    pin EN: signal in when topology == "switching";
    
    // Parameter usage in implementation
    generate if (topology == "linear") {
        VIN -> LinearReg(output_voltage, max_current).VIN;
    } else if (topology == "switching") {
        VIN -> BuckConverter(input_voltage, output_voltage, max_current).VIN;
    }
}

// Parameter override during instantiation
board PowerSystem {
    power VIN_12V = 12V @ 2A;
    ground GND;
    
    // Override specific parameters
    main_regulator: VoltageRegulator(
        output_voltage = 3.3V,              // Override default 5V
        max_current = 1.5A,                 // Override default 1A
        topology = "switching"              // Override default "linear"
        // input_voltage uses default 12V
        // efficiency uses default 85%
    ) {
        VIN <- @VIN_12V;
        VOUT -> @VCC_3V3;
        GND <- @GND;
        EN <- power_enable_signal;          // Conditional pin available due to topology override
    }
    
    // Use all default parameters
    auxiliary_regulator: VoltageRegulator() {
        VIN <- @VIN_12V;
        VOUT -> @VCC_5V;  // Uses default 5V output
        GND <- @GND;
        // No EN pin since topology defaults to "linear"
    }
}
```

**Parameter Override Rules:**

1. **Positional vs Named Parameters:**
```bhdl
// Both syntaxes are valid
regulator1: VoltageRegulator(12V, 3.3V, 2A);           // Positional
regulator2: VoltageRegulator(                          // Named (preferred)
    input_voltage = 12V,
    output_voltage = 3.3V, 
    max_current = 2A
);

// Mixed positional and named (positional must come first)
regulator3: VoltageRegulator(12V, output_voltage=3.3V, max_current=2A);
```

2. **Type Checking:**
```bhdl
// Type mismatches generate errors
bad_regulator: VoltageRegulator(
    input_voltage = "12V",          // ERROR: string literal, expected voltage type
    output_voltage = 3.3,           // ERROR: raw number, expected voltage type
    max_current = 2000mA,           // OK: current type with units
    topology = switching            // ERROR: unquoted identifier, expected string
);

// Correct typing
good_regulator: VoltageRegulator(
    input_voltage = 12V,            // voltage type
    output_voltage = 3.3V,          // voltage type  
    max_current = 2A,               // current type
    topology = "switching"          // string type
);
```

3. **Default Parameter Resolution:**
   - Unspecified parameters use their default values
   - Defaults are evaluated in the entity's context
   - Defaults can reference other parameters

```bhdl
entity SmartRegulator(
    input_voltage: voltage,                     // Required, no default
    output_voltage: voltage = input_voltage / 2, // Default references input_voltage
    ripple_spec: voltage = output_voltage * 0.01 // Default is 1% of output
) {
    // Implementation uses calculated defaults
}

// Usage
smart_reg: SmartRegulator(input_voltage = 12V);
// Automatically calculates: output_voltage = 6V, ripple_spec = 60mV
```

4. **Parameter Scope and Visibility:**
```bhdl
entity OuterEntity(outer_param: voltage = 5V) {
    // Parameter is visible throughout entity scope

    inner_entity: InnerEntity(
        param1 = outer_param,               // Reference outer entity parameter
        param2 = outer_param * 0.5          // Computed from outer parameter
    );
    
    // Parameters affect conditional compilation
    generate if (outer_param > 3.3V) {
        high_voltage_protection: TvsProtection();
    }
}
```

#### Net Attribute System

Net attributes provide metadata for electrical analysis and physical constraints:

```bhdl
// Net attributes specify electrical and physical properties
board AttributeExample {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Nets with electrical attributes
    @VCC @FILTERED_POWER-> input_filter: Cap(100µF).+ {
        // Net attributes for @FILTERED_POWER
        attribute impedance = 0.1Ω @ 100kHz;
        attribute current_rating = 1.5A;
        attribute ripple_spec = 50mVpp;
        attribute safety_class = SELV;      // Safety Extra Low Voltage
    };
    
    input_filter.- -> @GND;
    
    // High-frequency nets with SI attributes
    @FILTERED_POWER -> regulator: BuckConverter().VIN;
    regulator.SW @SWITCHING_NODE-> inductor: Ind(10µH).1 {
        // Switching net attributes
        attribute frequency = 500kHz;
        attribute voltage_swing = 0V to 5V;
        attribute slew_rate = 100V/µs;
        attribute emi_class = CISPR22_ClassB;
        attribute routing_priority = high;   // Layout priority
    };
    
    inductor.2 @REGULATED_OUT-> output_cap: Cap(22µF).+ {
        // Output net attributes
        attribute voltage_tolerance = 3.3V ± 3%;
        attribute load_regulation = ±1%;
        attribute transient_response = 50µs;
    };
    
    output_cap.- -> @GND;
}
```

**Attribute Categories:**

1. **Electrical Attributes:**
```bhdl
// Power net attributes
@VCC_3V3 {
    attribute voltage_nominal = 3.3V;
    attribute voltage_tolerance = ±5%;
    attribute current_capacity = 2A;
    attribute ripple_max = 100mVpp;
    attribute efficiency_min = 85%;
}

// Signal net attributes  
@CLOCK_100MHZ {
    attribute frequency = 100MHz;
    attribute duty_cycle = 50% ± 2%;
    attribute jitter_max = 100ps;
    attribute rise_time = 1ns;
    attribute fall_time = 1ns;
}

// Analog net attributes
@ANALOG_SIGNAL {
    attribute signal_range = -2.5V to +2.5V;
    attribute bandwidth = 1MHz;
    attribute snr_min = 60dB;
    attribute thd_max = 0.1%;
}
```

2. **Physical/Routing Attributes:**
```bhdl
// High-speed differential pair
@USB_DP, @USB_DM {
    attribute impedance_differential = 90Ω ± 10%;
    attribute length_match = ±0.1mm;
    attribute via_count_max = 2;
    attribute layer_preference = [1, 4];  // Top and bottom layers
    attribute guard_traces = required;
}

// Power distribution attributes
@VCC_POWER_PLANE {
    attribute plane_layer = 3;
    attribute copper_weight = 2oz;
    attribute thermal_vias = enabled;
    attribute current_density_max = 20A/mm²;
}
```

3. **Safety and Compliance Attributes:**
```bhdl
// Safety critical nets
@SAFETY_INTERLOCK {
    attribute safety_integrity = SIL2;
    attribute redundancy = dual_channel;
    attribute diagnostic_coverage = 95%;
    attribute mtbf_min = 100000h;
}

// EMC compliance attributes
@SWITCHING_SIGNALS {
    attribute emc_class = CISPR22_ClassB;
    attribute radiated_limit = 40dBµV/m @ 30MHz;
    attribute conducted_limit = 60dBµV @ 150kHz;
}
```

**Attribute Inheritance and Propagation:**

```bhdl
// Attributes propagate through connections
@VCC_CLEAN {
    attribute ripple_max = 10mVpp;
    attribute noise_floor = -80dBV;
} -> precision_amplifier.VCC;
// precision_amplifier.VCC inherits ripple and noise specifications

// Attribute conflicts are resolved by precedence
with routing(impedance = 50Ω) {
    // Global impedance specification
    
    data_bus -> cpu.DATA where impedance = 75Ω;  // Local override wins
    address_bus -> cpu.ADDR;                     // Uses global 50Ω
}
```

**Attribute Validation:**

The analyzer validates attribute consistency and electrical feasibility:

```bhdl
// Validation example
@POWER_RAIL {
    attribute voltage_nominal = 3.3V;
    attribute current_capacity = 1A;
    attribute wire_gauge = 30AWG;           // WARNING: Insufficient for 1A
    attribute voltage_drop_max = 50mV;      // OK: Realistic specification
}

// Analyzer checks:
// - Wire gauge supports current capacity
// - Voltage drop is achievable with given resistance
// - Temperature rise within limits
// - Component ratings exceed operating conditions
```

This attribute system enables sophisticated electrical analysis while maintaining design intent throughout the toolchain.
// Relative imports
import "../shared/connectors.bhdl";
import "./local_entities.bhdl";
```

#### Entity Aliases
```bhdl
// Create shorter names for frequently used entities
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
3. **Hierarchical Organization**: Build complex systems from simple entities
4. **Consistent Naming**: Use descriptive names for entities and instances
5. **Documentation**: Add comments explaining entity purpose and usage
6. **File Organization**: Group related entities in separate files
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

### 6.4 Units and Physical Values

BHDL supports electrical units in both Unicode and ASCII formats for maximum compatibility:

```bhdl
// Unicode format (preferred for readability)
electrical_units {
  // Voltage
  3.3V, 5V, 12V, 230Vac, 120Vrms, 100mV, 50µV, 1kV
  
  // Current
  100mA, 2A, 50µA, 10nA, 5kA
  
  // Resistance
  4.7kΩ, 1MΩ, 0.1Ω, 100mΩ, 1GΩ
  
  // Capacitance
  10µF, 100nF, 1pF, 470µF, 1mF
  
  // Inductance
  1mH, 100µH, 10nH, 1H
  
  // Frequency
  16MHz, 400kHz, 50Hz, 1GHz
  
  // Time
  10ns, 1µs, 100ms, 1s, 10ps
  
  // Temperature
  85°C, -40°C, 25°C, 150°C
  
  // Power
  0.25W, 100mW, 1kW, 50µW
  
  // Percentages
  5%, 85%, 0.1%
}

// ASCII format (for legacy tool compatibility)
electrical_units_ascii {
  // Voltage
  3.3V, 5V, 12V, 230Vac, 120Vrms, 100mV, 50uV, 1kV
  
  // Current
  100mA, 2A, 50uA, 10nA, 5kA
  
  // Resistance
  4.7kohm, 1Mohm, 0.1ohm, 100mohm, 1Gohm
  
  // Capacitance
  10uF, 100nF, 1pF, 470uF, 1mF
  
  // Inductance
  1mH, 100uH, 10nH, 1H
  
  // Frequency
  16MHz, 400kHz, 50Hz, 1GHz
  
  // Time
  10ns, 1us, 100ms, 1s, 10ps
  
  // Temperature
  85degC, -40degC, 25degC, 150degC
  
  // Power
  0.25W, 100mW, 1kW, 50uW
  
  // Percentages
  5pct, 85pct, 0.1pct
}
```

#### Complete Unit System

The implementation supports comprehensive electrical unit parsing with standard SI prefixes:

**Multiplier Prefixes:**
- **Giga (G)**: 10⁹ (e.g., `1GΩ`, `1GHz`)
- **Mega (M)**: 10⁶ (e.g., `1MΩ`, `10MHz`)
- **Kilo (k)**: 10³ (e.g., `4.7kΩ`, `400kHz`)
- **Base unit**: 10⁰ (e.g., `1Ω`, `50Hz`)
- **Milli (m)**: 10⁻³ (e.g., `100mA`, `10mV`)
- **Micro (µ/u)**: 10⁻⁶ (e.g., `100µF`, `50µA`)
- **Nano (n)**: 10⁻⁹ (e.g., `100nF`, `10nA`)
- **Pico (p)**: 10⁻¹² (e.g., `10pF`, `100ps`)

**Voltage Types:**
- **DC**: `5V`, `3.3V` (default assumption)
- **AC**: `230Vac`, `120Vac` (RMS values)
- **RMS**: `120Vrms` (explicit RMS designation)
- **Peak-to-peak**: `10Vpp` (for signal analysis)

#### Unit Equivalence Table

| Electrical Quantity | Unicode | ASCII | Example |
|---------------------|---------|-------|---------|
| Resistance | `Ω`, `kΩ`, `MΩ` | `Ohm`, `kOhm`, `MOhm` | `4.7kΩ` = `4.7kOhm` |
| Capacitance | `µF`, `nF`, `pF` | `uF`, `nF`, `pF` | `10µF` = `10uF` |
| Current | `µA`, `mA`, `A` | `uA`, `mA`, `A` | `50µA` = `50uA` |
| Voltage | `µV`, `mV`, `V` | `uV`, `mV`, `V` | `100µV` = `100uV` |
| Time | `µs`, `ns`, `ps` | `us`, `ns`, `ps` | `1µs` = `1us` |
| Temperature | `°C` | `degC` | `25°C` = `25degC` |
| Percentage | `%` | `pct` | `5%` = `5pct` |

#### Typing Unicode Characters

**Quick reference for common symbols:**
- **Ω (Ohm)**: Alt+234 (Windows), Option+Z (Mac), Compose+O+M (Linux)
- **µ (Micro)**: Alt+230 (Windows), Option+M (Mac), Compose+m+u (Linux)  
- **° (Degree)**: Alt+248 (Windows), Option+Shift+8 (Mac), Compose+0+0 (Linux)

The parser automatically recognizes both formats, allowing mixed usage within the same file if needed.

---

## 5. Component System

### 5.1 Component Inference Pattern

```bhdl
// Natural component instantiation - no pre-declaration needed
@VCC -> Res(4.7kΩ).1 -> LED(red, 20mA).A;
LED.K -> @GND;

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
@VCC -> pullup_bank(4.7kΩ) -> [@SCL, @SDA];  // I2C pullups
@VCC -> decoupling(10µF + 0.1µF) -> mcu.VDD;  // Power decoupling
@INPUT -> lowpass_filter(1kHz) -> @OUTPUT;  // RC filter
```

### 5.4 Component Handles

```bhdl
// Explicit handles for multiple references
@VCC -> current_sense: Res(0.1Ω).1 -> @VOUT;
current_sense.2 -> current_monitor.INPUT;
current_sense.voltage_drop -> power_calculation;
```

### 5.5 Component Handles and Net Naming

#### Component Handles
```bhdl
// Component handle syntax: name: Component(...).pin
@VCC -> r1: Res(10kΩ).1;  // Creates component with handle "r1"
r1.2 -> led: LED(red).A;  // Reference component pins via handle
led.K -> @GND;

// Handles are ONLY component references, not net names
```

#### Named Nets with @ Syntax
```bhdl
// Create and reference named nets with @ prefix
@VIN @RAW-> fuse: Fuse(1A).1;
fuse.2 @PROTECTED-> tvs: TVSDiode(15V).1;
tvs.2 -> @GND;

// Reference named nets - ALWAYS with @
@PROTECTED -> bulk_cap: ElectrolyticCap(100µF, 25V).+;
@PROTECTED -> ceramic_cap: Cap(0.1µF).1;
bulk_cap.- -> @GND;
ceramic_cap.2 -> @GND;
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
@VIN @RAW-> fuse: Fuse(1A).1;           // @RAW is net, fuse is component
fuse.2 @FUSED-> tvs: TVSDiode(15V).1;  // @FUSED is net, tvs is component
@FUSED -> reg: LM7805.IN;              // Reference @FUSED net
reg.OUT @5V-> c_out: Cap(100µF).+;    // Create @5V net
reg.GND -> @GND;
c_out.- -> @GND;

// Multiple connections to named net
@5V -> r1: Res(330Ω).1;    // Power indicator
r1.2 -> led: LED(green).A;
led.K -> @GND;
@5V -> conn: Header_1x3.1;  // Power output
@GND -> conn.2;
@5V -> conn.3;              // Second power pin
```
#### Enhanced Pin Reference Syntax

BHDL supports multiple pin reference formats for different component types:

```bhdl
// Numbered pins (for passives and simple components)
@VCC -> r1: Res(10kΩ).1;        // Pin 1 of resistor
r1.2 -> led: LED(red).A;        // Pin 2 of resistor to LED anode

// Named pins (for complex components)
mcu: STM32F103 {
    VDD <- @VCC_3V3;            // Power pin by name
    GND <- @GND;                // Ground pin by name
    PA0 -> gpio_output;         // GPIO pin by name
    UART1_TX -> serial_out;     // Peripheral pin by function
}

// Mixed pin access on same component
connector: USB_TypeC {
    VBUS -> @USB_5V;            // Named power pin
    1 -> usb_dp;                // Numbered differential pair
    2 -> usb_dm;                // Numbered differential pair
    GND -> @GND;                // Named ground pins
}

// Special pin designations
cap: ElectrolyticCap(100µF, 25V) {
    + -> @VCC;                  // Positive terminal
    - -> @GND;                  // Negative terminal
}

// Array/bus pin access
ddr_ram: DDR3_SODIMM {
    DQ[0..7] -> cpu.DDR_DQ[0..7];   // Bus range connection
    A[0] -> cpu.DDR_A0;             // Individual address line
    A[1..15] -> cpu.DDR_A[1..15];   // Address bus subset
}
```

#### Component Handle Creation Details

Component handles provide persistent references for complex component interactions:

```bhdl
// Handle creation during instantiation
board PowerSupply {
    power VIN = 12V @ 2A;
    ground GND;
    
    // Create handles with : syntax
    @VIN -> fuse: Fuse(2A).1;                    // fuse is handle
    fuse.2 -> regulator: LM7805().IN;            // regulator is handle
    regulator.OUT -> output_cap: Cap(100µF).+;   // output_cap is handle
    regulator.GND -> @GND;
    output_cap.- -> @GND;
    
    // Use handles for multiple connections
    regulator.OUT -> power_led: LED(green).A;    // LED from same regulator output
    power_led.K -> led_resistor: Res(330Ω).1;   // Current limiting resistor
    led_resistor.2 -> @GND;
    
    // Handles enable component property access
    regulator.thermal_shutdown -> thermal_monitor.input;
    regulator.enable <- power_on_switch.output;
}
```

**Handle Naming Rules:**
- Must start with letter or underscore: `reg1`, `_internal`, `powerStage`
- Can contain letters, numbers, underscores: `uart_debug`, `sensor2`, `ADC_ch0`  
- Case sensitive: `led1` and `LED1` are different handles
- No limit on length: `high_precision_voltage_reference` is valid

**Handle Scope:**
- Handles are scoped to their containing board or entity
- Handles can be referenced throughout their scope
- Module instances create hierarchical handle namespaces

```bhdl
// Handle scoping example
entity PowerEntity() {
    pin VIN: power in;
    pin VOUT: power out;
    pin GND: ground in;

    // Local handles within entity
    VIN -> input_filter: Cap(10µF).+;
    input_filter.- -> GND;
    input_filter.+ -> regulator: LinearReg(3.3V).IN;
    regulator.OUT -> VOUT;
    regulator.GND -> GND;
}

board MainBoard {
    power VCC_12V = 12V @ 1A;
    ground GND;
    
    // Entity instance creates handle namespace
    power_entity: PowerEntity() {
        VIN <- @VCC_12V;
        VOUT -> @VCC_3V3;
        GND <- @GND;
    }

    // Access entity internal handles (if exposed)
    // power_module.regulator.thermal_pad -> thermal_via;
}
```

### 5.6 Dual-Role Component Syntax

BHDL supports a revolutionary dual-role syntax where component parameters can serve as both **values** and **constraints**, with the toolchain using electrical simulation to determine appropriate values when constraints are specified.

#### Value Specification (Traditional)
```bhdl
// Direct value specification - traditional approach
@VCC -> Res(4.7kΩ).1 -> LED(red).A;     // Explicit 4.7kΩ value
@VCC -> Res(330Ω).1 -> LED(green).A;    // Explicit 330Ω value
```

#### Constraint Specification (Revolutionary)
```bhdl
// Constraint-based specification - BHDL innovation
@VCC -> Res(?, current=20mA).1 -> LED(red).A;    // Infer resistance for 20mA
@VCC -> Res(?, power=0.5W).1 -> load;            // Infer value within power rating
@VCC -> Cap(?, ripple<50mV).1 -> load;           // Infer capacitance for ripple spec
```

#### Mixed Specification
```bhdl
// Combine explicit values with constraints
@VCC -> Res(10kΩ, power=0.5W).1 -> load;         // 10kΩ with power validation
@VCC -> Cap(100µF, voltage=25V).+ -> load;       // 100µF rated for 25V
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
@VCC -> r1: Res(?, current=20mA).1 -> led: LED(red, Vf=2.0V).A;
led.K -> @GND;
// SPICE calculates: R = (5V - 2.0V) / 20mA = 150Ω

// Power dissipation constraint
high_current_path -> Res(?, power=2W, tolerance=5%).1 -> load;
// SPICE selects appropriate value within 2W rating

// Filtering capacitor selection
noisy_rail -> Cap(?, ripple<100mV, esr<0.1Ω).+ -> clean_rail;
// SPICE determines capacitance for ripple requirement

// Voltage divider with ratio constraint
@VIN -> R1: Res(?, ratio=2:1).1 -> @VOUT -> R2: Res(?).1 -> @GND;
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
@VCC -> [
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

#### Placeholder Parameters for Synthesis

Components may use placeholder parameters when exact values should be determined by synthesis tools based on circuit analysis:

```bhdl
// Placeholder syntax with constraints
board AutoResistorSelection {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Placeholder with power and tolerance constraints
    @VCC -> r1: Res(<?rating: 0.5W, tolerance: 5%>) -> led1: LED(red) -> @GND;
    
    // Multiple constraint types
    filter_cap: Cap(<?voltage: 25V, temperature: -55C to +105C>) {
        + <- noisy_rail;
        - <- @GND;
    }
}
```

**Placeholder Syntax Rules:**
- Placeholders use `<?...>` syntax around constraint specifications
- Constraints are specified as `name: value` pairs within placeholders
- Multiple constraints are separated by commas
- Synthesis tools resolve placeholder values based on circuit analysis and constraints
- Common constraint types:
  - `rating`: power rating (e.g., `0.25W`, `0.5W`, `2W`)
  - `tolerance`: value tolerance (e.g., `1%`, `5%`, `10%`)
  - `voltage`: voltage rating (e.g., `25V`, `50V`, `100V`)
  - `temperature`: operating temperature range (e.g., `-40C to +85C`)
  - `package`: physical package constraint (e.g., `"0805"`, `"1206"`, `"SOT23"`)

**Resolution Process:**
1. Circuit analysis determines electrical requirements (current, voltage, power)
2. Constraints filter available components from component database
3. Synthesis selects optimal component meeting all requirements
4. Safety analysis validates the selection

**Example with LED Current Limiting:**
```bhdl
// Let SPICE determine optimal resistance for 20mA LED current
@VCC -> r_led: Res(<?rating: 0.25W, tolerance: 5%>) {
    constraint current_limit = 20mA;  // Target LED current
    constraint led_forward_voltage = 2.0V;  // LED specification
} -> led: LED(red) -> @GND;

// Synthesis calculates: R = (5V - 2.0V) / 20mA = 150Ω
// Then selects 150Ω ±5%, 0.25W resistor from component database
```

### 5.7 Virtual Pins and Synthesis Expansion

Virtual pins are a powerful synthesis feature that allows components to declare output pins that don't physically exist on the IC but represent the connection point after required external components are added. This enables clean, intuitive component interfaces while ensuring correct circuit synthesis.

#### 5.7.1 Virtual Pin Declaration

```bhdl
entity TPS54331(vout: voltage = 3.3V, iout: current = 2A) {
    // Physical pins - these exist on the actual IC
    pin VIN: power in;
    pin SW: switch out;         // Switch node output
    pin GND: ground inout;
    pin FB: feedback in;
    pin EN: enable in;

    // Virtual pin - represents the regulated output after external components
    pin VOUT: virtual power out;
}
```

#### 5.7.2 Virtual Pin Expansion Rules

Components define how their virtual pins expand into actual circuit paths:

```bhdl
entity TPS54331(vout: voltage = 3.3V, iout: current = 2A) {
    // Pin declarations (as above)

    // Define how VOUT virtual pin expands
    @virtual_expansion(VOUT) {
        // Path from physical pin to virtual pin
        path: SW -> [external_fet] -> [inductor] -> [output_cap] -> VOUT
        
        components: {
            external_fet: {
                type: NFET,
                params: {
                    vds_max: VIN * 1.5,      // Voltage derating
                    id_max: iout * 1.5       // Current derating
                },
                connections: {
                    SW -> gate,
                    drain -> VIN,
                    source -> next           // 'next' links to next component
                },
                intent: power_switching(     // Intent for synthesized component
                    topology: buck,
                    frequency: 300kHz,
                    efficiency_target: 0.9
                )
            },
            inductor: {
                type: Inductor,
                params: {
                    value: calc_buck_inductor(VIN, vout, iout, 300kHz),
                    current_rating: iout * 1.3
                },
                connections: {
                    prev -> 1,               // 'prev' links from previous component
                    2 -> next
                },
                intent: energy_storage(      // Intent for energy storage
                    ripple_current: iout * 0.3,
                    switching_freq: 300kHz
                )
            },
            output_cap: {
                type: Capacitor,
                params: {
                    value: 22uF,
                    type: ceramic,
                    voltage: vout * 1.5
                },
                connections: {
                    prev -> pos,
                    neg -> GND,
                    pos -> VOUT              // Virtual pin appears here
                },
                intent: output_filtering(    // Intent for output filtering
                    ripple_voltage: 20mV,
                    load_transient: 100mA/us
                )
            }
        }
    }
    
    // Additional synthesis rules for feedback, compensation, etc.
    @synthesis_rule(feedback) {
        when: connected(FB),
        create: {
            R_top: Resistor((vout/0.8 - 1) * 10k, 1%),
            R_bot: Resistor(10k, 1%)
        },
        connect: [
            VOUT -> R_top.1,
            R_top.2 -> FB,
            R_top.2 -> R_bot.1,
            R_bot.2 -> GND
        ]
    }
}
```

#### 5.7.3 Using Components with Virtual Pins

From the user's perspective, virtual pins behave exactly like physical pins:

```bhdl
board PowerSupply {
    power VIN = 12V @ 3A;
    ground GND;
    
    // Instantiate buck controller
    U1: TPS54331(vout=5V, iout=2A);
    
    // Connect to physical pins
    VIN -> U1.VIN;
    U1.GND -> GND;
    U1.EN -> VIN;  // Always enabled
    
    // Connect to virtual pin - user gets clean interface!
    U1.VOUT -> @5V_RAIL;
    
    // Use the output normally
    @5V_RAIL -> Load.VIN;
}
```

The synthesizer automatically expands this to:

```bhdl
// After synthesis (generated automatically):
VIN -> U1.VIN;
U1.SW -> Q1: NFET(30V, 5A).gate;
VIN -> Q1.drain;
Q1.source -> L1: Inductor(4.7uH, 3A).1;
L1.2 -> C1: Cap(22uF, ceramic).pos;
C1.neg -> GND;
L1.2 -> @5V_RAIL;  // Virtual pin VOUT maps here

// Feedback network (from synthesis_rule)
@5V_RAIL -> R1: Res(52.5k, 1%).1;
R1.2 -> U1.FB;
R1.2 -> R2: Res(10k, 1%).1;
R2.2 -> GND;
```

#### 5.7.4 Virtual Pin Benefits

1. **Clean Interfaces**: Users see simple, logical connections (U1.VOUT) instead of complex intermediate components
2. **Guaranteed Correctness**: Synthesis ensures all required components are added
3. **Reusability**: Same component definition works for all instances
4. **Self-Documenting**: Virtual pins clearly show what the component provides
5. **Flexibility**: Different component variants can have different expansion rules

#### 5.7.5 Multiple Virtual Pins

Components can declare multiple virtual pins for multi-output converters:

```bhdl
entity TPS54240(vout1: voltage = 5V, vout2: voltage = 3.3V) {
    // Physical pins
    pin VIN: power in;
    pin SW1: switch out;
    pin SW2: switch out;
    pin GND: ground inout;
    
    // Two virtual outputs
    pin VOUT1: virtual power out;
    pin VOUT2: virtual power out;
    
    @virtual_expansion(VOUT1) {
        path: SW1 -> [inductor] -> [output_cap] -> VOUT1
        // ... component specifications
    }
    
    @virtual_expansion(VOUT2) {
        path: SW2 -> [inductor] -> [output_cap] -> VOUT2
        // ... component specifications
    }
}

// User code remains simple:
U1: TPS54240();
U1.VOUT1 -> @5V;
U1.VOUT2 -> @3V3;
```

#### 5.7.6 Conditional Virtual Pins

Virtual pins can be conditional based on component configuration:

```bhdl
entity BuckController(fixed_output: bool = false, vout: voltage = 3.3V) {
    pin VIN: power in;
    pin SW: switch out;
    pin GND: ground inout;
    pin FB: feedback in when !fixed_output;  // Only present if adjustable
    
    // Virtual output always present
    pin VOUT: virtual power out;
    
    @virtual_expansion(VOUT) {
        // Expansion adapts based on configuration
        path: SW -> [inductor] -> [output_cap] -> VOUT
        // ...
    }
    
    @synthesis_rule(feedback) {
        when: !fixed_output && connected(FB),
        // Add feedback network only for adjustable versions
    }
}
```

#### 5.7.7 Synthesis Rules with Virtual Pins

Components can reference their virtual pins in synthesis rules:

```bhdl
@synthesis_rule(input_protection) {
    when: VIN > 24V,
    create: TVSDiode(VIN * 1.2),
    connect: [VIN -> tvs.cathode, tvs.anode -> GND]
}

@synthesis_rule(output_sensing) {
    when: requires_remote_sense,
    connect: [VOUT -> SENSE+, GND -> SENSE-]  // VOUT is virtual pin
}
```

### 5.8 Synthesizer Intent Generation

The synthesizer automatically generates intent information for all components it creates or modifies during synthesis. This ensures that synthesized circuits are rich with semantic information about the purpose and function of each component, enabling better simulation, validation, and optimization.

#### 5.8.1 Intent in Virtual Pin Expansion

When expanding virtual pins, the synthesizer adds intent to each created component:

```bhdl
entity TPS54331(vout: voltage = 3.3V, iout: current = 2A) {
    pin VOUT: virtual power out;

    @virtual_expansion(VOUT) {
        path: SW -> [external_fet] -> [inductor] -> [output_cap] -> VOUT

        components: {
            external_fet: {
                type: NFET,
                params: { vds_max: VIN * 1.5, id_max: iout * 1.5 },
                // Synthesizer adds intent for power switching
                intent: power_switching(
                    topology: buck,
                    frequency: 300kHz,
                    efficiency_target: 0.9,
                    switching_loss: 200mW
                )
            },
            inductor: {
                type: Inductor,
                params: { value: calc_buck_inductor(VIN, vout, iout, 300kHz) },
                // Synthesizer knows this is for energy storage
                intent: energy_storage(
                    ripple_current: iout * 0.3,
                    switching_freq: 300kHz,
                    core_loss_budget: 200mW
                )
            },
            output_cap: {
                type: Capacitor,
                params: { value: 22uF, type: ceramic },
                // Synthesizer adds output filtering intent
                intent: output_filtering(
                    ripple_voltage: 20mV,
                    load_transient: 100mA/us,
                    esr_requirement: 10mOhm
                )
            }
        }
    }
}
```

#### 5.8.2 Intent in Synthesis Rules

Synthesis rules specify intent for component groups they create:

```bhdl
@synthesis_rule(feedback_network) {
    when: connected(FB),
    create: {
        R_top: Resistor((vout/0.8 - 1) * 10k, 1%),
        R_bot: Resistor(10k, 1%)
    },
    connect: [
        VOUT -> R_top.1,
        R_top.2 -> FB,
        R_top.2 -> R_bot.1,
        R_bot.2 -> GND
    ],
    // Intent for the entire feedback network
    intent: voltage_regulation(
        setpoint: vout,
        accuracy: 1%,
        temp_stability: 50ppm/C,
        bandwidth: 10kHz,
        phase_margin: 60deg
    )
}

@synthesis_rule(input_protection) {
    when: VIN > 24V || transient_spec > 100V,
    create: {
        tvs: TVSDiode(VIN * 1.2),
        series_r: Resistor(10R, pulse_rated)
    },
    connect: [
        VIN -> series_r.1,
        series_r.2 -> tvs.cathode,
        tvs.anode -> GND
    ],
    // Protection intent with specifications
    intent: overvoltage_protection(
        clamp_voltage: VIN * 1.2,
        response_time: 1ns,
        energy_rating: 10J,
        protection_standard: "IEC61000-4-2"
    )
}
```

#### 5.8.3 Intent Inference for User Components

When users specify components without intent, the synthesizer infers intent based on topology and context:

```bhdl
// User writes:
U1.SW -> L1: Inductor(4.7uH).1 -> @VOUT;

// Synthesizer infers and adds:
U1.SW -> L1: Inductor(4.7uH).1 -> @VOUT
    for energy_storage(
        topology: buck,
        switching_freq: 300kHz,  // Detected from U1
        ripple_current: 0.6A,     // Calculated from topology
        saturation_margin: 1.3    // Safety factor
    );
```

#### 5.8.4 Complete Synthesis with Intent

A fully synthesized circuit includes intent for every component:

```bhdl
// User input (minimal):
board PowerSupply {
    power VIN = 12V @ 3A;
    ground GND;
    
    VIN -> U1: TPS54331(vout=5V).VIN;
    U1.GND -> GND;
    U1.VOUT -> @5V_RAIL;
}

// After synthesis with intent generation:
board PowerSupply {
    power VIN = 12V @ 3A;
    ground GND;
    
    // Input decoupling with intent
    net input_filter: VIN -> C_in: Cap(10uF, ceramic).pos
        for input_decoupling(
            switching_freq: 300kHz,
            source_impedance: 0.1R,
            ripple_current: 2A
        );
    C_in.neg -> GND;
    
    // Main converter
    VIN -> U1: TPS54331(vout=5V).VIN;
    U1.GND -> GND;
    
    // Power switching path with intent
    net switching: U1.SW -> Q1: NFET(30V, 5A).gate
        for power_switching(
            topology: buck,
            frequency: 300kHz,
            duty_cycle: 0.42,
            switching_loss: 250mW
        );
    VIN -> Q1.drain;
    
    // Energy storage with calculated intent
    net energy: Q1.source -> L1: Inductor(4.7uH).1
        for energy_storage(
            ripple_current: 0.6A,
            dc_current: 2A,
            inductance_tolerance: 20%,
            saturation_current: 3A
        );
    
    // Output filtering with performance intent
    net output: L1.2 -> C_out: Cap(22uF).pos -> @5V_RAIL
        for output_filtering(
            ripple_voltage: 20mV,
            load_step_response: 10us,
            esr_max: 10mOhm
        );
    C_out.neg -> GND;
    
    // Feedback network with control intent
    net feedback: @5V_RAIL -> R_top: Res(52.5k, 1%).1 -> @FB
        for voltage_sensing(
            divider_ratio: 6.25,
            accuracy: 1%,
            bandwidth: 100kHz
        );
    @FB -> R_bot: Res(10k, 1%).1 -> GND;
    @FB -> U1.FB
        for control_feedback(
            loop_type: voltage_mode,
            crossover_freq: 10kHz,
            phase_margin: 60deg
        );
    
    // Bootstrap circuit with intent
    net bootstrap: U1.BOOT -> C_boot: Cap(100nF, ceramic).1
        for gate_drive_bootstrap(
            charge_time: 100ns,
            hold_time: 10us
        );
    C_boot.2 -> U1.SW;
}
```

#### 5.8.5 Intent Categories for Synthesis

The synthesizer uses these standard intent categories:

```bhdl
// Power Conversion
intent power_switching(topology, frequency, efficiency_target, switching_loss)
intent energy_storage(ripple_current, switching_freq, core_loss_budget)
intent power_rectification(forward_drop, reverse_recovery, power_dissipation)

// Filtering
intent input_filtering(noise_freq, attenuation, source_impedance)
intent output_filtering(ripple_voltage, load_transient, esr_requirement)
intent emi_filtering(frequency_range, attenuation, compliance_standard)

// Protection
intent overvoltage_protection(clamp_voltage, response_time, energy_rating)
intent overcurrent_protection(trip_current, response_time, reset_type)
intent reverse_polarity_protection(voltage_drop, current_rating)
intent esd_protection(level, standard, capacitance)

// Control & Sensing
intent voltage_regulation(setpoint, accuracy, bandwidth, stability_margin)
intent current_sensing(range, accuracy, bandwidth, isolation)
intent temperature_monitoring(range, accuracy, thermal_constant)

// Compensation
intent loop_compensation(type, crossover_freq, phase_margin, gain_margin)
intent frequency_compensation(poles, zeros, bandwidth)

// Decoupling & Bypassing
intent power_decoupling(frequency_range, target_impedance, current_slew_rate)
intent high_frequency_bypass(resonant_freq, q_factor, placement_critical)

// Signal Conditioning
intent signal_buffering(impedance_in, impedance_out, bandwidth)
intent level_shifting(voltage_from, voltage_to, propagation_delay)
intent signal_filtering(filter_type, cutoff_freq, rolloff)
```

#### 5.8.6 Intent Determination Algorithm

The synthesizer determines intent through multiple analysis layers:

```bhdl
// 1. Topology Analysis
if component in buck_topology.energy_path:
    intent = energy_storage(calculated_parameters)

// 2. Electrical Function Analysis
if component.type == Capacitor && connected_to_power:
    if frequency_analysis shows switching_noise:
        intent = input_decoupling(detected_frequency)
    else:
        intent = bulk_storage(hold_time_requirement)

// 3. Connection Pattern Analysis
if component between high_voltage and sensitive_node:
    intent = protection(voltage_limit, response_time)

// 4. Control Loop Analysis
if component in feedback_path:
    intent = control_feedback(loop_characteristics)
```

#### 5.8.7 Benefits of Synthesizer Intent Generation

1. **Simulation Optimization**: Simulators can use appropriate models based on intent
2. **Validation**: Verify synthesized components meet their intended purpose
3. **Documentation**: Generated circuits are self-documenting
4. **Optimization**: Choose optimal components based on intent requirements
5. **Fault Analysis**: Understand impact when components fail
6. **Layout Generation**: Intent guides critical placement and routing
7. **BOM Selection**: Select real parts that meet intent specifications
8. **Design Review**: Intent makes design decisions explicit and reviewable

#### 5.8.8 Intent Verification

The synthesizer can verify that components meet their intent requirements:

```bhdl
// After synthesis, verify intent is satisfied
verify inductor L1 {
    intent: energy_storage(ripple_current: 0.6A, saturation_margin: 1.3)
    actual: {
        ripple_current: calculate_ripple(L1.value, frequency, voltage)
        saturation_current: L1.isat_rating
    }
    assert: actual.ripple_current <= 0.6A
    assert: actual.saturation_current >= dc_current * 1.3
}
```

#### 5.8.9 Intent-Driven Optimization

The synthesizer can optimize component selection based on intent:

```bhdl
// Synthesizer selects optimal component for intent
optimize C_out for output_filtering {
    requirements: {
        ripple_voltage: 20mV
        load_transient: 100mA/us
        cost: minimize
    }
    
    // Synthesizer evaluates options:
    option1: Cap(22uF, ceramic, X7R) // Good ripple, moderate transient
    option2: Cap(47uF, ceramic, X5R) // Better transient, higher cost
    option3: Cap(100uF, aluminum)    // Poor high-freq, low cost
    
    selected: option1 // Meets requirements at lowest cost
}
```

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
entity MCU {
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
    @VCC -> Res(4.7kΩ).1 -> i2c_bus.SDA;
    @VCC -> Res(4.7kΩ).1 -> i2c_bus.SCL;
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
entity AudioCodec {
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

### 7.1 Power and Ground Declaration

Power and ground nets are declared using keywords but referenced with @ prefix:

```bhdl
// Declaration: use keywords without @
board Example {
    power VCC = 5V @ 1A;        // Declare power net
    power VCC_3V3 = 3.3V @ 500mA;
    ground GND;                 // Declare ground net
    
    // Reference: always use @ prefix
    @VCC -> Res(10k).1;         // Reference power net
    @VCC_3V3 -> mcu.VDD;        // Reference power net
    led.K -> @GND;              // Reference ground net
}
```

### 7.2 Power Domain Declaration

Power domains can be declared using either the simple keyword syntax or advanced block syntax:

#### Simple Power Domain Syntax
```bhdl
// Basic power domain declarations
board Example {
    // Simple power domains with voltage and current
    power VCC = 5V @ 1A;           // 5V rail, 1A capacity
    power VCC_3V3 = 3.3V @ 500mA;  // 3.3V rail, 500mA capacity
    power USB_5V = 5V @ 2A;        // USB power input
    
    // Ground domains
    ground GND;                    // Main ground reference
    ground CHASSIS_GND;            // Chassis/shield ground
    ground ANALOG_GND;             // Separate analog ground
}
```

#### Advanced Power Domain Block Syntax
```bhdl
// Advanced power domain declarations with detailed specifications
power_domains {
  USB_5V: input_power {
    voltage = 5V ± 5%;              // Voltage with tolerance
    current_max = 2A;               // Maximum current capacity
    source = USB_CONNECTOR.VBUS;    // Physical connection source
    protection = [overvoltage, overcurrent, reverse_polarity];
  };
  
  VCC_3V3: regulated_power {
    voltage = 3.3V ± 3%;            // Tight regulation tolerance
    current_max = 1A;               // Current capacity
    efficiency_min = 85%;           // Efficiency requirement
    ripple_max = 100mVpp;           // Ripple specification
    startup_time = 50ms;            // Power-up time
    dependencies = [USB_5V];        // Must come after USB_5V
  };
  
  VCC_1V8_CORE: core_power {
    voltage = 1.8V ± 2%;            // Core voltage precision
    current_max = 2A;               // High current for processor
    ripple_max = 50mVpp;            // Low ripple for sensitive circuits
    load_regulation = ±1%;          // Load regulation spec
    sequence_priority = 3;          // Power-up sequence order
    dependencies = [VCC_3V3];       // Derive from 3.3V rail
  };
}

// Ground domains with detailed specifications
ground_domains {
  GND: digital_ground {
    impedance_max = 1mΩ;           // Maximum ground impedance
    connection_type = star_point;   // Star grounding topology
  };
  
  ANALOG_GND: analog_ground {
    isolation_from = [GND];         // Isolated from digital ground
    connection_point = single_point; // Single-point connection
    noise_floor = -80dBV;          // Noise specification
  };
  
### 7.6 Intent Attachment to Flow Connections

BHDL allows attaching intent functions to specific flow connections to guide synthesis and analysis:

#### Basic Intent Attachment Syntax

```bhdl
// Intent attachment using 'for' keyword
board SignalProcessing {
    power VCC = 5V @ 1A;
    ground GND;
    
    // Intent attached to entire signal path
    analog_input -> amplifier.IN for low_noise(max_ripple=1mV);
    amplifier.OUT -> adc.IN for timing(setup_time=10ns);
    
    // Multiple intents on same connection
    sensitive_signal -> protection_circuit for [
        input_protection(overvoltage=5.5V),
        esd_protection(class=HBM_2kV)
    ];
    
    // Intent with parameters
    clock_source -> cpu.CLK for timing(
        frequency=100MHz,
        jitter<1ps,
        duty_cycle=50% ± 2%
    );
}
```

#### Intent Propagation Through Components

Intents propagate through component instances and affect the entire signal path:

```bhdl
// Intent propagation example
board MotorController {
    power VCC_12V = 12V @ 5A;
    ground GND;
    
    // Intent attached to flow affects entire path
    @VCC_12V -> motor_driver.VIN 
        -> motor_driver.OUT 
        -> motor.POWER 
        for high_current(rating=5A, protection=overcurrent);
    
    // Intent affects component selection and routing
    sensor.OUTPUT -> amplifier.IN 
        -> adc.IN 
        -> cpu.ADC_CH0 
        for low_noise(max_ripple=100µV);
}
```

#### Hierarchical Intent Inheritance

Intents are inherited through entity boundaries and can be overridden:

```bhdl
// Entity with intent specification
entity SensorInterface() {
    pin SENSOR_IN: signal in;
    pin DIGITAL_OUT: signal out;
    pin VCC: power in;
    pin GND: ground in;
    
    // Internal signal path with intent
    SENSOR_IN -> amplifier: OpAmp().IN for low_noise(max_ripple=10µV);
    amplifier.OUT -> adc: ADC().IN for precision(bits=16);
    adc.OUT -> DIGITAL_OUT;
}

board MainSystem {
    power VCC_ANALOG = 5V @ 100mA;
    ground ANALOG_GND;
    
    // Entity instance inherits and extends intents
    sensor_if: SensorInterface() {
        VCC <- @VCC_ANALOG;
        GND <- @ANALOG_GND;
        SENSOR_IN <- temperature_sensor.OUT for [
            // These intents combine with entity's internal intents
            temperature_compensation(range=-40C to +85C),
            calibration(points=3)
        ];
    }
    
    sensor_if.DIGITAL_OUT -> cpu.SPI_MISO;
}
```

#### Available Intent Functions

**Timing Intents:**
```bhdl
// Signal timing requirements
clock_path for timing(
    frequency=100MHz,
    jitter<500ps,
    duty_cycle=50% ± 1%,
    rise_time<2ns,
    fall_time<2ns
);

// Setup and hold timing
data_path for timing(
    setup_time=5ns,
    hold_time=2ns,
    propagation_delay<10ns
);

// Debouncing for mechanical inputs
switch_input for debounce(
    time=20ms,
    method=rc_filter
);
```

**Protection Intents:**
```bhdl
// Input protection
external_input for input_protection(
    overvoltage=5.5V,
    current_limit=100mA,
    esd_class=HBM_2kV
);

// Overcurrent protection
power_path for protection(
    overcurrent_limit=2A,
    thermal_shutdown=85C,
    short_circuit_protection=enabled
);

// EMI/EMC protection
switching_signal for emc_protection(
    frequency_limit=30MHz,
    rise_time_limit=5ns,
    filtering=ferrite_bead
);
```

**Signal Processing Intents:**
```bhdl
// Anti-aliasing filter
analog_input for anti_alias(
    before=adc_component,
    cutoff=1kHz,
    order=2,
    type=butterworth
);

// Low-noise requirements
sensitive_analog for low_noise(
    max_ripple=100µV,
    bandwidth=1kHz,
    snr>60dB,
    grounding=star_point
);

// Signal conditioning
sensor_output for signal_conditioning(
    gain=10x,
    offset_compensation=enabled,
    linearization=polynomial_3rd_order
);
```

#### Intent Resolution and Analysis

The toolchain uses intents to determine simulation requirements and synthesis hints:

```bhdl
// Intent analysis results
sensor_path for low_noise(max_ripple=1µV);
// Results in:
// - sim_mode: AnalogRequired (needs detailed noise analysis)
// - synthesis_hints: ["Use low-noise op-amps", "Star grounding", "Shielding recommended"]
// - validation_rules: ["Signal ripple must be < 1µV", "SNR > 80dB"]

high_speed_data for timing(frequency=1GHz);
// Results in:
// - sim_mode: DigitalWithTiming (needs timing simulation)
// - synthesis_hints: ["Differential signaling", "Impedance control", "Length matching"]
// - validation_rules: ["Rise time < 100ps", "Impedance = 50Ω ± 10%"]
```

#### Intent Conflict Resolution

When multiple intents apply to the same signal path, BHDL uses priority rules:

```bhdl
// Intent priority resolution
board ConflictExample {
    // Global intent for all connections in this board
    with intent(default_protection=overvoltage_5V5) {
        
        // Specific intent overrides global intent
        sensitive_input -> amplifier.IN for [
            input_protection(overvoltage=3V6),  // Overrides global 5.5V
            low_noise(max_ripple=10µV)          // Additional requirement
        ];
        
        // Global intent applies here (no override)
        digital_input -> buffer.IN;  // Uses overvoltage_5V5 protection
    }
}
```

**Priority Rules:**
1. **Most Specific**: Direct intent attachment overrides broader scopes
2. **Additive**: Non-conflicting intents are combined
3. **Error on Conflict**: Contradictory intents generate compilation errors
4. **Inheritance**: Child entities inherit parent intents unless overridden

This intent system enables design intent capture and guides both synthesis and analysis phases of the toolchain.
  CHASSIS_GND: safety_ground {
    connection = earth_ground;      // Earth ground connection
    isolation_voltage = 1500V;     // Safety isolation rating
  };
}
```

#### Power Domain Features

**Domain Types:**
- `input_power`: External power sources (USB, barrel jack, battery)
- `regulated_power`: Regulated supplies (linear or switching regulators)
- `core_power`: Processor/FPGA core voltages
- `memory_power`: Memory interface voltages (DDR, SRAM)
- `analog_power`: Clean power for analog circuits
- `backup_power`: Battery backup or supercap domains

**Common Properties:**
- `voltage`: Nominal voltage with optional tolerance (e.g., `3.3V ± 5%`)
- `current_max`: Maximum current capacity
- `ripple_max`: Maximum allowable ripple voltage
- `efficiency_min`: Minimum power conversion efficiency
- `startup_time`: Time to reach stable output
- `shutdown_time`: Time to discharge when disabled
- `dependencies`: Other domains that must be stable first
- `sequence_priority`: Numeric priority for power sequencing (1=first)

**Protection Features:**
```bhdl
// Protection specifications in power domains
protection = [
    overvoltage(threshold=5.5V, action=shutdown),
    overcurrent(limit=2.1A, action=current_limit),
    reverse_polarity(protection=schottky_diode),
    thermal_shutdown(threshold=85C),
    undervoltage_lockout(threshold=4.5V)
];
```

### 7.3 Power Flow Specification

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

### 7.4 Power Sequencing

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

### 7.5 Low-Power Modes

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

## 12. Toolchain and Integration

### 12.1 Toolchain Overview

The BHDL toolchain provides seamless integration with existing EDA workflows while offering advanced analysis capabilities:

```
BHDL Source (.bhdl)
       ↓
┌─────────────────┐
│ BHDL Compiler   │ ← Parse, analyze, synthesize
└─────────────────┘
       ↓
┌─────────────────┐
│ Circuit Analysis│ ← DC analysis, safety checks, optimization
└─────────────────┘
       ↓
┌─────────────────┐  
│ Netlist Export  │ ← Generate industry-standard outputs
└─────────────────┘
       ↓
┌─────────────────┐
│ EDA Tool Import │ ← Altium, KiCad, Cadence, etc.
└─────────────────┘
```

### 12.2 Import/Export Capabilities

#### Netlist Export
```bash
# Export to industry standard formats
bhdl export --format kicad_netlist project.bhdl
bhdl export --format altium_netlist project.bhdl  
bhdl export --format spice_netlist project.bhdl
bhdl export --format cadence_netlist project.bhdl

# Include component library references
bhdl export --format kicad_netlist --with-library project.bhdl
```

#### Component Library Integration
```bash
# Import existing component libraries
bhdl import --library kicad_symbols /path/to/library.kicad_sym
bhdl import --library altium_lib /path/to/library.IntLib

# Export BHDL component definitions  
bhdl export --library kicad_symbols --output symbols.kicad_sym
```

#### Schematic Import (Experimental)
```bash
# Import existing schematics for conversion
bhdl import --schematic kicad /path/to/schematic.kicad_sch
bhdl import --schematic altium /path/to/schematic.SchDoc
```

### 12.3 Constraint Export

Physical constraints defined in BHDL are exported to appropriate EDA tool formats:

```bhdl
// BHDL constraints
constrain routing {
  route ddr_signals {
    impedance = 50Ω ± 10%;
    length_match = ±0.1mm;
    via_count_max = 2;
  };
}
```

**Exports to:**
- **KiCad**: PCB rules (`.kicad_dru`) and constraint classes
- **Altium**: Design rules and room definitions  
- **Cadence**: Physical constraint sets
- **Mentor Graphics**: Constraint manager files

### 12.4 Debugging and Visualization

#### Net Connectivity Viewer
```bash
# Interactive net tracing
bhdl debug --trace-net @VCC_3V3 project.bhdl
bhdl debug --component-connections U1 project.bhdl

# Generate connectivity reports  
bhdl analyze --connectivity --output connectivity_report.html
```

#### Circuit Visualization
```bash
# Generate schematic-style diagrams
bhdl visualize --schematic --output schematic.svg project.bhdl

# Generate block diagrams
bhdl visualize --block-diagram --output blocks.svg project.bhdl

# Power flow visualization
bhdl visualize --power-flow --output power.svg project.bhdl
```

### 12.5 Quick Start Guide

#### 1. Installation
```bash
# Install BHDL toolchain
curl -sSL https://get.bhdl.dev | sh
# or
cargo install bhdl-cli
```

#### 2. Create Your First Project
```bash
# Create new project
bhdl new my_project
cd my_project

# Edit main board file
vim src/main_board.bhdl
```

#### 3. Build and Export
```bash
# Analyze design  
bhdl build --analyze

# Export to KiCad
bhdl export --format kicad_netlist --output build/netlist.net

# Export constraints
bhdl export --format kicad_rules --output build/rules.kicad_dru
```

#### 4. Import to EDA Tool
- **KiCad**: File → Import → Netlist, then load `build/netlist.net`
- **Altium**: Design → Import → Netlist, select `build/netlist.net`
- **Cadence**: Import → Netlist, specify format and file

### 12.6 Error Analysis and Safety Checks

```bash
# Run comprehensive safety analysis
bhdl check --safety project.bhdl

# Check for electrical violations
bhdl check --electrical project.bhdl  

# Verify power integrity
bhdl check --power project.bhdl

# Generate safety report
bhdl analyze --safety --output safety_report.html
```

#### Sample Error Output
```
[ERROR] Safety Analysis Failed
  Location: src/power.bhdl:15:8
  Component: LED 'D1' 
  Issue: Current limiting violation
  Details: LED current 500mA exceeds maximum 30mA (16.7x overcurrent)
  Suggestion: Add 180Ω resistor between @VCC and LED.A
  Auto-fix: bhdl fix --safety --component D1

[WARNING] Power Analysis  
  Location: src/main.bhdl:22:4
  Net: @VCC_3V3
  Issue: Supply current approaching limit
  Details: Total load 0.95A, supply rated 1.0A (95% utilization)
  Suggestion: Verify supply margins or increase capacity
```

### 12.7 IDE Integration

#### VS Code Extension
- Syntax highlighting and IntelliSense
- Real-time error checking
- Component library browser
- Interactive debugging

#### Language Server Protocol (LSP)
```bash
# Enable LSP for any editor
bhdl lsp --stdio
```

**Supported editors:** VS Code, Vim/Neovim, Emacs, Sublime Text, IntelliJ

### 12.8 Incremental Adoption Strategy

1. **Start Small**: Begin with simple circuits (LED, regulators)
2. **Library Building**: Convert existing component libraries to BHDL format  
3. **Power Circuits**: Leverage automatic analysis for power supplies
4. **Complex Designs**: Gradually adopt for larger, multi-board systems
5. **Team Workflow**: Implement multi-file collaboration features

### 12.9 Tool Compatibility Matrix

| EDA Tool | Netlist Import | Constraint Import | Library Import | Status |
|----------|----------------|-------------------|----------------|---------|
| KiCad 6+ | ✅ Full | ✅ PCB Rules | ✅ Symbols | Stable |
| Altium Designer | ✅ Full | ✅ Design Rules | ✅ Libraries | Stable |
| Cadence Allegro | ✅ Full | ✅ Constraint Sets | 🔄 Planned | Beta |
| Mentor Graphics | ✅ Basic | 🔄 Planned | 🔄 Planned | Alpha |
| Eagle | ✅ Basic | ❌ Limited | ❌ Limited | Legacy |

**Legend:** ✅ Supported, 🔄 In Development, ❌ Not Supported

### 12.10 Implementation Status

BHDL's advanced features are built on substantial existing infrastructure:

#### ✅ **Production Ready**
- **Component Database**: Full KiCad import/export with 10,000+ components
- **SPICE Analysis**: GLACIER solver with GPU acceleration  
- **Component Synthesis**: Two-stage synthesis with supplier integration
- **Safety Analysis**: Electrical limits checking with auto-fix suggestions
- **Multi-file Projects**: Full import/export system

#### 🔄 **Integration Phase**  
- **Dual-Role Syntax**: Parser and SPICE integration (constraint inference implemented)
- **Automatic Level Shifting**: Component database queries (level shifter components available)
- **Advanced Constraints**: Constraint resolution engine (individual components working)

#### 📋 **Roadmap**
- **IDE Extensions**: VS Code language server  
- **Advanced Visualization**: Interactive circuit diagrams
- **Cloud Component Libraries**: Distributed component databases

The core technical capabilities exist and are proven in production use - the focus is now on integration and user experience polish.

---

## 13. Multi-File Team Workflow

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

## 11. Standard Library and Custom Libraries

### 11.1 BHDL Standard Library (bhdl-stdlib)

The BHDL standard library provides a comprehensive collection of component definitions, electrical models, and design patterns. It serves as both a reference implementation and a practical component library.

#### Library Structure

```
bhdl-stdlib/
├── index.bhdl                  # Main library index
├── manifest.toml               # Library metadata
├── types/
│   └── electrical_types.bhdl   # Shared type definitions
├── passives/                   # Passive components
│   ├── resistor.bhdl
│   ├── capacitor.bhdl
│   ├── inductor.bhdl
│   ├── led.bhdl
│   └── diode.bhdl
├── regulators/                 # Voltage regulators
│   ├── linear_regulator_base.bhdl
│   └── lm7805.bhdl
├── power/                      # Power system components
│   ├── power.bhdl
│   └── ground.bhdl
├── connectors/
│   └── testpoint.bhdl
├── behavioral/                 # Behavioral models
│   └── buck_converter.bhdl
└── src/                       # Rust interface
    ├── lib.rs
    └── intents/               # Intent system
        ├── timing.rs
        ├── protection.rs
        └── signal_processing.rs
```

#### Component Library Examples

```bhdl
// Import standard library components
import "bhdl-stdlib/passives/resistor.bhdl";
import "bhdl-stdlib/passives/capacitor.bhdl";
import "bhdl-stdlib/regulators/lm7805.bhdl";

board ExampleCircuit {
    power VIN = 12V @ 1A;
    power VCC = 5V @ 500mA;
    ground GND;
    
    // Use stdlib components
    @VIN -> reg: LM7805() {
        IN <- @VIN;
        OUT -> @VCC;
        GND <- @GND;
    }
    
    // Standard passive components with full electrical models
    @VCC -> res: Res(330Ω, tolerance=1%, package="0805").1;
    res.2 -> led: LED(red).A;
    led.K -> @GND;
    
    // Decoupling with SPICE parameters
    @VCC -> cap: Cap(100µF, voltage=25V, type="electrolytic").+;
    cap.- -> @GND;
}
```

#### Electrical Type System

The stdlib provides comprehensive electrical type definitions:

```bhdl
// Shared electrical characteristics
type ElectricalLimits = {
    max_voltage: voltage?,
    max_current: current?,
    max_power: power?,
    operating_temp_min: temperature?,
    operating_temp_max: temperature?,
};

type ImpedanceCharacteristics = {
    dc_resistance: resistance?,
    output_impedance: resistance?,
    input_impedance: resistance?,
    can_source_current: bool,
    can_sink_current: bool,
    max_source_current: current?,
    max_sink_current: current?,
    voltage_drop: voltage?,
    current_limiting: bool,
    transient_response: time?,
};
```

### 11.2 Intent System

The intent system allows designers to specify functional requirements that guide synthesis and validation:

#### Available Intent Functions

```bhdl
// Timing intents
signal_path |> delay(10ns);                    // Specify signal delay
switch_input |> debounce(time=20ms);           // Switch debouncing

// Protection intents  
input_signal |> input_protection(             // Comprehensive protection
    overvoltage=5.5V, 
    current_limit=100mA
);
sensitive_line |> overvoltage_protection(3.6V); // Simple voltage clamping

// Signal processing intents
analog_input |> anti_alias(                   // Anti-aliasing filter
    before=adc_component,
    cutoff=1kHz
);
audio_path |> low_noise(max_ripple=1mV);      // Low-noise requirements
```

#### Intent Resolution

Intents automatically configure simulation and synthesis:

```bhdl
// Intent drives simulation mode and synthesis hints
sensor_output |> low_noise(max_ripple=100µV);

// This intent results in:
// - sim_mode: AnalogRequired (needs careful noise analysis)
// - synthesis_hints: ["Use low-noise components", "Consider shielding", "Star grounding"]
// - validation_rules: ["Signal ripple must be < 100µV"]
```

#### Custom Intent Functions

Define domain-specific intents:

```bhdl
// Custom intent for motor control
intent motor_protection(motor_component, max_current: current) -> IntentResult {
    return IntentResult {
        sim_mode: AnalogRequired,
        synthesis_hints: [
            format!("Current sense resistor for {}A", max_current),
            "Overcurrent shutdown circuit",
            "Thermal monitoring"
        ],
        validation_rules: [
            ValidationRule {
                condition: "has_current_sensing",
                error_message: "Motor protection requires current sensing"
            }
        ]
    };
}

// Usage
drive_motor |> motor_protection(max_current=5A);
```

### 11.3 Creating Custom Component Libraries

#### Library Structure

Create custom libraries with the same structure as bhdl-stdlib:

```
my-custom-lib/
├── manifest.toml               # Library metadata
├── index.bhdl                 # Export definitions
├── components/
│   ├── custom_amplifiers.bhdl
│   ├── sensors.bhdl
│   └── power_modules.bhdl
├── types/
│   └── custom_types.bhdl
└── src/                       # Optional Rust interface
    └── lib.rs
```

#### Library Manifest

```toml
# manifest.toml
[library]
name = "my-custom-lib"
version = "1.0.0"
authors = ["Your Team"]
description = "Custom component library for specific project"

[components]
categories = ["amplifiers", "sensors", "power"]

[compatibility]
bhdl-version = ">=2.0.0"

[dependencies]
bhdl-stdlib = "1.0.0"
```

#### Component Definition Example

```bhdl
// components/custom_amplifiers.bhdl
import "../types/custom_types.bhdl";

entity HighPrecisionOpAmp(
    gain: float = 1.0,
    bandwidth: frequency = 1MHz,
    offset: voltage = 1mV
) {
    pin VIN_P: signal in @metadata(
        function="NonInvertingInput",
        description="Non-inverting input pin",
        electrical_type="analog_input"
    );
    pin VIN_N: signal in @metadata(
        function="InvertingInput", 
        description="Inverting input pin",
        electrical_type="analog_input"
    );
    pin VOUT: signal out @metadata(
        function="Output",
        description="Amplifier output",
        electrical_type="analog_output"
    );
    pin VCC: power in;
    pin VEE: power in;
    
    // Custom electrical model
    attribute component_class = "operational_amplifier";
    attribute open_loop_gain = 120; // dB
    attribute bandwidth = bandwidth;
    attribute input_offset_voltage = offset;
    attribute slew_rate = 10V/µs;
    attribute supply_current = 2mA;
    
    // SPICE behavioral model
    attribute spice_model = "opamp_behavioral";
    attribute spice_gain = gain;
    attribute spice_bandwidth = bandwidth;
    attribute spice_offset = offset;
    
    // Custom validation rules
    intent high_precision() -> IntentResult {
        return IntentResult {
            sim_mode: AnalogRequired,
            synthesis_hints: [
                "Low-noise power supply required",
                "Guard rings recommended",
                "Thermal management needed"
            ],
            validation_rules: [
                ValidationRule {
                    condition: "supply_ripple < 1mV",
                    error_message: "High precision requires clean power"
                }
            ]
        };
    }
}
```

#### Library Integration

```bhdl
// Using custom libraries
import "my-custom-lib/components/custom_amplifiers.bhdl";
import "bhdl-stdlib/passives/resistor.bhdl";

board PrecisionInstrument {
    power VCC = 15V @ 100mA;
    power VEE = -15V @ 100mA;
    ground GND;
    
    // Use custom component with intent
    input_buffer: HighPrecisionOpAmp(gain=2.0, offset=100µV) {
        VIN_P <- sensor_input;
        VIN_N <- feedback_node;
        VOUT -> output_stage;
        VCC <- @VCC;
        VEE <- @VEE;
    } |> high_precision();
    
    // Standard feedback network
    output_stage -> Res(20kΩ).1 -> feedback_node;
    feedback_node -> Res(10kΩ).1 -> @GND;
}
```

### 11.4 Library Management

#### CLI Commands

```bash
# List available libraries
bhdl library list

# Install a library
bhdl library install my-custom-lib@1.0.0

# Create new library template
bhdl library create --name my-lib --template basic

# Validate library
bhdl library validate my-custom-lib/

# Publish library
bhdl library publish my-custom-lib/
```

#### Version Management

```bhdl
// Specify library versions
import "bhdl-stdlib@1.0.0/passives/resistor.bhdl";
import "my-custom-lib@>=1.2.0/sensors.bhdl";

// Library-specific configurations
library "my-custom-lib" {
    config temperature_units = "celsius";
    config default_packages = ["0805", "1206"];
}
```

### 11.5 Standard Library Components Reference

#### Passive Components
- `Res(value, tolerance=5%, package="0805")` - Resistors with SPICE models
- `Cap(value, voltage=50V, type="auto")` - Capacitors (ceramic/electrolytic)
- `Ind(value, current=1A, core="ferrite")` - Inductors with core models
- `LED(color, current=20mA)` - LEDs with electrical characteristics
- `Diode(type="silicon", voltage=600V)` - General-purpose diodes

#### Active Components
- `LM7805()` - 5V linear regulator with thermal model
- `LinearRegulatorBase(output_voltage, max_current)` - Generic regulator template

#### Infrastructure
- `Power(voltage, current)` - Power source modeling
- `Ground()` - Ground reference with impedance characteristics
- `TestPoint(style="pad")` - Test points for debugging

#### Pattern Library
- `voltage_divider(ratio, accuracy=5%)` - Resistor divider networks
- `rc_filter(cutoff_frequency, order=1)` - RC filter synthesis
- `power_on_reset(delay, threshold)` - Reset circuit generation

This comprehensive library system enables both standardized design patterns and project-specific customization while maintaining electrical accuracy and design intent capture.

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
    @VIN @RAW-> fuse: Fuse(1A).1;
    fuse.2 @PROTECTED-> tvs: TVSDiode(15V).1;
    tvs.2 -> @GND;
    
    // Input filtering capacitors on protected net
    @PROTECTED -> c_in1: ElectrolyticCap(100µF, 25V).+;
    @PROTECTED -> c_in2: Cap(0.1µF).1;
    c_in1.- -> @GND;
    c_in2.2 -> @GND;
    
    // Linear regulator circuit
    @PROTECTED -> reg: LM7805().IN;
    reg.GND -> @GND;
    reg.OUT @5V-> c_out1: ElectrolyticCap(10µF, 10V).+;
    
    // Output filtering
    @5V -> c_out2: Cap(0.1µF).1;
    c_out1.- -> @GND;
    c_out2.2 -> @GND;
    
    // LED power indicator
    @5V -> r_led: Res(330Ω).1;
    r_led.2 @LED_DRIVE-> led: LED(green).A;
    led.K -> @GND;
    
    // Test points for measurement
    @PROTECTED -> tp_vin: TestPoint().1;
    @5V -> tp_vout: TestPoint().1;
    @GND -> tp_gnd: TestPoint().1;
    
    // Output header
    @5V -> conn: Header_1x3.1;   // Power out
    @GND -> conn.2;               // Ground
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
   - Net references: `@PROTECTED`, `@5V`, `@VCC`, `@GND` always use @ prefix
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
    @USB_5V.enable();
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
    receiver: entity.UART_RX(domain=VCC_1V8);
    
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
mcu.VDD <- standard_decoupling <- @VCC;  // Uses 10µF + 0.1µF default
mcu.VDDA <- low_noise_decoupling <- @VCC;  // Uses 10µF + 1µF + 0.1µF + 10nF

// Location-aware decoupling
mcu.VDD <- local_decoupling(0.1µF within 2mm) + 
           bulk_decoupling(10µF within 10mm) <- @VCC;

// Pull-up/pull-down resistor banks
i2c_bus.SCL, i2c_bus.SDA <- pullup_bank(4.7kΩ) <- @VCC;
gpio_inputs[0:7] <- pulldown_bank(10kΩ) <- @GND;

// LED indicator arrays
status_pins[0:3] -> led_array(colors=[red,green,blue,yellow], current=2mA) -> @GND;

// Crystal oscillator with load caps
mcu.OSC_IN, mcu.OSC_OUT <-> crystal_circuit(25MHz, load=18pF);

// Voltage divider shortcuts
vref_2v5 <- voltage_divider(5V, ratio=0.5, accuracy=1%) <- @VCC;

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
6. **Entity Definition**: `entity Name(params) { implementation }`
7. **Constraint Declaration**: `constrain { placement, routing, timing }`

### 17.2 Operators

```bhdl
// Connection Operators
->    // Unidirectional connection (pin-to-pin)
<->   // Bidirectional connection  
<=>   // Interface connection
|>    // Flow operator (power/signal flows)

// Alternative ASCII Art Syntax (optional)
----> // Long arrow for emphasis
<---> // Long bidirectional  
<===> // Long interface connection
|===> // Flow with emphasis

// Grouping and Structure
[]    // Grouping/arrays
{}    // Code blocks
()    // Parameters

// Arithmetic and Logic
+, -, *, /, %     // Standard arithmetic
==, !=, <, >, <=, >= // Comparison
&&, ||, !         // Logic

// Special
.     // Pin access
@     // Net reference
:     // Component handle
```

#### Operator Visual Distinctions

To address visual similarity concerns, BHDL supports alternative syntax:

```bhdl
// Standard syntax
@VCC -> resistor.1 -> LED.A;
interface1 <=> interface2;
power |> regulation |> loads;

// Alternative with visual emphasis (optional)
@VCC ----> resistor.1 ----> LED.A;
interface1 <====> interface2;  
power |====> regulation |====> loads;

// Mixed usage is allowed
@VCC -> R1.1 ----> LED.A;  // Emphasize important connections
```

### 16.3 Keywords

```bhdl
// Core constructs
if else when generate for in entity constrain

// Declarations  
board system circuit interface power_domain

// Interface-specific
interface signal perspective require

// Types
signal power ground voltage current

// Modifiers
input output inout optional virtual extends implements

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
    
    @VCC -> led1: LED(red).A;  // Will detect overcurrent!
    led1.K -> @GND;
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
entity LED(color: string) {
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
    location: between @VCC and led1.A,
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

---

## 20. Formal Grammar

### 20.1 EBNF Grammar

The complete BHDL v2.0 grammar in Extended Backus-Naur Form (EBNF):

```ebnf
(* BHDL v2.0 Formal Grammar *)

(* Top-level constructs *)
bhdl_file = { import_statement | board_definition | entity_definition |
              system_definition | circuit_definition | interface_definition |
              constrain_statement } ;

(* Import statements *)
import_statement = "import" string_literal |
                   "import" "{" identifier_list "}" "from" string_literal ;

(* Board definition *)
board_definition = "board" identifier "{" board_body "}" ;
board_body = { power_declaration | ground_declaration | connection_statement |
               component_instantiation | attribute_statement | 
               constrain_statement | generate_statement | if_statement } ;

(* Entity definition *)
entity_definition = "entity" identifier [ parameter_list ] "{" entity_body "}" ;
entity_body = { pin_declaration | connection_statement | component_instantiation |
                generate_statement | if_statement | entity_instantiation } ;

(* Parameter list *)
parameter_list = "(" [ parameter { "," parameter } ] ")" ;
parameter = identifier ":" type_specification [ "=" default_value ] ;

(* Pin declaration *)
pin_declaration = "pin" identifier ":" ["virtual"] pin_type pin_direction [when_clause] ";" ;
pin_type = ( "signal" | "power" | "ground" ) [ pin_direction ] [ pin_attributes ] ;
pin_direction = "in" | "out" | "inout" ;

(* Power and ground declarations *)
power_declaration = "power" identifier "=" voltage_spec [ "@" current_spec ] ;
ground_declaration = "ground" identifier ;

(* Connection statements *)
connection_statement = connection_source connection_operator connection_target 
                      [ where_clause ] ;
connection_source = net_reference | component_pin | flow_expression ;
connection_target = net_reference | component_pin | flow_expression ;
connection_operator = "->" | "<->" | "<=>" | "|>" ;

(* Net references *)
net_reference = "@" identifier [ "@" identifier ] ;

(* Component instantiation *)
component_instantiation = [ identifier ":" ] component_type [ parameter_list ] 
                         [ pin_specification ] ;
component_type = identifier ;
pin_specification = "{" { pin_connection } "}" ;
pin_connection = pin_name ( "<-" | "->" ) ( net_reference | component_pin ) ;

(* Component pin reference *)
component_pin = identifier "." ( identifier | number ) ;

(* Flow expressions *)
flow_expression = flow_element { "|>" flow_element } ;
flow_element = identifier | function_call | flow_group ;
flow_group = "(" flow_expression ")" ;

(* Generate statements *)
generate_statement = "generate" "for" identifier "in" range_expression 
                    "{" { statement } "}" ;
range_expression = expression ".." expression ;

(* Conditional statements *)
if_statement = "if" "(" expression ")" "{" { statement } "}" 
              [ "else" "{" { statement } "}" ] ;

(* Constraint statements *)
constrain_statement = "constrain" constraint_type "{" { constraint_rule } "}" ;
constraint_type = "placement" | "routing" | "timing" | "power" ;
constraint_rule = identifier "{" { constraint_property } "}" ;

(* Where clauses *)
where_clause = "where" constraint_property_list ;
constraint_property_list = constraint_property { "," constraint_property } ;
constraint_property = identifier ( "=" | "<" | ">" | "<=" | ">=" ) expression ;

(* With blocks *)
with_statement = "with" constraint_type "(" constraint_property_list ")" 
                "{" { statement } "}" ;

(* Interface definitions *)
interface_definition = "interface" identifier [ parameter_list ] 
                      "{" { interface_element } "}" ;
interface_element = signal_declaration | require_statement | perspective_definition ;
signal_declaration = "signal" identifier ":" signal_type ;
require_statement = "require" requirement_expression ;
perspective_definition = "perspective" identifier "{" { signal_declaration } "}" ;

(* Expressions *)
expression = logical_or_expression ;
logical_or_expression = logical_and_expression { "||" logical_and_expression } ;
logical_and_expression = equality_expression { "&&" equality_expression } ;
equality_expression = relational_expression { ( "==" | "!=" ) relational_expression } ;
relational_expression = additive_expression { ( "<" | ">" | "<=" | ">=" ) additive_expression } ;
additive_expression = multiplicative_expression { ( "+" | "-" ) multiplicative_expression } ;
multiplicative_expression = unary_expression { ( "*" | "/" | "%" ) unary_expression } ;
unary_expression = [ ( "+" | "-" | "!" ) ] primary_expression ;
primary_expression = identifier | number | string_literal | 
                    electrical_value | "(" expression ")" | function_call ;

(* Function calls *)
function_call = identifier "(" [ argument_list ] ")" ;
argument_list = expression { "," expression } ;

(* Electrical values *)
electrical_value = number electrical_unit ;
electrical_unit = voltage_unit | current_unit | resistance_unit | 
                 capacitance_unit | inductance_unit | frequency_unit |
                 time_unit | temperature_unit | power_unit | percentage_unit ;

(* Units - Unicode and ASCII variants *)
voltage_unit = "V" | "mV" | "kV" | "µV" | "uV" | "nV" |
               "Vdc" | "Vac" | "Vrms" | "Vpp" ;
current_unit = "A" | "mA" | "µA" | "uA" | "nA" ;
resistance_unit = "Ω" | "Ohm" | "kΩ" | "kOhm" | "MΩ" | "MOhm" | 
                  "mΩ" | "mOhm" ;
capacitance_unit = "F" | "µF" | "uF" | "nF" | "pF" ;
inductance_unit = "H" | "mH" | "µH" | "uH" | "nH" ;
frequency_unit = "Hz" | "kHz" | "MHz" | "GHz" ;
time_unit = "s" | "ms" | "µs" | "us" | "ns" | "ps" ;
temperature_unit = "°C" | "degC" | "K" ;
power_unit = "W" | "mW" | "µW" | "uW" | "nW" | "kW" | "MW" ;
percentage_unit = "%" | "pct" ;

(* Lexical elements *)
identifier = letter { letter | digit | "_" } ;
number = [ sign ] ( integer | real ) ;
integer = digit { digit } ;
real = digit { digit } "." digit { digit } [ exponent ] ;
exponent = ( "e" | "E" ) [ sign ] digit { digit } ;
sign = "+" | "-" ;
string_literal = '"' { character } '"' ;
letter = "a" .. "z" | "A" .. "Z" ;
digit = "0" .. "9" ;
character = ? any character except '"' ? ;

(* Keywords *)
keywords = "board" | "entity" | "system" | "circuit" | "interface" |
          "power" | "ground" | "signal" | "pin" | "import" | "from" |
          "generate" | "for" | "in" | "if" | "else" | "when" |
          "constrain" | "where" | "with" | "require" | "perspective" |
          "attribute" | "alias" | "type" | "null" ;

(* Comments *)
single_line_comment = "//" { ? any character except newline ? } newline ;
multi_line_comment = "/*" { ? any character ? } "*/" ;
```

### 20.2 Operator Precedence

From highest to lowest precedence:

1. **Pin access**: `.` (left-to-right)
2. **Function calls**: `()` (left-to-right)  
3. **Unary operators**: `+`, `-`, `!` (right-to-left)
4. **Multiplicative**: `*`, `/`, `%` (left-to-right)
5. **Additive**: `+`, `-` (left-to-right)
6. **Relational**: `<`, `>`, `<=`, `>=` (left-to-right)
7. **Equality**: `==`, `!=` (left-to-right)
8. **Logical AND**: `&&` (left-to-right)
9. **Logical OR**: `||` (left-to-right)
10. **Connection**: `->`, `<->`, `<=>` (left-to-right)
11. **Flow**: `|>` (left-to-right)

### 20.3 Reserved Words

Complete list of reserved words in BHDL v2.0:

```
Keywords:
  alias, attribute, board, circuit, constrain, else, for, from,
  generate, ground, if, import, in, interface, entity, null,
  out, inout, perspective, pin, power, require, signal, system,
  type, when, where, with

Electrical Units (reserved when following numbers):
  A, F, H, Hz, K, Ohm, V, W, degC, mA, mF, mH, mOhm, mV, mW,
  nA, nF, nH, nV, nW, pF, pct, ps, uA, uF, uH, us, uV, uW,
  kOhm, kHz, kV, kW, MHz, MOhm, MV, MW, GHz, GOhm, GV, GW

Boolean Literals:
  true, false

Special Constants:
  auto, default
```

### 20.4 Grammar Notes

1. **Whitespace**: Spaces, tabs, newlines, and comments are ignored except within string literals
2. **Case Sensitivity**: All identifiers and keywords are case-sensitive
3. **Statement Termination**: Semicolons are optional but recommended for clarity
4. **Block Structure**: Braces `{}` define scope and group statements
5. **String Escaping**: Standard C-style escape sequences in string literals
6. **Unicode Support**: Full Unicode support in identifiers and string literals
7. **Comment Nesting**: Multi-line comments do not nest

---

## 21. Future Language Extensions

### 21.1 Planned Block Types

The following block types are planned for future BHDL versions to address real-world board design requirements. The syntax follows the existing `block_name { ... }` pattern, with intelligence implemented in the appropriate analyzer crates.

#### 21.1.1 Mechanical and Physical Constraints

```bhdl
mechanical {
    max_component_height = 5mm;        // Low-profile requirement
    keepout_area = rectangle(10mm, 10mm) at (50mm, 25mm);  // Screw hole
    connector_edge_clearance = 2mm;    // Board edge access requirements
    board_outline = rectangle(100mm, 80mm);
    mounting_holes = [
        circle(3mm) at (5mm, 5mm),
        circle(3mm) at (95mm, 75mm)
    ];
}

// Component placement with mechanical awareness
place connector at edge(top, clearance=2mm);
place heat_sink oriented_for airflow(direction=front_to_back);
```

#### 21.1.2 Thermal Management

```bhdl
thermal {
    ambient_temperature = 25°C;
    max_junction_temperature = 85°C;
    airflow_direction = bottom_to_top;
    airflow_velocity = 2m/s;
    
    heat_sources: [switching_regulator, cpu, power_amplifier];
    heat_sinks: copper_pour(area=100mm²) under switching_regulator;
    thermal_vias: array(3x3, spacing=2mm) under cpu;
}

// Thermal-aware placement constraints
place high_power_components away_from temperature_sensitive;
place thermal_sensitive_components near thermal_mass;
```

#### 21.1.3 EMI/EMC Design Rules

```bhdl
emc {
    target_class = "FCC_Class_B";
    frequency_range = 30MHz to 1GHz;
    
    // Automatic EMI mitigation strategies
    switching_signals require guard_traces;
    clock_frequencies above 10MHz require spread_spectrum;
    crystal_oscillators require guard_rings;
    power_planes require stitching_vias(spacing=10mm);
}

// EMI-aware design directives
sensitive_circuit |> shield(type=copper_pour, tie_to=@CHASSIS_GND);
power_input |> emi_filter(common_mode + differential_mode);
high_speed_signals require differential_pairs(impedance=100Ω);
```

#### 21.1.4 Manufacturing and Assembly

```bhdl
manufacturing {
    fab_house = "JLCPCB";
    pcb_thickness = 1.6mm;
    min_trace_width = 0.1mm;           // Fab capability limits
    min_via_size = 0.2mm;              // Drill capability
    min_component_spacing = 0.5mm;      // Pick-and-place constraints
    solder_paste_coverage = 80%;        // Assembly yield requirements
    
    assembly_sequence {
        place [R1, R2, C1] before U1;      // Small parts first
        U1 requires reflow_profile(lead_free);
        CONN1 requires wave_solder;         // Through-hole after SMT
    }
}

// Design for manufacturing validation
drc manufacturing {
    check min_trace_spacing;
    check component_courtyard_overlap;
    check solder_mask_clearance;
}
```

#### 21.1.5 Test and Debug Infrastructure

```bhdl
test_strategy {
    boundary_scan on [CPU, RAM, FLASH];      // JTAG chain definition
    in_circuit_test via test_points;         // ICT access points
    functional_test via debug_header;        // Runtime testing access
    
    coverage_requirements {
        power_rails = 100%;           // All power must be testable
        critical_signals = 95%;       // Key signals accessible
        digital_io = 80%;             // Most I/O brought out
    }
}

debug_access {
    jtag_chain: CPU.JTAG -> FPGA.JTAG -> @DEBUG_HEADER;
    serial_console: CPU.UART0 -> @CONSOLE_HEADER;
    scope_points: [@CLK_CPU, @VCC_CORE] -> test_points;
    
    // Programming interfaces
    swd_interface: CPU.SWDIO, CPU.SWCLK -> @PROG_HEADER;
}
```

#### 21.1.6 Advanced Power Integrity

```bhdl
power_integrity {
    target_impedance(@VCC_CORE) < 1mΩ @ 100MHz;
    target_impedance(@VCC_DDR) < 5mΩ @ 400MHz;
    
    decoupling_strategy = [
        bulk: 100µF near regulator,
        mid: 10µF per power_island(spacing<20mm),
        high_freq: 0.1µF per IC(distance<5mm),
        ultra_high: 1nF for switching_freq > 100MHz
    ];
    
    plane_resonance_damping via target_impedance_profile;
    via_inductance_modeling = enabled;
}

// Power delivery network optimization
pdn_analysis {
    switching_current_profile = sawtooth(10A, 100MHz);
    acceptable_ripple = 50mV;
    transient_response < 10µs;
}
```

#### 21.1.7 Supply Chain and Sourcing

```bhdl
sourcing {
    preferred_suppliers = [digikey, mouser, lcsc, arrow];
    avoid_single_source = true;
    target_bom_cost < $25;
    lifecycle_status = active_production;
    lead_time_max = 12_weeks;
    
    // Automatic alternative part suggestions
    enable_part_substitution for passive_components;
    require_approval for active_component_changes;
}

// Component selection with business constraints
components {
    R1: Res(4.7kΩ) {
        suppliers: minimum 2;
        cost_target: < $0.01;
        availability: > 10k_units;
        alternative_parts: ["RC0603FR-074K7L", "ERJ-3EKF4701V"];
    }
}
```

#### 21.1.8 Design Change Management

```bhdl
design_history {
    baseline_revision = "Rev_A";
    current_revision = "Rev_B";
    
    change_log = [
        change("ECO-001") {
            description = "Increase LED brightness per customer request";
            modified: R1.value from 1kΩ to 2kΩ;
            impact: current_increases_by(2x);
            approval: [electrical_lead, program_manager];
            effective_date = "2024-12-15";
        }
    ];
}

// Version-aware component specifications
R1: Res(value = if (revision >= "Rev_B") { 2kΩ } else { 1kΩ });
```

### 21.2 Proposed Operators

#### 21.2.1 Test Access Operators

The following operators would provide implicit test and debug access without requiring explicit component instantiation during the sketch phase:

```bhdl
// Test point operator: "make this signal testable"
@VCC_5V ->? tp_5v;                    // Auto-insert test point
critical_clock ->? scope_clk;         // Auto-insert scope access point

// Debug breakout operator: "bring this to a connector"
cpu.UART_TX =>? debug_console;        // Route to debug header
cpu.SWD_interface =>? prog_header;    // Programming interface

// Measurement operator: "I need to measure this"
switching_node ->@ current_sense;     // Insert current sensing
regulator_output ->@ voltage_monitor; // Insert voltage monitoring
```

**Implementation Note:** These operators would be syntactic sugar that expands to explicit test point instantiation and routing during the synthesis phase.

### 21.3 Implementation Timeline

#### Phase 1: Core Infrastructure (Next 6 months)
- `mechanical` block parser and basic DRC integration
- `test_strategy` block with test point auto-insertion
- Enhanced `thermal` block with temperature-aware placement

#### Phase 2: Manufacturing Integration (6-12 months)
- `manufacturing` block with DFM rule checking
- `sourcing` block with component database integration
- Advanced `power_integrity` analysis

#### Phase 3: Advanced Features (12-18 months)
- `emc` block with EMI simulation integration
- `design_history` with version control integration
- Test access operators (`->?`, `=>?`, `->@`)

#### Phase 4: Ecosystem Integration (18+ months)
- Full CAD tool integration for all block types
- Automated design rule checking across all domains
- Supply chain integration with real-time availability

### 21.4 Backward Compatibility

All future extensions will maintain backward compatibility with existing BHDL v2.0 syntax. New block types and operators will be additive enhancements that do not break existing designs.

Designs that don't use the new constructs will continue to work exactly as before, while designs that adopt them will gain additional analysis and optimization capabilities.

---

This completes the comprehensive BHDL v2.0 specification with roadmap for future enhancements. The language provides a complete framework for modern board design while maintaining simplicity through its seven core constructs and flow-based paradigm, with a clear path for addressing real-world production requirements.