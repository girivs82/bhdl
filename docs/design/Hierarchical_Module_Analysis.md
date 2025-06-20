# Hierarchical Module Analysis Pipeline

## Overview

The analyzer must perform comprehensive checks on module connectivity, including electrical compatibility, pin direction conflicts, and voltage level mismatches. This leverages both static analysis and SPICE simulation.

## Analysis Phases

### Phase 1: Static Connectivity Checks

#### 1.1 Pin Direction Validation
```rust
// Check for invalid connections
match (source_dir, dest_dir) {
    (Output, Output) => Error("Cannot connect two outputs"),
    (Input, Input) => Error("Cannot connect two inputs without a driver"),
    (Output, Input) => Ok,
    (InOut, InOut) => Ok,
    (OpenDrain, OpenDrain) => Ok, // Special case - allowed
    (OpenCollector, OpenCollector) => Ok, // Special case - allowed
    _ => check_detailed_compatibility()
}
```

#### 1.2 Open-Drain/Open-Collector Rules
```bhdl
// Valid: Multiple open-drain outputs can share a net
module I2CSystem {
    signal sda_bus;
    
    // Pull-up required for open-drain
    VCC -> R1: Res(4.7k).1 -> sda_bus;
    
    master: I2CMaster {
        SDA <-> sda_bus;  // Open-drain bidir
    }
    
    slave1: I2CSlave {
        SDA <-> sda_bus;  // Open-drain bidir - OK!
    }
    
    slave2: I2CSlave {
        SDA <-> sda_bus;  // Multiple open-drain - OK!
    }
}
```

#### 1.3 Missing Pull-Up Detection
```rust
fn check_open_drain_nets(net: &Net) -> Result<()> {
    if net.has_open_drain_drivers() && !net.has_pullup() {
        return Err(AnalysisError {
            severity: Error,
            message: "Open-drain net requires pull-up resistor",
            suggestion: Some("Add pull-up resistor to VCC"),
            fixes: vec![
                Fix::AddComponent(
                    "pullup", 
                    "Res(4.7k)", 
                    vec!["VCC -> .1", ".2 -> net"]
                )
            ]
        });
    }
    Ok(())
}
```

### Phase 2: Voltage Level Compatibility

#### 2.1 Digital Logic Level Checks
```rust
#[derive(Debug)]
struct VoltageLevel {
    vcc: f64,
    vih_min: f64,  // Min voltage for logic HIGH input
    vil_max: f64,  // Max voltage for logic LOW input
    voh_min: f64,  // Min voltage for logic HIGH output
    vol_max: f64,  // Max voltage for logic LOW output
}

impl VoltageLevel {
    fn is_compatible_with(&self, other: &VoltageLevel) -> Result<()> {
        // Check if output can drive input
        if self.voh_min < other.vih_min {
            return Err("Output HIGH level too low for input");
        }
        if self.vol_max > other.vil_max {
            return Err("Output LOW level too high for input");
        }
        
        // Check for overvoltage
        if self.vcc > other.vcc * 1.1 {  // 10% tolerance
            return Err("Output voltage exceeds input tolerance");
        }
        
        Ok(())
    }
}
```

#### 2.2 Common Logic Families
```rust
const TTL_5V: VoltageLevel = VoltageLevel {
    vcc: 5.0,
    vih_min: 2.0,
    vil_max: 0.8,
    voh_min: 2.7,
    vol_max: 0.5,
};

const CMOS_3V3: VoltageLevel = VoltageLevel {
    vcc: 3.3,
    vih_min: 2.0,
    vil_max: 0.8,
    voh_min: 2.4,
    vol_max: 0.4,
};

const LVTTL_3V3: VoltageLevel = VoltageLevel {
    vcc: 3.3,
    vih_min: 2.0,
    vil_max: 0.8,
    voh_min: 2.4,
    vol_max: 0.4,
};
```

#### 2.3 Level Shifter Insertion
```bhdl
// Analyzer detects voltage mismatch and suggests fix
module MixedVoltageSystem {
    // 5V MCU output to 3.3V input - INVALID!
    mcu_5v: MCU_5V {
        GPIO -> fpga_3v3.INPUT;  // ERROR: 5V -> 3.3V
    }
    
    // Suggested fix:
    mcu_5v: MCU_5V {
        GPIO -> level_shifter.A;
    }
    
    level_shifter: LevelShifter_5V_to_3V3 {
        A <- mcu_5v.GPIO;
        B -> fpga_3v3.INPUT;
        VCCA <- VCC_5V;
        VCCB <- VCC_3V3;
    }
}
```

### Phase 3: SPICE-Based Electrical Analysis

#### 3.1 DC Operating Point for Each Net
```rust
struct NetElectricalState {
    voltage: f64,
    current: f64,
    impedance: f64,
    driving_strength: DriveStrength,
}

fn analyze_net_electrical(net: &Net, spice: &SpiceEngine) -> NetElectricalState {
    // Run DC analysis to find steady-state
    let dc_result = spice.dc_analysis(&net);
    
    // Check voltage levels
    for pin in net.connected_pins() {
        if let Some(abs_max) = pin.absolute_max_voltage() {
            if dc_result.voltage > abs_max {
                report_error("Voltage exceeds absolute maximum rating");
            }
        }
    }
    
    NetElectricalState {
        voltage: dc_result.voltage,
        current: dc_result.current,
        impedance: calculate_thevenin_impedance(&net),
        driving_strength: categorize_drive(&dc_result),
    }
}
```

