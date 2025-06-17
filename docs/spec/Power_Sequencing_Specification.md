# Power Sequencing Specification for Circuit Flow Language

## 1. Power Flow Paradigm

Board designers think about power in terms of **sequences and dependencies**, not just static rails. The language should capture this temporal aspect naturally.

## 2. Power Domain Declaration

### 2.1 Basic Power Domain Definition

```cfl
power_domains {
  // Primary power input
  USB_5V: input_power {
    voltage = 5V ± 5%;
    current_max = 2A;
    source = USB_CONNECTOR.VBUS;
    protection = [overvoltage(5.5V), overcurrent(2.1A)];
  };
  
  // Intermediate rails
  VCC_3V3: regulated_power {
    voltage = 3.3V ± 3%;
    current_max = 1A;
    regulator = LinearRegulator(LM1117-3.3);
    load_regulation = 1%;
  };
  
  VCC_1V8_IO: regulated_power {
    voltage = 1.8V ± 3%;
    current_max = 500mA;
    regulator = LinearRegulator(LP38691);
    low_noise = true;
  };
  
  VCC_1V2_CORE: switching_power {
    voltage = 1.2V ± 2%;
    current_max = 3A;
    regulator = SwitchingRegulator(TPS62840);
    efficiency_min = 85%;
    ripple_max = 50mVpp;
  };
  
  // Special domains
  VCC_ANALOG: low_noise_power {
    voltage = 3.3V ± 1%;
    current_max = 100mA;
    isolation = ferrite_bead;
    source = VCC_3V3;
    ripple_max = 10mVpp;
  };
  
  VCC_DDR: memory_power {
    voltage = 1.5V ± 3%;
    current_max = 800mA;
    termination_voltage = 0.75V;  // VTT = VDD/2
  };
}
```

### 2.2 Power Domain States

```cfl
power_states {
  // Global system states
  SYSTEM_OFF: all_domains_off;
  SYSTEM_STANDBY: minimal_power_mode;
  SYSTEM_ON: full_operation;
  SYSTEM_SUSPEND: low_power_mode;
  
  // Domain-specific states for each power rail
  VCC_3V3.states = [OFF, STANDBY(100mA), FULL(1A)];
  VCC_1V2_CORE.states = [OFF, RETENTION(10mA), ACTIVE(3A)];
  VCC_DDR.states = [OFF, SELF_REFRESH(50mA), ACTIVE(800mA)];
}
```

## 3. Power-Up Sequencing

### 3.1 Declarative Sequence Specification

```cfl
power_up_sequence {
  // Stage 1: Primary power establishment
  stage1: {
    USB_5V.enable();
    wait_for USB_5V.power_good(timeout=100ms);
    
    // Soft-start delay
    delay(10ms);
  };
  
  // Stage 2: Main 3.3V rail
  stage2: {
    VCC_3V3.enable();
    wait_for VCC_3V3.power_good(timeout=50ms);
    
    // Allow rail to settle
    delay(5ms);
  };
  
  // Stage 3: Core power (can be parallel with I/O)
  stage3_parallel: {
    branch_A: {
      VCC_1V2_CORE.enable();
      wait_for VCC_1V2_CORE.power_good(timeout=20ms);
    };
    
    branch_B: {
      delay(2ms);  // Slight delay from 3.3V
      VCC_1V8_IO.enable();
      wait_for VCC_1V8_IO.power_good(timeout=30ms);
    };
    
    // Wait for both branches to complete
    sync_point;
  };
  
  // Stage 4: Specialized domains
  stage4: {
    // DDR power - needs core and I/O to be stable
    depends_on [VCC_1V2_CORE.power_good, VCC_1V8_IO.power_good];
    VCC_DDR.enable();
    wait_for VCC_DDR.power_good(timeout=20ms);
    
    // Analog domain - last to minimize noise
    delay(5ms);
    VCC_ANALOG.enable();
    wait_for VCC_ANALOG.power_good(timeout=10ms);
  };
  
  // Stage 5: System ready
  stage5: {
    // Release reset to MCU
    delay(10ms);  // Allow all rails to settle
    SYSTEM_RESET.deassert();
    
    // Power-up complete signal
    POWER_LED.enable();
    system_state = SYSTEM_ON;
  };
}
```

