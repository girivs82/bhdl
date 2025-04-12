# BHDL: Board Hardware Description Language Specification

## 1. Introduction

### 1.1 Purpose and Scope

BHDL (Board Hardware Description Language) is a domain-specific language for describing electronic circuit boards at multiple levels of abstraction. It bridges the gap between high-level functional specifications and low-level structural implementations, providing a unified language for the complete board design workflow.

BHDL addresses the unique challenges of analog and mixed-signal circuit design, power management, signal integrity, and physical design constraints that are not well-served by traditional digital HDLs. It aims to capture not only the electrical schematic intent but also key physical layout constraints and properties necessary for successful PCB implementation.

### 1.2 Design Philosophy

BHDL was designed with the following core principles:

1. **Intuitive for Board Designers**: Syntax should match how PCB designers think about connections and components
2. **Multi-level Abstraction**: Support high-level functional specifications, mid-level generic components, and low-level structural descriptions
3. **Visual Clarity**: Emphasize readability and visual representation over terse syntax
4. **Hierarchy and Modularity**: Enable reuse through hierarchical design
5. **Balance Power with Simplicity**: Powerful enough for complex designs without overwhelming complexity

### 1.3 Comparison to Existing HDLs

While VHDL and Verilog focus on digital logic with limited analog support, BHDL is specifically designed for board-level design with first-class support for:

- Analog and power domains
- Physical components and packages
- Signal integrity constraints
- Thermal considerations
- Manufacturing requirements

### 1.4 Connection-First Workflow

A key innovation of BHDL is its support for a connection-first design workflow. Unlike traditional HDLs that require rigid component declaration before connections, BHDL enables a more intuitive design process:

```
// Connection-first workflow example
// Step 1: Quickly sketch connections with minimal component details
VIN -> R1.1 -> C1.1;  // R1 and C1 implicitly created
C1.2 -> GND;
R1.2 -> LED1.A;       // LED1 implicitly created 
LED1.K -> GND;

// Step 2: Extract and formalize components when ready
component Resistor R1 {
  value: 1kOhm;  // Use Ohm
  tolerance: 5pct; // Use pct
}

component ElectrolyticCapacitor C1 {
  value: 10uF;   // Use uF
  voltage: 25Vdc;
}

component LED LED1 {
  color: red;
  current: 20mA;
}
```

This approach offers several advantages:

1. **Natural design flow** - Mirrors how engineers actually think about circuits
2. **Rapid prototyping** - Sketch connections quickly without formal component declarations
3. **Reduced context switching** - Fewer back-and-forth movements between declarations and connections
4. **Gradual refinement** - Start with a circuit sketch, then formalize incrementally
5. **Preserves separation** - Maintains clean separation between library components and connections in final designs

While the language syntax supports both traditional and connection-first workflows, the BHDL IDE tooling is specifically optimized for the connection-first approach, with features like component inference, property completion, and automated extraction.

### 1.5 Why a Text-Based Language for Board Design?

Many experienced board designers are highly proficient with graphical schematic capture tools. So, why introduce a text-based language like BHDL? BHDL is not intended to simply replicate graphical schematics in text, but rather to offer a different, complementary approach with distinct advantages, designed *specifically* with the board designer's workflow in mind:

1.  **Intuitive Capture at Design Speed:** Features like the **connection-first workflow** (Section 1.4), integrated **physical units** (Section 2.3), and **circuit functions** (Section 3.5) are designed to let you capture circuit ideas directly, often mirroring how you might initially sketch or think about connectivity, without getting bogged down in graphical layout or premature component definition. The goal is to describe the *intent* quickly and naturally.
2.  **Reducing Tool Friction:** While graphical tools excel at visualization, they can sometimes introduce friction in tasks like managing complex parameters, reusing circuits across projects, or performing detailed electrical rule checks early. BHDL aims to streamline these aspects.
3.  **Powerful Abstraction & Reuse:** Defining reusable **modules** (Section 3.2), **interfaces** (Section 3.4), **types** (Section 2.4), and **circuit functions** (Section 3.5) in text allows for powerful, parameterizable reuse that often goes beyond simple copy-paste in graphical tools. Standard circuit blocks can be instantiated with one line.
4.  **Enhanced Verification:** By capturing electrical properties directly in **types** and performing **unit-aware calculations** (Section 2.3.3), BHDL enables more robust automated checks for electrical compatibility and design rule violations early in the process.
5.  **Version Control & Collaboration:** Text-based designs integrate seamlessly with standard version control systems (like Git), making it easier to track changes, compare revisions, merge contributions, and collaborate effectively.
6.  **Parameterization & Automation:** Designs can be easily parameterized (Section 3.1), allowing for variations and automated generation of configurations or reports.
7.  **Bridging Schematic and Layout:** BHDL aims to integrate **physical design constraints** (Section 5) directly with the electrical description, creating a tighter link between schematic intent and PCB layout requirements.

**The Role of Tooling:** A text-based description does not mean abandoning visualization. BHDL is designed to be supported by IDEs and tools that can provide real-time rendering, interactive navigation, automated checks, and refactoring capabilities, effectively bridging the gap between the concise power of text and the clarity of visual representations.

BHDL offers a structured, verifiable, and reusable way to describe complex circuit boards, potentially enabling designers to express intricate designs more efficiently and reliably than with graphical tools alone for certain aspects of the workflow.

## 2. Language Structure

### 2.1 Basic Syntax

```
// Comments use C-style syntax
/* Multi-line comments
   are also supported */

// Top-level board definition
board BoardName {
  // Board contents
}

// Module definition
module ModuleName {
  // Module contents
}

// Component definition
component ComponentName {
  // Component specifications
}
```

**Reasoning**: The C-style syntax is familiar to most engineers, reducing the learning curve. The clear hierarchical structure mirrors how designers think about boards as collections of modules and components.

### 2.2 Naming Conventions

- Names are case-sensitive
- Valid names start with a letter and can contain letters, numbers, and underscores
- Reserved keywords cannot be used as names

**Reasoning**: Consistent with most modern programming languages for familiarity.

### 2.3 Basic Data Types

```
// Numeric types with units (using typable ASCII units)
voltage: 3.3Vdc;       // DC voltage
ac_voltage: 230Vac;    // AC voltage
rms_voltage: 0.894Vrms; // RMS voltage
current: 100mA;
resistance: 4.7kOhm;    // Use kOhm instead of kΩ
capacitance: 10uF;      // Use uF instead of µF
inductance: 10uH;       // Use uH instead of µH
frequency: 16MHz;
time: 10ns;
temperature: 85degC;    // Use degC instead of °C
duty_cycle: 50pct;      // Use pct instead of %

// Boolean type
enable: true;
active_low: false;

// String type
part_number: "LM317T";

// Enumerations
package: enum {SOIC-8, TSSOP-16, QFN-32};

// Arrays
capacitors: [10uF, 1uF, 100nF]; // Use uF/nF

// Ranges
input_voltage: 5Vdc to 24Vdc;
operating_temp: -40degC to 85degC;
tolerance: 5pct; // Use pct

// Enum Value Literal (distinct from declaration)
state_value: StateType'Active; // Example: Assigning an enum value
```

**Reasoning**: Including units directly in the syntax prevents unit conversion errors and makes the code more readable. Using standard ASCII characters (e.g., `Ohm`, `uF`, `degC`, `pct`, `Vdc`/`Vac`) enhances typability and portability across different systems. Advanced types like ranges directly express design constraints.

### 2.3.1 Component Specifications and Ratings

When specifying component ratings (such as voltage ratings for capacitors), values are assumed to be minimum required values unless explicitly stated otherwise.

```
// Capacitor with 16V minimum DC voltage rating
component Capacitor C1 {
  value: 100nF;
  voltage: 16Vdc;  // Minimum voltage rating (DC assumed if not specified)
}

// Inline component with voltage rating
[C2, Capacitor, 100nF, 25Vdc]  // Minimum 25Vdc rating
```

This simplifies the syntax by omitting explicit comparators (like `>=`) when they can be inferred from context. The rating values represent the minimum requirements that must be met or exceeded by the physical components.

Other component ratings follow the same pattern where practical:

```
// Current rating is minimum required
component Inductor L1 {
  value: 10uH;
  current: 1A;       // Minimum DC current rating
}

// Power rating is minimum required
component Resistor R1 {
  value: 10kOhm;
  power: 0.25W;      // Minimum power rating
}
```

When a maximum constraint is needed, it should be explicitly specified with the `<` or `<=` operator. Similarly, minimums can be explicitly denoted with `>` or `>=` if needed for clarity, though often implied by context for ratings.

```
// Maximum values use explicit operator
input_offset: <5mV;    // Maximum input offset
noise: <10nV/sqrt(Hz); // Maximum noise (example complex unit)
ripple: <50mVpp;       // Maximum peak-to-peak ripple

// Explicit minimum (less common for ratings, useful for specs)
output_drive_strength: >10mA;
```

### 2.3.2 Unit Definitions and Syntax

BHDL recognizes standard SI units and common electronics units using typable ASCII symbols. Prefixes are placed immediately before the unit symbol.

**Recognized Units (Examples):**

| Unit        | Symbol(s)     | Description         |
|-------------|---------------|---------------------|
| Volt        | `V`, `Vdc`, `Vac`, `Vrms`, `Vpp` | Voltage (Generic, DC, AC, RMS, Peak-to-Peak) |
| Ampere      | `A`           | Current             |
| Ohm         | `Ohm`         | Resistance          |
| Farad       | `F`           | Capacitance         |
| Henry       | `H`           | Inductance          |
| Watt        | `W`           | Power               |
| Hertz       | `Hz`          | Frequency           |
| Second      | `s`           | Time                |
| Celsius     | `degC`        | Temperature         |
| Percent     | `pct`         | Percentage          |
| Siemens     | `S`           | Conductance         |
| Decibel     | `dB`          | Ratio (Logarithmic) |
| (Unitless)  | `<none>`      | Scalar quantity     |
| Bit         | `bit`         | Data size           |
| Baud        | `Bd`          | Symbol rate         |
| Candela     | `cd`          | Luminous Intensity  |
| Lumen       | `lm`          | Luminous Flux       |
| Lux         | `lx`          | Illuminance         |

**Recognized Prefixes:**

| Prefix | Symbol | Factor    |
|--------|--------|-----------|
| Tera   | `T`    | 10<sup>12</sup> |
| Giga   | `G`    | 10<sup>9</sup>  |
| Mega   | `M`    | 10<sup>6</sup>  |
| Kilo   | `k`    | 10<sup>3</sup>  |
| Milli  | `m`    | 10<sup>-3</sup> |
| Micro  | `u`    | 10<sup>-6</sup> |
| Nano   | `n`    | 10<sup>-9</sup> |
| Pico   | `p`    | 10<sup>-12</sup>|
| Femto  | `f`    | 10<sup>-15</sup>|

**Syntax:** A numeric value followed by an optional prefix and a unit symbol (e.g., `10`, `3.3Vdc`, `4.7kOhm`, `100nF`, `50pct`). Spaces are not allowed between the prefix and the unit. Complex units like `nV/sqrt(Hz)` may also be supported by tooling.

### 2.3.3 Arithmetic Operations with Units

BHDL supports standard arithmetic operations directly on values with units, enforcing unit consistency.

1.  **Addition (`+`) and Subtraction (`-`):**
    *   Operands **must** have compatible units (e.g., `V` and `mV`, `kOhm` and `Ohm`). Automatic prefix scaling is applied.
    *   Adding or subtracting incompatible units (e.g., `V + A`) results in an error.
    *   The result retains the (scaled) unit of the operands.
    *   `5Vdc + 500mVdc = 5.5Vdc`
    *   `1kOhm - 10Ohm = 990Ohm`

