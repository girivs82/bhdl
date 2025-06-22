# BHDL Simulation Data Capture and Output System

## Overview

The BHDL simulator includes a comprehensive data capture and output system that allows users to record simulation results for analysis and visualization. The system supports multiple capture modes, flexible triggering, and various output formats.

## Architecture

### Core Components

1. **Waveform Capture** (`output/waveform.rs`)
   - Manages time-series data collection
   - Supports different capture modes
   - Implements automatic compression

2. **Probe Management** (`output/probe.rs`)
   - Configures what signals to capture
   - Implements trigger conditions
   - Supports auto-probing

3. **VCD Writer** (`output/vcd.rs`)
   - Generates industry-standard VCD files
   - Supports hierarchical signal organization
   - Handles both digital and analog values

4. **Output Formats** (`output/formats.rs`)
   - CSV format for spreadsheet analysis
   - JSON format for programmatic processing
   - Extensible trait-based system

## Waveform Capture

### Capture Modes

```rust
pub enum CaptureMode {
    AllChanges,    // Capture every value change
    Periodic,      // Sample at regular intervals
    EventDriven,   // Capture on specific events
}
```

### Signal Trace Storage

```rust
pub struct SignalTrace {
    pub name: String,
    pub points: Vec<TimePoint>,
    pub metadata: HashMap<String, String>,
}

pub struct TimePoint {
    pub time: f64,
    pub value: PinValue,
}
```

### Compression

The system automatically compresses traces when they exceed memory limits:
- Removes redundant middle points in constant sequences
- Implements aggressive compression for alternating signals
- Preserves critical transition points

## Probe System

### Probe Types

```rust
pub enum ProbeType {
    Pin { instance: InstanceId, pin: String },
    Net { net: NetId },
    Expression { expr: String },
    Bus { signals: Vec<String> },
}
```

### Trigger Conditions

```rust
pub enum TriggerCondition {
    RisingEdge,
    FallingEdge,
    AnyEdge,
    ValueEquals(PinValue),
    ValueInRange { min: f64, max: f64 },
    Expression(String),
}
```

### Auto-Probing

The system can automatically create probes based on:
- Signal name patterns (wildcards supported)
- Hierarchical depth limits
- Component types

## VCD Output

### Features

- Standard VCD format compatible with waveform viewers
- Hierarchical module organization
- Support for various timescales (fs to s)
- Both digital and analog signal representation

### Example VCD Structure

```vcd
$date
   Thu Nov 23 10:30:00 2023
$end
$version
   BHDL Simulator 1.0
$end
$timescale 1ns $end
$scope module top $end
$var wire 1 ! clk $end
$var wire 8 " data $end
$upscope $end
$enddefinitions $end
$dumpvars
0!
x"
$end
#0
0!
#5
1!
#10
0!
b10101010 "
```

## Output Formats

### CSV Format

```csv
Time,Signal1,Signal2,Signal3
0.0,0,1,2.5
1e-9,1,1,2.5
2e-9,1,0,3.0
```

### JSON Format

```json
{
  "simulation": {
    "version": "1.0",
    "date": "2023-11-23T10:30:00Z",
    "timescale": "1ns"
  },
  "signals": [
    {
      "name": "clk",
      "signal_type": "wire",
      "width": 1
    }
  ],
  "data": [
    {
      "time": 0.0,
      "values": {
        "clk": "0"
      }
    }
  ]
}
```

## Usage Examples

### Basic Probe Setup

```rust
// Create probe manager
let mut probe_manager = ProbeManager::new(10000);

// Add a clock probe with rising edge trigger
let clk_probe = Probe {
    name: "clk_probe".to_string(),
    probe_type: ProbeType::Pin {
        instance: cpu_instance,
        pin: "CLK".to_string(),
    },
    enabled: true,
    metadata: HashMap::new(),
    triggers: vec![TriggerCondition::RisingEdge],
};
probe_manager.add_probe(clk_probe)?;

// Enable auto-probing for all clock signals
probe_manager.set_auto_probe(true, vec!["clk*".to_string()]);
```

### VCD Generation

```rust
// Create VCD writer
let config = VcdConfig {
    timescale: "1ns".to_string(),
    date: Utc::now(),
    version: "BHDL Sim 1.0".to_string(),
    comment: Some("CPU simulation".to_string()),
};
let mut vcd_writer = VcdWriter::new("output.vcd", config)?;

// Write header and data
vcd_writer.write_header(&waveform_capture)?;
vcd_writer.write_initial_values(&waveform_capture)?;
vcd_writer.write_all_traces(&waveform_capture)?;
vcd_writer.finish()?;
```

### CSV Export

```rust
// Create CSV writer
let mut csv_writer = CsvWriter::new("output.csv")?;

// Write traces
csv_writer.write_traces(&waveform_capture)?;
csv_writer.finish()?;
```

## Performance Considerations

1. **Memory Management**
   - Automatic compression reduces memory usage
   - Configurable maximum points per signal
   - Efficient storage of unchanged values

2. **Capture Optimization**
   - Selective capture based on probes
   - Trigger conditions reduce data volume
   - Periodic sampling for long simulations

3. **Output Efficiency**
   - Streaming writes for large datasets
   - Buffered I/O for better performance
   - Incremental capture during simulation

## Integration with Simulation Engine

The data capture system integrates seamlessly with the simulation engine:

```rust
// During simulation step
for (path, value) in changed_values {
    probe_manager.capture_value(&path, current_time, value)?;
}

// After simulation
let capture = probe_manager.get_capture();
vcd_writer.write_all_traces(capture)?;
```

## Future Enhancements

1. **Additional Output Formats**
   - FST (Fast Signal Trace) for better compression
   - LXT2 for legacy tool compatibility
   - Custom binary formats for performance

2. **Advanced Triggering**
   - Complex boolean expressions
   - State machine triggers
   - Cross-signal correlation

3. **Real-time Streaming**
   - Live waveform updates
   - Network streaming support
   - Integration with external viewers

## Testing

The system includes comprehensive tests for:
- All capture modes
- Trigger conditions
- Compression algorithms
- Output format correctness
- Edge cases and error handling

See `bhdl-sim/src/output/` for test implementations.