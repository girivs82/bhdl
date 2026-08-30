# BHDL Testbench Specification v2.0

## Overview

This document specifies the testbench extension to BHDL for simulation control, waveform capture, verification, and fault injection testing. Testbenches allow users to define simulation scenarios, specify which signals to monitor, set up stimuli, verify circuit behavior, and test failure scenarios.

## Current Implementation Status

### Implemented Features
- ✅ Basic testbench structure (`testbench Name for Board { … }` with
  `simulation` / `scope` / `stimulus` / `verify` / `measure` blocks)
- ✅ Simulation configuration (duration, timestep, solver, temperature —
  temperature is a plain number in Celsius)
- ✅ Scope definition with signal lists (`@NET`, `inst.pin`) and capture
  modes `continuous`, `on_change(threshold)`, `periodic(interval: t)`
- ✅ Stimulus with constant values; waveform calls (`ramp`, `sine`,
  `pulse`, `steps`, `constant`) parse with named parameters
- ✅ Verify block with comparison assertions (`<`, `>`, `<=`, `>=`, `==`)
- ✅ Assertion time constraints (`always`, `after t`) and `message "..."`
- ✅ Measurement collection (`name = expression;`)
- ✅ SPICE solver integration
- ✅ Signal value extraction (voltages, currents)

### Pending Features
- ⏳ Range assertions (`signal in min..max`, `in range(a, b)`) — not yet
  parsed; write a pair of comparisons instead
- ⏳ Tolerance assertions (`signal == value +/- tolerance`) — not yet parsed
- ⏳ Waveform stimuli simulation (ramp, pulse, sine — the syntax parses,
  solver playback pending)
- ⏳ Fault injection (`faults` / scenario blocks — not yet parsed)
- ⏳ Monte Carlo analysis
- ⏳ Parametric sweeps
- ⏳ Mixed-signal simulation

## Testbench Syntax

### Basic Structure

```bhdl
testbench TB_Name for BoardName {
    // Simulation configuration
    simulation {
        duration: 10ms;
        timestep: 1us;
        solver: spice;  // spice, behavioral, mixed
        temperature: 25;  // Celsius (plain number)
    }
    
    // Waveform capture specification
    scope "main" {
        signals: @VCC, @GND, R1.current, LED1.voltage;
        capture: continuous;  // continuous, on_change(threshold), periodic(interval: t)
    }
    
    // Stimulus definition
    stimulus {
        @VCC: 5V;  // Constant stimulus (implemented)
        // Waveform calls also parse: @VIN: ramp(from: 0V, to: 12V, duration: 1ms);
    }
    
    // Assertions and checks (comparisons + always / after t + message)
    verify {
        assert R1.current < 15mA always message "Current out of range";
        assert @VCC >= 4.9V always message "VCC unstable";
        assert LED1.voltage > 1.8V after 1ms message "LED not conducting";
        // Planned: assert R1.current in 5mA..15mA;  assert @VCC == 5V +/- 0.1V;
    }
    
    // Measurements
    measure {
        avg_current = R1.current;  // Simple assignment (implemented)
        efficiency = (@VOUT * @IOUT) / (@VIN * @IIN) * 100%;
    }
}
```

### Assertion Syntax

Current implementation supports:
- **Comparison assertions**: `signal < value`, `signal > value`,
  `signal <= value`, `signal >= value`, `signal == value`
- **Time constraints**: `always`, `after time`
- **Failure text**: `message "..."`

Planned (not yet parsed):
- **Range assertions**: `signal in min..max` — write a pair of
  comparison assertions instead
- **Tolerance assertions**: `signal == value +/- tolerance`

```bhdl
testbench TB_Assertions for LEDBoard {
    verify {
        // Simple comparisons
        assert Q1.p_diss < 2W always message "Transistor power exceeded";
        assert C1.voltage > 0V always message "Reverse voltage on capacitor";

        // Equality
        assert @VCC == 5V always message "VCC off nominal";

        // Range check = a pair of comparisons
        assert R1.current > 5mA always message "LED current too low";
        assert R1.current < 15mA always message "LED current too high";

        // Time-based assertions
        assert @VOUT > 4.5V after 10ms message "Output failed to rise";

        // Planned (not yet parsed):
        //   assert R1.current in 5mA..15mA always message "...";
        //   assert @VOUT == 5V +/- 0.1V always message "...";
    }
}
```

## Fault Injection System

### Basic Fault Injection