2.  **Multiplication (`*`) and Division (`/`):**
    *   Operations are performed on both the numeric values and the units.
    *   Units combine according to standard physics rules (e.g., `V = A * Ohm`, `W = V * A`, `A = V / Ohm`).
    *   Prefixes are handled automatically based on the operation.
    *   `2mA * 1kOhm = 2V`
    *   `5V / 1kOhm = 5mA`
    *   `10V / 5V = 2` (Result is unitless)
    *   `10 * 5mA = 50mA` (Multiplying by a unitless number retains the unit)
    *   `10MHz * 5ns = 0.05` (Result is unitless: 10<sup>7</sup> Hz * 5 * 10<sup>-9</sup> s = 50 * 10<sup>-2</sup>)

3.  **Comparisons (`<`, `>`, `<=`, `>=`):**
    *   Operands must have compatible units.
    *   `10kOhm > 500Ohm` (evaluates to `true`)
    *   `5Vdc < 1mA` (results in an error)

4.  **Type System Integration:** The type system (Section 2.4) utilizes these units extensively for defining electrical characteristics and performing compatibility checks during connections.

**Reasoning:** Native unit arithmetic makes calculations within the design description type-safe, unit-aware, less error-prone, and significantly more readable than external functions or string manipulation. It allows designers to express relationships like Ohm's Law or power calculations directly and safely.

### 2.4 Type System

BHDL provides a flexible type system that models real-world electrical characteristics. The type system has two layers:

1. **Core Base Types**: Fundamental types representing basic electrical concepts
2. **Extended Types**: Domain-specific types built on core types using `typedef`

#### 2.4.1 Core Base Types

BHDL has three core base types for pins:

- **signal**: Represents information-carrying connections (digital, analog, or mixed)
- **power**: Represents power distribution connections
- **ground**: Represents ground or return path connections

These core types reflect the fundamental categories of connections in electronic circuits. When defining custom types using `typedef` (see Section 2.4.2), the mandatory `type` field **must** be one of these three core base types. Additional classification (e.g., digital, analog) can be specified using other properties like `domain`.

```
// Core base type examples 
pins {
  DATA: in signal;       // A generic signal input
  VCC: in power;         // A power supply input
  GND: ground;           // A ground connection
}
```

Each base type carries a different set of implicit electrical characteristics and validation rules:

- **signal**: Focus on information content, impedance matching, and signal integrity
- **power**: Focus on voltage level, current capacity, regulation, and noise characteristics
- **ground**: Focus on return paths, ground loops, and reference potentials

**Reasoning**: By limiting the core types to these three fundamental categories, BHDL maintains simplicity while still covering the essential electrical roles. Pin direction (in, out, inout) is orthogonal to the type and specified separately.

#### 2.4.2 Extending Types with typedef

The core types are extended using the `typedef` mechanism to create domain-specific types with rich electrical characteristics:

```
// Type definition syntax
typedef <TypeName> {
  type: <BaseType>;          // Mandatory: signal, power, or ground
  domain: <DomainName>;       // Optional: e.g., digital, analog, clock, differential
  // --- Common Electrical Properties --- 
  voltage_high: voltage;      // For digital types
  voltage_low: voltage;
  threshold_high: voltage;
  threshold_low: voltage;
  impedance: resistance;
  bandwidth: frequency_range;
  // --- Properties for Automation --- 
  is_open_drain: boolean = false; // Optional: Indicates open-drain/collector output
  default_pullup_resistance: resistance; // Optional: Default pull-up for open-drain
  // ... other properties like rise_time, leakage, etc. ...
}

// Example: 3.3V CMOS type
typedef cmos_3v3 {
  type: signal; domain: digital;
  voltage_high: 3.3Vdc; voltage_low: 0Vdc;
  threshold_high: 2.0Vdc; threshold_low: 0.8Vdc;
  rise_time: <10ns; input_leakage: <1uA;
}

// Example: I2C signal type defining open-drain and default pull-up
typedef i2c_signal_3v3 {
  type: signal; domain: digital;
  voltage_high: 3.3Vdc; voltage_low: 0Vdc;
  threshold_high: 2.0Vdc; threshold_low: 0.8Vdc;
  is_open_drain: true;
  default_pullup_resistance: 4.7kOhm; // Default pull-up value for automation
  // Could add max capacitance, speed ratings etc.
}

// Type usage
pins {
  CLK: in signal(cmos_3v3);   // A 3.3V CMOS clock input
  SDA: inout signal(i2c_signal_3v3); // An I2C signal pin
}
```

**Reasoning**: The `typedef` mechanism provides a general-purpose way to define structured types without introducing specialized keywords for each domain. This keeps the language simpler while enabling rich type checking and domain-specific parameters.

#### 2.4.3 Pin Directions and Types

Pin direction is orthogonal to the pin type and specified separately. BHDL supports three pin directions:

- **in**: Input to the component/module/board
- **out**: Output from the component/module/board
- **inout**: Bidirectional pin

Directions are combined with types to fully specify a pin's electrical characteristics:

```
pins {
  // Different directions with the same signal type
  DATA_IN: in signal(cmos_3v3);
  DATA_OUT: out signal(cmos_3v3);
  DATA_BIDIR: inout signal(cmos_3v3);
  
  // Differential signals (use inout and rely on type properties)
  DIFF_P: inout signal(lvds); // Changed from bidir to inout
  DIFF_N: inout signal(lvds); // Changed from bidir to inout
  
  // Power and ground
  VDD: in power(lv_digital_power);
  VOUT: out power(lv_digital);
  GND: ground;
}
```

**Component Pin Direction Semantics**:

For components representing physical parts, pin directions have these meanings:
- **in**: Receives a signal or power (input to the component)
- **out**: Generates a signal or power (output from the component)
- **inout**: Can both receive and generate signals/power (bidirectional)

**Board/Module Port Direction Semantics**:

For boards and modules, direction is from the perspective of the board/module:
- **in**: Signal/power entering the board/module
- **out**: Signal/power leaving the board/module
- **inout**: Signal that can flow in either direction

**Passive Components**:

For passive components like resistors or capacitors, all pins are typically `inout` since they don't have a defined directionality:

```
component Resistor R1 {
  pins {
    1: inout signal;
    2: inout signal;
  }
}
```

**Reasoning**: Separating pin direction from pin type creates a more flexible system that handles all kinds of components while maintaining strict type checking. The direction is about signal flow, while the type is about electrical characteristics.

#### 2.4.4 Standard Library Types

The BHDL standard library includes predefined types for common domains:
```
// Digital signal types in libraries/types.bhdl
typedef ttl {
  type: signal;
  domain: digital;
  voltage_high: 5.0Vdc;
  voltage_low: 0Vdc;
  threshold_high: 2.0Vdc;
  threshold_low: 0.8Vdc;
  rise_time: <22ns;
  fanout: 10;
}

typedef cmos_5v {
  type: signal;
  domain: digital;
  voltage_high: 5.0Vdc;
  voltage_low: 0Vdc;
  threshold_high: 3.5Vdc;
  threshold_low: 1.5Vdc;
  rise_time: <20ns;
  input_leakage: <1uA;
}

typedef cmos_3v3 {
  type: signal;
  domain: digital;
  voltage_high: 3.3Vdc;
  voltage_low: 0Vdc;
  threshold_high: 2.0Vdc;
  threshold_low: 0.8Vdc;
  rise_time: <10ns;
  input_leakage: <1uA;
}

typedef lvcmos_2v5 {
  type: signal;
  domain: digital;
  voltage_high: 2.5Vdc;
  voltage_low: 0Vdc;
  threshold_high: 1.7Vdc;
  threshold_low: 0.7Vdc;
  rise_time: <8ns;
  input_leakage: <1uA;
}

typedef lvcmos_1v8 {
  type: signal;
  domain: digital;
  voltage_high: 1.8Vdc;
  voltage_low: 0Vdc;
  threshold_high: 1.2Vdc;
  threshold_low: 0.6Vdc;
  rise_time: <5ns;
  input_leakage: <1uA;
}

typedef lvcmos_1v2 {
  type: signal;
  domain: digital;
  voltage_high: 1.2Vdc;
  voltage_low: 0Vdc;
  threshold_high: 0.8Vdc;
  threshold_low: 0.4Vdc;
  rise_time: <3ns;
  input_leakage: <1uA;
}

typedef lvds {
  type: signal;
  domain: differential; // More specific than just digital
  voltage_high: 1.4Vdc; // Typical high level
  voltage_low: 1.0Vdc;  // Typical low level
  differential: true;
  termination: 100Ohm;
  common_mode: 1.2Vdc;
  swing: 350mV;
  rise_time: <300ps;
}
```

**Audio Signal Types**:
```
typedef line_level {
  type: signal;
  domain: analog;
  voltage: 0.894Vrms;       // Consumer line level (-10dBV)
  impedance: 10kOhm;
  bandwidth: 20Hz to 20kHz;
}

typedef pro_line_level {
  type: signal;
  domain: analog;
  voltage: 1.228Vrms;       // Professional line level (+4dBu)
  impedance: 600Ohm;
  bandwidth: 20Hz to 20kHz;
}

typedef mic_level {
  type: signal;
  domain: analog;
  voltage: 2mVrms to 100mVrms;
  impedance: 150Ohm to 600Ohm;
  bandwidth: 20Hz to 20kHz;
}
```

**Power Types**:
```
typedef lv_digital_power { // Renamed for clarity
  type: power;
  voltage: 3.3Vdc;
  tolerance: 5pct; // Use pct
  ripple: <50mVpp; // Explicitly Peak-to-Peak
}

typedef lv_analog_power { // Renamed for clarity
  type: power;
  voltage: 5Vdc;
  tolerance: 2pct; // Use pct
  ripple: <10mVpp; // Explicitly Peak-to-Peak
  noise: <100uVrms in 20Hz to 20kHz; // Use uVrms
}
```

**Clock Types**:
```
typedef system_clock {
  type: signal;
  domain: clock;
  frequency: 100MHz;
  jitter: <100ps;
  duty_cycle: 50pct +/- 5pct; // Use pct and +/- syntax
  // Assuming clock uses standard logic levels, e.g., cmos_3v3
  // These can be specified directly or inherited from another type
  voltage_high: 3.3Vdc;
  voltage_low: 0Vdc;
}

typedef crystal_clock {
  type: signal;
  domain: clock;
  differential: true;
  amplitude: 1Vpeak;
  frequency: required; // Frequency must be specified at use time
}
```

**Thermal Types**:
```
// Note: Thermal properties might be better suited as component/board constraints
// rather than pin types. Defining them here without a core 'type' field.
property_set commercial_thermal {
  operating_temperature: 0degC to 70degC;
  storage_temperature: -40degC to 85degC;
}

property_set industrial_thermal {
  operating_temperature: -40degC to 85degC;
  storage_temperature: -55degC to 125degC;
}
```

#### 2.4.5 User-Defined Types

Users can define custom types for domain-specific needs:

```
// In myproject/custom_types.bhdl
typedef automotive_signal {
  type: signal;
  domain: custom; // Or perhaps 'analog' or 'digital' depending on use
  voltage: 0Vdc to 5Vdc;
  impedance: 100Ohm;
  rise_time: <100ns;
  esd_protection: 15kV;
  reverse_voltage_protection: true;
}

typedef medical_power {
  type: power;
  voltage: 12Vdc;
  isolation: 4kV;
  leakage_current: <10uA;
  certifications: ["IEC 60601-1", "UL 60601-1"];
}
```

#### 2.4.6 Type Inheritance and Extension

