# BHDL Testbench Infrastructure Implementation

## Overview

This document describes the implementation architecture for adding testbench capabilities to bhdl-cli, including testbench support, waveform capture, and coordination between existing simulation engines (bhdl-sim for behavioral and bhdl-spice for electrical).

## Architecture

### Component Structure

The testbench infrastructure will be added to the existing crates rather than creating a new simulation crate:

```
bhdl-testbench/           # New crate for testbench framework
├── src/
│   ├── lib.rs           # Public API
│   ├── parser.rs        # Testbench parsing extensions
│   ├── compiler.rs      # Testbench compilation
│   ├── coordinator.rs   # Simulation coordination
│   └── waveform/        # Waveform capture
│       ├── mod.rs
│       ├── capture.rs
│       ├── formats/
│       │   ├── vcd.rs
│       │   ├── fst.rs
│       │   └── csv.rs
│       └── buffer.rs

bhdl-cli/                # Extended with simulation commands
├── src/
│   └── commands/
│       └── simulate.rs  # New simulation command

bhdl-parser/             # Extended with testbench syntax
├── src/
│   └── testbench.rs     # Testbench parsing

bhdl-ast/                # Extended with testbench AST nodes
├── src/
│   └── testbench.rs     # Testbench AST definitions
```

### Core Types

```rust
use std::collections::HashMap;
use std::time::Duration;
use bhdl_netlist::{Netlist, NetId, InstanceId};

/// Main simulation configuration
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub duration: Duration,
    pub timestep: Duration,
    pub solver: SolverType,
    pub temperature: f64,
    pub output_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub enum SolverType {
    Adaptive,
    Fixed { timestep: Duration },
    Behavioral,
    MixedSignal {
        analog_timestep: Duration,
        digital_timestep: Duration,
    },
}

/// Testbench definition
#[derive(Debug, Clone)]
pub struct Testbench {
    pub name: String,
    pub target_board: String,
    pub config: SimulationConfig,
    pub scopes: Vec<Scope>,
    pub stimuli: Vec<Stimulus>,
    pub assertions: Vec<Assertion>,
    pub measurements: HashMap<String, Measurement>,
}

/// Waveform capture scope
#[derive(Debug, Clone)]
pub struct Scope {
    pub name: String,
    pub signals: Vec<SignalRef>,
    pub capture_mode: CaptureMode,
    pub trigger: Option<TriggerCondition>,
    pub output_file: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub enum SignalRef {
    Net(String),                    // @VCC, @GND
    Pin(String, String),            // U1.FB
    Current(String),                // R1.current
    Power(String),                  // R1.power
    Expression(Box<Expression>),    // Complex expressions
}

#[derive(Debug, Clone)]
pub enum CaptureMode {
    Continuous,
    OnChange { threshold: f64 },
    Periodic { interval: Duration },
    Triggered {
        start_condition: TriggerCondition,
        stop_condition: Option<TriggerCondition>,
        pre_trigger: Duration,
        post_trigger: Duration,
    },
    Windowed {
        windows: Vec<TimeWindow>,
    },
}

/// Stimulus definition
#[derive(Debug, Clone)]
pub struct Stimulus {
    pub target: SignalRef,
    pub waveform: Waveform,
}

#[derive(Debug, Clone)]
pub enum Waveform {
    Constant(f64),
    Ramp {
        start_value: f64,
        end_value: f64,
        duration: Duration,
    },
    Steps(Vec<(Duration, f64)>),
    Sine {
        amplitude: f64,
        frequency: f64,
        offset: f64,
        phase: f64,
    },
    Pulse {
        low: f64,
        high: f64,
        delay: Duration,
        width: Duration,
        period: Duration,
    },
    Custom(Box<dyn Fn(f64) -> f64>),
}
```

### Testbench Coordinator

