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

**Note:** While the original design explored implicit component creation, the revised specification emphasizes **explicit declaration** for improved clarity and parsing robustness. The "connection-first" *workflow* is now primarily supported by **IDE tooling** rather than language syntax allowing implicit creation.

**Tooling-Assisted Workflow:**

1.  **Sketch Connections:** In a BHDL-aware IDE, designers can still type connections involving potentially undeclared components or nets (e.g., `VIN -> R1.1;`).
2.  **IDE Assistance:** The IDE flags `R1` as undeclared and offers **code actions** (e.g., quick fixes, lightbulb suggestions) to:
    *   Automatically generate a basic `Resistor R1 {};` declaration in the `components` block.
    *   Automatically generate a `net VIN: signal;` declaration in the `nets` block (if `VIN` is also undeclared).
3.  **Refine Declarations:** The designer then refines the generated declarations by adding necessary parameters (`value = 1kOhm;`), types, etc.

**Example (Tooling-Assisted):**

```bhdl
// In connections block (designer types this first)
connections {
  VIN -> R1.1; // R1, VIN are initially flagged by IDE
  R1.1 -> C1.1; // C1 flagged
  C1.2 -> GND; // GND flagged (if not predefined)
  R1.2 -> LED1.A; // LED1 flagged
  LED1.K -> GND;
}

// IDE assists in generating these declarations:
nets { // Explicit net declarations required
  net VIN: power; // Assuming power type based on context or user choice
  net Net_R1_C1: signal; // Auto-generated or named net
  net Net_R1_LED: signal;
  net GND: ground; // Assuming ground is a predefined or declared net
}

components { // Explicit component declarations required
  Resistor R1 { value = 1kOhm; tolerance = 5pct; } // Designer fills in details
  Capacitor C1 { value = 10uF; voltage = 25Vdc; } // Designer fills in details
  LED LED1 { color = red; current = 20mA; } // Designer fills in details
}

// Refined connections using declared nets (optional but clearer)
connections {
  VIN -> R1.1;
  R1.1 -> Net_R1_C1; C1.1 -> Net_R1_C1; // Connecting both pins to the explicit net
  C1.2 -> GND;
  R1.2 -> Net_R1_LED; LED1.A -> Net_R1_LED;
  LED1.K -> GND;
  // OR simplified pin-to-pin if intermediate net name isn't needed
  // VIN -> R1.1;
  // R1.1 -> C1.1; // Implicit net between R1.1 and C1.1
  // C1.2 -> GND;
  // R1.2 -> LED1.A; // Implicit net between R1.2 and LED1.A
  // LED1.K -> GND;
}

```

This tooling-based approach offers similar advantages to the original concept:

1.  **Natural design flow:** Focus on connectivity first.
2.  **Rapid prototyping:** Sketch connections quickly.
3.  **Reduced context switching:** Generate declarations via code actions without manually navigating.
4.  **Gradual refinement:** Formalize declarations as needed.
5.  **Structural Clarity:** Maintains a clean, explicit structure in the final code, aiding parsing and readability.

### 1.5 Why a Text-Based Language for Board Design?

Many experienced board designers are highly proficient with graphical schematic capture tools. So, why introduce a text-based language like BHDL? BHDL is not intended to simply replicate graphical schematics in text, but rather to offer a different, complementary approach with distinct advantages, designed *specifically* with the board designer's workflow in mind:

1.  **Intuitive Capture at Design Speed:** Features like the **tooling-assisted connection-first workflow** (Section 1.4), integrated **physical units** (Section 2.3), and **circuit functions** (Section 3.5) are designed to let you capture circuit ideas directly, often mirroring how you might initially sketch or think about connectivity, without getting bogged down in graphical layout or premature component definition. The goal is to describe the *intent* quickly and naturally.
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

```bhdl
// Comments use C-style syntax
/* Multi-line comments
   are also supported */

// Top-level board definition uses { }
board BoardName {
  // Board contents (properties assigned with '=')
  author = "Design Team";

  // Blocks also use { }
  parameters {
    param1 = 10;
  }
  components { /* ... */ }
  nets { /* ... */ }
  connections { /* ... */ }
  // ... other blocks ...
}

// Module definition uses { }
module ModuleName {
  // Module contents
}

// Component definition uses { }
component ComponentName {
  // Component specifications (properties assigned with '=')
  value = 100uF;
}

// Interface definition uses { }
interface InterfaceName {
  // Interface contents
}

// Type definition uses { }
typedef TypeName {
  // Type properties assigned with '='
  type = signal;
  domain = digital;
}
```

**Reasoning**: The C-style syntax is familiar. Consistent use of `{}` for all blocks and `=` for all assignments simplifies parsing and improves readability by reducing syntactic variations.

### 2.2 Naming Conventions

- Names are case-sensitive
- Valid names start with a letter and can contain letters, numbers, and underscores
- Reserved keywords cannot be used as names

**Reasoning**: Consistent with most modern programming languages for familiarity.

### 2.3 Basic Data Types

```bhdl
// Properties assigned using '='
parameters {
  // Numeric types with units (using typable ASCII units)
  voltage = 3.3Vdc;       // DC voltage
  ac_voltage = 230Vac;    // AC voltage
  rms_voltage = 0.894Vrms; // RMS voltage
  current = 100mA;
  resistance = 4.7kOhm;    // Use kOhm instead of kΩ
  capacitance = 10uF;      // Use uF instead of µF
  inductance = 10uH;       // Use uH instead of µH
  frequency = 16MHz;
  time = 10ns;
  temperature = 85degC;    // Use degC instead of °C
  duty_cycle = 50pct;      // Use pct instead of %

  // Boolean type
  enable = true;
  active_low = false;

  // String type
  part_number = "LM317T";

  // Enumerations (Declaration - specific syntax TBD, example assumes predefined enum type 'PackageType')
  // package_type: PackageType = PackageType'SOIC8; // Example usage if enum type exists
  package_option = enum { SOIC8, TSSOP16, QFN32 }; // Inline enum definition (alternative)
  selected_package = package_option'SOIC8; // Selecting a value

  // Arrays/Lists
  capacitors = [10uF, 1uF, 100nF]; // Use uF/nF

  // Ranges (Used in types or constraints)
  // input_voltage_range = 5Vdc to 24Vdc; // Direct range assignment might be less common for simple parameters
  operating_temp = -40degC to 85degC;
  tolerance_pct = 5pct; // Use pct

  // Enum Value Literal (distinct from declaration)
  // state_value = StateType'Active; // Example: Assigning an enum value
}

// Type usage example within a pin definition
pins {
   // Assuming StateType exists
   // STATUS: out signal(StateType);
}
```