### 3.2 Timing-Based Sequencing

```cfl
power_up_timing {
  // Absolute timing from power-on
  t0: USB_5V.enable();
  t0+10ms: VCC_3V3.enable();
  t0+25ms: [VCC_1V2_CORE.enable(), VCC_1V8_IO.enable()];
  t0+50ms: VCC_DDR.enable();
  t0+70ms: VCC_ANALOG.enable();
  t0+100ms: SYSTEM_RESET.deassert();
  
  // Conditional timing based on power-good signals
  VCC_3V3.power_good + 5ms: VCC_1V2_CORE.enable();
  [VCC_1V2_CORE.power_good AND VCC_1V8_IO.power_good] + 2ms: VCC_DDR.enable();
}
```

## 4. Power-Down Sequencing

### 4.1 Controlled Power-Down

```cfl
power_down_sequence {
  // Stage 1: Prepare for shutdown
  stage1: {
    // Save critical state
    system_state = SYSTEM_SHUTTING_DOWN;
    mcu.enter_shutdown_mode();
    
    // Wait for MCU to save state
    wait_for mcu.shutdown_ready(timeout=1s);
  };
  
  // Stage 2: Assert reset and disable specialized domains first
  stage2: {
    SYSTEM_RESET.assert();
    delay(1ms);
    
    // Disable noise-sensitive domains first
    VCC_ANALOG.disable();
    
    // Disable memory power
    VCC_DDR.enter_self_refresh();
    delay(100µs);  // tRFC
    VCC_DDR.disable();
  };
  
  // Stage 3: Core power domains (reverse order of power-up)
  stage3: {
    VCC_1V8_IO.disable();
    delay(5ms);
    VCC_1V2_CORE.disable();
    delay(10ms);
  };
  
  // Stage 4: Main power
  stage4: {
    VCC_3V3.disable();
    delay(50ms);  // Allow discharge
    
    // Optional: force discharge for fast power-down
    VCC_3V3.force_discharge(method=resistive_load, time=100ms);
  };
  
  // Stage 5: Complete shutdown
  stage5: {
    USB_5V.disable();  // If controllable
    POWER_LED.disable();
    system_state = SYSTEM_OFF;
  };
}
```

### 4.2 Fast Power-Down with Active Discharge

```cfl
fast_power_down_sequence {
  // Emergency shutdown requirements
  requirements {
    total_time < 500ms;
    rail_discharge_time < 100ms;
    data_preservation = required;
  };
  
  // Parallel discharge implementation
  stage1_parallel: {
    branch_save_data: {
      mcu.emergency_save_state();
      wait_for data_saved OR timeout(100ms);
    };
    
    branch_prep_discharge: {
      // Pre-enable discharge circuits
      discharge_circuits.arm();
    };
  };
  
  stage2_fast_shutdown: {
    // Assert reset immediately
    SYSTEM_RESET.assert();
    
    // Disable all regulators simultaneously
    [VCC_ANALOG, VCC_DDR, VCC_1V8_IO, VCC_1V2_CORE].disable();
    
    // Activate discharge circuits
    VCC_3V3.force_discharge(method=active_fet, target_time=50ms);
    VCC_1V2_CORE.force_discharge(method=active_fet, target_time=30ms);
    VCC_DDR.force_discharge(method=resistive, target_time=20ms);
    
    // Main rail last
    delay(100ms);
    VCC_3V3.disable();
    VCC_3V3.force_discharge(method=active_fet, target_time=100ms);
  };
}
```

## 5. Low-Power Mode Management

### 5.1 Progressive Power Reduction

