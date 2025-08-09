# BHDL Testbench Specification

## Overview

This document specifies the testbench extension to BHDL for simulation control, waveform capture, and verification. Testbenches allow users to define simulation scenarios, specify which signals to monitor, set up stimuli, and verify circuit behavior.

## Testbench Syntax

### Basic Structure

```bhdl
testbench TB_PowerSupply for PowerSupplyBoard {
    // Simulation configuration
    simulation {
        duration: 10ms;
        timestep: 1us;
        solver: adaptive;  // adaptive, fixed, behavioral
        temperature: 25C;
    }
    
    // Waveform capture specification
    scope "power_rails" {
        signals: @VIN, @VOUT, @VCC_3V3;
        trigger: @VIN > 4.5V;
        capture: continuous;
    }
    
    scope "regulation" {
        signals: U1.FB, U1.COMP, @VOUT;
        capture: on_change(threshold: 10mV);
    }
    
    scope "current_monitoring" {
        signals: R_SENSE.current, D1.current;
        capture: periodic(interval: 100us);
    }
    
    // Stimulus definition
    stimulus {
        // Voltage ramp
        @VIN: ramp(from: 0V, to: 12V, duration: 1ms);
        
        // Load current steps
        @LOAD.current: steps([
            (time: 2ms, value: 0mA),
            (time: 3ms, value: 100mA),
            (time: 5ms, value: 500mA),
            (time: 7ms, value: 1A)
        ]);
        
        // Temperature sweep
        ambient_temp: linear(from: -40C, to: 85C, duration: 10ms);
    }
    
    // Assertions and checks
    verify {
        // Steady-state checks
        assert @VOUT in range(4.95V, 5.05V) after 2ms
            message "Output voltage regulation failed";
            
        assert U1.junction_temp < 125C always
            message "Thermal limit exceeded";
            
        // Transient checks
        assert rise_time(@VOUT, 10%, 90%) < 500us
            message "Output rise time too slow";
            
        assert overshoot(@VOUT) < 5%
            message "Excessive output overshoot";
    }
    
    // Measurements
    measure {
        efficiency = (@VOUT * @IOUT) / (@VIN * @IIN) * 100%;
        ripple = peak_to_peak(@VOUT, window: 100us);
        phase_margin = stability_margin(U1).phase;
        load_regulation = (@VOUT[no_load] - @VOUT[full_load]) / @VOUT[no_load] * 100%;
    }
}
```

### Advanced Features

#### 1. Behavioral Models for Testing

```bhdl
testbench TB_Communication for UARTBoard {
    // Define behavioral components for testing
    behavioral UARTGenerator {
        output TX: signal;
        parameter baud_rate: frequency = 115200Hz;
        parameter data: string = "Hello, World!";
        
        behavior {
            for byte in data.bytes() {
                // Start bit
                TX = 0;
                wait(1 / baud_rate);
                
                // Data bits
                for bit in 0..8 {
                    TX = (byte >> bit) & 1;
                    wait(1 / baud_rate);
                }
                
                // Stop bit
                TX = 1;
                wait(1 / baud_rate);
            }
        }
    }
    
    // Instantiate behavioral model
    uart_gen: UARTGenerator(baud_rate: 9600Hz, data: "Test");
    uart_gen.TX -> DUT.RX;
}
```

#### 2. Parametric Sweeps

```bhdl
testbench TB_FilterResponse for FilterCircuit {
    sweep frequency_response {
        parameter input_freq: logarithmic(from: 1Hz, to: 1MHz, points: 100);
        
        stimulus {
            @VIN: sine(amplitude: 1V, frequency: input_freq);
        }
        
        measure {
            gain[input_freq] = 20 * log10(rms(@VOUT) / rms(@VIN));
            phase[input_freq] = phase_shift(@VIN, @VOUT);
        }
        
        plot {
            bode_plot(frequency: input_freq, gain: gain, phase: phase);
        }
    }
}
```