Types can inherit and extend other types:

```
// Extending a base type
typedef high_quality_line_level extends line_level {
  thd: <0.001pct;           // Total harmonic distortion (use pct)
  snr: >100dB;            // Signal-to-noise ratio
  crosstalk: <-80dB;      // Channel crosstalk
}

// Creating a variant
typedef battery_power extends lv_digital_power { // Using renamed type
  voltage: 3.7Vdc;        // Li-ion nominal voltage
  voltage_range: 3.0Vdc to 4.2Vdc;
  protection: {
    overcurrent: true;
    overvoltage: true;
    undervoltage: true;
  };
}
```

#### 2.4.7 Using Type Definitions

Types are used throughout BHDL to provide rich, domain-specific parameters:

```
// Importing types
import Types.{cmos_3v3, lvds, lv_digital_power}; // Updated power type name

// Using types in a board definition
board DigitalInterface {
  ports {
    // Digital signal pins with specific electrical characteristics
    DATA[0:7]: in signal(cmos_3v3);
    SERIAL_TX: out signal(cmos_3v3);
    SERIAL_RX: in signal(cmos_3v3);
    
    // Differential signaling
    LVDS_P: out signal(lvds);
    LVDS_N: out signal(lvds);
    
    // Power pins
    VDD: in power(lv_digital_power); // Updated power type name
    GND: ground;
  }
  
  // Component instantiations
  components {
    FPGA U1 {
      io_standard: cmos_3v3;  // Using type as a parameter
    }
  }
  
  // Type-aware design rules
  design_rules {
    // LVDS requires termination resistors
    rule lvds_termination {
      when (pin_type.base_type == signal && pin_type.domain == differential && pin_type.differential == true && direction == in) { // Example of checking properties
        require: termination(differential, 100Ohm +/- 10pct); // Use new units
      }
    }
    
    // High-speed CMOS signals need termination
    rule cmos_termination {
      when (pin_type.base_type == signal && pin_type.domain == digital && pin_type.voltage_high == 3.3Vdc && trace_length > 2in) { // Example
        require: termination(series, 33Ohm +/- 10pct); // Use new units
      }
    }
  }
}
```

#### 2.4.8 Benefits of the Type System

The BHDL type system provides several key benefits:

1. **Electrical Correctness**: Type checking can verify compatible signal levels, preventing electrical mismatches
2. **Design Intent**: Types document the intended electrical characteristics of signals
3. **Constraint Generation**: Types can automatically generate appropriate design rules (termination, matching, etc.)
4. **Component Selection**: Types can guide automated component selection based on electrical requirements
5. **Knowledge Capture**: Domain expertise can be encoded into types and shared across projects
6. **Level Shifting**: Tools can automatically insert level shifters when connecting incompatible types
7. **Simulation Models**: Types provide the necessary information to generate appropriate simulation models

**Reasoning**: By encoding electrical characteristics directly in the type system, BHDL enables automated checking and optimization that would otherwise require manual review. This makes designs more robust and less error-prone.

### 2.5 Type Definitions

BHDL provides a flexible type definition mechanism that allows both standard library types and user-defined types. This enables type-safe design with domain-specific parameter sets.

```
// Type definition syntax
typedef line_level {
  type: signal;          // Base type category
  voltage: 0.894Vrms;    // Consumer line level (typical -10dBV)
  impedance: 10kOhm;
  bandwidth: 20Hz to 20kHz;
}

// Type usage
ports {
  AUDIO_IN: in signal(line_level);
}
```

**Reasoning**: The `typedef` mechanism provides a general-purpose way to define structured types without introducing specialized keywords for each domain. This keeps the language simpler while enabling rich type checking and domain-specific parameters.

#### 2.5.1 Standard Library Types

Standard types are defined in the BHDL standard library and can be imported:

```
// In libraries/types.bhdl
// Audio signal types
typedef line_level {
  type: signal;
  voltage: 0.894Vrms;       // Consumer line level (-10dBV)
  impedance: 10kOhm;
  bandwidth: 20Hz to 20kHz;
}

typedef pro_line_level {
  type: signal;
  voltage: 1.228Vrms;       // Professional line level (+4dBu)
  impedance: 600Ohm;
  bandwidth: 20Hz to 20kHz;
}

typedef mic_level {
  type: signal;
  voltage: 2mVrms to 100mVrms;
  impedance: 150Ohm to 600Ohm;
  bandwidth: 20Hz to 20kHz;
}

// Power types
typedef lv_digital_power {
  type: power;
  voltage: 3.3Vdc;
  tolerance: 5pct;
  ripple: <50mVpp;
}

typedef lv_analog_power {
  type: power;
  voltage: 5Vdc;
  tolerance: 2pct;
  ripple: <10mVpp;
  noise: <100uVrms in 20Hz to 20kHz;
}

// Digital interface types
typedef lvds {
  type: digital;
  voltage_high: 1.4Vdc;
  voltage_low: 1.0Vdc;
  differential: true;
  termination: 100Ohm;
  rise_time: <300ps;
}

typedef cmos_3v3 {
  type: digital;
  voltage_high: 3.3Vdc;
  voltage_low: 0Vdc;
  threshold_high: >2.0Vdc;
  threshold_low: <0.8Vdc;
  rise_time: <5ns;
}

// Clock types
typedef system_clock {
  type: clock;
  frequency: 100MHz;
  jitter: <100ps;
  duty_cycle: 50pct +/- 5pct;
}

// Thermal types
typedef commercial_thermal {
  type: thermal;
  operating_temperature: 0degC to 70degC;
  storage_temperature: -40degC to 85degC;
}
```

#### 2.5.2 User-Defined Types

Users can define custom types for domain-specific needs:

```
// In myproject/custom_types.bhdl
typedef automotive_signal {
  type: signal;
  domain: custom; // Or perhaps 'analog' or 'digital' depending on use
  voltage: 0Vdc to 5Vdc;
  impedance: 100Ohm;
  rise_time: <100ns;
  esd_protection: 15kV;
  reverse_voltage_protection: true;
}

typedef medical_power {
  type: power;
  voltage: 12Vdc;
  isolation: 4kV;
  leakage_current: <10uA;
  certifications: ["IEC 60601-1", "UL 60601-1"];
}

typedef adc_interface {
  type: analog;
  voltage_range: 0Vdc to 3.3Vdc;
  resolution: 12bit;
  sampling_rate: 1MSPS;
  input_impedance: >1MOhm;
}
```

#### 2.5.3 Type Inheritance and Extension

Types can inherit and extend other types:

```
// Extending a base type
typedef high_quality_line_level extends line_level {
  thd: <0.001pct;           // Total harmonic distortion (use pct)
  snr: >100dB;            // Signal-to-noise ratio
  crosstalk: <-80dB;      // Channel crosstalk
}

// Creating a variant
typedef battery_power extends lv_digital_power { // Using renamed type
  voltage: 3.7Vdc;        // Li-ion nominal voltage
  voltage_range: 3.0Vdc to 4.2Vdc;
  protection: {
    overcurrent: true;
    overvoltage: true;
    undervoltage: true;
  };
}
```

#### 2.5.4 Using Type Definitions

Types are used throughout BHDL to provide rich, domain-specific parameters:

```
// Importing types
import libraries.types.{line_level, lv_digital_power, cmos_3v3, system_clock};
import myproject.custom_types.{automotive_signal};

// Using types in a module definition
module AudioInterface {
  ports {
    // Signal types
    AUDIO_IN_L: in signal(line_level);
    AUDIO_IN_R: in signal(line_level);
    AUDIO_OUT: out signal(pro_line_level);
    
    // Power types
    VDD: in power(lv_analog_power);
    VDDA: in power(lv_analog_power);
    
    // Digital interface types
    SPI_MOSI: in digital(cmos_3v3);
    SPI_MISO: out digital(cmos_3v3);
    SPI_SCK: in digital(cmos_3v3);
    
    // Clock type
    CLK: in clock(system_clock);
  }
  
  // Internal components
  components {
    component ADC U1 {
      input_type: line_level;      // Using type as a parameter
      reference_voltage: 5Vdc;
      resolution: 24bit;
    }
    
    component OpAmp U2 {
      supply_voltage: lv_analog_power;   // Using type as a parameter
      gain_bandwidth: 10MHz;
      slew_rate: 10V/µs;
    }
  }
}

// Using types in connection constraints
connection AUDIO_IN to U1.IN {
  match_impedance: true;           // Ensures impedance matching based on types
  max_length: 50mm;
  shield: required;
}

// Using types in component selection
component_selection {
  filter: {
    input_type: line_level;        // Filter by type compatibility
    operating_temperature: commercial_thermal;
  }
}
```

**Reasoning**: This comprehensive type system enables design intent to be expressed at a high level while providing detailed electrical characteristics for validation and synthesis. It allows component selection and connection checking based on domain-specific requirements, reducing errors and improving design automation.

### 2.6 Generative Constructs (`generate for`)

To handle repetitive structures like large buses or arrays of components efficiently, BHDL provides a `generate for` loop. This allows pins, connections, or components to be created programmatically based on parameters or ranges, significantly reducing boilerplate code and the potential for manual errors.

**Syntax:**
```bhdl
generate for <variable> in <range_or_list> {
  // Pin definitions, component instantiations, or connection statements
  // The <variable> can be used within the generated statements (e.g., in names or indices)
}
```
*   `<variable>`: The loop variable (e.g., `i`, `byte_index`).
*   `<range_or_list>`: Defines the iteration space. Common forms include:
    *   `start_value to end_value`: Inclusive range (e.g., `0 to 63`).
    *   `start_value upto end_value`: Exclusive range (e.g., `0 upto 64`, equivalent to `0 to 63`).
    *   An existing list or array variable.
    *   A range with a step (Syntax TBD, e.g., `0 to 63 step 8`).

**Usage within `pins` block:**
```bhdl
// Example: Generating DDR Data/Strobe pins based on a parameter
component DDR_PHY {
  parameters {
    data_width: integer = 64;
  }
  pins {
    // Generate DQ pins using the data_width parameter
    generate for i in 0 to data_width-1 {
      DQ[i]: inout signal(ddr_dq_type); // Creates DQ[0], DQ[1], ..., DQ[63]
    }
    // Generate DQS pairs using the calculated num_bytes parameter
    generate for i in 0 to num_bytes-1 {
      DQS_P[i]: inout signal(ddr_dqs_type); // Creates DQS_P[0]...DQS_P[7]
      DQS_N[i]: inout signal(ddr_dqs_type); // Creates DQS_N[0]...DQS_N[7]
    }
    // ... other pins (ADDR, CMD, CLK etc.) ...
  }
  // ... component details ...
}
```

**Usage within `connections` block:**
```bhdl
// Example: Connecting a 64-bit data bus directly bit-by-bit
connections {
  // Assume CPU and PHY components have DQ[0..63] pins
  generate for i in 0 to 63 {
    CPU.DQ[i] -> PHY.DQ[i];
  }
}
```

**Usage within `components` block:**
```bhdl
// Example: Instantiating an array of termination resistors
parameters { num_leds: integer = 8; }
components {
  generate for i in 0 to num_leds-1 {
    Resistor R_LED[i] { value: 330Ohm; package: "0603"; }
  }
  // ... other components ...
}
```

### 2.7 Bus Notation and Slicing

Signals or ports declared as arrays (often using `generate for` or direct array syntax like `DATA[0:7]`) represent buses. BHDL provides standard notation for accessing individual elements or sub-ranges (slices) of these buses.

*   **Array Declaration:**
    *   Using `generate for` (see Section 2.6) is common for large parameterized buses.
    *   Direct declaration: `DATA[0:7]: out signal(cmos_3v3);` defines an 8-bit bus.

