# Circuit Flow Language (CFL) Specification

## Comprehensive Example: STM32H7 Development Board

This example demonstrates all language constructs using a realistic microcontroller board design.

## System Architecture Level

```cfl
system STM32H7_DevBoard {
  // System-level data flow specification
  power_distribution: 
    USB_5V |> power_management |> [VDD_3V3, VDD_1V8, VDD_1V2_CORE] |> soc_power_domains;
  
  memory_system:
    SoC |> memory_controller |> [DDR3_RAM, QSPI_FLASH, I2C_EEPROM];
  
  communication_flows:
    SoC <=> [USB_Interface, Debug_Interface, UART_Console, Expansion_Headers];
  
  clock_distribution:
    [HSE_Crystal, LSE_Crystal] |> SoC |> internal_plls |> system_clocks;
  
  control_signals:
    [Reset_Circuit, Boot_Config, User_Interfaces] <=> SoC;
}
```

## Module Definition Level (Reusable Patterns)

```cfl
// Power supply module
module LinearPowerSupply {
  parameters {
    input_voltage: voltage;
    output_voltage: voltage;
    max_current: current;
    dropout_voltage: voltage = 0.5V;
  }
  
  flow_definition {
    power_path: INPUT |> regulation |> filtering |> OUTPUT;
    feedback_path: OUTPUT |> voltage_sense |> regulation.control;
  }
  
  implementation {
    regulation using LinearRegulator {
      type = auto_select(output_voltage, max_current);
      dropout = dropout_voltage;
    };
    filtering using power_filtering_pattern {
      input_cap = Cap(10µF, input_voltage * 1.2);
      output_caps = [Cap(22µF), Cap(100nF)];
    };
  }
  
  ports {
    INPUT: in power(input_voltage);
    OUTPUT: out power(output_voltage, max_current);
    GND: ground;
    ENABLE: in signal(optional);
  }
}

// DDR3 interface module with generate constructs
module DDR3_Interface {
  parameters {
    data_width: integer = 16;
    address_width: integer = 14;
    bank_width: integer = 3;
  }
  
  // Generate construct for repetitive signals
  generate data_signals for i in 0..data_width-1 {
    DQ[i]: inout ddr_signal(1.5V, differential=false);
    DQS[i/8]: inout ddr_signal(1.5V, differential=true);  // DQS per byte
  }
  
  generate address_signals for i in 0..address_width-1 {
    A[i]: out ddr_signal(1.5V);
  }
  
  generate bank_signals for i in 0..bank_width-1 {
    BA[i]: out ddr_signal(1.5V);
  }
  
  // Control signals
  ports {
    CLK: out ddr_clock(1.5V, differential=true);
    CKE: out ddr_signal(1.5V);
    RAS_N, CAS_N, WE_N: out ddr_signal(1.5V);
    CS_N: out ddr_signal(1.5V);
    ODT: out ddr_signal(1.5V);
    RESET_N: out ddr_signal(1.5V);
    
    // Power
    VDD: in power(1.5V);
    VDDQ: in power(1.5V);
    VSS: ground;
  }
  
  constraints {
    // DDR3 specific timing and layout constraints
    route_group data_signals {
      length_match = ±0.1mm;
      impedance = 50Ω ± 10%;
      layer_assignment = [3, 4];  // Inner layers
    };
    
    route_group address_signals {
      length_match = ±0.2mm;
      impedance = 50Ω ± 10%;
    };
    
    route_diff CLK {
      impedance = 100Ω ± 10%;
      length_match = ±0.05mm;
      via_count_max = 2;
    };
  }
}

// Crystal oscillator module
module CrystalOscillator {
  parameters {
    frequency: frequency;
    load_capacitance: capacitance = 18pF;
    drive_level: power = 100µW;
  }
  
  flow_definition {
    oscillation_loop: 
      OSC_IN |> crystal_resonance |> OSC_OUT |> 
      internal_feedback |> OSC_IN;
  }
  
  implementation {
    crystal_resonance using Crystal {
      frequency = frequency;
      load_cap = load_capacitance;
      package = auto_select(frequency);
    };
    
    load_caps using generate for pin in [OSC_IN, OSC_OUT] {
      pin -> Cap(load_capacitance).1;
      Cap.2 -> GND;
    };
  }
  
  ports {
    OSC_IN: in analog_signal;
    OSC_OUT: out analog_signal;
    GND: ground;
  }
}
```

