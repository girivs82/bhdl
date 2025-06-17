# Team Workflow Extensions for Circuit Flow Language

## 1. Common Pattern Shortcuts for Board Designers

### 1.1 Decoupling Capacitor Patterns

Board designers need intuitive ways to specify common decoupling patterns:

```cfl
// Current verbose approach
VCC -> C1(10µF).+ -> C2(1µF).+ -> C3(0.1µF).+ -> mcu.VDD;
C1.-, C2.-, C3.- -> GND;
VCC -> C4(10µF).+ -> C5(1µF).+ -> C6(0.1µF).+ -> mcu.VDDA;
C4.-, C5.-, C6.- -> GND;

// Proposed intuitive shortcuts
mcu.VDD <- decoupling_pattern(10µF + 1µF + 0.1µF) <- VCC;
mcu.VDDA <- decoupling_pattern(10µF + 1µF + 0.1µF) <- VCC;

// Or even simpler with common patterns
mcu.VDD <- standard_decoupling <- VCC;  // Uses 10µF + 0.1µF default
mcu.VDDA <- low_noise_decoupling <- VCC;  // Uses 10µF + 1µF + 0.1µF + 10nF

// Bulk decoupling with quantity syntax
power_rail <- bulk_decoupling(10x 1µF + 2x 0.1µF) <- VCC;

// Location-aware decoupling
mcu.VDD <- local_decoupling(0.1µF within 2mm) + 
           bulk_decoupling(10µF within 10mm) <- VCC;
```

### 1.2 Other Common Patterns

```cfl
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

## 2. Multi-File Team Workflow

### 2.1 File Structure for Team Collaboration

```
project/
├── system/
│   ├── system_architecture.cfl     # System architect's domain
│   ├── power_budget.cfl           # Power requirements
│   └── interface_definitions.cfl   # External interfaces
├── circuit/
│   ├── power_management.cfl       # Board designer's domain
│   ├── signal_processing.cfl      # Circuit implementations
│   └── support_circuits.cfl       # Reset, clocks, etc.
├── layout/
│   ├── physical_constraints.cfl   # Layout engineer's domain
│   ├── layer_stackup.cfl         # PCB stackup definition
│   └── component_placement.cfl    # Placement constraints
└── integration/
    └── main_board.cfl             # Integrates all files
```

### 2.2 System Architecture Level (system_architecture.cfl)

```cfl
// System Architect's high-level specification
system_spec STM32H7_System {
  metadata {
    author = "System Architecture Team";
    version = "1.0";
    target_cost = $25;
    target_power = 2W;
  }
  
  // High-level functional blocks
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
    
    user_interface: {
      status_leds: MultiColor_LED_Array;
      control_buttons: [Reset, Boot_Mode];
    };
  }
  
  // Data flow specifications
  system_flows {
    power_distribution:
      USB_Input |> PowerManagement |> [ProcessingCore, Memory, Peripherals];
    
    data_flows:
      ExternalMemory <-> ProcessingCore <-> Peripherals;
    
    control_flows:
      DebugInterface -> ProcessingCore;
      UserInterface <-> ProcessingCore;
  }
  
  // Performance requirements
  requirements {
    boot_time < 2s;
    power_consumption < 2W;
    operating_temp = -40°C to +85°C;
    emc_compliance = FCC_ClassB + CE;
  }
  
  // Auto-generated block diagram output
  generate_diagrams {
    block_diagram: "system_block_diagram.svg";
    power_tree: "power_distribution.svg";
    interface_diagram: "external_interfaces.svg";
  }
}
```

### 2.3 Board Designer Level (power_management.cfl)

```cfl
// Board Designer's circuit implementation
import "../system/system_architecture.cfl";

circuit_implementation PowerManagement {
  // Reference system requirements
  implements STM32H7_System.power_management;
  
  // Detailed circuit flows
  power_flows {
    // USB input processing
    usb_input: USB_CONNECTOR.VBUS |> 
               input_protection(overvoltage=6V, overcurrent=2A) |>
               input_filtering |>
               main_5v_rail;
    
    // Power rail generation
    rail_3v3: main_5v_rail |> 
              LinearRegulator(LM1117-3.3, dropout=1.2V) |>
              output_filtering(bulk=22µF, ceramic=0.1µF) |>
              rail_3v3_clean;
    
    rail_1v8: rail_3v3_clean |>
              LinearRegulator(LM1117-1.8, dropout=1.2V) |>
              low_noise_filtering |>
              rail_1v8_clean;
    
    rail_1v2: rail_3v3_clean |>
              SwitchingRegulator(efficiency_min=85%) |>
              switching_filter |>
              rail_1v2_clean;
  }
  
  // Load distribution
  load_distribution {
    rail_3v3_clean |> distribute_to([
      mcu_digital_supply(500mA),
      mcu_analog_supply(100mA),
      peripherals_supply(200mA),
      expansion_header_supply(300mA)
    ]);
    
    rail_1v8_clean |> distribute_to([
      mcu_io_supply(200mA),
      ddr_supply(300mA)
    ]);
    
    rail_1v2_clean |> distribute_to([
      mcu_core_supply(1.5A)
    ]);
  }
  
  // Decoupling strategy
  decoupling_strategy {
    mcu_digital_supply <- standard_decoupling + local_ceramic(0.1µF within 2mm);
    mcu_analog_supply <- low_noise_decoupling + ferrite_bead_isolation;
    mcu_core_supply <- bulk_decoupling(10x 1µF) + local_ceramic(4x 0.1µF);
    ddr_supply <- ddr_decoupling_pattern;
  }
  
  // Protection and monitoring
  protection {
    usb_input -> esd_protection(type=TVS_array);
    all_rails -> overcurrent_protection;
    rail_3v3_clean -> power_good_monitoring -> system_reset_logic;
  }
}
```

### 2.4 Layout Engineer Level (physical_constraints.cfl)

```cfl
// Layout Engineer's physical implementation
import "../circuit/power_management.cfl";
import "../circuit/signal_processing.cfl";

