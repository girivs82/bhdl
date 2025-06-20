# BHDL User Simulation Implementation Plan

## Overview

This document outlines the implementation plan for adding user simulation capabilities to BHDL, enabling board designers to validate their designs through simulation before manufacturing.

## Phase 1: Foundation (Testbench Infrastructure)

### 1.1 AST Extensions
- Add `Testbench` as top-level construct
- Define AST nodes for:
  - `SimulationDirective` (type, duration, settings)
  - `StimulusBlock` (time-based events)
  - `MeasurementBlock` (signal measurements)
  - `AssertionBlock` (validation checks)
  - `PlotDirective` (waveform generation)

### 1.2 Parser Extensions
```rust
// In bhdl-parser
pub fn parse_testbench(&mut self) {
    self.expect(TESTBENCH_KW);
    let name = self.parse_ident();
    self.expect(FOR_KW);
    let circuit_ref = self.parse_ident();
    self.expect(L_BRACE);
    
    // Parse testbench blocks
    while !self.at(R_BRACE) {
        match self.current() {
            SIMULATION_KW => self.parse_simulation_block(),
            STIMULUS_KW => self.parse_stimulus_block(),
            MEASURE_KW => self.parse_measure_block(),
            ASSERT_KW => self.parse_assert_block(),
            PLOT_KW => self.parse_plot_block(),
            _ => self.error("Unknown testbench block"),
        }
    }
}
```

### 1.3 Semantic Analysis
- Validate testbench references valid circuit
- Check measurement references valid nodes/components
- Verify time values are monotonic in stimulus
- Type check expressions in measurements

## Phase 2: Simulation Engine Integration

### 2.1 Simulation Controller
```rust
// In bhdl-sim crate (new)
pub struct SimulationController {
    circuit: Circuit,
    testbench: Testbench,
    engine: Box<dyn SimulationEngine>,
    results: SimulationResults,
}

pub trait SimulationEngine {
    fn run_dc(&mut self, circuit: &Circuit) -> DcResult;
    fn run_transient(&mut self, circuit: &Circuit, config: &TransientConfig) -> TransientResult;
    fn run_ac(&mut self, circuit: &Circuit, config: &AcConfig) -> AcResult;
}
```

### 2.2 Measurement Framework
```rust
pub struct MeasurementEngine {
    measurements: HashMap<String, Measurement>,
    results: HashMap<String, MeasurementResult>,
}

pub enum Measurement {
    Voltage { node: NodeId },
    Current { component: ComponentId },
    Power { component: ComponentId },
    Expression { ast: MeasurementExpr },
}

impl MeasurementEngine {
    pub fn evaluate(&mut self, state: &SimulationState) {
        for (name, measurement) in &self.measurements {
            let value = match measurement {
                Measurement::Voltage { node } => state.get_voltage(*node),
                Measurement::Current { component } => state.get_current(*component),
                // ... etc
            };
            self.results.insert(name.clone(), value);
        }
    }
}
```

### 2.3 Stimulus Application
```rust
pub struct StimulusEngine {
    events: BTreeMap<SimTime, Vec<StimulusEvent>>,
}

pub enum StimulusEvent {
    SetVoltage { source: ComponentId, value: f64 },
    SetCurrent { source: ComponentId, value: f64 },
    SetLoad { node: NodeId, value: f64 },
    Ramp { source: ComponentId, target: f64, duration: f64 },
}
```

## Phase 3: Waveform Generation

### 3.1 Data Collection
```rust
pub struct WaveformRecorder {
    signals: HashMap<String, SignalRecording>,
    sample_rate: f64,
}

pub struct SignalRecording {
    times: Vec<f64>,
    values: Vec<f64>,
    signal_type: SignalType,
}
```

### 3.2 Plotting Engine
```rust
// Using plotters crate
pub struct PlotEngine {
    backend: Box<dyn DrawingBackend>,
}

impl PlotEngine {
    pub fn plot_waveform(&self, data: &WaveformData, config: &PlotConfig) -> Result<()> {
        let root = self.backend.into_drawing_area();
        let mut chart = ChartBuilder::on(&root)
            .caption(&config.title, ("sans-serif", 40))
            .x_label_area_size(35)
            .y_label_area_size(40)
            .build_cartesian_2d(
                config.x_range.clone(),
                config.y_range.clone()
            )?;
            
        // Plot signals
        for signal in &data.signals {
            chart.draw_series(
                LineSeries::new(
                    signal.points.iter().map(|&(x, y)| (x, y)),
                    &signal.color,
                )
            )?;
        }
        
        Ok(())
    }
}
```