#### 3. Monte Carlo Analysis

```bhdl
testbench TB_Tolerance for VoltageReference {
    monte_carlo {
        runs: 1000;
        
        // Component tolerances
        vary R1.resistance: gaussian(mean: 10k, sigma: 5%);
        vary R2.resistance: gaussian(mean: 10k, sigma: 5%);
        vary U1.reference_voltage: uniform(min: 2.495V, max: 2.505V);
        
        measure {
            output_voltage = @VREF;
            temperature_coefficient = d(@VREF) / d(temperature);
        }
        
        analyze {
            histogram(output_voltage, bins: 50);
            yield = count(output_voltage in range(2.98V, 3.02V)) / total * 100%;
        }
    }
}
```

#### 4. Corner Analysis

```bhdl
testbench TB_Corners for OpAmpCircuit {
    corners {
        // Process corners
        process: [slow, typical, fast];
        
        // Voltage corners  
        supply: [
            (VDD: 4.5V, VSS: -4.5V),  // Min
            (VDD: 5.0V, VSS: -5.0V),  // Typ
            (VDD: 5.5V, VSS: -5.5V)   // Max
        ];
        
        // Temperature corners
        temperature: [-40C, 25C, 85C, 125C];
        
        verify {
            assert gain_bandwidth_product > 1MHz always;
            assert phase_margin > 45deg always;
            assert slew_rate > 0.5V/us always;
        }
    }
}
```

#### 5. Mixed-Signal Testing

```bhdl
testbench TB_ADC for ADCBoard {
    simulation {
        solver: mixed_signal;
        analog_timestep: 1ns;
        digital_timestep: 10ns;
    }
    
    // Analog stimulus
    stimulus analog {
        @VIN: sine(amplitude: 2.5V, frequency: 1kHz, offset: 2.5V);
    }
    
    // Digital stimulus
    stimulus digital {
        CLK: clock(frequency: 10MHz);
        START: pulse(delay: 100ns, width: 50ns, period: 1us);
    }
    
    // Mixed-signal measurements
    measure {
        enob = effective_bits(@VIN, DATA[7:0]);
        snr = signal_to_noise(analog: @VIN, digital: DATA[7:0]);
        latency = time(START.posedge) - time(DONE.posedge);
    }
}
```

### Scope Specification Details

#### Capture Modes

1. **continuous**: Capture every simulation point
2. **on_change**: Capture when signal changes by threshold
3. **periodic**: Capture at fixed intervals
4. **triggered**: Start capture on trigger condition
5. **windowed**: Capture within time windows

```bhdl
scope "detailed_capture" {
    signals: @VOUT, I_LOAD;
    
    capture: triggered {
        start: @VIN > 11V;
        stop: simulation.time > 5ms;
        pre_trigger: 100us;
        post_trigger: 1ms;
    };
    
    format: binary;  // binary, ascii, compressed
    file: "output_transient.vcd";
}
```

### Simulation Control

```bhdl
testbench TB_PowerSequence for MultiRailSupply {
    // Sequenced startup test
    sequence power_up_sequence {
        step "Enable 3.3V" {
            EN_3V3 = 1;
            wait_until(@VCC_3V3 > 3.2V, timeout: 10ms);
        }
        
        step "Enable 1.8V" {
            EN_1V8 = 1;
            wait_until(@VCC_1V8 > 1.7V, timeout: 5ms);
        }
        
        step "Enable 1.2V core" {
            EN_CORE = 1;
            wait_until(@VCC_CORE > 1.15V, timeout: 5ms);
        }
        
        step "Release reset" {
            wait(100us);
            nRESET = 1;
        }
    }
    
    // Fault injection
    inject_fault {
        at time: 8ms {
            force R1.resistance = infinity;  // Open circuit
        }
        
        at @VOUT > 5.5V {
            force Q1.failed = true;  // Transistor failure
        }
    }
}
```

