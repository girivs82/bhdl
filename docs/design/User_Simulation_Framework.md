# BHDL User Simulation Framework Design

## Overview

This document outlines the design for user-driven simulation capabilities in BHDL, allowing board designers to create testbenches, run simulations, and generate waveforms similar to IC design tools.

## Motivation

Board designers need to validate their designs before manufacturing:
- Measure critical signals (e.g., inductor current in buck converters)
- Verify timing relationships
- Check voltage/current limits
- Analyze transient response
- Validate power sequencing
- Ensure stability margins

## Proposed Syntax

### 1. Testbench Definition

```bhdl
// Buck converter with testbench
board BuckConverter {
    power VIN = 12V @ 2A;
    power VOUT = 3.3V @ 1A;
    ground GND;
    
    // Circuit definition...
    VIN -> L1: Inductor(10µH).1;
    // ... rest of circuit
}

// Testbench for the buck converter
testbench BuckValidation for BuckConverter {
    // Simulation parameters
    simulation {
        type: transient;
        duration: 10ms;
        timestep: 10ns;
        temperature: 25°C;
    }
    
    // Input stimuli
    stimulus {
        // Voltage step at 2ms
        @2ms: VIN = 12V;
        
        // Load step at 5ms
        @5ms: load(VOUT) = 1A;
        
        // Input voltage dip at 7ms
        @7ms: VIN = 10V;
        @8ms: VIN = 12V;
    }
    
    // Measurements
    measure {
        // Inductor current
        IL: current(L1);
        
        // Output voltage ripple
        VOUT_RIPPLE: ripple(VOUT);
        
        // Efficiency
        EFFICIENCY: power(VOUT) / power(VIN) * 100%;
        
        // Settling time
        SETTLING_TIME: settling_time(VOUT, 3.3V, 0.05);
    }
    
    // Assertions/Checks
    assert {
        // Output voltage within 5%
        VOUT in range(3.135V, 3.465V) after 1ms;
        
        // Inductor current limit
        IL < 3A always;
        
        // Efficiency target
        EFFICIENCY > 85% after settling;
    }
    
    // Waveform outputs
    plot {
        // Time domain plots
        waveform("inductor_current.svg") {
            IL vs time;
            title: "Buck Converter Inductor Current";
            ylabel: "Current (A)";
        }
        
        waveform("output_voltage.svg") {
            VOUT vs time;
            VIN/4 vs time;  // Reference
            title: "Output Voltage Transient Response";
            ylabel: "Voltage (V)";
        }
        
        // FFT/Frequency domain
        spectrum("output_noise.svg") {
            fft(VOUT) from 1kHz to 1MHz;
            title: "Output Voltage Spectrum";
            ylabel: "Magnitude (dBV)";
        }
    }
}
```

### 2. AC Analysis Testbench

```bhdl
testbench FrequencyResponse for PowerSupply {
    simulation {
        type: ac;
        start: 1Hz;
        stop: 10MHz;
        points: 100;
        scale: logarithmic;
    }
    
    // Small signal injection
    stimulus {
        ac_source(VOUT, 10mV);
    }
    
    measure {
        // Loop gain
        LOOP_GAIN: gain(VOUT/VIN);
        PHASE: phase(VOUT/VIN);
        
        // Stability margins
        PHASE_MARGIN: phase_margin();
        GAIN_MARGIN: gain_margin();
        CROSSOVER_FREQ: unity_gain_frequency();
    }
    
    assert {
        PHASE_MARGIN > 45°;
        GAIN_MARGIN > 10dB;
    }
    
    plot {
        bode("loop_response.svg") {
            magnitude: LOOP_GAIN;
            phase: PHASE;
            title: "Power Supply Loop Response";
        }
    }
}
```

### 3. Monte Carlo Analysis