*   **Individual Element Access:** `BUS_NAME[index]`
    *   Accesses a single element of the array (e.g., `DATA[7]`, `DQ[0]`).
    *   The index must be within the defined bounds of the array.

*   **Slicing:** `BUS_NAME[high_index : low_index]`
    *   Selects a contiguous sub-range (slice) of the bus, from `low_index` up to `high_index`, inclusive.
    *   The indices define the range within the original bus numbering.
    *   Example: `DATA[15:8]` selects 8 bits (bits 8, 9, ..., 15) from the `DATA` bus.
    *   When used in connections, the width of the source slice must match the width of the target slice or port.

**Example: Connecting Slices**
```bhdl
// Assume MCU has DATA_OUT[31:0] and Peripheral has DATA_IN[15:0]
connections {
  // Connect lower 16 bits of MCU output to peripheral input
  MCU.DATA_OUT[15:0] -> Peripheral.DATA_IN[15:0];

  // Connect upper 8 bits of MCU output somewhere else
  MCU.DATA_OUT[31:24] -> DebugConnector.PINS[7:0];
}
```

**Example: Byte Swizzling Connection using `generate for` and slicing**

This powerful combination allows for complex bus manipulations, such as reversing byte lanes between two components.

```bhdl
// Connect a 64-bit bus (8 bytes) with byte lanes reversed
// MCU.DATA[7:0]   -> PHY.DATA[63:56]
// MCU.DATA[15:8]  -> PHY.DATA[55:48]
// ...
// MCU.DATA[63:56] -> PHY.DATA[7:0]
parameters {
  data_width: integer = 64;
  num_bytes: integer = data_width / 8;
}
connections {
  generate for byte_idx in 0 to num_bytes-1 {
    // Calculate indices for MCU slice (standard byte order)
    local mcu_high = (byte_idx + 1) * 8 - 1; // e.g., 7, 15, ..., 63
    local mcu_low = byte_idx * 8;           // e.g., 0, 8, ..., 56

    // Calculate indices for PHY slice (reversed byte order)
    local phy_byte_num = num_bytes - 1 - byte_idx; // e.g., 7, 6, ..., 0
    local phy_high = (phy_byte_num + 1) * 8 - 1;   // e.g., 63, 55, ..., 7
    local phy_low = phy_byte_num * 8;             // e.g., 56, 48, ..., 0

    // Connect the corresponding slices
    MCU.DATA[mcu_high : mcu_low] -> PHY.DATA[phy_high : phy_low];

    // --- Optional Bit Swizzling ---
    // If needed, a nested loop and a bit-level map could connect individual bits here instead
    // of connecting the 8-bit slices directly.
  }
}
```

### 2.8 Importing Libraries (`import`)

To promote reuse and modularity, BHDL supports importing definitions from other files or libraries using the `import` statement. This allows designs to leverage standard component definitions, types, interfaces, functions, net classes, via styles, and other reusable elements.

**Syntax:**
```bhdl
import <LibraryPath> { <Symbol1>, <Symbol2>, ... };
import <LibraryPath>.*; // Import all exported symbols (use with caution)
```
*   `<LibraryPath>`: A path to the BHDL file or library module, typically relative to the project source or configured library paths. Dot notation is often used for hierarchical libraries (e.g., `StandardLibrary.Components`, `CompanyStandards.DRC`). The exact path resolution mechanism depends on the tool implementation.
*   `<SymbolN>`: The specific names (components, types, functions, etc.) to be imported into the current scope.
*   `.*`: Imports all symbols exported by the target library. This can potentially pollute the namespace and is generally recommended only for foundational libraries or when explicitly desired.

**Scope:** Imported symbols are brought into the scope where the `import` statement appears.

**Example:**
```bhdl
// Import specific types and components from standard libraries
import StandardLibrary.Types.{cmos_3v3, lv_digital_power};
import StandardLibrary.Components.{Resistor, Capacitor};

// Import net class and via style definitions from a company library
import CompanyStandards.DRC.{PowerNetClass, StandardVia};

// Import a specific circuit function
import StandardLibrary.CircuitPatterns.non_inverting_amplifier;

board MyBoard {
  // Use imported definitions
  default_design_rules {
    default_via_style: "StandardVia"; // Reference imported style
  }
  ports { SIGNAL_IN: in signal(cmos_3v3); }
  components { Resistor R1 { value: 10kOhm; } }
  connections { ... }
  constrain (VDD) { net_class: "PowerNetClass"; } // Reference imported class
}
```

Libraries themselves typically define what symbols they `export` for external use (Syntax for `export` TBD or assumed implicit based on top-level definitions in the library file).

## 3. Board and Module Structure

### 3.1 Board Definition

```
board PowerSupply {
  // Metadata
  author: "Design Team";
  version: "1.0";
  
  // Parameters
  parameters {
    // Use '=' for default value assignment in parameters blocks
    input_voltage = 12Vdc;
    output_voltage = 5Vdc;
    max_current: current = 2A; // Optional type specification
  }
  
  // External ports
  ports {
    VIN: in power(12Vdc, 2A);  // Example: Type with implicit properties
    GND: ground;
    VOUT: out power(5Vdc, 2A); // Example: Type with implicit properties
  }
  
  // ** Layer Stackup Definition **
  layer_stackup {
    layer TOP: { type: signal; material: "Copper"; thickness: 0.035mm; };
    layer DIEL1: { type: dielectric; material: "FR4"; thickness: 1.5mm; epsilon_r: 4.5; };
    layer BOTTOM: { type: signal; material: "Copper"; thickness: 0.035mm; };
    // More complex stackups would include plane layers, masks, silk, etc.
  }
  
  // ** Default Design Rules **
  default_design_rules {
    min_trace_width: 0.2mm;
    min_clearance: 0.2mm; // Default trace-trace, trace-pad, etc.
    min_via_drill: 0.3mm;
    min_via_pad_diameter: 0.6mm;
    min_annular_ring: 0.1mm;
    default_via_style: "StandardVia"; // References a defined via_style (See Sec 5.5)
  
    // Optional: Default net class assignments
    // assign_net_class("Power", nets_matching("VDD_*"));
  }
  
  // Implementation contents (components, connections, constraints, etc.)
  // ...
}
```

**Reasoning**: The board structure follows a declarative style that clearly separates metadata, parameters, interfaces, physical structure (`layer_stackup`), default rules (`default_design_rules`), and implementation. This makes it easier to understand the board's purpose, requirements, and manufacturing constraints.

### 3.2 Module Definition

```
module VoltageRegulator {
  // External interface
  ports {
    IN: in power(8Vdc to 35Vdc, 2A);  // Can accept up to 2A
    OUT: out power(5Vdc, 1A);         // Can supply up to 1A
    GND: ground;
    ENABLE: in digital;
  }
  
  // Parameters
  parameters {
    // Use '=' for default value assignment
    output_voltage = 5Vdc;
    max_current: current = 1A;
  }
  
  // Internal implementation
  // ...
}

### 3.2.1 Modules for Encapsulating Component Context

A common and powerful use of modules is to encapsulate a base library component (`component`) along with its mandatory surrounding context, such as required pull-up/pull-down resistors or essential decoupling capacitors. This creates a higher-level, pre-configured block that enforces design standards and simplifies instantiation for board designers.

**Example:** Consider a complex IC component (`Base_IC`) that requires a specific configuration pin (`CONFIG`) to always be pulled down with a `10kOhm` resistor in standard company designs. Instead of requiring every designer to add this resistor manually when using `Base_IC`, a module can encapsulate this:

```bhdl
// --- In a company library file ---
import StandardLibrary.Components.{Resistor};
import CompanyInternalLib.Components.{Base_IC}; // The raw IC component

// Module providing the IC with its mandatory pull-down
module Configured_IC {
  // Expose only the necessary pins/ports of the Base_IC
  ports {
    DATA_BUS: like Base_IC.DATA_BUS;
    CONTROL_SIGNALS: like Base_IC.CONTROL_SIGNALS;
    POWER: like Base_IC.POWER;
    GND: ground;
    // Note: Base_IC.CONFIG pin is NOT exposed
  }

  // Pass through relevant parameters if needed
  parameters {
    speed_grade: string = "standard";
  }

  // Internal implementation
  components {
    Base_IC U1 { // Instantiate the raw IC
      speed: module.speed_grade;
    }
    Resistor R_PULLDOWN { // The mandatory pull-down
      value: 10kOhm;
      tolerance: 5pct;
    }
  }

  connections {
    // Internal connection enforces the pull-down
    U1.CONFIG -> R_PULLDOWN.1;
    R_PULLDOWN.2 -> GND;

    // Connect exposed module ports to internal IC pins
    DATA_BUS <=> U1.DATA_BUS;
    CONTROL_SIGNALS <=> U1.CONTROL_SIGNALS;
    POWER -> U1.POWER;
    GND -> U1.GND;
  }
}

// --- In a board design file ---
board MySystem {
  // ... other components ...
  components {
    // Designers instantiate the configured module, not the base IC
    Configured_IC IC_Main {
      speed_grade: "high";
    }
  }

  connections {
    // Connect to the ports of the Configured_IC module
    MAIN_DATA_BUS <=> IC_Main.DATA_BUS;
    // ... other connections ...
  }
}
```

By instantiating `Configured_IC`, the designer gets the base IC functionality with the mandatory pull-down already implemented and enforced internally, promoting consistency and reducing errors. This is a key way BHDL supports creating robust, reusable mid-level abstractions.

### 3.3 Module Instantiation
```

### 3.4 Interface Definition and Connection

// **Interfaces with Generated Arrays:**
// Interfaces can leverage `generate for` to define large, parameterized buses concisely.

// Simplified DDR Interface Definition Example
interface DDR_Interface (data_width: integer = 64) {
   parameters {
     num_bytes: integer = data_width / 8;
   }
   pins {
     // Generate data bus
     generate for i in 0 to data_width-1 {
       DQ[i]: inout signal(ddr_dq_type); // Assumes ddr_dq_type is defined elsewhere
     }
     // Generate strobe pairs
     generate for i in 0 to num_bytes-1 {
       DQS_P[i]: inout signal(ddr_dqs_type); // Assumes ddr_dqs_type is defined
       DQS_N[i]: inout signal(ddr_dqs_type);
     }
     // ... other DDR signals like ADDR, CMD, CLK etc. would also be defined here ...
   }
}

// Component using the interface
component DDR_Controller {
  parameters {
     dw: integer = 64; // Data width parameter for the controller
  }
  interfaces {
    MEM: interface DDR_Interface(data_width=dw); // Instantiate interface, passing the width
  }
  // ... other controller-specific pins ...
}

// Component representing the DDR PHY/Memory
component DDR_PHY {
  parameters {
    data_width: integer = 64;
  }
  interfaces {
     // Assuming PHY also uses the same interface definition for compatibility
     BUS: interface DDR_Interface(data_width=module.data_width);
  }
  // ... other PHY pins ...
}

// Connecting interfaces with generated arrays
board TopLevel {
  components {
    DDR_Controller CPU(dw=64);
    DDR_PHY MEM_PHY(data_width=64);
  }
  connections {
    // The interface connection operator `<=>` implicitly connects all generated pins
    // by matching names within the interface definition.
    // Connects CPU.MEM.DQ[0..63] to MEM_PHY.BUS.DQ[0..63],
    // CPU.MEM.DQS_P[0..7] to MEM_PHY.BUS.DQS_P[0..7], etc.
    CPU.MEM <=> MEM_PHY.BUS;

    // If manual connection or swizzling of generated buses within interfaces is needed,
    // use `generate for` loops combined with slicing on the interface pins:
    // generate for i in 0 to 63 { CPU.MEM.DQ[i] -> SOME_OTHER_DEVICE.DATA[i]; }
  }
}