```rust
use bhdl_spice::{Circuit, AdaptiveCircuitSolver, AnalysisResult};
use bhdl_sim::{SimulationEngine, EventQueue};

/// Main testbench coordinator that interfaces with existing simulators
pub struct TestbenchCoordinator {
    config: SimulationConfig,
    netlist: Netlist,
    testbench: Testbench,
    
    // Existing simulation engines
    spice_solver: Option<AdaptiveCircuitSolver>,
    behavioral_engine: Option<SimulationEngine>,
    
    // Waveform capture
    waveform_capture: WaveformCapture,
    
    // Verification
    verification_engine: VerificationEngine,
    
    // Current simulation state
    current_time: f64,
    signal_values: HashMap<SignalId, SignalValue>,
}

#[derive(Debug, Clone)]
pub enum SignalValue {
    Analog(f64),
    Digital(bool),
    Logic(LogicLevel),
}

impl TestbenchCoordinator {
    pub fn new(
        netlist: Netlist,
        testbench: Testbench,
        analysis_data: Option<AnalysisData>,
    ) -> Result<Self> {
        // Create appropriate solver based on configuration
        let (spice_solver, behavioral_engine) = match &testbench.config.solver {
            SolverType::Spice | SolverType::Adaptive => {
                // Convert netlist to SPICE circuit
                let circuit = convert_to_spice_circuit(&netlist, &analysis_data)?;
                let solver = AdaptiveCircuitSolver::new(circuit);
                (Some(solver), None)
            }
            SolverType::Behavioral => {
                // Create behavioral simulation engine
                let engine = SimulationEngine::from_netlist(&netlist)?;
                (None, Some(engine))
            }
            SolverType::MixedSignal { .. } => {
                // Create both engines for mixed-signal
                let circuit = convert_to_spice_circuit(&netlist, &analysis_data)?;
                let spice = AdaptiveCircuitSolver::new(circuit);
                let behavioral = SimulationEngine::from_netlist(&netlist)?;
                (Some(spice), Some(behavioral))
            }
        };
        
        // Set up waveform capture
        let waveform_capture = WaveformCapture::new(&testbench.scopes)?;
        
        // Set up verification
        let verification_engine = VerificationEngine::new(
            &testbench.assertions,
            &testbench.measurements,
        )?;
        
        Ok(Self {
            config: testbench.config.clone(),
            netlist,
            testbench,
            spice_solver,
            behavioral_engine,
            waveform_capture,
            verification_engine,
            current_time: 0.0,
            signal_values: HashMap::new(),
        })
    }
    
    pub fn run(&mut self) -> Result<SimulationResults> {
        // Apply initial conditions
        self.apply_initial_conditions()?;
        
        // Main simulation loop
        while self.current_time < self.config.duration.as_secs_f64() {
            // Apply stimuli
            self.apply_stimuli()?;
            
            // Step simulation
            match &self.config.solver {
                SolverType::Adaptive => self.step_adaptive()?,
                SolverType::Fixed { timestep } => self.step_fixed(*timestep)?,
                SolverType::Behavioral => self.step_behavioral()?,
                SolverType::MixedSignal { .. } => self.step_mixed_signal()?,
            }
            
            // Capture waveforms
            self.waveform_capture.capture(
                self.current_time,
                &self.signal_values,
            )?;
            
            // Check assertions
            self.verification_engine.check(
                self.current_time,
                &self.signal_values,
            )?;
            
            // Update measurements
            self.verification_engine.update_measurements(
                self.current_time,
                &self.signal_values,
            )?;
        }
        
        // Finalize and return results
        self.finalize_simulation()
    }
}
```

### Waveform Capture Implementation

```rust
pub struct WaveformCapture {
    scopes: Vec<ActiveScope>,
    output_writers: Vec<Box<dyn WaveformWriter>>,
}

pub struct ActiveScope {
    definition: Scope,
    signal_buffers: HashMap<SignalId, SignalBuffer>,
    trigger_state: TriggerState,
}

pub struct SignalBuffer {
    signal_id: SignalId,
    timestamps: Vec<f64>,
    values: Vec<f64>,
    
    // Compression
    last_value: Option<f64>,
    last_timestamp: Option<f64>,
}

impl SignalBuffer {
    pub fn add_sample(&mut self, timestamp: f64, value: f64, mode: &CaptureMode) {
        match mode {
            CaptureMode::Continuous => {
                self.timestamps.push(timestamp);
                self.values.push(value);
            }
            CaptureMode::OnChange { threshold } => {
                if let Some(last) = self.last_value {
                    if (value - last).abs() >= *threshold {
                        self.timestamps.push(timestamp);
                        self.values.push(value);
                        self.last_value = Some(value);
                    }
                } else {
                    self.timestamps.push(timestamp);
                    self.values.push(value);
                    self.last_value = Some(value);
                }
            }
            _ => {} // Other modes handled at scope level
        }
    }
}

/// Trait for waveform output formats
pub trait WaveformWriter: Send + Sync {
    fn initialize(&mut self, signals: &[SignalInfo]) -> Result<()>;
    fn write_timestamp(&mut self, timestamp: f64) -> Result<()>;
    fn write_value(&mut self, signal_id: SignalId, value: f64) -> Result<()>;
    fn finalize(&mut self) -> Result<()>;
}
```

### CLI Integration