<!-- doc-check: skip (documents planned fault-injection feature — not yet parsed) -->
```bhdl
testbench TB_Fault_Analysis for PowerSupply {
    simulation {
        duration: 100ms;
        timestep: 10us;
        solver: spice;
    }
    
    // Define fault scenarios
    faults {
        // Component failure modes
        scenario "R1_short" {
            at time: 10ms {
                override R1.resistance = 0.001;  // Near short (1mΩ)
            }
        }
        
        scenario "R1_open" {
            at time: 10ms {
                override R1.resistance = 1e12;  // Near open (1TΩ)
            }
        }
        
        scenario "C1_short" {
            at time: 10ms {
                override C1.model = short_circuit;  // Change component model
            }
        }
        
        // Parameter drift
        scenario "resistor_drift" {
            // 5% tolerance exceeded
            override R1.resistance = R1.resistance * 1.06;
            override R2.resistance = R2.resistance * 0.94;
        }
        
        // Multiple failures
        scenario "cascade_failure" {
            at time: 10ms {
                override Q1.failed = true;  // Transistor fails
            }
            // This should trigger safety analysis
        }
    }
    
    // Run specific scenarios
    run_scenarios: ["R1_short", "C1_short", "cascade_failure"];
    
    // Safety integration
    safety_analysis {
        enable: true;
        report_level: detailed;  // summary, detailed, verbose
        
        // Define safety limits
        limits {
            max_current: 5A;
            max_voltage: 50V;
            max_power: 100W;
            max_temperature: 150C;
        }
    }
}
```

### Advanced Fault Injection Modes

<!-- doc-check: skip (documents planned fault-injection feature — not yet parsed) -->
```bhdl
testbench TB_Comprehensive_Faults for Circuit {
    // Progressive fault injection
    faults progressive {
        // Gradually degrade component
        scenario "capacitor_aging" {
            over time: 0ms..50ms {
                C1.capacitance = interpolate(
                    start: 100uF,
                    end: 50uF,
                    curve: exponential
                );
                C1.esr = interpolate(
                    start: 0.1,
                    end: 1.0,
                    curve: linear
                );
            }
        }
        
        // Temperature-induced failures
        scenario "thermal_runaway" {
            when Q1.temperature > 100C {
                Q1.leakage_current = Q1.leakage_current * 2;
                Q1.beta = Q1.beta * 0.8;
            }
            when Q1.temperature > 150C {
                override Q1.model = failed_short;
            }
        }
    }
    
    // Probabilistic faults
    faults probabilistic {
        scenario "random_component_failure" {
            // 1% chance of failure per millisecond
            probability: 0.01 per 1ms;
            
            select_component: random from [R1, R2, C1, C2];
            failure_mode: random from [open, short, drift(20%)];
        }
    }
    
    // Correlated faults
    faults correlated {
        scenario "power_surge" {
            when @VIN > 15V {
                // Multiple components affected
                parallel {
                    override TVS1.clamping = false;  // TVS fails
                    override C1.voltage_rating = 10V;  // Cap degrades
                    override R1.power_rating = 0.125W;  // Resistor undersized
                }
            }
        }
    }
}
```

### Integration with Safety Analysis

<!-- doc-check: skip (documents planned fault-injection feature — not yet parsed) -->
```bhdl
testbench TB_Safety_Integration for HighPowerCircuit {
    simulation {
        duration: 100ms;
        solver: spice;
        safety_checks: true;  // Enable safety analysis
    }
    
    // Define what-if scenarios with safety analysis
    safety_scenarios {
        // Component failure analysis
        analyze "short_circuit_protection" {
            inject: R_sense.resistance = 0.001;  // Current sense resistor shorts
            
            expect {
                safety_violation: current_limit at Q1;
                safety_action: recommend_fuse(rating: 5A, speed: fast);
                damage_risk: high at [Q1, D1, L1];
            }
        }
        
        // Cascade failure analysis
        analyze "mosfet_failure_cascade" {
            inject: Q1.drain_source = short;
            
            track {
                current_path: @VIN -> Q1 -> L1 -> @VOUT;
                power_dissipation: all_components;
                temperature_rise: [Q1, L1, D1];
            }
            
            expect {
                secondary_failures: [L1.saturation, D1.breakdown];
                safety_recommendation: "Add current limiting and thermal protection";
            }
        }
        
        // Tolerance stack-up analysis
        analyze "worst_case_tolerance" {
            set_all_tolerances: worst_case;  // All components at limit
            
            measure {
                output_deviation: (@VOUT - 5V) / 5V * 100%;
                efficiency_drop: nominal_efficiency - worst_case_efficiency;
                thermal_margin: max_temperature - safe_operating_temp;
            }
        }
    }
    
    // Automated safety report generation
    safety_report {
        include: [
            fault_tree_analysis,
            failure_modes_effects,
            recommended_protections,
            derating_analysis
        ];
        
        format: html;
        output: "safety_analysis_report.html";
    }
}
```

### Fault Injection Implementation