### Output Formats

#### 1. VCD (Value Change Dump)
```bhdl
output vcd {
    file: "simulation.vcd";
    timescale: 1ns;
    hierarchy: full;  // full, flattened, custom
}
```

#### 2. FST (Fast Signal Trace)
```bhdl
output fst {
    file: "simulation.fst";
    compression: lz4;
    hierarchy: preserve;
}
```

#### 3. CSV for Data Analysis
```bhdl
output csv {
    file: "measurements.csv";
    signals: [@VIN, @VOUT, efficiency, ripple];
    delimiter: ",";
    header: true;
}
```

#### 4. Custom Format
```bhdl
output custom {
    format: json;
    file: "results.json";
    
    structure {
        "metadata": {
            "testbench": testbench.name,
            "timestamp": simulation.timestamp,
            "duration": simulation.duration
        },
        "measurements": measure.*,
        "violations": verify.failures
    }
}
```

## Integration with CLI

### Command Structure

```bash
# Run simulation with testbench
bhdl simulate circuit.bhdl --testbench tb_power.bhdl --output results/

# Run specific test scenario
bhdl simulate circuit.bhdl --testbench tb_power.bhdl --scenario "load_step"

# Run parameter sweep
bhdl simulate circuit.bhdl --testbench tb_filter.bhdl --sweep frequency

# Run Monte Carlo analysis
bhdl simulate circuit.bhdl --testbench tb_tolerance.bhdl --monte-carlo --runs 10000

# Interactive simulation
bhdl simulate circuit.bhdl --testbench tb_debug.bhdl --interactive
```

### Configuration File

```toml
[simulation]
default_solver = "adaptive"
default_timestep = "1us"
max_iterations = 1000
convergence_tolerance = 1e-9

[waveforms]
default_format = "fst"
compression = true
hierarchical = true

[output]
directory = "./sim_results"
save_matrices = false
save_convergence_history = true

[performance]
parallel_sweep = true
max_threads = 8
cache_models = true
```

## Implementation Considerations

### 1. Parser Extensions

The BHDL parser needs to be extended to support:
- `testbench` as a top-level construct
- New keywords: `simulation`, `scope`, `stimulus`, `verify`, `measure`
- Behavioral modeling constructs
- Time-based expressions

### 2. Simulation Engine Integration

- Unified interface for SPICE and behavioral simulators
- Event-driven simulation for digital/behavioral parts
- Mixed-signal synchronization
- Efficient waveform storage and compression

### 3. Waveform Capture Architecture

```rust
pub struct WaveformCapture {
    scopes: Vec<Scope>,
    buffers: HashMap<SignalId, SignalBuffer>,
    triggers: Vec<Trigger>,
    output_format: OutputFormat,
}

pub struct Scope {
    name: String,
    signals: Vec<SignalRef>,
    capture_mode: CaptureMode,
    trigger: Option<TriggerCondition>,
}

pub enum CaptureMode {
    Continuous,
    OnChange { threshold: f64 },
    Periodic { interval: Duration },
    Triggered { pre: Duration, post: Duration },
}
```

### 4. Verification Engine

```rust
pub struct VerificationEngine {
    assertions: Vec<Assertion>,
    measurements: HashMap<String, Measurement>,
    coverage: CoverageTracker,
}

pub struct Assertion {
    condition: Expression,
    time_window: TimeWindow,
    severity: Severity,
    message: String,
}
```

## Benefits

1. **Integrated Testing**: No need for external testbench languages
2. **Type Safety**: Leverages BHDL's type system for verification
3. **Reusability**: Testbenches can be shared and parameterized
4. **Comprehensive**: Supports analog, digital, and mixed-signal testing
5. **Standards Compatible**: Outputs industry-standard waveform formats

This testbench system makes BHDL a complete solution for design and verification of electronic circuits.