#### 3.2 Current Capacity Checks
```rust
fn check_output_current_capacity(net: &Net) -> Result<()> {
    let total_load_current = net.sinks()
        .map(|sink| estimate_input_current(sink))
        .sum();
        
    let driver_capacity = net.driver()
        .map(|driver| driver.max_output_current())
        .unwrap_or(0.0);
        
    if total_load_current > driver_capacity * 0.8 {  // 80% derating
        return Err(AnalysisError {
            message: format!(
                "Output overloaded: {}mA load on {}mA driver",
                total_load_current * 1000.0,
                driver_capacity * 1000.0
            ),
            suggestion: Some("Add buffer or reduce fanout"),
        });
    }
    
    Ok(())
}
```

#### 3.3 Rise/Fall Time Analysis
```rust
fn analyze_signal_integrity(net: &Net) -> SignalIntegrity {
    // Calculate net capacitance
    let total_capacitance = net.trace_capacitance() 
        + net.load_capacitance();
    
    // Get driver characteristics
    let driver_resistance = net.driver().output_resistance();
    
    // RC time constant
    let tau = driver_resistance * total_capacitance;
    let rise_time = 2.2 * tau;  // 10% to 90%
    
    SignalIntegrity {
        rise_time,
        fall_time: rise_time * 1.2,  // Typically asymmetric
        max_frequency: 0.35 / rise_time,
    }
}
```

### Phase 4: Module-Specific Checks

#### 4.1 Parameter Validation
```rust
fn validate_module_parameters(
    instance: &ModuleInstance,
    definition: &ModuleDefinition,
) -> Result<()> {
    for param in &instance.parameters {
        let param_def = definition.get_parameter(&param.name)?;
        
        // Type check
        if param.value.type_of() != param_def.param_type {
            return Err("Parameter type mismatch");
        }
        
        // Range check
        if let Some(constraints) = &param_def.constraints {
            constraints.validate(&param.value)?;
        }
    }
    Ok(())
}
```

#### 4.2 Port Width Matching
```rust
fn check_array_connections(mapping: &PortMapping) -> Result<()> {
    let source_width = mapping.source.width();
    let dest_width = mapping.dest.width();
    
    match (source_width, dest_width) {
        (Some(s), Some(d)) if s != d => {
            Err(format!("Width mismatch: [{}] -> [{}]", s, d))
        }
        _ => Ok(())
    }
}
```

## Complete Analysis Pipeline

```rust
impl Analyzer {
    fn analyze_module_connectivity(&mut self) -> Result<()> {
        // Phase 1: Basic connectivity
        for instance in &self.module_instances {
            self.check_pin_directions(&instance)?;
            self.check_required_pins(&instance)?;
        }
        
        // Phase 2: Electrical compatibility
        for net in &self.nets {
            self.check_voltage_compatibility(&net)?;
            self.check_open_drain_pullups(&net)?;
        }
        
        // Phase 3: SPICE analysis
        let spice_netlist = self.generate_spice_netlist();
        let dc_results = self.spice.analyze_dc(&spice_netlist)?;
        
        for (net, result) in dc_results {
            self.validate_electrical_limits(&net, &result)?;
            self.check_signal_integrity(&net, &result)?;
        }
        
        // Phase 4: Module-specific
        for instance in &self.module_instances {
            self.validate_parameters(&instance)?;
            self.check_array_mappings(&instance)?;
        }
        
        Ok(())
    }
}
```

## Error Reporting

```rust
#[derive(Debug)]
struct ConnectivityError {
    location: SourceLocation,
    severity: Severity,
    category: ErrorCategory,
    message: String,
    details: Vec<String>,
    fixes: Vec<SuggestedFix>,
}

#[derive(Debug)]
enum ErrorCategory {
    DirectionConflict,
    VoltageMismatch,
    CurrentOverload,
    MissingPullup,
    SignalIntegrity,
    ParameterError,
}

impl ConnectivityError {
    fn report(&self) -> String {
        format!(
            "{}: {} at {}\n{}",
            self.severity,
            self.message,
            self.location,
            self.format_details_and_fixes()
        )
    }
}
```

## Example Analysis Output

```
ERROR: Voltage level mismatch at line 45
  5V output (mcu.GPIO) connected to 3.3V input (fpga.IN)
  
  Details:
  - Output HIGH: 4.5V min
  - Input max: 3.6V absolute maximum
  - Risk of permanent damage
  
  Suggested fixes:
  1. Add level shifter between pins
  2. Use 3.3V tolerant input
  3. Add resistor divider (reduced speed)

WARNING: Open-drain net missing pull-up at line 67
  Net 'i2c_sda' has open-drain drivers but no pull-up
  
  Suggested fix:
  VCC -> R_PU: Res(4.7k).1 -> i2c_sda;

ERROR: Output current exceeded at line 89
  Pin 'mcu.LED_OUT' driving 45mA into 5 parallel LEDs
  Maximum output current: 25mA (20mA recommended)
  
  Suggested fixes:
  1. Add current limiting resistors
  2. Use transistor buffer
  3. Reduce number of LEDs
```

## Benefits

1. **Early Error Detection** - Catches electrical issues before PCB fabrication
2. **Automated Fixes** - Suggests level shifters, pull-ups, buffers
3. **Safety Validation** - Prevents component damage from overvoltage
4. **Signal Integrity** - Ensures proper digital communication
5. **SPICE Integration** - Accurate electrical analysis