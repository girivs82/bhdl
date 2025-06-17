# Level Shifting and Power Domain Isolation Specification

## 1. The Level Shifting Problem

Modern designs have multiple voltage domains (3.3V, 1.8V, 1.2V, etc.) and signals must cross between them safely. Manual level shifter design is error-prone and fails to handle:

- **Voltage translation** between different logic levels
- **Back-drive prevention** when one domain powers down
- **Direction control** for bidirectional signals
- **Speed/timing** requirements for high-frequency signals
- **Power sequencing dependencies** between domains

## 2. Power Domain-Aware Signal Types

### 2.1 Domain-Qualified Signal Types

```cfl
// Signals are qualified by their power domain
signal_types {
  // Standard CMOS signals with domain qualification
  logic_3v3: digital_signal(domain=VCC_3V3, levels=[0V, 3.3V]);
  logic_1v8: digital_signal(domain=VCC_1V8, levels=[0V, 1.8V]);
  logic_1v2: digital_signal(domain=VCC_1V2, levels=[0V, 1.2V]);
  
  // Special protocol signals with domain awareness
  i2c_3v3: i2c_signal(domain=VCC_3V3, open_drain=true);
  i2c_1v8: i2c_signal(domain=VCC_1V8, open_drain=true);
  
  spi_3v3: spi_signal(domain=VCC_3V3, drive_strength=4mA);
  spi_1v8: spi_signal(domain=VCC_1V8, drive_strength=2mA);
  
  // High-speed differential signals
  lvds_1v8: differential_signal(domain=VCC_1V8, common_mode=1.2V);
  lvpecl_3v3: differential_signal(domain=VCC_3V3, common_mode=1.6V);
}
```

### 2.2 Cross-Domain Signal Declaration

```cfl
cross_domain_signals {
  // Automatic level shifting inference
  mcu_uart_tx: logic_3v3 -> uart_console: logic_5v;     // 3.3V to 5V
  sensor_interrupt: logic_1v8 -> mcu_gpio: logic_3v3;   // 1.8V to 3.3V
  
  // Bidirectional signals with automatic direction control
  i2c_master: i2c_3v3 <-> i2c_slave: i2c_1v8;
  spi_flash: spi_3v3 <-> flash_memory: spi_1v8;
  
  // High-speed signals with timing requirements
  ddr_clock: lvds_1v8 -> ddr_memory: sstl_1v5 {
    max_skew = 50ps;
    impedance = 100Ω ± 5%;
  };
}
```

## 3. Automatic Level Shifter Insertion

### 3.1 Simple Unidirectional Level Shifting

```cfl
board LevelShiftingExample {
  power_domains {
    VCC_3V3: regulated_power(3.3V, 500mA);
    VCC_1V8: regulated_power(1.8V, 200mA);
    VCC_5V: regulated_power(5V, 100mA);
  };
  
  components {
    mcu: STM32H7 { io_voltage = VCC_3V3; };
    sensor: BME280 { io_voltage = VCC_1V8; };
    display: HD44780 { io_voltage = VCC_5V; };
  };
  
  connections {
    // Automatic level shifter insertion
    mcu.UART1_TX(logic_3v3) -> display.DATA_IN(logic_5v);
    // Tool automatically inserts: 3.3V->5V level shifter
    
    sensor.INT_PIN(logic_1v8) -> mcu.GPIO_PA5(logic_3v3);
    // Tool automatically inserts: 1.8V->3.3V level shifter
    
    mcu.SPI1_CS(logic_3v3) -> sensor.CS(logic_1v8);
    // Tool automatically inserts: 3.3V->1.8V level shifter
  };
}

// Tool automatically generates:
level_shifters {
  LS1: LevelShifter_3V3_to_5V {
    part = auto_select(channels=1, speed=low_speed);
    VL = VCC_3V3;
    VH = VCC_5V;
    A1 = mcu.UART1_TX;
    B1 = display.DATA_IN;
    DIR = fixed_A_to_B;
  };
  
  LS2: LevelShifter_1V8_to_3V3 {
    part = auto_select(channels=1, speed=medium_speed);
    VL = VCC_1V8;
    VH = VCC_3V3;
    A1 = sensor.INT_PIN;
    B1 = mcu.GPIO_PA5;
    DIR = fixed_A_to_B;
  };
}
```