```rust
// Core fault injection types
pub enum FaultType {
    // Component parameter override
    ParameterOverride {
        component: String,
        parameter: String,
        value: Value,
    },
    
    // Model replacement
    ModelOverride {
        component: String,
        model: ComponentModel,
    },
    
    // Connection faults
    ConnectionFault {
        net: String,
        fault: ConnectionFaultType,
    },
    
    // Progressive degradation
    ProgressiveFault {
        component: String,
        parameter: String,
        start_value: Value,
        end_value: Value,
        curve: InterpolationCurve,
    },
}

pub enum ConnectionFaultType {
    Open,           // High impedance
    Short,          // Low impedance
    ShortToNet(String),  // Short to another net
    Intermittent(f64),   // Probability of connection
}

// Safety analysis integration
pub struct SafetyAnalyzer {
    limits: SafetyLimits,
    monitor: ComponentMonitor,
    fault_tree: FaultTree,
}

impl SafetyAnalyzer {
    pub fn analyze_fault_scenario(
        &mut self,
        circuit: &Circuit,
        fault: &FaultType,
    ) -> SafetyReport {
        // 1. Apply fault to circuit
        let modified_circuit = self.apply_fault(circuit, fault);
        
        // 2. Run DC analysis to find new operating point
        let dc_result = self.run_dc_analysis(&modified_circuit);
        
        // 3. Check all safety limits
        let violations = self.check_safety_limits(&dc_result);
        
        // 4. Identify potential cascade failures
        let cascade_risks = self.analyze_cascade_failures(&dc_result, &violations);
        
        // 5. Generate recommendations
        let recommendations = self.generate_safety_recommendations(&violations, &cascade_risks);
        
        SafetyReport {
            fault_scenario: fault.clone(),
            violations,
            cascade_risks,
            recommendations,
            severity: self.calculate_severity(&violations),
        }
    }
}
```

### Practical Examples

#### Example 1: LED Driver Fault Analysis
<!-- doc-check: skip (documents planned fault-injection feature — not yet parsed) -->
```bhdl
testbench TB_LED_Driver_Faults for LEDDriver {
    faults {
        scenario "led_open" {
            override LED1.model = open_circuit;
            
            verify {
                // Current should drop to near zero
                assert R1.current < 1mA after 1us message "Current limiting failed";
                // But voltage might spike
                assert LED1.A.voltage < 10V always message "Voltage spike on LED open";
            }
        }
        
        scenario "led_short" {
            override LED1.forward_voltage = 0.1V;  // Near short
            
            safety_check {
                max_current: 50mA at R1;
                max_power: 0.5W at R1;
                thermal_rise: < 20C at all_components;
            }
        }
        
        scenario "resistor_drift_high" {
            override R1.resistance = R1.resistance * 1.1;  // +10% drift
            
            measure {
                current_change = (nominal_current - R1.current) / nominal_current * 100%;
                brightness_impact = led_brightness_model(R1.current);
            }
        }
    }
}
```

#### Example 2: Power Supply Protection Testing
<!-- doc-check: skip (documents planned fault-injection feature — not yet parsed) -->
```bhdl
testbench TB_Protection_Test for PowerSupply {
    // Test protection circuits
    faults protection_test {
        scenario "output_short" {
            at time: 50ms {
                force @VOUT -> @GND resistance: 0.01;  // 10mΩ short
            }
            
            verify {
                assert I_total < 2A within 100us message "Overcurrent protection failed";
                assert Q1.temperature < 100C always message "Thermal protection failed";
            }
        }
        
        scenario "input_overvoltage" {
            at time: 20ms {
                override @VIN = 30V;  // Well above normal 12V
            }
            
            safety_expect {
                tvs_clamps: true;
                input_current_limited: true;
                no_damage_to: [U1, Q1, C_out];
            }
        }
    }
}
```

## Benefits of Integrated Fault Injection

1. **Comprehensive Testing**: Test both normal operation and failure modes
2. **Safety Validation**: Verify protection mechanisms work correctly
3. **Design Robustness**: Identify weak points before production
4. **Automated Analysis**: Let safety module identify cascade failures
5. **Documentation**: Generate safety analysis reports automatically

## Implementation Roadmap

### Phase 1: Basic Fault Injection (Current Priority)
- Component parameter override
- Simple open/short faults
- Integration with existing SPICE solver

### Phase 2: Safety Analysis Integration
- Connect to bhdl-safety module
- Cascade failure detection
- Automated protection recommendations

### Phase 3: Advanced Features
- Progressive faults
- Monte Carlo with faults
- Fault tree analysis
- Automated report generation

This specification provides a comprehensive framework for fault injection and what-if scenario testing in BHDL, fully integrated with safety analysis capabilities.