## Board Implementation - Mixed Abstraction Levels

```cfl
board STM32H7_DevBoard {
  author = "BHDL Team";
  version = "2.0";
  description = "STM32H7 development board with DDR3, USB, and expansion";
  
  // Board-level parameters
  parameters {
    main_supply = 5V;
    core_frequency = 480MHz;
    ddr_frequency = 200MHz;
    usb_enabled = true;
  }
  
  // External connectors
  ports {
    // Power input
    USB_CONNECTOR: usb_type_c(power_delivery=false);
    BARREL_JACK: power_connector(5V, 2A);
    
    // Expansion headers
    GPIO_HEADER: expansion_header(40_pin, 2.54mm);
    ARDUINO_SHIELD: arduino_uno_compatible;
    
    // Debug interface
    SWD_CONNECTOR: cortex_debug_connector(10_pin);
    
    // Communication
    UART_HEADER: serial_connector(3.3V);
  }
  
  // System-level flow implementation
  power_flows {
    // High-level power management (tool selects implementation)
    main_power: 
      [USB_CONNECTOR.VBUS, BARREL_JACK.VIN] |> 
      power_selection |> 
      power_distribution(3.3V, 1.8V, 1.2V);
    
    // Specific implementation for critical rails
    implement power_distribution.3V3 using LinearPowerSupply {
      input_voltage = 5V;
      output_voltage = 3.3V;
      max_current = 1A;
    } as supply_3v3;
    
    implement power_distribution.1V8 using LinearPowerSupply {
      input_voltage = 3.3V;
      output_voltage = 1.8V;
      max_current = 500mA;
    } as supply_1v8;
    
    // Let tool optimize this one
    implement power_distribution.1V2 using efficient_regulator {
      input_voltage = 3.3V;
      output_voltage = 1.2V;
      max_current = 2A;
      efficiency_min = 85%;
    };
  }
  
  // Main components
  components {
    // SoC with detailed configuration
    mcu: STM32H743VIT6 {
      package = LQFP100;
      speed_grade = 480MHz;
      
      // Power domain mapping
      VDD -> supply_3v3.OUTPUT;
      VDDA -> supply_3v3.OUTPUT;  // Analog supply
      VDD12 -> supply_1v2.OUTPUT; // Core supply
      VDD18 -> supply_1v8.OUTPUT; // I/O supply
    };
    
    // Memory components
    ddr_ram: DDR3_SDRAM {
      size = 512MB;
      speed = DDR3_1600;
      organization = 16bit;
      package = BGA96;
    };
    
    flash_memory: QSPI_Flash {
      size = 32MB;
      speed = 104MHz;
      package = SOIC8;
    };
    
    config_eeprom: I2C_EEPROM {
      size = 4KB;
      address = 0x50;
      package = SOIC8;
    };
  }
  
  // Clock system - pattern level
  clock_system {
    // High-frequency main clock
    main_oscillator: CrystalOscillator {
      frequency = 25MHz;
      load_capacitance = 18pF;
    };
    
    // Low-frequency RTC clock  
    rtc_oscillator: CrystalOscillator {
      frequency = 32.768kHz;
      load_capacitance = 12.5pF;
    };
    
    // Connect to MCU
    main_oscillator.OSC_IN <-> mcu.OSC_IN;
    main_oscillator.OSC_OUT <-> mcu.OSC_OUT;
    rtc_oscillator.OSC_IN <-> mcu.RTC_OSC_IN;
    rtc_oscillator.OSC_OUT <-> mcu.RTC_OSC_OUT;
  }
  
  // DDR3 interface - using generate constructs
  memory_interface {
    ddr_controller: DDR3_Interface {
      data_width = 16;
      address_width = 14;
    };
    
    // Generate connections between MCU and DDR controller
    generate ddr_connections {
      // Data signals with byte lane mapping
      for byte in 0..1 {
        for bit in 0..7 {
          mcu.DDR_DQ[byte*8 + bit] <-> ddr_controller.DQ[byte*8 + bit];
        }
        mcu.DDR_DQS[byte] <-> ddr_controller.DQS[byte];
        mcu.DDR_DQS_N[byte] <-> ddr_controller.DQS_N[byte];
      }
      
      // Address and control signals
      for i in 0..13 {
        mcu.DDR_A[i] -> ddr_controller.A[i];
      }
      
      for i in 0..2 {
        mcu.DDR_BA[i] -> ddr_controller.BA[i];
      }
      
      // Control signals
      mcu.DDR_CLK -> ddr_controller.CLK;
      mcu.DDR_CLK_N -> ddr_controller.CLK_N;
      mcu.DDR_CKE -> ddr_controller.CKE;
      mcu.DDR_RAS_N -> ddr_controller.RAS_N;
      mcu.DDR_CAS_N -> ddr_controller.CAS_N;
      mcu.DDR_WE_N -> ddr_controller.WE_N;
      mcu.DDR_CS_N -> ddr_controller.CS_N;
      mcu.DDR_ODT -> ddr_controller.ODT;
    }
    
    // Connect DDR controller to actual DDR chip
    ddr_controller <=> ddr_ram;
    
    // DDR power supplies
    supply_1v8.OUTPUT -> ddr_ram.VDD, ddr_ram.VDDQ;
  }
  
  // Communication interfaces - mixed level specification
  communication {
    // USB - high level pattern
    usb_interface: USB2_FullSpeed {
      connector = USB_CONNECTOR;
      esd_protection = true;
      termination_resistors = internal;  // Use MCU internal
    };
    
    usb_interface.DP <-> mcu.USB_DP;
    usb_interface.DN <-> mcu.USB_DN;
    
    // UART - component level detail
    uart_console: {
      // Explicit component specification
      VCC -> Res(10kΩ).1 -> mcu.UART1_TX;
      mcu.UART1_RX -> Res(10kΩ).1 -> UART_HEADER.RX;
      mcu.UART1_TX -> UART_HEADER.TX;
      UART_HEADER.GND -> GND;
      UART_HEADER.VCC -> supply_3v3.OUTPUT;
    };
    
    // I2C - pattern level
    i2c_bus: I2C_Bus {
      pullup_voltage = 3.3V;
      pullup_resistors = 4.7kΩ;
      max_frequency = 400kHz;
    };
    
    mcu.I2C1_SCL <-> i2c_bus.SCL;
    mcu.I2C1_SDA <-> i2c_bus.SDA;
    config_eeprom.SCL <-> i2c_bus.SCL;
    config_eeprom.SDA <-> i2c_bus.SDA;
  }
  
  // GPIO expansion - generate construct
  gpio_expansion {
    // Generate connections for GPIO header
    generate gpio_header_connections {
      // Power pins
      GPIO_HEADER.VCC_5V -> USB_CONNECTOR.VBUS;
      GPIO_HEADER.VCC_3V3 -> supply_3v3.OUTPUT;
      GPIO_HEADER.GND[1:4] -> GND;
      
      // GPIO pins with automatic assignment
      for i in 0..15 {
        GPIO_HEADER.GPIO[i] <-> mcu.auto_assign_gpio();
      }
      
      // Dedicated protocol pins
      GPIO_HEADER.SPI_MOSI <-> mcu.SPI1_MOSI;
      GPIO_HEADER.SPI_MISO <-> mcu.SPI1_MISO;
      GPIO_HEADER.SPI_SCK <-> mcu.SPI1_SCK;
      GPIO_HEADER.SPI_CS <-> mcu.SPI1_CS;
      
      GPIO_HEADER.I2C_SCL <-> mcu.I2C2_SCL;
      GPIO_HEADER.I2C_SDA <-> mcu.I2C2_SDA;
    }
  }
  
  // Support circuits - mixed abstraction
  support_circuits {
    // Reset circuit - pattern level
    reset_system: PowerOnReset {
      reset_delay = 100ms;
      brown_out_threshold = 2.8V;
      external_reset = true;
    };
    
    reset_system.VCC -> supply_3v3.OUTPUT;
    reset_system.RESET_OUT -> mcu.RESET_N;
    reset_system.RESET_IN -> GPIO_HEADER.RESET_BTN;
    
    // Boot configuration - component level
    boot_config: {
      // Boot mode selection
      supply_3v3.OUTPUT -> boot_res: Res(10kΩ).1 -> mcu.BOOT0;
      boot_res.2 -> boot_switch: Switch(SPDT).COM;
      boot_switch.NO -> GND;        // Normal boot
      boot_switch.NC -> supply_3v3.OUTPUT;  // DFU boot
    };
    
    // Status LEDs - generate pattern
    generate status_leds for (color, pin) in [(green, "PA5"), (red, "PA6"), (blue, "PA7")] {
      mcu.gpio(pin) -> Res(330Ω).1 -> LED(color, 2mA).A;
      LED.K -> GND;
    };
  }
  
  // Debug interface - standard pattern
  debug_interface: SWD_Debug {
    connector = SWD_CONNECTOR;
    target_voltage = 3.3V;
    esd_protection = true;
  };
  
  debug_interface.SWDIO <-> mcu.SWDIO;
  debug_interface.SWCLK <-> mcu.SWCLK;
  debug_interface.RESET <-> mcu.RESET_N;
  debug_interface.VDD -> supply_3v3.OUTPUT;
  
  // Physical constraints and layout guidance
  constraints {
    // Power supply placement
    place_near supply_3v3, supply_1v8, supply_1v2 {
      group = power_management;
      area = rectangle(10mm, 15mm);
      location = edge(left, 5mm);
    };
    
    // Crystal placement critical
    place main_oscillator.crystal near mcu within 5mm;
    place rtc_oscillator.crystal near mcu within 3mm;
    
    // DDR routing constraints
    route_group ddr_interface {
      layer_stack = [signal1, gnd, signal2, power];
      reference_plane = gnd;
      via_stitching = 2mm_spacing;
    };
    
    // High-speed signal routing
    route_diff usb_interface.DP, usb_interface.DN {
      impedance = 90Ω ± 10%;
      length_match = ±0.1mm;
      spacing = 0.2mm;
    };
    
    // Power distribution
    route_power supply_3v3.OUTPUT {
      min_width = 0.5mm;
      via_size = 0.3mm;
      thermal_relief = false;
    };
  }
}
```