### 3.2 Bidirectional Level Shifting with Automatic Direction Control

```cfl
bidirectional_connections {
  // I2C bus crossing domains
  i2c_cross_domain: {
    master_side: mcu.I2C1(domain=VCC_3V3);
    slave_side: [sensor1(domain=VCC_1V8), sensor2(domain=VCC_1V8)];
    
    // Tool automatically inserts bidirectional level shifter
    level_shifter = auto_insert BiDirectionalLevelShifter {
      type = I2C_compatible;
      low_voltage = VCC_1V8;
      high_voltage = VCC_3V3;
      channels = 2;  // SCL + SDA
      pullup_support = internal;
    };
  };
  
  // SPI with bidirectional MISO
  spi_cross_domain: {
    master: mcu.SPI2(domain=VCC_3V3);
    slave: flash_memory.spi(domain=VCC_1V8);
    
    // Unidirectional signals
    master.MOSI -> auto_level_shift -> slave.MOSI;
    master.SCK -> auto_level_shift -> slave.SCK;
    master.CS -> auto_level_shift -> slave.CS;
    
    // Bidirectional signal with automatic direction sensing
    master.MISO <-> auto_level_shift(direction=auto_sense) <-> slave.MISO;
  };
}
```

## 4. Advanced Level Shifting Patterns

### 4.1 Multi-Voltage Bus Interfaces

```cfl
multi_voltage_bus {
  // UART bus with multiple voltage domains
  uart_bus: multi_domain_bus {
    master: mcu.UART1(domain=VCC_3V3);
    slaves: [
      gps_module(domain=VCC_1V8),
      bluetooth(domain=VCC_3V3),      // Same domain, no shifting
      rs485_transceiver(domain=VCC_5V)
    ];
    
    // Tool automatically creates star topology with level shifters
    implementation = star_topology {
      hub_voltage = VCC_3V3;  // Use master's voltage as hub
      
      connections {
        master <-> hub (direct);
        bluetooth <-> hub (direct);       // Same voltage
        gps_module <-> hub via LevelShifter_3V3_to_1V8;
        rs485_transceiver <-> hub via LevelShifter_3V3_to_5V;
      };
    };
  };
  
  // Multi-drop I2C with voltage translation
  i2c_multi_voltage: {
    master: mcu.I2C1(domain=VCC_3V3);
    devices: [
      sensor_1v8_group: [temp_sensor, humidity_sensor](domain=VCC_1V8),
      sensor_5v_group: [pressure_sensor](domain=VCC_5V),
      local_devices: [rtc, eeprom](domain=VCC_3V3)
    ];
    
    // Tool creates hierarchical level shifting
    implementation = hierarchical_translation {
      main_bus(VCC_3V3) -> {
        local_devices (direct),
        level_shift_to_1v8 -> sensor_1v8_group,
        level_shift_to_5v -> sensor_5v_group
      };
    };
  };
}
```

### 4.2 High-Speed Level Shifting

```cfl
high_speed_interfaces {
  // DDR interface with voltage translation
  ddr_interface: cross_domain_interface {
    controller: mcu.ddr_controller(domain=VCC_1V8);
    memory: ddr3_ram(domain=VCC_1V5);
    
    speed_requirements {
      frequency = 400MHz;
      setup_time = 90ps;
      hold_time = 90ps;
      skew_tolerance = 25ps;
    };
    
    // Tool selects appropriate high-speed level shifter
    level_shifter = auto_select HighSpeedLevelShifter {
      type = DDR_optimized;
      propagation_delay_max = 2ns;
      skew_matching = ±10ps;
      drive_strength = auto_calculate(load_capacitance);
    };
  };
  
  // High-speed differential signals
  differential_interface: {
    transmitter: serdes_tx(domain=VCC_1V8, standard=LVDS);
    receiver: serdes_rx(domain=VCC_2V5, standard=LVPECL);
    
    // Differential level/standard conversion
    conversion = auto_insert DifferentialConverter {
      input_standard = LVDS_1V8;
      output_standard = LVPECL_2V5;
      bandwidth = 2.5GHz;
      jitter_max = 1ps_rms;
    };
  };
}
```

