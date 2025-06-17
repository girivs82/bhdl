# Simplified Core BHDL Language Specification

## Design Philosophy: Minimal, Memorable, Composable

**Goal**: Core language small enough to fit on a single reference card, yet powerful enough for complex boards.

## 1. Core Language Constructs (Only 7!)

### 1.1 Component Instantiation
```bhdl
// Single pattern for all components
VCC -> Res(4.7kΩ).1 -> LED(red).A;
VCC -> supply: LinearReg(3.3V, 1A).IN;
```

### 1.2 Flow Specification  
```bhdl
// Universal flow operator
power_flow: USB_5V |> regulation |> distribution |> loads;
signal_flow: INPUT |> amplify(10x) |> filter(1kHz) |> OUTPUT;
```

### 1.3 Interface Declaration
```bhdl
// Bus interfaces as first-class objects
main_i2c: I2C(3.3V, 400kHz);
ddr_bus: DDR3(16bit, 800MHz);
```

### 1.4 Generate Loops
```bhdl
// For repetitive structures
generate for i in 0..7 {
  GPIO[i] -> LED(colors[i]).A;
}
```

### 1.5 Conditional Logic
```bhdl
// Universal conditional construct
if (condition) { actions } else { alternatives }
when (event) { response }
```

### 1.6 Module Definition
```bhdl
// Reusable patterns
module PowerSupply(input_v, output_v, current) {
  flow: INPUT |> regulation(output_v) |> filtering |> OUTPUT;
}
```

### 1.7 Constraint Declaration
```bhdl
// Physical and electrical constraints
constrain { placement, routing, timing, power }
```

## 2. Unified Syntax Patterns

### 2.1 Everything Uses Same Connection Syntax
```bhdl
// Components
VCC -> Res(1kΩ).1 -> LED(red).A;

// Interfaces  
mcu.i2c <-> sensor_bus <-> [temp, humidity];

// Flows
USB_INPUT |> protection |> regulation |> VCC_3V3;

// Power domains
VCC_3V3.enable() -> wait_for(stable) -> VCC_1V8.enable();
```

### 2.2 Everything Uses Same Parameter Syntax
```bhdl
// Components
Res(value=4.7kΩ, tolerance=5%, power=0.25W)
Cap(10µF, voltage=25V, type="X7R")

// Interfaces
I2C(voltage=3.3V, frequency=400kHz, pullups=4.7kΩ)

// Modules
PowerSupply(input=5V, output=3.3V, current=1A)

// Constraints
constrain(length<10mm, impedance=50Ω±10%)
```

### 2.3 Everything Uses Same Conditional Syntax
```bhdl
// Power sequencing
if (VCC_3V3.stable) { VCC_1V8.enable(); }

// Component selection
if (high_speed) { 
  level_shift using TXS0108E; 
} else { 
  level_shift using 74LVC1T45; 
}

// Generate conditions
generate for pin in gpio_pins {
  if (pin.usage == "led") { pin -> LED(green).A; }
}

// Flow conditions
power_flow: INPUT |> if (battery_mode) { 
  low_power_regulation 
} else { 
  high_performance_regulation 
} |> OUTPUT;
```

## 3. Simplified Power Management

### 3.1 Replace Complex Power Constructs with Simple Flow + Conditionals
```bhdl
// Instead of: power_up_sequence, enable_when, wait_for, etc.
// Use: flow + if + timing

power_flow: USB_5V |> 
  delay(10ms) |> VCC_3V3.enable() |>
  if (VCC_3V3.stable) { 
    [VCC_1V8.enable(), VCC_1V2.enable()] 
  } |>
  if (all_stable) { RESET.release(); };

// Power down is just reverse flow
power_down: RESET.assert() |> 
  VCC_1V2.disable() |> VCC_1V8.disable() |> 
  delay(50ms) |> VCC_3V3.disable();
```

### 3.2 Simple Power States
```bhdl
// Instead of complex state machines
power_modes {
  ACTIVE: all_rails_on;
  SLEEP: if (deep_sleep) { 
    [VCC_1V2.retention(), VCC_DDR.off()] 
  } else { 
    VCC_1V2.reduce(0.8V) 
  };
  OFF: all_rails_off;
}
```

## 4. Simplified Level Shifting

### 4.1 Automatic with Simple Override
```bhdl
// Automatic (95% of cases)
mcu.GPIO(3.3V) -> sensor.INT(1.8V);  // Auto level shift

// Manual override when needed
mcu.GPIO(3.3V) -> level_shift(type=TXS0108E) -> sensor.INT(1.8V);

// Conditional selection
mcu.GPIO(3.3V) -> level_shift(
  if (high_speed) { TXS0108E } else { 74LVC1T45 }
) -> sensor.INT(1.8V);
```

## 5. Simplified Interface Handling

### 5.1 Unified Interface Pattern
```bhdl
// Declare interface
main_i2c: I2C(3.3V, 400kHz);

// Connect components to interface
main_i2c <-> [mcu.i2c1, sensor1, sensor2];

// Cross-domain interface (auto level shift)
cross_i2c: I2C(from=3.3V, to=1.8V);
mcu.i2c1 <-> cross_i2c <-> sensor_1v8;
```

## 6. Simplified Generate Constructs