// **Interfaces within Components (Pin Multiplexing)**

When defined within a `component`, an `interface` serves not only to group related signals but also to map the logical pins of the interface (e.g., `MOSI`, `TX`) to the underlying physical pins of the component (e.g., `P1_0`, `GPIO_23`). This provides a clear mechanism for handling pin multiplexing (pinmux).

By connecting to a specific interface instance on the component (e.g., `MySoC.SPI1`), the designer explicitly declares the intended function for the underlying physical pins associated with that interface.

```bhdl
// --- Example: Component with Multiplexed Pins via Interfaces --- 
component ComplexSoC {
  // ... parameters ...

  // Define physical pins (potentially with function documentation - see Section 4.x)
  pins {
     P1_0: inout signal(cmos_3v3) { 
       functions: ["GPIO_1_0", "SPI1_MOSI", "UART0_TX", "I2C0_SDA"]; // Document potential roles
     }
     P1_1: inout signal(cmos_3v3) { 
       functions: ["GPIO_1_1", "SPI1_MISO", "UART0_RX", "I2C0_SCL"];
     }
     P1_2: inout signal(cmos_3v3) { functions: ["GPIO_1_2", "SPI1_SCK"]; }
     P1_3: inout signal(cmos_3v3) { functions: ["GPIO_1_3", "SPI1_CS"]; }
     // ... other pins ...
  }

  // Define available interfaces and map them to physical pins
  interfaces {
     SPI1: interface SPI { // Standard SPI interface
        // Map SPI logical pins to SoC physical pins
        pins: { MOSI: P1_0; MISO: P1_1; SCK: P1_2; CS: P1_3; }
        // Optionally override parameters like max_freq
        max_freq: 50MHz;
     }
     UART0: interface UART { // Standard UART interface
        // Map UART logical pins to SoC physical pins (Note: TX/RX share with SPI1)
        pins: { TX: P1_0; RX: P1_1; }
     }
     I2C0: interface I2C { // Standard I2C interface
        // Map I2C logical pins to SoC physical pins (Note: SDA/SCL share with SPI1/UART0)
        pins: { SDA: P1_0; SCL: P1_1; }
     }
     UART1: interface UART { // Another UART on dedicated pins
         pins: { TX: UART1_TX; RX: UART1_RX; }
     }
     // ... other interfaces like I2S, SDIO, GPIO_PortA ...
  }
  // ... package, footprint ...
}

// --- Example: Board using the SoC --- 
board MuxDemoBoard {
   components {
      ComplexSoC U_SOC {}; // Use {}
      SPI_Flash U_FLASH {}; // Use {}
      I2C_Sensor U_SENSOR {}; // Use {}
      UART_Header J_UART1 {}; // Use {}
   }

   connections {
      // Connect to the SPI Flash using the SPI1 interface.
      // This implicitly selects the SPI function for pins P1_0, P1_1, P1_2, P1_3.
      U_SOC.SPI1 <=> U_FLASH.SPI;

      // Connect to the I2C Sensor using the I2C0 interface.
      // This connection attempts to use P1_0 (SDA) and P1_1 (SCL).
      // **ERROR:** This conflicts with SPI1 usage! BHDL tools must detect this.
      // U_SOC.I2C0 <=> U_SENSOR.I2C; // <-- Expected validation error here.

      // Connect to the UART Header using UART1 (uses dedicated pins).
      // This is valid as it doesn't conflict with SPI1.
      U_SOC.UART1.TX -> J_UART1.RX_PIN;
      U_SOC.UART1.RX <- J_UART1.TX_PIN;

      // If P1_0 was *not* used by SPI1, it could be connected directly for GPIO:
      // U_SOC.P1_0 -> LED_INDICATOR; // Implies GPIO usage if no interface claims it.
   }
}
```

**Validation:** A key aspect of this approach is tool validation. BHDL tools are expected to track the usage of underlying physical pins. If multiple connections attempt to use interfaces that map to the same physical pin (like `SPI1.MOSI` and `UART0.TX` both mapping to `P1_0` in the example), the tool **must** report a design rule violation (see Section 7). This ensures the selected pin functions do not conflict at the board level.

**Reasoning**: Interfaces provide a modular and reusable way to define groups of related pins... [existing text] ...Using interfaces within component definitions to map logical functions to physical pins provides a clear, structured way to manage pin multiplexing, express design intent, and enable automated validation against conflicting pin usage.

### 3.5 Circuit Function Definitions

```
// ... existing code ...
```

### 6.2 Advanced DDR Controller-to-Memory Connection

This example demonstrates connecting a wide DDR controller data bus (e.g., 64-bit) to multiple narrower DRAM chips (e.g., four 16-bit chips), implementing byte-lane swizzling based on a configuration parameter. This showcases the use of parameters, `generate for`, and bus slicing for complex interface wiring.

```bhdl
// --- Assumed Type Definitions (Simplified) ---
typedef ddr4_dq {
  type: signal;
  domain: differential; // Often SSTL or similar
  // ... Add relevant voltage levels, timing, termination properties ...
}

// --- Assumed Component Definitions (Simplified) ---
component DDR_Controller {
  parameters {
    controller_width: integer = 64;
  }
  pins {
    generate for i in 0 to controller_width-1 {
      DQ[i]: inout signal(ddr4_dq);
    }
    // ... other DDR pins like ADDR, CMD, CLK, DQS etc. ...
  }
}

component DDR_Chip {
  parameters {
    chip_width: integer = 16;
  }
  pins {
    generate for i in 0 to chip_width-1 {
      DQ[i]: inout signal(ddr4_dq);
    }
    // ... other DDR pins like A, BA, CK, CKE, CS etc. ...
  }
}

// --- Board Definition ---
board DDR_System {
  parameters {
    // System Parameters
    controller_width: integer = 64;
    chip_width: integer = 16;
    num_chips: integer = controller_width / chip_width; // = 4 in this case
    bytes_per_chip: integer = chip_width / 8;           // = 2 in this case

    // ** Customizable Swizzle Map Parameter **
    // Defines how controller bytes map to chip byte lanes. This allows layout optimization
    // by changing the logical byte connections without modifying the core BHDL code.
    // Each element maps a physical chip slot.
    // 'controller_bytes': Lists which controller byte indices (0-7 for 64-bit) connect to this chip.
    // 'lane_order': Specifies the mapping of those controller bytes to the chip's internal
    //                byte lanes (0 to bytes_per_chip-1). The value at lane_order[chip_lane]
    //                indicates the *index* within the 'controller_bytes' list for that chip.
    chip_byte_map: list = [
      // Physical Chip Slot 0: Controller Bytes 0, 1 -> Chip Lanes 0, 1 (Normal Order)
      { chip_idx: 0, controller_bytes: [0, 1], lane_order: [0, 1] },

      // Physical Chip Slot 1: Controller Bytes 2, 3 -> Chip Lanes 1, 0 (Bytes Swapped on Chip 1)
      { chip_idx: 1, controller_bytes: [2, 3], lane_order: [1, 0] },

      // Physical Chip Slot 2: Controller Bytes 6, 7 -> Chip Lanes 0, 1 (Controller bytes 6/7 map here)
      { chip_idx: 2, controller_bytes: [6, 7], lane_order: [0, 1] },

      // Physical Chip Slot 3: Controller Bytes 4, 5 -> Chip Lanes 0, 1 (Controller bytes 4/5 map here)
      { chip_idx: 3, controller_bytes: [4, 5], lane_order: [0, 1] }
    ];
    // Note: This map implies controller byte order 0, 1, 2, 3, 6, 7, 4, 5 going to chips 0, 1, 2, 3 respectively.

    // ** Optional Bit Swizzle Map (More Complex - omitted for clarity) **
    // A similar map could define bit swaps within bytes if the DRAM standard allows.
  }

  components {
    DDR_Controller CTRL { controller_width: module.controller_width }; // Use {}

    // Generate DRAM chip instances, named U_DRAM[0], U_DRAM[1], etc.
    generate for i in 0 to num_chips-1 {
      DDR_Chip U_DRAM[i] { chip_width: module.chip_width }; // Use {}
    }
  }

  connections {
    // --- DQ Connections with Byte Swizzling ---

    // Iterate through each entry in our custom mapping definition
    generate for chip_map_entry in chip_byte_map {
      // Get the physical chip index (0, 1, 2, or 3) for this mapping entry
      local current_chip_idx = chip_map_entry.chip_idx;

      // Iterate through the byte lanes *on this specific chip* (0 to bytes_per_chip-1, so 0, 1 for 16-bit chips)
      generate for chip_lane_idx in 0 to bytes_per_chip-1 {

        // --- Determine the Source Controller Byte Index ---
        // 1. Use 'chip_lane_idx' to look up the position in the chip's 'lane_order' array.
        //    e.g., For chip 1, lane 0: lane_order[0] is 1. For chip 1, lane 1: lane_order[1] is 0.
        local controller_byte_list_pos = chip_map_entry.lane_order[chip_lane_idx];

        // 2. Use that position to get the actual Controller Byte Index from the 'controller_bytes' list.
        //    e.g., For chip 1, lane 0: controller_bytes[1] is 3. (Controller Byte 3 maps to Chip 1, Lane 0)
        //    e.g., For chip 1, lane 1: controller_bytes[0] is 2. (Controller Byte 2 maps to Chip 1, Lane 1)
        local controller_byte_idx = chip_map_entry.controller_bytes[controller_byte_list_pos];

        // --- Calculate Controller Slice Indices for this Byte ---
        local ctrl_hi = (controller_byte_idx + 1) * 8 - 1;
        local ctrl_lo = controller_byte_idx * 8;

        // --- Calculate Chip Slice Indices for this Byte Lane ---
        local chip_hi = (chip_lane_idx + 1) * 8 - 1;
        local chip_lo = chip_lane_idx * 8;

        // --- Generate the Swizzled Connection ---
        // Connect the calculated controller byte slice to the current chip's byte lane slice.
        // Example Trace for chip_idx=1, chip_lane_idx=0:
        //   controller_byte_list_pos = lane_order[0] = 1
        //   controller_byte_idx = controller_bytes[1] = 3
        //   ctrl_hi=31, ctrl_lo=24 --> CTRL.DQ[31:24]
        //   chip_hi=7, chip_lo=0 --> U_DRAM[1].DQ[7:0]
        //   Connects CTRL.DQ[31:24] -> U_DRAM[1].DQ[7:0]
        CTRL.DQ[ ctrl_hi : ctrl_lo ] -> U_DRAM[current_chip_idx].DQ[ chip_hi : chip_lo ];

        // --- Optional Bit Swizzling ---
        // If needed, a nested loop and a bit-level map could connect individual bits here instead
        // of connecting the 8-bit slices directly.
      }
    }

    // --- Address/Command/Control Connections --- 
    // These often have different routing topologies (e.g., bussed or fly-by).
    // This example shows simple bussing to all chips. (Actual topology depends on DDR standard)
    // generate for addr_bit in 0 to 15 { // Assuming 16 address lines in controller/chips
    //   generate for chip_idx in 0 to num_chips-1 {
    //      CTRL.A[addr_bit] -> U_DRAM[chip_idx].A[addr_bit];
    //   }
    // }
    // ... connections for CK, CS, CKE, DQS etc. would follow their specific topology rules ...
    // DQS pairs would likely be handled similarly to DQ, potentially using the same byte map
    // or a separate one if DQS swizzling differs from DQ.
  }

  // --- Constraints --- 
  // Add necessary impedance, length matching, and differential pair constraints
  // for DQ, DQS, ADDR/CMD, CLK signals according to DDR spec and layout needs.
  // Example (Conceptual - assumes constraint system supports groups and iteration):
  // generate for i in 0 to controller_width-1 {
  //   constrain (CTRL.DQ[i]) { impedance: 40Ohm +/- 10pct; }
  // }
  // generate for i in 0 to num_chips-1 {
  //   constrain (U_DRAM[i].DQ[*]) { impedance: 40Ohm +/- 10pct; }
  // }
  // group DQ_BYTES { ... } // Define groups based on byte lanes
  // constrain group DQ_BYTES { length_match_within_group: 0.5mm; ... }
}
```