## 5. Back-Drive Protection and Power Sequencing

### 5.1 Automatic Back-Drive Prevention

```cfl
back_drive_protection {
  // Power sequence aware level shifting
  cross_domain_uart: {
    transmitter: mcu.UART_TX(domain=VCC_3V3);
    receiver: module.UART_RX(domain=VCC_1V8);
    
    power_dependencies {
      VCC_3V3.power_up_time = 50ms;
      VCC_1V8.power_up_time = 30ms;
      
      // VCC_1V8 may power up first
      back_drive_risk = high;
    };
    
    // Tool automatically selects level shifter with back-drive protection
    level_shifter = auto_select BackDriveProtectedLevelShifter {
      features = [
        auto_direction_sensing,
        power_down_protection,
        output_disable_when_vcc_low
      ];
      
      // Specific part selection based on requirements
      part = "TXS0108E" {  // 8-channel with auto-direction
        channels_used = 1;
        auto_direction = true;
        partial_power_down = supported;
      };
    };
  };
  
  // I2C with power domain isolation
  protected_i2c: {
    master: mcu.I2C1(domain=VCC_3V3);
    slaves: sensors(domain=VCC_1V8_SWITCHED);  // Can be powered off
    
    protection_strategy {
      // Use I2C-specific level shifter with isolation
      level_shifter = "PCA9306" {
        features = [
          i2c_compatible,
          rise_time_accelerator,
          static_offset_voltage_protection
        ];
        
        isolation = {
          method = integrated_protection;
          back_drive_current_max = 1µA;
          power_off_leakage_max = 1µA;
        };
      };
      
      // Automatic pullup management
      pullup_management = {
        master_side_pullups = 4.7kΩ to VCC_3V3;
        slave_side_pullups = 4.7kΩ to VCC_1V8_SWITCHED;
        
        // Disable slave side pullups when domain is off
        slave_pullup_control = VCC_1V8_SWITCHED.power_good;
      };
    };
  };
}
```

### 5.2 Power Sequence Integration

```cfl
power_sequence_integration {
  // Level shifters in power-up sequence
  power_up_sequence {
    stage1: VCC_5V.enable();
    stage2: VCC_3V3.enable();
    stage3: VCC_1V8.enable();
    
    // Level shifters enable after both domains are stable
    stage4: {
      wait_for [VCC_3V3.power_good, VCC_1V8.power_good];
      level_shifters.enable();
      delay(1ms);  // Allow level shifters to stabilize
    };
    
    stage5: {
      // Now safe to enable communication
      communication_interfaces.enable();
      system_ready = true;
    };
  };
  
  // Power-down sequence with proper isolation
  power_down_sequence {
    stage1: {
      // Disable communication first
      communication_interfaces.disable();
      level_shifters.disable();
    };
    
    stage2: {
      // Power down domains in reverse order
      VCC_1V8.disable();
      delay(5ms);
    };
    
    stage3: {
      VCC_3V3.disable();
      VCC_5V.disable();
    };
  };
}
```

## 6. Automatic Level Shifter Selection

### 6.1 Smart Part Selection

```cfl
level_shifter_selection {
  // Tool considers multiple factors for part selection
  selection_criteria {
    voltage_compatibility: required;
    channel_count: optimize_for_minimal_parts;
    speed_requirements: meet_timing_constraints;
    direction_requirements: [unidirectional, bidirectional, auto_sensing];
    power_consumption: minimize;
    back_drive_protection: required_if_power_sequence_risk;
    package_preference: small_form_factor;
    cost_optimization: true;
  };
  
  // Example automatic selections
  auto_selections {
    // Low-speed, unidirectional: simple buffer
    uart_tx_3v3_to_5v: "74LVC1T45" {
      channels = 1;
      direction = fixed;
      speed = low;
      cost = lowest;
    };
    
    // Medium-speed, bidirectional: auto-direction
    gpio_3v3_to_1v8: "TXS0102" {
      channels = 2;
      direction = auto_sensing;
      speed = medium;
      back_drive_protection = true;
    };
    
    // High-speed, multiple channels: dedicated IC
    spi_bus_3v3_to_1v8: "TXS0108E" {
      channels = 4;  // MOSI, MISO, SCK, CS
      direction = auto_sensing;
      speed = high;
      features = [partial_power_down, output_enable];
    };
    
    // I2C specific: protocol-aware shifter
    i2c_3v3_to_1v8: "PCA9306" {
      channels = 2;  // SCL, SDA
      protocol = i2c_optimized;
      features = [rise_time_accelerator, static_protection];
    };
  };
}
```

