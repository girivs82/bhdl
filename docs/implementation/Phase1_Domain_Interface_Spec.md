# Phase 1: Domain Interface Implementation Specification

## Overview

This document provides detailed technical specifications for implementing the domain interface converters that enable signal translation between analog and digital simulation domains in BHDL.

## 1. Analog-to-Digital Converter (ADC)

### Purpose
Convert continuous analog voltages to discrete digital logic levels with proper threshold detection and hysteresis.

### Interface Design

```rust
use bhdl_netlist::NetId;
use bhdl_common::LogicLevel;

pub struct ADConverter {
    /// Configuration
    config: ADCConfig,
    
    /// State
    last_output: LogicLevel,
    last_voltage: f64,
    last_update_time: f64,
    
    /// Statistics
    transitions: usize,
    metastable_events: usize,
}

pub struct ADCConfig {
    /// Voltage thresholds
    pub v_il: f64,  // Input low voltage (max voltage for logic 0)
    pub v_ih: f64,  // Input high voltage (min voltage for logic 1)
    
    /// Hysteresis
    pub hysteresis: f64,  // Voltage hysteresis to prevent oscillation
    
    /// Timing
    pub t_pd_lh: f64,  // Propagation delay low-to-high
    pub t_pd_hl: f64,  // Propagation delay high-to-low
    
    /// Metastability
    pub metastable_time: f64,  // Time in undefined region before X
}

impl Default for ADCConfig {
    fn default() -> Self {
        Self {
            v_il: 0.8,   // TTL levels
            v_ih: 2.0,
            hysteresis: 0.1,
            t_pd_lh: 1e-9,  // 1ns
            t_pd_hl: 1e-9,  // 1ns
            metastable_time: 10e-9,  // 10ns
        }
    }
}
```

### Conversion Algorithm

```rust
impl ADConverter {
    pub fn convert(&mut self, voltage: f64, time: f64) -> Option<DigitalEvent> {
        let new_level = self.determine_logic_level(voltage);
        
        if new_level != self.last_output {
            let delay = match (self.last_output, new_level) {
                (LogicLevel::Low, LogicLevel::High) => self.config.t_pd_lh,
                (LogicLevel::High, LogicLevel::Low) => self.config.t_pd_hl,
                _ => 0.0,  // X transitions have no delay
            };
            
            self.last_output = new_level;
            self.last_update_time = time;
            self.transitions += 1;
            
            Some(DigitalEvent {
                time: time + delay,
                net: self.output_net,
                new_value: new_level,
                driver_strength: DriveStrength::Strong,
            })
        } else {
            None
        }
    }
    
    fn determine_logic_level(&mut self, voltage: f64) -> LogicLevel {
        match self.last_output {
            LogicLevel::Low => {
                if voltage > self.config.v_ih + self.config.hysteresis {
                    LogicLevel::High
                } else if voltage > self.config.v_ih {
                    // In metastable region
                    if self.in_metastable_region_too_long() {
                        LogicLevel::X
                    } else {
                        LogicLevel::Low  // Stay low
                    }
                } else {
                    LogicLevel::Low
                }
            }
            LogicLevel::High => {
                if voltage < self.config.v_il - self.config.hysteresis {
                    LogicLevel::Low
                } else if voltage < self.config.v_il {
                    // In metastable region
                    if self.in_metastable_region_too_long() {
                        LogicLevel::X
                    } else {
                        LogicLevel::High  // Stay high
                    }
                } else {
                    LogicLevel::High
                }
            }
            LogicLevel::X => {
                if voltage < self.config.v_il {
                    LogicLevel::Low
                } else if voltage > self.config.v_ih {
                    LogicLevel::High
                } else {
                    LogicLevel::X
                }
            }
            LogicLevel::Z => LogicLevel::X,  // High-Z to X
        }
    }
}
```

### Test Cases

1. **Basic Threshold Test**
   ```rust
   #[test]
   fn test_basic_thresholds() {
       let mut adc = ADConverter::default();
       
       // Test low level
       assert_eq!(adc.convert(0.5, 0.0), None);  // Already low
       
       // Test transition to high
       let event = adc.convert(2.5, 1e-6).unwrap();
       assert_eq!(event.new_value, LogicLevel::High);
       assert_eq!(event.time, 1e-6 + 1e-9);  // With propagation delay
   }
   ```

2. **Hysteresis Test**
   ```rust
   #[test]
   fn test_hysteresis() {
       let mut adc = ADConverter::default();
       
       // Start high
       adc.convert(3.0, 0.0);
       
       // Drop to just below v_ih - should stay high due to hysteresis
       assert_eq!(adc.convert(1.95, 1e-6), None);
       
       // Drop below hysteresis threshold
       let event = adc.convert(1.85, 2e-6).unwrap();
       assert_eq!(event.new_value, LogicLevel::Low);
   }
   ```