This example illustrates how `generate for` loops, parameterized components, bus slicing, and configuration parameters (`chip_byte_map`) can work together to manage complex, customizable bus wiring like DDR interfaces within BHDL.

## 7. Design Validation Rules

BHDL is designed not just for description but also for enabling automated design validation. BHDL compilers and analysis tools are expected to perform various checks to ensure design correctness, consistency, and adherence to electrical and physical rules. While specific tool implementations may vary, core validation rules should include:

1.  **Electrical Rules:**
    *   **Type Compatibility:** Verify compatible electrical types for connected pins (voltage levels, logic thresholds, domains). Report errors for definite incompatibilities (e.g., 5V output to 3.3V input) and warnings for potential issues (e.g., missing impedance match).
    *   **Pin Directionality:** Check for output-output conflicts (unless open-drain/bus), ensure inputs are driven.
    *   **Pin Usage Exclusivity (Pinmux):** Report errors if multiple interfaces claim the same physical pin.
    *   **Unit Consistency:** Ensure valid arithmetic/comparisons with units.
    *   **Connection Completeness (Optional):** Check for unconnected inputs/outputs.

2.  **Physical/Manufacturing Rules (DRC):**
    *   **Clearance Violations:** Check trace-to-trace, trace-to-pad, trace-to-via, pad-to-pad, via-to-via, component-to-component, etc., clearances against `default_design_rules` and applicable `net_class` rules.
    *   **Trace Width Violations:** Check trace widths against minimums defined in `default_design_rules` and `net_class` rules.
    *   **Annular Ring Violations:** Check via pad sizes against drill sizes based on `via_style` definitions and `default_design_rules` minimums.
    *   **Via Style Violations:** Check if via usage matches specified `via_style` constraints (e.g., layer span, drill size).
    *   **Layer Stackup Consistency:** Validate routing and via spans against the defined `layer_stackup`.
    *   **Constraint Adherence:** Verify that specific constraints defined in Section 5 (impedance, length matching, placement, keepouts, etc.) are met by the layout representation (requires integration with layout data or estimation).
    *   **Geometric Violations (Optional):** Check for potential manufacturing issues like acid traps, acute angles, silkscreen overlaps, soldermask slivers (typically requires geometric analysis).
    *   **Connectivity Checks:** Verify logical connectivity from BHDL matches physical connectivity in layout (e.g., checking for shorts and opens based on the BHDL netlist).

3.  **Parameter Consistency:**
    *   Verify compatibility of parameter values during instantiation.
    *   Check parameter consistency across connected interfaces.

4.  **Automation:**
    *   Ensure that implicit actions like pull-ups, level shifters, and termination resistors are correctly applied based on the design intent.
    *   Verify that the tool's automation does not introduce unintended design changes.

5.  **Design Rule Checks (DRC):**
    *   Perform checks related to trace widths, clearances, via sizes, layer stackup, and other manufacturing constraints.
    *   Ensure that the design adheres to the specified design rules and constraints.

6.  **Design Intent Verification:**
    *   Verify that the design intent captured in BHDL is correctly implemented in the layout.
    *   Check for any unintended design changes or deviations from the original specification.

7.  **Design Consistency:**
    *   Ensure that the design is consistent across different design iterations and revisions.
    *   Verify that the design rules and constraints are applied uniformly throughout the design.

8.  **Design Rule Checking (DRC):**
    *   Perform checks related to trace widths, clearances, via sizes, layer stackup, and other manufacturing constraints.
    *   Ensure that the design adheres to the specified design rules and constraints.

9.  **Design Intent Verification:**
    *   Verify that the design intent captured in BHDL is correctly implemented in the layout.
    *   Check for any unintended design changes or deviations from the original specification.

10.  **Design Consistency:**
    *   Ensure that the design is consistent across different design iterations and revisions.
    *   Verify that the design rules and constraints are applied uniformly throughout the design.

**Reasoning**: By incorporating these checks into the design process, BHDL enables automated design validation that can catch common errors early, significantly improving design reliability and reducing debugging time compared to purely manual or graphical verification methods.

### 4.3 Pin Definitions in Components

Within a `component` definition, pins are declared in the `pins { ... }` block. Each pin definition specifies its name, direction (`in`, `out`, `inout`), and electrical type (Section 2.4). The leading `pin` keyword is optional.

```bhdl
component ExampleIC {
  pins {
    // Keyword 'pin' is optional
    VDD: in power(lv_digital_power);
    GND: ground;
    ENABLE: in signal(cmos_3v3);
    DATA_OUT: out signal(cmos_3v3);
    BIDIR_PIN: inout signal(cmos_1v8);
    RESET_N: in signal; // Assumes signal type if only direction is given after colon
  }
}
```

**Optional Pin Function Documentation (for Multiplexed Pins):**

For components with pins that serve multiple functions depending on runtime configuration (pin multiplexing), you can optionally document these potential functions using a `functions` list. This information is primarily for human readability and tooltips; the active function in a specific design is determined by how the pin is connected (typically via an `interface`, see Section 3.4).

```bhdl
component ComplexSoC {
  pins {
     P1_0: inout signal(cmos_3v3) { 
       functions: ["GPIO_1_0", "SPI1_MOSI", "UART0_TX", "I2C0_SDA"]; // Document potential roles
     }
     P1_1: inout signal(cmos_3v3) { 
       functions: ["GPIO_1_1", "SPI1_MISO", "UART0_RX", "I2C0_SCL"];
     }
     P1_2: inout signal(cmos_3v3) { functions: ["GPIO_1_2", "SPI1_SCK"]; }
     P1_3: inout signal(cmos_3v3) { functions: ["GPIO_1_3", "SPI1_CS"]; }
     // ... other pins ...
  }
  // ... interfaces map functions to these physical pins ...
}
```

This documentation helps users understand the capabilities of the component's pins directly from the BHDL source.

### 4.4 Component Population and Variant Management (SKUs)

Real-world designs often require managing product variants or SKUs where certain components are intentionally not populated (DNP - Do Not Populate / DNI - Do Not Install) on specific board builds.

BHDL supports this through a combination of board parameterization and a standard component property:

**1. `population` Property:**

Components can have an optional `population` property:

*   **Type:** `string`
*   **Allowed Values:**
    *   `"Installed"`: The component is populated on the board (Default).
    *   `"DNP"` / `"DNI"`: The component is not populated. The footprint typically still exists on the PCB, but the component is omitted during assembly.
    *   Other tool-specific strings might be supported (e.g., `"Fitted"`, `"Not Fitted"`).
*   **Default:** If the `population` property is omitted, it defaults to `"Installed"`.

**2. Board Parameters for SKUs:**

The top-level `board` definition can accept parameters (Section 3.1) to specify the current SKU or feature set being built.

```bhdl
board MyVariantBoard (
  SKU: string = "Base", // e.g., "Base", "Premium", "Lite"
  Region: string = "WW" // e.g., "WW", "EU", "NA"
) {
  // ... components ...
}
```

**3. Conditional Population:**

The `population` property within a component definition can use conditional expressions based on the board parameters to determine its state for a given configuration.

**Example:**

```bhdl
board MyVariantBoard (SKU: string = "Base") {

  // --- Common Components ---
  component Resistor R1 { value: 1k; } // Implicitly population: "Installed"
  connect(NetA, R1.1);
  connect(R1.2, NetB);

  // --- SKU-Specific Components ---

  // Option 1: Feature only populated in "Premium" SKU
  component OpAmp U_FeatureAmp {
    part_number: "OPA123";
    population: (SKU == "Premium") ? "Installed" : "DNP"; // Conditional
  }
  connect(NetB, U_FeatureAmp.IN_POS);
  connect(U_FeatureAmp.OUT, NetC);
  // Connections remain defined; only assembly is affected by DNP

  // Option 2: Debug header DNP on cost-optimized SKU
  component DebugHeader J1 {
    connector_type: "2x5";
    // DNP on cost-optimized SKU 'Lite'
    population: (SKU != "Lite") ? "Installed" : "DNP";
  }

  // Option 3: Component property variation (value changes per SKU)
  component Resistor R_Tuning {
    // Population is always "Installed" (default), but value varies
    value: (SKU == "VariantA") ? 4.7k : 10k;
  }
  connect(NetC, R_Tuning.1);
  connect(R_Tuning.2, GND);
}

// Build process selects the active configuration:
// > bhdl_build MyVariantBoard --param SKU=Premium
// > bhdl_build MyVariantBoard --param SKU=Lite
```

**Tooling and Workflow:**

*   **BOM Generation:** Tools processing the BHDL design (after evaluating parameters for a specific SKU) can read the `population` property to generate accurate, SKU-specific Bills of Materials.
*   **Layout/Assembly:** Layout tools can use this information to mark DNP components on assembly drawings or adjust pick-and-place data.
*   **DRC:** Standard DRC typically operates on the superset netlist. Specific checks related to nets connected only to DNP components might be configurable.

**Reviewability Considerations:**

While conditional population provides flexibility, complex conditions scattered throughout a design can make reviews challenging. Effective management relies on:

1.  **Tooling Support (Primary):** BHDL IDEs and review tools should ideally provide features to visualize the evaluated population status for a selected SKU configuration (e.g., graying out DNP components, showing tooltips). This offers immediate clarity without manual evaluation.
2.  **Coding Conventions:**
    *   **Simplicity:** Keep conditional expressions straightforward (e.g., direct comparisons).
    *   **Proximity:** Group components related to specific features or SKUs.
    *   **Comments:** Explain the reasoning behind DNP choices, especially for non-obvious conditions.
    *   **Explicitness (Optional Convention):** Consider always specifying `population`, even if `"Installed"`, to make the intent explicit for every component.

By combining board parameters with the standard `population` property and emphasizing the role of tooling for visualization, BHDL provides a robust mechanism for managing board variants and DNP components.

**Note on Workflow and Component Declaration:**

While the specification requires all components to be explicitly declared in the `components` block for clarity and consistency, the intended development workflow leverages IDE tooling. Developers can focus on defining connections first. Language Servers and IDE extensions are expected to provide features (e.g., code actions) that automatically generate the corresponding component declaration in this block when a new component instance name is used in the `connections` block. This approach provides a fluid, sketch-like experience (similar to the intent behind potential inline instantiation features) while maintaining the structural benefits of a centralized component inventory and avoiding language complexities associated with inline declarations.


## 5. Physical Design Constraints

### 5.1 Connection Constraints

Constraints can be applied to specific nets or groups of nets to guide routing and influence automated actions (See Section 8).