**Reasoning**: Including units directly in the syntax prevents unit conversion errors and makes the code more readable. Using standard ASCII characters (e.g., `Ohm`, `uF`, `degC`, `pct`, `Vdc`/`Vac`) enhances typability and portability across different systems. Advanced types like ranges directly express design constraints.

### 2.3.1 Component Specifications and Ratings

When specifying component ratings (such as voltage ratings for capacitors) within component definitions or instances, use the standard assignment syntax. Values are assumed to be minimum required values unless explicitly stated otherwise using comparison operators.

```bhdl
// Capacitor definition with 16V minimum DC voltage rating
component Capacitor {
  // Default/base properties
  pins { 1: inout signal; 2: inout signal; }
  // Default parameters
  value: capacitance = required; // Must be specified at instantiation
  voltage: voltage = 0Vdc; // Default voltage rating
}

// Instantiation of the Capacitor
components {
  Capacitor C1 {
    value = 100nF;
    voltage = 16Vdc;  // Minimum voltage rating for this instance
  }
}

// Deprecated Inline component syntax is removed. Use explicit declaration.
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

The core types are extended using the `typedef` mechanism to create domain-specific types with rich electrical characteristics. Use standard assignment syntax (`=`) within the block.

```bhdl
// Type definition syntax uses '='
typedef <TypeName> {
  type = <BaseType>;          // Mandatory: signal, power, or ground
  domain = <DomainName>;       // Optional: e.g., digital, analog, clock, differential
  // --- Common Electrical Properties ---
  voltage_high = voltage;      // For digital types
  voltage_low = voltage;
  threshold_high = voltage;
  threshold_low = voltage;
  impedance = resistance;
  bandwidth = frequency_range; // Example: 20Hz to 20kHz
  // --- Properties for Automation ---
  is_open_drain = false; // Optional: Indicates open-drain/collector output
  default_pullup = resistance; // Optional: Default pull-up for open-drain
  // ... other properties like rise_time, leakage, etc. ...
}

// Example: 3.3V CMOS type
typedef cmos_3v3 {
  type = signal; domain = digital;
  voltage_high = 3.3Vdc; voltage_low = 0Vdc;
  threshold_high = 2.0Vdc; threshold_low = 0.8Vdc;
  rise_time = <10ns; input_leakage = <1uA;
}

// Example: I2C signal type defining open-drain and default pull-up
typedef i2c_signal_3v3 {
  type = signal; domain = digital;
  voltage_high = 3.3Vdc; voltage_low = 0Vdc;
  threshold_high = 2.0Vdc; threshold_low = 0.8Vdc;
  is_open_drain = true;
  default_pullup = 4.7kOhm; // Default pull-up value for automation
  // Could add max capacitance, speed ratings etc.
}