```cfl
low_power_modes {
  // Light sleep - CPU stopped, peripherals active
  LIGHT_SLEEP: {
    VCC_1V2_CORE.reduce_to(0.8V);  // Lower core voltage
    VCC_DDR.enter_self_refresh();
    VCC_ANALOG.maintain();  // Keep analog active
    
    wake_sources = [GPIO_interrupt, UART_activity, Timer];
    wake_time_typical = 10µs;
  };
  
  // Deep sleep - most domains off
  DEEP_SLEEP: {
    VCC_1V2_CORE.reduce_to(0.6V);  // Retention voltage
    VCC_1V8_IO.gate_unused_banks();  // Power gate unused I/O
    VCC_DDR.disable();  // Turn off DDR
    VCC_ANALOG.disable();  // Disable analog
    
    // Keep only RTC domain
    VCC_RTC.maintain(source=battery_backup);
    
    wake_sources = [RTC_alarm, GPIO_wakeup];
    wake_time_typical = 100ms;
  };
  
  // Hibernation - almost everything off
  HIBERNATION: {
    // Save state to non-volatile memory
    system_state.save_to(external_flash);
    
    // Disable all except backup power
    [VCC_1V2_CORE, VCC_1V8_IO, VCC_DDR, VCC_ANALOG].disable();
    VCC_3V3.disable();
    
    // Backup domain only
    VCC_BACKUP.maintain(source=coin_cell, current=1µA);
    
    wake_sources = [RTC_alarm, power_button];
    wake_time_typical = 2s;
  };
}
```

### 5.2 Dynamic Power Management

```cfl
dynamic_power_management {
  // Automatic power scaling based on load
  adaptive_scaling: {
    monitor VCC_1V2_CORE.current every 1ms;
    
    if current < 500mA for 100ms {
      VCC_1V2_CORE.reduce_voltage(step=0.05V, min=0.9V);
    }
    
    if current > 2A {
      VCC_1V2_CORE.increase_voltage(step=0.05V, max=1.3V);
    }
  };
  
  // Peripheral power gating
  peripheral_gating: {
    unused_peripherals.auto_gate(timeout=5s);
    
    when peripheral_access_requested {
      target_peripheral.power_domain.enable();
      wait_for power_good(timeout=1ms);
      grant_access();
    }
  };
}
```

## 6. Power Sequencing Implementation

### 6.1 Sequencing Controller Selection

```cfl
sequencing_implementation {
  // Option 1: Dedicated sequencing IC
  implement power_up_sequence using SequencingIC {
    part = "TPS65910";
    stages = 6;
    timing_accuracy = ±2%;
    
    // Map sequence to IC pins
    sequence_mapping {
      stage1 -> EN1_OUT;
      stage2 -> EN2_OUT;
      stage3 -> [EN3_OUT, EN4_OUT];  // Parallel enables
      stage4 -> EN5_OUT;
    };
  };
  
  // Option 2: MCU-controlled sequencing
  implement power_up_sequence using MCU_Control {
    controller = mcu.power_management_unit;
    
    // Use GPIO pins for enable signals
    gpio_mapping {
      VCC_3V3.enable -> mcu.GPIO_PA5;
      VCC_1V2_CORE.enable -> mcu.GPIO_PA6;
      VCC_1V8_IO.enable -> mcu.GPIO_PA7;
    };
    
    // Power-good monitoring
    power_good_inputs {
      VCC_3V3.power_good -> mcu.GPIO_PB1;
      VCC_1V2_CORE.power_good -> mcu.GPIO_PB2;
    };
  };
  
  // Option 3: Hybrid approach
  implement power_up_sequence using Hybrid {
    primary_controller = SequencingIC("TPS65910");
    backup_controller = mcu.power_management;
    
    // Sequencer handles main rails
    primary_sequence = [VCC_3V3, VCC_1V2_CORE, VCC_1V8_IO];
    
    // MCU handles specialized domains
    secondary_sequence = [VCC_ANALOG, VCC_DDR];
  };
}
```

### 6.2 Power Gating Implementation

```cfl
power_gating_circuits {
  // High-side P-FET gating
  VCC_1V8_IO.gating_circuit = HighSidePFET {
    fet = "SI2301DS";  // P-channel MOSFET
    gate_driver = mcu.GPIO_PA8;
    enable_polarity = active_low;
    
    // Soft-start control
    soft_start = RC_circuit(R=10kΩ, C=1nF);
    inrush_current_limit = 500mA;
  };
  
  // Low-side N-FET gating (for discharge)
  VCC_3V3.discharge_circuit = LowSideNFET {
    fet = "BSS84";  // N-channel MOSFET
    gate_driver = mcu.GPIO_PA9;
    enable_polarity = active_high;
    
    // Discharge resistor
    discharge_resistor = 10Ω;
    discharge_time_constant = 100ms;
  };
  
  // Load switch for peripheral power
  peripheral_power.gating = LoadSwitch {
    part = "TPS22960";
    enable_signal = mcu.GPIO_PA10;
    current_limit = 1A;
    overcurrent_protection = true;
  };
}
```