physical_implementation STM32H7_Layout {
  // Board specifications
  board_specs {
    size = 100mm x 80mm;
    layers = 4;
    thickness = 1.6mm;
    material = FR4;
    impedance_control = required;
  }
  
  // Component placement strategy
  placement_strategy {
    // Power management area
    place PowerManagement.components {
      area = rectangle(20mm, 15mm);
      location = corner(bottom_left, margin=5mm);
      keep_together = true;
    };
    
    // MCU and critical components
    place mcu at center(50mm, 40mm);
    place ddr_ram near mcu within 15mm;
    place main_crystal near mcu within 5mm {
      orientation = avoid_switching_nodes;
    };
    
    // Connector placement
    place USB_CONNECTOR at edge(top, center);
    place GPIO_HEADER at edge(right, center);
    place SWD_CONNECTOR at edge(top, 20mm);
  }
  
  // Layer stackup definition
  layer_stackup {
    layer1: signal + component_mounting;
    layer2: ground_plane(continuous);
    layer3: power_planes {
      rail_3v3: plane_area(60%);
      rail_1v8: plane_area(25%);
      rail_1v2: plane_area(15%);
    };
    layer4: signal + component_mounting;
  }
  
  // Critical routing constraints
  routing_constraints {
    // Power distribution
    route_power rail_3v3_clean {
      min_width = 0.5mm;
      via_size = 0.3mm;
      thermal_relief = disabled;
      plane_connection = direct;
    };
    
    // High-speed digital
    route_group ddr_interface {
      layer_assignment = [layer1, layer4];
      reference_plane = layer2;
      
      route_class address_control {
        impedance = 50Ω ± 10%;
        length_match = ±0.2mm;
        min_spacing = 0.15mm;
      };
      
      route_class data_signals {
        impedance = 50Ω ± 10%;
        length_match = ±0.1mm;
        byte_group_matching = ±0.05mm;
      };
      
      route_diff clock_signals {
        impedance = 100Ω ± 10%;
        length_match = ±0.05mm;
        via_count_max = 2;
      };
    };
    
    // Analog/sensitive routing
    route_group crystal_circuit {
      guard_traces = required;
      keepout_zone = 2mm;
      shield_with = ground_plane;
      layer_preference = layer1;
    };
    
    // USB differential pairs
    route_diff usb_dp, usb_dn {
      impedance = 90Ω ± 10%;
      length_match = ±0.1mm;
      spacing = 0.2mm;
      layer_preference = layer1;
    };
  }
  
  // Manufacturing constraints
  manufacturing {
    min_trace_width = 0.1mm;
    min_via_size = 0.2mm;
    min_drill_size = 0.15mm;
    solder_mask = green;
    silkscreen = white;
    finish = HASL_lead_free;
  }
  
  // Test and assembly
  test_points {
    add_testpoints_for rail_3v3_clean, rail_1v8_clean, rail_1v2_clean;
    add_testpoints_for critical_signals;
    testpoint_size = 1mm;
    testpoint_access = top_side;
  };
}
```

### 2.5 Integration File (main_board.cfl)

```cfl
// Main integration file - brings everything together
import "system/system_architecture.cfl";
import "circuit/power_management.cfl";
import "circuit/signal_processing.cfl";
import "layout/physical_constraints.cfl";

board STM32H7_DevBoard {
  // Integrate all specifications
  system_level: STM32H7_System;
  circuit_level: [PowerManagement, SignalProcessing, SupportCircuits];
  physical_level: STM32H7_Layout;
  
  // Final integration validation
  validation {
    verify_requirements STM32H7_System.requirements;
    verify_power_budget PowerManagement.load_distribution;
    verify_timing_constraints SignalProcessing.timing_requirements;
    verify_physical_constraints STM32H7_Layout.routing_constraints;
  }
  
  // Output generation
  generate_outputs {
    schematic: "STM32H7_DevBoard.pdf";
    netlist: "STM32H7_DevBoard.net";
    bom: "STM32H7_DevBoard_BOM.csv";
    layout_constraints: "STM32H7_DevBoard.rules";
    assembly_drawings: "STM32H7_Assembly.pdf";
    
    // Documentation
    design_review_package: "STM32H7_Design_Review.zip";
    manufacturing_package: "STM32H7_Manufacturing.zip";
  }
}
```

## 3. Tool Integration Benefits

### 3.1 Automatic Diagram Generation
- **System Level**: Block diagrams, power trees, interface diagrams
- **Circuit Level**: Schematic diagrams, signal flow diagrams
- **Physical Level**: Floorplan, routing guidelines, layer stackups

### 3.2 Team Coordination
- **Change Tracking**: Each file tracked independently in version control
- **Interface Contracts**: System specs define requirements that circuit/layout must meet
- **Validation**: Automatic checking that implementations meet specifications

### 3.3 Progressive Refinement
- System architect defines requirements → auto-generates templates
- Board designer fills in circuit details → auto-generates layout constraints
- Layout engineer adds physical constraints → validates against all requirements

This workflow enables true concurrent engineering where each team member works at their appropriate abstraction level while maintaining system coherence and automatic validation of requirements flow-down.