## Key Language Features Demonstrated

### 1. **Multiple Abstraction Levels**
- **System Level**: Power flows, communication flows
- **Pattern Level**: Reusable modules (power supplies, interfaces)  
- **Component Level**: Explicit component connections

### 2. **Generate Constructs**
```cfl
generate gpio_connections for i in 0..15 {
  GPIO_HEADER.GPIO[i] <-> mcu.auto_assign_gpio();
}

generate status_leds for (color, pin) in [(green, "PA5"), (red, "PA6")] {
  mcu.gpio(pin) -> Res(330Ω).1 -> LED(color).A;
}
```

### 3. **Hierarchical Modules**
- Reusable patterns: `LinearPowerSupply`, `CrystalOscillator`, `DDR3_Interface`
- Parameter customization
- Interface compatibility checking

### 4. **Flow-Based Thinking**
```cfl
power_path: INPUT |> regulation |> filtering |> OUTPUT;
oscillation_loop: OSC_IN |> crystal_resonance |> OSC_OUT |> feedback |> OSC_IN;
```

### 5. **Flexible Implementation**
```cfl
// High-level specification
implement power_distribution.3V3 using LinearPowerSupply { ... };

// Component-level detail  
VCC -> Res(330Ω).1 -> LED(red).A;

// Tool optimization
implement power_distribution.1V2 using efficient_regulator { efficiency_min = 85%; };
```

This approach allows:
- **Beginners**: Use high-level patterns and flows
- **Experts**: Drop to component level for critical circuits
- **Tool Optimization**: Let tools handle routine implementations
- **Reusability**: Standard modules across projects

The same board can be specified at any abstraction level, giving designers complete flexibility while maintaining consistency and reusability.