## 7. Power Monitoring and Protection

### 7.1 Real-Time Power Monitoring

```cfl
power_monitoring {
  // Current monitoring on critical rails
  VCC_1V2_CORE.monitor_current = CurrentSensor {
    method = shunt_resistor(0.01Ω);
    amplifier = "INA219";
    resolution = 1mA;
    alert_threshold = 2.5A;
    alert_action = reduce_frequency();
  };
  
  // Voltage monitoring
  all_rails.monitor_voltage = VoltageDivider {
    adc = mcu.ADC1;
    sampling_rate = 1kHz;
    alert_thresholds = {
      overvoltage = nominal + 10%;
      undervoltage = nominal - 10%;
    };
  };
  
  // Temperature monitoring
  power_components.monitor_temperature = ThermalSensor {
    sensor = "TMP75";
    i2c_address = 0x48;
    alert_threshold = 85°C;
    alert_action = reduce_power();
  };
}
```

### 7.2 Protection Mechanisms

```cfl
power_protection {
  // Overcurrent protection
  overcurrent_protection {
    detection_method = current_sensor + timer;
    response_time = 10µs;
    action = disable_regulator + set_fault_flag;
    
    retry_strategy = {
      attempts = 3;
      backoff_time = [1s, 5s, 30s];
      permanent_disable_after = 3_failures;
    };
  };
  
  // Overvoltage protection
  overvoltage_protection {
    detection = crowbar_circuit(SCR + fuse);
    trigger_voltage = nominal + 15%;
    response_time = 1µs;
  };
  
  // Thermal protection
  thermal_protection {
    temperature_sensors = [regulator_temp, pcb_temp];
    thermal_shutdown = 125°C;
    warning_threshold = 100°C;
    
    thermal_management = {
      reduce_switching_frequency();
      enable_additional_cooling();
      reduce_output_current();
    };
  };
}
```

## 8. Advanced Sequencing Features

### 8.1 Conditional Sequencing

```cfl
conditional_power_sequences {
  // Different sequences based on system state
  if (cold_boot) {
    use extended_power_up_sequence {
      additional_delays = [50ms, 25ms, 10ms];
      self_test_enabled = true;
    };
  } else if (warm_reset) {
    use fast_power_up_sequence {
      skip_stages = [USB_5V_setup];
      reduced_delays = true;
    };
  }
  
  // Battery vs USB power
  if (power_source == battery) {
    optimize_for_efficiency();
    enable_low_power_modes();
  } else if (power_source == usb) {
    optimize_for_performance();
    disable_battery_management();
  }
}
```

### 8.2 Fault Recovery Sequences

```cfl
fault_recovery {
  // Power-up failure recovery
  on power_up_timeout(rail, timeout_duration) {
    log_fault(rail, timestamp);
    
    if (retry_count < max_retries) {
      power_down_all_rails();
      delay(1s);  // Allow discharge
      retry_power_up();
    } else {
      enter_safe_mode();
      signal_fault_condition();
    }
  };
  
  // Brown-out recovery
  on brown_out_detected(rail) {
    immediate_actions {
      save_critical_state();
      reduce_system_load();
      enable_backup_power();
    };
    
    recovery_sequence {
      wait_for rail.voltage_recovery(timeout=100ms);
      if (recovered) {
        restore_normal_operation();
      } else {
        initiate_controlled_shutdown();
      }
    };
  };
}
```

## Key Benefits of Power Sequencing Language

1. **Intuitive Specification**: Matches how designers think about power sequences
2. **Automatic Implementation**: Tool selects appropriate sequencing circuits
3. **Timing Validation**: Ensures sequence timing meets component requirements
4. **Fault Handling**: Built-in protection and recovery mechanisms
5. **Power Optimization**: Automatic low-power mode management
6. **Implementation Flexibility**: Supports different sequencing architectures
7. **Verification**: Simulation and validation of power sequences

This approach transforms power sequencing from a complex, error-prone manual process into a declarative specification that captures design intent while automating the implementation details.