## 2. Digital-to-Analog Converter (DAC)

### Purpose
Convert discrete digital logic levels to continuous analog voltages with realistic rise/fall times.

### Interface Design

```rust
pub struct DAConverter {
    /// Configuration
    config: DACConfig,
    
    /// State
    current_voltage: f64,
    target_voltage: f64,
    transition_start_time: f64,
    in_transition: bool,
}

pub struct DACConfig {
    /// Output voltage levels
    pub v_ol: f64,  // Output low voltage
    pub v_oh: f64,  // Output high voltage
    
    /// Timing
    pub rise_time: f64,   // 10%-90% rise time
    pub fall_time: f64,   // 90%-10% fall time
    pub slew_rate: Option<f64>,  // V/s limit
    
    /// Output characteristics
    pub output_impedance: f64,
    pub output_capacitance: f64,
}

impl Default for DACConfig {
    fn default() -> Self {
        Self {
            v_ol: 0.0,
            v_oh: 5.0,
            rise_time: 1e-9,
            fall_time: 1e-9,
            slew_rate: Some(1e9),  // 1V/ns
            output_impedance: 50.0,
            output_capacitance: 5e-12,  // 5pF
        }
    }
}
```

### Voltage Generation Algorithm

```rust
impl DAConverter {
    pub fn update(&mut self, logic_level: LogicLevel, time: f64) -> AnalogUpdate {
        let new_target = match logic_level {
            LogicLevel::Low => self.config.v_ol,
            LogicLevel::High => self.config.v_oh,
            LogicLevel::X => (self.config.v_ol + self.config.v_oh) / 2.0,
            LogicLevel::Z => {
                return AnalogUpdate::HighImpedance;
            }
        };
        
        if (new_target - self.target_voltage).abs() > 1e-6 {
            self.target_voltage = new_target;
            self.transition_start_time = time;
            self.in_transition = true;
        }
        
        self.calculate_voltage(time)
    }
    
    fn calculate_voltage(&mut self, time: f64) -> AnalogUpdate {
        if !self.in_transition {
            return AnalogUpdate::Voltage(self.current_voltage);
        }
        
        let elapsed = time - self.transition_start_time;
        let is_rising = self.target_voltage > self.current_voltage;
        let transition_time = if is_rising { 
            self.config.rise_time 
        } else { 
            self.config.fall_time 
        };
        
        // Exponential transition (RC-like)
        let tau = transition_time / 2.197;  // For 10%-90% rise time
        let progress = 1.0 - (-elapsed / tau).exp();
        
        let voltage_diff = self.target_voltage - self.current_voltage;
        let mut new_voltage = self.current_voltage + voltage_diff * progress;
        
        // Apply slew rate limit
        if let Some(slew_rate) = self.config.slew_rate {
            let max_change = slew_rate * elapsed;
            let actual_change = new_voltage - self.current_voltage;
            if actual_change.abs() > max_change {
                new_voltage = self.current_voltage + max_change * actual_change.signum();
            }
        }
        
        // Check if transition complete
        if (new_voltage - self.target_voltage).abs() < 1e-6 {
            self.current_voltage = self.target_voltage;
            self.in_transition = false;
        } else {
            self.current_voltage = new_voltage;
        }
        
        AnalogUpdate::Voltage(self.current_voltage)
    }
}
```

### Output Model for SPICE

```rust
impl DAConverter {
    pub fn get_spice_model(&self) -> SpiceSourceModel {
        SpiceSourceModel::TheveninEquivalent {
            voltage: self.current_voltage,
            resistance: self.config.output_impedance,
            capacitance: Some(self.config.output_capacitance),
        }
    }
}
```

## 3. Domain Synchronizer

### Purpose
Coordinate time progression between event-driven digital and time-stepped analog domains.

### Interface Design

```rust
pub struct DomainSynchronizer {
    /// Time management
    current_time: f64,
    analog_timestep: f64,
    next_digital_event_time: Option<f64>,
    
    /// Synchronization points
    sync_points: BTreeSet<f64>,
    
    /// Convergence control
    convergence_tolerance: f64,
    max_iterations: usize,
}

pub struct SyncResult {
    pub next_time: f64,
    pub sync_type: SyncType,
    pub requires_iteration: bool,
}

pub enum SyncType {
    AnalogStep,
    DigitalEvent,
    Synchronized,  // Both domains need update
}
```