```bhdl
// ... [Existing connection examples] ...

  // Apply constraints using a 'constrain' block or inline modifiers

  // Option 1: Constrain block targeting specific nets
  constrain (MCU.CLK) {
    max_length: 50mm;
    impedance: 50Ohm +/- 5pct;
    layer_preference: "TOP";
  }

  // Option 2: Constrain block targeting multiple nets
  constrain (MCU.DATA[0], MCU.DATA[1]) {
    impedance: 60Ohm +/- 10pct;
    group: "DataBus"; // Assign to a routing group
  }

  // Option 3: Constraining differential pairs
  constrain (MCU.USB_P, MCU.USB_N) {
    type: differential_pair;
    impedance: 90Ohm +/- 5pct;
    length_match_tolerance: 1mm;
    max_length: 25mm;
    primary_layer: "SignalLayer1";
    gap: 0.2mm; // Target spacing for the pair
  }
  
  // ** Option 4: Constraints for Automation **
  
  // Constraining an I2C connection to override default pull-up
  // Assume U_SOC.I2C0 and U_SENSOR.I2C pins use type i2c_signal_3v3 (default 4.7k pull-up)
  constrain (U_SOC.I2C0.SDA -> U_SENSOR.SDA) {
     // Override the default pull-up defined in the i2c_signal_3v3 type for this specific net
     pullup_resistance: 2.2kOhm; // Use stronger pull-up for higher speed
     // auto_pullup: true; // This is implied by providing pullup_resistance
  }
  constrain (U_SOC.I2C0.SCL -> U_SENSOR.SCL) {
     pullup_resistance: 2.2kOhm; // Override for SCL too
  }

  // Example: Connecting between different voltage domains and disabling auto level shift
  // Assume MCU_3V3.TX is cmos_3v3 and FPGA_1V8.RX is cmos_1v8
  constrain (MCU_3V3.TX -> FPGA_1V8.RX) {
      auto_level_shift: false; // Disable automatic insertion, requires manual shifter elsewhere
  }

  // Example: Disabling automatic pull-up for an open-drain signal (manual implementation desired)
  // Assume ALERT_N pin type has is_open_drain: true
  constrain (Sensor.ALERT_N -> MCU.IRQ_PIN) {
      auto_pullup: false; 
  }
  
  // Option 4: Inline constraints (Syntax TBD - might use annotations or modifiers)
  // MCU.ADDR[0] -> MEM.A0 { impedance: 50Ohm; }; // Conceptual inline syntax
}
// ... [Existing constraint group examples] ...
```

**Common Connection Constraints:**

*   `impedance`: Target characteristic impedance (e.g., `50Ohm`).
*   `max_length`, `min_length`: Maximum or minimum trace length.
*   `length_match_tolerance`: Maximum length difference between nets in a group.
*   `max_skew`: Maximum timing difference between nets in a group.
*   `differential_pair`: Defines a pair with specific impedance and spacing (`gap`).
*   `layer`, `layer_preference`, `avoid_layers`: Layer routing rules.
*   `shielding`, `guard_trace`: Specify shielding requirements.
*   `group`: Assign nets to a named group for collective constraints.
*   `topology`: Specify routing topology (e.g., `daisy_chain`, `star`).
*   `max_via_count`: Limit the number of vias on a net.
*   **`net_class`** (`string`): Assigns the specified net(s) to a defined Net Class (Section 5.4), inheriting its rules.
*   **`via_style`** (`string`): Specifies a defined Via Style (Section 5.5) to be used for vias on the specified net(s), overriding defaults.
*   `pullup_resistance`: Specifies the desired pull-up resistor value for automated insertion (overrides `typedef` default). Implies `auto_pullup: true`.
*   `pulldown_resistance`: Specifies desired pull-down value for automation (requires relevant `typedef` properties). Implies `auto_pulldown: true`.
*   `auto_pullup` (`boolean`): Explicitly enable (`true`, default if implicit conditions met) or disable (`false`) automatic pull-up resistor insertion.
*   `auto_pulldown` (`boolean`): Explicitly enable/disable automatic pull-down insertion.
*   `auto_level_shift` (`boolean`): Explicitly enable (`true`, default if implicit conditions met) or disable (`false`) automatic level shifter insertion.

### 5.2 Placement Constraints
// ... existing code ...
```

**Reasoning:** Building validation rules directly into the language's expectation enables the development of powerful analysis tools that can catch common design errors early, significantly improving design reliability and reducing debugging time compared to purely graphical schematic capture where such checks are often less integrated or comprehensive.

## 8. Automation Conventions & Implicit Actions

To reduce boilerplate and enforce common design practices, BHDL tools are expected to support certain implicit actions based on the analysis of connections and component types. These conventions allow designers to focus on the core logic while the tool handles standard interface requirements like pull-ups or level shifting. Designers retain control via explicit constraints to override or disable these actions when necessary.

### 8.1 Voltage Domain Inference

Understanding the voltage domain in which signals operate is crucial for automation.

*   **Inference:** A signal pin's voltage domain is primarily inferred from:
    1.  The voltage levels specified in its `typedef` (e.g., `voltage_high: 3.3Vdc`).
    2.  The specific `power` rail(s) connected to the `power` input pin(s) of its parent component instance in the board design.
*   **Component Power:** Components must have clearly defined `power` and `ground` pins in their definition. Tools trace connections to these pins back to board-level power nets (e.g., `VDD_3V3`, `VCC_1V8`) to associate component instances with specific voltage rails.
*   **Multiple Domains:** Components operating across multiple voltage domains (e.g., separate core and I/O voltages) should define distinct `power` input pins for each domain (e.g., `VDD_CORE`, `VDD_IO`). The `typedef` for pins associated with a specific domain should reflect that domain's voltage levels.

### 8.2 Automatic Pull-up/Pull-down Insertion

Tools should automate the addition of pull-up/pull-down resistors for appropriate signal types, primarily open-drain outputs.

*   **Trigger:** A connection involving a pin whose `typedef` includes `is_open_drain: true` (or a similar property indicating a need for external pull-resistors).
*   **Action:** The tool implicitly instantiates and connects a pull-up resistor (or pull-down, if specified by type/constraint).
*   **Rules & Overrides:**
    1.  **Check Disable:** If the connection has the constraint `auto_pullup: false` (or `auto_pulldown: false`), no action is taken.
    2.  **Check Override Value:** If `auto_pullup` is not `false`, check if the connection has a `pullup_resistance` constraint (Section 5.1). If yes, use this value for the implicitly added resistor.
    3.  **Use Type Default:** If `auto_pullup` is not `false` and no `pullup_resistance` constraint exists, check the pin's `typedef` for `default_pullup_resistance`. If defined, use this value.
    4.  **Error/Warning:** If `auto_pullup` is effectively enabled (by `is_open_drain: true` or `pullup_resistance` constraint) but no value can be determined (no override constraint and no type default), the tool should issue an error or warning.
    5.  **Rail Connection:** The implicit resistor is connected between the signal net and the appropriate power rail associated with the signal's inferred voltage domain (typically the positive supply rail for pull-ups, ground for pull-downs).

### 8.3 Automatic Level Shifter Insertion

Tools should automate the insertion of level shifters when connecting signals between incompatible voltage domains.

*   **Trigger:** A connection is made between two signal pins (`A -> B`) where their inferred voltage domains (based on `typedef` voltages and component power connections) are significantly different and potentially incompatible (e.g., connecting a `3.3Vdc` output pin `A` to a non-tolerant `1.8Vdc` input pin `B`). Compatibility rules need to consider both nominal levels and input/output thresholds defined in the `typedef`s.
*   **Action:** The tool implicitly instantiates and connects an appropriate level shifter component.
*   **Rules & Overrides:**
    1.  **Check Disable:** If the connection has the constraint `auto_level_shift: false`, no action is taken. The designer is responsible for ensuring compatibility or inserting a manual shifter.
    2.  **Check Necessity:** If not disabled, the tool analyzes the voltage domains and type thresholds of the connected pins (A and B) to determine if level shifting is required according to built-in or configurable compatibility rules.
    3.  **Select Shifter Type:** If shifting is deemed necessary, the tool selects an appropriate level shifter component based on:
        *   Directionality (unidirectional, bidirectional).
        *   Voltage levels involved (e.g., 3.3V to 1.8V).
        *   Signal characteristics (speed, type - needed for choosing appropriate shifter topology).
        *   Availability in a configured component library accessible to the tool.
    4.  **Instantiate & Connect:** The tool implicitly instantiates the selected shifter, connects its low-voltage side pins to the lower-voltage signal(s)/domain, connects its high-voltage side pins to the higher-voltage signal(s)/domain, and connects the shifter's own power/ground pins to the appropriate board rails.
    5.  **Configuration:** The specific rules for determining incompatibility and the library of available level shifters are likely configurable within the BHDL tool environment.

### 8.4 Disabling Automation

Designers must have the ability to disable these implicit actions on a per-connection basis when manual control or alternative implementations are desired.

*   **Syntax:** Use connection constraints (Section 5.1).
    *   `auto_pullup: false;`
    *   `auto_pulldown: false;`
    *   `auto_level_shift: false;`
*   **Use Case:** Needed when using custom pull-up circuits, non-standard level shifting methods, or when the tool's default behavior or component choice is unsuitable for a specific case.

**Integration:** Tools would use these constraints and properties to automate layout tasks, perform DRC checks relevant to high-speed or power design, and ensure the physical implementation matches the design intent captured in BHDL.

### 5.4 Net Classes

Net classes allow grouping nets with similar physical routing requirements and assigning specific design rules that override the board's defaults (defined in `default_design_rules`).

**Definition:**
Net classes are defined at the board level or, more commonly, **within library files** (e.g., `company_standards.drc.bhdl`) for reuse across multiple projects using the `net_class` keyword. They are brought into a design file using the `import` statement (Section 2.8).

```bhdl
net_class <ClassName> {
  // Rule overrides - properties match those in default_design_rules
  min_trace_width: distance;
  min_clearance: distance;
  trace_impedance: resistance; // Target impedance for nets in this class
  default_via_style: string; // Name of a via_style (See Sec 5.5)
  max_length: distance;
  // ... other relevant rules ...
}

// Example Definitions
net_class Power {
  min_trace_width: 0.5mm;
  min_clearance: 0.4mm;
  default_via_style: "PowerVia";
}

net_class HighSpeedDiffPair {
  min_clearance: 0.15mm; // Clearance between pairs
  intra_pair_gap: 0.1mm;  // Clearance within a pair
  trace_impedance: 90Ohm +/- 5pct;
  default_via_style: "MicroVia_Stacked"; // Example advanced via
  length_match_group_tolerance: 0.2mm;
}

net_class AnalogSensitive {
  min_clearance: 0.5mm; // Extra clearance to other nets
  shielding_required: true;
}
```

**Rule Precedence:**
When determining the physical rules (width, clearance, via style, etc.) for a specific net, the following order of precedence applies:
1.  **Specific Constraint:** Rules defined directly on the net or connection within a `constrain` block (Section 5.1) have the highest priority.
2.  **Net Class:** Rules defined in the `net_class` assigned to the net (if any) have the next priority.
3.  **Board Default:** Rules defined in the `default_design_rules` block (Section 3.1) have the lowest priority.

**Assignment:**
Nets can be assigned to a class in several ways (Syntax TBD - needs refinement):

1.  **Explicitly in Connection Constraints:**
    ```bhdl
    constrain (VDD_3V3, VDD_1V8) { net_class: "Power"; }
    constrain (USB_P, USB_N) { net_class: "HighSpeedDiffPair"; }
    ```
2.  **By Name Pattern (in `default_design_rules` or dedicated block):**
    ```bhdl
    // Within default_design_rules or a dedicated assignment block
    assign_net_class("Power", nets_matching("VDD_*", "VCC_*"));
    assign_net_class("AnalogSensitive", nets_matching("AUDIO_*", "SENSOR_AIN"));
    ```
3.  **By Associated Component/Pin Property (Implicitly):** Tools could potentially infer classes based on pin types or component properties, but explicit assignment provides more control.

Tools use the rules defined in the assigned `net_class` for DRC checks on those specific nets, falling back to `default_design_rules` for unassigned nets, respecting the defined rule precedence.

### 5.5 Via Styles

Via styles define named templates for vias, specifying their physical construction. Like net classes, they are typically defined in **reusable library files** and imported into board designs.

**Definition:**
Via styles are defined using the `via_style` keyword.

```bhdl
via_style <StyleName> {
  drill: distance;
  pad_diameter: distance;
  layer_span: [ <TopLayerName>, <BottomLayerName> ]; // Defines span (e.g., ["TOP", "BOTTOM"] for through-hole)
  plating_thickness: distance; // Optional
  thermal_relief_type: enum {none, spokes, flood}; // Optional
  is_filled: boolean; // Optional: e.g., for via-in-pad
  // ... other properties ...
}