### 6.1 Single Generate Pattern for Everything
```bhdl
// GPIO connections
generate for i in 0..15 {
  GPIO[i] -> mcu.GPIO[i];
}

// DDR connections with conditions
generate for i in 0..15 {
  if (i < 8) {
    DDR.DQ[i] <-> mcu.DDR_DQ_BYTE0[i];
  } else {
    DDR.DQ[i] <-> mcu.DDR_DQ_BYTE1[i-8];
  }
}

// Power decoupling
generate for rail in [VCC_3V3, VCC_1V8, VCC_1V2] {
  rail -> Cap(10µF).+ -> Cap(0.1µF).+ -> load;
  Cap.-.all -> GND;
}
```

## 7. Unified Examples

### 7.1 Simple LED Circuit
```bhdl
board SimpleLED {
  VCC -> Res(330Ω).1 -> LED(red).A;
  LED.K -> GND;
}
```

### 7.2 Power Supply
```bhdl
board PowerSupply {
  power_flow: 
    USB_5V |> LinearReg(3.3V, 1A) |> 
    [bulk_cap: Cap(10µF), bypass: Cap(0.1µF)] |> VCC_3V3;
}
```

### 7.3 Microcontroller with DDR
```bhdl
board MCU_Board {
  // Power sequencing
  power_flow: USB_5V |> 
    delay(10ms) |> VCC_3V3.enable() |>
    if (VCC_3V3.stable) { VCC_1V8.enable() } |>
    if (all_stable) { RESET.release() };
  
  // Memory interface
  ddr_bus: DDR3(16bit, 800MHz);
  mcu.ddr <-> ddr_bus <-> ddr_ram;
  
  // Generate DDR power connections
  generate for pin in ddr_ram.power_pins {
    VCC_1V8 -> pin;
  }
  
  // Communication
  main_i2c: I2C(3.3V, 400kHz);
  mcu.i2c1 <-> main_i2c <-> [sensor1, sensor2];
  
  // Level shifting for external interface
  mcu.uart(3.3V) -> level_shift(if high_speed { TXS0108E }) -> 
                    console_header(5V);
}
```

### 7.4 Multi-File Team Workflow
```bhdl
// system_spec.bhdl (System Architect)
system SystemSpec {
  power_budget: 2W;
  interfaces: [USB, I2C, SPI, GPIO_Header];
  performance: 480MHz_ARM_Core + 512MB_DDR;
}

// board_implementation.bhdl (Board Designer)  
import "system_spec.bhdl";
board implements SystemSpec {
  power_flow: USB_5V |> regulation |> [3.3V, 1.8V, 1.2V];
  
  mcu: STM32H7(480MHz);
  memory: DDR3(512MB, 16bit);
  
  main_i2c: I2C(3.3V, 400kHz);
  mcu <-> main_i2c <-> sensors;
}

// constraints.bhdl (Layout Engineer)
import "board_implementation.bhdl";
constrain board {
  place mcu at center;
  place memory near mcu within 15mm;
  route ddr_bus { length_match=±0.1mm, impedance=50Ω };
}
```

## 8. Core Language Reference Card

```
BHDL Core Language (7 constructs)

1. COMPONENTS:     VCC -> Res(4.7kΩ).1 -> LED(red).A;
2. FLOWS:          INPUT |> amplify(10x) |> filter |> OUTPUT;  
3. INTERFACES:     main_i2c: I2C(3.3V, 400kHz);
4. GENERATE:       generate for i in 0..7 { GPIO[i] -> LED[i]; }
5. CONDITIONS:     if (condition) { action } else { alternative }
6. MODULES:        module Name(params) { implementation }
7. CONSTRAINTS:    constrain { placement, routing, timing }

Operators:  ->  <->  |>  <=>
Grouping:   []  {}  ()
Keywords:   if else when generate for in module constrain
```

## 9. Benefits of Simplified Language

### 9.1 Memorability
- **7 core constructs** vs 20+ specialized ones
- **Consistent syntax** across all constructs  
- **Single reference card** covers entire language

### 9.2 Composability
- **Same patterns** work for components, interfaces, power, etc.
- **Combine simple constructs** to handle complex scenarios
- **No special cases** or exception syntax

### 9.3 Learning Curve
- **Learn 7 patterns** instead of dozens
- **Transferable knowledge** between different design areas
- **Natural progression** from simple to complex designs

### 9.4 Tool Implementation
- **Fewer language constructs** = simpler parser/compiler
- **Consistent patterns** = easier tool development
- **Generic constructs** = more reusable code

## 10. Migration Strategy

### 10.1 Keep Power but Hide Complexity
```bhdl
// Simple syntax for common cases
VCC -> Res(4.7kΩ).1 -> LED(red).A;

// Advanced features available when needed
VCC -> current_limit: Res(4.7kΩ, tolerance=1%, power=0.5W, 
                          package="0805", placement=near(mcu)) 
    -> LED(red, current=20mA, package="0603").A;
```

### 10.2 Standard Library Hides Implementation
```bhdl
// Designer uses simple interface
power_supply: USB_to_3V3(efficiency=85%);

// Library handles complex implementation
module USB_to_3V3(efficiency) {
  // Complex switching regulator implementation
  // Automatic component selection
  // Protection circuits
  // etc.
}
```

## Result: Professional Power with Consumer Simplicity

**7 core constructs** handle everything from simple LED circuits to complex SoC boards. The language grows with the designer - simple projects use simple syntax, complex projects compose the same simple constructs in sophisticated ways.

**Learning curve**: Write first circuit in 5 minutes, master entire language in 2 hours.
**Power**: Express any board design concept with the same fundamental patterns.
**Adoption**: No cognitive overload, familiar patterns, immediate productivity.