// Type usage in pin definitions
pins {
  CLK: in signal(cmos_3v3);   // A 3.3V CMOS clock input
  SDA: inout signal(i2c_signal_3v3); // An I2C signal pin
}
```

**Reasoning**: The `typedef` mechanism provides a general-purpose way to define structured types without introducing specialized keywords for each domain. This keeps the language simpler while enabling rich type checking and domain-specific parameters.

#### 2.4.3 Pin Directions and Types

Pin direction is orthogonal to the pin type and specified separately. BHDL supports three pin directions: `in`, `out`, `inout`.

Directions are combined with types to fully specify a pin's electrical characteristics:

```bhdl
pins {
  // Different directions with the same signal type
  DATA_IN: in signal(cmos_3v3);
  DATA_OUT: out signal(cmos_3v3);
  DATA_BIDIR: inout signal(cmos_3v3);

  // Differential signals (use inout and rely on type properties)
  DIFF_P: inout signal(lvds); // Assumes 'lvds' type defined elsewhere
  DIFF_N: inout signal(lvds);

  // Power and ground
  VDD: in power(lv_digital_power); // Assumes 'lv_digital_power' type defined
  VOUT: out power(lv_digital); // Assumes 'lv_digital' type defined
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

The BHDL standard library includes predefined types for common domains (using `=` for assignment):
```bhdl
// Digital signal types in libraries/types.bhdl
typedef ttl {
  type = signal;
  domain = digital;
  voltage_high = 5.0Vdc;
  voltage_low = 0Vdc;
  threshold_high = 2.0Vdc;
  threshold_low = 0.8Vdc;
  rise_time = <22ns;
  fanout = 10;
}

typedef cmos_5v {
  type = signal;
  domain = digital;
  voltage_high = 5.0Vdc;
  voltage_low = 0Vdc;
  threshold_high = 3.5Vdc;
  threshold_low = 1.5Vdc;
  rise_time = <20ns;
  input_leakage = <1uA;
}

typedef cmos_3v3 {
  type = signal;
  domain = digital;
  voltage_high = 3.3Vdc;
  voltage_low = 0Vdc;
  threshold_high = 2.0Vdc;
  threshold_low = 0.8Vdc;
  rise_time = <10ns;
  input_leakage = <1uA;
}

typedef lvcmos_2v5 {
  type = signal;
  domain = digital;
  voltage_high = 2.5Vdc;
  voltage_low = 0Vdc;
  threshold_high = 1.7Vdc;
  threshold_low = 0.7Vdc;
  rise_time = <8ns;
  input_leakage = <1uA;
}

typedef lvcmos_1v8 {
  type = signal;
  domain = digital;
  voltage_high = 1.8Vdc;
  voltage_low = 0Vdc;
  threshold_high = 1.2Vdc;
  threshold_low = 0.6Vdc;
  rise_time = <5ns;
  input_leakage = <1uA;
}

typedef lvcmos_1v2 {
  type = signal;
  domain = digital;
  voltage_high = 1.2Vdc;
  voltage_low = 0Vdc;
  threshold_high = 0.8Vdc;
  threshold_low = 0.4Vdc;
  rise_time = <3ns;
  input_leakage = <1uA;
}

typedef lvds {
  type = signal;
  domain = differential; // More specific than just digital
  voltage_high = 1.4Vdc; // Typical high level
  voltage_low = 1.0Vdc;  // Typical low level
  differential = true;
  termination = 100Ohm;
  common_mode = 1.2Vdc;
  swing = 350mV;
  rise_time = <300ps;
}

// Audio Signal Types
typedef line_level {
  type = signal;
  domain = analog;
  voltage = 0.894Vrms;       // Consumer line level (-10dBV)
  impedance = 10kOhm;
  bandwidth = 20Hz to 20kHz;
}

// Power Types
typedef lv_digital_power { // Renamed for clarity
  type = power;
  voltage = 3.3Vdc;
  tolerance = 5pct; // Use pct
  ripple = <50mVpp; // Explicitly Peak-to-Peak
}

// Clock Types
typedef system_clock {
  type = signal;
  domain = clock;
  frequency = 100MHz;
  jitter = <100ps;
  duty_cycle = 50pct +/- 5pct; // Use pct and +/- syntax
  // Assuming clock uses standard logic levels, e.g., cmos_3v3
  // These can be specified directly or inherited from another type
  voltage_high = 3.3Vdc;
  voltage_low = 0Vdc;
}

// Thermal Types (Property Sets might be a better fit - see original spec)
// Define as property_set if not tied to pin types directly
property_set commercial_thermal {
  operating_temperature = 0degC to 70degC;
  storage_temperature = -40degC to 85degC;
}

property_set industrial_thermal {
  operating_temperature = -40degC to 85degC;
  storage_temperature = -55degC to 125degC;
}
```

#### 2.4.5 User-Defined Types

Users can define custom types for domain-specific needs (using `=`):

```bhdl
// In myproject/custom_types.bhdl
typedef automotive_signal {
  type = signal;
  domain = custom; // Or perhaps 'analog' or 'digital' depending on use
  voltage = 0Vdc to 5Vdc;
  impedance = 100Ohm;
  rise_time = <100ns;
  esd_protection = 15kV;
  reverse_voltage_protection = true;
}

typedef medical_power {
  type = power;
  voltage = 12Vdc;
  isolation = 4kV;
  leakage_current = <10uA;
  certifications = ["IEC 60601-1", "UL 60601-1"];
}
```

#### 2.4.6 Type Inheritance and Extension

Types can inherit and extend other types (using `extends` keyword, properties assigned with `=`):

```bhdl
// Extending a base type
typedef high_quality_line_level extends line_level {
  thd = <0.001pct;           // Total harmonic distortion (use pct)
  snr = >100dB;            // Signal-to-noise ratio
  crosstalk = <-80dB;      // Channel crosstalk
}

// Creating a variant
typedef battery_power extends lv_digital_power { // Using renamed type
  voltage = 3.7Vdc;        // Li-ion nominal voltage
  voltage_range = 3.0Vdc to 4.2Vdc;
  // Nested block for structured properties, still uses '='
  protection = {
    overcurrent = true;
    overvoltage = true;
    undervoltage = true;
  };
}
```

#### 2.4.7 Using Type Definitions

Types are used throughout BHDL (properties assigned with `=`):

```bhdl
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
      io_standard = cmos_3v3;  // Using type as a parameter
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

*(Note: This section largely duplicates 2.4.2-2.4.8 and can potentially be merged or removed for conciseness if 2.4 is sufficiently clear. The following retains the structure but updates syntax)*

BHDL provides a flexible type definition mechanism that allows both standard library types and user-defined types, using the `typedef` keyword and standard assignment (`=`).

```bhdl
// Type definition syntax uses '='
typedef line_level {
  type = signal;          // Base type category
  voltage = 0.894Vrms;    // Consumer line level (typical -10dBV)
  impedance = 10kOhm;
  bandwidth = 20Hz to 20kHz;
}

// Type usage
ports {
  AUDIO_IN: in signal(line_level);
}
```

**Reasoning**: The `typedef` mechanism provides a general-purpose way to define structured types. Consistent use of `=` simplifies parsing.

#### 2.5.1 Standard Library Types

Standard types are defined in the BHDL standard library and can be imported. Examples updated for `=` assignment:

```bhdl
// In libraries/types.bhdl
// Audio signal types
typedef line_level {
  type = signal;
  voltage = 0.894Vrms;       // Consumer line level (-10dBV)
  impedance = 10kOhm;
  bandwidth = 20Hz to 20kHz;
}

// ... other standard types updated similarly ...

// Power types
typedef lv_digital_power {
  type = power;
  voltage = 3.3Vdc;
  tolerance = 5pct;
  ripple = <50mVpp;
}

// ... other standard types updated similarly ...

// Digital interface types
typedef lvds {
  type = signal; // Clarified base type
  domain = differential;
  voltage_high = 1.4Vdc;
  voltage_low = 1.0Vdc;
  differential = true;
  termination = 100Ohm;
  rise_time = <300ps;
}

typedef cmos_3v3 {
  type = signal; // Clarified base type
  domain = digital;
  voltage_high = 3.3Vdc;
  voltage_low = 0Vdc;
  threshold_high = >2.0Vdc; // Or use threshold_high = 2.0Vdc and rely on tool interpretation
  threshold_low = <0.8Vdc; // Or use threshold_low = 0.8Vdc
  rise_time = <5ns;
}

// Clock types
typedef system_clock {
  type = signal; // Clarified base type
  domain = clock;
  frequency = 100MHz;
  jitter = <100ps;
  duty_cycle = 50pct +/- 5pct;
}

// Thermal types (as property sets)
property_set commercial_thermal {
  operating_temperature = 0degC to 70degC;
  storage_temperature = -40degC to 85degC;
}
```

#### 2.5.2 User-Defined Types

Users can define custom types using `=`:

```bhdl
// In myproject/custom_types.bhdl
typedef automotive_signal {
  type = signal;
  domain = custom;
  voltage = 0Vdc to 5Vdc;
  impedance = 100Ohm;
  rise_time = <100ns;
  esd_protection = 15kV;
  reverse_voltage_protection = true;
}

// ... other user types updated similarly ...

typedef adc_interface {
  type = signal; // Assuming analog signals are base type 'signal' with domain 'analog'
  domain = analog;
  voltage_range = 0Vdc to 3.3Vdc;
  resolution = 12bit;
  sampling_rate = 1MSPS;
  input_impedance = >1MOhm;
}
```

#### 2.5.3 Type Inheritance and Extension

Types inherit using `extends` and assign properties with `=`:

```bhdl
// Extending a base type
typedef high_quality_line_level extends line_level {
  thd = <0.001pct;
  snr = >100dB;
  crosstalk = <-80dB;
}

// Creating a variant
typedef battery_power extends lv_digital_power {
  voltage = 3.7Vdc;
  voltage_range = 3.0Vdc to 4.2Vdc;
  protection = { // Nested block uses '='
    overcurrent = true;
    overvoltage = true;
    undervoltage = true;
  };
}
```

#### 2.5.4 Using Type Definitions

Types are used throughout BHDL, assignments use `=`:

```bhdl
// Importing types
import libraries.types.{line_level, lv_digital_power, cmos_3v3, system_clock};
import myproject.custom_types.{automotive_signal};

// Using types in a module definition
module AudioInterface {
  ports {
    // Signal types
    AUDIO_IN_L: in signal(line_level);
    AUDIO_IN_R: in signal(line_level);
    AUDIO_OUT: out signal(pro_line_level); // Assumes pro_line_level is defined

    // Power types
    VDD: in power(lv_analog_power); // Assumes lv_analog_power is defined
    VDDA: in power(lv_analog_power);

    // Digital interface types
    SPI_MOSI: in signal(cmos_3v3); // Changed from 'digital' keyword if not a base type
    SPI_MISO: out signal(cmos_3v3);
    SPI_SCK: in signal(cmos_3v3);

    // Clock type
    CLK: in signal(system_clock); // Changed from 'clock' keyword if not a base type
  }

  // Internal components
  components {
    ADC U1 { // Assuming ADC is a defined component type
      input_type = line_level;      // Using type as a parameter value
      reference_voltage = 5Vdc;
      resolution = 24bit;
    }

    OpAmp U2 { // Assuming OpAmp is a defined component type
      supply_voltage_type = lv_analog_power; // Parameter name clarified
      gain_bandwidth = 10MHz;
      slew_rate = 10V/us; // Corrected unit
    }
  }
}

// Using types in connection constraints
// Uses the standard 'constrain' block (See Section 5.1)
constrain (AUDIO_IN -> U1.IN) { // Target the connection (syntax TBD) or the net
  match_impedance = true; // Ensures impedance matching based on types
  max_length = 50mm;
  shield = required;
}

// Using types in component selection (Conceptual - Tool-specific)
// component_selection {
//   filter = {
//     input_type = line_level;        // Filter by type compatibility
//     operating_temperature = commercial_thermal; // Reference property set
//   }
// }
```

// ... existing code ...

### 2.6 Generative Constructs (`generate for`)

*(Syntax remains largely the same, ensuring body is always enclosed in `{}` and using standard assignment `=` if properties are set within generated components)*

```bhdl
// Syntax reminder
generate for <variable> in <range_or_list> {
  // Pin definitions, component instantiations, or connection statements
  // Use '=' for any assignments within the block
}

// Usage within `pins` block:
component DDR_PHY {
  parameters {
    data_width = 64; // Use '='
  }
  pins {
    local num_bytes = data_width / 8; // Local calculation for clarity
    // Generate DQ pins using the data_width parameter
    generate for i in 0 to data_width-1 {
      DQ[i]: inout signal(ddr_dq_type); // Assumes ddr_dq_type defined
    }
    // Generate DQS pairs using the calculated num_bytes parameter
    generate for i in 0 to num_bytes-1 {
      DQS_P[i]: inout signal(ddr_dqs_type); // Assumes ddr_dqs_type defined
      DQS_N[i]: inout signal(ddr_dqs_type);
    }
    // ... other pins (ADDR, CMD, CLK etc.) ...
  }
  // ... component details ...
}

// Usage within `connections` block:
connections {
  // Assume CPU and PHY components declared with DQ[0..63] pins
  generate for i in 0 to 63 {
    CPU.DQ[i] -> PHY.DQ[i]; // Direct pin-to-pin connection
  }
}

// Usage within `components` block:
parameters { num_leds = 8; }
components {
  generate for i in 0 to num_leds-1 {
    // Use '=' for assignments in generated components
    Resistor R_LED[i] { value = 330Ohm; package = "0603"; }
  }
  // ... other components ...
}
```

// ... existing code ...

### 2.7 Bus Notation and Slicing

*(Syntax remains the same)*

// ... existing code ...

```bhdl
// Example: Byte Swizzling Connection using `generate for` and slicing
parameters {
  data_width = 64; // Use '='
  num_bytes = data_width / 8;
}
connections {
  // Assume MCU and PHY components and DATA nets/pins are declared
  generate for byte_idx in 0 to num_bytes-1 {
    // Calculate indices for MCU slice (standard byte order)
    local mcu_high = (byte_idx + 1) * 8 - 1;
    local mcu_low = byte_idx * 8;

    // Calculate indices for PHY slice (reversed byte order)
    local phy_byte_num = num_bytes - 1 - byte_idx;
    local phy_high = (phy_byte_num + 1) * 8 - 1;
    local phy_low = phy_byte_num * 8;

    // Connect the corresponding slices (Pin-to-Pin or via explicit Nets)
    MCU.DATA[mcu_high : mcu_low] -> PHY.DATA[phy_high : phy_low];
  }
}
```

### 2.8 Importing Libraries (`import`)

*(Syntax remains the same, example updated)*

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
    default_via_style = "StandardVia"; // Use '='
  }
  ports { SIGNAL_IN: in signal(cmos_3v3); }
  components { Resistor R1 { value = 10kOhm; } } // Use '='
  nets { net VDD: power(lv_digital_power); } // Declare nets explicitly
  connections { /* ... */ }
  constrain (VDD) { net_class = "PowerNetClass"; } // Use '='
}
```

// ... existing code ...

## 3. Board and Module Structure

### 3.1 Board Definition

```bhdl
board PowerSupply {
  // Metadata (use '=')
  author = "Design Team";
  version = "1.0";

  // Parameters (use '=')
  parameters {
    input_voltage = 12Vdc;
    output_voltage = 5Vdc;
    max_current = 2A; // Type specification less common here, inferred or checked by tool
  }

  // External ports (Syntax: name: direction type(optional_spec))
  ports {
    VIN: in power(12Vdc, 2A);  // Example: Type with implicit properties - Needs well-defined type 'power'
    GND: ground;
    VOUT: out power(5Vdc, 2A);
  }

  // ** Layer Stackup Definition ** (use '=')
  layer_stackup {
    layer TOP { type = signal; material = "Copper"; thickness = 0.035mm; };
    layer DIEL1 { type = dielectric; material = "FR4"; thickness = 1.5mm; epsilon_r = 4.5; };
    layer BOTTOM { type = signal; material = "Copper"; thickness = 0.035mm; };
    // More complex stackups would include plane layers, masks, silk, etc.
  }

  // ** Default Design Rules ** (use '=')
  default_design_rules {
    min_trace_width = 0.2mm;
    min_clearance = 0.2mm; // Default trace-trace, trace-pad, etc.
    min_via_drill = 0.3mm;
    min_via_pad_diameter = 0.6mm;
    min_annular_ring = 0.1mm;
    default_via_style = "StandardVia"; // References a defined via_style (See Sec 5.5)

    // Optional: Default net class assignments (syntax uses function-like call TBD)
    // assign_net_class("Power", nets_matching("VDD_*"));
  }

  // ** Component Instantiation ** (Mandatory block)
  components {
     // Instantiate components defined elsewhere (e.g., libraries or local definitions)
     VoltageRegulator U1 { output_voltage = 5Vdc; }; // Pass instance parameters using '='
     Resistor R1 { value = 1kOhm; };
     Capacitor C1 { value = 10uF; voltage = 16Vdc; };
     // ... other components
  }

  // ** Net Declarations ** (Mandatory block)
  nets {
     net VDD_IN: power(input_voltage); // Reference board parameter
     net VDD_OUT: power(output_voltage);
     net GND_NET: ground; // Explicit ground net
     net ENABLE_SIG: signal; // Generic signal
     // ... other nets
  }

  // ** Connections ** (Connect declared components using declared nets or pin-to-pin)
  connections {
     VIN -> VDD_IN; // Connect port to net
     VDD_IN -> U1.IN; // Connect net to component pin
     U1.OUT -> VDD_OUT; // Connect component pin to net
     VDD_OUT -> VOUT; // Connect net to port

     U1.ENABLE -> ENABLE_SIG;

     // Connect ground pins to the declared ground net
     U1.GND -> GND_NET;
     R1.2 -> GND_NET; // Example connection
     C1.2 -> GND_NET;
     GND -> GND_NET; // Connect ground port to ground net

     // Example pin-to-pin (implicitly creates anonymous net)
     // SomeOtherComponent.PIN_A -> R1.1;
  }

  // ** Constraints ** (Apply constraints to nets, pins, components, connections)
  constrain (VDD_OUT) { // Target a net
     net_class = "Power"; // Assign net class using '='
     max_ripple = 50mVpp;
  }
  // ... other blocks like placement_constraints ...
}
```

**Reasoning**: The board structure follows a declarative style. Explicit `components` and `nets` blocks enhance clarity and simplify parsing compared to implicit creation. Consistent use of `=` simplifies syntax.

### 3.2 Module Definition

```bhdl
module VoltageRegulator {
  // External interface
  ports {
    IN: in power(8Vdc to 35Vdc, 2A);
    OUT: out power(5Vdc, 1A);
    GND: ground;
    ENABLE: in signal; // Assume base 'signal' type if not specified
  }

  // Parameters (use '=')
  parameters {
    output_voltage = 5Vdc;
    max_current = 1A;
  }

  // Internal implementation (requires internal components, nets, connections)
  components {
     // Internal component instances, e.g., the regulator IC, passives
     RegulatorIC U_IC { /* ... */ };
     Resistor R_FB1 { value = 10kOhm; };
     // ...
  }
  nets {
     // Internal nets
     net FeedbackNet: signal;
     net InternalGND: ground;
     // ...
  }
  connections {
     // Connect ports to internal components/nets
     IN -> U_IC.VIN;
     ENABLE -> U_IC.EN;
     GND -> InternalGND; // Connect module ground port to internal ground net
     U_IC.GND -> InternalGND;
     R_FB1.2 -> InternalGND;

     // Internal connections
     U_IC.FB -> FeedbackNet; R_FB1.1 -> FeedbackNet; // Example feedback connection
     U_IC.VOUT -> OUT; // Connect internal IC output to module output port

     // ... other internal connections
  }
  // ... Optional internal constraints ...
}

### 3.2.1 Modules for Encapsulating Component Context
// ... (Example updated for '=' and explicit blocks) ...

```bhdl
// --- In a company library file ---
import StandardLibrary.Components.{Resistor};
import CompanyInternalLib.Components.{Base_IC}; // The raw IC component

// Module providing the IC with its mandatory pull-down
module Configured_IC {
  // Expose only the necessary pins/ports of the Base_IC
  ports {
    DATA_BUS: like Base_IC.DATA_BUS; // 'like' syntax might need refinement or replacement
    CONTROL_SIGNALS: like Base_IC.CONTROL_SIGNALS;
    POWER: like Base_IC.POWER;
    GND: ground;
  }

  // Pass through relevant parameters if needed
  parameters {
    speed_grade = "standard"; // Use '='
  }

  // Internal implementation
  components { // Explicit block
    Base_IC U1 { // Instantiate the raw IC
      speed = module.speed_grade; // Use '='
    }
    Resistor R_PULLDOWN { // The mandatory pull-down
      value = 10kOhm; // Use '='
      tolerance = 5pct; // Use '='
    }
  }
  nets { // Explicit block
      net ConfigNet: signal;
      net InternalGND: ground;
      // Potentially nets for DATA_BUS, CONTROL_SIGNALS, POWER if not directly passed through
  }
  connections { // Explicit block
    // Internal connection enforces the pull-down
    U1.CONFIG -> ConfigNet; R_PULLDOWN.1 -> ConfigNet;
    R_PULLDOWN.2 -> InternalGND;
    GND -> InternalGND; // Connect module port to internal net
    U1.GND -> InternalGND;

    // Connect exposed module ports to internal IC pins (using <=> for interface/bus assumed)
    // Or connect explicitly if not using interface operator
    DATA_BUS <=> U1.DATA_BUS; // Assumes operator works on port/pin groups/interfaces
    CONTROL_SIGNALS <=> U1.CONTROL_SIGNALS;
    POWER -> U1.POWER; // Assuming POWER is a simple port/pin here
  }
}

// --- In a board design file ---
board MySystem {
  // ... other components ...
  components { // Explicit block
    // Designers instantiate the configured module, not the base IC
    Configured_IC IC_Main {
      speed_grade = "high"; // Use '='
    }
    // ... other component instances
  }
  nets { // Explicit block
     net MAIN_DATA_BUS: bus( /* type? width? */ ); // Define bus net if needed
     // ... other nets
  }
  connections { // Explicit block
    // Connect to the ports of the Configured_IC module
    MAIN_DATA_BUS <=> IC_Main.DATA_BUS; // Assumes <=> connects declared net and module port group
    // ... other connections ...
  }
}
```

// ... existing code ...

### 3.4 Interface Definition and Connection

// ... (Interface definition uses '='. Component definition uses '=' and explicit `pin_map`) ...

// Simplified DDR Interface Definition Example
interface DDR_Interface (data_width = 64) { // Use '=' for default parameter value
   parameters {
     num_bytes = data_width / 8; // Use '='
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
     dw = 64; // Use '='
  }
  interfaces {
    // Instantiate interface, passing the width using '='
    MEM: interface DDR_Interface { data_width=dw; } // Instantiate using block + assignment
  }
  // ... other controller-specific pins ...
}

// Component representing the DDR PHY/Memory
component DDR_PHY {
  parameters {
    data_width = 64; // Use '='
  }
  interfaces {
     // Assuming PHY also uses the same interface definition for compatibility
     BUS: interface DDR_Interface { data_width=module.data_width; } // Use block + assignment
  }
  // ... other PHY pins ...
}

// Connecting interfaces with generated arrays
board TopLevel {
  components {
    DDR_Controller CPU { dw=64; }; // Use '=' for instance parameters
    DDR_PHY MEM_PHY { data_width=64; }; // Use '='
  }
  nets {
     // Nets might be implicitly created by interface connection or declared explicitly
  }
  connections {
    // The interface connection operator `<=>` implicitly connects all pins
    // by matching names within the interface definition.
    CPU.MEM <=> MEM_PHY.BUS;

    // Manual connection/swizzling still uses generate + slicing
    // generate for i in 0 to 63 { CPU.MEM.DQ[i] -> SOME_OTHER_DEVICE.DATA[i]; }
  }
}

// **Interfaces within Components (Pin Multiplexing)**
// Uses simplified 'functions' list property on pins and explicit 'pin_map' in interface instance.

```bhdl
// --- Example: Component with Multiplexed Pins via Interfaces ---
component ComplexSoC {
  // ... parameters ...

  // Define physical pins with optional function documentation
  pins {
     // Use comma separated properties after type, assign with '='
     P1_0: inout signal(cmos_3v3), functions = ["GPIO_1_0", "SPI1_MOSI", "UART0_TX", "I2C0_SDA"];
     P1_1: inout signal(cmos_3v3), functions = ["GPIO_1_1", "SPI1_MISO", "UART0_RX", "I2C0_SCL"];
     P1_2: inout signal(cmos_3v3), functions = ["GPIO_1_2", "SPI1_SCK"];
     P1_3: inout signal(cmos_3v3), functions = ["GPIO_1_3", "SPI1_CS"];
     UART1_TX: out signal(cmos_3v3); // Dedicated pin example
     UART1_RX: in signal(cmos_3v3); // Dedicated pin example
     // ... other pins ...
  }

  // Define available interfaces and map them to physical pins
  interfaces {
     // Instantiate an interface type (e.g., SPI) and provide a pin map
     SPI1: interface SPI { // Assumes SPI interface is defined elsewhere
        // Explicitly map logical interface pins to physical component pins
        pin_map = { MOSI = P1_0, MISO = P1_1, SCK = P1_2, CS = P1_3 };
        // Optionally override parameters like max_freq
        max_freq = 50MHz; // Use '='
     }
     UART0: interface UART { // Assumes UART interface is defined elsewhere
        // Map UART logical pins to SoC physical pins (Note: TX/RX share with SPI1)
        pin_map = { TX = P1_0, RX = P1_1 };
     }
     I2C0: interface I2C { // Assumes I2C interface is defined elsewhere
        // Map I2C logical pins to SoC physical pins (Note: SDA/SCL share with SPI1/UART0)
        pin_map = { SDA = P1_0, SCL = P1_1 };
     }
     UART1: interface UART { // Another UART on dedicated pins
         pin_map = { TX = UART1_TX, RX = UART1_RX };
     }
     // ... other interfaces like I2S, SDIO, GPIO_PortA ...
  }
  // ... package, footprint ...
}

// --- Example: Board using the SoC ---
board MuxDemoBoard {
   components {
      ComplexSoC U_SOC {}; // Use {} for empty parameters
      SPI_Flash U_FLASH {}; // Assumes SPI_Flash has SPI interface named 'SPI'
      I2C_Sensor U_SENSOR {}; // Assumes I2C_Sensor has I2C interface named 'I2C'
      UART_Header J_UART1 {}; // Assumes UART_Header has pins RX_PIN, TX_PIN
   }
   nets { // Define nets explicitly
      net UART1_TX_Net: signal(cmos_3v3);
      net UART1_RX_Net: signal(cmos_3v3);
      // Nets for SPI and I2C might be implicitly created by <=> or explicitly defined
   }
   connections {
      // Connect using the SPI1 interface instance on the SoC.
      // The <=> operator connects the interfaces based on the 'pin_map' in U_SOC.SPI1
      // and the implicit/explicit definition of the SPI interface on U_FLASH.
      // This implicitly selects the SPI function for pins P1_0, P1_1, P1_2, P1_3 on U_SOC.
      U_SOC.SPI1 <=> U_FLASH.SPI;

      // Connect to the I2C Sensor using the I2C0 interface.
      // **ERROR:** This connection attempts to map P1_0 (SDA) and P1_1 (SCL) again.
      // BHDL tools must detect this conflict based on the 'pin_map' definitions.
      // U_SOC.I2C0 <=> U_SENSOR.I2C; // <-- Expected validation error here.

      // Connect to the UART Header using UART1 (uses dedicated pins).
      // Explicit connection via nets.
      U_SOC.UART1.TX -> UART1_TX_Net; J_UART1.TX_PIN -> UART1_TX_Net; // Connect both to net
      UART1_RX_Net -> U_SOC.UART1.RX; UART1_RX_Net -> J_UART1.RX_PIN; // Connect both to net
      // Alternatively, use the <=> operator if J_UART1 exposes a UART interface.
      // U_SOC.UART1 <=> J_UART1.UART;

      // Direct pin connection (implies GPIO usage if not mapped by an active interface connection)
      // U_SOC.P1_0 -> LED_INDICATOR; // Connect P1_0 directly (if not used by SPI1/I2C0/UART0)
   }
}
```

**Validation:** ... BHDL tools are expected to track the usage of underlying physical pins based on the `pin_map` definitions in active interface connections. ...

**Reasoning**: ... Using interfaces within component definitions with an explicit `pin_map` provides a clear, structured way to manage pin multiplexing ...

// ... existing code ...

### 4.3 Pin Definitions in Components

Within a `component` definition, pins are declared in the `pins { ... }` block. Each pin definition specifies its name, direction (`in`, `out`, `inout`), electrical type (Section 2.4), and optional properties assigned using `=`.

```bhdl
component ExampleIC {
  pins {
    // Syntax: Name: direction type(optional_spec), property1 = value1, property2 = value2, ...
    VDD: in power(lv_digital_power);
    GND: ground;
    ENABLE: in signal(cmos_3v3);
    DATA_OUT: out signal(cmos_3v3);
    BIDIR_PIN: inout signal(cmos_1v8);
    RESET_N: in signal; // Assumes base 'signal' type if only direction is given
    CONFIG: in signal(cmos_3v3), pullup = true; // Example optional property
  }
}
```

**Optional Pin Function Documentation (for Multiplexed Pins):**

Use the `functions` property assigned to a list of strings to document potential roles.

```bhdl
component ComplexSoC {
  pins {
     P1_0: inout signal(cmos_3v3), functions = ["GPIO_1_0", "SPI1_MOSI", "UART0_TX", "I2C0_SDA"];
     P1_1: inout signal(cmos_3v3), functions = ["GPIO_1_1", "SPI1_MISO", "UART0_RX", "I2C0_SCL"];
     P1_2: inout signal(cmos_3v3), functions = ["GPIO_1_2", "SPI1_SCK"];
     P1_3: inout signal(cmos_3v3), functions = ["GPIO_1_3", "SPI1_CS"];
     // ... other pins ...
  }
  // ... interfaces map functions to these physical pins via 'pin_map' ...
}
```

// ... existing code ...

### 4.4 Component Population and Variant Management (SKUs)

// ... (Description of 'population' property and board parameters remains the same) ...

**Example:** (Updated for explicit blocks and `=`)

```bhdl
board MyVariantBoard (SKU = "Base") { // Use '=' for default parameter

  parameters { // Optional block if more parameters exist
     Region = "WW";
  }

  components { // Explicit component declaration block
    // --- Common Components ---
    Resistor R1 { value = 1k; } // Use '=', implicitly population = "Installed"

    // --- SKU-Specific Components ---
    OpAmp U_FeatureAmp {
      part_number = "OPA123"; // Use '='
      // Use standard ternary or if/else expression for conditional assignment
      population = (SKU == "Premium") ? "Installed" : "DNP";
    }
    DebugHeader J1 {
      connector_type = "2x5"; // Use '='
      population = (SKU != "Lite") ? "Installed" : "DNP";
    }
    Resistor R_Tuning {
      // Population is implicitly "Installed", value varies
      value = (SKU == "VariantA") ? 4.7k : 10k; // Use '='
    }
  }

  nets { // Explicit net declaration block
    net NetA: signal;
    net NetB: signal;
    net NetC: signal;
    net GND: ground; // Declare ground net
  }

  connections { // Explicit connection block
    // Connections remain defined; only assembly is affected by DNP
    NetA -> R1.1;
    R1.2 -> NetB;
    NetB -> U_FeatureAmp.IN_POS; // Connect even if U_FeatureAmp might be DNP
    U_FeatureAmp.OUT -> NetC;
    NetC -> R_Tuning.1;
    R_Tuning.2 -> GND;
  }
}

// Build process selects the active configuration:
// > bhdl_build MyVariantBoard --param SKU=Premium
// > bhdl_build MyVariantBoard --param SKU=Lite
```

// ... (Tooling, Workflow, Reviewability notes remain largely the same) ...

**Note on Workflow and Component Declaration:**

The specification **requires** all components and nets to be explicitly declared in the `components` and `nets` blocks, respectively, for clarity, consistency, and simpler parsing. The intended development workflow leverages IDE tooling. Developers can focus on defining connections first. Language Servers and IDE extensions are expected to provide features (e.g., code actions) that automatically generate the corresponding component and net declarations in the appropriate blocks when new identifiers are used in the `connections` block. This approach provides a fluid, sketch-like experience while maintaining the structural benefits of explicit declarations.

### 4.5 Nets and Connections Blocks (Revised)

This section details the mandatory `nets` block for defining logical connections and the `connections` block for specifying how component pins link to these nets.

**`nets` Block:**

All logical nets used to connect component pins must be explicitly declared within a `nets { ... }` block inside a `board` or `module`. This improves readability and simplifies parsing by providing a clear inventory of connections.

*   **Syntax:**
    ```bhdl
    nets {
      net <NetName>: <NetType>(optional_parameters);
      net <AnotherNetName>: <AnotherType>;
      net <BusName>[<range>]: <BusType>; // Example bus declaration
      // ...
    }
    ```
*   `<NetName>`: The unique identifier for the net within its scope.
*   `<NetType>`: Specifies the electrical characteristics, typically one of the core base types (`signal`, `power`, `ground`) or a user-defined `typedef` (e.g., `cmos_3v3`, `lv_digital_power`). Providing a type enables type checking during connection. If omitted, a generic `signal` type might be assumed by tools, but explicit typing is recommended.
*   `(optional_parameters)`: For `power` types or specific signal types, relevant parameters like voltage or current capacity can be included, matching the type definition.
*   Bus Declaration: Arrays of nets representing buses use standard index notation (e.g., `DATA[0:7]`).

**Example `nets` Block:**
```bhdl
nets {
  net SPI_MOSI: signal(cmos_3v3);
  net SPI_MISO: signal(cmos_3v3);
  net SPI_SCK: signal(cmos_3v3);
  net I2C_SDA: signal(i2c_signal_3v3); // Type includes open-drain info
  net VCC_3V3: power(3.3Vdc, 1A); // Power net with voltage/current spec
  net VCC_1V8: power(1.8Vdc);
  net AnalogInput: signal(analog_input_type); // User-defined analog type
  net DataBus[7:0]: signal(cmos_3v3); // 8-bit data bus
  net GND: ground; // Essential ground net declaration
}
```

**`connections` Block:**

The `connections { ... }` block defines how component instance pins are connected *to* the declared nets, or directly pin-to-pin (which implicitly creates an anonymous net).

*   **Syntax:**
    *   `NetName -> Pin1, Pin2, ...;` (Connects a declared net to one or more component pins)
    *   `Pin1, Pin2, ... -> NetName;` (Connects one or more component pins to a declared net)
    *   `Pin1 -> Pin2;` (Direct pin-to-pin connection)
    *   `Interface1 <=> Interface2;` (Connects all corresponding pins of two declared interfaces)
    *   `BusNet[slice] -> BusPin[slice];` (Connects buses or slices using declared nets/pins)

*   **Explicit Nets (Recommended):** Connecting pins explicitly to declared nets is the clearest method.
    ```bhdl
    connections {
      VCC_3V3 -> U1.VCC, U2.VCC, C1.1; // Connect VCC_3V3 net to multiple pins
      U1.TXD -> UART_TX_Net; // Connect U1.TXD pin to UART_TX_Net
      UART_RX_Net -> U1.RXD; // Connect UART_RX_Net to U1.RXD
      U1.GND, U2.GND, C1.2 -> GND; // Connect multiple pins to the GND net
    }
    ```

*   **Pin-to-Pin Connections:** Allowed for simple, direct connections. The tool implicitly understands there's a connection (net) between them. Avoid for complex routing or where constraints need to be applied to the net itself.
    ```bhdl
    connections {
       MCU.GPIO0 -> LED1.Anode; // Implicit net between GPIO0 and Anode
       SeriesResistor.1 -> SeriesResistor.2; // Connecting pins of the same component (less common)
    }
    ```

*   **Bus Connections:** Use declared bus nets or direct pin slicing.
    ```bhdl
    connections {
       CPU.DataBus[7:0] -> DataBus[7:0]; // Connect CPU pins to declared DataBus net
       DataBus[7:0] -> RAM.Data[7:0]; // Connect declared DataBus net to RAM pins
       // OR direct bus connection (if DataBus net declaration is omitted)
       // CPU.DataBus[7:0] -> RAM.Data[7:0];
    }
    ```

*   **Deprecated Syntax:** Multi-target connections (`PinA, PinB -> NetX, NetY;`) and specialized series/parallel syntax (`-[R1]->`, `<|C1|>`) are **removed** due to ambiguity and parsing complexity. Use `generate for` loops or simple one-to-many/many-to-one connections to explicit nets for repetitive patterns.

**Reasoning:** Explicit `nets` and simplified `connections` blocks make the design structure unambiguous, improve readability, facilitate easier parsing and analysis (like type checking), and align better with how connections are represented in traditional netlists. The tooling-assisted workflow mitigates the verbosity of explicit declarations during initial design sketching.

## 5. Physical Design Constraints

### 5.1 Connection Constraints

Constraints are applied using a `constrain` block, targeting specific declared nets, component pins, connections, or groups. Inline constraints are **removed**.

```bhdl
// Apply constraints using a 'constrain' block targeting declared elements
connections {
  // Assume nets MCU_CLK, MCU_DATA[0], MCU_DATA[1], MCU_USB_P, MCU_USB_N, etc.
  // and components MCU, Sensor, etc. are declared
  MCU.CLK_PIN -> MCU_CLK;
  MCU.DATA_PIN[0] -> MCU_DATA[0];
  MCU.DATA_PIN[1] -> MCU_DATA[1];
  MCU.USB_P_PIN -> MCU_USB_P;
  MCU.USB_N_PIN -> MCU_USB_N;
  // ... other connections
}

constrain (MCU_CLK) { // Target a specific NET
  max_length = 50mm; // Use '='
  impedance = 50Ohm +/- 5pct; // Use '='
  layer_preference = "TOP"; // Use '='
}

constrain (MCU_DATA[0], MCU_DATA[1]) { // Target multiple NETS
  impedance = 60Ohm +/- 10pct;
  group = "DataBus"; // Assign to a routing group
}

constrain (MCU_USB_P, MCU_USB_N) { // Target differential pair NETS
  type = differential_pair; // Property indicating pair relationship
  impedance = 90Ohm +/- 5pct;
  length_match_tolerance = 1mm;
  max_length = 25mm;
  primary_layer = "SignalLayer1";
  gap = 0.2mm; // Target spacing for the pair
}

// ** Constraints for Automation (Targeting Nets or Connections) **

// Constraining an I2C connection to override default pull-up
// Assume I2C_SDA, I2C_SCL nets are declared with type i2c_signal_3v3 (default 4.7k pull-up)
constrain (I2C_SDA) { // Target the net
   pullup_resistance = 2.2kOhm; // Override default pull-up for this specific net
   // auto_pullup = true; // Often implied by providing pullup_resistance
}
constrain (I2C_SCL) {
   pullup_resistance = 2.2kOhm; // Override for SCL too
}

// Example: Disabling auto level shift on a specific connection
// Target the specific connection (Syntax for connection targeting TBD, using pins for now)
// constrain (connection(MCU_3V3.TX -> FPGA_1V8.RX)) { // Conceptual connection targeting
// For now, apply to the net or pins involved if connection targeting not supported
constrain (Net_TX_to_RX) { // Assuming Net_TX_to_RX is the declared net
    auto_level_shift = false; // Disable automatic insertion on this net
}

// Example: Disabling automatic pull-up for an open-drain signal net
// Assume ALERT_Net is declared with a type having is_open_drain = true
constrain (ALERT_Net) {
    auto_pullup = false; // Disable automatic pull-up for this net
}

// Constraint groups (Conceptual, syntax may vary)
// group DataBus { ... }
// constrain group DataBus { length_match_tolerance = 0.5mm; }
```

**Common Connection Constraints:** (List remains mostly the same, assigned using `=`)
*   `impedance = 50Ohm`
*   `max_length`, `min_length`
*   `length_match_tolerance`
*   `max_skew`
*   `type = differential_pair` (used with `gap`, `impedance`, `length_match_tolerance`)
*   `gap = 0.2mm` (Spacing within a differential pair)
*   `layer`, `layer_preference`, `avoid_layers`
*   `shielding`, `guard_trace`
*   `group`
*   `topology`
*   `max_via_count`
*   `net_class = "ClassName"` (Assigns net(s) to a Net Class, Section 5.4)
*   `via_style = "StyleName"` (Specifies Via Style, Section 5.5)
*   `pullup_resistance` (Specifies desired pull-up value for automation)
*   `pulldown_resistance` (Specifies desired pull-down value)
*   `auto_pullup = boolean` (Explicitly enable/disable auto pull-up)
*   `auto_pulldown = boolean` (Explicitly enable/disable auto pull-down)
*   `auto_level_shift = boolean` (Explicitly enable/disable auto level shifter)

### 5.2 Placement Constraints
// ... existing code ...
```

**Reasoning:** Building validation rules directly into the language's expectation enables the development of powerful analysis tools that can catch common design errors early, significantly improving design reliability and reducing debugging time compared to purely graphical schematic capture where such checks are often less integrated or comprehensive.

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