// Example Definitions
via_style StandardVia {
  drill: 0.3mm;
  pad_diameter: 0.6mm;
  layer_span: ["TOP", "BOTTOM"]; // Standard through-hole
}

via_style PowerVia {
  drill: 0.5mm;
  pad_diameter: 1.0mm;
  layer_span: ["TOP", "BOTTOM"];
  thermal_relief_type: spokes;
}

via_style MicroVia_L1_L2 {
  drill: 0.1mm;
  pad_diameter: 0.25mm;
  layer_span: ["TOP", "Layer2"]; // Blind via example
  is_filled: true;
}
```

**Usage:**

*   A `default_via_style` can be set in `default_design_rules` or within a `net_class` by referencing the name of an imported or locally defined `via_style`.
*   A specific `via_style` can be assigned to override the default for a particular net using connection constraints (e.g., `constrain (MyNet) { via_style: "MicroVia_L1_L2"; }`). The precedence follows the same order as net class rules (Specific Constraint > Net Class Default > Board Default).

Tools use these definitions for DRC checks related to annular rings, via clearances, and manufacturing constraints based on the applicable style determined by the precedence rules.

## 6. Complete Examples
// ... existing code ...
```

# Appendix A: Integration with Functional Safety Analysis (ISO 26262)

**Note:** The mechanisms described in this appendix are optional and intended for projects requiring integration with functional safety analysis workflows, such as those following standards like ISO 26262. Core BHDL usage does not require these features.

## A.1 Motivation and Separation of Concerns

Functional safety analysis (e.g., FMEA/FMEDA) requires assessing the hardware design against safety goals, often involving hierarchical decomposition, failure rate analysis, and diagnostic coverage evaluation. While BHDL describes the hardware structure, embedding detailed safety parameters (FIT rates, failure modes, diagnostic coverage, safety goals) directly within component or module definitions would overload the hardware description and mix distinct concerns.

A cleaner approach separates these concerns:

1.  **BHDL Design Files (`.board.bhdl`, `.module.bhdl`, etc.):** Describe the functional hardware structure, components, and connections.
2.  **Safety Hierarchy File (`.safety.bhdl` - Optional):** Defines the hierarchical view required for safety analysis (System, Subsystem, Part, etc.) and maps elements from the functional BHDL design onto this safety view. This is crucial when the functional hardware decomposition differs from the required safety decomposition.
3.  **Safety Data Model (External File(s) - e.g., XML, JSON, CSV, Tool-specific DB):** Contains the detailed safety parameters for base hardware elements (e.g., component FIT rates, failure modes, diagnostic coverage associated with safety mechanisms).

BHDL provides optional linking mechanisms to connect these pieces.

## A.2 Linking Base Components (`safety_element_id`)

To link a fundamental hardware element defined in BHDL (typically a `component`) to its corresponding entry in the external safety data model, an optional property can be used:

*   **`safety_element_id: string` (Optional Property):**
    *   Can be added to `component` or `module` definitions.
    *   The string value is a unique identifier matching an entry in the external safety data model (specified via `safety_model_files` or build configuration).
    *   This property provides the link to retrieve base failure rates, modes, etc., for the lowest-level hardware elements.

```bhdl
// In a component definition file or library
component Microcontroller {
  part_number: "SPC5xxx";
  safety_element_id: "ELEM_MCU_SPC5"; // Links to safety data for this specific MCU type
  // ... pins, other properties ...
}

component VoltageRegulator {
  type: "Linear";
  safety_element_id: "ELEM_REG_GENERIC_LINEAR"; // Links to generic linear regulator data
  // ... pins, properties ...
}
```

## A.3 Defining the Safety Hierarchy View (`.safety.bhdl`)

When the functional hardware structure in BHDL does not directly match the hierarchy needed for safety analysis, a separate safety view file can define the required structure and mapping.

*   **File Extension:** Recommended: `.safety.bhdl`
*   **Content:** Contains one or more `safety_hierarchy` blocks.

```bhdl
// File: ECU_SafetyView.safety.bhdl

safety_hierarchy ECU_SafetyView {
    // Define nodes using standard keywords (system, subsystem, part, etc.)
    // or potentially custom node types if needed by tooling.
    system SYS_ECU {
        description: "Main Electronic Control Unit"; // Optional description

        subsystem SUB_PWR {
            description: "Power Supply Subsystem";
            // Maps BHDL instances from the importing board file to this safety node
            maps_elements: [ PowerManager ]; // Maps the 'PowerManager' instance

            part PART_REG5V {
                description: "5 Volt Regulation Circuit";
                // Map specific component instances or instances within module instances
                maps_elements: [ PowerManager.Reg5V_Circuit, // Instance within PowerManager
                                 Decoupling_Caps.C5,       // Component instance
                                 Decoupling_Caps.C6 ];      // Component instance
            }
            // ... other 'part' nodes within SUB_PWR ...
        }

        subsystem SUB_MCU {
            description: "Microcontroller Core and Peripherals";
            maps_elements: [ MainMCU_Instance ]; // Map the main MCU instance
        }

        // ... other 'subsystem' nodes within SYS_ECU ...
    }
}
```

*   **`maps_elements` List:** This list within each safety hierarchy node specifies which BHDL elements belong to that node. Paths refer to instance names in the board file that imports this hierarchy.
    *   Direct instance names (e.g., `PowerManager`, `MainMCU_Instance`).
    *   Nested instance names using dot notation (e.g., `PowerManager.Reg5V_Circuit`).
    *   (Potentially) Type-based mapping, though instance mapping is more explicit.

## A.4 Importing the Safety Hierarchy (`import safety`)

To apply a defined safety hierarchy view to a specific board design, use the `import safety` statement *inside* the `board` block:

```bhdl
// File: AutomotiveECU.board.bhdl

board AutomotiveECU {

    // 1. Import the safety hierarchy definition
    import safety ECU_SafetyView from "ECU_SafetyView.safety.bhdl";

    // 2. Specify the location of the external safety data model
    // (Syntax may vary - could be property or build config)
    safety_model_files: ["safety/ECU_SafetyData.xml"];

    // --- Functional BHDL Structure ---
    components {
      PowerManager PowerManager; // Module instance
      DecouplingCaps Decoupling_Caps; // Module instance
      Microcontroller MainMCU_Instance { safety_element_id: "ELEM_MCU_SPC5"; } // Component instance
      // ... other instances ...
    }
    connections { /* ... */ }
}
```

## A.5 Tooling Workflow

Functional safety analysis tools supporting BHDL would typically:

1.  Parse the main BHDL board file (`.board.bhdl`).
2.  If an `import safety` statement is present, parse the specified `.safety.bhdl` file to load the safety hierarchy structure and mapping rules.
3.  Parse the functional BHDL hierarchy (modules, components, connections).
4.  Read the specified external safety data model file(s) (e.g., `ECU_SafetyData.xml`).
5.  Use the `safety_hierarchy` structure, `maps_elements` rules, and `safety_element_id` properties to correlate BHDL elements with safety data.
6.  Perform FMEA/FMEDA calculations based on the hardware structure, safety hierarchy, and associated safety parameters (FIT rates, failure modes, diagnostic coverage, etc.) to generate safety metrics (SPFM, LFM, etc.).

This layered approach allows BHDL to integrate into functional safety workflows without overloading the core hardware description language, providing flexibility and maintaining separation of concerns. 

### 4.5 Connections Block

The `connections` block defines how component pins are connected using nets.

**Basic Connections:**

The fundamental connection uses the `->` operator to indicate signal flow or connection between a net and a pin, or between two pins.

```bhdl
connections {
  // Net to Pin
  Net_A -> U1.Pin1;

  // Pin to Net
  U1.Pin2 -> Net_B;

  // Pin to Pin (implicit intermediate net created by tool)
  U1.Pin3 -> U2.PinX;

  // Connecting to ground
  U1.GND_Pin -> GND; // Assuming GND is a defined ground port/net

  // Connecting to power
  VCC_Net -> U1.VCC_Pin;
}
```

**Multi-Connections (Connecting to/from Multiple Pins):**

To reduce verbosity when connecting a single net to multiple pins (e.g., power/ground rails to multiple component pins, decoupling capacitors) or multiple pins to a single net, a comma-separated list of pins can be used.

```bhdl
connections {
  // Connect VCC_3V3 net to pin 1 of multiple decoupling capacitors
  VCC_3V3 -> C_DECOUP1.1, C_DECOUP2.1, C_DECOUP3.1;

  // Connect pin 2 of multiple decoupling capacitors to the GND net
  C_DECOUP1.2, C_DECOUP2.2, C_DECOUP3.2 -> GND;

  // Connect multiple MCU GPIO output pins to LED anode pins
  MCU.GPIO[0] -> LED1.A;
  MCU.GPIO[1] -> LED2.A;
  MCU.GPIO[2] -> LED3.A;
  // Can be shortened to:
  MCU.GPIO[0], MCU.GPIO[1], MCU.GPIO[2] -> LED1.A, LED2.A, LED3.A; // Requires pins on right match pins on left 1:1
  // No, the above is ambiguous. Stick to one-to-many or many-to-one:
  // Connect multiple output signals to a single input pin (e.g., logic gate input)
  // Signal_A -> LogicGate.IN1;
  // Signal_B -> LogicGate.IN1; // This implies a short, likely an error unless intended (e.g. wire-OR)
  // Better: Define intermediate nets if needed or use appropriate logic.

  // Multi-connection examples:
  net ControlBus: signal;
  ControlBus -> U1.ENABLE, U2.ENABLE, U3.CHIP_SELECT;

  net StatusFlags: signal;
  U1.READY_FLAG, U2.ERROR_FLAG, U3.DONE_FLAG -> StatusFlags; // Connecting multiple outputs to one net requires care (e.g., open-drain)
}
```

**Note on Repetitive Connections and Deprecated Syntax:**

For handling highly repetitive connection patterns, such as connecting large numbers of decoupling capacitors or wiring buses, the preferred methods are:

1.  **`generate for` Loops:** Use generate loops (Section 6.2) to iterate through component arrays or indices and define connections programmatically. This is the most structured and scalable approach.
2.  **Multi-Connections:** Use the comma-separated pin list syntax described above for simpler cases involving connecting one net to many pins or many pins to one net.

Previous conceptual or experimental syntaxes for series connections (`-[R1]->`), parallel connections (`<|C1|>`), complex multi-port connections (`{A,B}-[Comp]->{X,Y}`), or direct pin mapping in connections (`-[Comp(Type):PinMap]->`) are **deprecated and removed** from the standard specification. While aiming for conciseness, these syntaxes introduced ambiguity and complexity compared to the standard explicit pin-to-pin connections facilitated by `generate` loops and the simple multi-connection syntax.


// ... rest of the section (Bus Connections, etc.) ...