### 3.3 Interactive Viewer (Future)
- HTML/JavaScript viewer using wasm-bindgen
- Zoom, pan, measure cursors
- Export to CSV/Excel

## Phase 4: Assertion Engine

### 4.1 Assertion Evaluation
```rust
pub struct AssertionEngine {
    assertions: Vec<Assertion>,
    results: Vec<AssertionResult>,
}

pub enum Assertion {
    InRange { signal: String, min: f64, max: f64, condition: TimeCondition },
    Always { expression: AssertionExpr },
    Eventually { expression: AssertionExpr, timeout: f64 },
}

pub enum TimeCondition {
    After(f64),
    Between(f64, f64),
    Always,
    AtSteadyState,
}
```

### 4.2 Reporting
```rust
pub struct SimulationReport {
    summary: ReportSummary,
    measurements: HashMap<String, f64>,
    assertions: Vec<AssertionResult>,
    plots: Vec<PathBuf>,
}

impl SimulationReport {
    pub fn generate_html(&self, template: &str) -> String {
        // Use handlebars or similar for templating
    }
    
    pub fn generate_json(&self) -> serde_json::Value {
        // Machine-readable format
    }
}
```

## Phase 5: Advanced Features

### 5.1 Parameter Sweeps
```rust
pub struct ParameterSweep {
    parameters: Vec<SweepParameter>,
    analysis: Box<dyn SimulationEngine>,
}

pub struct SweepParameter {
    target: ParameterTarget,
    values: Vec<f64>,
}

pub enum ParameterTarget {
    ComponentValue { instance: String, param: String },
    Temperature,
    SupplyVoltage { net: String },
}
```

### 5.2 Monte Carlo Analysis
```rust
pub struct MonteCarloEngine {
    variations: HashMap<ComponentType, Distribution>,
    num_runs: usize,
}

pub enum Distribution {
    Gaussian { mean: f64, std_dev: f64 },
    Uniform { min: f64, max: f64 },
    Tolerance { nominal: f64, percent: f64 },
}
```

## Phase 6: CLI Integration

### 6.1 New CLI Commands
```bash
# Run simulation
bhdl simulate <circuit.bhdl> --testbench <test.bhdl> [--output <dir>]

# List available testbenches
bhdl simulate --list <circuit.bhdl>

# Run all testbenches
bhdl simulate <circuit.bhdl> --all

# Interactive mode (future)
bhdl simulate --interactive <circuit.bhdl>
```

### 6.2 Output Organization
```
output/
├── summary.txt           # Pass/fail summary
├── report.html          # Full HTML report
├── measurements.json    # Machine-readable results
├── waveforms/
│   ├── transient_response.svg
│   ├── bode_plot.svg
│   └── efficiency_curve.svg
└── data/
    ├── transient.csv    # Raw simulation data
    └── ac_sweep.csv
```

## Implementation Priority

1. **MVP (Minimum Viable Product)**
   - Basic testbench parser
   - DC and transient simulation
   - Simple measurements (voltage, current)
   - Basic waveform plots
   - Pass/fail assertions

2. **Enhanced Features**
   - AC analysis
   - Parameter sweeps
   - Advanced measurements (RMS, FFT, settling time)
   - Multi-panel plots
   - HTML reports

3. **Advanced Features**
   - Monte Carlo analysis
   - Optimization
   - Co-simulation interfaces
   - Interactive viewer

## Technical Challenges

1. **Performance**
   - Large transient simulations
   - Multi-core parallelization for sweeps
   - Memory management for waveform data

2. **Accuracy**
   - Model fidelity
   - Numerical stability
   - Convergence issues

3. **Usability**
   - Intuitive syntax
   - Good error messages
   - Reasonable defaults

## Success Metrics

- Users can validate designs before PCB fabrication
- Simulation results match real-world measurements within 10%
- Common analyses complete in < 1 minute
- 90% of assertions can be expressed in the language
- Waveforms are publication-quality

## Next Steps

1. Create RFC for testbench syntax
2. Implement basic parser extensions
3. Build simulation controller architecture
4. Create proof-of-concept with buck converter
5. Gather user feedback and iterate