### Synchronization Algorithm

```rust
impl DomainSynchronizer {
    pub fn get_next_sync_point(&mut self) -> SyncResult {
        let next_analog = self.current_time + self.analog_timestep;
        let next_digital = self.next_digital_event_time.unwrap_or(f64::INFINITY);
        
        // Check for forced sync points (e.g., from user probes)
        let next_forced = self.sync_points
            .range((Excluded(self.current_time), Unbounded))
            .next()
            .copied()
            .unwrap_or(f64::INFINITY);
        
        let next_time = next_analog.min(next_digital).min(next_forced);
        
        let sync_type = match next_time {
            t if (t - next_analog).abs() < 1e-15 && (t - next_digital).abs() < 1e-15 => {
                SyncType::Synchronized
            }
            t if (t - next_analog).abs() < 1e-15 => SyncType::AnalogStep,
            t if (t - next_digital).abs() < 1e-15 => SyncType::DigitalEvent,
            _ => unreachable!(),
        };
        
        // Check if we need iteration (coupling between domains)
        let requires_iteration = self.check_coupling(sync_type);
        
        SyncResult {
            next_time,
            sync_type,
            requires_iteration,
        }
    }
    
    pub fn advance_time(&mut self, time: f64) {
        assert!(time >= self.current_time);
        self.current_time = time;
        
        // Adaptive timestep for analog domain
        self.adapt_analog_timestep();
    }
    
    fn adapt_analog_timestep(&mut self) {
        // Simple adaptive algorithm based on activity
        if let Some(next_event) = self.next_digital_event_time {
            let time_to_event = next_event - self.current_time;
            if time_to_event < self.analog_timestep * 2.0 {
                // Reduce timestep near digital events
                self.analog_timestep = (time_to_event / 2.0).max(1e-12);
            }
        }
    }
}
```

## 4. Integration Example

### Mixed-Signal Counter with DAC

```rust
#[test]
fn test_digital_counter_with_dac() {
    // Create a 4-bit counter driving a DAC
    let circuit = r#"
        board CounterDAC {
            power VDD = 5V @ 100mA;
            ground GND;
            
            // Digital counter
            counter: Counter4Bit {
                CLK <- clock_gen.out;
                Q[0:3] -> dac_bits[0:3];
            }
            
            // DAC interface (intent triggers D/A conversion)
            net analog_out: dac_bits[0:3] -> dac.in[0:3] for analog_output();
            
            // Output
            dac.out -> output_pin;
        }
    "#;
    
    // Run simulation
    let mut coordinator = SimulationCoordinator::new(circuit);
    let mut time = 0.0;
    
    while time < 100e-6 {  // 100us
        let sync = coordinator.get_next_sync();
        
        match sync.sync_type {
            SyncType::DigitalEvent => {
                coordinator.process_digital_events(sync.next_time);
            }
            SyncType::AnalogStep => {
                coordinator.step_analog(sync.next_time);
            }
            SyncType::Synchronized => {
                // Process both domains
                coordinator.process_digital_events(sync.next_time);
                coordinator.step_analog(sync.next_time);
                
                if sync.requires_iteration {
                    coordinator.iterate_coupling();
                }
            }
        }
        
        time = sync.next_time;
    }
    
    // Verify DAC output is staircase waveform
    let waveform = coordinator.get_waveform("analog_out");
    assert_staircase_waveform(waveform, 16, 0.0, 5.0);
}
```

## Implementation Checklist

### Week 1
- [ ] Create `bhdl-sim/src/integration/converters/` directory structure
- [ ] Implement ADConverter with basic threshold detection
- [ ] Add hysteresis support to ADConverter
- [ ] Write comprehensive ADC unit tests
- [ ] Implement DAConverter with exponential transitions
- [ ] Add slew rate limiting to DAC
- [ ] Write DAC unit tests

### Week 2
- [ ] Implement DomainSynchronizer basic time management
- [ ] Add adaptive timestep algorithm
- [ ] Create integration test framework
- [ ] Implement counter-DAC test circuit
- [ ] Add performance benchmarks
- [ ] Document public APIs
- [ ] Code review and cleanup

## Success Metrics

1. **Correctness**
   - All unit tests passing
   - Integration tests show correct waveforms
   - No missing transitions or glitches

2. **Performance**
   - A/D conversion < 100ns per event
   - D/A update calculation < 1μs
   - Synchronization overhead < 5% of simulation time

3. **Robustness**
   - Handles metastability correctly
   - No oscillation at thresholds
   - Convergence for feedback loops

This specification provides the foundation for implementing robust domain interfaces that will enable true mixed-signal simulation in BHDL.