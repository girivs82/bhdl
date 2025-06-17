# BHDL: Board Hardware Description Language
## Complete Specification v2.0

### Table of Contents
1. [Introduction](#1-introduction)
2. [Design Philosophy](#2-design-philosophy)
3. [Core Language Constructs](#3-core-language-constructs)
4. [Type System](#4-type-system)
5. [Component System](#5-component-system)
6. [Interface System](#6-interface-system)
7. [Power Management](#7-power-management)
8. [Level Shifting](#8-level-shifting)
9. [Physical Constraints](#9-physical-constraints)
10. [Multi-File Team Workflow](#10-multi-file-team-workflow)
11. [Standard Library](#11-standard-library)
12. [Complete Working Example](#12-complete-working-example)

---

## 1. Introduction

### 1.1 Purpose and Scope

BHDL (Board Hardware Description Language) is a domain-specific language for describing electronic circuit boards using a **circuit flow paradigm**. Unlike traditional HDLs designed for digital logic, BHDL captures the natural way board designers think about power distribution, signal flow, and component interconnection.

### 1.2 Key Innovations

- **Circuit Flow Paradigm**: Express designs as power/signal flows rather than structural hierarchies
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

### 3.1 Component Instantiation
```bhdl
// Universal pattern: source -> component(parameters) -> destination
VCC -> Res(4.7kΩ).1 -> LED(red).A;
USB_5V -> regulator: LinearReg(3.3V, 1A).IN;
```

### 3.2 Flow Specification
```bhdl
// Universal flow operator |> for any domain
power_flow: USB_5V |> protection |> regulation |> distribution;
signal_flow: INPUT |> amplify(10x) |> filter(1kHz) |> OUTPUT;
data_flow: sensors |> i2c_bus |> mcu |> processing;
```

### 3.3 Interface Declaration
```bhdl
// Bus interfaces as first-class objects
main_i2c: I2C(voltage=3.3V, frequency=400kHz);
ddr_bus: DDR3(width=16bit, speed=800MHz);
expansion: GPIO_Header(pins=40, pitch=2.54mm);
```

### 3.4 Generate Constructs
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

### 3.5 Conditional Logic
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

### 3.6 Module Definition
```bhdl
// Reusable patterns
module PowerSupply(input_voltage, output_voltage, current) {
  flow: INPUT |> regulation(output_voltage) |> filtering |> OUTPUT;
  
  implementation {
    if (switching_preferred) {
      regulator = SwitchingReg(efficiency_min=85%);
    } else {
      regulator = LinearReg(dropout_max=1.2V);
    }
  }
}
```

### 3.7 Constraint Declaration
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
// Named handles for multiple references
VCC -> current_sense: Res(0.1Ω).1 -> VOUT;
current_sense.2 -> current_monitor.INPUT;
current_sense.voltage_drop -> power_calculation;
```

---

## 6. Interface System

### 6.1 Standard Bus Interfaces

```bhdl
// Pre-defined interface types
interface_types {
  I2C(voltage, frequency, pullups);
  SPI(voltage, frequency, mode);
  UART(voltage, baud_rate, flow_control);
  DDR3(width, speed, voltage);
  USB2(speed, power_delivery);
  PCIe(lanes, generation);
  GPIO_Header(pins, pitch, assignment);
}
```

### 6.2 Interface Usage

```bhdl
// Declare interface instances
interfaces {
  main_i2c: I2C(voltage=3.3V, frequency=400kHz);
  high_speed_spi: SPI(voltage=1.8V, frequency=50MHz);
  memory_bus: DDR3(width=16bit, speed=800MHz);
}

// Connect components to interfaces
main_i2c <-> [mcu.i2c1, temp_sensor, humidity_sensor];
memory_bus <-> [mcu.ddr_controller, ddr_ram];

// Cross-domain interfaces (automatic level shifting)
cross_i2c: I2C(from=3.3V, to=1.8V);
mcu.i2c1 <-> cross_i2c <-> low_voltage_sensors;
```

### 6.3 Interface Generation

```bhdl
// Generate repetitive interface connections
generate ddr_connections {
  for byte in 0..1 {
    for bit in 0..7 {
      mcu.DDR_DQ[byte*8 + bit] <-> ddr_ram.DQ[byte*8 + bit];
    }
    mcu.DDR_DQS[byte] <-> ddr_ram.DQS[byte];
  }
  
  for i in 0..13 {
    mcu.DDR_A[i] -> ddr_ram.A[i];
  }
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

## 12. Language Reference

### 12.1 Operators

```bhdl
->    // Unidirectional connection
<->   // Bidirectional connection  
<=>   // Interface connection
|>    // Flow operator
[]    // Grouping/arrays
{}    // Code blocks
()    // Parameters
```

### 12.2 Keywords

```bhdl
// Core constructs
if else when generate for in module constrain

// Declarations  
board system circuit interface power_domain

// Types
signal power ground voltage current

// Modifiers
input output inout extends implements
```

### 12.3 Built-in Functions

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
```

This completes the comprehensive BHDL specification. The language provides a complete framework for board design while maintaining simplicity through its seven core constructs.