```rust
// In bhdl-cli/src/commands/simulate.rs

use clap::Args;
use bhdl_simulation::{SimulationCoordinator, Testbench};

#[derive(Args)]
pub struct SimulateCommand {
    /// Input BHDL circuit file
    #[arg(value_name = "FILE")]
    pub input: PathBuf,
    
    /// Testbench file
    #[arg(short, long)]
    pub testbench: PathBuf,
    
    /// Output directory
    #[arg(short, long, default_value = "./sim_results")]
    pub output: PathBuf,
    
    /// Specific test scenario to run
    #[arg(long)]
    pub scenario: Option<String>,
    
    /// Enable interactive mode
    #[arg(long)]
    pub interactive: bool,
    
    /// Waveform format
    #[arg(long, default_value = "vcd")]
    pub format: WaveformFormat,
}

pub fn execute_simulate(args: SimulateCommand) -> Result<()> {
    // Parse circuit
    let circuit_ast = parse_bhdl_file(&args.input)?;
    
    // Analyze circuit
    let (netlist, analysis_data) = analyze_circuit(circuit_ast)?;
    
    // Parse testbench
    let testbench_ast = parse_testbench_file(&args.testbench)?;
    let testbench = compile_testbench(testbench_ast, &netlist)?;
    
    // Create simulation coordinator
    let mut coordinator = SimulationCoordinator::new(
        netlist,
        testbench,
        Some(analysis_data),
    )?;
    
    // Run simulation
    if args.interactive {
        run_interactive_simulation(&mut coordinator)?;
    } else {
        let results = coordinator.run()?;
        
        // Save results
        save_simulation_results(&results, &args.output)?;
        
        // Print summary
        print_simulation_summary(&results);
    }
    
    Ok(())
}
```

### Behavioral Simulation Engine

```rust
/// Behavioral simulation for digital and high-level models
pub struct BehavioralSimulator {
    event_queue: BinaryHeap<Event>,
    processes: Vec<Box<dyn Process>>,
    signal_values: HashMap<SignalId, LogicValue>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LogicValue {
    Low,
    High,
    HighZ,
    Unknown,
    Analog(f64),
}

pub struct Event {
    time: f64,
    signal_id: SignalId,
    new_value: LogicValue,
}

pub trait Process: Send + Sync {
    fn sensitivity_list(&self) -> Vec<SignalId>;
    fn execute(&mut self, signals: &HashMap<SignalId, LogicValue>) -> Vec<Event>;
}

impl BehavioralSimulator {
    pub fn step(&mut self, until_time: f64) -> Result<()> {
        while let Some(event) = self.event_queue.peek() {
            if event.time > until_time {
                break;
            }
            
            let event = self.event_queue.pop().unwrap();
            
            // Update signal
            self.signal_values.insert(event.signal_id, event.new_value);
            
            // Find sensitive processes
            for process in &mut self.processes {
                if process.sensitivity_list().contains(&event.signal_id) {
                    let new_events = process.execute(&self.signal_values);
                    for new_event in new_events {
                        self.event_queue.push(new_event);
                    }
                }
            }
        }
        
        Ok(())
    }
}
```

### Mixed-Signal Synchronization

```rust
pub struct MixedSignalCoordinator {
    analog_solver: SpiceSimulator,
    digital_solver: BehavioralSimulator,
    
    // Interface points
    a2d_converters: Vec<A2DConverter>,
    d2a_converters: Vec<D2AConverter>,
    
    // Synchronization
    analog_timestep: Duration,
    digital_timestep: Duration,
    next_sync_time: f64,
}

pub struct A2DConverter {
    analog_signal: SignalId,
    digital_signal: SignalId,
    threshold_low: f64,
    threshold_high: f64,
    propagation_delay: Duration,
}

pub struct D2AConverter {
    digital_signal: SignalId,
    analog_signal: SignalId,
    low_voltage: f64,
    high_voltage: f64,
    rise_time: Duration,
    fall_time: Duration,
}
```

## Implementation Phases

### Phase 1: Basic Infrastructure (Week 1-2)
1. Create bhdl-simulation crate
2. Implement basic testbench parser
3. Add simulation configuration structures
4. Create waveform capture framework

### Phase 2: SPICE Integration (Week 3-4)
1. Integrate with existing bhdl-spice solver
2. Implement stimulus generation
3. Add continuous waveform capture
4. Create VCD output writer

### Phase 3: CLI Integration (Week 5)
1. Add simulate command to bhdl-cli
2. Implement testbench compilation
3. Add results visualization
4. Create simulation summary reports

### Phase 4: Advanced Features (Week 6-8)
1. Add behavioral simulation engine
2. Implement mixed-signal coordination
3. Add verification engine
4. Support multiple output formats

### Phase 5: Testing and Documentation (Week 9-10)
1. Create comprehensive test suite
2. Write user documentation
3. Add example testbenches
4. Performance optimization

## Benefits

1. **Unified Environment**: Design and verification in one language
2. **Type Safety**: Catch errors at compile time
3. **Performance**: Optimized waveform capture and storage
4. **Flexibility**: Support for various simulation types
5. **Standards**: Industry-standard output formats

This infrastructure provides a solid foundation for simulation capabilities in BHDL.