```bhdl
testbench MonteCarloAnalysis for LEDDriver {
    simulation {
        type: monte_carlo;
        runs: 1000;
        analysis: dc;
        
        // Component variations
        variations {
            resistors: 5%;      // 5% tolerance
            capacitors: 10%;    // 10% tolerance
            inductors: 20%;     // 20% tolerance
        }
    }
    
    measure {
        LED_CURRENT: current(LED1);
        POWER_DISSIPATION: power(total);
    }
    
    plot {
        histogram("led_current_distribution.svg") {
            LED_CURRENT;
            bins: 50;
            title: "LED Current Distribution";
        }
        
        scatter("power_vs_current.svg") {
            x: LED_CURRENT;
            y: POWER_DISSIPATION;
            title: "Power vs LED Current";
        }
    }
}
```

### 4. Parameter Sweep

```bhdl
testbench EfficiencySweep for BuckConverter {
    simulation {
        type: parameter_sweep;
        analysis: dc;
        
        sweep {
            // Sweep load current
            parameter: load(VOUT);
            from: 0.1A;
            to: 2A;
            steps: 20;
        }
    }
    
    measure {
        EFFICIENCY: power(VOUT) / power(VIN) * 100%;
        VOUT_VOLTAGE: voltage(VOUT);
    }
    
    plot {
        xy("efficiency_curve.svg") {
            x: load(VOUT);
            y: EFFICIENCY;
            title: "Efficiency vs Load Current";
            xlabel: "Load Current (A)";
            ylabel: "Efficiency (%)";
        }
    }
}
```

## Implementation Architecture

### 1. Testbench Parser Extension
- Add testbench as a top-level construct in BHDL grammar
- Support simulation directives, stimulus, measurements, assertions, and plotting

### 2. Simulation Controller
```rust
pub struct SimulationController {
    testbench: Testbench,
    circuit: Circuit,
    engine: SimulationEngine,
}

impl SimulationController {
    pub fn run(&mut self) -> SimulationResults {
        // 1. Apply initial conditions
        // 2. Run simulation based on type
        // 3. Collect measurements
        // 4. Check assertions
        // 5. Generate plots
    }
}
```

### 3. Measurement Framework
```rust
pub enum Measurement {
    Voltage(NodeId),
    Current(ComponentId),
    Power(PowerDomain),
    Ripple(NodeId),
    SettlingTime(NodeId, f64, f64),
    Custom(Box<dyn Fn(&SimulationState) -> f64>),
}
```

### 4. Waveform Generation
- Use `plotters` crate for SVG generation
- Support time-domain and frequency-domain plots
- Interactive HTML output option using `wasm-bindgen`

### 5. Integration Points

#### CLI Command
```bash
bhdl simulate circuit.bhdl --testbench validation.bhdl --output results/
```

#### Output Structure
```
results/
├── summary.txt          # Simulation summary and assertions
├── measurements.json    # All measured values
├── waveforms/          # Generated plots
│   ├── inductor_current.svg
│   ├── output_voltage.svg
│   └── efficiency_curve.svg
└── data/               # Raw simulation data
    └── transient.csv
```

## Advanced Features

### 1. Multi-Domain Simulation
- Thermal analysis (junction temperatures)
- EMI pre-compliance (conducted emissions)
- Power integrity (PDN impedance)

### 2. Co-Simulation
- IBIS models for high-speed signals
- S-parameter blocks for RF sections
- Behavioral models for digital controllers

### 3. Design Space Exploration
- Optimization goals (minimize power, maximize efficiency)
- Automated component selection based on simulation results
- Pareto frontier visualization

### 4. Interactive Debugging
- Probe placement during simulation
- Breakpoints on conditions
- State inspection and modification

## Benefits

1. **Early Validation**: Catch issues before PCB fabrication
2. **Design Confidence**: Verify performance across conditions
3. **Documentation**: Simulation results as design documentation
4. **Optimization**: Find optimal component values
5. **Education**: Learn circuit behavior through visualization

## Next Steps

1. Implement testbench parser extensions
2. Create simulation controller architecture
3. Build measurement and assertion framework
4. Integrate plotting capabilities
5. Add example testbenches for common circuits

## Example Use Cases

### Power Supply Validation
- Load transient response
- Line regulation
- Efficiency curves
- Thermal derating

### LED Driver Testing
- Current regulation accuracy
- PWM dimming response
- Thermal runaway protection
- EMI compliance

### Motor Controller Verification
- Current limiting
- Back-EMF handling
- Protection circuit validation
- Efficiency optimization