### 6.2 Integration with Standard Component Library

```cfl
// Standard level shifter library
import std.level_shifters.*;

level_shifter_library {
  // Voltage translation functions
  auto_level_shift(from_domain, to_domain, signal_type, speed_class);
  
  // Protocol-specific shifters
  i2c_level_shift(from_voltage, to_voltage, frequency_max);
  spi_level_shift(from_voltage, to_voltage, frequency_max, channels);
  uart_level_shift(from_voltage, to_voltage, baud_rate_max);
  
  // High-speed interfaces
  differential_level_shift(from_standard, to_standard, frequency);
  clock_level_shift(from_voltage, to_voltage, frequency, jitter_max);
  
  // Special purpose
  back_drive_protected_shift(from_domain, to_domain, isolation_type);
  power_sequence_safe_shift(domains, sequence_timing);
}

// Usage in design
connections {
  // Simple function calls for common cases
  mcu.UART_TX -> uart_level_shift(3.3V, 5V, 115200) -> console.RX;
  
  sensor.INT -> back_drive_protected_shift(1.8V, 3.3V) -> mcu.GPIO;
  
  mcu.I2C -> i2c_level_shift(3.3V, 1.8V, 400kHz) -> sensor_bus;
}
```

## 7. Validation and Design Rules

### 7.1 Automatic Design Rule Checking

```cfl
level_shift_validation {
  // Automatic checks during compilation
  voltage_compatibility_check {
    verify all_cross_domain_signals have valid_level_shifters;
    verify level_shifter_voltage_ranges are compatible;
    verify no_direct_connection_between_incompatible_domains;
  };
  
  timing_validation {
    verify propagation_delays meet_setup_hold_requirements;
    verify level_shifter_bandwidth >= signal_frequency;
    verify skew_matching within_tolerance;
  };
  
  power_sequence_validation {
    verify level_shifters_enable_after_both_domains_stable;
    verify no_back_drive_current_during_power_sequence;
    verify isolation_during_power_down;
  };
  
  electrical_validation {
    verify drive_strength_adequate for_load_capacitance;
    verify input_thresholds compatible_with_output_levels;
    verify noise_margins adequate;
  };
}
```

### 7.2 Simulation and Verification

```cfl
level_shift_simulation {
  // Behavioral simulation of level shifting
  simulate power_up_sequence {
    model level_shifter_enable_delays;
    verify no_back_drive_current;
    check signal_integrity_during_transition;
  };
  
  simulate signal_transmission {
    model propagation_delays;
    verify timing_margins;
    check signal_quality_metrics;
  };
  
  simulate power_down_sequence {
    verify proper_isolation;
    check discharge_characteristics;
    validate fault_scenarios;
  };
}
```

## Key Benefits of Automatic Level Shifting

1. **Zero Manual Effort**: Cross-domain connections automatically get appropriate level shifters
2. **Correct by Construction**: No forgotten level shifters or wrong voltage connections
3. **Back-Drive Protection**: Automatic protection against power sequencing issues
4. **Optimal Part Selection**: Tool selects best level shifter for each application
5. **Power Sequence Integration**: Level shifters properly integrated with power management
6. **High-Speed Support**: Automatic timing and signal integrity management
7. **Protocol Awareness**: I2C, SPI, etc. get protocol-specific level shifters
8. **Cost Optimization**: Minimal number of level shifter ICs while meeting requirements

This approach eliminates one of the most common sources of board design errors while making multi-voltage designs as easy to specify as single-voltage designs. The designer thinks about functional connections, and the tool handles all the voltage domain complexities automatically.