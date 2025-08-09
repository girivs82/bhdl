//! Testbench definition structures

use std::collections::HashMap;
use crate::{SignalRef, TimeSpec, TimeUnit, Result};

/// Complete testbench definition
#[derive(Debug, Clone)]
pub struct Testbench {
    pub name: String,
    pub target_board: String,
    pub simulation_config: SimulationConfig,
    pub scopes: Vec<Scope>,
    pub stimuli: Vec<Stimulus>,
    pub assertions: Vec<Assertion>,
    pub measurements: HashMap<String, Measurement>,
}

/// Simulation configuration
#[derive(Debug, Clone)]
pub struct SimulationConfig {
    pub duration: TimeSpec,
    pub timestep: TimeSpec,
    pub solver_type: SolverType,
    pub temperature: f64,
    pub save_matrices: bool,
}

impl Default for SimulationConfig {
    fn default() -> Self {
        Self {
            duration: TimeSpec { value: 1.0, unit: TimeUnit::Milliseconds },
            timestep: TimeSpec { value: 1.0, unit: TimeUnit::Microseconds },
            solver_type: SolverType::SpiceAdaptive,
            temperature: 27.0,
            save_matrices: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum SolverType {
    /// Use bhdl-spice adaptive solver
    SpiceAdaptive,
    /// Use bhdl-spice with fixed timestep
    SpiceFixed,
    /// Use bhdl-sim behavioral engine
    Behavioral,
    /// Mixed signal using both engines
    MixedSignal {
        analog_timestep: TimeSpec,
        digital_timestep: TimeSpec,
    },
}

/// Waveform capture scope
#[derive(Debug, Clone)]
pub struct Scope {
    pub name: String,
    pub signals: Vec<SignalRef>,
    pub capture_mode: CaptureMode,
    pub trigger: Option<TriggerCondition>,
    pub output_file: Option<String>,
}

#[derive(Debug, Clone)]
pub enum CaptureMode {
    /// Capture every simulation point
    Continuous,
    
    /// Capture when signal changes by threshold
    OnChange { threshold: f64 },
    
    /// Capture at fixed intervals
    Periodic { interval: TimeSpec },
    
    /// Start capture on trigger
    Triggered {
        pre_trigger: TimeSpec,
        post_trigger: TimeSpec,
    },
    
    /// Capture within time windows
    Windowed {
        windows: Vec<TimeWindow>,
    },
}

#[derive(Debug, Clone)]
pub struct TimeWindow {
    pub start: TimeSpec,
    pub end: TimeSpec,
}

#[derive(Debug, Clone)]
pub struct TriggerCondition {
    pub signal: SignalRef,
    pub condition: TriggerType,
}

#[derive(Debug, Clone)]
pub enum TriggerType {
    Rising { threshold: f64 },
    Falling { threshold: f64 },
    Above { value: f64 },
    Below { value: f64 },
    InRange { min: f64, max: f64 },
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
        duration: TimeSpec,
    },
    Steps(Vec<(TimeSpec, f64)>),
    Sine {
        amplitude: f64,
        frequency: f64,
        offset: f64,
        phase: f64,
    },
    Pulse {
        low: f64,
        high: f64,
        delay: TimeSpec,
        width: TimeSpec,
        period: TimeSpec,
    },
}

/// Assertion for verification
#[derive(Debug, Clone)]
pub struct Assertion {
    pub name: String,
    pub condition: AssertionCondition,
    pub time_constraint: TimeConstraint,
    pub severity: Severity,
    pub message: String,
}

#[derive(Debug, Clone)]
pub enum AssertionCondition {
    SignalInRange {
        signal: SignalRef,
        min: f64,
        max: f64,
    },
    SignalEquals {
        signal: SignalRef,
        value: f64,
        tolerance: f64,
    },
    Expression(String), // Parsed later
}

#[derive(Debug, Clone)]
pub enum TimeConstraint {
    Always,
    After(TimeSpec),
    Between { start: TimeSpec, end: TimeSpec },
    When { condition: Box<AssertionCondition> },
}

#[derive(Debug, Clone, Copy)]
pub enum Severity {
    Info,
    Warning,
    Error,
    Fatal,
}

/// Measurement definition
#[derive(Debug, Clone)]
pub struct Measurement {
    pub name: String,
    pub measurement_type: MeasurementType,
}

#[derive(Debug, Clone)]
pub enum MeasurementType {
    Average { signal: SignalRef },
    RMS { signal: SignalRef },
    PeakToPeak { signal: SignalRef, window: Option<TimeSpec> },
    RiseTime { signal: SignalRef, low_pct: f64, high_pct: f64 },
    FallTime { signal: SignalRef, high_pct: f64, low_pct: f64 },
    Frequency { signal: SignalRef },
    DutyCycle { signal: SignalRef, threshold: f64 },
    Integral { expression: String },
    Expression { formula: String },
}